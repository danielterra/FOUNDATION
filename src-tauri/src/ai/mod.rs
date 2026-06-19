use serde::{Deserialize, Serialize};

pub mod functions;
pub mod local;
pub mod providers;


use providers::{MessageContent, ContentBlock};

/// Minimal async generate contract used by `call_with_empty_retry`.
/// Keeps the retry logic testable with lightweight mocks instead of
/// depending on the concrete `AiProvider` enum (which drags in reqwest,
/// llama bindings and the full provider surface).
///
/// Used only within this crate; `Send` bound not required because the
/// retry loop is driven by Tokio's single-thread executor in tests and
/// the Tauri async runtime in production.
#[allow(async_fn_in_trait)]
pub trait GenerateOnce {
    async fn generate_once(&self, req: GenerateRequest) -> Result<GenerateResponse, String>;
}

impl GenerateOnce for AiProvider {
    async fn generate_once(&self, req: GenerateRequest) -> Result<GenerateResponse, String> {
        self.generate(req).await
    }
}

pub enum AiProvider {
    Claude(providers::ClaudeProvider),
    Local(local::LocalProvider),
    OpenRouter(providers::OpenRouterProvider),
}

impl AiProvider {
    pub async fn generate_stream(
        &self,
        request: GenerateRequest,
        app: &tauri::AppHandle,
        conversation_id: &str,
    ) -> Result<GenerateResponse, String> {
        match self {
            AiProvider::Claude(p) => p.generate_stream(request, app, conversation_id).await,
            AiProvider::Local(p) => p.generate_stream(request, app, conversation_id).await,
            AiProvider::OpenRouter(p) => p.generate_stream(request, app, conversation_id).await,
        }
    }

    pub async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, String> {
        match self {
            AiProvider::Claude(p) => p.generate(request).await,
            AiProvider::Local(_) => Err("Local provider does not support non-streaming generate".to_string()),
            AiProvider::OpenRouter(p) => p.generate(request).await,
        }
    }
}

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
    pub blackboard_context: Option<String>,
    pub tools: Option<Vec<providers::ToolDefinition>>,
    pub supports_web_tools: bool,
    pub thinking: Option<ThinkingConfig>,
    pub tool_choice: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingConfig {
    Adaptive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingBlock {
    Thinking {
        thinking: String,
        signature: String,
    },
    RedactedThinking {
        data: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub thinking_blocks: Vec<ThinkingBlock>,
    pub stop_reason: Option<String>,
    pub usage: Option<providers::UsageInfo>,
    /// Actual model used (populated by providers that support runtime model selection, e.g. OpenRouter fallback)
    pub model_used: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}
