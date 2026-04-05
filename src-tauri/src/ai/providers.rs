use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use crate::ai::{GenerateRequest, GenerateResponse, ToolCall};

const WEB_TOOL_MAX_USES: u32 = 5;
const WEB_FETCH_MAX_CONTENT_TOKENS: u32 = 100_000;
const CLAUDE_CACHE_READ_PRICE_PER_MILLION_TOKENS: f64 = 2.70;
const DEFAULT_MAX_TOKENS: u32 = 4096;

pub struct ClaudeProvider {
    api_key: String,
    client: reqwest::Client,
    model_identifier: Option<String>,
}

#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    ContentBlocks(Vec<ContentBlock>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        source: ImageSource,
    },
    #[serde(rename = "document")]
    Document {
        source: DocumentSource,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        signature: String,
    },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking {
        data: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DocumentSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClaudeMessage {
    role: String,
    content: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UsageInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

impl ClaudeProvider {
    pub fn new(api_key: String, timeout_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            api_key,
            client,
            model_identifier: None,
        }
    }

    pub fn with_model(api_key: String, model_identifier: String, timeout_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            api_key,
            client,
            model_identifier: Some(model_identifier),
        }
    }
}

fn extract_speak_text(json: &str) -> &str {
    for prefix in &["\"message\":\"", "\"message\": \""] {
        if let Some(pos) = json.find(prefix) {
            let after = &json[pos + prefix.len()..];
            let bytes = after.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2; // skip JSON escape sequence
                } else if bytes[i] == b'"' {
                    return &after[..i]; // found closing quote
                } else {
                    i += 1;
                }
            }
            return after; // partial — no closing quote yet
        }
    }
    ""
}

