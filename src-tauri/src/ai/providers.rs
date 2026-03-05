use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::ai::{GenerateRequest, GenerateResponse, ToolCall};

const WEB_TOOL_MAX_USES: u32 = 5;
const WEB_FETCH_MAX_CONTENT_TOKENS: u32 = 100_000;
const CLAUDE_CACHE_READ_PRICE_PER_MILLION_TOKENS: f64 = 2.70;

#[async_trait]
pub trait AIProvider: Send + Sync {
    async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, String>;
}

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

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ResponseContentBlock>,
    stop_reason: Option<String>,
    usage: Option<UsageInfo>,
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

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ResponseContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        name: String,
        #[allow(dead_code)]
        input: serde_json::Value,
    },
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult {
        tool_use_id: String,
        #[allow(dead_code)]
        content: serde_json::Value,
    },
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult {
        tool_use_id: String,
        #[allow(dead_code)]
        content: serde_json::Value,
    },
    #[serde(other)]
    Other,
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

#[async_trait]
impl AIProvider for ClaudeProvider {
    async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, String> {
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
                ClaudeMessage {
                    role: msg.role,
                    content,
                }
            })
            .collect();

        let model = self.model_identifier.clone()
            .ok_or_else(|| {
                "No AI model configured. Please configure a model in Settings.".to_string()
            })?;

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
                    "citations": {
                        "enabled": true
                    }
                }));
            }

            if let Some(last_tool) = tools.last_mut() {
                if let Some(obj) = last_tool.as_object_mut() {
                    obj.insert(
                        "cache_control".to_string(),
                        serde_json::json!({ "type": "ephemeral" }),
                    );
                }
            }

            Some(tools)
        } else {
            None
        };

        let claude_request = ClaudeRequest {
            model,
            max_tokens: request.max_tokens.unwrap_or(4096),
            messages,
            system: request.system.map(|s| {
                serde_json::json!([{
                    "type": "text",
                    "text": s,
                    "cache_control": { "type": "ephemeral" }
                }])
            }),
            temperature: request.temperature,
            tools,
        };

        let api_start = std::time::Instant::now();
        crate::commands::log_backend("info", "[CLAUDE API] Sending request to Claude API...");

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header(
                "anthropic-beta",
                if request.supports_web_tools {
                    "prompt-caching-2024-07-31,code-execution-web-tools-2026-02-09"
                } else {
                    "prompt-caching-2024-07-31"
                },
            )
            .header("content-type", "application/json")
            .json(&claude_request)
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        let api_elapsed = api_start.elapsed();
        let msg = format!("[CLAUDE API] Request completed in {:?}", api_elapsed);
        crate::commands::log_backend("info", &msg);

        let status = response.status();
        if !status.is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("API request failed with status {}: {}", status, error_text));
        }

        let response_text = response.text().await
            .map_err(|e| format!("Failed to read response text: {}", e))?;

        crate::commands::log_backend("debug", &format!(
            "[CLAUDE API] Response preview: {}...",
            &response_text.chars().take(500).collect::<String>()
        ));

        let claude_response: ClaudeResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                crate::commands::log_backend("error", &format!(
                    "[CLAUDE API] Parse error details: {} at line {} column {}",
                    e, e.line(), e.column()
                ));
                crate::commands::log_backend("error", &format!(
                    "[CLAUDE API] Full response: {}",
                    &response_text
                ));
                format!("Failed to parse response: {} - Preview: {}...", e,
                    &response_text.chars().take(500).collect::<String>())
            })?;

        if let Some(ref stop_reason) = claude_response.stop_reason {
            let msg = format!("[CLAUDE API] Stop reason: {}", stop_reason);
            crate::commands::log_backend("info", &msg);
        }

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for content_block in claude_response.content {
            match content_block {
                ResponseContentBlock::Text { text } => {
                    text_parts.push(text);
                }
                ResponseContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall { id, name, input });
                }
                ResponseContentBlock::ServerToolUse { id, name, .. } => {
                    crate::commands::log_backend("debug", &format!(
                        "[CLAUDE API] Ignoring server tool use: {} ({})", name, id
                    ));
                }
                ResponseContentBlock::WebSearchToolResult { tool_use_id, .. } => {
                    crate::commands::log_backend("debug", &format!(
                        "[CLAUDE API] Received web_search_tool_result for {}", tool_use_id
                    ));
                }
                ResponseContentBlock::WebFetchToolResult { tool_use_id, .. } => {
                    crate::commands::log_backend("debug", &format!(
                        "[CLAUDE API] Received web_fetch_tool_result for {}", tool_use_id
                    ));
                }
                ResponseContentBlock::Other => {
                    crate::commands::log_backend(
                        "debug",
                        "[CLAUDE API] Ignoring unknown content block type",
                    );
                }
            }
        }

        let content = text_parts.join("\n");

        if let Some(ref usage) = claude_response.usage {
            let cache_savings = if usage.cache_read_input_tokens > 0 {
                format!(
                    " | cache hit: {} tokens (~${:.4} saved)",
                    usage.cache_read_input_tokens,
                    (usage.cache_read_input_tokens as f64 / 1_000_000.0) * CLAUDE_CACHE_READ_PRICE_PER_MILLION_TOKENS
                )
            } else {
                String::new()
            };
            crate::commands::log_backend("info", &format!(
                "[CLAUDE API] Tokens — input: {}, output: {}, cache_write: {}, cache_read: {}{}",
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
                cache_savings,
            ));
        }

        let msg = format!(
            "[CLAUDE API] Response content length: {} chars, {} tool calls",
            content.len(),
            tool_calls.len(),
        );
        crate::commands::log_backend("info", &msg);

        Ok(GenerateResponse {
            content,
            tool_calls,
            stop_reason: claude_response.stop_reason,
            usage: claude_response.usage,
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

#[async_trait]
impl AIProvider for OpenAIProvider {
    async fn generate(&self, _request: GenerateRequest) -> Result<GenerateResponse, String> {
        Err("OpenAI provider not yet implemented".to_string())
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

#[async_trait]
impl AIProvider for GeminiProvider {
    async fn generate(&self, _request: GenerateRequest) -> Result<GenerateResponse, String> {
        Err("Gemini provider not yet implemented".to_string())
    }
}
