use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::ai::{GenerateRequest, ChatMessage};

#[async_trait]
pub trait AIProvider: Send + Sync {
    async fn generate(&self, request: GenerateRequest) -> Result<String, String>;
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
}

#[derive(Debug, Serialize, Deserialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Debug, Deserialize)]
struct ClaudeContent {
    text: String,
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
    async fn generate(&self, request: GenerateRequest) -> Result<String, String> {
        let messages: Vec<ClaudeMessage> = request
            .messages
            .into_iter()
            .map(|msg| ClaudeMessage {
                role: msg.role,
                content: msg.content,
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
        };

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&claude_request)
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("API request failed with status {}: {}", status, error_text));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        claude_response
            .content
            .first()
            .map(|c| c.text.clone())
            .ok_or_else(|| "No content in response".to_string())
    }
}

// Placeholder for future providers
pub struct OpenAIProvider {
    api_key: String,
    client: reqwest::Client,
}

impl OpenAIProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AIProvider for OpenAIProvider {
    async fn generate(&self, _request: GenerateRequest) -> Result<String, String> {
        Err("OpenAI provider not yet implemented".to_string())
    }
}

// Placeholder for Gemini
pub struct GeminiProvider {
    api_key: String,
    client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AIProvider for GeminiProvider {
    async fn generate(&self, _request: GenerateRequest) -> Result<String, String> {
        Err("Gemini provider not yet implemented".to_string())
    }
}
