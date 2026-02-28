use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::ai::{GenerateRequest, GenerateResponse, ToolCall};

#[async_trait]
pub trait AIProvider: Send + Sync {
    async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, String>;
}

// Claude provider implementation
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
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ClaudeTool>>,
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
    pub source_type: String, // "base64"
    pub media_type: String,   // "image/png", "image/jpeg", "image/webp", "image/gif"
    pub data: String,         // base64-encoded image data
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DocumentSource {
    #[serde(rename = "type")]
    pub source_type: String, // "base64"
    pub media_type: String,   // "application/pdf"
    pub data: String,         // base64-encoded document data
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
}

impl ClaudeProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
            model_identifier: None,
        }
    }

    pub fn with_model(api_key: String, model_identifier: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
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

        // Use provided model identifier or fallback to default
        let model = self.model_identifier.clone()
            .unwrap_or_else(|| "claude-3-5-sonnet-20240620".to_string());

        let claude_request = ClaudeRequest {
            model,
            max_tokens: request.max_tokens.unwrap_or(4096),
            messages,
            system: request.system,
            temperature: request.temperature,
            tools: request.tools,
        };

        let api_start = std::time::Instant::now();
        crate::commands::log_backend("info", "[CLAUDE API] Sending request to Claude API...");

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&claude_request)
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        let api_elapsed = api_start.elapsed();
        crate::commands::log_backend("info", &format!("[CLAUDE API] Request completed in {:?}", api_elapsed));

        let status = response.status();
        if !status.is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("API request failed with status {}: {}", status, error_text));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Log stop reason if present
        if let Some(ref stop_reason) = claude_response.stop_reason {
            crate::commands::log_backend("info", &format!("[CLAUDE API] Stop reason: {}", stop_reason));
        }

        // Extract text content and tool calls
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
            }
        }

        let content = text_parts.join("\n");

        // Log usage if present
        if let Some(ref usage) = claude_response.usage {
            crate::commands::log_backend("info", &format!(
                "[CLAUDE API] Usage - Input: {} tokens, Output: {} tokens, Cache Creation: {}, Cache Read: {}",
                usage.input_tokens, usage.output_tokens, usage.cache_creation_input_tokens, usage.cache_read_input_tokens
            ));
        }

        crate::commands::log_backend("info", &format!("[CLAUDE API] Response content length: {} chars, {} tool calls", content.len(), tool_calls.len()));

        Ok(GenerateResponse {
            content,
            tool_calls,
            stop_reason: claude_response.stop_reason,
            usage: claude_response.usage,
        })
    }
}

// Placeholder for future providers
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

// Placeholder for Gemini
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
