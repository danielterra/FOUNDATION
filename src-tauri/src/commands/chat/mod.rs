mod tool_execution;
mod message_utils;
pub mod settings;
mod recovery;
mod cancellation;
mod subconscious;
pub mod retention;
pub mod conversation;
mod loop_tools;
mod engine;

pub use tool_execution::execute_tools_from_message;
pub use cancellation::AiCancellationState;
pub use recovery::run_conversation_from_current_state;

use crate::owl::{Individual, Object, DbExecutor};
use rusqlite::OptionalExtension;
use tauri::{Emitter, Manager, State};
use base64::Engine as _;
use super::chat_attachments::PENDING_ATTACHMENTS;

pub use super::chat_storage::{
    ContentBlock,
    create_user_message, load_conversation_history,
};
use super::chat_storage::create_message;

use settings::{get_max_input_tokens, load_agent_config};
use recovery::delete_messages_from_timestamp;
use cancellation::AiCancellationState as CancellationState;

pub const MAX_OUTPUT_TOKENS: u32 = 16000;
pub const SPEAK_MAX_CHARS: usize = 288;

pub async fn build_blackboard_context(executor: &crate::owl::DbExecutor, conversation_id: &str) -> Option<String> {
    let conv_id = conversation_id.to_string();
    executor.read(move |conn| {
        let widgets = crate::commands::widget::owl_get_widgets_for_conversation(conn, &conv_id)
            .unwrap_or_default();

        if widgets.is_empty() {
            return Ok(None);
        }

        use std::collections::HashSet;

        let entity_iris: Vec<String> = widgets.iter().map(|w| w.entity_id.clone()).collect();

        let entity_things = crate::owl::Thing::get_batch(conn, &entity_iris);

        let entity_types = crate::eavto::query::get_first_iri_property_batch(
            conn, &entity_iris, "rdf:type",
        ).unwrap_or_default();

        let class_iris: Vec<String> = entity_types.values()
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let class_things = crate::owl::Thing::get_batch(conn, &class_iris);

        let mut lines = Vec::new();
        for w in &widgets {
            let entity_label = entity_things.get(&w.entity_id)
                .map(|t| t.label.as_str())
                .unwrap_or_default();
            let class_iri = entity_types.get(&w.entity_id)
                .cloned()
                .unwrap_or_default();
            let class_label = class_things.get(&class_iri)
                .map(|t| t.label.as_str())
                .unwrap_or_default();

            lines.push(format!(
                "- widget_type={}, class_iri={}, class_name={}, instance_iri={}, instance_name={}",
                w.widget_type, class_iri, class_label, w.entity_id, entity_label
            ));
        }

        Ok(Some(format!(
            "## Blackboard\nThe user is currently viewing the following widgets:\n{}",
            lines.join("\n")
        )))
    }).await.ok().flatten()
}

pub(super) fn parse_timestamp(obj: &Object) -> Option<i64> {
    match obj {
        Object::DateTime(rfc3339) => chrono::DateTime::parse_from_rfc3339(rfc3339).ok().map(|dt| dt.timestamp_millis()),
        _ => None,
    }
}

#[tauri::command]
pub async fn chat__cancel(
    conversation_id: String,
    cancellation: State<'_, CancellationState>,
) -> Result<(), String> {
    cancellation.cancel(&conversation_id);
    Ok(())
}

