use crate::owl::DbExecutor;
use crate::owl::{Individual, Object, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Image { source: ImageSource },
    Document { source: DocumentSource },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

/// A message in an AI conversation
#[derive(Debug, Clone, Serialize)]
pub struct AIConversationMessage {
    pub iri: String,
    pub role: String,
    pub content: Vec<ContentBlock>,
    pub timestamp: i64,
    pub token_count: Option<usize>,
    pub model: Option<String>,
    pub stop_reason: Option<String>,
    pub input_tokens: Option<usize>,
    pub output_tokens: Option<usize>,
}

/// Create a user message with text content
pub async fn create_user_message(
    executor: &DbExecutor,
    conversation_id: &str,
    text: &str,
) -> Result<String, String> {
    let content_blocks = vec![ContentBlock::Text { text: text.to_string() }];

    let content_json = serde_json::to_string(&content_blocks)
        .map_err(|e| format!("Failed to serialize content: {}", e))?;

    create_message(executor, conversation_id, "user", &content_json, None, None, None).await
}

/// Create an assistant message from API response
pub async fn create_assistant_message(
    executor: &DbExecutor,
    conversation_id: &str,
    content_json: &str,
    model: &str,
    stop_reason: &str,
    input_tokens: usize,
    output_tokens: usize,
) -> Result<String, String> {
    create_message(
        executor,
        conversation_id,
        "assistant",
        content_json,
        Some(model),
        Some(stop_reason),
        Some((input_tokens, output_tokens)),
    ).await
}

/// Internal: Create a message entity in the database
pub(super) async fn create_message(
    executor: &DbExecutor,
    conversation_id: &str,
    role: &str,
    content_json: &str,
    model: Option<&str>,
    stop_reason: Option<&str>,
    tokens: Option<(usize, usize)>,
) -> Result<String, String> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let message_iri = format!("foundation:AIConversationMessage_{}", timestamp);

    let token_count = calculate_content_tokens(content_json)?;

    let msg_iri_clone = message_iri.clone();
    let conversation_iri = conversation_id.to_string();
    let role_str = role.to_string();
    let content_str = content_json.to_string();
    let model_opt = model.map(|s| s.to_string());
    let stop_reason_opt = stop_reason.map(|s| s.to_string());

    executor.write(move |conn| {
        let msg = Individual::new(&msg_iri_clone);

        msg.assert(
            conn,
            "foundation:AIConversationMessage",
            &format!("{} message", role_str),
            "chat",
            "ai"
        ).map_err(|e| format!("Failed to create message: {}", e))?;

        msg.add_property(conn, "foundation:role", Object::Literal {
            value: role_str.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "ai").map_err(|e| format!("Failed to set role: {}", e))?;

        msg.add_property(conn, "foundation:content", Object::Literal {
            value: content_str,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "ai").map_err(|e| format!("Failed to set content: {}", e))?;

        msg.add_property(conn, "foundation:sentAt", Object::DateTime(timestamp), "ai")
            .map_err(|e| format!("add_property failed: {}", e))?;

        msg.add_property(conn, "foundation:partOfConversation", Object::Iri(conversation_iri), "ai")
            .map_err(|e| format!("Property error: {}", e))?;

        msg.add_property(conn, "foundation:tokenCount", Object::Integer(token_count as i64), "ai")
            .map_err(|e| format!("Property error: {}", e))?;

        if role_str == "user" {
            msg.add_property(conn, "foundation:sender",
                Object::Iri("foundation:ThisUser".to_string()), "ai")
                .map_err(|e| format!("Property error: {}", e))?;
            msg.add_property(conn, "foundation:receiver",
                Object::Iri("foundation:LocalAIAssistant".to_string()), "ai")
                .map_err(|e| format!("Property error: {}", e))?;
        } else {
            msg.add_property(conn, "foundation:sender",
                Object::Iri("foundation:LocalAIAssistant".to_string()), "ai")
                .map_err(|e| format!("Property error: {}", e))?;
            msg.add_property(conn, "foundation:receiver",
                Object::Iri("foundation:ThisUser".to_string()), "ai")
                .map_err(|e| format!("Property error: {}", e))?;
        }

        if let Some(model_str) = model_opt {
            msg.add_property(conn, "foundation:model", Object::Literal {
                value: model_str,
                datatype: Some("xsd:string".to_string()),
                language: None,
            }, "ai").map_err(|e| format!("Failed to set model: {}", e))?;
        }

        if let Some(stop_str) = stop_reason_opt {
            msg.add_property(conn, "foundation:stopReason", Object::Literal {
                value: stop_str,
                datatype: Some("xsd:string".to_string()),
                language: None,
            }, "ai").map_err(|e| format!("Failed to set stopReason: {}", e))?;
        }

        if let Some((input, output)) = tokens {
            msg.add_property(conn, "foundation:inputTokens", Object::Integer(input as i64), "ai")
                .map_err(|e| format!("Property error: {}", e))?;
            msg.add_property(conn, "foundation:outputTokens", Object::Integer(output as i64), "ai")
                .map_err(|e| format!("Property error: {}", e))?;
        }

        Ok(msg_iri_clone)
    }).await
}

/// Load conversation history with token budget
pub async fn load_conversation_history(
    executor: &DbExecutor,
    conversation_id: &str,
    max_tokens: usize,
) -> Result<Vec<AIConversationMessage>, String> {
    super::log_backend("info", &format!(
        "[CHAT] Loading conversation history for: {}", conversation_id
    ));

    let conversation_id = conversation_id.to_string();
    let messages = executor.read(move |conn| {
        // Query all messages in conversation using OWL layer
        let message_iris = Individual::find_by_class_and_properties(
            conn,
            "foundation:AIConversationMessage",
            &[("foundation:partOfConversation", &conversation_id)],
        ).map_err(|e| format!("Failed to query messages: {}", e))?;

        // Load each message
        let mut messages = Vec::new();
        let mut failed_count = 0;
        for iri in message_iris {
            match load_message(conn, &iri) {
                Ok(msg) => {
                    messages.push(msg);
                },
                Err(_e) => {
                    failed_count += 1;
                }
            }
        }

        Ok::<(Vec<AIConversationMessage>, usize), String>((messages, failed_count))
    }).await?;

    let (mut messages, failed_count) = messages;

    super::log_backend("info", &format!(
        "[CHAT] Loaded {} messages, {} failed", messages.len(), failed_count
    ));

    // Sort by timestamp
    messages.sort_by_key(|m| m.timestamp);

    // Apply token budget, ensuring tool_use/tool_result pairs stay together
    let mut selected = Vec::new();
    let mut total_tokens = 0;
    let mut i = messages.len();

    while i > 0 {
        i -= 1;
        let msg = messages[i].clone();
        let msg_tokens = msg.token_count.unwrap_or(0);

        // Check if this message has tool_result blocks
        let has_tool_results = msg.content.iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { .. }));

        if has_tool_results && i > 0 {
            // Must include the previous message (assistant with tool_use)
            let prev_msg = messages[i - 1].clone();
            let prev_tokens = prev_msg.token_count.unwrap_or(0);
            let pair_tokens = msg_tokens + prev_tokens;

            // Check if pair fits in budget
            if total_tokens + pair_tokens > max_tokens {
                break; // Can't fit the pair, stop here
            }

            // Add msg first, then prev_msg so that after selected.reverse()
            // they appear in correct chronological order: prev_msg (older) before msg (newer)
            selected.push(msg);
            selected.push(prev_msg);
            total_tokens += pair_tokens;
            i -= 1; // Skip the previous message in next iteration
        } else {
            // Single message (no tool results)
            if total_tokens + msg_tokens > max_tokens {
                break;
            }
            selected.push(msg);
            total_tokens += msg_tokens;
        }
    }

    // Reverse to chronological order
    selected.reverse();

    // Claude requires tool_result to have its corresponding tool_use in the previous message
    let selected_count = selected.len();
    let mut cleaned = Vec::new();
    let mut prev_tool_use_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for msg in selected {
        let mut clean_content = Vec::new();

        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { id, .. } => {
                    prev_tool_use_ids.insert(id.clone());
                    clean_content.push(block.clone());
                },
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    if prev_tool_use_ids.contains(tool_use_id) {
                        clean_content.push(block.clone());
                        prev_tool_use_ids.remove(tool_use_id);
                    } else {
                        super::log_backend("warn", &format!(
                            "[CHAT] Removing orphaned tool_result: {}", tool_use_id
                        ));
                    }
                },
                _ => {
                    clean_content.push(block.clone());
                }
            }
        }

        // (This can happen if a user message only had orphaned tool_results)
        if !clean_content.is_empty() {
            let mut cleaned_msg = msg;
            cleaned_msg.content = clean_content;
            cleaned.push(cleaned_msg);
        } else {
            super::log_backend("warn", &format!(
                "[CHAT] Removing empty message after cleanup: {}", msg.iri
            ));
        }
    }

    // Remove orphaned tool_use blocks — happens when the token budget cut off the user's response
    let mut final_cleaned = Vec::new();
    for msg in cleaned {
        let has_orphaned_tool_use = msg.content.iter().any(|b| {
            if let ContentBlock::ToolUse { id, .. } = b {
                prev_tool_use_ids.contains(id)
            } else {
                false
            }
        });

        if has_orphaned_tool_use {
            let clean_content: Vec<_> = msg.content.into_iter()
                .filter_map(|block| {
                    match &block {
                        ContentBlock::ToolUse { id, .. } => {
                            if prev_tool_use_ids.contains(id) {
                                super::log_backend("warn", &format!(
                                    "[CHAT] Removing orphaned tool_use: {}", id
                                ));
                                None
                            } else {
                                Some(block)
                            }
                        },
                        _ => Some(block),
                    }
                })
                .collect();

            if !clean_content.is_empty() {
                final_cleaned.push(AIConversationMessage {
                    iri: msg.iri,
                    role: msg.role,
                    content: clean_content,
                    timestamp: msg.timestamp,
                    token_count: msg.token_count,
                    model: msg.model,
                    stop_reason: msg.stop_reason,
                    input_tokens: msg.input_tokens,
                    output_tokens: msg.output_tokens,
                });
            }
        } else {
            final_cleaned.push(msg);
        }
    }

    super::log_backend("info", &format!(
        "[CHAT] Selected {} messages within token budget ({}/{}), cleaned {} orphaned blocks",
        final_cleaned.len(), total_tokens, max_tokens,
        selected_count.saturating_sub(final_cleaned.len())
    ));

    Ok(final_cleaned)
}

