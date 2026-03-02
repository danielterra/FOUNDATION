/// Simplified Chat Implementation
///
/// This is a complete rewrite that eliminates complexity:
/// - Messages store content as JSON (no separate ToolUse/ToolResult entities)
/// - Simple linear conversation flow
/// - Recovery logic for incomplete conversations (tool_use/tool_result pairs)
/// - Direct mapping to LLM APIs (Claude, OpenAI, etc.)

use crate::owl::DbExecutor;
use crate::ai::functions::FunctionCall;
use crate::owl::{Individual, Object, Connection};
use tauri::{Emitter, State};

pub use super::chat_storage::{
    ContentBlock, AIConversationMessage,
    create_user_message, create_assistant_message, load_conversation_history,
};
use super::chat_storage::{create_message, load_message};

/// Execute tools from an assistant message and create user message with results
pub async fn execute_tools_from_message(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    assistant_message: &AIConversationMessage,
) -> Result<String, String> {
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

    if tool_results.is_empty() {
        return Err("No tool use blocks found in message".to_string());
    }

    let content_json = serde_json::to_string(&tool_results)
        .map_err(|e| format!("Failed to serialize tool results: {}", e))?;

    create_message(executor, conversation_id, "user", &content_json, None, None, None).await
}

/// Execute a single tool function
async fn execute_tool(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    name: &str,
    input: &serde_json::Value,
) -> Result<String, String> {
    let call = FunctionCall {
        name: name.to_string(),
        arguments: input.clone(),
    };

    let app_clone = app.clone();
    let result_json = executor.write(move |conn| {
        let result = crate::ai::functions::execute_function(conn, &call, Some(&app_clone));
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await.map_err(|e| format!("Failed to execute function: {}", e))?;

    let func_result: crate::ai::functions::FunctionResult = serde_json::from_str(&result_json)
        .map_err(|e| format!("Failed to parse result: {}", e))?;

    if func_result.success {
        let content = func_result.result
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| v.to_string()))
            .unwrap_or_default();
        Ok(content)
    } else {
        Err(func_result.error.unwrap_or_else(|| "Unknown error".to_string()))
    }
}

/// Convert AIConversationMessage to Claude API format
fn message_to_api_format(msg: &AIConversationMessage) -> crate::ai::ChatMessage {
    use crate::ai::providers::{
        ContentBlock as ApiContentBlock,
        MessageContent,
        ImageSource as ApiImageSource,
        DocumentSource as ApiDocumentSource,
    };

    let api_blocks: Vec<ApiContentBlock> = msg.content.iter().map(|block| {
        match block {
            ContentBlock::Text { text } => {
                ApiContentBlock::Text { text: text.clone() }
            },
            ContentBlock::Image { source } => {
                ApiContentBlock::Image {
                    source: ApiImageSource {
                        source_type: source.source_type.clone(),
                        media_type: source.media_type.clone(),
                        data: source.data.clone(),
                    }
                }
            },
            ContentBlock::Document { source } => {
                ApiContentBlock::Document {
                    source: ApiDocumentSource {
                        source_type: source.source_type.clone(),
                        media_type: source.media_type.clone(),
                        data: source.data.clone(),
                    }
                }
            },
            ContentBlock::ToolUse { id, name, input } => {
                ApiContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                }
            },
            ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                ApiContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: content.clone(),
                    is_error: *is_error,
                }
            },
        }
    }).collect();

    crate::ai::ChatMessage {
        role: msg.role.clone(),
        content: MessageContent::ContentBlocks(api_blocks),
    }
}

