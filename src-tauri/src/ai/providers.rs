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
        /// Plain text results are `Value::String`; image/PDF results are
        /// `Value::Array` of content-block objects (e.g. `[{"type":"image",…}]`).
        content: serde_json::Value,
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

fn add_cache_breakpoint(message: &mut ClaudeMessage) {
    let marker = serde_json::json!({"type": "ephemeral"});
    match &mut message.content {
        serde_json::Value::Array(blocks) => {
            if let Some(last) = blocks.last_mut() {
                if let Some(obj) = last.as_object_mut() {
                    obj.insert("cache_control".to_string(), marker);
                }
            }
        }
        serde_json::Value::String(text) => {
            let text_val = text.clone();
            message.content = serde_json::json!([{
                "type": "text",
                "text": text_val,
                "cache_control": {"type": "ephemeral"}
            }]);
        }
        _ => {}
    }
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

        let mut messages: Vec<ClaudeMessage> = request
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

        // System prompt + tools use 2 of the 4 available cache slots, leaving 2 for messages.
        // Place breakpoints at the two highest multiples-of-20 positions (indices 19, 39, 59, …)
        // below the last message. As the conversation grows they advance in steps of 20,
        // so the lower breakpoint is always a prior cache hit and only the upper one
        // needs to be written. The last 1–20 messages are always processed fresh.
        let msg_count = messages.len();
        let k_max = msg_count.saturating_sub(1) / 20; // highest k where index 20k-1 < last msg
        if k_max >= 2 {
            add_cache_breakpoint(&mut messages[20 * k_max - 1]);
            add_cache_breakpoint(&mut messages[20 * (k_max - 1) - 1]);
        } else if k_max == 1 {
            add_cache_breakpoint(&mut messages[19]);
        }

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
            signature: String,
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
                                        }
                                        "signature_delta" => {
                                            let sig = delta["signature"].as_str().unwrap_or("");
                                            block.signature.push_str(sig);
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
                        signature: block.signature,
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
            model_used: None,
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
            tools: request.tools.as_ref().map(|tools| {
                tools.iter().map(|t| serde_json::to_value(t).unwrap_or_default()).collect()
            }),
            tool_choice: request.tool_choice.as_ref()
                .map(|tc| serde_json::to_value(tc).unwrap_or_default()),
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

        let blocks = body["content"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);
        let content = blocks.iter()
            .find(|b| b["type"] == "text")
            .and_then(|b| b["text"].as_str())
            .unwrap_or("")
            .to_string();
        let tool_calls: Vec<ToolCall> = blocks.iter()
            .filter(|b| b["type"] == "tool_use")
            .filter_map(|b| {
                Some(ToolCall {
                    id: b["id"].as_str()?.to_string(),
                    name: b["name"].as_str()?.to_string(),
                    input: b["input"].clone(),
                })
            })
            .collect();

        let usage = body["usage"].as_object().map(|u| UsageInfo {
            input_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
            cache_creation_input_tokens: u["cache_creation_input_tokens"].as_u64().unwrap_or(0) as u32,
            cache_read_input_tokens: u["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32,
        });

        Ok(GenerateResponse {
            content,
            tool_calls,
            thinking_blocks: vec![],
            stop_reason,
            usage,
            model_used: None,
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

// ── OpenRouter provider ────────────────────────────────────────────────────────
// Uses the OpenAI-compatible Chat Completions API exposed by OpenRouter,
// keeping Foundation's internal GenerateRequest/GenerateResponse interface intact.

pub struct OpenRouterProvider {
    api_key: String,
    base_url: String,
    model_identifier: Option<String>,
    pub fallback_models: Vec<String>,
    client: reqwest::Client,
}

impl OpenRouterProvider {
    pub fn with_model(
        api_key: String,
        base_url: String,
        model_identifier: String,
        fallback_models: Vec<String>,
        timeout_secs: u64,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            base_url,
            model_identifier: Some(model_identifier),
            fallback_models,
            client,
        }
    }
}

/// Add cache_control to a message object for OpenRouter/Anthropic prompt caching.
/// Handles both string and array content formats.
fn inject_cache_control_openrouter(message: &mut serde_json::Value) {
    let marker = serde_json::json!({"type": "ephemeral"});
    match message.get_mut("content") {
        Some(serde_json::Value::String(s)) => {
            let text = s.clone();
            message["content"] = serde_json::json!([{
                "type": "text",
                "text": text,
                "cache_control": marker
            }]);
        }
        Some(serde_json::Value::Array(arr)) => {
            if let Some(last) = arr.last_mut() {
                if let Some(obj) = last.as_object_mut() {
                    obj.insert("cache_control".to_string(), marker);
                }
            }
        }
        _ => {}
    }
}

fn build_openai_messages(
    system: Option<String>,
    blackboard_context: Option<String>,
    messages: Vec<crate::ai::ChatMessage>,
) -> Vec<serde_json::Value> {
    let mut result: Vec<serde_json::Value> = Vec::new();

    let mut sys_parts: Vec<String> = Vec::new();
    if let Some(s) = system { sys_parts.push(s); }
    if let Some(ctx) = blackboard_context { sys_parts.push(ctx); }
    if !sys_parts.is_empty() {
        result.push(serde_json::json!({"role": "system", "content": sys_parts.join("\n\n")}));
    }

    for msg in messages {
        match msg.content {
            MessageContent::Text(text) => {
                result.push(serde_json::json!({"role": msg.role, "content": text}));
            }
            MessageContent::ContentBlocks(blocks) if msg.role == "user" => {
                let mut text_parts: Vec<serde_json::Value> = Vec::new();
                for block in blocks {
                    match block {
                        ContentBlock::ToolResult { tool_use_id, content, .. } => {
                            let text = match &content {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            result.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": tool_use_id,
                                "content": text
                            }));
                        }
                        ContentBlock::Text { text } => {
                            text_parts.push(serde_json::json!({"type": "text", "text": text}));
                        }
                        _ => {}
                    }
                }
                if !text_parts.is_empty() {
                    let val = if text_parts.len() == 1 {
                        text_parts[0]["text"].clone()
                    } else {
                        serde_json::Value::Array(text_parts)
                    };
                    result.push(serde_json::json!({"role": "user", "content": val}));
                }
            }
            MessageContent::ContentBlocks(blocks) if msg.role == "assistant" => {
                let mut text_parts: Vec<String> = Vec::new();
                let mut tool_calls: Vec<serde_json::Value> = Vec::new();
                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => text_parts.push(text),
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_calls.push(serde_json::json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(&input).unwrap_or_default()
                                }
                            }));
                        }
                        _ => {}
                    }
                }
                let mut obj = serde_json::json!({"role": "assistant"});
                if !text_parts.is_empty() {
                    obj["content"] = serde_json::json!(text_parts.join("\n"));
                } else if !tool_calls.is_empty() {
                    // OpenAI/OpenRouter requires content to be empty string (not null) when there are tool_calls
                    obj["content"] = serde_json::json!("");
                } else {
                    obj["content"] = serde_json::Value::Null;
                }
                if !tool_calls.is_empty() {
                    obj["tool_calls"] = serde_json::json!(tool_calls);
                }
                result.push(obj);
            }
            MessageContent::ContentBlocks(blocks) => {
                result.push(serde_json::json!({
                    "role": msg.role,
                    "content": serde_json::to_value(&blocks).unwrap_or(serde_json::Value::Null)
                }));
            }
        }
    }
    result
}

