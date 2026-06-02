use std::cell::RefCell;
use serde_json::Value;
use rusqlite::Connection;
use crate::eavto::enter_batch_transaction;
use super::ToolResult;

thread_local! {
    static PENDING_EVENTS: RefCell<Vec<(String, Value)>> = const { RefCell::new(Vec::new()) };
    static PENDING_PROPERTY_RECALC: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static PENDING_INSTANCE_RECALC: RefCell<Vec<(String, String)>> =
        const { RefCell::new(Vec::new()) };
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
        Some(_) => return ToolResult {
            success: false,
            result: None,
            error: Some("Operations array must not be empty".to_string()),
            concept: None,
        },
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Arguments must be a non-empty array of operations".to_string()),
            concept: None,
        },
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
        Some(_) => return ToolResult {
            success: false,
            result: None,
            error: Some("Operations array must not be empty".to_string()),
            concept: None,
        },
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Arguments must be a non-empty array of operations".to_string()),
            concept: None,
        },
    };

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
        for (name, payload) in take_events() {
            crate::realtime::emit_queued(app_handle, &name, payload);
        }

        dispatch_formula_recalcs(conn, app_handle);
    } else {
        take_events();
        take_property_recalcs();
        take_instance_recalcs();
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "operationsCompleted": results.len(),
            "results": results,
        })),
        error: None,
        concept: None,
    }
}

fn dispatch_formula_recalcs(conn: &Connection, app: &tauri::AppHandle) {
    use crate::owl::formula_worker::{FormulaWorker, WorkerCommand};
    use tauri::Manager;

    let worker = match app.try_state::<FormulaWorker>() {
        Some(w) => w,
        None => {
            take_property_recalcs();
            take_instance_recalcs();
            return;
        }
    };

    for property_iri in take_property_recalcs() {
        if let Some(job_id) =
            crate::owl::formula_worker::cancel_and_create_property_job(conn, &property_iri)
        {
            let _ = worker.sender.try_send(WorkerCommand::Enqueue { job_id });
        }
    }

    for (instance_iri, changed_property_iri) in take_instance_recalcs() {
        let job_ids = crate::owl::formula_worker::create_instance_recalc_jobs(
            conn, &instance_iri, &changed_property_iri,
        );
        for job_id in job_ids {
            let _ = worker.sender.try_send(WorkerCommand::Enqueue { job_id });
        }
    }
}