/// Send a message and get AI response (with automatic tool execution loop)
#[tauri::command]
pub async fn chat__send_and_reply(
    content: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    attachment_iris: Option<Vec<String>>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    const CONVERSATION_ID: &str = "foundation:MainChatConversation";
    const MAX_TOOL_LOOPS: usize = 10; // Prevent infinite loops

    let max_tokens = get_max_input_tokens(&executor).await?;

    // Location and attachments are received but not yet forwarded to the AI context.
    // Deferred: pass location as context in the system prompt, and process attachment_iris
    // into file content blocks in the message payload.
    if latitude.is_some() || longitude.is_some() {
        super::log_backend(
            "info",
            &format!("[CHAT] Location: lat={:?}, lon={:?}", latitude, longitude),
        );
    }
    if let Some(ref attachments) = attachment_iris {
        super::log_backend("info", &format!("[CHAT] Attachments: {:?}", attachments));
    }

    let mut response_messages = Vec::new();

    let user_msg_iri = create_user_message(&executor, CONVERSATION_ID, &content).await?;
    super::log_backend("info", &format!("[CHAT] Created user message: {}", user_msg_iri));

    app.emit("chat-message-added", ()).ok();

    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > MAX_TOOL_LOOPS {
            return Err(
                "Too many tool execution loops - stopping to prevent infinite loop".to_string(),
            );
        }

        app.emit("ai-status", serde_json::json!({ "status": "Loading conversation history" })).ok();

        let history = load_conversation_history(&executor, CONVERSATION_ID, max_tokens).await?;
        super::log_backend(
            "info",
            &format!("[CHAT] Loaded {} messages from history", history.len()),
        );

        let api_messages: Vec<crate::ai::ChatMessage> = history.iter()
            .map(message_to_api_format)
            .collect();

        let system_prompt = get_system_prompt(&executor).await?;
        let tools = crate::ai::functions::get_claude_tools();

        let request = crate::ai::GenerateRequest {
            messages: api_messages,
            max_tokens: Some(4096),
            temperature: Some(0.3),
            system: Some(system_prompt),
            tools: Some(tools),
        };

        app.emit("ai-status", serde_json::json!({ "status": "Claude is thinking" })).ok();
        super::log_backend("info", "[CHAT] Calling Claude API...");
        let api_response = crate::ai::generate_response(request).await
            .map_err(|e| format!("Claude API error: {}", e))?;

        let stop_reason = api_response.stop_reason.clone()
            .unwrap_or_else(|| "end_turn".to_string());
        super::log_backend(
            "info",
            &format!("[CHAT] Claude responded (stop_reason: {})", stop_reason),
        );

        let content_blocks = response_content_to_blocks(
            &api_response.content,
            &api_response.tool_calls,
        )?;
        let content_json = serde_json::to_string(&content_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        let assistant_msg_iri = create_assistant_message(
            &executor,
            CONVERSATION_ID,
            &content_json,
            "claude-sonnet-4-6", // Model is hardcoded for now
            &stop_reason,
            api_response.usage.as_ref().map(|u| u.input_tokens as usize).unwrap_or(0),
            api_response.usage.as_ref().map(|u| u.output_tokens as usize).unwrap_or(0),
        ).await?;

        super::log_backend(
            "info",
            &format!("[CHAT] Created assistant message: {}", assistant_msg_iri),
        );

        app.emit("chat-message-added", ()).ok();

        response_messages.push(serde_json::json!({
            "iri": assistant_msg_iri,
            "role": "assistant",
            "content": api_response.content,
            "stop_reason": stop_reason,
        }));

        if stop_reason == "tool_use" {
            let assistant_msg = executor.read(move |conn| {
                load_message(conn, &assistant_msg_iri)
            }).await?;

            let tool_count = assistant_msg.content.iter()
                .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
                .count();
            app.emit("ai-status", serde_json::json!({
                "status": format!(
                    "Executing {} tool{}",
                    tool_count,
                    if tool_count != 1 { "s" } else { "" }
                )
            })).ok();

            super::log_backend("info", "[CHAT] Executing tools...");
            let tool_result_msg_iri = execute_tools_from_message(
                &executor,
                &app,
                CONVERSATION_ID,
                &assistant_msg,
            ).await?;

            super::log_backend(
                "info",
                &format!("[CHAT] Created tool result message: {}", tool_result_msg_iri),
            );

            app.emit("chat-message-added", ()).ok();

            continue;
        }

        break;
    }

    Ok(response_messages)
}

