use crate::owl::{Individual, Connection, DbExecutor};
use crate::commands::chat_storage::{create_assistant_message, load_conversation_history, load_message};
use crate::ai::providers::{ContentBlock as ApiContentBlock, MessageContent};
use tauri::Emitter;
use super::tool_execution::execute_tools_from_message;
use super::message_utils::{message_to_api_format, inject_datetime_context, sanitize_tool_pairs, response_content_to_blocks};
use super::settings::load_agent_config;
use super::super::log_backend;
use super::MAX_OUTPUT_TOKENS;

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
            .and_then(|(_, v)| super::parse_timestamp(v))
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

pub async fn continue_conversation_after_recovery(
    app: tauri::AppHandle,
    executor: DbExecutor,
    conversation_id: String,
    cancellation: &super::cancellation::AiCancellationState,
    silent: bool,
) -> Result<(), String> {
    const MAX_TOOL_LOOPS: usize = 50;

    // Register this recovery as the active one for this conversation.
    // If another recovery is already running (e.g. concurrent startup paths),
    // this cancels it so only one loop runs at a time.
    let _cancel_rx = cancellation.begin(&conversation_id);

    let conv_id_for_config = conversation_id.clone();
    let agent_config = executor.read(move |conn| {
        load_agent_config(conn, &conv_id_for_config)
    }).await?;

    let mut loop_count = 0;
    let mut strip_thinking = false;
    loop {
        loop_count += 1;
        if loop_count > MAX_TOOL_LOOPS {
            return Err("Too many tool execution loops during recovery".to_string());
        }

        if cancellation.is_cancelled(&conversation_id) {
            break;
        }

        let history = load_conversation_history(&executor, &conversation_id, agent_config.max_tokens).await?;

        let last_msg = history.last();
        let ended_cleanly = last_msg.map_or(false, |m| {
            m.role == "assistant"
                && m.content.is_empty()
                && m.stop_reason.as_deref() == Some("end_turn")
        });
        if ended_cleanly {
            log_backend("info", "[RECOVERY] Conversation already ended cleanly (empty end_turn) — nothing to do");
            break;
        }

        let mut api_messages: Vec<crate::ai::ChatMessage> = history.iter()
            .map(message_to_api_format)
            .collect();

        inject_datetime_context(&mut api_messages);
        sanitize_tool_pairs(&mut api_messages);

        if strip_thinking {
            log_backend("warn", "[RECOVERY] Stripping thinking blocks from history (previous 400 thinking-block error)");
            for msg in api_messages.iter_mut() {
                if msg.role != "assistant" { continue; }
                if let MessageContent::ContentBlocks(ref mut blocks) = msg.content {
                    blocks.retain(|b| !matches!(
                        b,
                        ApiContentBlock::Thinking { .. } | ApiContentBlock::RedactedThinking { .. }
                    ));
                }
            }
            api_messages.retain(|m| {
                if m.role != "assistant" { return true; }
                match &m.content {
                    MessageContent::ContentBlocks(b) => !b.is_empty(),
                    _ => true,
                }
            });
        }

        let last_role = api_messages.last().map(|m| m.role.as_str());
        if last_role == Some("assistant") {
            log_backend("info", "[RECOVERY] Conversation ends with assistant message after sanitization — nothing to do");
            break;
        }

        let widget_context = executor.read(|conn| {
            Ok(crate::commands::widget::widget_system_context(conn))
        }).await.unwrap_or_default();
        let system_prompt = format!(
            "{}\n\n{}\n\n{}",
            crate::ai::BASE_SYSTEM_PROMPT,
            widget_context,
            agent_config.system_prompt,
        );
        let blackboard_context = super::build_blackboard_context(&executor, &conversation_id).await;
        let tools = crate::ai::functions::get_claude_tools();
        let supports_web_tools = agent_config.supports_web_tools;

        let request = crate::ai::GenerateRequest {
            messages: api_messages,
            max_tokens: Some(MAX_OUTPUT_TOKENS),
            temperature: None,
            system: Some(system_prompt),
            blackboard_context,
            tools: Some(tools),
            supports_web_tools,
            thinking: Some(crate::ai::ThinkingConfig::Adaptive),
        };

        if !silent {
            app.emit(
                "ai-status",
                serde_json::json!({ "status": "Claude is thinking (recovery)", "conversationId": conversation_id }),
            ).ok();
        }
        log_backend("info", "[RECOVERY] Calling Claude API...");
        let provider = crate::ai::providers::ClaudeProvider::with_model(
            agent_config.api_key.clone(),
            agent_config.model_identifier.clone(),
            agent_config.timeout_secs,
        );
        let assistant = crate::ai::AIAssistant::new(Box::new(provider));
        let api_response = match assistant.generate(request).await {
            Ok(r) => r,
            Err(e) if !strip_thinking && e.contains("thinking") && e.contains("cannot be modified") => {
                log_backend("warn", &format!("[RECOVERY] Thinking block validation failed, will retry without thinking blocks: {}", e));
                strip_thinking = true;
                continue;
            }
            Err(e) => return Err(format!("Claude API error during recovery: {}", e)),
        };

        let stop_reason = api_response.stop_reason.clone()
            .unwrap_or_else(|| "end_turn".to_string());
        log_backend(
            "info",
            &format!("[RECOVERY] Claude responded (stop_reason: {})", stop_reason),
        );

        let content_blocks = response_content_to_blocks(
            &api_response.content,
            &api_response.tool_calls,
            &api_response.thinking_blocks,
        )?;

        if content_blocks.is_empty() && stop_reason == "end_turn" {
            log_backend("info", "[RECOVERY] Claude returned empty end_turn — conversation complete, not saving");
            break;
        }

        let content_json = serde_json::to_string(&content_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        let current_model = agent_config.model_identifier.clone();

        if let Some(usage) = &api_response.usage {
            crate::commands::chat_storage::log_api_call(
                &executor,
                &current_model,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
                Some(&conversation_id),
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
            let (tool_result_msg_iri, had_successful_speak) = execute_tools_from_message(
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

            if cancellation.is_cancelled(&conversation_id) || had_successful_speak {
                break;
            }

            continue;
        }

        break;
    }

    if !silent {
        app.emit(
            "ai-status",
            serde_json::json!({ "status": null, "conversationId": conversation_id }),
        ).ok();
    }

    Ok(())
}
