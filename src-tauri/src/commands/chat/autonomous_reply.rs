use tauri::{Emitter, Manager};

use crate::owl::DbExecutor;

use super::{
    MAX_OUTPUT_TOKENS,
    build_blackboard_context,
    create_assistant_message,
    load_conversation_history,
    load_message,
    execute_tools_from_message,
};
use super::message_utils::{message_to_api_format, inject_datetime_context, sanitize_tool_pairs, response_content_to_blocks};
use super::settings::load_agent_config;

pub async fn run_autonomous_reply(app: &tauri::AppHandle, conversation_id: &str) -> Result<(), String> {
    const MAX_TOOL_LOOPS: usize = 50;
    let executor = app.state::<DbExecutor>();

    let conv_id = conversation_id.to_string();
    let agent_config = executor.read(move |conn| {
        load_agent_config(conn, &conv_id)
    }).await?;

    let conversation_id = conversation_id.to_string();
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > MAX_TOOL_LOOPS {
            return Err("Too many tool execution loops - stopping to prevent infinite loop".to_string());
        }

        let history = load_conversation_history(&executor, &conversation_id, agent_config.max_tokens).await?;

        let mut api_messages: Vec<crate::ai::ChatMessage> = history.iter()
            .map(message_to_api_format)
            .collect();

        inject_datetime_context(&mut api_messages);
        sanitize_tool_pairs(&mut api_messages);

        if api_messages.is_empty() {
            return Err("Conversation history is empty".to_string());
        }

        let blackboard_context = build_blackboard_context(&executor).await;
        let tools = crate::ai::functions::get_claude_tools();

        let request = crate::ai::GenerateRequest {
            messages: api_messages,
            max_tokens: Some(MAX_OUTPUT_TOKENS),
            temperature: Some(0.3),
            system: Some(agent_config.system_prompt.clone()),
            blackboard_context,
            tools: Some(tools),
            supports_web_tools: agent_config.supports_web_tools,
        };

        let provider = crate::ai::providers::ClaudeProvider::with_model(
            agent_config.api_key.clone(),
            agent_config.model_identifier.clone(),
            agent_config.timeout_secs,
        );
        let assistant = crate::ai::AIAssistant::new(Box::new(provider));

        let api_response = assistant.generate(request).await
            .map_err(|e| format!("Claude API error: {}", e))?;

        let stop_reason = api_response.stop_reason.clone()
            .unwrap_or_else(|| "end_turn".to_string());

        let content_blocks = response_content_to_blocks(
            &api_response.content,
            &api_response.tool_calls,
        )?;
        let content_blocks = super::message_utils::extract_and_save_file_summaries(content_blocks, &executor).await;
        let content_json = serde_json::to_string(&content_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        if let Some(usage) = &api_response.usage {
            super::super::chat_storage::log_api_call(
                &executor,
                &agent_config.model_identifier,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
            ).await
                .unwrap_or_else(|e| super::super::log_backend("warn", &format!("[CHAT] Failed to log API call: {}", e)));
        }

        let assistant_msg_iri = create_assistant_message(
            &executor,
            &conversation_id,
            &content_json,
            &agent_config.model_identifier,
            &stop_reason,
            api_response.usage.as_ref().map(|u| u.input_tokens as usize).unwrap_or(0),
            api_response.usage.as_ref().map(|u| u.output_tokens as usize).unwrap_or(0),
        ).await?;

        app.emit("chat-message-added", ()).ok();

        let has_tool_use = !api_response.tool_calls.is_empty();
        if stop_reason == "tool_use" || (stop_reason == "max_tokens" && has_tool_use) {
            let assistant_msg = executor.read(move |conn| {
                load_message(conn, &assistant_msg_iri)
            }).await?;

            execute_tools_from_message(&executor, app, &conversation_id, &assistant_msg).await?;
            app.emit("chat-message-added", ()).ok();
            continue;
        }

        break;
    }

    Ok(())
}
