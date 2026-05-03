use std::collections::HashSet;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::ai::{GenerateRequest, ChatMessage};
use crate::ai::providers::{MessageContent, ContentBlock};
use crate::ai::functions::{get_claude_tools, ToolCall as FunctionToolCall, execute_tool as execute_fn};
use crate::owl::{
    DbExecutor, Individual,
    get_literal_property, get_iri_property,
    replace_all_property_iris,
};

type Result<T> = std::result::Result<T, String>;

const MAX_TOOL_LOOPS: usize = 50;
const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMPERATURE: f32 = 0.3;
const DEFAULT_TIMEOUT_SECS: u64 = 180;

const STATUS_IN_PROGRESS: &str = "foundation:InProgress";
const STATUS_COMPLETED: &str = "foundation:Completed";
const STATUS_REJECTED: &str = "foundation:Rejected";

/// Returns true if an execute_task failure is a configuration problem that
/// won't resolve on retry — API key missing, agent not wired up, model not
/// configured, etc. The task should be marked Rejected so the recovery loop
/// stops re-running it on every startup.
fn is_config_error(err: &str) -> bool {
    err.contains("API key not configured")
        || err.contains("API key has no value")
        || err.contains("has no usesService")
        || err.contains("has no usesModel")
        || err.contains("has no modelIdentifier")
        || err.contains("has no assignee")
}

pub struct TaskExecutionState {
    running: Mutex<HashSet<String>>,
}

impl TaskExecutionState {
    pub fn new() -> Self {
        Self { running: Mutex::new(HashSet::new()) }
    }
}

fn maybe_execute_task_for_entity(app: AppHandle, entity_id: String) {
    tauri::async_runtime::spawn(async move {
        let executor = match app.try_state::<DbExecutor>() {
            Some(e) => e,
            None => return,
        };
        let entity = entity_id.clone();
        let (is_in_progress, has_agent) = executor
            .read(move |conn| {
                if !crate::owl::is_instance_of(conn, &entity, "foundation:Task") {
                    return Ok((false, false));
                }
                let status = get_iri_property(conn, &entity, "foundation:hasStatus")
                    .map_err(|e| e.to_string())?;
                let in_progress = status
                    .as_deref()
                    .map(|s| super::task_scheduler::is_status_descendant_of(conn, s, STATUS_IN_PROGRESS))
                    .unwrap_or(false);
                if !in_progress {
                    return Ok((false, false));
                }
                let assignee = get_iri_property(conn, &entity, "foundation:assignee")
                    .map_err(|e| e.to_string())?;
                let has_agent = assignee
                    .map(|a| crate::owl::is_instance_of(conn, &a, "foundation:SoftwareAgent"))
                    .unwrap_or(false);
                Ok((true, has_agent))
            })
            .await
            .unwrap_or((false, false));

        if !is_in_progress || !has_agent {
            return;
        }

        let execution_state = match app.try_state::<TaskExecutionState>() {
            Some(s) => s,
            None => return,
        };
        {
            let mut running = execution_state.running.lock().unwrap_or_else(|e| e.into_inner());
            if running.contains(&entity_id) {
                return;
            }
            running.insert(entity_id.clone());
        }

        let app2 = app.clone();
        let task_iri = entity_id.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = execute_task(&app2, &task_iri).await {
                crate::commands::log_backend("error", &format!(
                    "[task_manager] execute_task failed for {}: {}", task_iri, e
                ));

                // Permanent config errors (API key missing, agent not wired up)
                // would be retried forever otherwise: every startup the
                // task_scheduler resets InProgress→Pending and re-fires the
                // timer. Mark the task Rejected so the cycle stops; the user
                // can re-trigger manually after fixing config.
                if is_config_error(&e) {
                    if let Some(executor) = app2.try_state::<DbExecutor>() {
                        let task_for_write = task_iri.clone();
                        let err_for_write = e.clone();
                        let result_write = executor.write(move |conn| -> Result<String> {
                            set_status(conn, &task_for_write, STATUS_REJECTED)?;
                            Individual::new(&task_for_write)
                                .add_property(
                                    conn,
                                    "foundation:result",
                                    vec![crate::owl::Object::Literal {
                                        value: format!("Task automatically rejected (config error): {}", err_for_write),
                                        datatype: Some("xsd:string".to_string()),
                                        language: None,
                                    }],
                                    "task_manager",
                                )
                                .map_err(|e| e.to_string())?;
                            Ok(String::new())
                        }).await;
                        if let Err(set_err) = result_write {
                            crate::commands::log_backend("error", &format!(
                                "[task_manager] failed to mark {} as Rejected: {}", task_iri, set_err,
                            ));
                        } else {
                            app2.emit("entity-updated", serde_json::json!({ "entityId": task_iri })).ok();
                        }
                    }
                }
            }
            if let Some(state) = app2.try_state::<TaskExecutionState>() {
                let mut running = state.running.lock().unwrap_or_else(|e| e.into_inner());
                running.remove(&task_iri);
            }
        });
    });
}

