use crate::owl::DbExecutor;
use crate::owl::{Individual, Object, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Image { source: ImageSource },
    CameraRef { file_path: String, token_estimate: usize },
    Document { source: DocumentSource },
    FileRef { file_iri: String, file_name: String, token_estimate: usize },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    Thinking {
        thinking: String,
        signature: String,
    },
    RedactedThinking {
        data: String,
    },
    SpeakOutput {
        text: String,
    },
    QuestionOutput {
        id: String,
        question: String,
        question_type: String,
        #[serde(default)]
        options: Vec<String>,
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

pub async fn create_user_message_raw(
    executor: &DbExecutor,
    conversation_id: &str,
    content_json: &str,
) -> Result<String, String> {
    create_message(executor, conversation_id, "user", content_json, None, None, None).await
}

pub async fn create_assistant_message(
    executor: &DbExecutor,
    conversation_id: &str,
    content_json: &str,
    model: &str,
    stop_reason: &str,
    input_tokens: usize,
    output_tokens: usize,
    cache_creation_tokens: usize,
    cache_read_tokens: usize,
) -> Result<String, String> {
    create_message(
        executor,
        conversation_id,
        "assistant",
        content_json,
        Some(model),
        Some(stop_reason),
        Some((input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens)),
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
    tokens: Option<(usize, usize, usize, usize)>,
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

        msg.add_property(conn, "foundation:role", vec![Object::Literal {
            value: role_str.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "ai").map_err(|e| format!("Failed to set role: {}", e))?;

        msg.add_property(conn, "foundation:content", vec![Object::Literal {
            value: content_str,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "ai").map_err(|e| format!("Failed to set content: {}", e))?;

        msg.add_property(conn, "foundation:sentAt", vec![Object::DateTime(chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339())], "ai")
            .map_err(|e| format!("add_property failed: {}", e))?;

        msg.add_property(
            conn, "foundation:partOfConversation", vec![Object::Iri(conversation_iri.clone())], "ai",
        ).map_err(|e| format!("Property error: {}", e))?;

        msg.add_property(
            conn, "foundation:tokenCount", vec![Object::Integer(token_count as i64)], "ai",
        ).map_err(|e| format!("Property error: {}", e))?;

        if role_str == "user" {
            msg.add_property(conn, "foundation:sender",
                vec![Object::Iri("foundation:ThisUser".to_string())], "ai")
                .map_err(|e| format!("Property error: {}", e))?;
            msg.add_property(conn, "foundation:receiver",
                vec![Object::Iri("foundation:LocalAIAssistant".to_string())], "ai")
                .map_err(|e| format!("Property error: {}", e))?;
        } else {
            msg.add_property(conn, "foundation:sender",
                vec![Object::Iri("foundation:LocalAIAssistant".to_string())], "ai")
                .map_err(|e| format!("Property error: {}", e))?;
            msg.add_property(conn, "foundation:receiver",
                vec![Object::Iri("foundation:ThisUser".to_string())], "ai")
                .map_err(|e| format!("Property error: {}", e))?;
        }

        if let Some((input, output, cache_creation, cache_read)) = tokens {
            msg.add_property(
                conn, "foundation:inputTokens", vec![Object::Integer(input as i64)], "ai",
            ).map_err(|e| format!("Property error: {}", e))?;
            msg.add_property(
                conn, "foundation:outputTokens", vec![Object::Integer(output as i64)], "ai",
            ).map_err(|e| format!("Property error: {}", e))?;
            msg.add_property(
                conn, "foundation:cacheCreationTokens", vec![Object::Integer(cache_creation as i64)], "ai",
            ).map_err(|e| format!("Property error: {}", e))?;
            msg.add_property(
                conn, "foundation:cacheReadTokens", vec![Object::Integer(cache_read as i64)], "ai",
            ).map_err(|e| format!("Property error: {}", e))?;

            if let Some(model_str) = &model_opt {
                if let Some(cost) = estimate_call_cost(
                    conn, model_str,
                    input as u32, output as u32,
                    cache_creation as u32, cache_read as u32,
                ) {
                    msg.add_property(
                        conn, "foundation:estimatedCost", vec![Object::Number(cost)], "ai",
                    ).map_err(|e| format!("Property error: {}", e))?;
                }
            }
        }

        if let Some(model_str) = model_opt {
            msg.add_property(conn, "foundation:model", vec![Object::Literal {
                value: model_str,
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai").map_err(|e| format!("Failed to set model: {}", e))?;
        }

        if let Some(stop_str) = stop_reason_opt {
            msg.add_property(conn, "foundation:stopReason", vec![Object::Literal {
                value: stop_str,
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai").map_err(|e| format!("Failed to set stopReason: {}", e))?;
        }

        crate::search::reindex_subjects(conn, &[msg_iri_clone.clone()]);

        crate::owl::touch(conn, &conversation_iri);

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
    let selected = executor.read(move |conn| {
        // Load IRIs ordered newest-first — light query, no message content yet
        let iris_desc = Individual::find_messages_by_conversation(conn, &conversation_id, usize::MAX, 0)
            .map_err(|e| format!("Failed to query messages: {}", e))?;

        let mut selected: Vec<AIConversationMessage> = Vec::new();
        let mut total_tokens = 0;
        let mut failed_count = 0usize;
        let mut i = 0;

        while i < iris_desc.len() {
            let msg = match load_message(conn, &iris_desc[i]) {
                Ok(m) => m,
                Err(_) => { failed_count += 1; i += 1; continue; }
            };
            let msg_tokens = msg.token_count.unwrap_or(0);

            let has_tool_results = msg.content.iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { .. }));

            if has_tool_results && i + 1 < iris_desc.len() {
                let prev_msg = match load_message(conn, &iris_desc[i + 1]) {
                    Ok(m) => m,
                    Err(_) => {
                        // Paired tool_use unreadable — include only this message
                        failed_count += 1;
                        if !selected.is_empty() && total_tokens + msg_tokens > max_tokens { break; }
                        selected.push(msg);
                        total_tokens += msg_tokens;
                        i += 1;
                        continue;
                    }
                };
                let pair_tokens = msg_tokens + prev_msg.token_count.unwrap_or(0);
                // Always include the first pair so we never send an empty messages array to the API,
                // even if a large tool result pushes it over budget.
                if !selected.is_empty() && total_tokens + pair_tokens > max_tokens { break; }
                // Push newer first, then older — selected.reverse() restores chronological order
                selected.push(msg);
                selected.push(prev_msg);
                total_tokens += pair_tokens;
                i += 2;
            } else {
                if !selected.is_empty() && total_tokens + msg_tokens > max_tokens { break; }
                selected.push(msg);
                total_tokens += msg_tokens;
                i += 1;
            }
        }

        super::log_backend("info", &format!(
            "[CHAT] Loaded {} messages ({} skipped, {} failed)",
            selected.len(), iris_desc.len().saturating_sub(i), failed_count,
        ));

        selected.reverse();
        Ok::<(Vec<AIConversationMessage>, usize), String>((selected, total_tokens))
    }).await?;

    let (selected, total_tokens) = selected;

    // Validate tool_use/tool_result adjacency — Claude API requires that each assistant message
    // with tool_use blocks is IMMEDIATELY followed by a user message containing tool_result blocks
    // for ALL those IDs. The two-pass approach (tracking orphans) is insufficient because a
    // tool_use may have a matching tool_result later in history (not immediately after), which
    // fools the orphan check while still being invalid for the API.
    let selected_count = selected.len();
    let mut validated: Vec<AIConversationMessage> = Vec::with_capacity(selected.len());

    for msg in selected {
        let pending_tool_ids: Vec<String> = validated.last()
            .filter(|prev| prev.role == "assistant")
            .map(|prev| prev.content.iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse { id, .. } => Some(id.clone()),
                    ContentBlock::QuestionOutput { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .collect())
            .unwrap_or_default();

        if !pending_tool_ids.is_empty() {
            // Previous assistant message has tool_use — this message must satisfy ALL of them
            let resolved_ids: std::collections::HashSet<&str> = msg.content.iter()
                .filter_map(|b| if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                    Some(tool_use_id.as_str())
                } else {
                    None
                })
                .collect();

            let all_satisfied = pending_tool_ids.iter()
                .all(|id| resolved_ids.contains(id.as_str()));

            if all_satisfied {
                validated.push(msg);
            } else {
                // Strip tool_use blocks from the previous assistant message — the required
                // tool_results are not in the immediately following message
                if let Some(prev) = validated.last_mut() {
                    super::log_backend("warn", &format!(
                        "[CHAT] Stripping tool_use and thinking blocks from {} — tool_results not after",
                        prev.iri
                    ));
                    prev.content.retain(|b| !matches!(
                        b,
                        ContentBlock::ToolUse { .. } | ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. }
                    ));
                }

                let clean_content: Vec<ContentBlock> = msg.content.iter()
                    .filter(|b| !matches!(b, ContentBlock::ToolResult { .. }))
                    .cloned()
                    .collect();

                if !clean_content.is_empty() {
                    let mut clean_msg = msg;
                    clean_msg.content = clean_content;
                    validated.push(clean_msg);
                } else {
                    super::log_backend(
                        "warn", "[CHAT] Dropped message with only orphaned tool_results",
                    );
                }
            }
        } else {
            if msg.role == "user"
                && msg.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            {
                let clean_content: Vec<ContentBlock> = msg.content.iter()
                    .filter(|b| !matches!(b, ContentBlock::ToolResult { .. }))
                    .cloned()
                    .collect();

                super::log_backend(
                    "warn", "[CHAT] Stripping orphaned tool_result blocks (no preceding tool_use)",
                );

                if !clean_content.is_empty() {
                    let mut clean_msg = msg;
                    clean_msg.content = clean_content;
                    validated.push(clean_msg);
                }
                // else: message only contained orphaned tool_results — drop it entirely
            } else {
                validated.push(msg);
            }
        }
    }

    let final_cleaned: Vec<AIConversationMessage> = validated
        .into_iter()
        .filter(|msg| !msg.content.is_empty())
        .collect();

    // Merge consecutive same-role messages — Claude API requires strict alternation.
    // This can happen when a network error leaves an unanswered user message in the DB and the
    // user sends another message before the conversation is recovered.
    let mut merged: Vec<AIConversationMessage> = Vec::new();
    for msg in final_cleaned {
        if let Some(prev) = merged.last_mut() {
            if prev.role == msg.role {
                super::log_backend("warn", &format!(
                    "[CHAT] Merging consecutive {} messages: {} into {}",
                    msg.role, msg.iri, prev.iri
                ));
                prev.content.extend(msg.content);
                continue;
            }
        }
        merged.push(msg);
    }

    super::log_backend("info", &format!(
        "[CHAT] Selected {} messages within token budget ({}/{}), cleaned {} messages",
        merged.len(), total_tokens, max_tokens,
        selected_count.saturating_sub(merged.len())
    ));

    Ok(merged)
}

/// Load a single message from the database
pub(super) fn load_message(conn: &Connection, iri: &str) -> Result<AIConversationMessage, String> {
    let ind = Individual::get(conn, iri)
        .map_err(|e| format!("Failed to load message: {}", e))?
        .ok_or_else(|| format!("Message {} not found", iri))?;

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
            Object::DateTime(rfc3339) => chrono::DateTime::parse_from_rfc3339(rfc3339).ok().map(|dt| dt.timestamp_millis()),
            _ => None,
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

/// Log a single API call to the ontology as a foundation:AIAPICall entity
pub async fn log_api_call(
    executor: &DbExecutor,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_tokens: u32,
    cache_read_tokens: u32,
    conversation_id: Option<&str>,
) -> Result<(), String> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let iri = format!("foundation:AIAPICall_{}", timestamp);
    let model = model.to_string();
    let conversation_iri = conversation_id.map(|s| s.to_string());

    executor.write(move |conn| {
        let call = Individual::new(&iri);

        call.assert(conn, "foundation:AIAPICall", "AI API Call", "api", "ai")
            .map_err(|e| format!("Failed to create AIAPICall: {}", e))?;

        call.add_property(conn, "foundation:model", vec![Object::Literal {
            value: model.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "ai").map_err(|e| format!("Failed to set model: {}", e))?;

        call.add_property(conn, "foundation:inputTokens",
            vec![Object::Integer(input_tokens as i64)], "ai")
            .map_err(|e| format!("Failed to set inputTokens: {}", e))?;

        call.add_property(conn, "foundation:outputTokens",
            vec![Object::Integer(output_tokens as i64)], "ai")
            .map_err(|e| format!("Failed to set outputTokens: {}", e))?;

        call.add_property(conn, "foundation:cacheCreationTokens",
            vec![Object::Integer(cache_creation_tokens as i64)], "ai")
            .map_err(|e| format!("Failed to set cacheCreationTokens: {}", e))?;

        call.add_property(conn, "foundation:cacheReadTokens",
            vec![Object::Integer(cache_read_tokens as i64)], "ai")
            .map_err(|e| format!("Failed to set cacheReadTokens: {}", e))?;

        call.add_property(conn, "foundation:calledAt",
            vec![Object::DateTime(chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339())], "ai")
            .map_err(|e| format!("Failed to set calledAt: {}", e))?;

        if let Some(conv_iri) = conversation_iri {
            call.add_property(conn, "foundation:generatedByConversation",
                vec![Object::Iri(conv_iri)], "ai")
                .map_err(|e| format!("Failed to set generatedByConversation: {}", e))?;
        }

        if let Some(cost) = estimate_call_cost(
            conn, &model, input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens,
        ) {
            call.add_property(conn, "foundation:estimatedCost",
                vec![Object::Number(cost)], "ai")
                .map_err(|e| format!("Failed to set estimatedCost: {}", e))?;
        }

        Ok(iri)
    }).await.map(|_| ())
}

fn estimate_call_cost(
    conn: &crate::eavto::Connection,
    model_identifier: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_tokens: u32,
    cache_read_tokens: u32,
) -> Option<f64> {
    let model_iris = Individual::find_by_class_and_properties(
        conn,
        "foundation:AIModel",
        &[("foundation:modelIdentifier", model_identifier)],
    ).ok()?;

    let model_iri = model_iris.into_iter().next()?;
    let model_ind = Individual::get(conn, &model_iri).ok().flatten()?;

    let get_price = |prop: &str| -> Option<f64> {
        model_ind.properties.iter()
            .find(|(k, _)| k == prop)
            .and_then(|(_, v)| match v {
                Object::Number(n) => Some(*n),
                Object::Literal { value, .. } => value.parse::<f64>().ok(),
                _ => None,
            })
    };

    let input_price = get_price("foundation:inputPricePerMTok")?;
    let output_price = get_price("foundation:outputPricePerMTok")?;
    let cache_write_price = get_price("foundation:cacheWrite5minPricePerMTok").unwrap_or(0.0);
    let cache_read_price = get_price("foundation:cacheReadPricePerMTok").unwrap_or(0.0);

    let cost = (input_tokens as f64 * input_price
        + output_tokens as f64 * output_price
        + cache_creation_tokens as f64 * cache_write_price
        + cache_read_tokens as f64 * cache_read_price) / 1_000_000.0;

    Some(cost)
}

fn get_tokenizer() -> Result<&'static tiktoken_rs::CoreBPE, String> {
    static TOKENIZER: std::sync::OnceLock<tiktoken_rs::CoreBPE> = std::sync::OnceLock::new();
    if let Some(bpe) = TOKENIZER.get() {
        return Ok(bpe);
    }
    let bpe = tiktoken_rs::cl100k_base()
        .map_err(|e| format!("Failed to load tokenizer: {}", e))?;
    Ok(TOKENIZER.get_or_init(|| bpe))
}

pub fn tokenize_text(text: &str) -> usize {
    get_tokenizer()
        .map(|bpe| bpe.encode_with_special_tokens(text).len())
        .unwrap_or(0)
}

const MESSAGE_TOKEN_OVERHEAD: usize = 4;
const TOOL_TOKEN_OVERHEAD: usize = 10;

fn calculate_content_tokens(content_json: &str) -> Result<usize, String> {
    let bpe = get_tokenizer()?;

    let blocks: Vec<ContentBlock> = serde_json::from_str(content_json)
        .map_err(|e| format!("Failed to parse content: {}", e))?;

    let mut total = MESSAGE_TOKEN_OVERHEAD;

    for block in &blocks {
        total += match block {
            ContentBlock::Text { text } => bpe.encode_with_special_tokens(text).len(),
            ContentBlock::ToolUse { name, input, .. } => {
                let json = serde_json::to_string(input).unwrap_or_default();
                bpe.encode_with_special_tokens(name).len() +
                bpe.encode_with_special_tokens(&json).len() + TOOL_TOKEN_OVERHEAD
            },
            ContentBlock::ToolResult { content, .. } => {
                bpe.encode_with_special_tokens(content).len() + TOOL_TOKEN_OVERHEAD
            },
            ContentBlock::Image { .. } => 0,
            ContentBlock::CameraRef { token_estimate, .. } => *token_estimate,
            ContentBlock::Document { .. } => 0,
            ContentBlock::FileRef { token_estimate, .. } => *token_estimate,
            ContentBlock::Thinking { thinking, .. } => bpe.encode_with_special_tokens(thinking).len(),
            ContentBlock::RedactedThinking { .. } => 0,
            ContentBlock::SpeakOutput { text } => bpe.encode_with_special_tokens(text).len(),
            ContentBlock::QuestionOutput { question, options, .. } => {
                let opts = options.join(", ");
                bpe.encode_with_special_tokens(question).len() +
                bpe.encode_with_special_tokens(&opts).len() + TOOL_TOKEN_OVERHEAD
            },
        };
    }

    Ok(total)
}
