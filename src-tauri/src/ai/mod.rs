use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod functions;
pub mod providers;

use providers::{AIProvider, ClaudeProvider};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub system: Option<String>,
}

pub struct AIAssistant {
    provider: Box<dyn AIProvider>,
}

impl AIAssistant {
    pub fn new(provider: Box<dyn AIProvider>) -> Self {
        Self { provider }
    }

    pub async fn generate(&self, request: GenerateRequest) -> Result<String, String> {
        self.provider.generate(request).await
    }
}

// Thread-safe global instance
lazy_static::lazy_static! {
    pub static ref AI_INSTANCE: Arc<Mutex<Option<AIAssistant>>> = Arc::new(Mutex::new(None));
}

pub async fn initialize_ai(api_key: String) -> Result<(), String> {
    let provider = ClaudeProvider::new(api_key);
    let assistant = AIAssistant::new(Box::new(provider));

    let mut instance = AI_INSTANCE.lock().await;
    *instance = Some(assistant);

    Ok(())
}

pub async fn initialize_ai_with_model(api_key: String, model_identifier: Option<String>) -> Result<(), String> {
    let provider = if let Some(model) = model_identifier {
        ClaudeProvider::with_model(api_key, model)
    } else {
        ClaudeProvider::new(api_key)
    };

    let assistant = AIAssistant::new(Box::new(provider));

    let mut instance = AI_INSTANCE.lock().await;
    *instance = Some(assistant);

    Ok(())
}

pub async fn generate_response(request: GenerateRequest) -> Result<String, String> {
    let instance = AI_INSTANCE.lock().await;

    match instance.as_ref() {
        Some(assistant) => assistant.generate(request).await,
        None => Err("AI not initialized".to_string()),
    }
}
