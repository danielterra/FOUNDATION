use super::ToolResult;
use crate::eavto::enter_batch_transaction;
use crate::owl::{Individual, Object};
use rusqlite::Connection;
use serde_json::Value;
use std::cell::RefCell;

#[derive(Clone)]
pub(crate) struct IngestionIdempotencyContext {
    pub action_iri: String,
    pub batch_iri: String,
    pub tool_name: String,
    pub input_hash: String,
}

thread_local! {
    static PENDING_EVENTS: RefCell<Vec<(String, Value)>> = const { RefCell::new(Vec::new()) };
    static PENDING_PROPERTY_RECALC: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static PENDING_INSTANCE_RECALC: RefCell<Vec<(String, String)>> =
        const { RefCell::new(Vec::new()) };
    static INGESTION_IDEMPOTENCY: RefCell<Option<IngestionIdempotencyContext>> = const { RefCell::new(None) };
}

struct IdempotencyContextGuard(Option<IngestionIdempotencyContext>);

impl Drop for IdempotencyContextGuard {
    fn drop(&mut self) {
        let previous = self.0.take();
        INGESTION_IDEMPOTENCY.with(|context| *context.borrow_mut() = previous);
    }
}

pub(crate) fn with_ingestion_idempotency<R>(
    context: IngestionIdempotencyContext,
    run: impl FnOnce() -> R,
) -> R {
    let previous = INGESTION_IDEMPOTENCY.with(|slot| slot.replace(Some(context)));
    let _guard = IdempotencyContextGuard(previous);
    run()
}

pub(crate) fn ingestion_action_context(
    batch_iri: &str,
    tool_name: &str,
    arguments: &Value,
) -> IngestionIdempotencyContext {
    use sha2::{Digest, Sha256};

    let canonical_arguments = canonical_json(arguments);
    let input_hash = format!(
        "sha256:{:x}",
        Sha256::digest(canonical_arguments.as_bytes())
    );
    let identity = format!("{batch_iri}\0{tool_name}\0{input_hash}");
    let action_hash = Sha256::digest(identity.as_bytes());
    let action_iri = format!(
        "foundation:IngestionAction_{}",
        action_hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    IngestionIdempotencyContext {
        action_iri,
        batch_iri: batch_iri.to_string(),
        tool_name: tool_name.to_string(),
        input_hash,
    }
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let members = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("JSON object keys serialize"),
                        canonical_json(&object[key])
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", members.join(","))
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(value).expect("JSON values serialize"),
    }
}

pub(crate) fn active_ingestion_idempotency() -> Option<IngestionIdempotencyContext> {
    INGESTION_IDEMPOTENCY.with(|slot| slot.borrow().clone())
}

pub(super) fn queue_event(name: &str, payload: Value) {
    PENDING_EVENTS.with(|e| e.borrow_mut().push((name.to_string(), payload)));
}

pub(super) fn queue_formula_recalc(property_iri: String) {
    PENDING_PROPERTY_RECALC.with(|q| q.borrow_mut().push(property_iri));
}

pub(super) fn queue_instance_recalc(instance_iri: String, changed_property_iri: String) {
    PENDING_INSTANCE_RECALC.with(|q| q.borrow_mut().push((instance_iri, changed_property_iri)));
}

fn take_events() -> Vec<(String, Value)> {
    PENDING_EVENTS.with(|e| e.borrow_mut().drain(..).collect())
}

fn take_property_recalcs() -> Vec<String> {
    PENDING_PROPERTY_RECALC.with(|q| q.borrow_mut().drain(..).collect())
}

fn take_instance_recalcs() -> Vec<(String, String)> {
    PENDING_INSTANCE_RECALC.with(|q| q.borrow_mut().drain(..).collect())
}

