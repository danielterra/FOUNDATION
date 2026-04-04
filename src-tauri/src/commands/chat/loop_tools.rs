use crate::ai::providers::{ClaudeTool, UsageInfo};
use crate::owl::DbExecutor;
use crate::commands::chat_storage::{ContentBlock, create_assistant_message, create_user_message_raw, log_api_call};
use super::tool_execution::try_speak;
use super::settings::AgentConfig;
use super::super::log_backend;
use tauri::Emitter;

pub fn build_conversation_tools() -> Vec<ClaudeTool> {
    let mut tools = crate::ai::functions::get_claude_tools();

    tools.push(ClaudeTool {
        name: "speak".to_string(),
        description: format!(
            "Send a message to the user. Your only output channel. Maximum {} characters.",
            super::SPEAK_MAX_CHARS
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": format!("The message to deliver to the user. Maximum {} characters.", super::SPEAK_MAX_CHARS)
                },
                "iris": {
                    "type": "array",
                    "description": "IRIs of entities to display as widgets on the blackboard. The best widget type for each entity will be selected automatically.",
                    "items": { "type": "string" }
                }
            },
            "required": ["message"]
        }),
    });

    tools.push(ClaudeTool {
        name: "ask_question".to_string(),
        description: "Ask the user a structured question and wait for their answer. Use only when a user decision is required to proceed. The conversation pauses until the user responds.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user."
                },
                "type": {
                    "type": "string",
                    "enum": ["single", "multi", "text"],
                    "description": "'single' for one option button, 'multi' for checkboxes, 'text' for a free-text field."
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of options. Required for 'single' and 'multi'. Omit for 'text'."
                }
            },
            "required": ["question", "type"]
        }),
    });

    tools
}

pub fn build_system_prompt(agent_config: &AgentConfig, widget_context: &str, camera_count: Option<usize>) -> String {
    let base = format!(
        "{}\n\n{}\n\n{}",
        crate::ai::BASE_SYSTEM_PROMPT,
        widget_context,
        agent_config.system_prompt,
    );
    match camera_count {
        Some(n) => format!(
            "{}\n\n[Camera Vision] {} webcam snapshot{} of the user were captured during the typing of this message and are included as image blocks at the start of the user's message, in chronological order. Use them to read the evolution of the user's facial expression, posture, and emotional energy over the course of composing the message, and calibrate your tone and depth of response accordingly. Do not mention the camera or the images to the user unless they explicitly ask.",
            base, n, if n == 1 { "" } else { "s" }
        ),
        None => base,
    }
}

pub enum SpeakOutcome {
    /// Speak delivered successfully.
    Success,
    /// Speak failed. Error pair was saved to history so Claude can retry.
    Failure,
}

