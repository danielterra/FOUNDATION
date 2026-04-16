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
                    content: tool_result_content_to_value(content),
                    is_error: *is_error,
                }
            ),
            ContentBlock::Thinking { thinking, signature } => Some(
                ApiContentBlock::Thinking {
                    thinking: thinking.clone(),
                    signature: signature.clone(),
                }
            ),
            ContentBlock::RedactedThinking { data } => Some(
                ApiContentBlock::RedactedThinking {
                    data: data.clone(),
                }
            ),
            ContentBlock::SpeakOutput { text } => Some(
                ApiContentBlock::Text { text: text.clone() }
            ),
            ContentBlock::QuestionOutput { id, question, question_type, options } => Some(
                ApiContentBlock::ToolUse {
                    id: id.clone(),
                    name: "ask_question".to_string(),
                    input: serde_json::json!({
                        "question": question,
                        "type": question_type,
                        "options": options,
                    }),
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

    // Always target the last user message, even if it contains tool_result blocks.
    // After a merge of consecutive user turns (tool_result message + new user text),
    // the resulting message starts with tool_results followed by the current user content.
    // Filtering out such messages would inject into the wrong (earlier) turn.
    let target = messages.iter_mut().rev().find(|msg| msg.role == "user");

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
        } else if mime_type.starts_with("text/") {
            use base64::Engine as _;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_default();
            inject.push(ApiContentBlock::Text {
                text: format!("<file_content mime_type=\"{}\">\n{}\n</file_content>", mime_type, decoded),
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
            // Remove FileRef text placeholders that were created by message_to_api_format —
            // the actual vision/document blocks being injected supersede them.
            if !attachment_binaries.is_empty() {
                blocks.retain(|b| !matches!(b, ApiContentBlock::Text { text } if text.starts_with("[Attached file:")));
            }
            // Insert after any leading tool_result blocks so the API requirement
            // (tool_results must precede other content in a user message) is preserved.
            let insert_pos = blocks.iter()
                .position(|b| !matches!(b, ApiContentBlock::ToolResult { .. }))
                .unwrap_or(blocks.len());
            for (i, block) in inject.into_iter().enumerate() {
                blocks.insert(insert_pos + i, block);
            }
        }
        MessageContent::Text(text) => {
            let mut new_blocks = inject;
            new_blocks.push(ApiContentBlock::Text { text: text.clone() });
            msg.content = MessageContent::ContentBlocks(new_blocks);
        }
    }
}

/// Append subconscious context as a text block to the last user message.
/// Keeps the system prompt fully static (cacheable) while surfacing dynamic memory context.
/// Unlike inject_datetime_context (which prepends), this appends — safe to add after tool_result
/// blocks without violating the API requirement that tool_results come first.
pub fn inject_subconscious_context(messages: &mut Vec<ChatMessage>, context: &str) {
    let target = messages.iter_mut().rev().find(|msg| msg.role == "user");

    let Some(msg) = target else { return };

    let block = ApiContentBlock::Text { text: context.to_string() };
    match &mut msg.content {
        MessageContent::ContentBlocks(ref mut blocks) => blocks.push(block),
        MessageContent::Text(text) => {
            msg.content = MessageContent::ContentBlocks(vec![
                ApiContentBlock::Text { text: text.clone() },
                block,
            ]);
        }
    }
}

/// Prepend current date/time as a text block to the last user message in the list.
/// This keeps the system prompt fully static (cacheable) while still giving Claude
/// temporal context on every request. No-op if the list is empty.
///
/// When the last user message starts with ToolResult blocks (e.g. after inject_speak_results
/// wraps the user's reply as a speak tool result), the datetime is appended instead of
/// prepended — the API requires tool_result blocks to come first in user messages.
pub fn inject_datetime_context(messages: &mut Vec<ChatMessage>) {
    let date_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z").to_string();
    let datetime_block = ApiContentBlock::Text {
        text: format!("Current date/time: {}", date_time),
    };

    let target = messages.iter_mut().rev().find(|msg| msg.role == "user");

    if let Some(msg) = target {
        match &mut msg.content {
            MessageContent::ContentBlocks(ref mut blocks) => {
                let starts_with_tool_result = blocks.first()
                    .map_or(false, |b| matches!(b, ApiContentBlock::ToolResult { .. }));
                if starts_with_tool_result {
                    blocks.push(datetime_block);
                } else {
                    blocks.insert(0, datetime_block);
                }
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

/// Transform user messages that follow an unmatched assistant speak tool_use into ToolResults.
///
/// Speak results are not stored in the DB. Instead, the user's next message IS the result of
/// the speak tool — they are responding to what the assistant said. This function detects
/// unmatched speak tool_uses and wraps the following user message's content (text, images,
/// documents) into a ToolResult, giving the API a properly matched tool_use / tool_result pair.
pub fn inject_speak_results(messages: &mut Vec<ChatMessage>) {
    let mut i = 0;
    while i + 1 < messages.len() {
        if messages[i].role != "assistant" {
            i += 1;
            continue;
        }

        let speak_ids: Vec<String> = if let MessageContent::ContentBlocks(ref blocks) = messages[i].content {
            blocks.iter().filter_map(|b| match b {
                ApiContentBlock::ToolUse { id, name, .. } if name == "speak" => Some(id.clone()),
                _ => None,
            }).collect()
        } else {
            i += 1;
            continue;
        };

        if speak_ids.is_empty() {
            i += 1;
            continue;
        }

        let next = i + 1;
        if messages[next].role != "user" {
            i += 1;
            continue;
        }

        let already_has_result = if let MessageContent::ContentBlocks(ref blocks) = messages[next].content {
            blocks.iter().any(|b| match b {
                ApiContentBlock::ToolResult { tool_use_id, .. } => speak_ids.contains(tool_use_id),
                _ => false,
            })
        } else {
            false
        };

        if already_has_result {
            i += 1;
            continue;
        }

        let speak_id = speak_ids[0].clone();

        let user_blocks = match std::mem::replace(
            &mut messages[next].content,
            MessageContent::ContentBlocks(Vec::new()),
        ) {
            MessageContent::ContentBlocks(blocks) => blocks,
            MessageContent::Text(text) => vec![ApiContentBlock::Text { text }],
        };

        let result_content = blocks_to_speak_result_content(&user_blocks);

        messages[next].content = MessageContent::ContentBlocks(vec![
            ApiContentBlock::ToolResult {
                tool_use_id: speak_id,
                content: result_content,
                is_error: None,
            }
        ]);

        i += 1;
    }
}

fn blocks_to_speak_result_content(blocks: &[ApiContentBlock]) -> serde_json::Value {
    let transferable: Vec<&ApiContentBlock> = blocks.iter().filter(|b| {
        matches!(b, ApiContentBlock::Text { .. } | ApiContentBlock::Image { .. } | ApiContentBlock::Document { .. })
    }).collect();

    if transferable.is_empty() {
        return serde_json::Value::String(String::new());
    }

    if transferable.len() == 1 {
        if let ApiContentBlock::Text { text } = transferable[0] {
            return serde_json::Value::String(text.clone());
        }
    }

    let arr: Vec<serde_json::Value> = transferable.iter().filter_map(|b| match b {
        ApiContentBlock::Text { text } => Some(serde_json::json!({"type": "text", "text": text})),
        ApiContentBlock::Image { source } => Some(serde_json::json!({
            "type": "image",
            "source": {
                "type": source.source_type,
                "media_type": source.media_type,
                "data": source.data
            }
        })),
        ApiContentBlock::Document { source } => Some(serde_json::json!({
            "type": "document",
            "source": {
                "type": source.source_type,
                "media_type": source.media_type,
                "data": source.data
            }
        })),
        _ => None,
    }).collect();

    serde_json::Value::Array(arr)
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
            // Prepend so tool_result blocks come before any text blocks — the Claude API
            // requires tool_result blocks to appear before other content in user messages.
            let mut pos = 0;
            for id in missing {
                blocks.insert(pos, ApiContentBlock::ToolResult {
                    tool_use_id: id,
                    content: serde_json::Value::String("Tool result unavailable (conversation was interrupted)".to_string()),
                    is_error: Some(true),
                });
                pos += 1;
            }
        }
    }

    // Strip trailing tool_use and thinking blocks from the last assistant message.
    // The Claude API requires every tool_use to be followed by a tool_result, and
    // thinking blocks without a subsequent tool_use are invalid at end of history.
    if let Some(last) = messages.last_mut() {
        if last.role == "assistant" {
            if let MessageContent::ContentBlocks(ref mut blocks) = last.content {
                let had_tool_use =
                    blocks.iter().any(|b| matches!(b, ApiContentBlock::ToolUse { .. }));
                if had_tool_use {
                    log_backend(
                        "warn", "[CHAT] Stripping trailing tool_use and thinking blocks (end of history)",
                    );
                    blocks.retain(|b| !matches!(
                        b,
                        ApiContentBlock::ToolUse { .. } | ApiContentBlock::Thinking { .. } | ApiContentBlock::RedactedThinking { .. }
                    ));
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
    thinking_blocks: &[crate::ai::ThinkingBlock],
) -> Result<Vec<ContentBlock>, String> {
    let mut blocks = Vec::new();

    for tb in thinking_blocks {
        match tb {
            crate::ai::ThinkingBlock::Thinking { thinking, signature } => {
                blocks.push(ContentBlock::Thinking {
                    thinking: thinking.clone(),
                    signature: signature.clone(),
                });
            }
            crate::ai::ThinkingBlock::RedactedThinking { data } => {
                blocks.push(ContentBlock::RedactedThinking {
                    data: data.clone(),
                });
            }
        }
    }

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

/// Convert a stored tool-result content string to a `serde_json::Value`.
///
/// Image and PDF results are stored as a JSON array string, e.g.
/// `[{"type":"image","source":{...}}]`. Parse those back into a `Value::Array`
/// so the Claude API receives a proper content-block list (not a quoted string).
/// Every other result is a plain string and is returned as `Value::String`.
fn tool_result_content_to_value(s: &str) -> serde_json::Value {
    if s.starts_with('[') {
        if let Ok(v @ serde_json::Value::Array(_)) = serde_json::from_str(s) {
            return v;
        }
    }
    serde_json::Value::String(s.to_string())
}