/// Send a message and get AI response (with automatic tool execution loop)
#[tauri::command]
pub async fn chat__send_and_reply(
    content: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    attachment_iris: Option<Vec<String>>,
    conversation_id: String,
    camera_images: Option<Vec<String>>,
    thinking_enabled: Option<bool>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
    cancellation: State<'_, CancellationState>,
) -> Result<Vec<serde_json::Value>, String> {
    let conv_id = conversation_id.clone();
    let agent_config = executor.read(move |conn| {
        load_agent_config(conn, &conv_id)
    }).await?;

    if latitude.is_some() || longitude.is_some() {
        super::log_backend(
            "info",
            &format!("[CHAT] Location: lat={:?}, lon={:?}", latitude, longitude),
        );
    }
    if let Some(ref attachments) = attachment_iris {
        super::log_backend("info", &format!("[CHAT] Attachments: {:?}", attachments));
    }

    // (mime_type, base64_data) — injected into API for current turn only, never stored in DB
    let mut attachment_binaries: Vec<(String, String)> = Vec::new();
    // (file_iri, file_name) — files without a foundation:aiSummary, AI is asked to save one
    let mut files_needing_summary: Vec<(String, String)> = Vec::new();
    // (file_iri, file_name, file_path) — camera frames pending File entity creation
    let mut camera_file_data: Vec<(String, String, String)> = Vec::new();
    // IRIs of existing File entities to link via hasAttachment (regular attachments)
    let mut existing_file_iris: Vec<String> = Vec::new();

    let user_msg_iri = if attachment_iris.is_some() || camera_images.is_some() {
        let mut blocks: Vec<ContentBlock> = Vec::new();

        if let Some(ref frames) = camera_images {
            let capture_ts = chrono::Utc::now().timestamp_millis();
            for (i, frame_data) in frames.iter().enumerate() {
                let raw = base64::engine::general_purpose::STANDARD.decode(frame_data).unwrap_or_default();
                let token_estimate = super::chat_attachments::estimate_image_tokens(&raw);
                let file_path = super::chat_attachments::save_camera_frame(frame_data, i).await
                    .unwrap_or_default();
                let file_iri = format!("foundation:File_{}", capture_ts + i as i64);
                let file_name = format!("camera_frame_{}.jpg", i + 1);
                files_needing_summary.push((file_iri.clone(), file_name.clone()));
                camera_file_data.push((file_iri, file_name, file_path.clone()));
                blocks.push(ContentBlock::CameraRef { file_path, token_estimate });
            }
        }

        if let Some(ref iris) = attachment_iris {
            let mut store = PENDING_ATTACHMENTS.lock().await;
            for iri in iris {
                if let Some(att) = store.remove(iri) {
                    existing_file_iris.push(att.file_iri.clone());
                    if att.mime_type.starts_with("image/") || att.mime_type == "application/pdf" || att.mime_type.starts_with("text/") {
                        attachment_binaries.push((att.mime_type.clone(), att.data.clone()));
                    }
                    let file_iri = att.file_iri.clone();
                    let file_name = att.file_name.clone();
                    blocks.push(ContentBlock::FileRef {
                        file_iri: att.file_iri,
                        file_name: att.file_name,
                        token_estimate: att.token_estimate,
                    });
                    let iri_for_check = file_iri.clone();
                    let has_summary = executor.read(move |conn| {
                        crate::owl::get_literal_property(conn, &iri_for_check, "foundation:aiSummary")
                            .map(|v| v.is_some())
                            .map_err(|e| e.to_string())
                    }).await.unwrap_or(false);
                    if !has_summary {
                        files_needing_summary.push((file_iri, file_name));
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

    if !camera_file_data.is_empty() || !existing_file_iris.is_empty() {
        let msg_iri = user_msg_iri.clone();
        executor.write(move |conn| {
            let mut all_attachment_iris: Vec<String> = existing_file_iris;
            for (file_iri, file_name, file_path) in camera_file_data {
                let ind = Individual::new(&file_iri);
                ind.assert(conn, "foundation:File", &file_name, "photo_camera", "chat")
                    .map_err(|e| format!("Failed to create camera File entity: {}", e))?;
                ind.add_property(conn, "foundation:fileName", vec![Object::Literal {
                    value: file_name,
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "chat").map_err(|e| format!("Failed to set fileName: {}", e))?;
                if !file_path.is_empty() {
                    ind.add_property(conn, "foundation:filePath", vec![Object::Literal {
                        value: format!("file://{}", file_path),
                        datatype: Some("xsd:anyURI".to_string()),
                        language: None,
                    }], "chat").map_err(|e| format!("Failed to set filePath: {}", e))?;
                }
                all_attachment_iris.push(file_iri);
            }
            let msg = Individual::new(&msg_iri);
            msg.add_property(conn, "foundation:hasAttachment",
                all_attachment_iris.into_iter().map(Object::Iri).collect(),
                "chat")
                .map_err(|e| format!("Failed to set hasAttachment: {}", e))?;
            Ok(String::new())
        }).await?;
    }

    app.emit("chat-message-added", ()).ok();

    let content_for_subconscious = content.clone();
    let exclude_iri = user_msg_iri.clone();
    let subconscious_entities = executor.read(move |conn| {
        Ok(subconscious::run_subconscious(&content_for_subconscious, Some(&exclude_iri), conn))
    }).await.unwrap_or_default();

    if !subconscious_entities.is_empty() {
        let msg_iri_sc = user_msg_iri.clone();
        let entities_json = serde_json::to_string(&subconscious_entities)
            .unwrap_or_else(|_| "[]".to_string());
        executor.write(move |conn| {
            let msg = Individual::new(&msg_iri_sc);
            msg.add_property(conn, "foundation:subconsciousContext", vec![Object::Literal {
                value: entities_json,
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "chat").map_err(|e| format!("Failed to set subconsciousContext: {}", e))?;
            Ok(String::new())
        }).await.ok();
        app.emit("chat-message-added", ()).ok();
    }

    let subconscious_context = subconscious::format_context(&subconscious_entities);
    let blackboard_context = build_blackboard_context(&executor, &conversation_id).await;

    let first_turn_ctx = engine::FirstTurnContext {
        camera_images,
        attachment_binaries,
        files_needing_summary,
        subconscious_context,
        blackboard_context,
    };

    engine::run_conversation_loop(
        &app,
        executor.inner(),
        &conversation_id,
        &agent_config,
        Some(first_turn_ctx),
        false,
        thinking_enabled.unwrap_or(false),
        &cancellation,
    ).await?;

    Ok(vec![])
}

#[tauri::command]
pub async fn chat__get_recent_messages(
    limit: usize,
    conversation_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    let conv_id = conversation_id;

    let messages = executor.read(move |conn| {
        // Load only the N most recent message IRIs directly from SQL — no in-memory sort needed
        let message_iris = Individual::find_messages_by_conversation(conn, &conv_id, limit, 0)
            .map_err(|e| format!("Failed to query messages: {}", e))?;

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

            let input_tokens = msg.properties.iter()
                .find(|(k, _)| k == "foundation:inputTokens")
                .and_then(|(_, v)| match v { Object::Integer(n) => Some(*n), _ => None });

            let output_tokens = msg.properties.iter()
                .find(|(k, _)| k == "foundation:outputTokens")
                .and_then(|(_, v)| match v { Object::Integer(n) => Some(*n), _ => None });

            let estimated_cost = msg.properties.iter()
                .find(|(k, _)| k == "foundation:estimatedCost")
                .and_then(|(_, v)| match v {
                    Object::Number(n) => Some(*n),
                    Object::Literal { value, .. } => value.parse::<f64>().ok(),
                    _ => None,
                });

            let subconscious_entities: Vec<subconscious::SubconsciousEntity> = msg.properties.iter()
                .find(|(k, _)| k == "foundation:subconsciousContext")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => serde_json::from_str(value).ok(),
                    _ => None,
                })
                .unwrap_or_default();

            let msg_json = serde_json::json!({
                "iri": iri,
                "role": role,
                "content": content_blocks,
                "timestamp": timestamp,
                "attachments": attachments,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "estimated_cost": estimated_cost,
                "subconscious_entities": subconscious_entities,
            });

            messages_with_ts.push((timestamp, msg_json));
        }

        let messages: Vec<serde_json::Value> = messages_with_ts
            .into_iter()
            .rev()
            .map(|(_, msg)| msg)
            .collect();

        Ok(messages)
    }).await?;

    Ok(messages)
}

/// Edit a user message and re-run the conversation from that point.
/// All messages after the edited message are deleted before re-running.
#[tauri::command]
pub async fn chat__edit_and_retry(
    message_iri: String,
    new_content: String,
    conversation_id: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
    cancellation: State<'_, CancellationState>,
) -> Result<Vec<serde_json::Value>, String> {
    let _cancel_rx = cancellation.begin(&conversation_id);

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
    run_conversation_from_current_state(app_clone, executor_clone, conversation_id.clone(), &cancellation, false).await?;

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
    conversation_id: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
    cancellation: State<'_, CancellationState>,
) -> Result<Vec<serde_json::Value>, String> {
    let _cancel_rx = cancellation.begin(&conversation_id);

    let (msg_timestamp, is_user_message) = executor.read(move |conn| {
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

        if role != "assistant" && role != "user" {
            return Err(format!("Message {} has unexpected role: {}", message_iri, role));
        }

        let timestamp = ind.properties.iter()
            .find(|(k, _)| k == "foundation:sentAt")
            .and_then(|(_, v)| parse_timestamp(v))
            .ok_or("Missing timestamp")?;

        Ok((timestamp, role == "user"))
    }).await?;

    let conv_id_for_write = conversation_id.clone();
    executor.write(move |conn| {
        // For user messages: keep the message, delete everything after it.
        // For assistant messages: delete from that message onward.
        delete_messages_from_timestamp(conn, &conv_id_for_write, msg_timestamp, is_user_message)?;
        Ok(String::new())
    }).await?;

    app.emit("chat-message-added", ()).ok();

    let app_clone = app.clone();
    let executor_clone = executor.inner().clone();
    run_conversation_from_current_state(app_clone, executor_clone, conversation_id.clone(), &cancellation, false).await?;

    let max_tokens = get_max_input_tokens(&executor).await?;
    let history = load_conversation_history(&executor, &conversation_id, max_tokens).await?;
    let mut response_messages = Vec::new();
    for msg in history.iter().rev() {
        if msg.role == "assistant" && msg.timestamp > msg_timestamp {
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

pub async fn chat__recover_pending_tools(
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
    cancellation: State<'_, CancellationState>,
) -> Result<usize, String> {
    super::log_backend("info", "[RECOVERY] Checking for pending tool executions...");
    let max_tokens = get_max_input_tokens(&executor).await?;

    // Find the most recent conversation by picking the conversation that contains the
    // single most-recent AIConversationMessage across all conversations.
    let most_recent_conv = executor.read(|conn| {
        conn.query_row(
            "SELECT t_conv.object
             FROM triples t_type
             INNER JOIN triples t_conv
                 ON t_type.subject = t_conv.subject
                 AND t_conv.predicate = 'foundation:partOfConversation'
                 AND t_conv.retracted = 0
             LEFT JOIN triples t_sent
                 ON t_type.subject = t_sent.subject
                 AND t_sent.predicate = 'foundation:sentAt'
                 AND t_sent.retracted = 0
             WHERE t_type.predicate = 'rdf:type'
               AND t_type.object = 'foundation:AIConversationMessage'
               AND t_type.retracted = 0
             ORDER BY t_sent.object_value DESC
             LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| format!("Failed to find most recent conversation: {}", e))
    }).await?;

    let Some(conv_id) = most_recent_conv else {
        super::log_backend("info", "[RECOVERY] No conversations found, skipping");
        return Ok(0);
    };

    super::log_backend("info", &format!("[RECOVERY] Most recent conversation: {}", conv_id));

    let history = load_conversation_history(&executor, &conv_id, max_tokens).await?;

    super::log_backend("info", &format!(
        "[RECOVERY] Loaded {} messages from history", history.len()
    ));

    if history.is_empty() {
        super::log_backend("info", "[RECOVERY] History is empty, skipping");
        return Ok(0);
    }

    let Some(last_msg) = history.last() else {
        return Ok(0);
    };

    let has_tool_use = last_msg.content.iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));

    super::log_backend("info", &format!(
        "[RECOVERY] Last message: role={}, has_tool_use={}, content_blocks={}",
        last_msg.role, has_tool_use, last_msg.content.len()
    ));

    let has_tool_results = last_msg.role == "user"
        && last_msg.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }));

    let all_delivered = has_tool_results
        && last_msg.content.iter().all(|b| matches!(
            b,
            ContentBlock::ToolResult { content, is_error, .. }
            if content == "Delivered." && !matches!(is_error, Some(true))
        ));

    let needs_recovery = (last_msg.role == "assistant" && has_tool_use)
        || (has_tool_results && !all_delivered);

    if !needs_recovery {
        super::log_backend("info", &format!(
            "[RECOVERY] No recovery needed (role={}, has_tool_use={}, all_delivered={})",
            last_msg.role, has_tool_use, all_delivered
        ));
        return Ok(0);
    }

    super::log_backend(
        "info",
        &format!("[RECOVERY] Conversation {} needs recovery (last role: {})", conv_id, last_msg.role),
    );

    if last_msg.role == "assistant" && has_tool_use {
        let (tool_result_msg_iri, _) = execute_tools_from_message(
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

    run_conversation_from_current_state(app.clone(), executor.inner().clone(), conv_id, &cancellation, true).await?;
    Ok(1)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__purge_conversations(
    executor: State<'_, DbExecutor>,
) -> Result<usize, String> {
    retention::run_retention_policy(executor.inner()).await;
    Ok(0)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__dismiss_question(
    conversation_id: String,
    tool_use_id: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let result_blocks = vec![ContentBlock::ToolResult {
        tool_use_id,
        content: "[Question dismissed by user]".to_string(),
        is_error: None,
    }];
    let result_json = serde_json::to_string(&result_blocks)
        .map_err(|e| format!("Failed to serialize dismiss result: {}", e))?;

    super::chat_storage::create_user_message_raw(&executor, &conversation_id, &result_json).await?;
    app.emit("chat-message-added", ()).ok();

    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__answer_question(
    conversation_id: String,
    tool_use_id: String,
    answer: serde_json::Value,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let answer_str = match &answer {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };

    let result_blocks = vec![ContentBlock::ToolResult {
        tool_use_id: tool_use_id.clone(),
        content: answer_str,
        is_error: None,
    }];
    let result_json = serde_json::to_string(&result_blocks)
        .map_err(|e| format!("Failed to serialize answer: {}", e))?;

    super::chat_storage::create_user_message_raw(&executor, &conversation_id, &result_json).await?;
    app.emit("chat-message-added", ()).ok();

    let app_clone = app.clone();
    let executor_clone = executor.inner().clone();
    let conv_id = conversation_id.clone();
    tauri::async_runtime::spawn(async move {
        let cancellation = app_clone.state::<CancellationState>();
        if let Err(e) = recovery::run_conversation_from_current_state(
            app_clone.clone(), executor_clone, conv_id, &cancellation, false,
        ).await {
            super::log_backend("warn", &format!("[CHAT] Recovery after answer failed: {}", e));
        }
    });

    Ok(())
}
