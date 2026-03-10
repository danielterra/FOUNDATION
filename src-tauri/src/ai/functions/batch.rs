use std::cell::RefCell;
use serde_json::Value;
use rusqlite::Connection;
use tauri::Emitter;
use crate::eavto::enter_batch_transaction;
use super::ToolResult;

thread_local! {
    static PENDING_EVENTS: RefCell<Vec<(String, Value)>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn queue_event(name: &str, payload: Value) {
    PENDING_EVENTS.with(|e| e.borrow_mut().push((name.to_string(), payload)));
}

fn take_events() -> Vec<(String, Value)> {
    PENDING_EVENTS.with(|e| e.borrow_mut().drain(..).collect())
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

    if let Err(e) = conn.execute_batch("BEGIN") {
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
            app_handle.emit(&name, payload).ok();
        }
    } else {
        take_events();
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
