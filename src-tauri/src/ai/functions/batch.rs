use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use serde_json::Value;
use turso::Connection;
use tauri::Emitter;
use super::ToolResult;

static BATCH_SP_COUNTER: AtomicU64 = AtomicU64::new(0);

static PENDING_EVENTS: Mutex<Vec<(String, Value)>> = Mutex::new(Vec::new());

pub(super) fn queue_event(name: &str, payload: Value) {
    if let Ok(mut events) = PENDING_EVENTS.lock() {
        events.push((name.to_string(), payload));
    }
}

fn take_events() -> Vec<(String, Value)> {
    PENDING_EVENTS.lock().map(|mut e| e.drain(..).collect()).unwrap_or_default()
}

pub type AsyncOpFn = for<'a> fn(&'a Connection, &'a Value) -> Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>>;

pub(super) async fn run_multi_read(
    conn: &Connection,
    args: &Value,
    exec_one: AsyncOpFn,
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
        let result = exec_one(conn, op).await;
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

pub(super) async fn run_atomic(
    conn: &Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
    exec_one: AsyncOpFn,
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

    let sp = format!("batch_{}", BATCH_SP_COUNTER.fetch_add(1, Ordering::Relaxed));

    if let Err(e) = conn.execute(&format!("SAVEPOINT {sp}"), ()).await {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to start transaction: {e}")),
            concept: None,
        };
    }

    let mut results = Vec::new();

    for (i, op) in ops.iter().enumerate() {
        let result = exec_one(conn, op).await;
        if !result.success {
            let _ = conn.execute(&format!("ROLLBACK TO {sp}"), ()).await;
            let _ = conn.execute(&format!("RELEASE {sp}"), ()).await;
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

    if let Err(e) = conn.execute(&format!("RELEASE {sp}"), ()).await {
        let _ = conn.execute(&format!("ROLLBACK TO {sp}"), ()).await;
        let _ = conn.execute(&format!("RELEASE {sp}"), ()).await;
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
