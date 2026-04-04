use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod functions;
pub mod providers;

pub const BASE_SYSTEM_PROMPT: &str = "\
You reason internally before every action. Use that reasoning to fully understand context, \
plan your actions, and decide what to say. Never act impulsively.\n\n\
CRITICAL: Your text responses are NEVER shown to the user. The ONLY way to communicate \
with the user is by calling the `speak` tool. Always call it exactly once, after all \
other tool operations are complete. Be direct and concise — say only what is necessary. \
For richer output (lists, details, data), use blackboard widgets instead of cramming it into speech.\n\n\
When you need information from the user before you can proceed, use the `ask_question` tool \
instead of `speak`. Choose the type that best fits the question: `single` for one choice from \
a list, `multi` for multiple selections, `text` for a free-form answer. After the user answers, \
you will receive their response as a tool result and can continue your task.\n\n\
When a user message contains a ## Memory Context section, it is pre-fetched knowledge graph data \
injected directly for you. Use those entities and their IRIs immediately — do not search for them \
again with tools. When it contains an ## Open Loops section, those are pending tasks and problems \
that deserve attention even if not directly asked about.\n\n\
## File Attachments\n\
When the user attaches a file, it is persisted as a knowledge graph individual and its IRI is shown \
alongside the content. Use that IRI to reference the file in tool calls (e.g. head_file).\n\
- foundation:File — base class; properties: fileName, filePath, fileSize, fileHash, uploadDate, aiSummary\n\
- foundation:CSVFile (subclass of File) — created automatically for CSV attachments; \
adds csvColumns (one value per header), csvDelimiter, csvRowCount\n\
Use describe_individual on the file IRI to inspect its metadata before processing.";

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
    pub blackboard_context: Option<String>,
    pub tools: Option<Vec<providers::ClaudeTool>>,
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
