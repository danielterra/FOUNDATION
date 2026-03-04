/// Simplified Chat Implementation
///
/// This is a complete rewrite that eliminates complexity:
/// - Messages store content as JSON (no separate ToolUse/ToolResult entities)
/// - Simple linear conversation flow
/// - Recovery logic for incomplete conversations (tool_use/tool_result pairs)
/// - Direct mapping to LLM APIs (Claude, OpenAI, etc.)

use crate::owl::DbExecutor;
use crate::ai::functions::ToolCall;
use crate::owl::{Individual, Object, Connection};
use tauri::{Emitter, State};
use super::chat_attachments::PENDING_ATTACHMENTS;

pub use super::chat_storage::{
    ContentBlock, AIConversationMessage,
    create_user_message, create_assistant_message, load_conversation_history,
};
use super::chat_storage::{create_message, load_message, ImageSource, DocumentSource};

const MAX_OUTPUT_TOKENS: u32 = 4096;

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
        super::log_backend("warn", &format!(
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
            ContentBlock::FileRef { file_iri, file_name, .. } => {
                ApiContentBlock::Text {
                    text: format!(
                        "[Attached file: {} | Knowledge base IRI: {}]",
                        file_name, file_iri
                    ),
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

/// Prepend current date/time as a text block to the last user message in the list.
/// This keeps the system prompt fully static (cacheable) while still giving Claude
/// temporal context on every request. No-op if the list is empty.
fn inject_datetime_context(messages: &mut Vec<crate::ai::ChatMessage>) {
    use crate::ai::providers::{ContentBlock as ApiContentBlock, MessageContent};

    let date_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z").to_string();
    let datetime_block = ApiContentBlock::Text {
        text: format!("Current date/time: {}", date_time),
    };

    // The Claude API requires tool_result messages to begin with tool_result blocks —
    // injecting a Text block before them causes a 400 error.
    let target = messages.iter_mut().rev().find(|msg| {
        if msg.role != "user" {
            return false;
        }
        match &msg.content {
            MessageContent::ContentBlocks(blocks) => {
                !blocks.iter().any(|b| matches!(b, ApiContentBlock::ToolResult { .. }))
            }
            MessageContent::Text(_) => true,
        }
    });

    if let Some(msg) = target {
        match &mut msg.content {
            MessageContent::ContentBlocks(ref mut blocks) => {
                blocks.insert(0, datetime_block);
            }
            MessageContent::Text(text) => {
                msg.content = MessageContent::ContentBlocks(vec![
                    datetime_block,
                    ApiContentBlock::Text { text: text.clone() },
                ]);
            }
        }
    }
}

/// Sanitize tool pairs: ensure every ToolUse in an assistant message has a matching
/// ToolResult in the next user message. Injects synthetic error results for any orphaned
/// tool_use ids (e.g. when the conversation was interrupted before all results were saved).
fn sanitize_tool_pairs(messages: &mut Vec<crate::ai::ChatMessage>) {
    use crate::ai::providers::{ContentBlock as ApiContentBlock, MessageContent};
    use std::collections::HashSet;

    for i in 0..messages.len().saturating_sub(1) {
        if messages[i].role != "assistant" {
            continue;
        }

        let tool_use_ids: Vec<String> =
            if let MessageContent::ContentBlocks(ref blocks) = messages[i].content {
            blocks.iter().filter_map(|b| {
                if let ApiContentBlock::ToolUse { id, .. } = b { Some(id.clone()) } else { None }
            }).collect()
        } else {
            continue;
        };

        if tool_use_ids.is_empty() {
            continue;
        }

        let next = i + 1;
        if next >= messages.len() || messages[next].role != "user" {
            continue;
        }

        let existing_results: HashSet<String> =
            if let MessageContent::ContentBlocks(ref blocks) = messages[next].content {
            blocks.iter().filter_map(|b| {
                if let ApiContentBlock::ToolResult { tool_use_id, .. } = b {
                    Some(tool_use_id.clone())
                } else {
                    None
                }
            }).collect()
        } else {
            HashSet::new()
        };

        let missing: Vec<String> = tool_use_ids.into_iter()
            .filter(|id| !existing_results.contains(id))
            .collect();

        if missing.is_empty() {
            continue;
        }

        super::log_backend(
            "warn",
            &format!(
                "[RECOVERY] Injecting {} synthetic tool_result(s) for orphaned tool_use ids: {:?}",
                missing.len(), missing
            ),
        );

        if let MessageContent::ContentBlocks(ref mut blocks) = messages[next].content {
            for id in missing {
                blocks.push(ApiContentBlock::ToolResult {
                    tool_use_id: id,
                    content: "Tool result unavailable (conversation was interrupted)".to_string(),
                    is_error: Some(true),
                });
            }
        }
    }

    // Strip trailing tool_use blocks from the last assistant message — the Claude API requires
    // every tool_use to be followed by a tool_result, which is impossible for the last message.
    if let Some(last) = messages.last_mut() {
        if last.role == "assistant" {
            if let MessageContent::ContentBlocks(ref mut blocks) = last.content {
                let had_tool_use =
                    blocks.iter().any(|b| matches!(b, ApiContentBlock::ToolUse { .. }));
                if had_tool_use {
                    super::log_backend(
                        "warn", "[CHAT] Stripping trailing tool_use blocks (end of history)",
                    );
                    blocks.retain(|b| !matches!(b, ApiContentBlock::ToolUse { .. }));
                }
            }
        }
    }
    if messages.last().map_or(false, |m| {
        matches!(&m.content, MessageContent::ContentBlocks(b) if b.is_empty())
    }) {
        messages.pop();
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

    let mut attached_file_iris: Vec<String> = Vec::new();

    let user_msg_iri = if let Some(ref iris) = attachment_iris {
        let mut blocks: Vec<ContentBlock> = Vec::new();
        {
            let mut store = PENDING_ATTACHMENTS.lock().await;
            for iri in iris {
                if let Some(att) = store.remove(iri) {
                    attached_file_iris.push(att.file_iri.clone());
                    if att.mime_type.starts_with("image/") {
                        blocks.push(ContentBlock::Image {
                            source: ImageSource {
                                source_type: "base64".to_string(),
                                media_type: att.mime_type,
                                data: att.data,
                            },
                        });
                        blocks.push(ContentBlock::FileRef {
                            file_iri: att.file_iri,
                            file_name: att.file_name,
                            token_estimate: att.token_estimate,
                        });
                    } else if att.mime_type == "application/pdf" {
                        blocks.push(ContentBlock::Document {
                            source: DocumentSource {
                                source_type: "base64".to_string(),
                                media_type: att.mime_type,
                                data: att.data,
                            },
                        });
                        blocks.push(ContentBlock::FileRef {
                            file_iri: att.file_iri,
                            file_name: att.file_name,
                            token_estimate: att.token_estimate,
                        });
                    } else {
                        super::log_backend(
                            "warn",
                            &format!("[CHAT] Unsupported MIME type: {}", att.mime_type),
                        );
                    }
                } else {
                    super::log_backend("warn", &format!("[CHAT] Attachment not found: {}", iri));
                }
            }
        }
        if !content.is_empty() {
            blocks.push(ContentBlock::Text { text: content.clone() });
        }
        let content_json = serde_json::to_string(&blocks)
            .map_err(|e| format!("Failed to serialize message content: {}", e))?;
        create_message(&executor, CONVERSATION_ID, "user", &content_json, None, None, None).await?
    } else {
        create_user_message(&executor, CONVERSATION_ID, &content).await?
    };
    super::log_backend("info", &format!("[CHAT] Created user message: {}", user_msg_iri));

    if !attached_file_iris.is_empty() {
        let msg_iri = user_msg_iri.clone();
        executor.write(move |conn| {
            let msg = Individual::new(&msg_iri);
            for file_iri in attached_file_iris {
                msg.add_property(conn, "foundation:hasAttachment",
                    vec![Object::Iri(file_iri)], "chat")
                    .map_err(|e| format!("Failed to set hasAttachment: {}", e))?;
            }
            Ok(String::new())
        }).await?;
    }

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

        let current_model = crate::ai::get_current_model()?;
        let assistant_msg_iri = create_assistant_message(
            &executor,
            CONVERSATION_ID,
            &content_json,
            &current_model,
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

        let has_tool_use = !api_response.tool_calls.is_empty();
        if stop_reason == "tool_use" || (stop_reason == "max_tokens" && has_tool_use) {
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

async fn get_system_prompt(executor: &DbExecutor) -> Result<String, String> {
    executor.read(|conn| {
        let template = if let Ok(Some(setting)) = Individual::get(
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

        let user = Individual::get(conn, "foundation:ThisUser").ok().flatten();
        let ai = Individual::get(conn, "foundation:LocalAIAssistant").ok().flatten();

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

        let language = Individual::get(conn, "foundation:DefaultLanguageSetting")
            .ok()
            .flatten()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "English".to_string());

        let locale = Individual::get(conn, "foundation:DefaultLocaleSetting")
            .ok()
            .flatten()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "en_US".to_string());

        let country = Individual::get(conn, "foundation:DefaultCountrySetting")
            .ok()
            .flatten()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "United States".to_string());

        let location_info = Individual::get(conn, "foundation:DefaultLocationInfoSetting")
            .ok()
            .flatten()
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
            .replace("{date_time}", "")
            .replace("{language}", &language)
            .replace("{locale}", &locale)
            .replace("{country}", &country)
            .replace("{location_info}", &location_info);

        Ok(prompt)
    }).await
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
                .map_err(|e| format!("Failed to get message {}: {}", iri, e))?
                .ok_or_else(|| format!("Message {} not found", iri))?;

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

            let attachments: Vec<serde_json::Value> = content_blocks.iter()
                .filter_map(|block| {
                    if let ContentBlock::FileRef { file_iri, file_name, .. } = block {
                        let file_entity = Individual::get(conn, file_iri).ok().flatten()?;

                        let file_path = file_entity.properties.iter()
                            .find(|(k, _)| k == "foundation:filePath")
                            .and_then(|(_, v)| match v {
                                Object::Literal { value, .. } => {
                                    Some(value.trim_start_matches("file://").to_string())
                                },
                                _ => None,
                            })?;

                        let file_size = file_entity.properties.iter()
                            .find(|(k, _)| k == "foundation:fileSize")
                            .and_then(|(_, v)| match v {
                                Object::Integer(n) => Some(*n),
                                _ => None,
                            })
                            .unwrap_or(0);

                        let mime_type = file_entity.properties.iter()
                            .find(|(k, _)| k == "foundation:hasFileType")
                            .and_then(|(_, v)| match v {
                                Object::Iri(iri) => Individual::get(conn, iri.as_str()).ok()
                                    .flatten()
                                    .and_then(|ft| ft.properties.into_iter()
                                        .find(|(k, _)| k == "foundation:mimeType")
                                        .and_then(|(_, v)| match v {
                                            Object::Literal { value, .. } => Some(value),
                                            _ => None,
                                        })
                                    ),
                                _ => None,
                            })
                            .unwrap_or_else(|| "application/octet-stream".to_string());

                        Some(serde_json::json!({
                            "fileName": file_name,
                            "filePath": file_path,
                            "fileSize": file_size,
                            "mimeType": mime_type,
                        }))
                    } else {
                        None
                    }
                })
                .collect();

            let msg_json = serde_json::json!({
                "iri": iri,
                "role": role,
                "content": content_blocks,
                "timestamp": timestamp,
                "attachments": attachments,
            });

            messages_with_ts.push((timestamp, msg_json));
        }

        messages_with_ts.sort_by(|a, b| b.0.cmp(&a.0));

        let messages: Vec<serde_json::Value> = messages_with_ts
            .into_iter()
            .take(limit)
            .map(|(_, msg)| msg)
            .collect();

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
        if let Ok(Some(setting)) =
            Individual::get(conn, "foundation:DefaultMaxInputTokensSetting")
        {
            if let Some(Object::Literal { value, .. }) = setting.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .map(|(_, v)| v) {
                if let Ok(tokens) = value.parse::<usize>() {
                    return Ok(tokens);
                }
            }
        }

        let model_iri = get_ai_model_iri(conn)?;

        if let Some(iri) = model_iri {
            let model = Individual::get(conn, &iri)
                .map_err(|e| format!("Failed to get AI model: {}", e))?
                .ok_or_else(|| format!("Failed to get AI model: IRI not found"))?;

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
    if let Ok(Some(setting)) = Individual::get(conn, "foundation:DefaultAIModelSetting") {
        if let Some(Object::Literal { value, .. }) = setting.properties.iter()
            .find(|(k, _)| k == "foundation:settingValue")
            .map(|(_, v)| v) {
            return Ok(Some(value.clone()));
        }
    }

    Ok(None)
}

/// Retract all messages in the conversation with sentAt >= from_timestamp (exclusive of the
/// message at exactly from_timestamp when exclude_exact is true).
fn delete_messages_from_timestamp(
    conn: &mut crate::owl::Connection,
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

        let timestamp = ind.properties.iter()
            .find(|(k, _)| k == "foundation:sentAt")
            .and_then(|(_, v)| match v {
                Object::DateTime(ts) => Some(*ts),
                Object::Literal { value, .. } => value.parse::<i64>().ok(),
                _ => None,
            });

        let ts = match timestamp {
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

/// Edit a user message and re-run the conversation from that point.
/// All messages after the edited message are deleted before re-running.
#[tauri::command]
pub async fn chat__edit_and_retry(
    message_iri: String,
    new_content: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    const CONVERSATION_ID: &str = "foundation:MainChatConversation";

    // Find the message timestamp
    let msg_timestamp = executor.read(move |conn| {
        let ind = Individual::get(conn, &message_iri)
            .map_err(|e| format!("Failed to load message: {}", e))?
            .ok_or_else(|| format!("Message {} not found", message_iri))?;

        let role = ind.properties.iter()
            .find(|(k, _)| k == "foundation:role")
            .and_then(|(_, v)| match v {
                Object::Literal { value, .. } => Some(value.clone()),
                _ => None,
            })
            .ok_or("Missing role")?;

        if role != "user" {
            return Err(format!("Message {} is not a user message", message_iri));
        }

        let timestamp = ind.properties.iter()
            .find(|(k, _)| k == "foundation:sentAt")
            .and_then(|(_, v)| match v {
                Object::DateTime(ts) => Some(*ts),
                Object::Literal { value, .. } => value.parse::<i64>().ok(),
                _ => None,
            })
            .ok_or("Missing timestamp")?;

        Ok((message_iri.clone(), timestamp))
    }).await?;

    let (iri, timestamp) = msg_timestamp;

    // Delete all messages after the edited message (exclusive — keep the message itself)
    let conversation_id = CONVERSATION_ID.to_string();
    let iri_clone = iri.clone();
    executor.write(move |conn| {
        // Delete messages strictly after this timestamp
        delete_messages_from_timestamp(conn, &conversation_id, timestamp, true)?;

        // Update the message content: retract old content, assert new content
        let new_blocks = vec![ContentBlock::Text { text: new_content.clone() }];
        let new_content_json = serde_json::to_string(&new_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        // Retract old content triple
        let ind = Individual::get(conn, &iri_clone)
            .map_err(|e| format!("Failed to reload message: {}", e))?
            .ok_or_else(|| format!("Message {} not found after delete", iri_clone))?;

        for (k, v) in &ind.properties {
            if k == "foundation:content" {
                let value_str = match v {
                    Object::Literal { value, .. } => value.clone(),
                    _ => continue,
                };
                Individual::remove_property_value(conn, &iri_clone, "foundation:content", &value_str, "chat")
                    .map_err(|e| format!("Failed to retract old content: {}", e))?;
                break;
            }
        }

        // Assert new content
        let msg = Individual::new(&iri_clone);
        msg.add_property(conn, "foundation:content", vec![Object::Literal {
            value: new_content_json,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "chat").map_err(|e| format!("Failed to set new content: {}", e))?;

        Ok(String::new())
    }).await?;

    app.emit("chat-message-added", ()).ok();

    // Re-run the conversation loop from the updated history
    let app_clone = app.clone();
    let executor_clone = executor.inner().clone();
    let mut response_messages = Vec::new();
    continue_conversation_after_recovery(app_clone, executor_clone).await.map_err(|e| e)?;

    // Load newly created assistant messages to return
    let max_tokens = get_max_input_tokens(&executor).await?;
    let history = load_conversation_history(&executor, CONVERSATION_ID, max_tokens).await?;
    for msg in history.iter().rev() {
        if msg.role == "assistant" && msg.timestamp > timestamp {
            response_messages.push(serde_json::json!({
                "iri": msg.iri,
                "role": "assistant",
                "content": msg.content,
            }));
        }
    }
    response_messages.reverse();

    Ok(response_messages)
}

/// Retry from an assistant message: delete it and all subsequent messages, then re-run.
#[tauri::command]
pub async fn chat__retry_from_message(
    message_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    const CONVERSATION_ID: &str = "foundation:MainChatConversation";

    let msg_timestamp = executor.read(move |conn| {
        let ind = Individual::get(conn, &message_iri)
            .map_err(|e| format!("Failed to load message: {}", e))?
            .ok_or_else(|| format!("Message {} not found", message_iri))?;

        let role = ind.properties.iter()
            .find(|(k, _)| k == "foundation:role")
            .and_then(|(_, v)| match v {
                Object::Literal { value, .. } => Some(value.clone()),
                _ => None,
            })
            .ok_or("Missing role")?;

        if role != "assistant" {
            return Err(format!("Message {} is not an assistant message", message_iri));
        }

        let timestamp = ind.properties.iter()
            .find(|(k, _)| k == "foundation:sentAt")
            .and_then(|(_, v)| match v {
                Object::DateTime(ts) => Some(*ts),
                Object::Literal { value, .. } => value.parse::<i64>().ok(),
                _ => None,
            })
            .ok_or("Missing timestamp")?;

        Ok(timestamp)
    }).await?;

    // Delete the assistant message and all subsequent messages (inclusive of this timestamp)
    let conversation_id = CONVERSATION_ID.to_string();
    executor.write(move |conn| {
        delete_messages_from_timestamp(conn, &conversation_id, msg_timestamp, false)?;
        Ok(String::new())
    }).await?;

    app.emit("chat-message-added", ()).ok();

    // Re-run the conversation loop
    let app_clone = app.clone();
    let executor_clone = executor.inner().clone();
    continue_conversation_after_recovery(app_clone, executor_clone).await?;

    // Load newly created assistant messages after the deleted timestamp
    let max_tokens = get_max_input_tokens(&executor).await?;
    let history = load_conversation_history(&executor, CONVERSATION_ID, max_tokens).await?;
    let mut response_messages = Vec::new();
    for msg in history.iter().rev() {
        if msg.role == "assistant" && msg.timestamp >= msg_timestamp {
            response_messages.push(serde_json::json!({
                "iri": msg.iri,
                "role": "assistant",
                "content": msg.content,
            }));
        }
    }
    response_messages.reverse();

    Ok(response_messages)
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

        let current_model = crate::ai::get_current_model()?;
        let assistant_msg_iri = create_assistant_message(
            &executor,
            CONVERSATION_ID,
            &content_json,
            &current_model,
            &stop_reason,
            api_response.usage.as_ref().map(|u| u.input_tokens as usize).unwrap_or(0),
            api_response.usage.as_ref().map(|u| u.output_tokens as usize).unwrap_or(0),
        ).await?;

        super::log_backend(
            "info",
            &format!("[RECOVERY] Created assistant message: {}", assistant_msg_iri),
        );
        app.emit("chat-message-added", ()).ok();

        let has_tool_use = !api_response.tool_calls.is_empty();
        if stop_reason == "tool_use" || (stop_reason == "max_tokens" && has_tool_use) {
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

            continue;
        }

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

    let has_tool_use = last_msg.content.iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));

    let has_tool_result = last_msg.content.iter()
        .any(|b| matches!(b, ContentBlock::ToolResult { .. }));

    if last_msg.role == "assistant" && has_tool_use {
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

        continue_conversation_after_recovery(app, executor.inner().clone()).await?;

        Ok(1)
    } else if last_msg.role == "user" && has_tool_result {
        super::log_backend("info", "[RECOVERY] Found user message with pending tool result");

        continue_conversation_after_recovery(app, executor.inner().clone()).await?;

        Ok(1)
    } else if last_msg.role == "user" {
        super::log_backend("info", "[RECOVERY] Found unanswered user message, sending to Claude");

        continue_conversation_after_recovery(app, executor.inner().clone()).await?;

        Ok(1)
    } else {
        Ok(0)
    }
}
