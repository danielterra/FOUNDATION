use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod functions;
pub mod providers;

use providers::{AIProvider, ClaudeProvider, MessageContent, ContentBlock};

#[allow(unused_imports)]
pub use providers::UsageInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(flatten)]
    pub content: MessageContent,
}

impl ChatMessage {
    /// Create a simple text message
    #[allow(dead_code)]
    pub fn text(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: MessageContent::Text(text.into()),
        }
    }

    /// Create a message with content blocks (text, tool_use, tool_result)
    #[allow(dead_code)]
    pub fn with_blocks(role: impl Into<String>, blocks: Vec<ContentBlock>) -> Self {
        Self {
            role: role.into(),
            content: MessageContent::ContentBlocks(blocks),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub system: Option<String>,
    pub tools: Option<Vec<providers::ClaudeTool>>,
    pub supports_web_tools: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: Option<String>,
    pub usage: Option<providers::UsageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

pub struct AIAssistant {
    provider: Box<dyn AIProvider>,
}

impl AIAssistant {
    pub fn new(provider: Box<dyn AIProvider>) -> Self {
        Self { provider }
    }

    pub async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, String> {
        self.provider.generate(request).await
    }
}

lazy_static::lazy_static! {
    pub static ref AI_INSTANCE: Arc<Mutex<Option<AIAssistant>>> = Arc::new(Mutex::new(None));
    static ref CURRENT_MODEL: std::sync::RwLock<Option<String>> =
        std::sync::RwLock::new(None);
}

pub fn get_current_model() -> Result<String, String> {
    CURRENT_MODEL.read()
        .map_err(|_| "CURRENT_MODEL lock poisoned".to_string())?
        .clone()
        .ok_or_else(|| "No AI model configured. Please configure a model in Settings.".to_string())
}

#[allow(dead_code)]
pub async fn initialize_ai(api_key: String) -> Result<(), String> {
    let provider = ClaudeProvider::new(api_key, 180);
    let assistant = AIAssistant::new(Box::new(provider));

    let mut instance = AI_INSTANCE.lock().await;
    *instance = Some(assistant);

    Ok(())
}

pub async fn initialize_ai_with_model(
    api_key: String,
    model_identifier: Option<String>,
    timeout_secs: u64,
) -> Result<(), String> {
    if let Ok(mut current) = CURRENT_MODEL.write() {
        *current = model_identifier.clone();
    }

    let provider = if let Some(model) = model_identifier {
        ClaudeProvider::with_model(api_key, model, timeout_secs)
    } else {
        ClaudeProvider::new(api_key, timeout_secs)
    };

    let assistant = AIAssistant::new(Box::new(provider));

    let mut instance = AI_INSTANCE.lock().await;
    *instance = Some(assistant);

    Ok(())
}

pub async fn generate_response(request: GenerateRequest) -> Result<GenerateResponse, String> {
    let instance = AI_INSTANCE.lock().await;

    match instance.as_ref() {
        Some(assistant) => assistant.generate(request).await,
        None => Err("AI not initialized".to_string()),
    }
}
