use crate::commands::chat_storage::{AIConversationMessage, ContentBlock};
use crate::ai::ChatMessage;
use crate::ai::providers::{ContentBlock as ApiContentBlock, MessageContent, ImageSource as ApiImageSource, DocumentSource as ApiDocumentSource};
use super::super::log_backend;

pub fn message_to_api_format(msg: &AIConversationMessage) -> ChatMessage {
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

    ChatMessage {
        role: msg.role.clone(),
        content: MessageContent::ContentBlocks(api_blocks),
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
