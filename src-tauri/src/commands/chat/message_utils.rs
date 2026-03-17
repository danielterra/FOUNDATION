use crate::commands::chat_storage::{AIConversationMessage, ContentBlock};
use crate::ai::ChatMessage;
use crate::ai::providers::{ContentBlock as ApiContentBlock, MessageContent, ImageSource as ApiImageSource, DocumentSource as ApiDocumentSource};
use crate::owl::{DbExecutor, Individual, Object};
use super::super::log_backend;

pub fn message_to_api_format(msg: &AIConversationMessage) -> ChatMessage {
    let api_blocks: Vec<ApiContentBlock> = msg.content.iter().filter_map(|block| {
        match block {
            ContentBlock::Text { text } => Some(
                ApiContentBlock::Text { text: text.clone() }
            ),
            ContentBlock::Image { source } => Some(
                ApiContentBlock::Image {
                    source: ApiImageSource {
                        source_type: source.source_type.clone(),
                        media_type: source.media_type.clone(),
                        data: source.data.clone(),
                    }
                }
            ),
            ContentBlock::Document { source } => Some(
                ApiContentBlock::Document {
                    source: ApiDocumentSource {
                        source_type: source.source_type.clone(),
                        media_type: source.media_type.clone(),
                        data: source.data.clone(),
                    }
                }
            ),
            ContentBlock::FileRef { file_iri, file_name, .. } => Some(
                ApiContentBlock::Text {
                    text: format!(
                        "[Attached file: {} | Knowledge base IRI: {}]",
                        file_name, file_iri
                    ),
                }
            ),
            ContentBlock::CameraRef { .. } => None,
            ContentBlock::ToolUse { id, name, input } => Some(
                ApiContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                }
            ),
            ContentBlock::ToolResult { tool_use_id, content, is_error } => Some(
                ApiContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: content.clone(),
                    is_error: *is_error,
                }
            ),
        }
    }).collect();

    ChatMessage {
        role: msg.role.clone(),
        content: MessageContent::ContentBlocks(api_blocks),
    }
}

/// Inject camera frames and file attachment binaries into the last user message.
/// Called only on the first loop iteration — subsequent turns receive no binary content.
pub fn inject_attachments_for_current_turn(
    messages: &mut Vec<ChatMessage>,
    camera_frames: Option<&[String]>,
    attachment_binaries: &[(String, String)],
    files_needing_summary: &[(String, String)],
) {
    if camera_frames.map_or(true, |f| f.is_empty())
        && attachment_binaries.is_empty()
        && files_needing_summary.is_empty()
    {
        return;
    }

    let target = messages.iter_mut().rev().find(|msg| {
        if msg.role != "user" { return false; }
        match &msg.content {
            MessageContent::ContentBlocks(blocks) =>
                !blocks.iter().any(|b| matches!(b, ApiContentBlock::ToolResult { .. })),
            MessageContent::Text(_) => true,
        }
    });

    let Some(msg) = target else { return };

    let mut inject: Vec<ApiContentBlock> = Vec::new();

    for (mime_type, data) in attachment_binaries {
        if mime_type.starts_with("image/") {
            inject.push(ApiContentBlock::Image {
                source: ApiImageSource {
                    source_type: "base64".to_string(),
                    media_type: mime_type.clone(),
                    data: data.clone(),
                },
            });
        } else if mime_type == "application/pdf" {
            inject.push(ApiContentBlock::Document {
                source: ApiDocumentSource {
                    source_type: "base64".to_string(),
                    media_type: mime_type.clone(),
                    data: data.clone(),
                },
            });
        }
    }

    if let Some(frames) = camera_frames {
        for frame_data in frames {
            inject.push(ApiContentBlock::Image {
                source: ApiImageSource {
                    source_type: "base64".to_string(),
                    media_type: "image/jpeg".to_string(),
                    data: frame_data.clone(),
                },
            });
        }
    }

    if !files_needing_summary.is_empty() {
        let camera_frames: Vec<&(String, String)> = files_needing_summary.iter()
            .filter(|(_, name)| name.starts_with("camera_frame_"))
            .collect();
        let other_files: Vec<&(String, String)> = files_needing_summary.iter()
            .filter(|(_, name)| !name.starts_with("camera_frame_"))
            .collect();

        let mut parts: Vec<String> = Vec::new();

        if !camera_frames.is_empty() {
            let frame_lines: Vec<String> = camera_frames.iter().enumerate()
                .map(|(i, (iri, name))| {
                    if i == 0 {
                        format!("- {} (IRI: {}) — user's emotional/body state in 1-3 words + one-line environment note", name, iri)
                    } else {
                        format!("- {} (IRI: {}) — user's emotional/body state in 1-3 words; omit environment unless it changed", name, iri)
                    }
                })
                .collect();
            parts.push(format!("Camera frames:\n{}", frame_lines.join("\n")));
        }

        if !other_files.is_empty() {
            let file_lines = other_files.iter()
                .map(|(iri, name)| format!("- {} (IRI: {})", name, iri))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("Other files (concise summary):\n{}", file_lines));
        }

        inject.push(ApiContentBlock::Text {
            text: format!(
                "[System: The following attached files have no AI summary yet:\n{}\nFor each, include a tag in your response: <filesummary file=\"IRI\">summary</filesummary>. Summaries are saved automatically for future conversations.]",
                parts.join("\n")
            ),
        });
    }

    if inject.is_empty() { return; }

    match &mut msg.content {
        MessageContent::ContentBlocks(ref mut blocks) => {
            let mut new_blocks = inject;
            new_blocks.append(blocks);
            *blocks = new_blocks;
        }
        MessageContent::Text(text) => {
            let mut new_blocks = inject;
            new_blocks.push(ApiContentBlock::Text { text: text.clone() });
            msg.content = MessageContent::ContentBlocks(new_blocks);
        }
    }
}