pub(super) fn run_multi_read(
    conn: &Connection,
    args: &Value,
    exec_one: impl Fn(&Connection, &Value) -> ToolResult,
) -> ToolResult {
    let ops = match args.as_array() {
        Some(ops) if !ops.is_empty() => ops.clone(),
        Some(_) => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Operations array must not be empty".to_string()),
                concept: None,
            }
        }
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Arguments must be a non-empty array of operations".to_string()),
                concept: None,
            }
        }
    };

    let mut results = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        let result = exec_one(conn, op);
        if !result.success {
            return ToolResult {
                success: false,
                result: result.result,
                error: Some(format!(
                    "Operation {} failed: {}",
                    i,
                    result.error.unwrap_or_else(|| "unknown error".to_string()),
                )),
                concept: result.concept,
            };
        }
        results.push(result.result);
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({ "results": results })),
        error: None,
        concept: None,
    }
}

pub(super) fn run_atomic(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
    exec_one: impl Fn(&mut Connection, &Value) -> ToolResult,
) -> ToolResult {
    let ops = match args.as_array() {
        Some(ops) if !ops.is_empty() => ops.clone(),
        Some(_) => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Operations array must not be empty".to_string()),
                concept: None,
            }
        }
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Arguments must be a non-empty array of operations".to_string()),
                concept: None,
            }
        }
    };

    if let Some(context) = active_ingestion_idempotency() {
        match load_ingestion_action_result(conn, &context.action_iri) {
            Ok(Some(result)) => return result,
            Ok(None) => {}
            Err(error) => {
                return ToolResult {
                    success: false,
                    result: None,
                    error: Some(format!(
                        "Failed to inspect idempotent ingestion action: {error}"
                    )),
                    concept: None,
                }
            }
        }
    }

    take_events();

    if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE") {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to start transaction: {e}")),
            concept: None,
        };
    }

    let _guard = enter_batch_transaction();
    let mut results = Vec::new();

    for (i, op) in ops.iter().enumerate() {
        let result = exec_one(conn, op);
        if !result.success {
            conn.execute_batch("ROLLBACK").ok();
            take_events();
            return ToolResult {
                success: false,
                result: result.result,
                error: Some(format!(
                    "Operation {} failed: {}",
                    i,
                    result.error.unwrap_or_else(|| "unknown error".to_string()),
                )),
                concept: result.concept,
            };
        }
        results.push(result.result);
    }

    let output = ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "operationsCompleted": results.len(),
            "results": results,
        })),
        error: None,
        concept: None,
    };

    if let Some(context) = active_ingestion_idempotency() {
        if let Err(error) = persist_ingestion_action_result(conn, &context, &output) {
            conn.execute_batch("ROLLBACK").ok();
            take_events();
            return ToolResult {
                success: false,
                result: None,
                error: Some(format!(
                    "Failed to commit idempotent ingestion action: {error}"
                )),
                concept: None,
            };
        }
    }

    let mut pending_events = take_events();
    for (_, payload) in &mut pending_events {
        if let Some(entity_id) = payload.get("entityId").and_then(serde_json::Value::as_str) {
            let tx = match crate::owl::entity_last_tx(conn, entity_id) {
                Ok(Some(tx)) if tx > 0 => tx,
                Ok(_) => {
                    conn.execute_batch("ROLLBACK").ok();
                    take_events();
                    return ToolResult {
                        success: false,
                        result: None,
                        error: Some(format!("No transaction revision exists for {entity_id}")),
                        concept: None,
                    };
                }
                Err(error) => {
                    conn.execute_batch("ROLLBACK").ok();
                    take_events();
                    return ToolResult {
                        success: false,
                        result: None,
                        error: Some(format!(
                            "Failed to identify revision for {entity_id}: {error}"
                        )),
                        concept: None,
                    };
                }
            };
            payload["tx"] = serde_json::Value::Number(tx.into());
        }
    }

    if let Err(e) = conn.execute_batch("COMMIT") {
        conn.execute_batch("ROLLBACK").ok();
        take_events();
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to commit transaction: {e}")),
            concept: None,
        };
    }

    if let Some(app_handle) = app {
        for (name, payload) in pending_events {
            crate::realtime::emit_queued(app_handle, &name, payload);
        }

        dispatch_formula_recalcs(conn, app_handle);
    } else {
        take_events();
        take_property_recalcs();
        take_instance_recalcs();
    }

    output
}

