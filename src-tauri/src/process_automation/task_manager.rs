use std::collections::HashSet;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::ai::ChatMessage;
use crate::ai::providers::{ContentBlock, MessageContent};
use crate::owl::{
    DbExecutor, Individual,
    get_literal_property, get_iri_property,
};

type Result<T> = std::result::Result<T, String>;

const MAX_TOOL_LOOPS: usize = 50;
const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMPERATURE: f32 = 0.3;

/// Statuses that permanently block execution — do not auto-start even if
/// description and assignee are present.
const HALTED_STATUSES: &[&str] = &["foundation:Blocked", "foundation:Rejected"];

/// Returns true if an execute_task failure is a configuration problem that
/// won't resolve on retry — API key missing, agent not wired up, model not
/// configured, etc. A result is written so the task won't be retried on
/// the next startup.
fn is_config_error(err: &str) -> bool {
    err.contains("API key not configured")
        || err.contains("API key has no value")
        || err.contains("has no usesService")
        || err.contains("has no usesModel")
        || err.contains("No AI service configured")
        || err.contains("No AI model configured")
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

/// Returns true when a task is ready for automatic execution:
/// has a non-empty description, is assigned to a SoftwareAgent, has not yet
/// started (no startedAt), has no result, is not in a halted status, and is
/// not scheduled for a future time.
fn is_task_ready(conn: &rusqlite::Connection, entity_id: &str) -> Result<bool> {
    if !crate::owl::is_instance_of(conn, entity_id, "foundation:Task") {
        return Ok(false);
    }
    let description = get_literal_property(conn, entity_id, "rdfs:comment")
        .map_err(|e| e.to_string())?;
    if description.as_deref().unwrap_or("").trim().is_empty() {
        return Ok(false);
    }
    let assignee = get_iri_property(conn, entity_id, "foundation:assignee")
        .map_err(|e| e.to_string())?;
    let has_agent = assignee
        .map(|a| crate::owl::is_instance_of(conn, &a, "foundation:SoftwareAgent"))
        .unwrap_or(false);
    if !has_agent {
        return Ok(false);
    }
    if get_literal_property(conn, entity_id, "foundation:startedAt")
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Ok(false);
    }
    if get_literal_property(conn, entity_id, "foundation:result")
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Ok(false);
    }
    let status = get_iri_property(conn, entity_id, "foundation:hasStatus")
        .map_err(|e| e.to_string())?;
    if let Some(ref s) = status {
        if HALTED_STATUSES.iter().any(|h| s == h) {
            return Ok(false);
        }
    }
    let scheduled_at = get_literal_property(conn, entity_id, "foundation:scheduledAt")
        .map_err(|e| e.to_string())?;
    if let Some(ref dt) = scheduled_at {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(dt) {
            if parsed.with_timezone(&chrono::Utc) > chrono::Utc::now() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn maybe_execute_task_for_entity(app: AppHandle, entity_id: String) {
    tauri::async_runtime::spawn(async move {
        let executor = match app.try_state::<DbExecutor>() {
            Some(e) => e,
            None => return,
        };
        let entity = entity_id.clone();
        let ready = executor
            .read(move |conn| is_task_ready(conn, &entity))
            .await
            .unwrap_or(false);

        if !ready {
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

                // Any failure leaves startedAt set without a result, which means the
                // task would be recovered and re-run on every startup. Write result to
                // prevent re-execution — the user can clear it manually to retry.
                if let Some(executor) = app2.try_state::<DbExecutor>() {
                    let task_for_write = task_iri.clone();
                    let err_for_write = e.clone();
                    let prefix = if is_config_error(&e) { "Erro de configuração" } else { "Erro de execução" };
                    let result_write = executor.write(move |conn| -> Result<String> {
                        set_result(conn, &task_for_write, &format!("{}: {}", prefix, err_for_write))?;
                        set_status(conn, &task_for_write, "foundation:Rejected")?;
                        Ok(String::new())
                    }).await;
                    if let Err(set_err) = result_write {
                        crate::commands::log_backend("error", &format!(
                            "[task_manager] failed to write error result for {}: {}", task_iri, set_err,
                        ));
                    } else {
                        app2.emit("entity-updated", serde_json::json!({ "entityId": task_iri })).ok();
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

fn set_started_at(conn: &mut rusqlite::Connection, task_iri: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    Individual::new(task_iri)
        .add_property(
            conn,
            "foundation:startedAt",
            vec![crate::owl::Object::Literal {
                value: now,
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }],
            "task_manager",
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn set_status(conn: &mut rusqlite::Connection, task_iri: &str, status_iri: &str) -> Result<()> {
    crate::owl::replace_all_property_iris(conn, task_iri, "foundation:hasStatus", &[status_iri], "task_manager")
        .map_err(|e| e.to_string())
}

fn set_result(conn: &mut rusqlite::Connection, task_iri: &str, value: &str) -> Result<()> {
    Individual::new(task_iri)
        .add_property(
            conn,
            "foundation:result",
            vec![crate::owl::Object::Literal {
                value: value.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }],
            "task_manager",
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn execute_task(app: &AppHandle, task_iri: &str) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let (label, description, agent_iri, ai_behavior_rules) = executor
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

                let ai_behavior_rules = get_literal_property(conn, &task_iri, "foundation:aiBehaviorRules")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();

                Ok((label, description, agent_iri, ai_behavior_rules))
            }
        })
        .await?;

    let agent_config = super::agent_runner::resolve_agent_config(&executor, &agent_iri).await?;

    let conv_iri = super::agent_task::create_conversation(&executor, task_iri, "foundation:generatedByTask", &label, Some(&agent_iri)).await;

    let task_iri_owned = task_iri.to_string();
    executor
        .write(move |conn| -> Result<String> {
            set_started_at(conn, &task_iri_owned)?;
            set_status(conn, &task_iri_owned, "foundation:InProgress")?;
            Ok(String::new())
        })
        .await?;

    let current_datetime = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let behavior_section = if ai_behavior_rules.trim().is_empty() {
        String::new()
    } else {
        format!("\n\n## Behavior Rules\n{}", ai_behavior_rules)
    };
    let task_prompt = format!(
        "currentDateTime: {}\n\n\
         You are executing the task: **{}**\n\n\
         ## Task Description\n{}{}\n\n\
         ## Required Final Step\n\
         After completing the task, you MUST call `replace_property_values` with:\n\
         - iri: `{}`\n\
         - property_iri: `foundation:result`\n\
         - values: [your detailed result]\n\n\
         Do NOT modify `rdfs:comment` — that field contains the original instructions.",
        current_datetime, label, description, behavior_section, task_iri
    );

    let initial_messages = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::ContentBlocks(vec![ContentBlock::Text { text: task_prompt }]),
    }];

    let loop_config = super::tool_loop::ToolLoopConfig {
        system: Some(agent_config.system_prompt),
        tools: crate::ai::functions::get_claude_tools(),
        max_iterations: MAX_TOOL_LOOPS,
        max_tokens: DEFAULT_MAX_TOKENS,
        temperature: DEFAULT_TEMPERATURE,
        completion_tool: None,
        compaction_threshold: 0.80,
        context_window: 180_000,
        persist_to: Some(super::tool_loop::PersistConfig {
            conv_iri: conv_iri.clone(),
            model_identifier: agent_config.model_identifier.clone(),
        }),
    };

    let output = super::tool_loop::run_tool_loop(
        &executor, &agent_config.provider, initial_messages, loop_config,
    ).await?;

    let task_iri_done = task_iri.to_string();
    let last_text = output.last_text.clone();
    let delegated_conv_iri = executor
        .write(move |conn| -> Result<String> {
            // Guarantee result is always set so the task is never re-run on the
            // next startup (the AI should have called replace_property_values, but
            // if it didn't we write the last message as a fallback).
            if get_literal_property(conn, &task_iri_done, "foundation:result")
                .unwrap_or(None)
                .is_none()
            {
                let fallback = if last_text.is_empty() { "Concluído." } else { &last_text };
                set_result(conn, &task_iri_done, fallback)?;
            }
            set_status(conn, &task_iri_done, "foundation:Completed")?;
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
        "result_summary": output.last_text,
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

        let result = if result_text.is_empty() { &output.last_text } else { &result_text };
        inject_and_trigger_conversation(app, &delegated_conv_iri, task_iri, &label, result).await;
    }

    Ok(output.last_text)
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

    let has_result = get_literal_property(conn, task_iri, "foundation:result")
        .unwrap_or(None)
        .is_some();
    if !has_result {
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

    // Predicates already set above or that must NOT carry over (instance-specific state).
    const SKIP: &[&str] = &[
        "rdf:type", "rdfs:label", "rdfs:comment",
        "foundation:recurrence", "foundation:scheduledAt", "foundation:assignee",
        "foundation:result", "foundation:nextTask", "foundation:hasStatus",
        "foundation:lastUpdatedAt", "foundation:startedAt",
    ];
    // Copy all remaining properties so delegatedFromConversation, aiBehaviorRules,
    // dueDate, relatedTo, and any future task-level config survive every recurrence cycle.
    if let Ok(all) = crate::eavto::query::get_by_entity(conn, task_iri) {
        for t in all.triples {
            if !SKIP.contains(&t.predicate.as_str()) {
                triples.push(Triple::new(&new_iri, t.predicate, t.object));
            }
        }
    }

    if let Err(e) = store::assert_triples(conn, &triples, "recurrence") {
        crate::commands::log_backend("error", &format!(
            "[recurrence] falha ao criar task: {}", e
        ));
        return;
    }

    if let Err(e) = crate::owl::replace_all_property_iris(conn, task_iri, "foundation:nextTask", &[&new_iri], "recurrence") {
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
                    Ok(get_literal_property(conn, &entity, "foundation:result")
                        .map_err(|e| e.to_string())?
                        .is_some())
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
    fn test_set_started_at_persiste_datetime() {
        let mut conn = setup_test_db();
        insert(&mut conn, &task_triples("foundation:Task_test1", Some("foundation:Agent1")));

        set_started_at(&mut conn, "foundation:Task_test1")
            .expect("set_started_at should succeed");

        let started = get_literal_property(&conn, "foundation:Task_test1", "foundation:startedAt")
            .unwrap();
        assert!(started.is_some(), "deve ter startedAt após set_started_at");
    }

    #[test]
    fn test_execute_task_requires_assignee() {
        let mut conn = setup_test_db();
        insert(&mut conn, &task_triples("foundation:Task_noagent", None));

        let result = get_iri_property(&conn, "foundation:Task_noagent", "foundation:assignee")
            .unwrap();
        assert!(result.is_none(), "task sem assignee deve retornar None");
    }

    fn task_with_result(task_iri: &str) -> Vec<Triple> {
        vec![
            Triple::new(task_iri, "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new(task_iri, "rdfs:label", Object::Literal {
                value: task_iri.to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new(task_iri, "foundation:result", Object::Literal {
                value: "resultado concluído".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
        ]
    }

    #[test]
    fn task_sem_recorrencia_nao_cria_nova_task() {
        let mut conn = setup_test_db();
        insert(&mut conn, &task_with_result("foundation:Task_norec"));

        check_and_recur(&mut conn, "foundation:Task_norec");

        let next = get_iri_property(&conn, "foundation:Task_norec", "foundation:nextTask").unwrap();
        assert!(next.is_none(), "sem recorrência não deve criar nextTask");
    }

    #[test]
    fn task_recorrente_concluida_cria_nova_task_pendente() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Task_rec1", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_rec1", "rdfs:label", Object::Literal {
                value: "Reunião semanal".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_rec1", "foundation:result", Object::Literal {
                value: "concluído".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_rec1", "foundation:recurrence", Object::Literal {
                value: "FREQ=DAILY;INTERVAL=1".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
        ]);

        check_and_recur(&mut conn, "foundation:Task_rec1");

        let next_iri = get_iri_property(&conn, "foundation:Task_rec1", "foundation:nextTask").unwrap();
        assert!(next_iri.is_some(), "deve ter criado nextTask");
        let new_task = next_iri.unwrap();
        let result = get_literal_property(&conn, &new_task, "foundation:result").unwrap();
        assert!(result.is_none(), "nova task não deve ter resultado ainda");
        let started = get_literal_property(&conn, &new_task, "foundation:startedAt").unwrap();
        assert!(started.is_none(), "nova task não deve ter startedAt ainda");
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
        insert(&mut conn, &[
            Triple::new("foundation:Task_rec2", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_rec2", "rdfs:label", Object::Literal {
                value: "Tarefa recorrente".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_rec2", "foundation:result", Object::Literal {
                value: "concluído".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
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

    #[test]
    fn recorrencia_copia_delegated_from_conversation_e_demais_props() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Task_fullcopy", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_fullcopy", "rdfs:label", Object::Literal {
                value: "Triagem de emails".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_fullcopy", "foundation:result", Object::Literal {
                value: "concluído".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_fullcopy", "foundation:recurrence", Object::Literal {
                value: "FREQ=HOURLY;INTERVAL=1".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_fullcopy", "foundation:delegatedFromConversation",
                Object::Iri("foundation:Conv_test".to_string())),
            Triple::new("foundation:Task_fullcopy", "foundation:aiBehaviorRules", Object::Literal {
                value: "Processar apenas emails não lidos.".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_fullcopy", "foundation:dueDate", Object::Literal {
                value: "2026-12-31T23:59:59Z".to_string(), datatype: Some("xsd:dateTime".to_string()), language: None,
            }),
            Triple::new("foundation:Task_fullcopy", "foundation:relatedTo",
                Object::Iri("foundation:Process_email".to_string())),
        ]);

        check_and_recur(&mut conn, "foundation:Task_fullcopy");

        let next_iri = get_iri_property(&conn, "foundation:Task_fullcopy", "foundation:nextTask")
            .unwrap().expect("deve ter criado nextTask");

        let conv = get_iri_property(&conn, &next_iri, "foundation:delegatedFromConversation").unwrap();
        assert_eq!(conv.as_deref(), Some("foundation:Conv_test"), "delegatedFromConversation deve ser copiado");

        let rules = get_literal_property(&conn, &next_iri, "foundation:aiBehaviorRules").unwrap();
        assert!(rules.is_some(), "aiBehaviorRules deve ser copiado");

        let due = get_literal_property(&conn, &next_iri, "foundation:dueDate").unwrap();
        assert!(due.is_some(), "dueDate deve ser copiado");

        let related = get_iri_property(&conn, &next_iri, "foundation:relatedTo").unwrap();
        assert_eq!(related.as_deref(), Some("foundation:Process_email"), "relatedTo deve ser copiado");

        let result = get_literal_property(&conn, &next_iri, "foundation:result").unwrap();
        assert!(result.is_none(), "result não deve ser copiado para a nova task");
    }
}