/// Prepend current date/time as a text block to the last user message in the list.
/// This keeps the system prompt fully static (cacheable) while still giving Claude
/// temporal context on every request. No-op if the list is empty.
pub fn inject_datetime_context(messages: &mut Vec<ChatMessage>) {
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
pub fn sanitize_tool_pairs(messages: &mut Vec<ChatMessage>) {
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

        log_backend(
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
                    log_backend(
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

pub fn response_content_to_blocks(
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

/// Parse `<filesummary file="iri">text</filesummary>` tags from assistant response text blocks,
/// save the summaries to the DB, and strip the tags from the stored content.
pub async fn extract_and_save_file_summaries(
    mut blocks: Vec<ContentBlock>,
    executor: &DbExecutor,
) -> Vec<ContentBlock> {
    for block in &mut blocks {
        let ContentBlock::Text { ref mut text } = block else { continue };

        let mut result = String::with_capacity(text.len());
        let mut remaining = text.as_str();

        while let Some(start) = remaining.find("<filesummary ") {
            result.push_str(&remaining[..start]);
            remaining = &remaining[start..];

            let Some(tag_end) = remaining.find('>') else { break };
            let tag = &remaining[..tag_end + 1];

            let file_iri = tag
                .trim_start_matches("<filesummary ")
                .trim_end_matches('>')
                .split_once("file=\"")
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(iri, _)| iri.to_string());

            remaining = &remaining[tag_end + 1..];

            let Some(close) = remaining.find("</filesummary>") else { break };
            let summary = remaining[..close].trim().to_string();
            remaining = &remaining[close + "</filesummary>".len()..];

            if let Some(iri) = file_iri {
                if !summary.is_empty() {
                    let iri_clone = iri.clone();
                    let summary_clone = summary.clone();
                    let saved = executor.write(move |conn| {
                        let ind = Individual::new(&iri_clone);
                        ind.add_property(conn, "foundation:aiSummary", vec![Object::Literal {
                            value: summary_clone,
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        }], "ai").map_err(|e| e.to_string())?;
                        Ok(String::new())
                    }).await;
                    match saved {
                        Ok(_) => log_backend("info", &format!("[CHAT] Saved aiSummary for {}", iri)),
                        Err(e) => log_backend("warn", &format!("[CHAT] Failed to save aiSummary for {}: {}", iri, e)),
                    }
                }
            }
        }

        result.push_str(remaining);
        *text = result.trim().to_string();
    }

    // Remove any text blocks that became empty after stripping
    blocks.retain(|b| !matches!(b, ContentBlock::Text { text } if text.is_empty()));
    blocks
}