/// Convert API response content and tool calls to our content block format
fn response_content_to_blocks(
    content: &str,
    tool_calls: &[crate::ai::ToolCall],
) -> Result<Vec<ContentBlock>, String> {
    let mut blocks = Vec::new();

    if !content.is_empty() {
        blocks.push(ContentBlock::Text { text: content.to_string() });
    }

    for tool_call in tool_calls {
        blocks.push(ContentBlock::ToolUse {
            id: tool_call.id.clone(),
            name: tool_call.name.clone(),
            input: tool_call.input.clone(),
        });
    }

    Ok(blocks)
}

/// Get system prompt from database settings with template variables replaced
async fn get_system_prompt(executor: &DbExecutor) -> Result<String, String> {
    executor.read(|conn| {
        let template = if let Ok(setting) = Individual::get(
            conn,
            "foundation:DefaultSystemPromptSetting",
        ) {
            if let Some(Object::Literal { value, .. }) = setting.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .map(|(_, v)| v) {
                value.clone()
            } else {
                return Err("DefaultSystemPromptSetting has no settingValue".to_string());
            }
        } else {
            return Err(
                "Failed to get system prompt: DefaultSystemPromptSetting not found".to_string(),
            );
        };

        let user = Individual::get(conn, "foundation:ThisUser").ok();
        let ai = Individual::get(conn, "foundation:LocalAIAssistant").ok();

        let user_name = user.as_ref()
            .and_then(|u| u.properties.iter()
                .find(|(k, _)| k == "rdfs:label")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "User".to_string());

        let ai_name = ai.as_ref()
            .and_then(|a| a.properties.iter()
                .find(|(k, _)| k == "rdfs:label")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "NOVA".to_string());

        let now = chrono::Local::now();
        let date_time = now.format("%Y-%m-%d %H:%M:%S %Z").to_string();

        let language = Individual::get(conn, "foundation:DefaultLanguageSetting")
            .ok()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "English".to_string());

        let locale = Individual::get(conn, "foundation:DefaultLocaleSetting")
            .ok()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "en_US".to_string());

        let country = Individual::get(conn, "foundation:DefaultCountrySetting")
            .ok()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "United States".to_string());

        let location_info = Individual::get(conn, "foundation:DefaultLocationInfoSetting")
            .ok()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "".to_string());

        let prompt = template
            .replace("{user_name}", &user_name)
            .replace("{ai_name}", &ai_name)
            .replace("{date_time}", &date_time)
            .replace("{language}", &language)
            .replace("{locale}", &locale)
            .replace("{country}", &country)
            .replace("{location_info}", &location_info);

        Ok(prompt)
    }).await
}

#[tauri::command]
pub async fn chat__attach_file(
    _file_path: String,
    _executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    Err("Not implemented in refactored version yet".to_string())
}

#[tauri::command]
pub async fn chat__send_message(
    _message: String,
    _executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    Err("Use chat__send_and_reply instead".to_string())
}