pub fn listen_for_in_progress(app: AppHandle) {
    use tauri::Listener;

    let app2 = app.clone();
    app.clone().listen("entity-updated", move |event| {
        if let Some(entity_id) = parse_entity_id(event.payload()) {
            maybe_execute_task_for_entity(app2.clone(), entity_id);
        }
    });

    app.clone().listen("entity-created", move |event| {
        if let Some(entity_id) = parse_entity_id(event.payload()) {
            maybe_execute_task_for_entity(app.clone(), entity_id);
        }
    });
}

fn parse_entity_id(payload: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v["entityId"].as_str().map(|s| s.to_string()))
}

fn set_status(conn: &mut rusqlite::Connection, task_iri: &str, status_iri: &str) -> Result<()> {
    replace_all_property_iris(conn, task_iri, "foundation:hasStatus", &[status_iri], "task_manager")
        .map_err(|e| e.to_string())?;
    crate::owl::touch(conn, task_iri);
    Ok(())
}

pub async fn execute_task(app: &AppHandle, task_iri: &str) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let (label, description, agent_iri) = executor
        .read({
            let task_iri = task_iri.to_string();
            move |conn| {
                let label = get_literal_property(conn, &task_iri, "rdfs:label")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| task_iri.clone());

                let description = get_literal_property(conn, &task_iri, "rdfs:comment")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();

                let agent_iri = get_iri_property(conn, &task_iri, "foundation:assignee")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Task {} has no assignee", task_iri))?;

                Ok((label, description, agent_iri))
            }
        })
        .await?;

    let (api_key, model_identifier, system_prompt, timeout_secs) = executor
        .read({
            let agent_iri = agent_iri.clone();
            move |conn| {
                let service_iri = get_iri_property(conn, &agent_iri, "foundation:usesService")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Agent {} has no usesService", agent_iri))?;

                let api_key_iri = get_iri_property(conn, &service_iri, "foundation:apiKey")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "API key not configured".to_string())?;

                let api_key = get_literal_property(conn, &api_key_iri, "foundation:credentialValue")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "API key has no value".to_string())?;

                let model_iri = get_iri_property(conn, &agent_iri, "foundation:usesModel")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Agent {} has no usesModel", agent_iri))?;

                let model_identifier = get_literal_property(conn, &model_iri, "foundation:modelIdentifier")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Model {} has no modelIdentifier", model_iri))?;

                let base_prompt = Individual::get(conn, &agent_iri)
                    .ok()
                    .flatten()
                    .and_then(|ind| {
                        ind.properties.iter()
                            .find(|(k, _)| k == "foundation:basePrompt")
                            .and_then(|(_, v)| v.as_literal())
                    })
                    .unwrap_or_default();

                let base_system_prompt = crate::commands::chat::settings::load_base_system_prompt(conn);
                let system_prompt = if base_prompt.is_empty() {
                    base_system_prompt
                } else {
                    format!("{}\n\n{}", base_system_prompt, base_prompt)
                };

                let timeout_secs = get_literal_property(conn, &agent_iri, "foundation:requestTimeout")
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(DEFAULT_TIMEOUT_SECS);

                Ok((api_key, model_identifier, system_prompt, timeout_secs))
            }
        })
        .await?;

    let task_iri_owned = task_iri.to_string();
    executor
        .write(move |conn| -> Result<String> { set_status(conn, &task_iri_owned, STATUS_IN_PROGRESS)?; Ok(String::new()) })
        .await?;
    app.emit("entity-updated", serde_json::json!({ "entityId": task_iri })).ok();

    let conv_iri = super::agent_task::create_conversation(&executor, task_iri, &label, Some(&agent_iri)).await;

    let current_datetime = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let task_prompt = format!(
        "currentDateTime: {}\n\n\
         You are executing the task: **{}**\n\n\
         ## Task Description\n{}\n\n\
         ## Required Final Step\n\
         After completing the task, you MUST call `replace_property_values` with:\n\
         - iri: `{}`\n\
         - property_iri: `foundation:result`\n\
         - values: [your detailed result]\n\n\
         Do NOT modify `rdfs:comment` — that field contains the original instructions.",
        current_datetime, label, description, task_iri
    );

    let mut messages: Vec<ChatMessage> = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::ContentBlocks(vec![ContentBlock::Text { text: task_prompt }]),
    }];

    super::agent_task::append_message(&executor, &conv_iri, &messages[0], &model_identifier, 0).await;

    let tools = get_claude_tools();

    let provider = crate::ai::providers::ClaudeProvider::with_model(
        api_key.clone(),
        model_identifier.clone(),
        timeout_secs,
    );

    let mut last_text = String::new();

    for _ in 0..MAX_TOOL_LOOPS {
        let request = GenerateRequest {
            messages: messages.clone(),
            max_tokens: Some(DEFAULT_MAX_TOKENS),
            temperature: Some(DEFAULT_TEMPERATURE),
            system: Some(system_prompt.clone()),
            blackboard_context: None,
            tools: Some(tools.clone()),
            supports_web_tools: false,
            thinking: None,
            tool_choice: None,
        };

        let exec_iri = format!("foundation:TaskExecution_{}", task_iri);
        let response = provider.generate_stream(request, app, &exec_iri).await
            .map_err(|e| format!("Task execution API error: {}", e))?;

        let stop_reason = response.stop_reason.clone().unwrap_or_else(|| "end_turn".to_string());
        last_text = response.content.clone();

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        if !response.content.is_empty() {
            assistant_blocks.push(ContentBlock::Text { text: response.content.clone() });
        }
        for tc in &response.tool_calls {
            assistant_blocks.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.input.clone(),
            });
        }
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: MessageContent::ContentBlocks(assistant_blocks),
        });
        super::agent_task::append_message(&executor, &conv_iri, messages.last().unwrap(), &model_identifier, messages.len() - 1).await;

        if stop_reason != "tool_use" {
            break;
        }

        let mut result_blocks: Vec<ContentBlock> = Vec::new();
        for tc in &response.tool_calls {
            let call = FunctionToolCall {
                name: tc.name.clone(),
                arguments: tc.input.clone(),
            };
            let tc_id = tc.id.clone();
            let app_clone = app.clone();
            let result_json = executor
                .write(move |conn| {
                    let r = execute_fn(conn, &call, Some(&app_clone), None);
                    serde_json::to_string(&r).map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|e| format!("\"{}\"", e));

            let tool_result: crate::ai::functions::ToolResult =
                serde_json::from_str(&result_json).unwrap_or(crate::ai::functions::ToolResult {
                    success: false,
                    result: None,
                    error: Some(result_json.clone()),
                    concept: None,
                });

            let content = if tool_result.success {
                tool_result.result
                    .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| v.to_string()))
                    .unwrap_or_default()
            } else {
                tool_result.error.unwrap_or_default()
            };

            result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: tc_id,
                content: serde_json::Value::String(content),
                is_error: Some(!tool_result.success),
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: MessageContent::ContentBlocks(result_blocks),
        });
        super::agent_task::append_message(&executor, &conv_iri, messages.last().unwrap(), &model_identifier, messages.len() - 1).await;
    }

    let task_iri_done = task_iri.to_string();
    let delegated_conv_iri = executor
        .write(move |conn| -> Result<String> {
            set_status(conn, &task_iri_done, STATUS_COMPLETED)?;
            super::task_blocker::check_and_unblock(conn, &task_iri_done);
            let conv = get_iri_property(conn, &task_iri_done, "foundation:delegatedFromConversation")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            Ok(conv)
        })
        .await?;

    app.emit("entity-updated", serde_json::json!({ "entityId": task_iri })).ok();
    app.emit("task-completed", serde_json::json!({
        "task_iri": task_iri,
        "label": label,
        "result_summary": last_text,
    })).ok();

    if !delegated_conv_iri.is_empty() {
        let result_text = executor
            .read({
                let task_iri = task_iri.to_string();
                move |conn| {
                    Ok(get_literal_property(conn, &task_iri, "foundation:result")
                        .unwrap_or(None)
                        .unwrap_or_default())
                }
            })
            .await
            .unwrap_or_default();

        let result = if result_text.is_empty() { &last_text } else { &result_text };
        inject_and_trigger_conversation(app, &delegated_conv_iri, task_iri, &label, result).await;
    }

    Ok(last_text)
}

