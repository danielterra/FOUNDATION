use crate::owl::{Individual, Object, Connection, DbExecutor};
use crate::commands::chat_storage::{create_assistant_message, load_conversation_history, load_message};
use tauri::Emitter;
use super::tool_execution::execute_tools_from_message;
use super::message_utils::{message_to_api_format, inject_datetime_context, sanitize_tool_pairs, response_content_to_blocks};
use super::settings::{get_system_prompt, get_max_input_tokens};
use super::super::log_backend;
use super::MAX_OUTPUT_TOKENS;

fn parse_timestamp(obj: &Object) -> Option<i64> {
    match obj {
        Object::DateTime(ts) => Some(*ts),
        _ => None,
    }
}

/// Retract all messages in the conversation with sentAt >= from_timestamp (exclusive of the
/// message at exactly from_timestamp when exclude_exact is true).
pub fn delete_messages_from_timestamp(
    conn: &mut Connection,
    conversation_iri: &str,
    from_timestamp: i64,
    exclude_exact: bool,
) -> Result<(), String> {
    let message_iris = Individual::find_by_class_and_properties(
        conn,
        "foundation:AIConversationMessage",
        &[("foundation:partOfConversation", conversation_iri)],
    ).map_err(|e| format!("Failed to query messages: {}", e))?;

    for iri in message_iris {
        let ind = match Individual::get(conn, &iri) {
            Ok(Some(i)) => i,
            _ => continue,
        };

        let ts = match ind.properties.iter()
            .find(|(k, _)| k == "foundation:sentAt")
            .and_then(|(_, v)| parse_timestamp(v))
        {
            Some(t) => t,
            None => continue,
        };

        let should_delete = if exclude_exact {
            ts > from_timestamp
        } else {
            ts >= from_timestamp
        };

        if should_delete {
            Individual::retract(conn, &iri, "chat")
                .map_err(|e| format!("Failed to retract message {}: {}", iri, e))?;
        }
    }

    Ok(())
}

/// Helper function to continue conversation loop after recovery
pub async fn continue_conversation_after_recovery(
    app: tauri::AppHandle,
    executor: DbExecutor,
    conversation_id: String,
) -> Result<(), String> {
    const MAX_TOOL_LOOPS: usize = 50;

    let max_tokens = get_max_input_tokens(&executor).await?;

    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > MAX_TOOL_LOOPS {
            return Err("Too many tool execution loops during recovery".to_string());
        }

        let history = load_conversation_history(&executor, &conversation_id, max_tokens).await?;

        let mut api_messages: Vec<crate::ai::ChatMessage> = history.iter()
            .map(message_to_api_format)
            .collect();

        inject_datetime_context(&mut api_messages);
        sanitize_tool_pairs(&mut api_messages);

        let system_prompt = get_system_prompt(&executor).await?;
        let tools = crate::ai::functions::get_claude_tools();

        let request = crate::ai::GenerateRequest {
            messages: api_messages,
            max_tokens: Some(MAX_OUTPUT_TOKENS),
            temperature: Some(0.3),
            system: Some(system_prompt),
            tools: Some(tools),
        };

        app.emit(
            "ai-status",
            serde_json::json!({ "status": "Claude is thinking (recovery)" }),
        ).ok();
        log_backend("info", "[RECOVERY] Calling Claude API...");
        let api_response = crate::ai::generate_response(request).await
            .map_err(|e| format!("Claude API error during recovery: {}", e))?;

        let stop_reason = api_response.stop_reason.clone()
            .unwrap_or_else(|| "end_turn".to_string());
        log_backend(
            "info",
            &format!("[RECOVERY] Claude responded (stop_reason: {})", stop_reason),
        );

        let content_blocks = response_content_to_blocks(
            &api_response.content,
            &api_response.tool_calls,
        )?;
        let content_json = serde_json::to_string(&content_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        let current_model = crate::ai::get_current_model()?;

        if let Some(usage) = &api_response.usage {
            crate::commands::chat_storage::log_api_call(
                &executor,
                &current_model,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
            ).await
                .unwrap_or_else(|e| log_backend("warn", &format!("[RECOVERY] Failed to log API call: {}", e)));
        }

        let assistant_msg_iri = create_assistant_message(
            &executor,
            &conversation_id,
            &content_json,
            &current_model,
            &stop_reason,
            api_response.usage.as_ref().map(|u| u.input_tokens as usize).unwrap_or(0),
            api_response.usage.as_ref().map(|u| u.output_tokens as usize).unwrap_or(0),
        ).await?;

        log_backend(
            "info",
            &format!("[RECOVERY] Created assistant message: {}", assistant_msg_iri),
        );
        app.emit("chat-message-added", ()).ok();

        let has_tool_use = !api_response.tool_calls.is_empty();
        if stop_reason == "tool_use" || (stop_reason == "max_tokens" && has_tool_use) {
            let assistant_msg = executor.read(move |conn| {
                load_message(conn, &assistant_msg_iri)
            }).await?;

            log_backend("info", "[RECOVERY] Executing tools...");
            let tool_result_msg_iri = execute_tools_from_message(
                &executor,
                &app,
                &conversation_id,
                &assistant_msg,
            ).await?;

            log_backend(
                "info",
                &format!("[RECOVERY] Created tool result message: {}", tool_result_msg_iri),
            );
            app.emit("chat-message-added", ()).ok();

            continue;
        }

        break;
    }

    Ok(())
}
