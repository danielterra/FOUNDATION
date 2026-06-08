use crate::owl::DbExecutor;
use crate::commands::chat_storage::{
    ContentBlock, AIConversationMessage, create_assistant_message, load_conversation_history, log_api_call,
};
use crate::commands::chat::message_utils::{
    message_to_api_format, inject_datetime_context, inject_attachments_for_current_turn,
    inject_file_binaries_into_placeholders, inject_subconscious_context, sanitize_tool_pairs,
    response_content_to_blocks, extract_and_save_file_summaries,
};
use crate::commands::chat::loop_tools::{
    build_conversation_tools, build_system_prompt,
};
use crate::commands::chat::settings::AgentConfig;
use crate::commands::chat::tool_execution::execute_tools_from_message;
use crate::commands::chat::cancellation::AiCancellationState;
use super::trace;
use crate::ai::{AiProvider};
use crate::ai::local::LocalProvider;
use crate::ai::providers::{ClaudeProvider, OpenRouterProvider, MessageContent, ContentBlock as ApiContentBlock};
use super::super::log_backend;
use tauri::Emitter;

const MAX_TOOL_LOOPS: usize = 20;

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

    let provider = if agent_config.is_local {
        let model_path = crate::commands::local_model::resolve_model_path(app)
            .to_string_lossy()
            .into_owned();
        AiProvider::Local(LocalProvider::new(model_path))
    } else if agent_config.base_url.contains("openrouter.ai") {
        AiProvider::OpenRouter(OpenRouterProvider::with_model(
            agent_config.api_key.clone(),
            agent_config.base_url.clone(),
            agent_config.model_identifier.clone(),
            agent_config.fallback_models.clone(),
            agent_config.timeout_secs,
        ))
    } else {
        AiProvider::Claude(ClaudeProvider::with_model(
            agent_config.api_key.clone(),
            agent_config.model_identifier.clone(),
            agent_config.timeout_secs,
        ))
    };

    let mut loop_count = 0;
    let mut strip_thinking = false;
    let mut is_first_iteration = true;
    let mut none_stop_retries = 0usize;
    let mut empty_end_nudged = false;
    let mut tool_fingerprints: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    let loop_start = std::time::Instant::now();
    let mut trace_steps: Vec<trace::TraceStep> = Vec::new();
    let mut total_input_tokens = 0usize;
    let mut total_output_tokens = 0usize;
    let mut termination_reason = "end_turn";
    let mut loop_error: Option<String> = None;

    // Foundation architecture and widget type context are static for the entire lifetime
    // of a conversation — loading them once here avoids one executor.read() per iteration.
    let (foundation_context, widget_context) = executor.read(|conn| {
        Ok((
            super::foundation_context::foundation_architecture_context(conn),
            crate::commands::widget::widget_system_context(conn),
        ))
    }).await.unwrap_or_default();

    // Load history once — cached in memory for all iterations.
    // Each iteration appends the new assistant + tool_result messages to the cache
    // instead of reloading the full history from DB every turn.
    if !silent {
        app.emit("ai-status", serde_json::json!({
            "status": "Loading history...", "phase": "prep", "conversationId": conversation_id
        })).ok();
    }
    let history_start = std::time::Instant::now();
    let (mut cached_history, mut cached_tokens) =
        load_conversation_history(executor, conversation_id, agent_config.max_tokens).await?;
    log_backend("debug", &format!(
        "[ENGINE] Initial history loaded in {}ms (msgs={}, tokens={})",
        history_start.elapsed().as_millis(), cached_history.len(), cached_tokens
    ));

    // Compaction: if over threshold, run it and reload so the model sees the summary.
    let compaction_threshold = (agent_config.max_tokens as f64 * 0.80) as usize;
    if cached_tokens >= compaction_threshold {
        super::compaction::run_compaction_if_needed(
            executor.clone(),
            app.clone(),
            conversation_id.to_string(),
            super::compaction::CompactionConfig::from_agent_config(agent_config),
            cached_tokens,
        ).await;
        let (new_history, new_tokens) =
            load_conversation_history(executor, conversation_id, agent_config.max_tokens).await?;
        cached_history = new_history;
        cached_tokens = new_tokens;
        log_backend("info", &format!("[ENGINE] Post-compaction history: msgs={} tokens={}", cached_history.len(), cached_tokens));
    }

    let mut iter_start = std::time::Instant::now();

    'main: loop {
        loop_count += 1;
        let iter_elapsed_ms = loop_start.elapsed().as_millis();
        log_backend("info", &format!(
            "[ENGINE] Iteration {} started (elapsed={}ms, cached_msgs={}, cached_tokens={})",
            loop_count, iter_elapsed_ms, cached_history.len(), cached_tokens
        ));

        if loop_count > MAX_TOOL_LOOPS {
            let msg = "Too many tool execution loops — stopping to prevent infinite loop".to_string();
            termination_reason = "max_loops";
            loop_error = Some(msg);
            break 'main;
        }

        if cancellation.is_cancelled(conversation_id) {
            termination_reason = "cancelled";
            break 'main;
        }

        // Use the in-memory cache — no DB read needed.
        let history = &cached_history;

        {
            let mut pending_q: Option<(String, usize)> = None; // (tool_use_id, assistant_index)
            for (i, msg) in history.iter().enumerate().rev() {
                if msg.role == "assistant" {
                    let q_id = msg.content.iter().find_map(|b| match b {
                        ContentBlock::ToolUse { id, name, .. } if name == "ask_question" => Some(id.clone()),
                        ContentBlock::QuestionOutput { id, .. } => Some(id.clone()),
                        _ => None,
                    });
                    if let Some(id) = q_id {
                        let already_answered = history.iter().skip(i + 1).any(|m| {
                            m.role == "user" && m.content.iter().any(|b| {
                                matches!(b,
                                    ContentBlock::ToolResult { tool_use_id, .. }
                                    if tool_use_id == &id)
                            })
                        });
                        if !already_answered {
                            pending_q = Some((id, i));
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
                    termination_reason = "question";
                    break 'main;
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
            break 'main;
        }

        let mut api_messages: Vec<crate::ai::ChatMessage> = history.iter()
            .map(message_to_api_format)
            .collect();

        // File binaries: re-inject on every iteration. On iterations 2+ the history
        // is reloaded from DB (which only stores FileRef, not the binary), so
        // message_to_api_format produces an [Attached file:] placeholder. The function
        // below finds that placeholder message and replaces it with the actual binary.
        if let Some(ref ctx) = first_turn_ctx {
            inject_file_binaries_into_placeholders(&mut api_messages, &ctx.attachment_binaries);
        }

        // Camera frames and system hints: first iteration only.
        if is_first_iteration {
            if let Some(ref ctx) = first_turn_ctx {
                inject_attachments_for_current_turn(
                    &mut api_messages,
                    ctx.camera_images.as_deref(),
                    &ctx.files_needing_summary,
                );
                if let Some(ref sc) = ctx.subconscious_context {
                    inject_subconscious_context(&mut api_messages, sc);
                }
            }
        }

        sanitize_tool_pairs(&mut api_messages);
        inject_datetime_context(&mut api_messages);

        if empty_end_nudged {
            api_messages.push(crate::ai::ChatMessage {
                role: "user".to_string(),
                content: crate::ai::providers::MessageContent::Text(
                    "[System] You returned an empty response. You MUST reply to the user with text now — \
                     summarize what was done, report the result, or explain what happened.".to_string()
                ),
            });
        }

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
            break 'main;
        }

        if api_messages.is_empty() {
            loop_error = Some("Conversation history is empty — cannot send request to Claude".to_string());
            termination_reason = "error";
            break 'main;
        }

        let camera_count = first_turn_ctx.as_ref()
            .and_then(|ctx| ctx.camera_images.as_ref())
            .filter(|f| !f.is_empty())
            .map(|f| f.len());

        let system_prompt = build_system_prompt(
            agent_config, &foundation_context, &widget_context, camera_count, conversation_id,
        );

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
            tool_choice: None,
        };

        if !silent {
            app.emit("ai-status", serde_json::json!({
                "status": "Waiting for response...",
                "phase": "llm",
                "conversationId": conversation_id,
            })).ok();
        }
        log_backend("info", "[ENGINE] Calling AI provider (streaming)...");
        let llm_start = std::time::Instant::now();

        let api_result = tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                log_backend("info", "[ENGINE] Cancelled mid-request");
                termination_reason = "cancelled";
                break 'main;
            }
            result = provider.generate_stream(request, app, conversation_id) => result,
        };
        let llm_ms = llm_start.elapsed().as_millis();

        let api_response = match api_result {
            Ok(r) => r,
            Err(e) if !strip_thinking && e.contains("thinking") && e.contains("cannot be modified") => {
                log_backend("warn", &format!("[ENGINE] Thinking block error, retrying without thinking: {}", e));
                strip_thinking = true;
                is_first_iteration = false;
                continue;
            }
            Err(e) => {
                loop_error = Some(format!("AI error: {}", e));
                termination_reason = "error";
                break 'main;
            }
        };

        is_first_iteration = false;

        let stop_reason = match api_response.stop_reason.clone() {
            Some(r) => {
                none_stop_retries = 0;
                r
            }
            None => {
                none_stop_retries += 1;
                if none_stop_retries >= 3 {
                    loop_error = Some("API returned a truncated response (stop_reason=None) 3 consecutive times".to_string());
                    termination_reason = "error";
                    break 'main;
                }
                log_backend("warn", &format!(
                    "[ENGINE] stop_reason=None — truncated response, retry {}/3",
                    none_stop_retries
                ));
                continue;
            }
        };
        let usage = api_response.usage.as_ref();
        log_backend("info", &format!(
            "[ENGINE] LLM responded in {}ms (stop_reason={}, in={}, out={}, iter_since_last={}ms)",
            llm_ms,
            stop_reason,
            usage.map(|u| u.input_tokens).unwrap_or(0),
            usage.map(|u| u.output_tokens).unwrap_or(0),
            iter_start.elapsed().as_millis(),
        ));
        iter_start = std::time::Instant::now();
        if let Some(u) = usage {
            total_input_tokens += u.input_tokens as usize;
            total_output_tokens += u.output_tokens as usize;
        }

        let content_blocks = response_content_to_blocks(
            &api_response.content,
            &api_response.tool_calls,
            &api_response.thinking_blocks,
        )?;

        let content_blocks = extract_and_save_file_summaries(content_blocks, executor).await;

        if content_blocks.is_empty() && stop_reason != "tool_use" && stop_reason != "tool_calls" {
            if !empty_end_nudged {
                log_backend("warn", "[ENGINE] Empty end turn — nudging model to respond");
                empty_end_nudged = true;
                continue;
            }
            log_backend("info", "[ENGINE] Empty end turn after nudge — conversation complete, not saving");
            if !silent {
                app.emit("ai-status", serde_json::json!({
                    "status": null, "conversationId": conversation_id
                })).ok();
            }
            break 'main;
        }

        // Loop detection must run before saving the assistant message to avoid orphaned ToolUseBlocks.
        for tc in &api_response.tool_calls {
            if tc.name == "ask_question" { continue; }
            let fingerprint = format!("{}:{}", tc.name, tc.input);
            let count = tool_fingerprints.entry(fingerprint).or_insert(0);
            *count += 1;
            if *count >= 3 {
                log_backend("warn", &format!(
                    "[ENGINE] Loop detected — '{}' called {} times with the same arguments",
                    tc.name, count
                ));
                loop_error = Some(format!(
                    "Loop detected: the tool '{}' was called {} consecutive times with the same arguments",
                    tc.name, count
                ));
                termination_reason = "loop_detected";
                break 'main;
            }
        }

        let model = &agent_config.model_identifier;

        // Save assistant message (includes ask_question ToolUse if present — treated as a regular tool)
        let content_json = serde_json::to_string(&content_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        let assistant_msg_iri = create_assistant_message(
            executor, conversation_id, &content_json, model, &stop_reason,
            usage.map(|u| u.input_tokens as usize).unwrap_or(0),
            usage.map(|u| u.output_tokens as usize).unwrap_or(0),
            usage.map(|u| u.cache_creation_input_tokens as usize).unwrap_or(0),
            usage.map(|u| u.cache_read_input_tokens as usize).unwrap_or(0),
        ).await?;

        // Fire-and-forget: log_api_call writes ~8 DB triples sequentially,
        // each emitting entity-updated → chat-message-added. Running it on the
        // critical path delays the final message save and causes the streaming
        // bubble to stay visible because the frontend sees intermediate loads
        // with the same IRI as the current last message.
        if let Some(u) = usage {
            let (executor_c, app_c) = (executor.clone(), app.clone());
            let (model_c, conv_c, msg_c) = (model.to_string(), conversation_id.to_string(), assistant_msg_iri.clone());
            let (inp, out, cc, cr) = (u.input_tokens, u.output_tokens, u.cache_creation_input_tokens, u.cache_read_input_tokens);
            tokio::spawn(async move {
                log_api_call(&executor_c, &app_c, &model_c, inp, out, cc, cr, Some(&conv_c), Some(&msg_c))
                    .await
                    .unwrap_or_else(|e| log_backend("warn", &format!("[ENGINE] Failed to log API call: {}", e)));
            });
        }

        // Build the assistant message in-memory and append to cache — no DB read needed.
        let assistant_msg = AIConversationMessage {
            iri: assistant_msg_iri.clone(),
            role: "assistant".to_string(),
            content: content_blocks.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            token_count: usage.map(|u| u.output_tokens as usize),
            model: Some(model.to_string()),
            stop_reason: Some(stop_reason.clone()),
            input_tokens: usage.map(|u| u.input_tokens as usize),
            output_tokens: usage.map(|u| u.output_tokens as usize),
        };
        cached_tokens += assistant_msg.token_count.unwrap_or(0);
        cached_history.push(assistant_msg.clone());

        log_backend("info", &format!("[ENGINE] Created assistant message: {}", assistant_msg_iri));
        app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();

        // ask_question pauses the loop — user must answer via the UI before we continue
        if api_response.tool_calls.iter().any(|tc| tc.name == "ask_question") {
            log_backend("info", "[ENGINE] ask_question — conversation paused awaiting user answer");
            termination_reason = "question";
            break 'main;
        }

        let has_tool_use = !api_response.tool_calls.is_empty();

        if stop_reason == "tool_use" || stop_reason == "tool_calls" || (stop_reason == "max_tokens" && has_tool_use) {
            log_backend("info", "[ENGINE] Executing tools...");
            let (tool_result_msg_iri, tool_result_msg, mut new_steps) =
                execute_tools_from_message(executor, app, conversation_id, &assistant_msg).await?;

            // Append tool result to cache so the next iteration sees it immediately.
            cached_tokens += tool_result_msg.token_count.unwrap_or(0);
            cached_history.push(tool_result_msg);

            for step in &mut new_steps {
                step.iteration = loop_count;
            }
            trace_steps.extend(new_steps);

            log_backend("info", &format!("[ENGINE] Tool results saved: {}", tool_result_msg_iri));

            if cancellation.is_cancelled(conversation_id) {
                termination_reason = "cancelled";
                break 'main;
            }

            continue;
        }

        break 'main;
    }

    // Save trace (best-effort — never fail the conversation over a trace write error)
    let duration_ms = loop_start.elapsed().as_millis() as u64;
    if total_input_tokens > 0 || !trace_steps.is_empty() {
        trace::save_trace(
            executor,
            conversation_id,
            &trace_steps,
            termination_reason,
            loop_count,
            total_input_tokens,
            total_output_tokens,
            duration_ms,
        ).await.unwrap_or_else(|e| log_backend("warn", &format!("[ENGINE] Trace save failed: {}", e)));
    }

    match loop_error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}
