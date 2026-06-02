use crate::owl::DbExecutor;
use crate::owl::{Individual, Object, Triple, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Image { source: ImageSource },
    CameraRef { file_path: String, token_estimate: usize },
    Document { source: DocumentSource },
    FileRef {
        file_iri: String,
        file_name: String,
        token_estimate: usize,
        /// AI-generated summary populated at load time from `foundation:aiSummary`.
        /// Not persisted — derived from the knowledge base on every history load.
        #[serde(skip, default)]
        ai_summary: Option<String>,
    },
    ToolUse { id: String, name: String, input: serde_json::Value, #[serde(skip_serializing_if = "Option::is_none")] reason: Option<String> },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        duration_ms: Option<u64>,
    },
    Thinking {
        thinking: String,
        signature: String,
    },
    RedactedThinking {
        data: String,
    },
    QuestionOutput {
        id: String,
        question: String,
        question_type: String,
        #[serde(default)]
        options: Vec<String>,
    },
    CompactionSummary { text: String },
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
    let blocks = vec![ContentBlock::Text { text: text.to_string() }];
    create_message(executor, conversation_id, "user", blocks, None, None, None).await
}

pub async fn create_user_message_raw(
    executor: &DbExecutor,
    conversation_id: &str,
    content_json: &str,
) -> Result<String, String> {
    let blocks: Vec<ContentBlock> = serde_json::from_str(content_json)
        .map_err(|e| format!("Failed to parse content JSON: {}", e))?;
    create_message(executor, conversation_id, "user", blocks, None, None, None).await
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
    let blocks: Vec<ContentBlock> = serde_json::from_str(content_json)
        .map_err(|e| format!("Failed to parse content JSON: {}", e))?;
    create_message(
        executor,
        conversation_id,
        "assistant",
        blocks,
        Some(model),
        Some(stop_reason),
        Some((input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens)),
    ).await
}

pub(crate) async fn create_message(
    executor: &DbExecutor,
    conversation_id: &str,
    role: &str,
    blocks: Vec<ContentBlock>,
    model: Option<&str>,
    stop_reason: Option<&str>,
    tokens: Option<(usize, usize, usize, usize)>,
) -> Result<String, String> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let message_iri = format!("foundation:AIConversationMessage_{}", timestamp);

    let token_count = calculate_content_tokens(&blocks);

    let msg_iri_clone = message_iri.clone();
    let conversation_iri = conversation_id.to_string();
    let role_str = role.to_string();
    let model_opt = model.map(|s| s.to_string());
    let stop_reason_opt = stop_reason.map(|s| s.to_string());

    executor.write(move |conn| {
        // Build all triples in memory first, then insert in a single append_triples call.
        // This bypasses the OWL validation pipeline (has_prop + cardinality + domain/range
        // checks) that add_property runs per-property. For a brand-new IRI every check
        // trivially returns "no existing value", making them pure overhead.
        let mut triples: Vec<Triple> = Vec::with_capacity(32);
        let lit = |value: String, datatype: &str| Object::Literal {
            value,
            datatype: Some(datatype.to_string()),
            language: None,
        };

        triples.push(Triple::new(&msg_iri_clone, "rdf:type", Object::Iri("foundation:AIConversationMessage".to_string())));
        triples.push(Triple::new(&msg_iri_clone, "rdfs:label", lit(format!("{} message", role_str), "xsd:string")));
        triples.push(Triple::new(&msg_iri_clone, "foundation:role", lit(role_str.clone(), "xsd:string")));
        triples.push(Triple::new(&msg_iri_clone, "foundation:sentAt",
            Object::DateTime(chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339())));
        triples.push(Triple::new(&msg_iri_clone, "foundation:partOfConversation", Object::Iri(conversation_iri.clone())));
        triples.push(Triple::new(&msg_iri_clone, "foundation:tokenCount", Object::Integer(token_count as i64)));

        if role_str == "user" {
            triples.push(Triple::new(&msg_iri_clone, "foundation:sender", Object::Iri("foundation:ThisUser".to_string())));
            triples.push(Triple::new(&msg_iri_clone, "foundation:receiver", Object::Iri("foundation:LocalAIAssistant".to_string())));
        } else {
            triples.push(Triple::new(&msg_iri_clone, "foundation:sender", Object::Iri("foundation:LocalAIAssistant".to_string())));
            triples.push(Triple::new(&msg_iri_clone, "foundation:receiver", Object::Iri("foundation:ThisUser".to_string())));
        }

        if let Some((input, output, cache_creation, cache_read)) = tokens {
            triples.push(Triple::new(&msg_iri_clone, "foundation:inputTokens", Object::Integer(input as i64)));
            triples.push(Triple::new(&msg_iri_clone, "foundation:outputTokens", Object::Integer(output as i64)));
            triples.push(Triple::new(&msg_iri_clone, "foundation:cacheCreationTokens", Object::Integer(cache_creation as i64)));
            triples.push(Triple::new(&msg_iri_clone, "foundation:cacheReadTokens", Object::Integer(cache_read as i64)));
            if let Some(model_str) = &model_opt {
                if let Some(cost) = estimate_call_cost(conn, model_str, input as u32, output as u32, cache_creation as u32, cache_read as u32) {
                    triples.push(Triple::new(&msg_iri_clone, "foundation:estimatedCost", Object::Number(cost)));
                }
            }
        }
        if let Some(model_str) = &model_opt {
            triples.push(Triple::new(&msg_iri_clone, "foundation:model", lit(model_str.clone(), "xsd:string")));
        }
        if let Some(stop_str) = &stop_reason_opt {
            triples.push(Triple::new(&msg_iri_clone, "foundation:stopReason", lit(stop_str.clone(), "xsd:string")));
        }

        let block_iris = collect_block_triples(conn, &blocks, timestamp, &mut triples)?;
        for block_iri in &block_iris {
            triples.push(Triple::new(&msg_iri_clone, "foundation:hasContentBlock", Object::Iri(block_iri.clone())));
        }

        crate::owl::batch_insert_triples(conn, &triples, "ai")
            .map_err(|e| format!("Failed to create message: {}", e))?;

        crate::owl::touch(conn, &conversation_iri);

        Ok(msg_iri_clone)
    }).await?;

    let iri_for_reindex = message_iri.clone();
    let executor_for_reindex = executor.clone();
    tauri::async_runtime::spawn(async move {
        let _ = executor_for_reindex.read(move |conn| {
            crate::search::reindex_subjects(conn, &[iri_for_reindex]);
            Ok(String::new())
        }).await;
    });

    Ok(message_iri)
}