#[tauri::command]
pub async fn chat__get_recent_messages(
    limit: usize,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    const CONVERSATION_ID: &str = "foundation:MainChatConversation";

    let messages = executor.read(move |conn| {
        let message_iris = Individual::find_by_class_and_properties(
            conn,
            "foundation:AIConversationMessage",
            &[("foundation:partOfConversation", CONVERSATION_ID)],
        ).map_err(|e| format!("Failed to query messages: {}", e))?;

        let mut messages_with_ts: Vec<(i64, serde_json::Value)> = Vec::new();

        for iri in message_iris {
            let msg = Individual::get(conn, &iri)
                .map_err(|e| format!("Failed to get message {}: {}", iri, e))?;

            let role = msg.properties.iter()
                .find(|(k, _)| k == "foundation:role")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_default();

            let content_json = msg.properties.iter()
                .find(|(k, _)| k == "foundation:content")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_default();

            let timestamp = msg.properties.iter()
                .find(|(k, _)| k == "foundation:sentAt")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => value.parse::<i64>().ok(),
                    _ => None,
                })
                .unwrap_or(0);

            let content_blocks: Vec<ContentBlock> = serde_json::from_str(&content_json)
                .unwrap_or_else(|_| vec![ContentBlock::Text { text: content_json.clone() }]);

            let msg_json = serde_json::json!({
                "iri": iri,
                "role": role,
                "content": content_blocks,
                "timestamp": timestamp,
            });

            messages_with_ts.push((timestamp, msg_json));
        }

        // Sort by timestamp descending (most recent first)
        messages_with_ts.sort_by(|a, b| b.0.cmp(&a.0));

        // Take only the requested limit
        let messages: Vec<serde_json::Value> = messages_with_ts
            .into_iter()
            .take(limit)
            .map(|(_, msg)| msg)
            .collect();

        // Reverse to get chronological order (oldest first)
        let mut messages = messages;
        messages.reverse();

        Ok(messages)
    }).await?;

    Ok(messages)
}

#[tauri::command]
pub async fn chat__get_conversation_info(
    _executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    Err("Not implemented in refactored version yet".to_string())
}

/// Get max input tokens with fallback logic:
/// 1. Check DefaultMaxInputTokensSetting (user updates this setting, not creates new one)
/// 2. Fall back to AIModel's maxInputTokens
async fn get_max_input_tokens(executor: &DbExecutor) -> Result<usize, String> {
    executor.read(|conn| {
        // Try setting from ontology (user updates this, not creates new one)
        if let Ok(setting) = Individual::get(conn, "foundation:DefaultMaxInputTokensSetting") {
            if let Some(Object::Literal { value, .. }) = setting.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .map(|(_, v)| v) {
                if let Ok(tokens) = value.parse::<usize>() {
                    return Ok(tokens);
                }
            }
        }

        // Fall back to model's maxInputTokens
        let model_iri = get_ai_model_iri(conn)?;

        if let Some(iri) = model_iri {
            let model = Individual::get(conn, &iri)
                .map_err(|e| format!("Failed to get AI model: {}", e))?;

            if let Some(Object::Integer(max_tokens)) = model.properties.iter()
                .find(|(k, _)| k == "foundation:maxInputTokens")
                .map(|(_, v)| v) {
                return Ok(*max_tokens as usize);
            }
        }

        Err(concat!(
            "Failed to get max input tokens: DefaultMaxInputTokensSetting not found",
            " and no AI model configured",
        ).to_string())
    }).await
}

/// Get AI model IRI with fallback logic:
/// Check DefaultAIModelSetting (user updates this setting, not creates new one)
fn get_ai_model_iri(conn: &Connection) -> Result<Option<String>, String> {
    // Get setting from ontology (user updates this, not creates new one)
    if let Ok(setting) = Individual::get(conn, "foundation:DefaultAIModelSetting") {
        if let Some(Object::Literal { value, .. }) = setting.properties.iter()
            .find(|(k, _)| k == "foundation:settingValue")
            .map(|(_, v)| v) {
            return Ok(Some(value.clone()));
        }
    }

    Ok(None)
}

