use crate::ai::{AiProvider, GenerateRequest, ChatMessage};
use crate::ai::providers::{MessageContent, ContentBlock, ClaudeTool};
use crate::ai::functions::{ToolCall as FunctionToolCall, execute_tool as execute_fn};
use crate::owl::DbExecutor;

type Result<T> = std::result::Result<T, String>;

const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMPERATURE: f32 = 0.3;
const DEFAULT_MAX_ITERATIONS: usize = 50;

pub struct ToolLoopConfig {
    pub system: Option<String>,
    pub tools: Vec<ClaudeTool>,
    pub max_iterations: usize,
    pub max_tokens: u32,
    pub temperature: f32,
    /// If set, the loop watches for this tool name and exits early when called.
    pub completion_tool: Option<CompletionToolConfig>,
}

impl Default for ToolLoopConfig {
    fn default() -> Self {
        Self {
            system: None,
            tools: Vec::new(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: DEFAULT_TEMPERATURE,
            completion_tool: None,
        }
    }
}

pub struct CompletionToolConfig {
    /// Tool name that signals explicit task completion (e.g. "task_complete").
    pub tool_name: String,
    /// If set, the output_iri argument is validated to be an instance of this class.
    pub output_class: Option<String>,
}

pub struct ToolLoopOutput {
    /// All messages in order: initial + every assistant/tool-result turn.
    pub messages: Vec<ChatMessage>,
    /// Text of the last assistant response.
    pub last_text: String,
    /// Set when the completion tool was called successfully.
    pub completion: Option<CompletionResult>,
}

pub struct CompletionResult {
    pub output_iri: String,
    pub message: String,
}

/// Pure tool execution loop — no streaming, no persistence, no side effects beyond tool calls.
/// The caller is responsible for persisting messages and emitting events.
pub async fn run_tool_loop(
    executor: &DbExecutor,
    provider: &AiProvider,
    initial_messages: Vec<ChatMessage>,
    config: ToolLoopConfig,
) -> Result<ToolLoopOutput> {
    let mut messages = initial_messages;
    let mut last_text = String::new();
    let mut completion: Option<CompletionResult> = None;

    'outer: for _ in 0..config.max_iterations {
        let request = GenerateRequest {
            messages: messages.clone(),
            max_tokens: Some(config.max_tokens),
            temperature: Some(config.temperature),
            system: config.system.clone(),
            blackboard_context: None,
            tools: Some(config.tools.clone()),
            supports_web_tools: false,
            thinking: None,
            tool_choice: None,
        };

        let response = provider.generate(request).await
            .map_err(|e| format!("tool loop: {}", e))?;

        let stop_reason = response.stop_reason.clone().unwrap_or_default();
        last_text = response.content.clone();

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        if !response.content.is_empty() {
            assistant_blocks.push(ContentBlock::Text { text: response.content.clone() });
        }
        for tc in &response.tool_calls {
            assistant_blocks.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.input.clone(),
            });
        }
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: MessageContent::ContentBlocks(assistant_blocks),
        });

        if stop_reason != "tool_use" && stop_reason != "tool_calls" {
            break;
        }

        let mut result_blocks: Vec<ContentBlock> = Vec::new();

        for tc in &response.tool_calls {
            if let Some(ref ct) = config.completion_tool {
                if tc.name == ct.tool_name {
                    let output_iri = tc.input.get("output_iri")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let msg = tc.input.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if let Some(ref expected_class) = ct.output_class {
                        if let Some(error) = validate_output_iri(executor, &output_iri, expected_class).await {
                            result_blocks.push(ContentBlock::ToolResult {
                                tool_use_id: tc.id.clone(),
                                content: serde_json::Value::String(error),
                                is_error: Some(true),
                            });
                            break;
                        }
                    }

                    result_blocks.push(ContentBlock::ToolResult {
                        tool_use_id: tc.id.clone(),
                        content: serde_json::Value::String("Task completion acknowledged.".to_string()),
                        is_error: Some(false),
                    });
                    messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: MessageContent::ContentBlocks(result_blocks),
                    });
                    completion = Some(CompletionResult { output_iri, message: msg });
                    break 'outer;
                }
            }

            let call = FunctionToolCall {
                name: tc.name.clone(),
                arguments: tc.input.clone(),
            };
            let tc_id = tc.id.clone();
            let result_json = executor
                .write(move |conn| {
                    let r = execute_fn(conn, &call, None, None);
                    serde_json::to_string(&r).map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|e| format!("\"{}\"", e));

            let tool_result: crate::ai::functions::ToolResult =
                serde_json::from_str(&result_json).unwrap_or(crate::ai::functions::ToolResult {
                    success: false,
                    result: None,
                    error: Some(result_json.clone()),
                    concept: None,
                });

            let content = if tool_result.success {
                tool_result.result
                    .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| v.to_string()))
                    .unwrap_or_default()
            } else {
                tool_result.error.unwrap_or_default()
            };

            result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: tc_id,
                content: serde_json::Value::String(content),
                is_error: Some(!tool_result.success),
            });
        }

        if completion.is_none() {
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: MessageContent::ContentBlocks(result_blocks),
            });
        }
    }

    Ok(ToolLoopOutput { messages, last_text, completion })
}

async fn validate_output_iri(executor: &DbExecutor, iri: &str, expected_class: &str) -> Option<String> {
    if iri.is_empty() || !iri.contains(':') || iri.contains(' ') {
        return Some(format!(
            "output_iri '{}' is not a valid IRI. Create the {} individual first, then call the completion tool with its IRI.",
            iri, expected_class
        ));
    }
    let iri = iri.to_string();
    let class = expected_class.to_string();
    executor.read(move |conn| {
        match crate::owl::Individual::get(conn, &iri) {
            Ok(Some(ind)) => {
                if ind.types.iter().any(|t| t.iri == class) {
                    Ok(None)
                } else {
                    let actual: Vec<String> = ind.types.iter().map(|t| t.iri.clone()).collect();
                    Ok(Some(format!(
                        "IRI '{}' is of type {:?}, not {}. Create a {} individual and retry.",
                        iri, actual, class, class
                    )))
                }
            }
            Ok(None) => Ok(Some(format!(
                "IRI '{}' does not exist. Create the {} individual first.",
                iri, class
            ))),
            Err(e) => Ok(Some(format!("Failed to validate '{}': {}", iri, e))),
        }
    }).await.unwrap_or(None)
}
