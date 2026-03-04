use serde_json::Value;
use rusqlite::Connection;
use tauri::Emitter;
use crate::eavto::enter_batch_transaction;
use super::{ToolCall, ToolResult, execute_tool};

pub fn batch_operations(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let operations = match args.get("operations").and_then(|v| v.as_array()) {
        Some(ops) => ops.clone(),
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: operations (must be an array)".to_string()),
        },
    };

    if operations.is_empty() {
        return ToolResult {
            success: false,
            result: None,
            error: Some("operations array must not be empty".to_string()),
        };
    }

    let outcome = run_batch(conn, &operations, app);

    match outcome {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}

fn run_batch(
    conn: &mut Connection,
    operations: &[Value],
    app: Option<&tauri::AppHandle>,
) -> Result<Value, String> {
    conn.execute_batch("BEGIN")
        .map_err(|e| format!("Failed to start transaction: {e}"))?;

    // The guard clears IN_BATCH_TX when run_batch returns (on both success and error paths).
    // This is safe because after COMMIT/ROLLBACK below, the outer transaction is gone.
    let _guard = enter_batch_transaction();

    let mut results = Vec::new();

    for (op_index, op) in operations.iter().enumerate() {
        let tool_name = match op.get("tool").and_then(|v| v.as_str()) {
            Some(name) => name,
            None => {
                conn.execute_batch("ROLLBACK").ok();
                return Err(format!("Operation {op_index}: missing 'tool' field"));
            }
        };

        let arguments = op.get("arguments")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));

        let call = ToolCall {
            name: tool_name.to_string(),
            arguments,
        };

        let result = execute_tool(conn, &call, app);

        if !result.success {
            let err = result.error.unwrap_or_else(|| "unknown error".to_string());
            conn.execute_batch("ROLLBACK").ok();
            return Err(format!("Operation {op_index} ({tool_name}) failed: {err}"));
        }

        results.push(serde_json::json!({
            "index": op_index,
            "tool": tool_name,
            "result": result.result,
        }));
    }

    conn.execute_batch("COMMIT").map_err(|e| {
        conn.execute_batch("ROLLBACK").ok();
        format!("Failed to commit transaction: {e}")
    })?;

    if let Some(app_handle) = app {
        app_handle.emit("batch-completed", serde_json::json!({"count": results.len()})).ok();
    }

    Ok(serde_json::json!({
        "success": true,
        "operationsCompleted": results.len(),
        "results": results,
    }))
}