fn format_delegation_result(task_iri: &str, label: &str, result: &str) -> String {
    format!("[Resultado da tarefa delegada: \"{}\" (`{}`)]\n\n{}", label, task_iri, result)
}

async fn inject_and_trigger_conversation(
    app: &AppHandle,
    conv_iri: &str,
    task_iri: &str,
    task_label: &str,
    result: &str,
) {
    let executor = app.state::<DbExecutor>();
    let queue_state = match app.try_state::<crate::commands::ConversationProcessingState>() {
        Some(s) => s,
        None => {
            crate::commands::log_backend(
                "warn",
                "[task_manager] ConversationProcessingState not registered",
            );
            return;
        }
    };

    let message = format_delegation_result(task_iri, task_label, result);
    if let Err(e) = crate::commands::create_user_message(&executor, conv_iri, &message).await {
        crate::commands::log_backend("error", &format!(
            "[task_manager] Failed to inject delegation result into {}: {}", conv_iri, e
        ));
        return;
    }

    app.emit("chat-message-added", serde_json::json!({ "conversationId": conv_iri })).ok();

    if queue_state.try_acquire(conv_iri) {
        let app_clone = app.clone();
        let executor_clone = executor.inner().clone();
        let conv_iri_owned = conv_iri.to_string();
        tokio::spawn(async move {
            crate::commands::process_conversation_queue(
                app_clone, executor_clone, conv_iri_owned,
            ).await;
        });
    }
}