impl ClaudeProvider {
    /// Stream a request via SSE, emitting Tauri events for each delta.
    pub async fn generate_stream(
        &self,
        request: GenerateRequest,
        app: &tauri::AppHandle,
        conversation_id: &str,
    ) -> Result<GenerateResponse, String> {
        use tauri::Emitter;

        let messages: Vec<ClaudeMessage> = request
            .messages
            .into_iter()
            .map(|msg| {
                let content = match msg.content {
                    MessageContent::Text(text) => serde_json::Value::String(text),
                    MessageContent::ContentBlocks(blocks) => {
                        serde_json::to_value(&blocks).unwrap_or(serde_json::Value::Null)
                    }
                };
                ClaudeMessage { role: msg.role, content }
            })
            .collect();

        let model = self.model_identifier.clone()
            .ok_or_else(|| "No AI model configured. Please configure a model in Settings.".to_string())?;

        let tools = if let Some(custom_tools) = request.tools {
            let mut tools: Vec<serde_json::Value> = custom_tools.into_iter()
                .map(|t| serde_json::to_value(t).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;

            if request.supports_web_tools {
                tools.push(serde_json::json!({
                    "type": "web_search_20260209",
                    "name": "web_search",
                    "max_uses": WEB_TOOL_MAX_USES
                }));
                tools.push(serde_json::json!({
                    "type": "web_fetch_20260209",
                    "name": "web_fetch",
                    "max_uses": WEB_TOOL_MAX_USES,
                    "max_content_tokens": WEB_FETCH_MAX_CONTENT_TOKENS,
                    "citations": { "enabled": true }
                }));
            }

            if let Some(last_tool) = tools.last_mut() {
                if let Some(obj) = last_tool.as_object_mut() {
                    obj.insert("cache_control".to_string(), serde_json::json!({ "type": "ephemeral" }));
                }
            }
            Some(tools)
        } else {
            None
        };

        let thinking = request.thinking.as_ref().map(|t|
            serde_json::to_value(t).unwrap_or(serde_json::Value::Null)
        );

        let mut claude_request = serde_json::to_value(ClaudeRequest {
            model,
            max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages,
            thinking,
            system: {
                let mut blocks: Vec<serde_json::Value> = Vec::new();
                if let Some(s) = request.system {
                    blocks.push(serde_json::json!({
                        "type": "text",
                        "text": s,
                        "cache_control": { "type": "ephemeral" }
                    }));
                }
                if let Some(ctx) = request.blackboard_context {
                    blocks.push(serde_json::json!({ "type": "text", "text": ctx }));
                }
                if blocks.is_empty() { None } else { Some(serde_json::Value::Array(blocks)) }
            },
            temperature: request.temperature,
            tools,
            tool_choice: request.tool_choice,
        }).map_err(|e| format!("Failed to serialize request: {}", e))?;

        claude_request["stream"] = serde_json::Value::Bool(true);

        crate::commands::log_backend("info", "[CLAUDE API] Sending streaming request...");

        let beta_header = if request.supports_web_tools {
            "prompt-caching-2024-07-31,code-execution-web-tools-2026-02-09"
        } else {
            "prompt-caching-2024-07-31"
        };

        let http_response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", beta_header)
            .header("content-type", "application/json")
            .json(&claude_request)
            .send()
            .await
            .map_err(|e| format!("Failed to send streaming request: {}", e))?;

        let status = http_response.status();
        if !status.is_success() {
            let error_text = http_response.text().await.unwrap_or_default();
            return Err(format!("API request failed with status {}: {}", status, error_text));
        }

        // SSE parser state
        #[derive(Default)]
        struct BlockState {
            block_type: String,   // "text" | "thinking" | "tool_use"
            text: String,
            tool_id: String,
            tool_name: String,
            tool_input_json: String,
            speak_text_emitted: usize,
        }

        let mut blocks: Vec<BlockState> = Vec::new();
        let mut stop_reason: Option<String> = None;
        let mut usage: Option<UsageInfo> = None;
        let mut buf = String::new();
        let conv_id = conversation_id.to_string();

        let mut stream = http_response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("Stream read error: {}", e))?;
            let text = std::str::from_utf8(&chunk)
                .map_err(|e| format!("Stream UTF-8 error: {}", e))?;
            buf.push_str(text);

            // Process all complete lines in the buffer
            loop {
                match buf.find('\n') {
                    None => break,
                    Some(pos) => {
                        let line = buf[..pos].trim_end_matches('\r').to_string();
                        buf = buf[pos + 1..].to_string();

                        if !line.starts_with("data: ") {
                            continue;
                        }
                        let json_str = &line["data: ".len()..];
                        if json_str == "[DONE]" {
                            break;
                        }

                        let event: serde_json::Value = match serde_json::from_str(json_str) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        match event["type"].as_str().unwrap_or("") {
                            "content_block_start" => {
                                let idx = event["index"].as_u64().unwrap_or(0) as usize;
                                let cb = &event["content_block"];
                                let block_type = cb["type"].as_str().unwrap_or("").to_string();
                                let mut state = BlockState {
                                    block_type: block_type.clone(),
                                    ..Default::default()
                                };
                                if block_type == "tool_use" {
                                    state.tool_id = cb["id"].as_str().unwrap_or("").to_string();
                                    state.tool_name = cb["name"].as_str().unwrap_or("").to_string();
                                }
                                if blocks.len() <= idx {
                                    blocks.resize_with(idx + 1, BlockState::default);
                                }
                                blocks[idx] = state;
                            }

                            "content_block_delta" => {
                                let idx = event["index"].as_u64().unwrap_or(0) as usize;
                                let delta = &event["delta"];
                                let delta_type = delta["type"].as_str().unwrap_or("");

                                if let Some(block) = blocks.get_mut(idx) {
                                    match delta_type {
                                        "text_delta" => {
                                            let chunk_text = delta["text"].as_str().unwrap_or("");
                                            block.text.push_str(chunk_text);
                                            app.emit("chat-ai-delta", serde_json::json!({
                                                "conversationId": conv_id,
                                                "type": "text",
                                                "text": chunk_text,
                                            })).ok();
                                        }
                                        "thinking_delta" => {
                                            let chunk_text = delta["thinking"].as_str().unwrap_or("");
                                            block.text.push_str(chunk_text);
                                            app.emit("chat-ai-delta", serde_json::json!({
                                                "conversationId": conv_id,
                                                "type": "thinking",
                                                "text": chunk_text,
                                            })).ok();
                                        }
                                        "input_json_delta" => {
                                            let partial = delta["partial_json"].as_str().unwrap_or("");
                                            block.tool_input_json.push_str(partial);
                                            if block.tool_name == "speak" {
                                                let full_text = extract_speak_text(&block.tool_input_json);
                                                crate::commands::log_backend("debug", &format!(
                                                    "[STREAM] speak delta conv={} full_text_len={} emitted={}",
                                                    conv_id, full_text.len(), block.speak_text_emitted
                                                ));
                                                if full_text.len() > block.speak_text_emitted {
                                                    let new_text = &full_text[block.speak_text_emitted..];
                                                    let unescaped = new_text
                                                        .replace("\\n", "\n")
                                                        .replace("\\t", "\t")
                                                        .replace("\\\"", "\"")
                                                        .replace("\\\\", "\\");
                                                    if !unescaped.is_empty() {
                                                        block.speak_text_emitted = full_text.len();
                                                        crate::commands::log_backend("debug", &format!(
                                                            "[STREAM] emitting speak delta: {:?}", &unescaped[..unescaped.len().min(40)]
                                                        ));
                                                        app.emit("chat-ai-delta", serde_json::json!({
                                                            "conversationId": conv_id,
                                                            "type": "speak",
                                                            "text": unescaped,
                                                        })).ok();
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            "message_delta" => {
                                stop_reason = event["delta"]["stop_reason"]
                                    .as_str()
                                    .map(String::from);
                                if let Ok(u) = serde_json::from_value::<UsageInfo>(event["usage"].clone()) {
                                    usage = Some(u);
                                }
                            }

                            "message_start" => {
                                if let Ok(u) = serde_json::from_value::<UsageInfo>(
                                    event["message"]["usage"].clone()
                                ) {
                                    usage = Some(u);
                                }
                            }

                            _ => {}
                        }
                    }
                }
            }
        }

        crate::commands::log_backend("info", &format!(
            "[CLAUDE API] Stream complete. stop_reason={:?}", stop_reason
        ));

        if let Some(ref u) = usage {
            let cache_savings = if u.cache_read_input_tokens > 0 {
                format!(" | cache hit: {} tokens (~${:.4} saved)",
                    u.cache_read_input_tokens,
                    (u.cache_read_input_tokens as f64 / 1_000_000.0) * CLAUDE_CACHE_READ_PRICE_PER_MILLION_TOKENS,
                )
            } else { String::new() };
            crate::commands::log_backend("info", &format!(
                "[CLAUDE API] Tokens — input: {}, output: {}, cache_write: {}, cache_read: {}{}",
                u.input_tokens, u.output_tokens,
                u.cache_creation_input_tokens, u.cache_read_input_tokens, cache_savings,
            ));
        }

        // Assemble GenerateResponse from block states
        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut thinking_blocks: Vec<crate::ai::ThinkingBlock> = Vec::new();

        for block in blocks {
            match block.block_type.as_str() {
                "text" => {
                    if !block.text.is_empty() {
                        text_parts.push(block.text);
                    }
                }
                "thinking" => {
                    crate::commands::log_backend("debug", "[CLAUDE API] Received thinking block");
                    thinking_blocks.push(crate::ai::ThinkingBlock::Thinking {
                        thinking: block.text,
                        signature: String::new(),
                    });
                }
                "tool_use" => {
                    let input: serde_json::Value = serde_json::from_str(&block.tool_input_json)
                        .unwrap_or(serde_json::Value::Object(Default::default()));
                    tool_calls.push(ToolCall {
                        id: block.tool_id,
                        name: block.tool_name,
                        input,
                    });
                }
                _ => {}
            }
        }

        Ok(GenerateResponse {
            content: text_parts.join("\n"),
            tool_calls,
            thinking_blocks,
            stop_reason,
            usage,
        })
    }

    /// Non-streaming request for integration tests and simple single-turn use cases.
    pub async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, String> {
        let messages: Vec<ClaudeMessage> = request
            .messages
            .into_iter()
            .map(|msg| {
                let content = match msg.content {
                    MessageContent::Text(text) => serde_json::Value::String(text),
                    MessageContent::ContentBlocks(blocks) => {
                        serde_json::to_value(&blocks).unwrap_or(serde_json::Value::Null)
                    }
                };
                ClaudeMessage { role: msg.role, content }
            })
            .collect();

        let model = self.model_identifier.clone()
            .ok_or_else(|| "No AI model configured".to_string())?;

        let claude_request = serde_json::to_value(ClaudeRequest {
            model,
            max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages,
            thinking: None,
            system: request.system.map(|s| serde_json::json!([{
                "type": "text",
                "text": s,
                "cache_control": { "type": "ephemeral" }
            }])),
            temperature: request.temperature,
            tools: None,
            tool_choice: None,
        }).map_err(|e| format!("Failed to serialize request: {}", e))?;

        let http_response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .header("content-type", "application/json")
            .json(&claude_request)
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        let status = http_response.status();
        if !status.is_success() {
            let error_text = http_response.text().await.unwrap_or_default();
            return Err(format!("API request failed with status {}: {}", status, error_text));
        }

        let body: serde_json::Value = http_response.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let stop_reason = body["stop_reason"].as_str().map(String::from);
        let content = body["content"]
            .as_array()
            .and_then(|blocks| blocks.iter()
                .find(|b| b["type"] == "text")
                .and_then(|b| b["text"].as_str())
            )
            .unwrap_or("")
            .to_string();

        let usage = body["usage"].as_object().map(|u| UsageInfo {
            input_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
            cache_creation_input_tokens: u["cache_creation_input_tokens"].as_u64().unwrap_or(0) as u32,
            cache_read_input_tokens: u["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32,
        });

        Ok(GenerateResponse {
            content,
            tool_calls: vec![],
            thinking_blocks: vec![],
            stop_reason,
            usage,
        })
    }
}

#[allow(dead_code)]
pub struct OpenAIProvider {
    api_key: String,
    client: reqwest::Client,
}

impl OpenAIProvider {
    #[allow(dead_code)]
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[allow(dead_code)]
pub struct GeminiProvider {
    api_key: String,
    client: reqwest::Client,
}

impl GeminiProvider {
    #[allow(dead_code)]
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