/// Collect triples for all content blocks into `out`, returning the block IRIs.
/// Does NOT write to the DB — the caller is responsible for the actual insert.
/// Used by `create_message` to batch all writes into a single `append_triples` call.
fn collect_block_triples(
    conn: &Connection,
    blocks: &[ContentBlock],
    message_timestamp: i64,
    out: &mut Vec<Triple>,
) -> Result<Vec<String>, String> {
    let lit = |value: String, datatype: &str| Object::Literal {
        value,
        datatype: Some(datatype.to_string()),
        language: None,
    };
    let mut block_iris = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        let (class_iri, suffix) = match block {
            ContentBlock::Text { .. }             => ("anthropic:TextBlock", "TextBlock"),
            ContentBlock::ToolUse { .. }          => ("anthropic:ToolUseBlock", "ToolUseBlock"),
            ContentBlock::ToolResult { .. }       => ("anthropic:ToolResultBlock", "ToolResultBlock"),
            ContentBlock::Thinking { .. }         => ("anthropic:ThinkingBlock", "ThinkingBlock"),
            ContentBlock::RedactedThinking { .. } => ("anthropic:RedactedThinkingBlock", "RedactedThinkingBlock"),
            ContentBlock::Image { .. }            => ("anthropic:ImageBlock", "ImageBlock"),
            ContentBlock::Document { .. }         => ("anthropic:DocumentBlock", "DocumentBlock"),
            ContentBlock::QuestionOutput { .. }   => ("foundation:QuestionOutputBlock", "QuestionOutputBlock"),
            ContentBlock::CameraRef { .. }        => ("foundation:CameraRefBlock", "CameraRefBlock"),
            ContentBlock::FileRef { .. }          => ("foundation:FileRefBlock", "FileRefBlock"),
            ContentBlock::CompactionSummary { .. } => ("foundation:CompactionSummaryBlock", "CompactionSummaryBlock"),
        };
        let prefix_end = class_iri.find(':').map(|i| i + 1).unwrap_or(0);
        let block_iri = format!("{}{}_{}_{}", &class_iri[..prefix_end], suffix, message_timestamp, index);

        out.push(Triple::new(&block_iri, "rdf:type", Object::Iri(class_iri.to_string())));
        out.push(Triple::new(&block_iri, "rdfs:label", lit(suffix.to_string(), "xsd:string")));
        out.push(Triple::new(&block_iri, "foundation:blockIndex", Object::Integer(index as i64)));

        match block {
            ContentBlock::Text { text } => {
                out.push(Triple::new(&block_iri, "anthropic:text", lit(text.clone(), "xsd:string")));
            }
            ContentBlock::ToolUse { id, name, input, reason } => {
                out.push(Triple::new(&block_iri, "anthropic:toolUseId", lit(id.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "anthropic:toolName", lit(name.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "anthropic:toolInput",
                    lit(serde_json::to_string(input).unwrap_or_default(), "xsd:string")));
                if let Some(r) = reason {
                    out.push(Triple::new(&block_iri, "foundation:toolCallReason", lit(r.clone(), "xsd:string")));
                }
            }
            ContentBlock::ToolResult { tool_use_id, content, is_error, duration_ms } => {
                out.push(Triple::new(&block_iri, "anthropic:resultContent", lit(content.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "anthropic:isError",
                    lit(is_error.unwrap_or(false).to_string(), "xsd:boolean")));
                if let Some(d) = duration_ms {
                    out.push(Triple::new(&block_iri, "foundation:durationMs", lit(d.to_string(), "xsd:integer")));
                }
                if !tool_use_id.is_empty() {
                    let found = Individual::find_by_class_and_properties(
                        conn, "anthropic:ToolUseBlock", &[("anthropic:toolUseId", tool_use_id.as_str())],
                    ).ok().and_then(|v| v.into_iter().next())
                    .or_else(|| Individual::find_by_class_and_properties(
                        conn, "foundation:QuestionOutputBlock", &[("foundation:questionToolUseId", tool_use_id.as_str())],
                    ).ok().and_then(|v| v.into_iter().next()));
                    if let Some(ref_iri) = found {
                        out.push(Triple::new(&block_iri, "anthropic:resultOf", Object::Iri(ref_iri)));
                    }
                }
            }
            ContentBlock::Thinking { thinking, signature } => {
                out.push(Triple::new(&block_iri, "anthropic:thinking", lit(thinking.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "anthropic:signature", lit(signature.clone(), "xsd:string")));
            }
            ContentBlock::RedactedThinking { data } => {
                out.push(Triple::new(&block_iri, "anthropic:redactedData", lit(data.clone(), "xsd:string")));
            }
            ContentBlock::Image { source } => {
                out.push(Triple::new(&block_iri, "anthropic:mediaType", lit(source.media_type.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "anthropic:imageData", lit(source.data.clone(), "xsd:string")));
            }
            ContentBlock::Document { source } => {
                out.push(Triple::new(&block_iri, "anthropic:mediaType", lit(source.media_type.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "anthropic:documentData", lit(source.data.clone(), "xsd:string")));
            }
            ContentBlock::QuestionOutput { id, question, question_type, options } => {
                out.push(Triple::new(&block_iri, "foundation:questionToolUseId", lit(id.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "foundation:questionText", lit(question.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "foundation:questionType", lit(question_type.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "foundation:questionOptions",
                    lit(serde_json::to_string(options).unwrap_or_else(|_| "[]".to_string()), "xsd:string")));
            }
            ContentBlock::CameraRef { file_path, token_estimate } => {
                out.push(Triple::new(&block_iri, "foundation:cameraFilePath", lit(file_path.clone(), "xsd:anyURI")));
                out.push(Triple::new(&block_iri, "foundation:tokenEstimate", Object::Integer(*token_estimate as i64)));
            }
            ContentBlock::FileRef { file_iri, file_name, token_estimate, .. } => {
                out.push(Triple::new(&block_iri, "foundation:fileRef", Object::Iri(file_iri.clone())));
                out.push(Triple::new(&block_iri, "foundation:attachedFileName", lit(file_name.clone(), "xsd:string")));
                out.push(Triple::new(&block_iri, "foundation:tokenEstimate", Object::Integer(*token_estimate as i64)));
            }
            ContentBlock::CompactionSummary { text } => {
                out.push(Triple::new(&block_iri, "foundation:summaryText", lit(text.clone(), "xsd:string")));
            }
        }

        block_iris.push(block_iri);
    }
    Ok(block_iris)
}

/// Save content blocks as OWL individuals linked to the given message IRI.
/// Returns the list of block IRIs created.
pub fn save_content_blocks(
    conn: &mut Connection,
    message_iri: &str,
    message_timestamp: i64,
    blocks: &[ContentBlock],
) -> Result<Vec<String>, String> {
    let msg = Individual::new(message_iri);
    let mut block_iris = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        let (class_iri, block_iri_suffix) = match block {
            ContentBlock::Text { .. }           => ("anthropic:TextBlock", "TextBlock"),
            ContentBlock::ToolUse { .. }        => ("anthropic:ToolUseBlock", "ToolUseBlock"),
            ContentBlock::ToolResult { .. }     => ("anthropic:ToolResultBlock", "ToolResultBlock"),
            ContentBlock::Thinking { .. }       => ("anthropic:ThinkingBlock", "ThinkingBlock"),
            ContentBlock::RedactedThinking { .. } => ("anthropic:RedactedThinkingBlock", "RedactedThinkingBlock"),
            ContentBlock::Image { .. }          => ("anthropic:ImageBlock", "ImageBlock"),
            ContentBlock::Document { .. }       => ("anthropic:DocumentBlock", "DocumentBlock"),
            ContentBlock::QuestionOutput { .. } => ("foundation:QuestionOutputBlock", "QuestionOutputBlock"),
            ContentBlock::CameraRef { .. }      => ("foundation:CameraRefBlock", "CameraRefBlock"),
            ContentBlock::FileRef { .. }        => ("foundation:FileRefBlock", "FileRefBlock"),
            ContentBlock::CompactionSummary { .. } => ("foundation:CompactionSummaryBlock", "CompactionSummaryBlock"),
        };

        let prefix_end = class_iri.find(':').map(|i| i + 1).unwrap_or(0);
        let block_iri = format!("{}{}_{}_{}",
            &class_iri[..prefix_end],
            block_iri_suffix,
            message_timestamp,
            index
        );

        let ind = Individual::new(&block_iri);
        ind.assert(conn, class_iri, block_iri_suffix, "chat", "ai")
            .map_err(|e| format!("Failed to assert block {}: {}", block_iri, e))?;

        ind.add_property(conn, "foundation:blockIndex", vec![Object::Integer(index as i64)], "ai")
            .map_err(|e| format!("Failed to set blockIndex: {}", e))?;

        match block {
            ContentBlock::Text { text } => {
                ind.add_property(conn, "anthropic:text", vec![Object::Literal {
                    value: text.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set text: {}", e))?;
            }

            ContentBlock::ToolUse { id, name, input, reason } => {
                let input_str = serde_json::to_string(input).unwrap_or_default();
                ind.add_property(conn, "anthropic:toolUseId", vec![Object::Literal {
                    value: id.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set toolUseId: {}", e))?;
                ind.add_property(conn, "anthropic:toolName", vec![Object::Literal {
                    value: name.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set toolName: {}", e))?;
                ind.add_property(conn, "anthropic:toolInput", vec![Object::Literal {
                    value: input_str,
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set toolInput: {}", e))?;
                if let Some(r) = reason {
                    ind.add_property(conn, "foundation:toolCallReason", vec![Object::Literal {
                        value: r.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }], "ai").map_err(|e| format!("Failed to set toolCallReason: {}", e))?;
                }
            }

            ContentBlock::ToolResult { tool_use_id, content, is_error, duration_ms } => {
                ind.add_property(conn, "anthropic:resultContent", vec![Object::Literal {
                    value: content.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set resultContent: {}", e))?;

                let error_val = is_error.unwrap_or(false);
                ind.add_property(conn, "anthropic:isError", vec![Object::Literal {
                    value: error_val.to_string(),
                    datatype: Some("xsd:boolean".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set isError: {}", e))?;

                if let Some(d) = duration_ms {
                    ind.add_property(conn, "foundation:durationMs", vec![Object::Literal {
                        value: d.to_string(),
                        datatype: Some("xsd:integer".to_string()),
                        language: None,
                    }], "ai").map_err(|e| format!("Failed to set durationMs: {}", e))?;
                }

                if !tool_use_id.is_empty() {
                    let found_iri = Individual::find_by_class_and_properties(
                        conn,
                        "anthropic:ToolUseBlock",
                        &[("anthropic:toolUseId", tool_use_id.as_str())],
                    ).ok().and_then(|v| v.into_iter().next())
                    .or_else(|| {
                        Individual::find_by_class_and_properties(
                            conn,
                            "foundation:QuestionOutputBlock",
                            &[("foundation:questionToolUseId", tool_use_id.as_str())],
                        ).ok().and_then(|v| v.into_iter().next())
                    });
                    if let Some(block_iri) = found_iri {
                        ind.add_property(conn, "anthropic:resultOf",
                            vec![Object::Iri(block_iri)], "ai")
                            .map_err(|e| format!("Failed to set resultOf: {}", e))?;
                    }
                }
            }

            ContentBlock::Thinking { thinking, signature } => {
                ind.add_property(conn, "anthropic:thinking", vec![Object::Literal {
                    value: thinking.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set thinking: {}", e))?;
                ind.add_property(conn, "anthropic:signature", vec![Object::Literal {
                    value: signature.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set signature: {}", e))?;
            }

            ContentBlock::RedactedThinking { data } => {
                ind.add_property(conn, "anthropic:redactedData", vec![Object::Literal {
                    value: data.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set redactedData: {}", e))?;
            }

            ContentBlock::Image { source } => {
                ind.add_property(conn, "anthropic:mediaType", vec![Object::Literal {
                    value: source.media_type.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set mediaType: {}", e))?;
                ind.add_property(conn, "anthropic:imageData", vec![Object::Literal {
                    value: source.data.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set imageData: {}", e))?;
            }

            ContentBlock::Document { source } => {
                ind.add_property(conn, "anthropic:mediaType", vec![Object::Literal {
                    value: source.media_type.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set mediaType: {}", e))?;
                ind.add_property(conn, "anthropic:documentData", vec![Object::Literal {
                    value: source.data.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set documentData: {}", e))?;
            }

            ContentBlock::QuestionOutput { id, question, question_type, options } => {
                let options_json = serde_json::to_string(options).unwrap_or_else(|_| "[]".to_string());
                ind.add_property(conn, "foundation:questionToolUseId", vec![Object::Literal {
                    value: id.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set questionToolUseId: {}", e))?;
                ind.add_property(conn, "foundation:questionText", vec![Object::Literal {
                    value: question.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set questionText: {}", e))?;
                ind.add_property(conn, "foundation:questionType", vec![Object::Literal {
                    value: question_type.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set questionType: {}", e))?;
                ind.add_property(conn, "foundation:questionOptions", vec![Object::Literal {
                    value: options_json,
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set questionOptions: {}", e))?;
            }

            ContentBlock::CameraRef { file_path, token_estimate } => {
                ind.add_property(conn, "foundation:cameraFilePath", vec![Object::Literal {
                    value: file_path.clone(),
                    datatype: Some("xsd:anyURI".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set cameraFilePath: {}", e))?;
                ind.add_property(conn, "foundation:tokenEstimate",
                    vec![Object::Integer(*token_estimate as i64)], "ai")
                    .map_err(|e| format!("Failed to set tokenEstimate: {}", e))?;
            }

            ContentBlock::FileRef { file_iri, file_name, token_estimate, .. } => {
                ind.add_property(conn, "foundation:fileRef",
                    vec![Object::Iri(file_iri.clone())], "ai")
                    .map_err(|e| format!("Failed to set fileRef: {}", e))?;
                ind.add_property(conn, "foundation:attachedFileName", vec![Object::Literal {
                    value: file_name.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set attachedFileName: {}", e))?;
                ind.add_property(conn, "foundation:tokenEstimate",
                    vec![Object::Integer(*token_estimate as i64)], "ai")
                    .map_err(|e| format!("Failed to set tokenEstimate: {}", e))?;
            }
            ContentBlock::CompactionSummary { text } => {
                ind.add_property(conn, "foundation:summaryText", vec![Object::Literal {
                    value: text.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }], "ai").map_err(|e| format!("Failed to set summaryText: {}", e))?;
            }
        }

        block_iris.push(block_iri);
    }

    // Link all blocks in a single add_property call so every IRI gets the same
    // tx_id. triples_current uses MAX(tx) per (subject, predicate), so separate
    // calls would cause earlier blocks to be shadowed by the last one.
    if !block_iris.is_empty() {
        msg.add_property(conn, "foundation:hasContentBlock",
            block_iris.iter().map(|iri| Object::Iri(iri.clone())).collect(),
            "ai")
            .map_err(|e| format!("Failed to link blocks to message: {}", e))?;
    }

    Ok(block_iris)
}

/// Load all content blocks for a message from OWL individuals.
pub fn load_content_blocks(
    conn: &Connection,
    message_iri: &str,
) -> Result<Vec<ContentBlock>, String> {
    let ind = Individual::get(conn, message_iri)
        .map_err(|e| format!("Failed to load message for blocks: {}", e))?
        .ok_or_else(|| format!("Message {} not found", message_iri))?;

    let block_iris: Vec<String> = ind.properties.iter()
        .filter(|(k, _)| k == "foundation:hasContentBlock")
        .filter_map(|(_, v)| if let Object::Iri(iri) = v { Some(iri.clone()) } else { None })
        .collect();

    if block_iris.is_empty() {
        return Ok(Vec::new());
    }

    let mut indexed_blocks: Vec<(i64, ContentBlock)> = Vec::new();

    for block_iri in &block_iris {
        let block_ind = match Individual::get(conn, block_iri) {
            Ok(Some(b)) => b,
            Ok(None) => continue,
            Err(_) => continue,
        };

        let block_index = block_ind.properties.iter()
            .find(|(k, _)| k == "foundation:blockIndex")
            .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n) } else { None })
            .unwrap_or(0);

        let class_iri = block_ind.types.first().map(|t| t.iri.as_str()).unwrap_or("");

        let get_str = |prop: &str| -> Option<String> {
            block_ind.properties.iter()
                .find(|(k, _)| k == prop)
                .and_then(|(_, v)| v.as_literal())
        };

        let block = match class_iri {
            "anthropic:TextBlock" => {
                let text = get_str("anthropic:text").unwrap_or_default();
                ContentBlock::Text { text }
            }

            "anthropic:ToolUseBlock" => {
                let id = get_str("anthropic:toolUseId").unwrap_or_default();
                let name = get_str("anthropic:toolName").unwrap_or_default();
                let input_str = get_str("anthropic:toolInput").unwrap_or_else(|| "{}".to_string());
                let input = serde_json::from_str(&input_str).unwrap_or(serde_json::Value::Object(Default::default()));
                let reason = get_str("foundation:toolCallReason");
                ContentBlock::ToolUse { id, name, input, reason }
            }

            "anthropic:ToolResultBlock" => {
                let tool_use_id = block_ind.properties.iter()
                    .find(|(k, _)| k == "anthropic:resultOf")
                    .and_then(|(_, v)| if let Object::Iri(iri) = v { Some(iri.clone()) } else { None })
                    .and_then(|result_of_iri| {
                        Individual::get(conn, &result_of_iri).ok().flatten()
                    })
                    .and_then(|tu_ind| {
                        tu_ind.properties.iter()
                            .find(|(k, _)| k == "anthropic:toolUseId" || k == "foundation:questionToolUseId")
                            .and_then(|(_, v)| v.as_literal())
                    })
                    .unwrap_or_default();
                let content = get_str("anthropic:resultContent").unwrap_or_default();
                let is_error = get_str("anthropic:isError").map(|s| s == "true");
                let duration_ms = get_str("foundation:durationMs")
                    .and_then(|s| s.parse::<u64>().ok());
                ContentBlock::ToolResult { tool_use_id, content, is_error, duration_ms }
            }

            "anthropic:ThinkingBlock" => {
                let thinking = get_str("anthropic:thinking").unwrap_or_default();
                let signature = get_str("anthropic:signature").unwrap_or_default();
                ContentBlock::Thinking { thinking, signature }
            }

            "anthropic:RedactedThinkingBlock" => {
                let data = get_str("anthropic:redactedData").unwrap_or_default();
                ContentBlock::RedactedThinking { data }
            }

            "anthropic:ImageBlock" => {
                let media_type = get_str("anthropic:mediaType").unwrap_or_default();
                let data = get_str("anthropic:imageData").unwrap_or_default();
                ContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64".to_string(),
                        media_type,
                        data,
                    }
                }
            }

            "anthropic:DocumentBlock" => {
                let media_type = get_str("anthropic:mediaType").unwrap_or_default();
                let data = get_str("anthropic:documentData").unwrap_or_default();
                ContentBlock::Document {
                    source: DocumentSource {
                        source_type: "base64".to_string(),
                        media_type,
                        data,
                    }
                }
            }

            "foundation:QuestionOutputBlock" => {
                let id = get_str("foundation:questionToolUseId").unwrap_or_default();
                let question = get_str("foundation:questionText").unwrap_or_default();
                let question_type = get_str("foundation:questionType").unwrap_or_default();
                let options: Vec<String> = get_str("foundation:questionOptions")
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                ContentBlock::QuestionOutput { id, question, question_type, options }
            }

            "foundation:CameraRefBlock" => {
                let file_path = get_str("foundation:cameraFilePath").unwrap_or_default();
                let token_estimate = block_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:tokenEstimate")
                    .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as usize) } else { None })
                    .unwrap_or(0);
                ContentBlock::CameraRef { file_path, token_estimate }
            }

            "foundation:FileRefBlock" => {
                let file_iri = block_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:fileRef")
                    .and_then(|(_, v)| if let Object::Iri(iri) = v { Some(iri.clone()) } else { None })
                    .unwrap_or_default();
                let file_name = get_str("foundation:attachedFileName").unwrap_or_default();
                let token_estimate = block_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:tokenEstimate")
                    .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as usize) } else { None })
                    .unwrap_or(0);
                ContentBlock::FileRef { file_iri, file_name, token_estimate, ai_summary: None }
            }

            "foundation:CompactionSummaryBlock" => {
                let text = get_str("foundation:summaryText").unwrap_or_default();
                ContentBlock::CompactionSummary { text }
            }

            _ => continue,
        };

        indexed_blocks.push((block_index, block));
    }

    indexed_blocks.sort_by_key(|(idx, _)| *idx);
    Ok(indexed_blocks.into_iter().map(|(_, b)| b).collect())
}

pub fn triple_str(triples: &[Triple], predicate: &str) -> Option<String> {
    triples.iter()
        .find(|t| t.predicate == predicate)
        .and_then(|t| t.object.as_literal())
}

pub fn triple_int(triples: &[Triple], predicate: &str) -> Option<i64> {
    triples.iter()
        .find(|t| t.predicate == predicate)
        .and_then(|t| if let Object::Integer(n) = &t.object { Some(*n) } else { None })
}

fn triple_iri<'a>(triples: &'a [Triple], predicate: &str) -> Option<&'a str> {
    triples.iter()
        .find(|t| t.predicate == predicate)
        .and_then(|t| t.object.as_iri())
}

pub fn triple_iris(triples: &[Triple], predicate: &str) -> Vec<String> {
    triples.iter()
        .filter(|t| t.predicate == predicate)
        .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
        .collect()
}

pub fn parse_content_block_from_batch(
    block_triples: &[Triple],
    tool_use_triples_map: &std::collections::HashMap<String, Vec<Triple>>,
) -> Option<(i64, ContentBlock)> {
    let class_iri = triple_iri(block_triples, "rdf:type").unwrap_or("");
    let block_index = triple_int(block_triples, "foundation:blockIndex").unwrap_or(0);

    let block = match class_iri {
        "anthropic:TextBlock" => {
            ContentBlock::Text { text: triple_str(block_triples, "anthropic:text").unwrap_or_default() }
        }
        "anthropic:ToolUseBlock" => {
            let id = triple_str(block_triples, "anthropic:toolUseId").unwrap_or_default();
            let name = triple_str(block_triples, "anthropic:toolName").unwrap_or_default();
            let input_str = triple_str(block_triples, "anthropic:toolInput").unwrap_or_else(|| "{}".to_string());
            let input = serde_json::from_str(&input_str).unwrap_or(serde_json::Value::Object(Default::default()));
            let reason = triple_str(block_triples, "foundation:toolCallReason");
            ContentBlock::ToolUse { id, name, input, reason }
        }
        "anthropic:ToolResultBlock" => {
            let tool_use_id = triple_iri(block_triples, "anthropic:resultOf")
                .and_then(|result_of_iri| tool_use_triples_map.get(result_of_iri))
                .and_then(|tu_triples| {
                    tu_triples.iter()
                        .find(|t| t.predicate == "anthropic:toolUseId" || t.predicate == "foundation:questionToolUseId")
                        .and_then(|t| t.object.as_literal())
                })
                .unwrap_or_default();
            let content = triple_str(block_triples, "anthropic:resultContent").unwrap_or_default();
            let is_error = triple_str(block_triples, "anthropic:isError").map(|s| s == "true");
            let duration_ms = triple_str(block_triples, "foundation:durationMs")
                .and_then(|s| s.parse::<u64>().ok());
            ContentBlock::ToolResult { tool_use_id, content, is_error, duration_ms }
        }
        "anthropic:ThinkingBlock" => ContentBlock::Thinking {
            thinking: triple_str(block_triples, "anthropic:thinking").unwrap_or_default(),
            signature: triple_str(block_triples, "anthropic:signature").unwrap_or_default(),
        },
        "anthropic:RedactedThinkingBlock" => ContentBlock::RedactedThinking {
            data: triple_str(block_triples, "anthropic:redactedData").unwrap_or_default(),
        },
        "anthropic:ImageBlock" => ContentBlock::Image {
            source: ImageSource {
                source_type: "base64".to_string(),
                media_type: triple_str(block_triples, "anthropic:mediaType").unwrap_or_default(),
                data: triple_str(block_triples, "anthropic:imageData").unwrap_or_default(),
            }
        },
        "anthropic:DocumentBlock" => ContentBlock::Document {
            source: DocumentSource {
                source_type: "base64".to_string(),
                media_type: triple_str(block_triples, "anthropic:mediaType").unwrap_or_default(),
                data: triple_str(block_triples, "anthropic:documentData").unwrap_or_default(),
            }
        },
        "foundation:QuestionOutputBlock" => ContentBlock::QuestionOutput {
            id: triple_str(block_triples, "foundation:questionToolUseId").unwrap_or_default(),
            question: triple_str(block_triples, "foundation:questionText").unwrap_or_default(),
            question_type: triple_str(block_triples, "foundation:questionType").unwrap_or_default(),
            options: triple_str(block_triples, "foundation:questionOptions")
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
        },
        "foundation:CameraRefBlock" => ContentBlock::CameraRef {
            file_path: triple_str(block_triples, "foundation:cameraFilePath").unwrap_or_default(),
            token_estimate: triple_int(block_triples, "foundation:tokenEstimate").unwrap_or(0) as usize,
        },
        "foundation:FileRefBlock" => ContentBlock::FileRef {
            file_iri: triple_iri(block_triples, "foundation:fileRef").unwrap_or("").to_string(),
            file_name: triple_str(block_triples, "foundation:attachedFileName").unwrap_or_default(),
            token_estimate: triple_int(block_triples, "foundation:tokenEstimate").unwrap_or(0) as usize,
            ai_summary: None,
        },
        "foundation:CompactionSummaryBlock" => ContentBlock::CompactionSummary {
            text: triple_str(block_triples, "foundation:summaryText").unwrap_or_default(),
        },
        _ => return None,
    };

    Some((block_index, block))
}

fn load_message_from_batch(
    iri: &str,
    msg_triples: &[Triple],
    block_triples_map: &std::collections::HashMap<String, Vec<Triple>>,
    tool_use_triples_map: &std::collections::HashMap<String, Vec<Triple>>,
) -> Result<AIConversationMessage, String> {
    let role = triple_str(msg_triples, "foundation:role").ok_or("Missing role")?;

    let timestamp = msg_triples.iter()
        .find(|t| t.predicate == "foundation:sentAt")
        .and_then(|t| {
            if let Object::DateTime(rfc3339) = &t.object {
                chrono::DateTime::parse_from_rfc3339(rfc3339).ok().map(|dt| dt.timestamp_millis())
            } else {
                None
            }
        })
        .ok_or("Missing timestamp")?;

    let token_count = triple_int(msg_triples, "foundation:tokenCount").map(|n| n as usize);
    let model = triple_str(msg_triples, "foundation:model");
    let stop_reason = triple_str(msg_triples, "foundation:stopReason");
    let input_tokens = triple_int(msg_triples, "foundation:inputTokens").map(|n| n as usize);
    let output_tokens = triple_int(msg_triples, "foundation:outputTokens").map(|n| n as usize);

    let block_iris = triple_iris(msg_triples, "foundation:hasContentBlock");
    let mut indexed_blocks: Vec<(i64, ContentBlock)> = block_iris.iter()
        .filter_map(|block_iri| {
            block_triples_map.get(block_iri)
                .and_then(|bt| parse_content_block_from_batch(bt, tool_use_triples_map))
        })
        .collect();
    indexed_blocks.sort_by_key(|(idx, _)| *idx);
    let content = indexed_blocks.into_iter().map(|(_, b)| b).collect();

    Ok(AIConversationMessage { iri: iri.to_string(), role, content, timestamp, token_count, model, stop_reason, input_tokens, output_tokens })
}

/// Load conversation history with token budget
pub async fn load_conversation_history(
    executor: &DbExecutor,
    conversation_id: &str,
    max_tokens: usize,
) -> Result<(Vec<AIConversationMessage>, usize), String> {
    super::log_backend("info", &format!(
        "[CHAT] Loading conversation history for: {}", conversation_id
    ));

    let conversation_id = conversation_id.to_string();
    let t_read_start = std::time::Instant::now();
    let selected = executor.read(move |conn| {
        let t0 = std::time::Instant::now();

        let iris_desc = crate::core_ontology::chat::find_messages_by_conversation(conn, &conversation_id, usize::MAX, 0)
            .map_err(|e| format!("Failed to query messages: {}", e))?;
        let t1 = std::time::Instant::now();

        if iris_desc.is_empty() {
            return Ok::<(Vec<AIConversationMessage>, usize), String>((Vec::new(), 0));
        }

        // Batch 1: load all message-level triples in a single query instead of N individual gets.
        let msg_triples_map = crate::eavto::query::batch_load_triples_for_subjects(conn, &iris_desc)
            .map_err(|e| format!("Failed to batch-load message triples: {}", e))?;
        let t2 = std::time::Instant::now();

        // Batch 2: collect all block IRIs from message triples, then load them all at once.
        let all_block_iris: Vec<String> = iris_desc.iter()
            .filter_map(|iri| msg_triples_map.get(iri))
            .flat_map(|triples| triple_iris(triples, "foundation:hasContentBlock"))
            .collect();

        let block_triples_map = crate::eavto::query::batch_load_triples_for_subjects(conn, &all_block_iris)
            .map_err(|e| format!("Failed to batch-load block triples: {}", e))?;
        let t3 = std::time::Instant::now();

        // Batch 3: collect all ToolUseBlock IRIs referenced by ToolResultBlocks, then load them.
        let tool_use_iris: Vec<String> = block_triples_map.values()
            .flat_map(|triples| {
                triples.iter()
                    .filter(|t| t.predicate == "anthropic:resultOf")
                    .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let tool_use_triples_map = crate::eavto::query::batch_load_triples_for_subjects(conn, &tool_use_iris)
            .map_err(|e| format!("Failed to batch-load tool-use block triples: {}", e))?;
        let t4 = std::time::Instant::now();

        super::log_backend("debug", &format!(
            "[HISTORY] msgs={} blocks={} tool_use_blocks={} | find={}ms batch1={}ms batch2={}ms batch3={}ms",
            iris_desc.len(), all_block_iris.len(), tool_use_iris.len(),
            t1.duration_since(t0).as_millis(),
            t2.duration_since(t1).as_millis(),
            t3.duration_since(t2).as_millis(),
            t4.duration_since(t3).as_millis(),
        ));

        // Compacted path: if a compaction message exists, use summary + recent messages instead
        // of the full rolling window. iris_desc is newest-first, so the first compaction IRI
        // found is the most recent compaction.
        let last_compaction_iri = iris_desc.iter().find(|iri| {
            msg_triples_map.get(*iri)
                .and_then(|t| triple_str(t, "foundation:role"))
                .as_deref() == Some("compaction")
        });

        if let Some(compaction_iri) = last_compaction_iri {
            let compaction_ts = msg_triples_map.get(compaction_iri.as_str())
                .and_then(|t| t.iter()
                    .find(|tr| tr.predicate == "foundation:sentAt")
                    .and_then(|tr| if let Object::DateTime(rfc) = &tr.object {
                        chrono::DateTime::parse_from_rfc3339(rfc).ok().map(|dt| dt.timestamp_millis())
                    } else { None }))
                .unwrap_or(0);

            // The compaction message is saved after the user's last message, so filtering by
            // compaction_ts would exclude that message. Use summaryUpToMessage's sentAt instead —
            // everything after the last summarized message is "recent".
            let summary_up_to_iri: Option<String> = crate::owl::get_iri_property(conn, &conversation_id, "foundation:summaryUpToMessage")
                .ok()
                .flatten();

            let cutoff_ts = summary_up_to_iri
                .as_deref()
                .and_then(|iri| msg_triples_map.get(iri))
                .and_then(|t| t.iter()
                    .find(|tr| tr.predicate == "foundation:sentAt")
                    .and_then(|tr| if let Object::DateTime(rfc) = &tr.object {
                        chrono::DateTime::parse_from_rfc3339(rfc).ok().map(|dt| dt.timestamp_millis())
                    } else { None }))
                .unwrap_or(compaction_ts);

            let summary_text = triple_iris(
                msg_triples_map.get(compaction_iri.as_str()).map(|v| v.as_slice()).unwrap_or_default(),
                "foundation:hasContentBlock",
            ).iter().find_map(|block_iri| {
                block_triples_map.get(block_iri)
                    .and_then(|bt| triple_str(bt, "foundation:summaryText"))
            }).unwrap_or_default();

            let summary_ts = compaction_ts.saturating_sub(1);
            let summary_msg = AIConversationMessage {
                iri: format!("synthetic:summary_{}", compaction_ts),
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: format!("[Resumo do contexto anterior]\n{}", summary_text),
                }],
                timestamp: summary_ts,
                token_count: None,
                model: None,
                stop_reason: None,
                input_tokens: None,
                output_tokens: None,
            };

            // Collect all normal messages that came AFTER the compaction message, within max_tokens.
            let recent_iris: Vec<&String> = iris_desc.iter()
                .filter(|iri| {
                    let role = msg_triples_map.get(*iri)
                        .and_then(|t| triple_str(t, "foundation:role"))
                        .unwrap_or_default();
                    if role == "compaction" { return false; }
                    let ts = msg_triples_map.get(*iri)
                        .and_then(|t| t.iter()
                            .find(|tr| tr.predicate == "foundation:sentAt")
                            .and_then(|tr| if let Object::DateTime(rfc) = &tr.object {
                                chrono::DateTime::parse_from_rfc3339(rfc).ok().map(|dt| dt.timestamp_millis())
                            } else { None }))
                        .unwrap_or(0);
                    ts > cutoff_ts
                })
                .collect();

            let mut recent_msgs: Vec<AIConversationMessage> = recent_iris.iter()
                .filter_map(|iri| {
                    msg_triples_map.get(iri.as_str()).and_then(|msg_triples| {
                        load_message_from_batch(iri, msg_triples, &block_triples_map, &tool_use_triples_map).ok()
                    })
                })
                .collect();
            recent_msgs.sort_by_key(|m| m.timestamp);

            let total_tokens = recent_msgs.iter().map(|m| m.token_count.unwrap_or(0)).sum::<usize>();
            let mut result = vec![summary_msg];
            result.extend(recent_msgs);

            super::log_backend("info", &format!(
                "[CHAT] Loaded compacted history: summary + {} recent messages ({} tokens)",
                result.len() - 1, total_tokens
            ));

            return Ok::<(Vec<AIConversationMessage>, usize), String>((result, total_tokens));
        }

        // Determine whether a message has tool_results without loading it fully.
        let msg_has_tool_results = |iri: &str| -> bool {
            msg_triples_map.get(iri)
                .map(|triples| triple_iris(triples, "foundation:hasContentBlock").iter().any(|block_iri| {
                    block_triples_map.get(block_iri)
                        .map(|bt| bt.iter().any(|t| t.predicate == "rdf:type"
                            && t.object.as_iri() == Some("anthropic:ToolResultBlock")))
                        .unwrap_or(false)
                }))
                .unwrap_or(false)
        };

        let msg_token_count = |iri: &str| -> usize {
            msg_triples_map.get(iri)
                .and_then(|triples| triple_int(triples, "foundation:tokenCount"))
                .unwrap_or(0) as usize
        };

        let mut selected_iris: Vec<String> = Vec::new();
        let mut total_tokens = 0usize;
        let mut failed_count = 0usize;
        let mut i = 0;

        while i < iris_desc.len() {
            let iri = &iris_desc[i];
            if msg_triples_map.get(iri).map(|t| t.is_empty()).unwrap_or(true) {
                failed_count += 1;
                i += 1;
                continue;
            }
            let msg_tokens = msg_token_count(iri);
            let has_tool_results = msg_has_tool_results(iri);

            if has_tool_results && i + 1 < iris_desc.len() {
                let prev_iri = &iris_desc[i + 1];
                if msg_triples_map.get(prev_iri).map(|t| t.is_empty()).unwrap_or(true) {
                    failed_count += 1;
                    if !selected_iris.is_empty() && total_tokens + msg_tokens > max_tokens { break; }
                    selected_iris.push(iri.clone());
                    total_tokens += msg_tokens;
                    i += 1;
                    continue;
                }
                let pair_tokens = msg_tokens + msg_token_count(prev_iri);
                if !selected_iris.is_empty() && total_tokens + pair_tokens > max_tokens { break; }
                selected_iris.push(iri.clone());
                selected_iris.push(prev_iri.clone());
                total_tokens += pair_tokens;
                i += 2;
            } else {
                if !selected_iris.is_empty() && total_tokens + msg_tokens > max_tokens { break; }
                selected_iris.push(iri.clone());
                total_tokens += msg_tokens;
                i += 1;
            }
        }

        super::log_backend("info", &format!(
            "[CHAT] Loaded {} messages ({} skipped, {} failed)",
            selected_iris.len(), iris_desc.len().saturating_sub(i), failed_count,
        ));

        let mut selected: Vec<AIConversationMessage> = selected_iris.iter()
            .filter_map(|iri| {
                msg_triples_map.get(iri.as_str()).and_then(|msg_triples| {
                    load_message_from_batch(iri, msg_triples, &block_triples_map, &tool_use_triples_map).ok()
                })
            })
            .collect();

        // Enrich FileRef blocks with AI summaries so the API receives text descriptions
        // instead of binary blobs for files that have already been summarised.
        for msg in &mut selected {
            for block in &mut msg.content {
                if let ContentBlock::FileRef { ref file_iri, ref mut ai_summary, .. } = block {
                    *ai_summary = crate::owl::get_literal_property(conn, file_iri, "foundation:aiSummary")
                        .ok()
                        .flatten();
                }
            }
        }

        selected.reverse();
        Ok::<(Vec<AIConversationMessage>, usize), String>((selected, total_tokens))
    }).await?;

    let t_read_ms = t_read_start.elapsed().as_millis();
    let (selected, total_tokens) = selected;
    let t_sanitize_start = std::time::Instant::now();

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
                if let Some(prev) = validated.last_mut() {
                    super::log_backend("warn", &format!(
                        "[CHAT] Stripping tool_use and thinking blocks from {} — tool_results not after",
                        prev.iri
                    ));
                    prev.content.retain(|b| match b {
                        ContentBlock::ToolUse { .. } => false,
                        ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. } => false,
                        _ => true,
                    });
                    if prev.content.is_empty() {
                        super::log_backend("warn", "[CHAT] Removed assistant message that became empty after stripping tool_use");
                        validated.pop();
                    }
                }

                // Strip all orphaned tool_results (no matching tool_use).
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
                // Strip all orphaned tool_results that have no matching tool_use in prev.
                let clean_content: Vec<ContentBlock> = msg.content.iter()
                    .filter(|b| !matches!(b, ContentBlock::ToolResult { .. }))
                    .cloned()
                    .collect();

                let had_orphan = msg.content.len() > clean_content.len();
                if had_orphan {
                    super::log_backend(
                        "warn", "[CHAT] Stripping orphaned tool_result blocks (no preceding tool_use)",
                    );
                }

                if !clean_content.is_empty() {
                    let mut clean_msg = msg;
                    clean_msg.content = clean_content;
                    validated.push(clean_msg);
                }
            } else {
                validated.push(msg);
            }
        }
    }

    let final_cleaned: Vec<AIConversationMessage> = validated
        .into_iter()
        .filter(|msg| !msg.content.is_empty())
        .collect();

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
        "[CHAT] history done: msgs_out={} tokens={}/{} read={}ms sanitize={}ms total={}ms",
        merged.len(), total_tokens, max_tokens,
        t_read_ms,
        t_sanitize_start.elapsed().as_millis(),
        t_read_start.elapsed().as_millis(),
    ));

    Ok((merged, total_tokens))
}


/// Log a single API call to the ontology as a foundation:AIAPICall entity
pub async fn log_api_call(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_tokens: u32,
    cache_read_tokens: u32,
    conversation_id: Option<&str>,
    message_iri: Option<&str>,
) -> Result<(), String> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let iri = format!("foundation:AIAPICall_{}", timestamp);
    let model = model.to_string();
    let conversation_iri = conversation_id.map(|s| s.to_string());
    let message_iri_opt = message_iri.map(|s| s.to_string());

    executor.write(move |conn| {
        // Build all triples in memory and insert in one bulk call — bypasses OWL
        // validation (add_property runs has_prop + cardinality + domain/range checks
        // per property; for a brand-new IRI every check is pure overhead, and
        // generatedByConversation validation alone cost ~250ms, causing read-pool
        // exhaustion under concurrent load).
        let lit = |value: String, datatype: &str| Object::Literal {
            value,
            datatype: Some(datatype.to_string()),
            language: None,
        };
        let called_at_dt = chrono::DateTime::from_timestamp_millis(timestamp)
            .unwrap_or_default()
            .to_rfc3339();
        let cost = estimate_call_cost(
            conn, &model, input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens,
        );

        let mut triples: Vec<Triple> = Vec::with_capacity(16);
        triples.push(Triple::new(&iri, "rdf:type", Object::Iri("foundation:AIAPICall".to_string())));
        triples.push(Triple::new(&iri, "rdfs:label", lit("AI API Call".to_string(), "xsd:string")));
        triples.push(Triple::new(&iri, "foundation:model", lit(model.clone(), "xsd:string")));
        triples.push(Triple::new(&iri, "foundation:inputTokens", Object::Integer(input_tokens as i64)));
        triples.push(Triple::new(&iri, "foundation:outputTokens", Object::Integer(output_tokens as i64)));
        triples.push(Triple::new(&iri, "foundation:cacheCreationTokens", Object::Integer(cache_creation_tokens as i64)));
        triples.push(Triple::new(&iri, "foundation:cacheReadTokens", Object::Integer(cache_read_tokens as i64)));
        triples.push(Triple::new(&iri, "foundation:calledAt", Object::DateTime(called_at_dt.clone())));
        triples.push(Triple::new(&iri, "foundation:hasStartTime", Object::DateTime(called_at_dt.clone())));
        triples.push(Triple::new(&iri, "foundation:hasEndTime", Object::DateTime(called_at_dt)));
        if let Some(conv_iri) = conversation_iri {
            triples.push(Triple::new(&iri, "foundation:generatedByConversation", Object::Iri(conv_iri)));
        }
        if let Some(msg_iri) = message_iri_opt {
            triples.push(Triple::new(&iri, "foundation:generatedMessage", Object::Iri(msg_iri)));
        }
        if let Some(c) = cost {
            triples.push(Triple::new(&iri, "foundation:estimatedCost", Object::Number(c)));
        }

        crate::owl::assert_raw_triples(conn, &triples, "ai")
            .map_err(|e| format!("Failed to save AIAPICall: {}", e))?;

        Ok(iri)
    }).await
    .map(|call_iri| {
        if conversation_id.is_some() {
            use crate::owl::formula_worker::{FormulaWorker, WorkerCommand, create_instance_recalc_jobs};
            use tauri::Manager;
            if let Some(worker) = app.try_state::<FormulaWorker>() {
                let worker_sender = worker.sender.clone();
                let executor_clone = executor.clone();
                tauri::async_runtime::spawn(async move {
                    let job_ids = executor_clone.read(move |conn| {
                        Ok(create_instance_recalc_jobs(conn, &call_iri, "foundation:generatedByConversation"))
                    }).await.unwrap_or_default();
                    for job_id in job_ids {
                        let _ = worker_sender.try_send(WorkerCommand::Enqueue { job_id });
                    }
                });
            }
        }
    })
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

pub fn calculate_content_tokens(blocks: &[ContentBlock]) -> usize {
    let bpe = match get_tokenizer() {
        Ok(b) => b,
        Err(_) => return 0,
    };

    let mut total = MESSAGE_TOKEN_OVERHEAD;

    for block in blocks {
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
            ContentBlock::QuestionOutput { question, options, .. } => {
                let opts = options.join(", ");
                bpe.encode_with_special_tokens(question).len() +
                bpe.encode_with_special_tokens(&opts).len() + TOOL_TOKEN_OVERHEAD
            },
            ContentBlock::CompactionSummary { text } => bpe.encode_with_special_tokens(text).len(),
        };
    }

    total
}
