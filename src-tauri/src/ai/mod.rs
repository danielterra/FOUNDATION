use serde::{Deserialize, Serialize};

pub mod functions;
pub mod providers;

pub const BASE_SYSTEM_PROMPT: &str = "\
## Communication Discipline\n\n\
You operate in a tool loop. Each iteration you must call at least one tool. \
Text responses are NEVER shown to the user — `speak` is your only output channel.\n\n\
**To end the loop and respond to the user, use exactly one of:**\n\
- `speak(message, iris?)` — when you have a complete answer. Called once, as the last tool. \
Be concise. For rich data, pass entity IRIs to display as blackboard widgets.\n\
- `ask_question(question, type, options?)` — when you need user input before you can continue. \
Pauses the loop until the user answers. Do NOT also call `speak` in the same turn.\n\n\
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

use providers::{MessageContent, ContentBlock};

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