/// Load a single message from the database
pub(super) fn load_message(conn: &Connection, iri: &str) -> Result<AIConversationMessage, String> {
    let ind = Individual::get(conn, iri)
        .map_err(|e| format!("Failed to load message: {}", e))?;

    // Extract properties
    let role = ind.properties.iter()
        .find(|(k, _)| k == "foundation:role")
        .and_then(|(_, v)| v.as_literal())
        .ok_or("Missing role")?;

    let content_json = ind.properties.iter()
        .find(|(k, _)| k == "foundation:content")
        .and_then(|(_, v)| v.as_literal())
        .ok_or("Missing content")?;

    let timestamp = ind.properties.iter()
        .find(|(k, _)| k == "foundation:sentAt")
        .and_then(|(_, v)| match v {
            Object::DateTime(ts) => Some(*ts),
            Object::Literal { value, .. } => value.parse::<i64>().ok(),
            _ => None
        })
        .ok_or("Missing timestamp")?;

    let token_count = ind.properties.iter()
        .find(|(k, _)| k == "foundation:tokenCount")
        .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as usize) } else { None });

    let model = ind.properties.iter()
        .find(|(k, _)| k == "foundation:model")
        .and_then(|(_, v)| v.as_literal());

    let stop_reason = ind.properties.iter()
        .find(|(k, _)| k == "foundation:stopReason")
        .and_then(|(_, v)| v.as_literal());

    let input_tokens = ind.properties.iter()
        .find(|(k, _)| k == "foundation:inputTokens")
        .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as usize) } else { None });

    let output_tokens = ind.properties.iter()
        .find(|(k, _)| k == "foundation:outputTokens")
        .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as usize) } else { None });

    // Parse content JSON
    let content: Vec<ContentBlock> = serde_json::from_str(&content_json)
        .map_err(|e| format!("Failed to parse content JSON: {}", e))?;

    Ok(AIConversationMessage {
        iri: iri.to_string(),
        role,
        content,
        timestamp,
        token_count,
        model,
        stop_reason,
        input_tokens,
        output_tokens,
    })
}

