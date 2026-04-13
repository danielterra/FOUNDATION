use crate::owl::DbExecutor;
use crate::commands::chat_storage::{
    ContentBlock, create_assistant_message, load_conversation_history, load_message, log_api_call,
};
use crate::commands::chat::message_utils::{
    message_to_api_format, inject_datetime_context, inject_attachments_for_current_turn,
    inject_subconscious_context, sanitize_tool_pairs, response_content_to_blocks,
    extract_and_save_file_summaries,
};
use crate::commands::chat::loop_tools::{
    build_conversation_tools, build_system_prompt, handle_speak, handle_ask_question, SpeakOutcome,
};
use crate::commands::chat::settings::AgentConfig;
use crate::commands::chat::tool_execution::execute_tools_from_message;
use crate::commands::chat::cancellation::AiCancellationState;
use crate::ai::providers::{ClaudeProvider, MessageContent, ContentBlock as ApiContentBlock};
use super::super::log_backend;
use tauri::Emitter;

const MAX_TOOL_LOOPS: usize = 50;

/// Context injected only on the first API call of a new user turn.
/// During recovery loops this is None — history already contains everything.
pub struct FirstTurnContext {
    pub camera_images: Option<Vec<String>>,
    pub attachment_binaries: Vec<(String, String)>,
    pub files_needing_summary: Vec<(String, String)>,
    pub subconscious_context: Option<String>,
    pub blackboard_context: Option<String>,
}