fn build_openai_tools(tools: &[ClaudeTool]) -> Vec<serde_json::Value> {
    tools.iter().map(|t| serde_json::json!({
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description,
            "parameters": t.input_schema
        }
    })).collect()
}

impl OpenRouterProvider {
    pub async fn generate_stream(
        &self,
        request: crate::ai::GenerateRequest,
        app: &tauri::AppHandle,
        conversation_id: &str,
    ) -> Result<crate::ai::GenerateResponse, String> {
        use tauri::Emitter;

        let model = self.model_identifier.clone()
            .ok_or_else(|| "No AI model configured. Please configure a model in Settings.".to_string())?;

        let max_tokens = request.max_tokens;
        let temperature = request.temperature;
        let tool_choice = request.tool_choice;
        let tools = request.tools;
        let mut messages = build_openai_messages(request.system, request.blackboard_context, request.messages);

        // For Anthropic models via OpenRouter, add cache_control to the first user message
        // (the compaction summary prefix when present) to enable prompt caching.
        if model.starts_with("anthropic/") {
            if let Some(first_user) = messages.iter_mut().find(|m| m["role"] == "user") {
                inject_cache_control_openrouter(first_user);
            }
        }

        let mut body = serde_json::json!({
            "messages": messages,
            "max_tokens": max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            "stream": true,
            "stream_options": {"include_usage": true}
        });

        if !self.fallback_models.is_empty() {
            let mut all_models = vec![model];
            all_models.extend(self.fallback_models.clone());
            body["models"] = serde_json::json!(all_models);
        } else {
            body["model"] = serde_json::json!(model);
        }

        if let Some(temp) = temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        if let Some(ref tool_list) = tools.as_ref().filter(|t| !t.is_empty()) {
            body["tools"] = serde_json::json!(build_openai_tools(tool_list));
            body["tool_choice"] = tool_choice.unwrap_or(serde_json::json!("auto"));
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        crate::commands::log_backend("info", "[OPENROUTER API] Sending streaming request...");

        let http_response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://foundation.w3id.org")
            .header("X-OpenRouter-Title", "FOUNDATION")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to send OpenRouter streaming request: {}", e))?;

        let status = http_response.status();
        if !status.is_success() {
            let error_body = http_response.text().await.unwrap_or_default();

            // Parse OpenRouter error for better user messages
            let friendly_error = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&error_body) {
                if let Some(raw) = json["error"]["metadata"]["raw"].as_str() {
                    // Extract the actual provider error message
                    raw.to_string()
                } else if let Some(msg) = json["error"]["message"].as_str() {
                    msg.to_string()
                } else {
                    error_body.clone()
                }
            } else {
                error_body.clone()
            };

            // Provide helpful context for common errors
            let user_message = match status.as_u16() {
                429 => {
                    if friendly_error.contains("rate-limited") || friendly_error.contains("rate limit") {
                        format!("Limite de requisições atingido: {}\n\nTente novamente em alguns segundos, ou configure sua própria chave de API nas configurações.", friendly_error)
                    } else {
                        format!("Muitas requisições (429): {}", friendly_error)
                    }
                },
                401 => format!("Chave de API inválida ou expirada. Verifique suas configurações.\n\nDetalhes: {}", friendly_error),
                403 => format!("Acesso negado. Verifique suas permissões ou chave de API.\n\nDetalhes: {}", friendly_error),
                500 | 502 | 503 | 504 => format!("Erro no servidor do provedor ({}). Tente novamente em alguns minutos.\n\nDetalhes: {}", status, friendly_error),
                _ => format!("Erro {} do OpenRouter: {}", status, friendly_error),
            };

            return Err(user_message);
        }

        struct ToolCallState {
            id: String,
            name: String,
            arguments: String,
        }

        let mut text_content = String::new();
        let mut tool_states: Vec<ToolCallState> = Vec::new();
        let mut stop_reason: Option<String> = None;
        let mut usage: Option<UsageInfo> = None;
        let mut actual_model: Option<String> = None;
        let mut buf = String::new();
        let conv_id = conversation_id.to_string();

        let mut stream = http_response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("Stream read error: {}", e))?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|e| format!("Stream UTF-8 error: {}", e))?;
            buf.push_str(text);

            loop {
                match buf.find('\n') {
                    None => break,
                    Some(pos) => {
                        let line = buf[..pos].trim_end_matches('\r').to_string();
                        buf = buf[pos + 1..].to_string();

                        if !line.starts_with("data: ") { continue; }
                        let data = &line["data: ".len()..];
                        if data == "[DONE]" { break; }

                        let event: serde_json::Value = match serde_json::from_str(data) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        if actual_model.is_none() {
                            if let Some(m) = event["model"].as_str() {
                                actual_model = Some(m.to_string());
                            }
                        }

                        if let Some(u) = event.get("usage").filter(|v| !v.is_null()) {
                            usage = Some(UsageInfo {
                                input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                                output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                                cache_creation_input_tokens: 0,
                                cache_read_input_tokens: 0,
                            });
                        }

                        let choices = match event["choices"].as_array() {
                            Some(c) if !c.is_empty() => c,
                            _ => continue,
                        };

                        let choice = &choices[0];
                        if let Some(fr) = choice["finish_reason"].as_str() {
                            if !fr.is_empty() && fr != "null" {
                                stop_reason = Some(fr.to_string());
                            }
                        }

                        let delta = &choice["delta"];
                        if let Some(chunk_text) = delta["content"].as_str() {
                            if !chunk_text.is_empty() {
                                text_content.push_str(chunk_text);
                                app.emit("chat-ai-delta", serde_json::json!({
                                    "conversationId": conv_id,
                                    "type": "text",
                                    "text": chunk_text,
                                })).ok();
                            }
                        }

                        if let Some(tc_deltas) = delta["tool_calls"].as_array() {
                            for tc in tc_deltas {
                                let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                                if tool_states.len() <= idx {
                                    tool_states.resize_with(idx + 1, || ToolCallState {
                                        id: String::new(),
                                        name: String::new(),
                                        arguments: String::new(),
                                    });
                                }
                                let state = &mut tool_states[idx];
                                if let Some(id) = tc["id"].as_str() { state.id = id.to_string(); }
                                if let Some(name) = tc["function"]["name"].as_str() { state.name = name.to_string(); }
                                if let Some(args) = tc["function"]["arguments"].as_str() { state.arguments.push_str(args); }
                            }
                        }
                    }
                }
            }
        }

        crate::commands::log_backend("info", &format!(
            "[OPENROUTER API] Stream complete. model={}, stop_reason={:?}",
            actual_model.as_deref().unwrap_or("unknown"), stop_reason
        ));
        if let Some(ref u) = usage {
            crate::commands::log_backend("info", &format!(
                "[OPENROUTER API] Tokens — input: {}, output: {}", u.input_tokens, u.output_tokens
            ));
        }

        let tool_calls: Vec<crate::ai::ToolCall> = tool_states.into_iter()
            .filter(|s| !s.name.is_empty())
            .map(|s| {
                let input = serde_json::from_str(&s.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                crate::ai::ToolCall { id: s.id, name: s.name, input }
            })
            .collect();

        Ok(crate::ai::GenerateResponse {
            content: text_content,
            tool_calls,
            thinking_blocks: vec![],
            stop_reason,
            usage,
            model_used: actual_model,
        })
    }

    pub async fn generate(&self, request: crate::ai::GenerateRequest) -> Result<crate::ai::GenerateResponse, String> {
        let model = self.model_identifier.clone()
            .ok_or_else(|| "No AI model configured".to_string())?;

        let max_tokens = request.max_tokens;
        let temperature = request.temperature;
        let tool_choice = request.tool_choice;
        let tools = request.tools;
        let messages = build_openai_messages(request.system, request.blackboard_context, request.messages);

        let mut body = serde_json::json!({
            "messages": messages,
            "max_tokens": max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        });

        if !self.fallback_models.is_empty() {
            let mut all_models = vec![model];
            all_models.extend(self.fallback_models.clone());
            body["models"] = serde_json::json!(all_models);
        } else {
            body["model"] = serde_json::json!(model);
        }

        if let Some(temp) = temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        if let Some(ref tool_list) = tools.as_ref().filter(|t| !t.is_empty()) {
            body["tools"] = serde_json::json!(build_openai_tools(tool_list));
            body["tool_choice"] = tool_choice.unwrap_or(serde_json::json!("auto"));
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let http_response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://foundation.w3id.org")
            .header("X-OpenRouter-Title", "FOUNDATION")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to send OpenRouter request: {}", e))?;

        let status = http_response.status();
        if !status.is_success() {
            let error_body = http_response.text().await.unwrap_or_default();

            // Parse OpenRouter error for better user messages
            let friendly_error = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&error_body) {
                if let Some(raw) = json["error"]["metadata"]["raw"].as_str() {
                    raw.to_string()
                } else if let Some(msg) = json["error"]["message"].as_str() {
                    msg.to_string()
                } else {
                    error_body.clone()
                }
            } else {
                error_body.clone()
            };

            let user_message = match status.as_u16() {
                429 => {
                    if friendly_error.contains("rate-limited") || friendly_error.contains("rate limit") {
                        format!("Limite de requisições atingido: {}\n\nTente novamente em alguns segundos, ou configure sua própria chave de API nas configurações.", friendly_error)
                    } else {
                        format!("Muitas requisições (429): {}", friendly_error)
                    }
                },
                401 => format!("Chave de API inválida ou expirada. Verifique suas configurações.\n\nDetalhes: {}", friendly_error),
                403 => format!("Acesso negado. Verifique suas permissões ou chave de API.\n\nDetalhes: {}", friendly_error),
                500 | 502 | 503 | 504 => format!("Erro no servidor do provedor ({}). Tente novamente em alguns minutos.\n\nDetalhes: {}", status, friendly_error),
                _ => format!("Erro {} do OpenRouter: {}", status, friendly_error),
            };

            return Err(user_message);
        }

        let resp: serde_json::Value = http_response.json()
            .await
            .map_err(|e| format!("Failed to parse OpenRouter response: {}", e))?;

        let stop_reason = resp["choices"][0]["finish_reason"].as_str().map(String::from);
        let content = resp["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
        let model_used = resp["model"].as_str().map(String::from);

        let tool_calls: Vec<crate::ai::ToolCall> = resp["choices"][0]["message"]["tool_calls"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|tc| {
                Some(crate::ai::ToolCall {
                    id: tc["id"].as_str()?.to_string(),
                    name: tc["function"]["name"].as_str()?.to_string(),
                    input: serde_json::from_str(
                        tc["function"]["arguments"].as_str().unwrap_or("{}")
                    ).unwrap_or(serde_json::Value::Object(Default::default())),
                })
            }).collect())
            .unwrap_or_default();

        let usage = resp.get("usage").and_then(|u| u.as_object()).map(|u| UsageInfo {
            input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        });

        Ok(crate::ai::GenerateResponse {
            content,
            tool_calls,
            thinking_blocks: vec![],
            stop_reason,
            usage,
            model_used,
        })
    }
}