fn load_ingestion_action_result(
    conn: &Connection,
    action_iri: &str,
) -> Result<Option<ToolResult>, String> {
    let Some(action) = Individual::get(conn, action_iri).map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let serialized = action
        .properties
        .iter()
        .find(|(property, _)| property == "foundation:ingestionActionResult")
        .and_then(|(_, value)| value.as_literal())
        .ok_or_else(|| format!("Idempotency record {action_iri} has no stored result"))?;
    serde_json::from_str(&serialized)
        .map(Some)
        .map_err(|error| format!("Idempotency record {action_iri} has invalid result: {error}"))
}

fn persist_ingestion_action_result(
    conn: &mut Connection,
    context: &IngestionIdempotencyContext,
    result: &ToolResult,
) -> Result<(), String> {
    let serialized = serde_json::to_string(result).map_err(|error| error.to_string())?;
    let action = Individual::new(&context.action_iri);
    action
        .assert(
            conn,
            "foundation:IngestionAction",
            &format!("{} · {}", context.tool_name, &context.input_hash[..12]),
            "home",
            "ingestion",
        )
        .map_err(|error| error.to_string())?;
    action
        .add_property(
            conn,
            "foundation:ingestionActionStatus",
            vec![Object::Literal {
                value: "Completed".into(),
                datatype: Some("xsd:string".into()),
                language: None,
            }],
            "ingestion",
        )
        .map_err(|error| error.to_string())?;
    action
        .add_property(
            conn,
            "foundation:ingestionActionBatch",
            vec![Object::Iri(context.batch_iri.clone())],
            "ingestion",
        )
        .map_err(|error| error.to_string())?;
    action
        .add_property(
            conn,
            "foundation:ingestionActionTool",
            vec![Object::Literal {
                value: context.tool_name.clone(),
                datatype: Some("xsd:string".into()),
                language: None,
            }],
            "ingestion",
        )
        .map_err(|error| error.to_string())?;
    action
        .add_property(
            conn,
            "foundation:ingestionActionInputHash",
            vec![Object::Literal {
                value: context.input_hash.clone(),
                datatype: Some("xsd:string".into()),
                language: None,
            }],
            "ingestion",
        )
        .map_err(|error| error.to_string())?;
    action
        .add_property(
            conn,
            "foundation:ingestionActionResult",
            vec![Object::Literal {
                value: serialized,
                datatype: Some("xsd:string".into()),
                language: None,
            }],
            "ingestion",
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn dispatch_formula_recalcs(_conn: &Connection, app: &tauri::AppHandle) {
    use crate::commands::formula_worker::FormulaWorker;
    use tauri::Manager;

    if app.try_state::<FormulaWorker>().is_none() {
        take_property_recalcs();
        take_instance_recalcs();
        return;
    }

    let properties = take_property_recalcs();
    let instances = take_instance_recalcs();
    let executor = match app.try_state::<crate::owl::DbExecutor>() {
        Some(executor) => executor.inner().clone(),
        None => return,
    };
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        for property_iri in properties {
            match crate::commands::formula_worker::cancel_and_create_property_job(
                &executor,
                &property_iri,
            )
            .await
            {
                Ok(job) => {
                    if let Some(worker) = app_handle.try_state::<FormulaWorker>() {
                        if let Err(error) = worker.enqueue(job, vec![], vec![]) {
                            crate::commands::log_backend("error", &format!("Failed to enqueue formula property recalculation {property_iri}: {error}"));
                        }
                    }
                }
                Err(error) => crate::commands::log_backend(
                    "error",
                    &format!(
                        "Failed to enqueue formula property recalculation {property_iri}: {error}"
                    ),
                ),
            }
        }
        for (instance_iri, changed_property_iri) in instances {
            match crate::commands::formula_worker::create_instance_recalc_jobs(&executor, &instance_iri, &changed_property_iri).await {
                Ok(jobs) => for job in jobs {
                    if let Some(worker) = app_handle.try_state::<FormulaWorker>() {
                        if let Err(error) = worker.enqueue(job, vec![], vec![]) {
                            crate::commands::log_backend("error", &format!("Failed to enqueue formula instance recalculation for {instance_iri}: {error}"));
                        }
                    }
                },
                Err(error) => crate::commands::log_backend("error", &format!("Failed to enqueue formula instance recalculation for {instance_iri}: {error}")),
            }
        }
    });
}