/// The single shared conversation loop used by both chat__send_and_reply and recovery.
///
/// Callers must call `cancellation.begin(conversation_id)` before calling this function
/// so that the cancel_rx is registered. This function retrieves it internally.
pub async fn run_conversation_loop(
    app: &tauri::AppHandle,
    executor: &DbExecutor,
    conversation_id: &str,
    agent_config: &AgentConfig,
    first_turn_ctx: Option<FirstTurnContext>,
    silent: bool,
    thinking_enabled: bool,
    cancellation: &AiCancellationState,
) -> Result<(), String> {
    let mut cancel_rx = cancellation.begin(conversation_id);

    let provider = ClaudeProvider::with_model(
        agent_config.api_key.clone(),
        agent_config.model_identifier.clone(),
        agent_config.timeout_secs,
    );

    let mut loop_count = 0;
    let mut strip_thinking = false;
    let mut is_first_iteration = true;

    loop {
        loop_count += 1;
        if loop_count > MAX_TOOL_LOOPS {
            return Err("Too many tool execution loops — stopping to prevent infinite loop".to_string());
        }

        if cancellation.is_cancelled(conversation_id) {
            break;
        }

        // Pending question guard: if the last assistant message has a QuestionOutput
        // with no matching ToolResult, check whether a newer user message exists.
        // If yes, auto-dismiss the question so the new message is processed normally.
        // If no newer user message exists, pause and wait for the user to answer.
        let history = load_conversation_history(executor, conversation_id, agent_config.max_tokens).await?;

        {
            let mut pending_q: Option<(String, usize)> = None; // (tool_use_id, assistant_index)
            for (i, msg) in history.iter().enumerate().rev() {
                if msg.role == "assistant" {
                    if let Some(ContentBlock::QuestionOutput { id, .. }) =
                        msg.content.iter().find(|b| matches!(b, ContentBlock::QuestionOutput { .. }))
                    {
                        let already_answered = history.iter().skip(i + 1).any(|m| {
                            m.role == "user" && m.content.iter().any(|b| {
                                matches!(b,
                                    ContentBlock::ToolResult { tool_use_id, .. }
                                    if tool_use_id == id)
                            })
                        });
                        if !already_answered {
                            pending_q = Some((id.clone(), i));
                        }
                    }
                    break;
                }
            }

            if let Some((_q_id, q_idx)) = pending_q {
                let user_message_after = history.iter().skip(q_idx + 1).any(|m| {
                    m.role == "user"
                        && m.content.iter().any(|b| !matches!(b, ContentBlock::ToolResult { .. }))
                });

                if user_message_after {
                    log_backend(
                        "info",
                        "[ENGINE] Unanswered ask_question — user continued; dismiss will be injected in-memory",
                    );
                    // Do NOT create a DB record: the dismiss timestamp would fall after the user's
                    // new message, causing the history validator to see the wrong order and strip the
                    // tool_use instead. sanitize_tool_pairs injects the synthetic ToolResult into
                    // api_messages before the API call.
                } else {
                    log_backend(
                        "info",
                        "[ENGINE] Conversation paused — waiting for user answer to ask_question",
                    );
                    break;
                }
            }
        }

        let ended_cleanly = history.last().map_or(false, |m| {
            m.role == "assistant"
                && m.content.is_empty()
                && m.stop_reason.as_deref() == Some("end_turn")
        });
        if ended_cleanly {
            log_backend("info", "[ENGINE] Conversation already ended cleanly — nothing to do");
            break;
        }

        let mut api_messages: Vec<crate::ai::ChatMessage> = history.iter()
            .map(message_to_api_format)
            .collect();

        // First-turn injections (attachments, subconscious, camera)
        if is_first_iteration {
            if let Some(ref ctx) = first_turn_ctx {
                inject_attachments_for_current_turn(
                    &mut api_messages,
                    ctx.camera_images.as_deref(),
                    &ctx.attachment_binaries,
                    &ctx.files_needing_summary,
                );
                if let Some(ref sc) = ctx.subconscious_context {
                    inject_subconscious_context(&mut api_messages, sc);
                }
            }
        }

        // sanitize first: synthetic ToolResults must be prepended before inject_datetime
        // so that inject_datetime skips the tool-result user message and targets the
        // actual user turn instead.
        sanitize_tool_pairs(&mut api_messages);
        inject_datetime_context(&mut api_messages);

        if strip_thinking {
            log_backend("warn", "[ENGINE] Stripping thinking blocks from history (previous 400 thinking-block error)");
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
            log_backend("info", "[ENGINE] Conversation ends with assistant message after sanitization — nothing to do");
            break;
        }

        if api_messages.is_empty() {
            return Err("Conversation history is empty — cannot send request to Claude".to_string());
        }

        let widget_context = executor.read(|conn| {
            Ok(crate::commands::widget::widget_system_context(conn))
        }).await.unwrap_or_default();

        let camera_count = first_turn_ctx.as_ref()
            .and_then(|ctx| ctx.camera_images.as_ref())
            .filter(|f| !f.is_empty())
            .map(|f| f.len());

        let system_prompt = build_system_prompt(agent_config, &widget_context, camera_count);

        let blackboard_context = if is_first_iteration {
            first_turn_ctx.as_ref()
                .and_then(|ctx| ctx.blackboard_context.clone())
        } else {
            super::build_blackboard_context(executor, conversation_id).await
        };

        let tools = build_conversation_tools();

        let request = crate::ai::GenerateRequest {
            messages: api_messages,
            max_tokens: Some(agent_config.max_output_tokens),
            temperature: None,
            system: Some(system_prompt),
            blackboard_context,
            tools: Some(tools),
            supports_web_tools: agent_config.supports_web_tools,
            thinking: if thinking_enabled { Some(crate::ai::ThinkingConfig::Adaptive) } else { None },
            tool_choice: Some(serde_json::json!({ "type": "any" })),
        };

        if !silent {
            app.emit("ai-status", serde_json::json!({
                "status": "Claude is thinking",
                "conversationId": conversation_id
            })).ok();
        }
        log_backend("info", "[ENGINE] Calling Claude API (streaming)...");

        let api_result = tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                log_backend("info", "[ENGINE] Cancelled mid-request");
                break;
            }
            result = provider.generate_stream(request, app, conversation_id) => result,
        };

        let api_response = match api_result {
            Ok(r) => r,
            Err(e) if !strip_thinking && e.contains("thinking") && e.contains("cannot be modified") => {
                log_backend("warn", &format!("[ENGINE] Thinking block error, retrying without thinking: {}", e));
                strip_thinking = true;
                is_first_iteration = false;
                continue;
            }
            Err(e) => return Err(format!("Claude API error: {}", e)),
        };

        is_first_iteration = false;

        let stop_reason = api_response.stop_reason.clone()
            .unwrap_or_else(|| "end_turn".to_string());
        log_backend("info", &format!("[ENGINE] Claude responded (stop_reason: {})", stop_reason));

        let content_blocks = response_content_to_blocks(
            &api_response.content,
            &api_response.tool_calls,
            &api_response.thinking_blocks,
        )?;
        let content_blocks = extract_and_save_file_summaries(content_blocks, executor).await;

        if content_blocks.is_empty() && stop_reason == "end_turn" {
            log_backend("info", "[ENGINE] Empty end_turn — conversation complete, not saving");
            break;
        }

        let model = &agent_config.model_identifier;
        let usage = api_response.usage.as_ref();

        // Intercept speak
        if let Some(speak_tc) = api_response.tool_calls.iter().find(|tc| tc.name == "speak") {
            match handle_speak(
                executor, app, conversation_id,
                speak_tc, &api_response.tool_calls,
                &content_blocks,
                usage, model, &stop_reason,
            ).await? {
                SpeakOutcome::Success => break,
                SpeakOutcome::Failure => {
                    // Error pair saved to history; loop again so Claude can self-correct
                    continue;
                }
            }
        }

        // Intercept ask_question
        if let Some(q_tc) = api_response.tool_calls.iter().find(|tc| tc.name == "ask_question") {
            handle_ask_question(executor, app, conversation_id, q_tc, usage, model).await?;
            break;
        }

        // General tool use: save assistant message, execute tools, loop
        let content_json = serde_json::to_string(&content_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        if let Some(u) = usage {
            log_api_call(executor, model,
                u.input_tokens, u.output_tokens,
                u.cache_creation_input_tokens, u.cache_read_input_tokens,
                Some(conversation_id),
            ).await.unwrap_or_else(|e| log_backend("warn", &format!("[ENGINE] Failed to log API call: {}", e)));
        }

        let assistant_msg_iri = create_assistant_message(
            executor, conversation_id, &content_json, model, &stop_reason,
            usage.map(|u| u.input_tokens as usize).unwrap_or(0),
            usage.map(|u| u.output_tokens as usize).unwrap_or(0),
            usage.map(|u| u.cache_creation_input_tokens as usize).unwrap_or(0),
            usage.map(|u| u.cache_read_input_tokens as usize).unwrap_or(0),
        ).await?;

        log_backend("info", &format!("[ENGINE] Created assistant message: {}", assistant_msg_iri));
        app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();

        let has_tool_use = !api_response.tool_calls.is_empty();
        if stop_reason == "tool_use" || (stop_reason == "max_tokens" && has_tool_use) {
            let iri = assistant_msg_iri.clone();
            let assistant_msg = executor.read(move |conn| {
                load_message(conn, &iri)
            }).await?;

            let tool_count = assistant_msg.content.iter()
                .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
                .count();
            if !silent {
                app.emit("ai-status", serde_json::json!({
                    "status": format!("Executing {} tool{}", tool_count, if tool_count != 1 { "s" } else { "" }),
                    "conversationId": conversation_id
                })).ok();
            }

            log_backend("info", "[ENGINE] Executing tools...");
            let (tool_result_msg_iri, had_successful_speak) = execute_tools_from_message(
                executor, app, conversation_id, &assistant_msg,
            ).await?;

            log_backend("info", &format!("[ENGINE] Tool results saved: {}", tool_result_msg_iri));

            if cancellation.is_cancelled(conversation_id) || had_successful_speak {
                break;
            }

            continue;
        }

        break;
    }

    Ok(())
}
