use axum::{Router, extract::State, routing::post, Json, response::{IntoResponse, Response}};
use axum::http::StatusCode;
use axum_server::tls_rustls::RustlsConfig;
use rcgen::{generate_simple_self_signed, CertifiedKey};
use serde_json::{Value, json};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::ai::functions::{ToolCall, ToolResult, execute_tool, get_available_tools};
use crate::eavto::DbExecutor;

const PORT_HTTPS: u16 = 47177;
const PORT_HTTP: u16 = 47178;
const JSONRPC_METHOD_NOT_FOUND: i32 = -32601;
const JSONRPC_INVALID_PARAMS: i32 = -32602;
const JSONRPC_INTERNAL_ERROR: i32 = -32603;

#[derive(Clone)]
struct McpState {
    app: Arc<AppHandle>,
}

fn generate_or_load_cert(config_dir: &std::path::Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cert_path = config_dir.join("mcp-cert.pem");
    let key_path = config_dir.join("mcp-key.pem");

    if cert_path.exists() && key_path.exists() {
        let cert = std::fs::read(&cert_path).map_err(|e| e.to_string())?;
        let key = std::fs::read(&key_path).map_err(|e| e.to_string())?;
        return Ok((cert, key));
    }

    let CertifiedKey { cert, key_pair } =
        generate_simple_self_signed(vec!["localhost".to_string(), "127.0.0.1".to_string()])
            .map_err(|e| e.to_string())?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    std::fs::write(&cert_path, &cert_pem).map_err(|e| e.to_string())?;
    std::fs::write(&key_path, &key_pem).map_err(|e| e.to_string())?;

    Ok((cert_pem.into_bytes(), key_pem.into_bytes()))
}

pub async fn serve(app: AppHandle) {
    let config_dir = match app.path().app_config_dir() {
        Ok(dir) => dir,
        Err(e) => {
            crate::commands::log_backend("error", &format!("MCP: cannot get config dir: {e}"));
            return;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        crate::commands::log_backend("error", &format!("MCP: cannot create config dir: {e}"));
        return;
    }

    let (cert_pem, key_pem) = match generate_or_load_cert(&config_dir) {
        Ok(v) => v,
        Err(e) => {
            crate::commands::log_backend("error", &format!("MCP: TLS cert error: {e}"));
            return;
        }
    };

    let tls_config = match RustlsConfig::from_pem(cert_pem, key_pem).await {
        Ok(c) => c,
        Err(e) => {
            crate::commands::log_backend("error", &format!("MCP: TLS config error: {e}"));
            return;
        }
    };

    let state = McpState { app: Arc::new(app) };
    let router = Router::new()
        .route("/mcp", post(handle_mcp))
        .with_state(state);

    let https_addr: std::net::SocketAddr = format!("127.0.0.1:{PORT_HTTPS}").parse().unwrap();
    let http_addr: std::net::SocketAddr = format!("127.0.0.1:{PORT_HTTP}").parse().unwrap();

    crate::commands::log_backend("info", &format!("MCP server listening on https://127.0.0.1:{PORT_HTTPS}/mcp and http://127.0.0.1:{PORT_HTTP}/mcp"));

    let http_router = router.clone();
    tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(http_addr).await {
            Ok(l) => l,
            Err(e) => {
                crate::commands::log_backend("error", &format!("MCP HTTP bind error: {e}"));
                return;
            }
        };
        let _ = axum::serve(listener, http_router).await;
    });

    let _ = axum_server::bind_rustls(https_addr, tls_config)
        .serve(router.into_make_service())
        .await;
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
            let tools: Vec<Value> = get_available_tools()
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
                None => return Json(
                    error_response(id, JSONRPC_INVALID_PARAMS, "Missing tool name"),
                ).into_response(),
            };

            let executor = match state.app.try_state::<DbExecutor>() {
                Some(e) => e.inner().clone(),
                None => return Json(error_response(
                    id,
                    JSONRPC_INTERNAL_ERROR,
                    "Foundation not initialized yet — please wait for the app to finish loading",
                )).into_response(),
            };

            let app = (*state.app).clone();
            let call = ToolCall { name, arguments: req["params"]["arguments"].clone() };

            let result_json = match executor.write(move |conn| -> Result<String, String> {
                let result = execute_tool(conn, &call, Some(&app), None);
                serde_json::to_string(&result).map_err(|e| e.to_string())
            }).await {
                Ok(json) => json,
                Err(e) => return Json(
                    error_response(id, JSONRPC_INTERNAL_ERROR, &e),
                ).into_response(),
            };

            let func_result: ToolResult = match serde_json::from_str(&result_json) {
                Ok(r) => r,
                Err(e) => return Json(
                    error_response(id, JSONRPC_INTERNAL_ERROR, &e.to_string()),
                ).into_response(),
            };

            let is_error = !func_result.success;
            let text = serde_json::to_string(&func_result).unwrap_or_default();
            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "isError": is_error,
                    "content": [{
                        "type": "text",
                        "text": text
                    }]
                }
            })).into_response()
        }

        _ => Json(error_response(id, JSONRPC_METHOD_NOT_FOUND, "Method not found")).into_response(),
    }
}

fn to_mcp_tool(f: crate::ai::functions::ToolTemplate) -> Value {
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

    let item_schema = json!({
        "type": "object",
        "properties": properties,
        "required": required,
    });

    let input_schema = if f.array_mode {
        json!({
            "type": "object",
            "properties": {
                "operations": {
                    "type": "array",
                    "items": item_schema,
                    "minItems": 1,
                }
            },
            "required": ["operations"],
        })
    } else {
        item_schema
    };

    json!({
        "name": f.name,
        "description": f.description,
        "inputSchema": input_schema
    })
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
