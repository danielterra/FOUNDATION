use crate::ai::providers::ToolDefinition;
use crate::owl::{DbExecutor, Individual, Object};
use crate::process_automation::tool_loop::{
    run_tool_loop, CompletionToolConfig, PersistConfig, ProgressEventConfig, ToolLoopConfig,
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

const AGENT_IRI: &str = "foundation:SoftwareAgent_1773313705318";
const QUIET_WINDOW_MS: i64 = 3_000;
const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
const MAX_CONCURRENT_SESSIONS: usize = 4;
static WORKER_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Batch {
    session_iri: String,
    conversation_iri: String,
    batch_iri: String,
    upper_bound: i64,
    item_iris: Vec<String>,
    prompt: String,
}

pub fn start(app: AppHandle) {
    if WORKER_STARTED.swap(true, Ordering::AcqRel) {
        crate::commands::log_backend("warn", "[ingestion] duplicate worker start ignored");
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let task = tauri::async_runtime::spawn(run(app.clone()));
            match task.await {
                Ok(()) => crate::commands::log_backend(
                    "error",
                    "[ingestion] worker loop exited unexpectedly; restarting supervisor",
                ),
                Err(error) => crate::commands::log_backend(
                    "error",
                    &format!(
                        "[ingestion] worker loop terminated unexpectedly: {error}; restarting"
                    ),
                ),
            }
            tokio::time::sleep(SCAN_INTERVAL).await;
        }
    });
}

async fn run(app: AppHandle) {
    let executor = app.state::<DbExecutor>().inner().clone();
    if let Err(error) = recover_processing_sessions(&executor).await {
        crate::commands::log_backend("error", &format!("[ingestion] recovery failed: {error}"));
    }
    let running = Arc::new(Mutex::new(HashSet::<String>::new()));
    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_SESSIONS));

    loop {
        let sessions = match pending_sessions(&executor).await {
            Ok(sessions) => sessions,
            Err(error) => {
                crate::commands::log_backend(
                    "error",
                    &format!("[ingestion] queue scan failed: {error}"),
                );
                tokio::time::sleep(SCAN_INTERVAL).await;
                continue;
            }
        };
        for session_iri in sessions {
            let should_start = running
                .lock()
                .map(|mut set| set.insert(session_iri.clone()))
                .unwrap_or(false);
            if !should_start {
                continue;
            }
            let permit = match permits.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    if let Ok(mut set) = running.lock() {
                        set.remove(&session_iri);
                    }
                    break;
                }
            };
            let executor = executor.clone();
            let app = app.clone();
            let running = running.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = permit;
                let session_task = session_iri.clone();
                let session_executor = executor.clone();
                let session_app = app.clone();
                let task = tauri::async_runtime::spawn(async move {
                    process_session(&app, &session_executor, &session_task).await
                });
                let failure = match task.await {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(error) => {
                        Some(format!("Ingestion worker task ended unexpectedly: {error}"))
                    }
                };
                if let Some(error) = failure {
                    mark_session_failed(&executor, &session_iri, &error).await;
                    crate::commands::log_backend(
                        "error",
                        &format!("[ingestion] session {session_iri} failed: {error}"),
                    );
                    emit_session_progress(&session_app, &session_iri, "failed", None, None);
                }
                if let Ok(mut set) = running.lock() {
                    set.remove(&session_iri);
                }
            });
        }
        tokio::time::sleep(SCAN_INTERVAL).await;
    }
}