fn get_tokenizer() -> &'static tiktoken_rs::CoreBPE {
    static TOKENIZER: std::sync::OnceLock<tiktoken_rs::CoreBPE> = std::sync::OnceLock::new();
    TOKENIZER.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("Failed to load tokenizer")
    })
}

fn calculate_content_tokens(content_json: &str) -> Result<usize, String> {
    let bpe = get_tokenizer();

    // Parse content blocks
    let blocks: Vec<ContentBlock> = serde_json::from_str(content_json)
        .map_err(|e| format!("Failed to parse content: {}", e))?;

    let mut total = 4; // Base overhead per message

    for block in blocks {
        total += match block {
            ContentBlock::Text { text } => {
                bpe.encode_with_special_tokens(&text).len()
            },
            ContentBlock::ToolUse { name, input, .. } => {
                let json = serde_json::to_string(&input).unwrap_or_default();
                bpe.encode_with_special_tokens(&name).len() +
                bpe.encode_with_special_tokens(&json).len() + 10
            },
            ContentBlock::ToolResult { content, .. } => {
                bpe.encode_with_special_tokens(&content).len() + 10
            },
            ContentBlock::Image { .. } => 1000, // Rough estimate
            ContentBlock::Document { .. } => 2000, // Rough estimate
        };
    }

    Ok(total)
}
