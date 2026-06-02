use crate::owl::DbExecutor;
use crate::ai::functions::ToolCall;
use crate::commands::chat_storage::{AIConversationMessage, ContentBlock, create_message};
use super::super::log_backend;
use super::trace::{TraceStep, make_step};
use tauri::Emitter;

/// Returns `(tool_result_msg_iri, tool_result_msg, steps)`.
/// `tool_result_msg` is the in-memory message built from tool results — callers can
/// append it to their history cache without an additional DB read.
pub async fn execute_tools_from_message(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    assistant_message: &AIConversationMessage,
) -> Result<(String, AIConversationMessage, Vec<TraceStep>), String> {
    let has_any_tool_use = assistant_message.content.iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));

    if !has_any_tool_use {
        return Err("No tool use blocks found in message".to_string());
    }

    let tool_use_ids: Vec<String> = assistant_message.content.iter()
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();

    // Guard against storing duplicate tool_results for the same tool_use_id.
    // This can happen when the recovery path re-executes tools that already ran.
    // Single JOIN query per tool_use_id avoids loading all conversation messages (N+1).
    let conv_id_check = conversation_id.to_string();
    let ids_to_check = tool_use_ids;
    let existing_iri = executor.read(move |conn| {
        let found = ids_to_check.iter()
            .find_map(|id| crate::core_ontology::chat::find_tool_result_message_iri(conn, id, &conv_id_check));
        Ok(found)
    }).await?;

    if let Some(iri) = existing_iri {
        log_backend("warn", &format!(
            "[CHAT] Skipping duplicate tool_result — results already stored in: {}", iri
        ));
        let empty_msg = AIConversationMessage {
            iri: iri.clone(),
            role: "user".to_string(),
            content: Vec::new(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            token_count: None,
            model: None,
            stop_reason: None,
            input_tokens: None,
            output_tokens: None,
        };
        return Ok((iri, empty_msg, Vec::new()));
    }

    let mut tool_results = Vec::new();
    let mut trace_steps = Vec::new();

    for block in &assistant_message.content {
        if let ContentBlock::ToolUse { id, name, input, .. } = block {
            if name == "ask_question" {
                continue; // answered by user via UI — not executed automatically
            }
            app.emit("ai-status", serde_json::json!({
                "status": name,
                "phase": "tool",
                "conversationId": conversation_id,
            })).ok();
            let start = std::time::Instant::now();
            log_backend("info", &format!("[ENGINE] tool '{}' started (id={})", name, id));
            let (content, is_error) = execute_tool(executor, app, conversation_id, name, input).await;
            let duration_ms = start.elapsed().as_millis() as u64;
            log_backend(
                "info",
                &format!(
                    "[ENGINE] tool '{}' finished in {}ms (error={}, result_len={})",
                    name,
                    duration_ms,
                    is_error,
                    content.len()
                ),
            );

            trace_steps.push(make_step(0, name, input, &content, is_error, duration_ms));

            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: id.clone(),
                content,
                is_error: Some(is_error),
                duration_ms: Some(duration_ms),
            });
        }
    }

    if tool_results.is_empty() {
        let empty_msg = AIConversationMessage {
            iri: assistant_message.iri.clone(),
            role: "user".to_string(),
            content: Vec::new(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            token_count: None,
            model: None,
            stop_reason: None,
            input_tokens: None,
            output_tokens: None,
        };
        return Ok((assistant_message.iri.clone(), empty_msg, trace_steps));
    }

    let iri = create_message(executor, conversation_id, "user", tool_results.clone(), None, None, None).await?;

    let tool_result_msg = AIConversationMessage {
        iri: iri.clone(),
        role: "user".to_string(),
        content: tool_results,
        timestamp: chrono::Utc::now().timestamp_millis(),
        token_count: None,
        model: None,
        stop_reason: None,
        input_tokens: None,
        output_tokens: None,
    };

    app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();
    Ok((iri, tool_result_msg, trace_steps))
}

async fn execute_tool(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    name: &str,
    input: &serde_json::Value,
) -> (String, bool) {
    let call = ToolCall {
        name: name.to_string(),
        arguments: input.clone(),
    };

    let app_clone = app.clone();
    let conv_id = conversation_id.to_string();
    let result_json = match executor.write(move |conn| {
        let result = crate::ai::functions::execute_tool(conn, &call, Some(&app_clone), Some(&conv_id));
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await {
        Ok(json) => json,
        Err(e) => return (format!("{{\"success\":false,\"error\":\"{}\"}}", e), true),
    };

    let tool_result: crate::ai::functions::ToolResult = match serde_json::from_str(&result_json) {
        Ok(r) => r,
        Err(e) => return (format!("{{\"success\":false,\"error\":\"{}\"}}", e), true),
    };

    if tool_result.success {
        // For read_binary_file / read_pdf_page, convert image/PDF results to a content-block
        // array so Claude receives a proper vision/document block instead of raw base64 text.
        // read_binary_file inlines the bytes as `data`; read_pdf_page persists to disk and
        // returns `page_path` — load lazily from disk so the JSON tool result stays small.
        if name == "read_binary_file" || name == "read_pdf_page" {
            if let Some(ref result) = tool_result.result {
                let media_type = result["media_type"].as_str().unwrap_or("");
                let inline = result["data"].as_str().unwrap_or("");
                let from_path = result["page_path"].as_str().unwrap_or("");
                let data: String = if !inline.is_empty() {
                    inline.to_string()
                } else if !from_path.is_empty() {
                    use base64::Engine;
                    match std::fs::read(from_path) {
                        Ok(bytes) => base64::engine::general_purpose::STANDARD.encode(&bytes),
                        Err(e) => {
                            crate::commands::log_backend(
                                "warn",
                                &format!("{name}: page_path '{from_path}' could not be read ({e}); falling back to raw JSON tool result"),
                            );
                            String::new()
                        }
                    }
                } else {
                    String::new()
                };
                if !data.is_empty() {
                    if media_type.starts_with("image/") {
                        let blocks = serde_json::json!([
                            {"type": "image", "source": {"type": "base64", "media_type": media_type, "data": data}}
                        ]);
                        return (serde_json::to_string(&blocks).unwrap_or_default(), false);
                    } else if media_type == "application/pdf" {
                        let blocks = serde_json::json!([
                            {"type": "document", "source": {"type": "base64", "media_type": media_type, "data": data}}
                        ]);
                        return (serde_json::to_string(&blocks).unwrap_or_default(), false);
                    }
                }
            }
        }
        let content = tool_result.result
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| v.to_string()))
            .unwrap_or_default();
        (content, false)
    } else {
        (result_json, true)
    }
}

