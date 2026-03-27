use crate::owl::{Individual, DbExecutor};
use crate::ai::functions::ToolCall;
use crate::commands::chat_storage::{AIConversationMessage, ContentBlock, load_message, create_message};
use super::super::log_backend;

pub async fn execute_tools_from_message(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    assistant_message: &AIConversationMessage,
) -> Result<String, String> {
    let tool_use_ids: Vec<String> = assistant_message.content.iter()
        .filter_map(|b| {
            if let ContentBlock::ToolUse { id, .. } = b { Some(id.clone()) } else { None }
        })
        .collect();

    if tool_use_ids.is_empty() {
        return Err("No tool use blocks found in message".to_string());
    }

    // Guard against storing duplicate tool_results for the same tool_use_id.
    // This can happen when the recovery path re-executes tools that already ran.
    let conv_id_check = conversation_id.to_string();
    let ids_to_check = tool_use_ids;
    let existing_iri = executor.read(move |conn| {
        let message_iris = Individual::find_by_class_and_properties(
            conn,
            "foundation:AIConversationMessage",
            &[("foundation:partOfConversation", &conv_id_check)],
        ).map_err(|e| format!("Failed to query messages: {}", e))?;

        for iri in message_iris {
            if let Ok(msg) = load_message(conn, &iri) {
                let is_duplicate = msg.content.iter().any(|b| {
                    if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                        ids_to_check.contains(tool_use_id)
                    } else {
                        false
                    }
                });
                if is_duplicate {
                    return Ok(Some(iri));
                }
            }
        }
        Ok(None)
    }).await?;

    if let Some(iri) = existing_iri {
        log_backend("warn", &format!(
            "[CHAT] Skipping duplicate tool_result — results already stored in: {}", iri
        ));
        return Ok(iri);
    }

    let mut tool_results = Vec::new();

    for block in &assistant_message.content {
        if let ContentBlock::ToolUse { id, name, input } = block {
            let (content, is_error) = execute_tool(executor, app, conversation_id, name, input).await;
            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: id.clone(),
                content,
                is_error: Some(is_error),
            });
        }
    }

    let content_json = serde_json::to_string(&tool_results)
        .map_err(|e| format!("Failed to serialize tool results: {}", e))?;

    create_message(executor, conversation_id, "user", &content_json, None, None, None).await
}

const SPEAK_MAX_CHARS: usize = 144;

async fn execute_tool(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    name: &str,
    input: &serde_json::Value,
) -> (String, bool) {
    if name == "speak" {
        let message = input.get("message").and_then(|v| v.as_str()).unwrap_or("");
        if message.chars().count() > SPEAK_MAX_CHARS {
            return (format!("Message exceeds {} characters ({} chars). Split into shorter calls.", SPEAK_MAX_CHARS, message.chars().count()), true);
        }
        return ("Delivered.".to_string(), false);
    }

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
        let content = tool_result.result
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| v.to_string()))
            .unwrap_or_default();
        (content, false)
    } else {
        (result_json, true)
    }
}
