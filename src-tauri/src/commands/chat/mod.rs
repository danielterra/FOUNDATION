mod tool_execution;
mod message_utils;
pub mod settings;
mod recovery;
mod cancellation;
mod stopwords;
mod subconscious;
pub mod retention;
pub mod conversation;
mod loop_tools;
mod engine;
mod trace;
mod foundation_context;
pub mod compaction;

pub use tool_execution::execute_tools_from_message;
pub use cancellation::{AiCancellationState, ConversationProcessingState};
pub use recovery::{run_conversation_from_current_state, process_conversation_queue};

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

pub const MAX_OUTPUT_TOKENS: u32 = 64000;

pub async fn build_blackboard_context(executor: &crate::owl::DbExecutor, conversation_id: &str) -> Option<String> {
    let conv_id = conversation_id.to_string();
    let bb_iri = executor.write(move |conn| {
        crate::commands::widget::resolve_blackboard_iri(conn, Some(&conv_id))
    }).await.ok()?;

    executor.read(move |conn| {
        let widgets = crate::commands::widget::owl_get_widgets_for_blackboard(conn, &bb_iri)
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

    // Compact before persisting the user message so the context sent to the API is always
    // [summary] + [tail] + [user_message] — the user's last message is never swallowed by
    // compaction and the resulting history is deterministic.
    {
        let (_, current_tokens) = load_conversation_history(executor.inner(), &conversation_id, agent_config.max_tokens)
            .await
            .unwrap_or_default();
        compaction::run_compaction_if_needed(
            executor.inner().clone(),
            app.clone(),
            conversation_id.clone(),
            compaction::CompactionConfig::from_agent_config(&agent_config),
            current_tokens,
        ).await;
    }

    // (mime_type, base64_data) — injected into API for current turn only, never stored in DB
    let mut attachment_binaries: Vec<(String, String)> = Vec::new();
    // (file_iri, file_name) — files without a foundation:aiSummary, AI is asked to save one
    let mut files_needing_summary: Vec<(String, String)> = Vec::new();
    // (file_iri, file_name, file_path) — camera frames pending File entity creation
    let mut camera_file_data: Vec<(String, String, String)> = Vec::new();
    // IRIs of existing File entities to link via hasAttachment (regular attachments)
    let mut existing_file_iris: Vec<String> = Vec::new();

    app.emit("ai-status", serde_json::json!({
        "status": "Salvando mensagem...", "phase": "prep", "conversationId": conversation_id
    })).ok();

    let user_msg_iri = if attachment_iris.is_some() || camera_images.is_some() {
        let mut blocks: Vec<ContentBlock> = Vec::new();

        if let Some(ref frames) = camera_images {
            app.emit("ai-status", serde_json::json!({
                "status": "Processando imagens...", "phase": "prep", "conversationId": conversation_id
            })).ok();
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
            app.emit("ai-status", serde_json::json!({
                "status": "Processando anexos...", "phase": "prep", "conversationId": conversation_id
            })).ok();
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
                        ai_summary: None,
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
        create_message(&executor, &conversation_id, "user", blocks, None, None, None).await?
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
                    // file_path is already in portable form (relative to foundation_dir
                    // when possible) — see save_camera_frame() / to_portable_path().
                    ind.add_property(conn, "foundation:filePath", vec![Object::Literal {
                        value: file_path,
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

    app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();

    app.emit("ai-status", serde_json::json!({
        "status": "Consultando subconsciente...", "phase": "prep", "conversationId": conversation_id
    })).ok();

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
        app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();
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
    processing: State<'_, ConversationProcessingState>,
) -> Result<serde_json::Value, String> {
    let conv_id = conversation_id;
    let actually_processing = processing.is_active(&conv_id);

    let display_units = executor.read(move |conn| {
        // Over-fetch raw messages to account for intermediate tool_result / tool_use messages
        // that will be folded into display units and not counted toward `limit`.
        let fetch_count = limit * 5 + 10;
        let message_iris = crate::core_ontology::chat::find_messages_by_conversation(conn, &conv_id, fetch_count, 0)
            .map_err(|e| format!("Failed to query messages: {}", e))?;

        // `find_messages_by_conversation` returns newest-first; reverse to chronological order.
        let message_iris: Vec<String> = message_iris.into_iter().rev().collect();

        // --- batch load all data (4 queries instead of O(messages × blocks × 10)) ---
        let t_batch_start = std::time::Instant::now();

        // Batch 1: all message-level triples
        let t1 = std::time::Instant::now();
        let msg_triples_map = Individual::batch_load_triples(conn, &message_iris)
            .map_err(|e| format!("Failed to batch-load message triples: {}", e))?;
        let ms1 = t1.elapsed().as_millis();

        // Batch 2: all content block triples
        let all_block_iris: Vec<String> = message_iris.iter()
            .filter_map(|iri| msg_triples_map.get(iri))
            .flat_map(|triples| super::chat_storage::triple_iris(triples, "foundation:hasContentBlock"))
            .collect();

        let t2 = std::time::Instant::now();
        let block_triples_map = Individual::batch_load_triples(conn, &all_block_iris)
            .map_err(|e| format!("Failed to batch-load block triples: {}", e))?;
        let ms2 = t2.elapsed().as_millis();

        // Batch 3: ToolUseBlocks referenced by ToolResultBlocks (for tool_use_id resolution)
        let tool_use_iris: Vec<String> = block_triples_map.values()
            .flat_map(|triples| {
                triples.iter()
                    .filter(|t| t.predicate == "anthropic:resultOf")
                    .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let t3 = std::time::Instant::now();
        let tool_use_triples_map = Individual::batch_load_triples(conn, &tool_use_iris)
            .map_err(|e| format!("Failed to batch-load tool-use block triples: {}", e))?;
        let ms3 = t3.elapsed().as_millis();

        // Batch 4: file entity triples for FileRef attachment resolution
        let file_iris: Vec<String> = block_triples_map.values()
            .flat_map(|triples| {
                triples.iter()
                    .filter(|t| t.predicate == "rdf:type" && t.object.as_iri() == Some("foundation:FileRefBlock"))
                    .flat_map(|_| triples.iter()
                        .filter(|t| t.predicate == "foundation:fileRef")
                        .filter_map(|t| t.object.as_iri().map(|s| s.to_string())))
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let t4 = std::time::Instant::now();
        let file_triples_map = Individual::batch_load_triples(conn, &file_iris)
            .map_err(|e| format!("Failed to batch-load file triples: {}", e))?;

        // Collect all file type IRIs for mime-type resolution
        let file_type_iris: Vec<String> = file_triples_map.values()
            .flat_map(|triples| {
                triples.iter()
                    .filter(|t| t.predicate == "foundation:hasFileType")
                    .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let file_type_triples_map = Individual::batch_load_triples(conn, &file_type_iris)
            .map_err(|e| format!("Failed to batch-load file type triples: {}", e))?;
        let ms4 = t4.elapsed().as_millis();

        let total_batch_ms = t_batch_start.elapsed().as_millis();
        super::log_backend("debug", &format!(
            "[CMD] chat_messages({}msgs {}blocks {}tool_uses) batch1={}ms batch2={}ms batch3={}ms batch4={}ms total={}ms",
            message_iris.len(), all_block_iris.len(), tool_use_iris.len(),
            ms1, ms2, ms3, ms4, total_batch_ms
        ));

        // --- helpers (all work from in-memory maps, zero additional DB queries) ---

        let get_blocks_for = |iri: &str| -> Vec<ContentBlock> {
            let block_iris = msg_triples_map.get(iri)
                .map(|t| super::chat_storage::triple_iris(t, "foundation:hasContentBlock"))
                .unwrap_or_default();
            let mut indexed: Vec<(i64, ContentBlock)> = block_iris.iter()
                .filter_map(|block_iri| {
                    block_triples_map.get(block_iri)
                        .and_then(|bt| super::chat_storage::parse_content_block_from_batch(bt, &tool_use_triples_map))
                })
                .collect();
            indexed.sort_by_key(|(idx, _)| *idx);
            indexed.into_iter().map(|(_, b)| b).collect()
        };

        let resolve_attachments = |msg_iri: &str, blocks: &[ContentBlock]| -> Vec<serde_json::Value> {
            blocks.iter().filter_map(|block| {
                let ContentBlock::FileRef { file_iri, file_name, .. } = block else { return None; };
                let file_triples = file_triples_map.get(file_iri.as_str())?;

                let file_path = super::chat_storage::triple_str(file_triples, "foundation:filePath")
                    .map(|p| crate::paths::resolve_path(&p).to_string_lossy().into_owned())?;

                let file_size = super::chat_storage::triple_int(file_triples, "foundation:fileSize")
                    .unwrap_or(0);

                let mime_type = file_triples.iter()
                    .find(|t| t.predicate == "foundation:hasFileType")
                    .and_then(|t| t.object.as_iri())
                    .and_then(|ft_iri| file_type_triples_map.get(ft_iri))
                    .and_then(|ft_triples| super::chat_storage::triple_str(ft_triples, "foundation:mimeType"))
                    .unwrap_or_else(|| "application/octet-stream".to_string());

                super::log_backend("debug", &format!("[CHAT] Message {} FileRef resolved", msg_iri));
                Some(serde_json::json!({
                    "fileName": file_name,
                    "filePath": file_path,
                    "fileSize": file_size,
                    "mimeType": mime_type,
                }))
            }).collect()
        };

        // --- walk messages and assemble display units ---

        // Pre-build map: tool_use_id → (content, is_error, duration_ms)
        let mut all_tool_results: std::collections::HashMap<String, (String, bool, Option<u64>)> = std::collections::HashMap::new();
        for iri in &message_iris {
            for block in get_blocks_for(iri) {
                if let ContentBlock::ToolResult { tool_use_id, content, is_error, duration_ms } = block {
                    all_tool_results.insert(tool_use_id, (content, is_error.unwrap_or(false), duration_ms));
                }
            }
        }

        let mut units: Vec<serde_json::Value> = Vec::new();

        for iri in &message_iris {
            let msg_triples = match msg_triples_map.get(iri) {
                Some(t) => t,
                None => continue,
            };

            let role = super::chat_storage::triple_str(msg_triples, "foundation:role").unwrap_or_default();
            let blocks = get_blocks_for(iri);
            let timestamp = msg_triples.iter()
                .find(|t| t.predicate == "foundation:sentAt")
                .and_then(|t| if let Object::DateTime(rfc) = &t.object {
                    chrono::DateTime::parse_from_rfc3339(rfc).ok().map(|dt| dt.timestamp_millis())
                } else { None })
                .unwrap_or(0);

            if blocks.is_empty() {
                continue;
            }

            let has_text = blocks.iter().any(|b| matches!(b, ContentBlock::Text { .. }));
            let has_question = blocks.iter().any(|b| match b {
                ContentBlock::QuestionOutput { .. } => true,
                ContentBlock::ToolUse { name, .. } => name == "ask_question",
                _ => false,
            });
            let has_tool_use = blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name != "ask_question"));
            let has_thinking = blocks.iter().any(|b| matches!(b, ContentBlock::Thinking { .. }));
            let only_tool_results = blocks.iter().all(|b| matches!(b, ContentBlock::ToolResult { .. }));

            match role.as_str() {
                "user" => {
                    if only_tool_results {
                        // Pure tool_result message — skip display; results already indexed in all_tool_results
                        continue;
                    }
                    if !has_text {
                        continue;
                    }

                    let text = blocks.iter().filter_map(|b| {
                        if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
                    }).collect::<Vec<_>>().join(" ");

                    let attachments = resolve_attachments(iri, &blocks);
                    if !attachments.is_empty() {
                        super::log_backend("debug", &format!("[CHAT] Message {} has {} attachment(s)", iri, attachments.len()));
                    }

                    let subconscious_entities: Vec<subconscious::SubconsciousEntity> =
                        super::chat_storage::triple_str(msg_triples, "foundation:subconsciousContext")
                            .and_then(|v| serde_json::from_str(&v).ok())
                            .unwrap_or_default();

                    units.push(serde_json::json!({
                        "type": "user",
                        "iri": iri,
                        "text": text,
                        "attachments": attachments,
                        "timestamp": timestamp,
                        "subconscious_entities": subconscious_entities,
                    }));
                }

                "assistant" => {
                    if has_question {
                        let question_block = blocks.iter().find_map(|b| match b {
                            ContentBlock::ToolUse { id, name, input, .. } if name == "ask_question" => {
                                let question = input.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let qtype = input.get("type").and_then(|v| v.as_str()).unwrap_or("text").to_string();
                                let options: Vec<String> = input.get("options")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                                    .unwrap_or_default();
                                Some((id.clone(), question, qtype, options))
                            }
                            ContentBlock::QuestionOutput { id, question, question_type, options } => {
                                Some((id.clone(), question.clone(), question_type.clone(), options.clone()))
                            }
                            _ => None,
                        });

                        if let Some((q_id, question, question_type, options)) = question_block {
                            let answer: Option<String> = all_tool_results.get(&q_id).map(|(content, _, _)| content.clone());
                            units.push(serde_json::json!({
                                "type": "question",
                                "iri": iri,
                                "id": q_id,
                                "question": question,
                                "question_type": question_type,
                                "options": options,
                                "answer": answer,
                                "timestamp": timestamp,
                            }));
                        }

                    } else if has_tool_use {
                        let tool_calls: Vec<serde_json::Value> = blocks.iter().filter_map(|b| {
                            let ContentBlock::ToolUse { id, name, input, reason } = b else { return None; };
                            if name == "ask_question" { return None; }
                            let (result, is_error, duration_ms) = all_tool_results.get(id)
                                .map(|(c, e, d)| (Some(c.clone()), *e, *d))
                                .unwrap_or((None, false, None));
                            Some(serde_json::json!({
                                "name": name,
                                "input": input,
                                "result": result,
                                "is_error": is_error,
                                "reason": reason,
                                "duration_ms": duration_ms,
                            }))
                        }).collect();

                        let reasoning: Option<String> = {
                            let parts: Vec<String> = blocks.iter().filter_map(|b| match b {
                                ContentBlock::Thinking { thinking, .. } => Some(thinking.clone()),
                                _ => None,
                            }).collect();
                            if parts.is_empty() { None } else { Some(parts.join("\n\n")) }
                        };

                        let text: Option<String> = {
                            let parts: Vec<&str> = blocks.iter().filter_map(|b| {
                                if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
                            }).collect();
                            if parts.is_empty() { None } else { Some(parts.join("\n\n")) }
                        };

                        if !tool_calls.is_empty() {
                            units.push(serde_json::json!({
                                "type": "tool_use",
                                "iri": iri,
                                "tool_calls": tool_calls,
                                "text": text,
                                "reasoning": reasoning,
                                "timestamp": timestamp,
                            }));
                        }

                    } else if has_text || has_thinking {
                        let text = blocks.iter().filter_map(|b| {
                            if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
                        }).collect::<Vec<_>>().join("\n\n");

                        let reasoning: Option<String> = {
                            let parts: Vec<String> = blocks.iter().filter_map(|b| {
                                if let ContentBlock::Thinking { thinking, .. } = b { Some(thinking.clone()) } else { None }
                            }).collect();
                            if parts.is_empty() { None } else { Some(parts.join("\n\n")) }
                        };

                        let input_tokens = super::chat_storage::triple_int(msg_triples, "foundation:inputTokens");
                        let output_tokens = super::chat_storage::triple_int(msg_triples, "foundation:outputTokens");
                        let estimated_cost = msg_triples.iter()
                            .find(|t| t.predicate == "foundation:estimatedCost")
                            .and_then(|t| match &t.object {
                                Object::Number(n) => Some(*n),
                                Object::Literal { value, .. } => value.parse::<f64>().ok(),
                                _ => None,
                            });

                        if !text.is_empty() || reasoning.is_some() {
                            units.push(serde_json::json!({
                                "type": "text",
                                "iri": iri,
                                "text": text,
                                "reasoning": reasoning,
                                "timestamp": timestamp,
                                "input_tokens": input_tokens,
                                "output_tokens": output_tokens,
                                "estimated_cost": estimated_cost,
                            }));
                        }
                    }
                }

                "compaction" => {
                    let text = blocks.iter().find_map(|b| {
                        if let ContentBlock::CompactionSummary { text } = b { Some(text.clone()) } else { None }
                    }).unwrap_or_default();
                    if !text.is_empty() {
                        units.push(serde_json::json!({
                            "type": "compaction",
                            "iri": iri,
                            "text": text,
                            "timestamp": timestamp,
                        }));
                    }
                }

                _ => {}
            }
        }

        // Return the last `limit` units.
        let total = units.len();
        let result = if total > limit {
            units.split_off(total - limit - 1)
        } else {
            units
        };

        Ok(serde_json::json!({
            "messages": result,
        }))
    }).await?;

    // is_processing comes from in-memory state, not DB inference.
    // DB-based heuristics (e.g. "last message is user text") incorrectly return true after
    // app restart when nothing is actually running, blocking retry and edit buttons.
    let result = display_units.as_object().map(|o| {
        let mut m = o.clone();
        m.insert("is_processing".to_string(), serde_json::Value::Bool(actually_processing));
        serde_json::Value::Object(m)
    }).unwrap_or(display_units);

    Ok(result)
}

fn recompute_subconscious_for_message(conn: &mut rusqlite::Connection, message_iri: &str) {
    let blocks = super::chat_storage::load_content_blocks(conn, message_iri).unwrap_or_default();
    let text: String = blocks.iter()
        .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None })
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() { return; }

    let entities = subconscious::run_subconscious(&text, Some(message_iri), conn);
    if entities.is_empty() { return; }

    let json = serde_json::to_string(&entities).unwrap_or_else(|_| "[]".to_string());
    let msg = Individual::new(message_iri);
    msg.add_property(conn, "foundation:subconsciousContext", vec![Object::Literal {
        value: json,
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "chat").ok();
}

fn find_last_user_message(conn: &rusqlite::Connection, conversation_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT t_msg.subject FROM triples t_msg
         JOIN triples t_role ON t_role.subject = t_msg.subject
             AND t_role.predicate = 'foundation:role'
             AND t_role.object_value = 'user'
             AND t_role.retracted = 0
         JOIN triples t_ts ON t_ts.subject = t_msg.subject
             AND t_ts.predicate = 'foundation:sentAt'
             AND t_ts.retracted = 0
         WHERE t_msg.predicate = 'foundation:partOfConversation'
             AND t_msg.object = ?1
             AND t_msg.retracted = 0
         ORDER BY t_ts.object_datetime DESC
         LIMIT 1",
        [conversation_id],
        |row| row.get(0),
    ).ok()
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

    app.emit("ai-status", serde_json::json!({
        "status": "Editando mensagem...", "phase": "prep", "conversationId": conversation_id
    })).ok();

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

        let ind = Individual::get(conn, &iri_clone)
            .map_err(|e| format!("Failed to reload message: {}", e))?
            .ok_or_else(|| format!("Message {} not found after delete", iri_clone))?;

        let old_block_iris: Vec<String> = ind.properties.iter()
            .filter(|(k, _)| k == "foundation:hasContentBlock")
            .filter_map(|(_, v)| if let Object::Iri(iri) = v { Some(iri.clone()) } else { None })
            .collect();

        for block_iri in &old_block_iris {
            let _ = Individual::remove_property_value(conn, &iri_clone, "foundation:hasContentBlock", block_iri, "chat");
        }

        let new_blocks = vec![ContentBlock::Text { text: new_content.clone() }];
        let new_ts = chrono::Utc::now().timestamp_millis();
        super::chat_storage::save_content_blocks(conn, &iri_clone, new_ts, &new_blocks)?;

        recompute_subconscious_for_message(conn, &iri_clone);

        Ok(String::new())
    }).await?;

    app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();

    let app_clone = app.clone();
    let executor_clone = executor.inner().clone();
    let mut response_messages = Vec::new();
    run_conversation_from_current_state(app_clone, executor_clone, conversation_id.clone(), &cancellation, false).await?;

    let max_tokens = get_max_input_tokens(&executor).await?;
    let (history, _) = load_conversation_history(&executor, &conversation_id, max_tokens).await?;
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

    app.emit("ai-status", serde_json::json!({
        "status": "Preparando retry...", "phase": "prep", "conversationId": conversation_id
    })).ok();

    let message_iri_for_sc = message_iri.clone();
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

        let target_iri = if is_user_message {
            Some(message_iri_for_sc)
        } else {
            find_last_user_message(conn, &conv_id_for_write)
        };
        if let Some(iri) = target_iri {
            recompute_subconscious_for_message(conn, &iri);
        }

        Ok(String::new())
    }).await?;

    app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();

    let app_clone = app.clone();
    let executor_clone = executor.inner().clone();
    run_conversation_from_current_state(app_clone, executor_clone, conversation_id.clone(), &cancellation, false).await?;

    let max_tokens = get_max_input_tokens(&executor).await?;
    let (history, _) = load_conversation_history(&executor, &conversation_id, max_tokens).await?;
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

    let (history, _) = load_conversation_history(&executor, &conv_id, max_tokens).await?;

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
        .any(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name != "ask_question"));

    let has_truncated_tool = has_tool_use && last_msg.content.iter().any(|b| matches!(b,
        ContentBlock::ToolUse { input, name, .. }
        if name != "ask_question" && input.as_object().map_or(false, |o| o.is_empty())
    ));
    let last_msg_timestamp = last_msg.timestamp;

    super::log_backend("info", &format!(
        "[RECOVERY] Last message: role={}, has_tool_use={}, has_truncated_tool={}, content_blocks={}",
        last_msg.role, has_tool_use, has_truncated_tool, last_msg.content.len()
    ));

    let has_tool_results = last_msg.role == "user"
        && last_msg.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }));

    let all_delivered = has_tool_results
        && last_msg.content.iter().all(|b| matches!(
            b,
            ContentBlock::ToolResult { content, is_error, .. }
            if (content == "Delivered." || content == "[Question dismissed by user]")
                && !matches!(is_error, Some(true))
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
        if has_truncated_tool {
            super::log_backend("warn", &format!(
                "[RECOVERY] Tool call truncado detectado em {} — excluindo mensagem incompleta",
                conv_id
            ));
            let conv_id_del = conv_id.clone();
            executor.write(move |conn| {
                delete_messages_from_timestamp(conn, &conv_id_del, last_msg_timestamp, false)?;
                Ok(String::new())
            }).await?;
            app.emit("chat-message-added", serde_json::json!({"conversationId": conv_id})).ok();
        } else {
            let (tool_result_msg_iri, _, _) = execute_tools_from_message(
                &executor,
                &app,
                &conv_id,
                last_msg,
            ).await?;
            super::log_backend(
                "info",
                &format!("[RECOVERY] Executed tools, created message: {}", tool_result_msg_iri),
            );
            app.emit("chat-message-added", serde_json::json!({"conversationId": conv_id})).ok();
        }
    }

    run_conversation_from_current_state(app.clone(), executor.inner().clone(), conv_id.clone(), &cancellation, true).await?;

    // Always clear the ai-status chip after recovery regardless of silent mode — the chip
    // may be stuck showing a stale status from the previous interrupted run.
    use tauri::Emitter;
    app.emit("ai-status", serde_json::json!({ "status": null, "conversationId": conv_id })).ok();

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
        tool_use_id: tool_use_id.clone(),
        content: "[Question dismissed by user]".to_string(),
        is_error: None,
        duration_ms: None,
    }];
    let result_json = serde_json::to_string(&result_blocks)
        .map_err(|e| format!("Failed to serialize dismiss result: {}", e))?;

    super::chat_storage::create_user_message_raw(&executor, &conversation_id, &result_json).await?;
    app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();

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
        duration_ms: None,
    }];
    let result_json = serde_json::to_string(&result_blocks)
        .map_err(|e| format!("Failed to serialize answer: {}", e))?;

    super::chat_storage::create_user_message_raw(&executor, &conversation_id, &result_json).await?;
    app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();

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

