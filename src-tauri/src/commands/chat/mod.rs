mod tool_execution;
mod message_utils;
mod settings;
mod recovery;

pub use tool_execution::execute_tools_from_message;

use crate::owl::{Individual, Object, DbExecutor};
use tauri::{Emitter, State};
use super::chat_attachments::PENDING_ATTACHMENTS;

pub use super::chat_storage::{
    ContentBlock,
    create_user_message, create_assistant_message, load_conversation_history,
};
use super::chat_storage::{create_message, load_message, ImageSource, DocumentSource};

use message_utils::{message_to_api_format, inject_datetime_context, sanitize_tool_pairs, response_content_to_blocks};
use settings::{get_system_prompt, get_max_input_tokens, get_supports_web_tools};
use recovery::{delete_messages_from_timestamp, continue_conversation_after_recovery};

pub const MAX_OUTPUT_TOKENS: u32 = 4096;

fn parse_timestamp(obj: &Object) -> Option<i64> {
    match obj {
        Object::DateTime(ts) => Some(*ts),
        _ => None,
    }
}

/// Send a message and get AI response (with automatic tool execution loop)
#[tauri::command]
pub async fn chat__send_and_reply(
    content: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    attachment_iris: Option<Vec<String>>,
    conversation_id: Option<String>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    let conversation_id = conversation_id.unwrap_or_else(|| "foundation:MainChatConversation".to_string());
    const MAX_TOOL_LOOPS: usize = 50;

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
        create_message(&executor, &conversation_id, "user", &content_json, None, None, None).await?
    } else {
        create_user_message(&executor, &conversation_id, &content).await?
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

        let history = load_conversation_history(&executor, &conversation_id, max_tokens).await?;
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
        let supports_web_tools = get_supports_web_tools(&executor).await;

        let request = crate::ai::GenerateRequest {
            messages: api_messages,
            max_tokens: Some(MAX_OUTPUT_TOKENS),
            temperature: Some(0.3),
            system: Some(system_prompt),
            tools: Some(tools),
            supports_web_tools,
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

        if let Some(usage) = &api_response.usage {
            super::chat_storage::log_api_call(
                &executor,
                &current_model,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
            ).await
                .unwrap_or_else(|e| super::log_backend("warn", &format!("[CHAT] Failed to log API call: {}", e)));
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
                &conversation_id,
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
    conversation_id: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    let conv_id = conversation_id.unwrap_or_else(|| "foundation:MainChatConversation".to_string());

    let messages = executor.read(move |conn| {
        let message_iris = Individual::find_by_class_and_properties(
            conn,
            "foundation:AIConversationMessage",
            &[("foundation:partOfConversation", &conv_id)],
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
                .and_then(|(_, v)| parse_timestamp(v))
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

/// Edit a user message and re-run the conversation from that point.
/// All messages after the edited message are deleted before re-running.
#[tauri::command]
pub async fn chat__edit_and_retry(
    message_iri: String,
    new_content: String,
    conversation_id: Option<String>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    let conversation_id = conversation_id.unwrap_or_else(|| "foundation:MainChatConversation".to_string());

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
            .and_then(|(_, v)| parse_timestamp(v))
            .ok_or("Missing timestamp")?;

        Ok((message_iri.clone(), timestamp))
    }).await?;

    let (iri, timestamp) = msg_timestamp;

    let iri_clone = iri.clone();
    let conv_id_for_write = conversation_id.clone();
    executor.write(move |conn| {
        delete_messages_from_timestamp(conn, &conv_id_for_write, timestamp, true)?;

        let new_blocks = vec![ContentBlock::Text { text: new_content.clone() }];
        let new_content_json = serde_json::to_string(&new_blocks)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

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

        let msg = Individual::new(&iri_clone);
        msg.add_property(conn, "foundation:content", vec![Object::Literal {
            value: new_content_json,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "chat").map_err(|e| format!("Failed to set new content: {}", e))?;

        Ok(String::new())
    }).await?;

    app.emit("chat-message-added", ()).ok();

    let app_clone = app.clone();
    let executor_clone = executor.inner().clone();
    let mut response_messages = Vec::new();
    continue_conversation_after_recovery(app_clone, executor_clone, conversation_id.clone()).await?;

    let max_tokens = get_max_input_tokens(&executor).await?;
    let history = load_conversation_history(&executor, &conversation_id, max_tokens).await?;
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
    conversation_id: Option<String>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    let conversation_id = conversation_id.unwrap_or_else(|| "foundation:MainChatConversation".to_string());

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
            .and_then(|(_, v)| parse_timestamp(v))
            .ok_or("Missing timestamp")?;

        Ok(timestamp)
    }).await?;

    let conv_id_for_write = conversation_id.clone();
    executor.write(move |conn| {
        delete_messages_from_timestamp(conn, &conv_id_for_write, msg_timestamp, false)?;
        Ok(String::new())
    }).await?;

    app.emit("chat-message-added", ()).ok();

    let app_clone = app.clone();
    let executor_clone = executor.inner().clone();
    continue_conversation_after_recovery(app_clone, executor_clone, conversation_id.clone()).await?;

    let max_tokens = get_max_input_tokens(&executor).await?;
    let history = load_conversation_history(&executor, &conversation_id, max_tokens).await?;
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

#[tauri::command]
pub async fn chat__recover_pending_tools(
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<usize, String> {
    let max_tokens = get_max_input_tokens(&executor).await?;

    let mut conversation_iris: Vec<String> = vec!["foundation:MainChatConversation".to_string()];
    let additional = executor.read(|conn| {
        Individual::find_by_class_and_properties(conn, "foundation:AIConversation", &[])
            .map_err(|e| format!("Failed to query conversations: {}", e))
    }).await?;
    for iri in additional {
        if iri != "foundation:MainChatConversation" {
            conversation_iris.push(iri);
        }
    }

    let mut recovered = 0usize;

    for conv_id in conversation_iris {
        let history = load_conversation_history(&executor, &conv_id, max_tokens).await?;

        if history.is_empty() {
            continue;
        }

        let last_msg = history.last().expect("history checked non-empty");

        let has_tool_use = last_msg.content.iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }));

        let needs_recovery = (last_msg.role == "assistant" && has_tool_use)
            || last_msg.role == "user";

        if !needs_recovery {
            continue;
        }

        super::log_backend(
            "info",
            &format!("[RECOVERY] Conversation {} needs recovery (last role: {})", conv_id, last_msg.role),
        );

        if last_msg.role == "assistant" && has_tool_use {
            let tool_result_msg_iri = execute_tools_from_message(
                &executor,
                &app,
                &conv_id,
                last_msg,
            ).await?;
            super::log_backend(
                "info",
                &format!("[RECOVERY] Executed tools, created message: {}", tool_result_msg_iri),
            );
            app.emit("chat-message-added", ()).ok();
        }

        continue_conversation_after_recovery(app.clone(), executor.inner().clone(), conv_id).await?;
        recovered += 1;
    }

    Ok(recovered)
}

#[tauri::command]
pub async fn chat__create_conversation(
    label: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let conv_iri = format!("foundation:Conversation_{}", timestamp);
    let conv_label = label.unwrap_or_else(|| "New Conversation".to_string());

    let iri_clone = conv_iri.clone();
    let label_clone = conv_label.clone();

    executor.write(move |conn| {
        let conv = Individual::new(&iri_clone);

        conv.assert(conn, "foundation:AIConversation", &label_clone, "chat", "ai")
            .map_err(|e| format!("Failed to create conversation: {}", e))?;

        conv.add_property(conn, "foundation:createdAt", vec![
            Object::DateTime(timestamp),
        ], "ai").map_err(|e| format!("Failed to set createdAt: {}", e))?;

        conv.add_property(conn, "foundation:hasStatus", vec![
            Object::Iri("foundation:InProgress".to_string()),
        ], "ai").map_err(|e| format!("Failed to set conversation status: {}", e))?;

        Ok(iri_clone)
    }).await?;

    Ok(serde_json::json!({ "iri": conv_iri, "label": conv_label }))
}

#[tauri::command]
pub async fn chat__list_conversations(
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor.read(move |conn| {
        let iris = Individual::find_by_class_with_date_range(
            conn,
            "foundation:AIConversation",
            None,
            None,
            false,
        ).map_err(|e| format!("Failed to query conversations: {}", e))?;

        let mut conversations: Vec<(i64, serde_json::Value)> = Vec::new();

        let main_iri = "foundation:MainChatConversation";
        let main_already_included = iris.iter().any(|i| i == main_iri);
        let all_iris: Vec<String> = if main_already_included {
            iris
        } else {
            std::iter::once(main_iri.to_string()).chain(iris).collect()
        };

        for iri in all_iris {
            let (label, started_at) = if iri == main_iri {
                ("Main Chat".to_string(), 0i64)
            } else {
                let ind = Individual::get(conn, &iri)
                    .ok().flatten()
                    .unwrap_or_else(|| Individual::new(&iri));

                let lbl = ind.properties.iter()
                    .find(|(k, _)| k == "rdfs:label")
                    .and_then(|(_, v)| match v {
                        Object::Literal { value, .. } => Some(value.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| iri.clone());

                let ts = ind.properties.iter()
                    .find(|(k, _)| k == "foundation:createdAt")
                    .and_then(|(_, v)| parse_timestamp(v))
                    .unwrap_or(0);

                (lbl, ts)
            };

            conversations.push((started_at, serde_json::json!({
                "iri": iri,
                "label": label,
                "startedAt": started_at,
            })));
        }

        conversations.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(conversations.into_iter().map(|(_, v)| v).collect())
    }).await
}