/// Intercept a speak tool call before any assistant message is saved.
/// On success: saves a SpeakOutput assistant message, logs usage, executes side-effect tools, returns Success.
/// On failure: saves tool_use + error ToolResult to history so Claude can self-correct, returns Failure.
pub async fn handle_speak(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    tc: &crate::ai::ToolCall,
    all_tool_calls: &[crate::ai::ToolCall],
    response_blocks: &[ContentBlock],
    usage: Option<&UsageInfo>,
    model: &str,
    stop_reason: &str,
) -> Result<SpeakOutcome, String> {
    let (speak_result, speak_is_error) = try_speak(executor, app, conversation_id, &tc.input).await;

    if speak_is_error {
        log_backend("warn", &format!("[ENGINE] Speak failed ({}), saving error to history", speak_result));

        let tool_use_blocks = vec![ContentBlock::ToolUse {
            id: tc.id.clone(),
            name: "speak".to_string(),
            input: tc.input.clone(),
        }];
        let tool_use_json = serde_json::to_string(&tool_use_blocks)
            .map_err(|e| format!("Failed to serialize speak tool_use: {}", e))?;
        create_assistant_message(
            executor, conversation_id, &tool_use_json, model, "tool_use",
            usage.map(|u| u.input_tokens as usize).unwrap_or(0),
            usage.map(|u| u.output_tokens as usize).unwrap_or(0),
            usage.map(|u| u.cache_creation_input_tokens as usize).unwrap_or(0),
            usage.map(|u| u.cache_read_input_tokens as usize).unwrap_or(0),
        ).await?;

        let error_blocks = vec![ContentBlock::ToolResult {
            tool_use_id: tc.id.clone(),
            content: speak_result,
            is_error: Some(true),
        }];
        let error_json = serde_json::to_string(&error_blocks)
            .map_err(|e| format!("Failed to serialize speak error: {}", e))?;
        create_user_message_raw(executor, conversation_id, &error_json).await?;

        return Ok(SpeakOutcome::Failure);
    }

    if let Some(u) = usage {
        log_api_call(executor, model, u.input_tokens, u.output_tokens,
            u.cache_creation_input_tokens, u.cache_read_input_tokens,
            Some(conversation_id),
        ).await.unwrap_or_else(|e| log_backend("warn", &format!("[ENGINE] Failed to log API call: {}", e)));
    }

    // Execute non-speak side-effect tools (fire-and-forget)
    for side_tc in all_tool_calls.iter().filter(|t| t.name != "speak") {
        let call = crate::ai::functions::ToolCall {
            name: side_tc.name.clone(),
            arguments: side_tc.input.clone(),
        };
        let app_clone = app.clone();
        let conv_id = conversation_id.to_string();
        executor.write(move |conn| {
            let _ = crate::ai::functions::execute_tool(conn, &call, Some(&app_clone), Some(&conv_id));
            Ok(String::new())
        }).await.ok();
    }

    let mut speak_blocks: Vec<ContentBlock> = response_blocks.iter()
        .filter(|b| matches!(b, ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. }))
        .cloned()
        .collect();
    speak_blocks.push(ContentBlock::SpeakOutput { text: speak_result });
    let speak_json = serde_json::to_string(&speak_blocks)
        .map_err(|e| format!("Failed to serialize speak content: {}", e))?;
    create_assistant_message(
        executor, conversation_id, &speak_json, model, stop_reason,
        usage.map(|u| u.input_tokens as usize).unwrap_or(0),
        usage.map(|u| u.output_tokens as usize).unwrap_or(0),
        usage.map(|u| u.cache_creation_input_tokens as usize).unwrap_or(0),
        usage.map(|u| u.cache_read_input_tokens as usize).unwrap_or(0),
    ).await?;
    app.emit("chat-message-added", ()).ok();

    Ok(SpeakOutcome::Success)
}

/// Intercept an ask_question tool call.
/// Saves a QuestionOutput assistant message and logs usage.
pub async fn handle_ask_question(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    tc: &crate::ai::ToolCall,
    usage: Option<&UsageInfo>,
    model: &str,
) -> Result<(), String> {
    let question = tc.input.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let question_type = tc.input.get("type").and_then(|v| v.as_str()).unwrap_or("text").to_string();
    let options: Vec<String> = tc.input.get("options")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    if let Some(u) = usage {
        log_api_call(executor, model, u.input_tokens, u.output_tokens,
            u.cache_creation_input_tokens, u.cache_read_input_tokens,
            Some(conversation_id),
        ).await.unwrap_or_else(|e| log_backend("warn", &format!("[ENGINE] Failed to log API call: {}", e)));
    }

    let q_blocks = vec![ContentBlock::QuestionOutput {
        id: tc.id.clone(),
        question,
        question_type,
        options,
    }];
    let q_json = serde_json::to_string(&q_blocks)
        .map_err(|e| format!("Failed to serialize question content: {}", e))?;
    create_assistant_message(
        executor, conversation_id, &q_json, model, "tool_use",
        usage.map(|u| u.input_tokens as usize).unwrap_or(0),
        usage.map(|u| u.output_tokens as usize).unwrap_or(0),
        usage.map(|u| u.cache_creation_input_tokens as usize).unwrap_or(0),
        usage.map(|u| u.cache_read_input_tokens as usize).unwrap_or(0),
    ).await?;
    app.emit("chat-message-added", ()).ok();
    log_backend("info", "[ENGINE] ask_question saved — conversation paused awaiting user answer");

    Ok(())
}
