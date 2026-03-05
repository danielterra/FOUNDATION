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
            let result = match execute_tool(executor, app, name, input).await {
                Ok(content) => ContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content,
                    is_error: Some(false),
                },
                Err(e) => ContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content: e,
                    is_error: Some(true),
                },
            };

            tool_results.push(result);
        }
    }

    let content_json = serde_json::to_string(&tool_results)
        .map_err(|e| format!("Failed to serialize tool results: {}", e))?;

    create_message(executor, conversation_id, "user", &content_json, None, None, None).await
}

async fn execute_tool(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    name: &str,
    input: &serde_json::Value,
) -> Result<String, String> {
    let call = ToolCall {
        name: name.to_string(),
        arguments: input.clone(),
    };

    let app_clone = app.clone();
    let result_json = executor.write(move |conn| {
        let result = crate::ai::functions::execute_tool(conn, &call, Some(&app_clone));
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await.map_err(|e| format!("Failed to execute tool: {}", e))?;

    let tool_result: crate::ai::functions::ToolResult = serde_json::from_str(&result_json)
        .map_err(|e| format!("Failed to parse result: {}", e))?;

    if tool_result.success {
        let content = tool_result.result
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| v.to_string()))
            .unwrap_or_default();
        Ok(content)
    } else {
        Err(tool_result.error.unwrap_or_else(|| "Unknown error".to_string()))
    }
}