async fn recover_processing_sessions(executor: &DbExecutor) -> Result<(), String> {
    executor
        .write(|conn| {
            for status in ["Processing", "Failed"] {
                let sessions = Individual::find_by_class_and_properties(
                    conn,
                    "foundation:IngestionSession",
                    &[("foundation:ingestionSessionStatus", status)],
                )
                .map_err(|error| error.to_string())?;
                for iri in sessions {
                    Individual::new(&iri)
                        .add_property(
                            conn,
                            "foundation:ingestionSessionStatus",
                            vec![literal("Pending")],
                            "ingestion_recovery",
                        )
                        .map_err(|error| error.to_string())?;
                    Individual::new(&iri)
                        .add_property(
                            conn,
                            "foundation:hasStatus",
                            vec![Object::Iri("foundation:Pending".into())],
                            "ingestion_recovery",
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            for status in ["Processing", "Failed"] {
                let items = Individual::find_by_class_and_properties(
                    conn,
                    "foundation:IngestionItem",
                    &[("foundation:ingestionItemStatus", status)],
                )
                .map_err(|error| error.to_string())?;
                for iri in items {
                    Individual::new(&iri)
                        .add_property(
                            conn,
                            "foundation:ingestionItemStatus",
                            vec![literal("Pending")],
                            "ingestion_recovery",
                        )
                        .map_err(|error| error.to_string())?;
                    Individual::new(&iri)
                        .add_property(
                            conn,
                            "foundation:hasStatus",
                            vec![Object::Iri("foundation:Pending".into())],
                            "ingestion_recovery",
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            Ok(String::new())
        })
        .await?;
    Ok(())
}

async fn pending_sessions(executor: &DbExecutor) -> Result<Vec<String>, String> {
    executor
        .read(|conn| {
            Individual::find_by_class_and_properties(
                conn,
                "foundation:IngestionSession",
                &[("foundation:ingestionSessionStatus", "Pending")],
            )
            .map_err(|error| error.to_string())
        })
        .await
}

async fn process_session(
    app: &AppHandle,
    executor: &DbExecutor,
    session_iri: &str,
) -> Result<(), String> {
    let Some(batch) = freeze_ready_batch(executor, session_iri).await? else {
        return Ok(());
    };
    let config =
        crate::process_automation::agent_runner::resolve_agent_config_without_request_deadline(
            executor, AGENT_IRI,
        )
        .await?;
    process_frozen_batch(Some(app), executor, batch, config).await
}

#[cfg(test)]
async fn process_session_with_config(
    app: Option<&AppHandle>,
    executor: &DbExecutor,
    session_iri: &str,
    config: crate::process_automation::agent_runner::AgentConfig,
) -> Result<(), String> {
    let Some(batch) = freeze_ready_batch(executor, session_iri).await? else {
        return Ok(());
    };
    process_frozen_batch(app, executor, batch, config).await
}

async fn process_frozen_batch(
    app: Option<&AppHandle>,
    executor: &DbExecutor,
    batch: Batch,
    config: crate::process_automation::agent_runner::AgentConfig,
) -> Result<(), String> {
    let conv_iri =
        ensure_agent_conversation(executor, &batch.session_iri, &batch.conversation_iri).await?;
    crate::commands::chat_storage::create_user_message(executor, &conv_iri, &batch.prompt).await?;
    let (history, _) =
        crate::commands::chat_storage::load_conversation_history(executor, &conv_iri, 120_000)
            .await?;
    let messages = history
        .iter()
        .map(crate::commands::chat::message_to_api_format)
        .collect();
    let mut tools = crate::ai::functions::get_tool_definitions();
    tools.push(completion_tool());
    for tool in tools.iter_mut() {
        crate::commands::chat::loop_tools::inject_reason_into_schema(&mut tool.input_schema);
    }

    let output = run_tool_loop(
        executor,
        app,
        &config.provider,
        messages,
        ToolLoopConfig {
            system: Some(config.system_prompt),
            tools,
            max_iterations: 100,
            temperature: None,
            completion_tool: Some(CompletionToolConfig {
                tool_name: "ingestion_complete".into(),
                output_class: None,
            }),
            progress_event: Some(ProgressEventConfig {
                event_name: "ingestion-progress".into(),
                session_iri: batch.session_iri.clone(),
                batch_iri: batch.batch_iri.clone(),
            }),
            idempotency_scope: Some(batch.batch_iri.clone()),
            persist_to: Some(PersistConfig {
                conv_iri,
                model_identifier: config.model_identifier,
            }),
            ..ToolLoopConfig::default()
        },
    )
    .await?;
    if output.completion.is_none() {
        #[cfg(test)]
        eprintln!(
            "ingestion provider ended without completion: {}",
            output.last_text
        );
        return Err("Ontologist ended without ingestion_complete; cursor remains unchanged".into());
    }
    complete_batch(executor, &batch).await?;
    if let Some(app) = app {
        emit_session_progress(
            app,
            &batch.session_iri,
            "cursor_confirmed",
            Some(&batch.batch_iri),
            None,
        );
    }
    Ok(())
}

async fn freeze_ready_batch(
    executor: &DbExecutor,
    session_iri: &str,
) -> Result<Option<Batch>, String> {
    let session = session_iri.to_string();
    let frozen = executor.write(move |conn| {
        let session_obj = Individual::get(conn, &session).map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Ingestion session {session} not found"))?;
        let cursor = value_literal(&session_obj, "foundation:ingestionCursor").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        let conversation_key = value_literal(&session_obj, "foundation:ingestionConversationKey").unwrap_or_default();
        let conv_iri = stable_iri("AIConversation", &session);
        let items = Individual::find_by_class_and_properties(
            conn,
            "foundation:IngestionItem",
            &[("foundation:ingestionSession", &session)],
        )
        .map_err(|error| error.to_string())?;
        let mut candidates = Vec::new();
        for iri in items {
            let Some(item) = Individual::get(conn, &iri).map_err(|error| error.to_string())? else { continue; };
            let sequence = value_literal(&item, "foundation:ingestionSequence").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let status = value_literal(&item, "foundation:ingestionItemStatus").unwrap_or_default();
            if sequence > cursor && status != "Completed" { candidates.push((sequence, iri, item)); }
        }
        candidates.sort_by_key(|candidate| candidate.0);
        let Some((_, _, newest)) = candidates.last() else { return Ok("null".to_string()); };
        if received_at_ms(newest).is_some_and(|time| !quiet_window_elapsed(time, chrono::Utc::now().timestamp_millis())) {
            return Ok("null".to_string());
        }
        let batch_items: Vec<_> = candidates.into_iter().filter(|(sequence, _, _)| *sequence > cursor).collect();
        let Some(upper_bound) = batch_items.last().map(|(sequence, _, _)| *sequence) else { return Ok("null".to_string()); };
        let batch_iri = stable_iri("IngestionBatch", &format!("{session}:{upper_bound}"));
        let batch_label = format!("Ingestion batch through {upper_bound}");
        if Individual::get(conn, &batch_iri).map_err(|error| error.to_string())?.is_none() {
            let batch = Individual::new(&batch_iri);
            batch.assert(conn, "foundation:IngestionBatch", &batch_label, "smart_toy", "ingestion").map_err(|error| error.to_string())?;
            batch.add_property(conn, "foundation:hasStatus", vec![Object::Iri("foundation:InProgress".into())], "ingestion").map_err(|error| error.to_string())?;
            batch.add_property(conn, "foundation:ingestionBatchUpperBound", vec![Object::Integer(upper_bound)], "ingestion").map_err(|error| error.to_string())?;
            batch.add_property(conn, "foundation:ingestionBatchStatus", vec![literal("Processing")], "ingestion").map_err(|error| error.to_string())?;
        } else {
            Individual::new(&batch_iri).add_property(conn, "foundation:ingestionBatchStatus", vec![literal("Processing")], "ingestion").map_err(|error| error.to_string())?;
            Individual::new(&batch_iri).add_property(conn, "foundation:hasStatus", vec![Object::Iri("foundation:InProgress".into())], "ingestion").map_err(|error| error.to_string())?;
        }
        let mut content = Vec::new();
        let mut item_iris = Vec::new();
        for (sequence, item_iri, item) in batch_items {
            Individual::new(&item_iri).add_property(conn, "foundation:ingestionBatch", vec![Object::Iri(batch_iri.clone())], "ingestion").map_err(|error| error.to_string())?;
            Individual::new(&item_iri).add_property(conn, "foundation:ingestionItemStatus", vec![literal("Processing")], "ingestion").map_err(|error| error.to_string())?;
            let source_iri = value_iri(&item, "foundation:ingestionSourceEntity").unwrap_or_default();
            let source = Individual::get(conn, &source_iri).map_err(|error| error.to_string())?;
            let facts = source.as_ref().map(render_entity_facts).unwrap_or_else(|| format!("Source entity unavailable: {source_iri}"));
            content.push(format!("### New content item {sequence}\nSource entity: {source_iri}\n{facts}"));
            item_iris.push(item_iri);
        }
        let prompt = format!("Ingest new content for source conversation `{conversation_key}`.\n\nPreviously confirmed through sequence {cursor}. The following exact batch contains new items only; preserve and connect facts with the conversation context already in this private session. Read linked attachments or referenced entities when needed. Record all relevant facts in the ontology, evolve the ontology when necessary, and use an automation only if you determine the batch is high-volume structured content. Do not ask the user. This batch may be retried: search for existing facts and entities before creating them, reuse stable source identities where possible, and avoid duplicating facts already recorded. After all ontology writes succeed, call `ingestion_complete`.\n\n{}", content.join("\n\n"));
        Individual::new(&session).add_property(conn, "foundation:ingestionSessionStatus", vec![literal("Processing")], "ingestion").map_err(|error| error.to_string())?;
        Individual::new(&session).add_property(conn, "foundation:ingestionProgressStage", vec![literal("Processing a frozen batch with the Ontologist")], "ingestion").map_err(|error| error.to_string())?;
        serde_json::to_string(&Some(Batch { session_iri: session, conversation_iri: conv_iri, batch_iri, upper_bound, item_iris, prompt }))
            .map_err(|error| error.to_string())
    }).await?;
    serde_json::from_str(&frozen).map_err(|error| error.to_string())
}

async fn ensure_agent_conversation(
    executor: &DbExecutor,
    session_iri: &str,
    conversation_iri: &str,
) -> Result<String, String> {
    let session = session_iri.to_string();
    let conv = conversation_iri.to_string();
    let conv_for_return = conv.clone();
    executor
        .write(move |conn| {
            if Individual::get(conn, &conv)
                .map_err(|error| error.to_string())?
                .is_none()
            {
                let item = Individual::new(&conv);
                item.assert(
                    conn,
                    "foundation:AIConversation",
                    "Background Ingestion",
                    "smart_toy",
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
                item.add_property(
                    conn,
                    "foundation:hasStatus",
                    vec![Object::Iri("foundation:Active".into())],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
                item.add_property(
                    conn,
                    "foundation:handledBy",
                    vec![Object::Iri(AGENT_IRI.into())],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
                item.add_property(
                    conn,
                    "foundation:ingestionInternalSession",
                    vec![Object::Iri(session.clone())],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
                Individual::new(&session)
                    .add_property(
                        conn,
                        "foundation:ingestionAgentConversation",
                        vec![Object::Iri(conv.clone())],
                        "ingestion",
                    )
                    .map_err(|error| error.to_string())?;
            }
            Ok(conv_for_return)
        })
        .await
}

async fn complete_batch(executor: &DbExecutor, batch: &Batch) -> Result<(), String> {
    let batch = batch.clone();
    executor
        .write(move |conn| {
            for item_iri in &batch.item_iris {
                Individual::new(item_iri)
                    .add_property(
                        conn,
                        "foundation:ingestionItemStatus",
                        vec![literal("Completed")],
                        "ingestion",
                    )
                    .map_err(|error| error.to_string())?;
                Individual::new(item_iri)
                    .add_property(
                        conn,
                        "foundation:hasStatus",
                        vec![Object::Iri("foundation:Completed".into())],
                        "ingestion",
                    )
                    .map_err(|error| error.to_string())?;
            }
            Individual::new(&batch.batch_iri)
                .add_property(
                    conn,
                    "foundation:ingestionBatchStatus",
                    vec![literal("Completed")],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Individual::new(&batch.batch_iri)
                .add_property(
                    conn,
                    "foundation:hasStatus",
                    vec![Object::Iri("foundation:Completed".into())],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Individual::new(&batch.session_iri)
                .add_property(
                    conn,
                    "foundation:ingestionCursor",
                    vec![Object::Integer(batch.upper_bound)],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Individual::new(&batch.session_iri)
                .add_property(
                    conn,
                    "foundation:ingestionSessionStatus",
                    vec![literal("Pending")],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Individual::new(&batch.session_iri)
                .add_property(
                    conn,
                    "foundation:ingestionProgressStage",
                    vec![literal(&format!(
                        "Confirmed through item {}",
                        batch.upper_bound
                    ))],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Individual::new(&batch.session_iri)
                .add_property(
                    conn,
                    "foundation:hasStatus",
                    vec![Object::Iri("foundation:Pending".into())],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Ok(String::new())
        })
        .await?;
    Ok(())
}

async fn mark_session_failed(executor: &DbExecutor, session_iri: &str, error: &str) {
    let session = session_iri.to_string();
    let detail = error.chars().take(1000).collect::<String>();
    let detail_for_log = detail.clone();
    let _ = executor
        .write(move |conn| {
            let items = Individual::find_by_class_and_properties(
                conn,
                "foundation:IngestionItem",
                &[
                    ("foundation:ingestionSession", &session),
                    ("foundation:ingestionItemStatus", "Processing"),
                ],
            )
            .map_err(|error| error.to_string())?;
            for item_iri in items {
                let Some(item) =
                    Individual::get(conn, &item_iri).map_err(|error| error.to_string())?
                else {
                    continue;
                };
                Individual::new(&item_iri)
                    .add_property(
                        conn,
                        "foundation:ingestionItemStatus",
                        vec![literal("Failed")],
                        "ingestion",
                    )
                    .map_err(|error| error.to_string())?;
                if let Some(batch_iri) = value_iri(&item, "foundation:ingestionBatch") {
                    Individual::new(&batch_iri)
                        .add_property(
                            conn,
                            "foundation:ingestionBatchStatus",
                            vec![literal("Failed")],
                            "ingestion",
                        )
                        .map_err(|error| error.to_string())?;
                    Individual::new(&batch_iri)
                        .add_property(
                            conn,
                            "foundation:ingestionFailure",
                            vec![literal(&detail)],
                            "ingestion",
                        )
                        .map_err(|error| error.to_string())?;
                    Individual::new(&batch_iri)
                        .add_property(
                            conn,
                            "foundation:hasStatus",
                            vec![Object::Iri("foundation:Blocked".into())],
                            "ingestion",
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            Individual::new(&session)
                .add_property(
                    conn,
                    "foundation:ingestionSessionStatus",
                    vec![literal("Failed")],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Individual::new(&session)
                .add_property(
                    conn,
                    "foundation:ingestionProgressStage",
                    vec![literal(
                        "Processing failed; pending items are preserved for retry",
                    )],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Individual::new(&session)
                .add_property(
                    conn,
                    "foundation:hasStatus",
                    vec![Object::Iri("foundation:Blocked".into())],
                    "ingestion",
                )
                .map_err(|error| error.to_string())?;
            Ok(String::new())
        })
        .await;
    crate::commands::log_backend(
        "error",
        &format!("[ingestion] session {session_iri} failed ({detail_for_log})"),
    );
}

fn emit_session_progress(
    app: &AppHandle,
    session_iri: &str,
    stage: &str,
    batch_iri: Option<&str>,
    completed_tool_calls: Option<usize>,
) {
    use tauri::Emitter;
    let _ = app.emit(
        "ingestion-progress",
        serde_json::json!({
            "sessionIri": session_iri,
            "batchIri": batch_iri,
            "stage": stage,
            "completedToolCalls": completed_tool_calls,
        }),
    );
}

fn completion_tool() -> ToolDefinition {
    ToolDefinition {
        name: "ingestion_complete".into(),
        description: "Call only after all ontology writes for this batch have succeeded. This confirms processing and advances the durable source cursor.".into(),
        input_schema: serde_json::json!({"type":"object","properties":{"output_iri":{"type":"string"},"output_value":{"type":"string"},"message":{"type":"string"}},"required":[]}),
    }
}

fn value_literal(individual: &Individual, property: &str) -> Option<String> {
    individual
        .properties
        .iter()
        .find(|(predicate, _)| predicate == property)
        .and_then(|(_, object)| match object {
            Object::Literal { value, .. } => Some(value.clone()),
            Object::Integer(value) => Some(value.to_string()),
            Object::Number(value) => Some(value.to_string()),
            _ => None,
        })
}

fn value_iri(individual: &Individual, property: &str) -> Option<String> {
    individual
        .properties
        .iter()
        .find(|(predicate, _)| predicate == property)
        .and_then(|(_, object)| match object {
            Object::Iri(iri) => Some(iri.clone()),
            _ => None,
        })
}

fn received_at_ms(individual: &Individual) -> Option<i64> {
    let object = &individual
        .properties
        .iter()
        .find(|(predicate, _)| predicate == "foundation:ingestionReceivedAt")?
        .1;
    let value = match object {
        Object::Literal { value, .. } | Object::DateTime(value) => value.clone(),
        _ => return None,
    };
    chrono::DateTime::parse_from_rfc3339(&value)
        .ok()
        .map(|time| time.timestamp_millis())
}

fn quiet_window_elapsed(last_received_at_ms: i64, now_ms: i64) -> bool {
    now_ms.saturating_sub(last_received_at_ms) >= QUIET_WINDOW_MS
}

fn render_entity_facts(entity: &Individual) -> String {
    let mut lines = vec![format!(
        "Entity: {}\nLabel: {}",
        entity.iri,
        entity.label.as_deref().unwrap_or(&entity.iri)
    )];
    for (predicate, value) in &entity.properties {
        // Email HTML is retained for faithful Inspector rendering, but it is a
        // noisy transport representation. Ingestion consumes emailSubject and
        // emailBody (the text/plain content) instead.
        if predicate == "foundation:hasStatus" || predicate == "foundation:emailBodyHtml" {
            continue;
        }
        let rendered = match value {
            Object::Iri(value) => value.clone(),
            Object::Literal {
                value,
                datatype,
                language,
            } => format!(
                "{value}{}{}",
                datatype
                    .as_deref()
                    .map(|value| format!("^^{value}"))
                    .unwrap_or_default(),
                language
                    .as_deref()
                    .map(|value| format!("@{value}"))
                    .unwrap_or_default()
            ),
            Object::Integer(value) => value.to_string(),
            Object::Number(value) => value.to_string(),
            Object::Boolean(value) => value.to_string(),
            Object::DateTime(value) => value.clone(),
            Object::Blank(value) => value.clone(),
        };
        lines.push(format!("{predicate}: {rendered}"));
    }
    lines.join("\n")
}

fn literal(value: &str) -> Object {
    Object::Literal {
        value: value.to_string(),
        datatype: Some("xsd:string".into()),
        language: None,
    }
}

fn stable_iri(kind: &str, source: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(source.as_bytes());
    format!(
        "foundation:{kind}_{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        process_session_with_config, quiet_window_elapsed, received_at_ms, render_entity_facts,
    };
    use crate::core_ontology::ingestion::individuals_of_class;
    use crate::owl::{DbExecutor, Individual, Object};

    #[test]
    fn debounce_waits_for_a_full_quiet_window_after_latest_item() {
        assert!(!quiet_window_elapsed(10_000, 12_999));
        assert!(quiet_window_elapsed(10_000, 13_000));
        assert!(!quiet_window_elapsed(13_000, 12_000));
    }

    #[test]
    fn received_at_parses_datetime_values_from_the_ontology() {
        let mut individual = Individual::new("foundation:IngestionItem_fixture");
        individual.properties.push((
            "foundation:ingestionReceivedAt".into(),
            Object::Literal {
                value: "2026-11-17T12:00:00Z".into(),
                datatype: Some("xsd:dateTime".into()),
                language: None,
            },
        ));

        assert_eq!(received_at_ms(&individual), Some(1_794_916_800_000));
    }

    #[test]
    fn email_ingestion_uses_text_body_and_excludes_rendering_html() {
        // Sanitized excerpt from a real LinkedIn job-alert email in the local
        // mailbox. Links, tracking tokens, recipient identity, and footer are
        // intentionally omitted from this repository fixture.
        let expected_entries = [
            (
                "Applied AI Engineer, Digital Natives",
                "OpenAI",
                "São Paulo e Região",
            ),
            ("Staff AI Engineer", "Serasa Experian", "São Paulo, SP"),
            (
                "Engineering Manager – Brazil – Vaga para mulheres",
                "DoorDash",
                "São Paulo, SP",
            ),
            ("Lead Software Engineer - BR", "Nubank", "São Paulo, SP"),
            (
                "Gerente de Projetos de Tecnologia e Inteligência Artificial",
                "Accenture Brasil",
                "São Paulo, SP",
            ),
            ("Software Engineer Lead", "Kyndryl", "São Paulo, SP"),
        ];
        let body = expected_entries
            .iter()
            .map(|(title, company, location)| format!("{title}\n{company}\n{location}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut email = Individual::new("foundation:Email_fixture");
        email.label = Some("Job alert".into());
        email.properties.extend([
            (
                "foundation:emailBody".into(),
                Object::Literal {
                    value: body,
                    datatype: Some("xsd:string".into()),
                    language: None,
                },
            ),
            (
                "foundation:emailBodyHtml".into(),
                Object::Literal {
                    value: "<html><script>tracking noise</script></html>".into(),
                    datatype: Some("rdf:HTML".into()),
                    language: None,
                },
            ),
        ]);

        let facts = render_entity_facts(&email);
        for (title, company, location) in expected_entries {
            assert!(facts.contains(title), "Missing title: {title}");
            assert!(facts.contains(company), "Missing company: {company}");
            assert!(facts.contains(location), "Missing location: {location}");
        }
        assert!(!facts.contains("emailBodyHtml"));
        assert!(!facts.contains("tracking noise"));
    }

    #[tokio::test]
    #[ignore = "runs the Ontologist tool loop with the configured ChatGPT subscription against an isolated database"]
    async fn provider_eval_ingests_email_through_the_durable_queue_and_persists_expected_facts() {
        use crate::core_ontology::ingestion::{enqueue_source_item, sync_schema};
        use crate::owl::{Connection, Object};
        use std::collections::HashSet;

        let source_db_path = crate::paths::foundation_dir().join("FOUNDATION.db");
        let source_connection = Connection::open(&source_db_path)
            .expect("Could not open the local database to resolve the configured provider");
        let source_executor = DbExecutor::new(source_connection, source_db_path);
        let config =
            crate::process_automation::agent_runner::resolve_agent_config_without_request_deadline(
                &source_executor,
                super::AGENT_IRI,
            )
            .await
            .expect("The Ontologist must have a configured provider and model");
        assert!(
            config.model_identifier.to_lowercase().contains("luna"),
            "Expected the approved Luna model, got {}",
            config.model_identifier
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("isolated-ingestion-eval.db");
        let mut conn = crate::owl::initialize_db(&db_path).unwrap();
        crate::owl::seed_icon_library(&mut conn);
        sync_schema(&mut conn).unwrap();
        for (iri, label) in [
            ("foundation:Pending", "Pending"),
            ("foundation:InProgress", "In Progress"),
            ("foundation:Completed", "Completed"),
            ("foundation:Blocked", "Blocked"),
            ("foundation:Active", "Active"),
        ] {
            if Individual::get(&conn, iri).unwrap().is_none() {
                Individual::new(iri)
                    .assert(
                        &mut conn,
                        "foundation:Status",
                        label,
                        "task_alt",
                        "ingestion_eval",
                    )
                    .unwrap();
            }
        }
        if Individual::get(&conn, super::AGENT_IRI).unwrap().is_none() {
            Individual::new(super::AGENT_IRI)
                .assert(
                    &mut conn,
                    "foundation:SoftwareAgent",
                    "Ontologist",
                    "smart_toy",
                    "ingestion_eval",
                )
                .unwrap();
        }

        let email_iri = "foundation:Email_IngestionProviderEval";
        let conversation_key = "provider-eval-linkedin-job-alert";
        let expected = [
            (
                "Applied AI Engineer, Digital Natives",
                "OpenAI",
                "São Paulo e Região",
            ),
            ("Staff AI Engineer", "Serasa Experian", "São Paulo, SP"),
            (
                "Engineering Manager – Brazil – Vaga para mulheres",
                "DoorDash",
                "São Paulo, SP",
            ),
            ("Lead Software Engineer - BR", "Nubank", "São Paulo, SP"),
            (
                "Gerente de Projetos de Tecnologia e Inteligência Artificial",
                "Accenture Brasil",
                "São Paulo, SP",
            ),
            ("Software Engineer Lead", "Kyndryl", "São Paulo, SP"),
        ];
        let text_body = expected
            .iter()
            .map(|(title, company, location)| format!("{title}\n{company}\n{location}"))
            .collect::<Vec<_>>()
            .join("\n\n");

        let email = Individual::new(email_iri);
        email
            .assert(
                &mut conn,
                "foundation:Email",
                "LinkedIn job alert",
                "mail",
                "ingestion_eval",
            )
            .unwrap();
        email
            .add_property(
                &mut conn,
                "foundation:emailSubject",
                vec![Object::Literal {
                    value: "Seu alerta de vaga para Líder técnico em São Paulo".into(),
                    datatype: Some("xsd:string".into()),
                    language: None,
                }],
                "ingestion_eval",
            )
            .unwrap();
        email
            .add_property(
                &mut conn,
                "foundation:emailBody",
                vec![Object::Literal {
                    value: text_body,
                    datatype: Some("xsd:string".into()),
                    language: None,
                }],
                "ingestion_eval",
            )
            .unwrap();
        email
            .add_property(
                &mut conn,
                "foundation:emailBodyHtml",
                vec![Object::Literal {
                    value: "<html><script>tracking noise</script>not ingestion input</html>".into(),
                    datatype: Some("rdf:HTML".into()),
                    language: None,
                }],
                "ingestion_eval",
            )
            .unwrap();

        enqueue_source_item(
            &mut conn,
            "imap_email",
            conversation_key,
            email_iri,
            "provider-eval:email-1",
        )
        .unwrap();
        let items = individuals_of_class(&conn, "foundation:IngestionItem").unwrap();
        assert_eq!(items.len(), 1);
        Individual::new(&items[0])
            .add_property(
                &mut conn,
                "foundation:ingestionReceivedAt",
                vec![Object::Literal {
                    value: (chrono::Utc::now() - chrono::Duration::seconds(10)).to_rfc3339(),
                    datatype: Some("xsd:dateTime".into()),
                    language: None,
                }],
                "ingestion_eval",
            )
            .unwrap();
        drop(conn);

        let executor = DbExecutor::new(
            crate::owl::Connection::open(&db_path).unwrap(),
            db_path.clone(),
        );
        let session_iri = executor
            .read(|conn| {
                let mut sessions = individuals_of_class(conn, "foundation:IngestionSession")?;
                sessions.sort();
                sessions.into_iter().next().ok_or_else(|| {
                    "The queued email did not create an ingestion session".to_string()
                })
            })
            .await
            .unwrap();
        let model_identifier = config.model_identifier.clone();
        process_session_with_config(None, &executor, &session_iri, config)
            .await
            .expect("The full queue → Ontologist → persistence → cursor flow should succeed");

        let session_iri_for_verify = session_iri.clone();
        let stored_values = executor
            .read(move |conn| {
                let mut statement = conn
                    .prepare(
                        "SELECT t.object_value FROM triples t \
                         WHERE t.retracted = 0 AND t.object_value IS NOT NULL \
                         AND t.subject NOT IN ( \
                           SELECT subject FROM triples \
                           WHERE predicate = 'rdf:type' AND object = 'foundation:AIConversationMessage' \
                           AND retracted = 0 \
                         )",
                    )
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|error| error.to_string())?;
                let mut values = HashSet::new();
                for row in rows {
                    values.insert(row.map_err(|error| error.to_string())?);
                }
                let session = Individual::get(conn, &session_iri_for_verify)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "The ingestion session disappeared".to_string())?;
                let cursor = session
                    .properties
                    .iter()
                    .find(|(property, _)| property == "foundation:ingestionCursor")
                    .and_then(|(_, value)| match value {
                        Object::Integer(value) => Some(*value),
                        _ => None,
                    })
                    .unwrap_or_default();
                Ok(serde_json::json!({"values": values, "cursor": cursor}))
            })
            .await
            .unwrap();
        let values: HashSet<String> =
            serde_json::from_value(stored_values["values"].clone()).unwrap();
        let cursor = stored_values["cursor"].as_i64().unwrap_or_default();
        assert_eq!(
            cursor, 1,
            "The durable cursor must advance only after Ontologist completion"
        );

        let normalized_values: HashSet<String> = values
            .iter()
            .map(|value| {
                value
                    .chars()
                    .flat_map(char::to_lowercase)
                    .filter(|character| character.is_alphanumeric())
                    .collect()
            })
            .collect();
        let matched = expected
            .iter()
            .filter(|(title, company, location)| {
                [title, company, location].iter().all(|fact| {
                    let normalized_fact: String = fact
                        .chars()
                        .flat_map(char::to_lowercase)
                        .filter(|character| character.is_alphanumeric())
                        .collect();
                    normalized_values
                        .iter()
                        .any(|value| value.contains(&normalized_fact))
                })
            })
            .count();
        eprintln!(
            "integrated ingestion eval: model={} matched_postings={}/{} cursor={}",
            model_identifier,
            matched,
            expected.len(),
            cursor,
        );
        assert!(
            matched >= 5,
            "At least five of six expected postings must be persisted as ontology facts; matched {matched}"
        );
    }
}