pub fn check_and_recur(conn: &mut rusqlite::Connection, task_iri: &str) {
    use crate::eavto::{store, Triple, Object};

    if get_iri_property(conn, task_iri, "foundation:nextTask").unwrap_or(None).is_some() {
        return;
    }

    let status = get_iri_property(conn, task_iri, "foundation:hasStatus").unwrap_or(None);
    let is_completed = status
        .as_deref()
        .map(|s| super::task_scheduler::is_status_descendant_of(conn, s, STATUS_COMPLETED))
        .unwrap_or(false);
    if !is_completed {
        return;
    }

    let rrule_str = match get_literal_property(conn, task_iri, "foundation:recurrence")
        .unwrap_or(None)
    {
        Some(r) => r,
        None => return,
    };

    let rrule = match super::recurrence::parse_rrule(&rrule_str) {
        Some(r) => r,
        None => {
            crate::commands::log_backend("warn", &format!(
                "[recurrence] RRULE inválida para {}: {}", task_iri, rrule_str
            ));
            return;
        }
    };

    let now = chrono::Utc::now().naive_utc();
    let next_dt = match super::recurrence::next_occurrence(&rrule, now) {
        Some(dt) => dt,
        None => {
            crate::commands::log_backend("error", &format!(
                "[recurrence] não foi possível calcular próxima data para {}", task_iri
            ));
            return;
        }
    };
    let next_iso = next_dt.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let label = get_literal_property(conn, task_iri, "rdfs:label").unwrap_or(None).unwrap_or_default();
    let comment = get_literal_property(conn, task_iri, "rdfs:comment").unwrap_or(None).unwrap_or_default();
    let assignee = get_iri_property(conn, task_iri, "foundation:assignee").unwrap_or(None);

    let new_iri = format!("foundation:Task_{}", chrono::Utc::now().timestamp_millis());

    let mut triples = vec![
        Triple::new(&new_iri, "rdf:type", Object::Iri("foundation:Task".to_string())),
        Triple::new(&new_iri, "rdfs:label", Object::Literal {
            value: label,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new(&new_iri, "foundation:hasStatus", Object::Iri("foundation:Pending".to_string())),
        Triple::new(&new_iri, "foundation:recurrence", Object::Literal {
            value: rrule_str.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new(&new_iri, "foundation:scheduledAt", Object::Literal {
            value: next_iso.clone(),
            datatype: Some("xsd:dateTime".to_string()),
            language: None,
        }),
    ];
    if !comment.is_empty() {
        triples.push(Triple::new(&new_iri, "rdfs:comment", Object::Literal {
            value: comment,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }));
    }
    if let Some(a) = assignee {
        triples.push(Triple::new(&new_iri, "foundation:assignee", Object::Iri(a)));
    }

    if let Err(e) = store::assert_triples(conn, &triples, "recurrence") {
        crate::commands::log_backend("error", &format!(
            "[recurrence] falha ao criar task: {}", e
        ));
        return;
    }

    if let Err(e) = replace_all_property_iris(conn, task_iri, "foundation:nextTask", &[&new_iri], "recurrence") {
        crate::commands::log_backend("error", &format!(
            "[recurrence] falha ao setar nextTask: {}", e
        ));
        return;
    }

    crate::commands::log_backend("info", &format!(
        "[recurrence] criada {} a partir de {} ({})", new_iri, task_iri, next_iso
    ));
}

pub fn listen_for_recurrence(app: AppHandle) {
    use tauri::Listener;

    app.clone().listen("entity-updated", move |event| {
        let entity_id = match parse_entity_id(event.payload()) {
            Some(id) => id,
            None => return,
        };
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let executor = match app2.try_state::<DbExecutor>() {
                Some(e) => e,
                None => return,
            };
            let entity = entity_id.clone();
            let should_recur = executor
                .read(move |conn| {
                    if !crate::owl::is_instance_of(conn, &entity, "foundation:Task") {
                        return Ok(false);
                    }
                    let status = get_iri_property(conn, &entity, "foundation:hasStatus")
                        .map_err(|e| e.to_string())?;
                    Ok(status
                        .as_deref()
                        .map(|s| super::task_scheduler::is_status_descendant_of(conn, s, STATUS_COMPLETED))
                        .unwrap_or(false))
                })
                .await
                .unwrap_or(false);

            if should_recur {
                let entity2 = entity_id.clone();
                let _ = executor
                    .write(move |conn| {
                        check_and_recur(conn, &entity2);
                        Ok(String::new())
                    })
                    .await;
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    fn insert(conn: &mut rusqlite::Connection, triples: &[Triple]) {
        store::assert_triples(conn, triples, "test").expect("Failed to insert triples");
    }

    fn task_triples(task_iri: &str, agent_iri: Option<&str>) -> Vec<Triple> {
        let mut t = vec![
            Triple::new(task_iri, "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new(task_iri, "rdfs:label", Object::Literal {
                value: "Test Task".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new(task_iri, "foundation:hasStatus", Object::Iri("foundation:Pending".to_string())),
        ];
        if let Some(a) = agent_iri {
            t.push(Triple::new(task_iri, "foundation:assignee", Object::Iri(a.to_string())));
        }
        t
    }

    #[test]
    fn test_set_status_updates_and_touches() {
        let mut conn = setup_test_db();
        insert(&mut conn, &task_triples("foundation:Task_test1", Some("foundation:Agent1")));

        set_status(&mut conn, "foundation:Task_test1", STATUS_IN_PROGRESS)
            .expect("set_status should succeed");

        let status = get_iri_property(&conn, "foundation:Task_test1", "foundation:hasStatus")
            .unwrap();
        assert_eq!(status, Some(STATUS_IN_PROGRESS.to_string()));
    }

    #[test]
    fn test_set_status_replaces_previous_value() {
        let mut conn = setup_test_db();
        insert(&mut conn, &task_triples("foundation:Task_test2", Some("foundation:Agent1")));

        set_status(&mut conn, "foundation:Task_test2", STATUS_IN_PROGRESS).unwrap();
        set_status(&mut conn, "foundation:Task_test2", STATUS_COMPLETED).unwrap();

        let status = get_iri_property(&conn, "foundation:Task_test2", "foundation:hasStatus")
            .unwrap();
        assert_eq!(status, Some(STATUS_COMPLETED.to_string()));

        let result = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:Task_test2", "foundation:hasStatus"
        ).unwrap();
        let active: Vec<_> = result.triples.iter()
            .filter(|t| !t.retracted)
            .collect();
        assert_eq!(active.len(), 1, "deve haver exatamente um status ativo");
    }

    #[test]
    fn test_execute_task_requires_assignee() {
        let mut conn = setup_test_db();
        insert(&mut conn, &task_triples("foundation:Task_noagent", None));

        let result = get_iri_property(&conn, "foundation:Task_noagent", "foundation:assignee")
            .unwrap();
        assert!(result.is_none(), "task sem assignee deve retornar None");
    }

    fn completed_tree(conn: &mut rusqlite::Connection) {
        insert(conn, &[Triple::new(
            "foundation:Completed", "foundation:parentStatus",
            Object::Iri("foundation:Completed".to_string()),
        )]);
        insert(conn, &[Triple::new(
            "foundation:Pending", "foundation:parentStatus",
            Object::Iri("foundation:Pending".to_string()),
        )]);
    }

    #[test]
    fn task_sem_recorrencia_nao_cria_nova_task() {
        let mut conn = setup_test_db();
        completed_tree(&mut conn);
        insert(&mut conn, &[
            Triple::new("foundation:Task_norec", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_norec", "rdfs:label", Object::Literal {
                value: "Sem recorrência".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_norec", "foundation:hasStatus", Object::Iri("foundation:Completed".to_string())),
        ]);

        check_and_recur(&mut conn, "foundation:Task_norec");

        let next = get_iri_property(&conn, "foundation:Task_norec", "foundation:nextTask").unwrap();
        assert!(next.is_none(), "sem recorrência não deve criar nextTask");
    }

    #[test]
    fn task_recorrente_concluida_cria_nova_task_pendente() {
        let mut conn = setup_test_db();
        completed_tree(&mut conn);
        insert(&mut conn, &[
            Triple::new("foundation:Task_rec1", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_rec1", "rdfs:label", Object::Literal {
                value: "Reunião semanal".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_rec1", "foundation:hasStatus", Object::Iri("foundation:Completed".to_string())),
            Triple::new("foundation:Task_rec1", "foundation:recurrence", Object::Literal {
                value: "FREQ=DAILY;INTERVAL=1".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
        ]);

        check_and_recur(&mut conn, "foundation:Task_rec1");

        let next_iri = get_iri_property(&conn, "foundation:Task_rec1", "foundation:nextTask").unwrap();
        assert!(next_iri.is_some(), "deve ter criado nextTask");
        let new_task = next_iri.unwrap();
        let status = get_iri_property(&conn, &new_task, "foundation:hasStatus").unwrap();
        assert_eq!(status.as_deref(), Some("foundation:Pending"), "nova task deve ser Pendente");
        let label = get_literal_property(&conn, &new_task, "rdfs:label").unwrap();
        assert_eq!(label.as_deref(), Some("Reunião semanal"), "deve preservar o label");
    }

    #[test]
    fn resultado_delegado_formatado_contem_label_e_resultado() {
        let msg = format_delegation_result("foundation:Task_123", "Analisar dados", "Crescimento de 15%");
        assert!(msg.contains("Analisar dados"));
        assert!(msg.contains("Crescimento de 15%"));
        assert!(msg.contains("foundation:Task_123"));
    }

    #[test]
    fn task_sem_delegated_from_conversation_nao_tem_conv_iri() {
        let mut conn = setup_test_db();
        insert(&mut conn, &task_triples("foundation:Task_nodelegation", Some("foundation:Agent1")));

        let conv_iri = get_iri_property(&conn, "foundation:Task_nodelegation", "foundation:delegatedFromConversation")
            .unwrap();
        assert!(conv_iri.is_none(), "task sem delegatedFromConversation deve retornar None");
    }

    #[test]
    fn task_com_delegated_from_conversation_tem_conv_iri() {
        let mut conn = setup_test_db();
        let mut triples = task_triples("foundation:Task_delegated", Some("foundation:Agent1"));
        triples.push(Triple::new(
            "foundation:Task_delegated",
            "foundation:delegatedFromConversation",
            Object::Iri("foundation:AIConversation_test".to_string()),
        ));
        insert(&mut conn, &triples);

        let conv_iri = get_iri_property(&conn, "foundation:Task_delegated", "foundation:delegatedFromConversation")
            .unwrap();
        assert_eq!(conv_iri.as_deref(), Some("foundation:AIConversation_test"));
    }

    #[test]
    fn idempotencia_check_and_recur_duas_vezes() {
        let mut conn = setup_test_db();
        completed_tree(&mut conn);
        insert(&mut conn, &[
            Triple::new("foundation:Task_rec2", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_rec2", "rdfs:label", Object::Literal {
                value: "Tarefa recorrente".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_rec2", "foundation:hasStatus", Object::Iri("foundation:Completed".to_string())),
            Triple::new("foundation:Task_rec2", "foundation:recurrence", Object::Literal {
                value: "FREQ=DAILY;INTERVAL=1".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
        ]);

        check_and_recur(&mut conn, "foundation:Task_rec2");
        check_and_recur(&mut conn, "foundation:Task_rec2");

        let result = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:Task_rec2", "foundation:nextTask"
        ).unwrap();
        let active: Vec<_> = result.triples.iter().filter(|t| !t.retracted).collect();
        assert_eq!(active.len(), 1, "deve haver exatamente uma nextTask, não duplicatas");
    }
}