/// Fetch the display-unit for a single message by IRI for targeted reactive updates.
/// Returns null when the message no longer has displayable content (e.g. pure tool_result).
#[tauri::command]
pub async fn chat__get_message_by_iri(
    message_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<Option<serde_json::Value>, String> {
    executor.read(move |conn| {
        let message_iris = vec![message_iri.clone()];

        let msg_triples_map = Individual::batch_load_triples(conn, &message_iris)
            .map_err(|e| format!("Failed to batch-load message triples: {}", e))?;

        let msg_triples = match msg_triples_map.get(&message_iri) {
            Some(t) => t,
            None => return Ok(None),
        };

        let all_block_iris: Vec<String> =
            super::chat_storage::triple_iris(msg_triples, "foundation:hasContentBlock");

        let block_triples_map = Individual::batch_load_triples(conn, &all_block_iris)
            .map_err(|e| format!("Failed to batch-load block triples: {}", e))?;

        let tool_use_iris: Vec<String> = block_triples_map.values()
            .flat_map(|triples| {
                triples.iter()
                    .filter(|t| t.predicate == "anthropic:resultOf")
                    .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let tool_use_triples_map = Individual::batch_load_triples(conn, &tool_use_iris)
            .map_err(|e| format!("Failed to batch-load tool-use block triples: {}", e))?;

        let file_iris: Vec<String> = block_triples_map.values()
            .flat_map(|triples| {
                triples.iter()
                    .filter(|t| t.predicate == "rdf:type" && t.object.as_iri() == Some("foundation:FileRefBlock"))
                    .flat_map(|_| triples.iter()
                        .filter(|t| t.predicate == "foundation:fileRef")
                        .filter_map(|t| t.object.as_iri().map(|s| s.to_string())))
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let file_triples_map = Individual::batch_load_triples(conn, &file_iris)
            .map_err(|e| format!("Failed to batch-load file triples: {}", e))?;

        let file_type_iris: Vec<String> = file_triples_map.values()
            .flat_map(|triples| {
                triples.iter()
                    .filter(|t| t.predicate == "foundation:hasFileType")
                    .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let file_type_triples_map = Individual::batch_load_triples(conn, &file_type_iris)
            .map_err(|e| format!("Failed to batch-load file type triples: {}", e))?;

        let block_iris_ordered = super::chat_storage::triple_iris(msg_triples, "foundation:hasContentBlock");
        let mut indexed_blocks: Vec<(i64, ContentBlock)> = block_iris_ordered.iter()
            .filter_map(|block_iri| {
                block_triples_map.get(block_iri)
                    .and_then(|bt| super::chat_storage::parse_content_block_from_batch(bt, &tool_use_triples_map))
            })
            .collect();
        indexed_blocks.sort_by_key(|(idx, _)| *idx);
        let blocks: Vec<ContentBlock> = indexed_blocks.into_iter().map(|(_, b)| b).collect();

        if blocks.is_empty() {
            return Ok(None);
        }

        let role = super::chat_storage::triple_str(msg_triples, "foundation:role").unwrap_or_default();

        let timestamp = msg_triples.iter()
            .find(|t| t.predicate == "foundation:sentAt")
            .and_then(|t| if let Object::DateTime(rfc) = &t.object {
                chrono::DateTime::parse_from_rfc3339(rfc).ok().map(|dt| dt.timestamp_millis())
            } else { None })
            .unwrap_or(0);

        let resolve_attachments = |blocks: &[ContentBlock]| -> Vec<serde_json::Value> {
            blocks.iter().filter_map(|block| {
                let ContentBlock::FileRef { file_iri, file_name, .. } = block else { return None; };
                let file_triples = file_triples_map.get(file_iri.as_str())?;
                let file_path = super::chat_storage::triple_str(file_triples, "foundation:filePath")
                    .map(|p| crate::paths::resolve_path(&p).to_string_lossy().into_owned())?;
                let file_size = super::chat_storage::triple_int(file_triples, "foundation:fileSize").unwrap_or(0);
                let mime_type = file_triples.iter()
                    .find(|t| t.predicate == "foundation:hasFileType")
                    .and_then(|t| t.object.as_iri())
                    .and_then(|ft_iri| file_type_triples_map.get(ft_iri))
                    .and_then(|ft_triples| super::chat_storage::triple_str(ft_triples, "foundation:mimeType"))
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                Some(serde_json::json!({
                    "fileName": file_name,
                    "filePath": file_path,
                    "fileSize": file_size,
                    "mimeType": mime_type,
                }))
            }).collect()
        };

        let unit = match role.as_str() {
            "user" => {
                let only_tool_results = blocks.iter().all(|b| matches!(b, ContentBlock::ToolResult { .. }));
                if only_tool_results { return Ok(None); }
                let has_text = blocks.iter().any(|b| matches!(b, ContentBlock::Text { .. }));
                if !has_text { return Ok(None); }
                let text = blocks.iter().filter_map(|b| {
                    if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
                }).collect::<Vec<_>>().join(" ");
                let attachments = resolve_attachments(&blocks);
                let subconscious_entities: Vec<subconscious::SubconsciousEntity> =
                    super::chat_storage::triple_str(msg_triples, "foundation:subconsciousContext")
                        .and_then(|v| serde_json::from_str(&v).ok())
                        .unwrap_or_default();
                Some(serde_json::json!({
                    "type": "user",
                    "iri": message_iri,
                    "text": text,
                    "attachments": attachments,
                    "timestamp": timestamp,
                    "subconscious_entities": subconscious_entities,
                }))
            }
            "assistant" => {
                let has_question = blocks.iter().any(|b| match b {
                    ContentBlock::QuestionOutput { .. } => true,
                    ContentBlock::ToolUse { name, .. } => name == "ask_question",
                    _ => false,
                });
                let has_tool_use = blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name != "ask_question"));
                let has_text = blocks.iter().any(|b| matches!(b, ContentBlock::Text { .. }));
                let has_thinking = blocks.iter().any(|b| matches!(b, ContentBlock::Thinking { .. }));

                if has_question {
                    None // question blocks need full context for answer resolution
                } else if has_tool_use {
                    let tool_calls: Vec<serde_json::Value> = blocks.iter().filter_map(|b| {
                        let ContentBlock::ToolUse { id: _, name, input, reason } = b else { return None; };
                        if name == "ask_question" { return None; }
                        // Without the full tool_results map we cannot resolve results here;
                        // the frontend will reload the full set when a question is involved.
                        Some(serde_json::json!({
                            "name": name,
                            "input": input,
                            "result": serde_json::Value::Null,
                            "is_error": false,
                            "reason": reason,
                            "duration_ms": serde_json::Value::Null,
                        }))
                    }).collect();
                    let reasoning = {
                        let parts: Vec<String> = blocks.iter().filter_map(|b| {
                            if let ContentBlock::Thinking { thinking, .. } = b { Some(thinking.clone()) } else { None }
                        }).collect();
                        if parts.is_empty() { None } else { Some(parts.join("\n\n")) }
                    };
                    let text = {
                        let parts: Vec<&str> = blocks.iter().filter_map(|b| {
                            if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
                        }).collect();
                        if parts.is_empty() { None } else { Some(parts.join("\n\n")) }
                    };
                    if tool_calls.is_empty() { None } else {
                        Some(serde_json::json!({
                            "type": "tool_use",
                            "iri": message_iri,
                            "tool_calls": tool_calls,
                            "text": text,
                            "reasoning": reasoning,
                            "timestamp": timestamp,
                        }))
                    }
                } else if has_text || has_thinking {
                    let text = blocks.iter().filter_map(|b| {
                        if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
                    }).collect::<Vec<_>>().join("\n\n");
                    let reasoning = {
                        let parts: Vec<String> = blocks.iter().filter_map(|b| {
                            if let ContentBlock::Thinking { thinking, .. } = b { Some(thinking.clone()) } else { None }
                        }).collect();
                        if parts.is_empty() { None } else { Some(parts.join("\n\n")) }
                    };
                    let input_tokens = super::chat_storage::triple_int(msg_triples, "foundation:inputTokens");
                    let output_tokens = super::chat_storage::triple_int(msg_triples, "foundation:outputTokens");
                    let estimated_cost = msg_triples.iter()
                        .find(|t| t.predicate == "foundation:estimatedCost")
                        .and_then(|t| match &t.object {
                            Object::Number(n) => Some(*n),
                            Object::Literal { value, .. } => value.parse::<f64>().ok(),
                            _ => None,
                        });
                    if !text.is_empty() || reasoning.is_some() {
                        Some(serde_json::json!({
                            "type": "text",
                            "iri": message_iri,
                            "text": text,
                            "reasoning": reasoning,
                            "timestamp": timestamp,
                            "input_tokens": input_tokens,
                            "output_tokens": output_tokens,
                            "estimated_cost": estimated_cost,
                        }))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "compaction" => {
                let text = blocks.iter().find_map(|b| {
                    if let ContentBlock::CompactionSummary { text } = b { Some(text.clone()) } else { None }
                }).unwrap_or_default();
                if text.is_empty() { None } else {
                    Some(serde_json::json!({
                        "type": "compaction",
                        "iri": message_iri,
                        "text": text,
                        "timestamp": timestamp,
                    }))
                }
            }
            _ => None,
        };

        Ok(unit)
    }).await
}

/// Returns the max(tx) across all triples for a given conversation's messages,
/// giving the frontend a cursor for lossless replay on subscription.
#[tauri::command]
pub async fn chat__get_conversation_snapshot_tx(
    conversation_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<i64, String> {
    executor.read(move |conn| {
        let tx: i64 = conn.query_row(
            "SELECT COALESCE(MAX(t.tx), 0)
             FROM triples t
             WHERE t.subject IN (
                 SELECT subject FROM triples
                 WHERE predicate = 'foundation:partOfConversation'
                   AND object = ?1
                   AND retracted = 0
                   AND tx = (SELECT MAX(tx) FROM triples t2
                              WHERE t2.subject = triples.subject
                                AND t2.predicate = triples.predicate)
             )",
            rusqlite::params![conversation_id],
            |row| row.get(0),
        ).map_err(|e| e.to_string())?;
        Ok(tx)
    }).await
}
