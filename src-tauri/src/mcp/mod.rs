use axum::{Router, extract::State, routing::post, Json, response::{IntoResponse, Response}};
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::ai::functions::{FunctionCall, FunctionResult, execute_function, get_available_functions};
use crate::eavto::DbExecutor;

const PORT: u16 = 47177;

#[derive(Clone)]
struct McpState {
    app: Arc<AppHandle>,
}

pub async fn serve(app: AppHandle) {
    let state = McpState { app: Arc::new(app) };
    let router = Router::new()
        .route("/mcp", post(handle_mcp))
        .with_state(state);

    let addr = format!("127.0.0.1:{PORT}");
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            crate::commands::log_backend("error", &format!("MCP server failed to bind on {addr}: {e}"));
            return;
        }
    };

    crate::commands::log_backend("info", &format!("MCP server listening on http://{addr}/mcp"));
    let _ = axum::serve(listener, router).await;
}

async fn handle_mcp(
    State(state): State<McpState>,
    Json(req): Json<Value>,
) -> Response {
    let method = req["method"].as_str().unwrap_or("");
    let id = req.get("id").cloned().unwrap_or(Value::Null);

    // Notifications are fire-and-forget — MCP spec requires HTTP 202 with empty body
    if method.starts_with("notifications/") {
        return StatusCode::ACCEPTED.into_response();
    }

    match method {
        "initialize" => {
            // Echo back the client's protocol version if we support it
            let protocol_version = match req["params"]["protocolVersion"].as_str() {
                Some("2025-03-26") => "2025-03-26",
                _ => "2024-11-05",
            };
            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": protocol_version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "foundation", "version": env!("CARGO_PKG_VERSION") }
                }
            })).into_response()
        }

        "tools/list" => {
            let tools: Vec<Value> = get_available_functions()
                .into_iter()
                .map(to_mcp_tool)
                .collect();
            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tools }
            })).into_response()
        }

        "tools/call" => {
            let name = match req["params"]["name"].as_str() {
                Some(n) => n.to_string(),
                None => return Json(error_response(id, -32602, "Missing tool name")).into_response(),
            };

            // Use the shared DbExecutor — same path as the internal AI in chat.rs
            let executor = match state.app.try_state::<DbExecutor>() {
                Some(e) => e.inner().clone(),
                None => return Json(error_response(
                    id,
                    -32603,
                    "Foundation not initialized yet — please wait for the app to finish loading",
                )).into_response(),
            };

            let app = (*state.app).clone();
            let call = FunctionCall { name, arguments: req["params"]["arguments"].clone() };

            let result_json = match executor.write(move |conn| -> Result<String, String> {
                let result = execute_function(conn, &call, Some(&app));
                serde_json::to_string(&result).map_err(|e| e.to_string())
            }).await {
                Ok(json) => json,
                Err(e) => return Json(error_response(id, -32603, &e)).into_response(),
            };

            let func_result: FunctionResult = match serde_json::from_str(&result_json) {
                Ok(r) => r,
                Err(e) => return Json(error_response(id, -32603, &e.to_string())).into_response(),
            };

            if func_result.success {
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": func_result.result.unwrap_or_default().to_string() }]
                    }
                })).into_response()
            } else {
                Json(error_response(
                    id,
                    -32603,
                    &func_result.error.unwrap_or_else(|| "Tool execution failed".to_string()),
                )).into_response()
            }
        }

        _ => Json(error_response(id, -32601, "Method not found")).into_response(),
    }
}

fn to_mcp_tool(f: crate::ai::functions::FunctionDefinition) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in f.parameters {
        properties.insert(
            param.name.clone(),
            json!({ "type": param.param_type, "description": param.description }),
        );
        if param.required {
            required.push(param.name);
        }
    }

    json!({
        "name": f.name,
        "description": f.description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required
        }
    })
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