#[cfg(test)]
mod idempotency_tests {
    use super::{ingestion_action_context, run_atomic, with_ingestion_idempotency};
    use crate::ai::functions::ToolResult;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::Individual;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn action_key_ignores_object_key_order_but_scopes_to_batch_and_tool() {
        let first = ingestion_action_context(
            "foundation:IngestionBatch_a",
            "assert_individual",
            &json!({"iri":"foundation:Person_1", "label":"Ana"}),
        );
        let reordered = ingestion_action_context(
            "foundation:IngestionBatch_a",
            "assert_individual",
            &json!({"label":"Ana", "iri":"foundation:Person_1"}),
        );
        let other_batch = ingestion_action_context(
            "foundation:IngestionBatch_b",
            "assert_individual",
            &json!({"iri":"foundation:Person_1", "label":"Ana"}),
        );
        let other_tool = ingestion_action_context(
            "foundation:IngestionBatch_a",
            "replace_property_values",
            &json!({"iri":"foundation:Person_1", "label":"Ana"}),
        );

        assert_eq!(first.action_iri, reordered.action_iri);
        assert_ne!(first.action_iri, other_batch.action_iri);
        assert_ne!(first.action_iri, other_tool.action_iri);
    }

    #[test]
    fn committed_write_replays_result_without_running_mutation_again() {
        let mut conn = setup_test_db();
        crate::owl::seed_icon_library(&mut conn);
        crate::core_ontology::ingestion::sync_schema(&mut conn).unwrap();
        Individual::new("foundation:IngestionBatch_fixture")
            .assert(
                &mut conn,
                "foundation:IngestionBatch",
                "Fixture Batch",
                "home",
                "test",
            )
            .unwrap();
        let context = ingestion_action_context(
            "foundation:IngestionBatch_fixture",
            "assert_individual",
            &json!({"iri":"foundation:IngestionFixture_1"}),
        );
        let mutation_count = AtomicUsize::new(0);
        let arguments = json!([{}]);

        let first = with_ingestion_idempotency(context.clone(), || {
            run_atomic(&mut conn, &arguments, None, |_conn, _operation| {
                mutation_count.fetch_add(1, Ordering::SeqCst);
                ToolResult {
                    success: true,
                    result: Some(json!({"iri":"foundation:IngestionFixture_1"})),
                    error: None,
                    concept: None,
                }
            })
        });
        let second = with_ingestion_idempotency(context, || {
            run_atomic(&mut conn, &arguments, None, |_conn, _operation| {
                mutation_count.fetch_add(1, Ordering::SeqCst);
                ToolResult {
                    success: true,
                    result: Some(json!({"iri":"foundation:IngestionFixture_1"})),
                    error: None,
                    concept: None,
                }
            })
        });

        assert!(first.success, "{:?}", first.error);
        assert!(second.success);
        assert_eq!(first.result, second.result);
        assert_eq!(mutation_count.load(Ordering::SeqCst), 1);
    }
}