/// Helper function to continue conversation loop after recovery
async fn continue_conversation_after_recovery(
    app: tauri::AppHandle,
    executor: DbExecutor,
) -> Result<(), String> {
    const CONVERSATION_ID: &str = "foundation:MainChatConversation";
    const MAX_TOOL_LOOPS: usize = 10;

    let max_tokens = get_max_input_tokens(&executor).await?;

    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > MAX_TOOL_LOOPS {
            return Err("Too many tool execution loops during recovery".to_string());
        }

        let history = load_conversation_history(&executor, CONVERSATION_ID, max_tokens).await?;

        let api_messages: Vec<crate::ai::ChatMessage> = history.iter()
            .map(message_to_api_format)
            .collect();

        let system_prompt = get_system_prompt(&executor).await?;
        let tools = crate::ai::functions::get_claude_tools();

        let request = crate::ai::GenerateRequest {
            messages: api_messages,
            max_tokens: Some(4096),
            temperature: Some(0.3),
            system: Some(system_prompt),
            tools: Some(tools),
        };

        app.emit(
            "ai-status",
            serde_json::json!({ "status": "Claude is thinking (recovery)" }),
        ).ok();
        super::log_backend("info", "[RECOVERY] Calling Claude API...");
        let api_response = crate::ai::generate_response(request).await
            .map_err(|e| format!("Claude API error during recovery: {}", e))?;

        let stop_reason = api_response.stop_reason.clone()
            .unwrap_or_else(|| "end_turn".to_string());
        super::log_backend(
            "info",
            &format!("[RECOVERY] Claude responded (stop_reason: {})", stop_reason),
        );

        let content_blocks = response_content_to_blocks(
            &api_response.content,
            &api_response.tool_calls,
        )?;
        let content_json = serde_json::to_string(&content_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        let assistant_msg_iri = create_assistant_message(
            &executor,
            CONVERSATION_ID,
            &content_json,
            "claude-sonnet-4-6",
            &stop_reason,
            api_response.usage.as_ref().map(|u| u.input_tokens as usize).unwrap_or(0),
            api_response.usage.as_ref().map(|u| u.output_tokens as usize).unwrap_or(0),
        ).await?;

        super::log_backend(
            "info",
            &format!("[RECOVERY] Created assistant message: {}", assistant_msg_iri),
        );
        app.emit("chat-message-added", ()).ok();

        if stop_reason == "tool_use" {
            let assistant_msg = executor.read(move |conn| {
                load_message(conn, &assistant_msg_iri)
            }).await?;

            super::log_backend("info", "[RECOVERY] Executing tools...");
            let tool_result_msg_iri = execute_tools_from_message(
                &executor,
                &app,
                CONVERSATION_ID,
                &assistant_msg,
            ).await?;

            super::log_backend(
                "info",
                &format!("[RECOVERY] Created tool result message: {}", tool_result_msg_iri),
            );
            app.emit("chat-message-added", ()).ok();

            // Continue loop
            continue;
        }

        // Natural completion - we're done
        break;
    }

    Ok(())
}

#[tauri::command]
pub async fn chat__recover_pending_tools(
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<usize, String> {
    const CONVERSATION_ID: &str = "foundation:MainChatConversation";

    let max_tokens = get_max_input_tokens(&executor).await?;

    let history = load_conversation_history(&executor, CONVERSATION_ID, max_tokens).await?;

    if history.is_empty() {
        return Ok(0); // No messages, nothing to recover
    }

    let last_msg = &history[history.len() - 1];

    // Check if last message has tool_use (assistant interrupted before tool execution)
    let has_tool_use = last_msg.content.iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));

    // Check if last message has tool_result (tool executed but no Claude response yet)
    let has_tool_result = last_msg.content.iter()
        .any(|b| matches!(b, ContentBlock::ToolResult { .. }));

    if last_msg.role == "assistant" && has_tool_use {
        // Case 1: Assistant message with tool_use - execute tools
        super::log_backend(
            "info",
            "[RECOVERY] Found assistant message with pending tool execution",
        );

        let tool_result_msg_iri = execute_tools_from_message(
            &executor,
            &app,
            CONVERSATION_ID,
            last_msg,
        ).await?;

        super::log_backend(
            "info",
            &format!("[RECOVERY] Executed tools, created message: {}", tool_result_msg_iri),
        );
        app.emit("chat-message-added", ()).ok();

        // Now continue to get Claude's response
        continue_conversation_after_recovery(app, executor.inner().clone()).await?;

        Ok(1)
    } else if last_msg.role == "user" && has_tool_result {
        // Case 2: User message with tool_result - send to Claude
        super::log_backend("info", "[RECOVERY] Found user message with pending tool result");

        continue_conversation_after_recovery(app, executor.inner().clone()).await?;

        Ok(1)
    } else {
        // Conversation is complete
        Ok(0)
    }
}
