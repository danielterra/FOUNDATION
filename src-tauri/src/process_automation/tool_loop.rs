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
    /// Compact the in-memory message history when input_tokens / context_window exceeds this ratio.
    /// Set to 0.0 to disable. Defaults to 0.80.
    pub compaction_threshold: f32,
    /// Model context window size in tokens. Used to determine when compaction is needed.
    /// Defaults to 180_000 (conservative for Claude models).
    pub context_window: u32,
    /// If set, each new message is persisted to this conversation as it is generated.
    pub persist_to: Option<PersistConfig>,
}

pub struct PersistConfig {
    pub conv_iri: String,
    pub model_identifier: String,
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
            compaction_threshold: 0.80,
            context_window: 180_000,
            persist_to: None,
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

    let conv_label = config.persist_to.as_ref().map(|p| p.conv_iri.as_str()).unwrap_or("-");
    crate::commands::log_backend("info", &format!(
        "[tool_loop] start conv={} max_iter={}", conv_label, config.max_iterations
    ));

    'outer: for iteration in 0..config.max_iterations {
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

        crate::commands::log_backend("debug", &format!(
            "[tool_loop] iter={} calling provider (msgs={})", iteration, messages.len()
        ));
        let response = provider.generate(request).await
            .map_err(|e| {
                crate::commands::log_backend("error", &format!(
                    "[tool_loop] iter={} provider error: {}", iteration, e
                ));
                format!("tool loop: {}", e)
            })?;
        crate::commands::log_backend("debug", &format!(
            "[tool_loop] iter={} provider responded stop_reason={} input_tokens={}",
            iteration,
            response.stop_reason.as_deref().unwrap_or("none"),
            response.usage.as_ref().map(|u| u.input_tokens).unwrap_or(0)
        ));

        if config.compaction_threshold > 0.0 {
            let input_tokens = response.usage.as_ref().map(|u| u.input_tokens).unwrap_or(0);
            // Compare against the model context window, not max_tokens (output limit).
            // max_tokens controls output size; context window is how much input the model accepts.
            let ratio = input_tokens as f32 / config.context_window as f32;
            if ratio >= config.compaction_threshold && messages.len() > 2 {
                crate::commands::log_backend("info", &format!(
                    "[TASK COMPACTION] input_tokens={} ratio={:.2} — compacting {} messages",
                    input_tokens, ratio, messages.len()
                ));
                if let Some(compacted) = compact_messages(provider, &messages, config.system.as_deref(), config.max_tokens).await {
                    crate::commands::log_backend("info", &format!(
                        "[TASK COMPACTION] input_tokens={} -> compacted to 1 summary message",
                        input_tokens
                    ));
                    messages = compacted;
                }
            }
        }

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
        let assistant_msg = ChatMessage {
            role: "assistant".to_string(),
            content: MessageContent::ContentBlocks(assistant_blocks),
        };
        if let Some(ref p) = config.persist_to {
            persist_message(executor, &p.conv_iri, &assistant_msg, Some(&p.model_identifier)).await;
        }
        messages.push(assistant_msg);

        if stop_reason != "tool_use" && stop_reason != "tool_calls" {
            crate::commands::log_backend("info", &format!(
                "[tool_loop] iter={} stop_reason={} — loop ended",
                iteration, stop_reason
            ));
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

            crate::commands::log_backend("debug", &format!(
                "[tool_loop] iter={} tool={}", iteration, tc.name
            ));
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
            let tool_result_msg = ChatMessage {
                role: "user".to_string(),
                content: MessageContent::ContentBlocks(result_blocks),
            };
            if let Some(ref p) = config.persist_to {
                persist_message(executor, &p.conv_iri, &tool_result_msg, None).await;
            }
            messages.push(tool_result_msg);
        }
    }

    Ok(ToolLoopOutput { messages, last_text, completion })
}

async fn persist_message(executor: &DbExecutor, conv_iri: &str, msg: &ChatMessage, model: Option<&str>) {
    let blocks = super::agent_task::to_storage_blocks(&msg.content);
    if let Err(e) = crate::commands::chat_storage::create_message(
        executor, conv_iri, &msg.role, blocks, model, None, None,
    ).await {
        crate::commands::log_backend("warn", &format!(
            "[tool_loop] persist_message failed for {}: {}", conv_iri, e
        ));
    }
}

/// Summarise the message history into a single user message so the loop can continue
/// without hitting the context limit. Returns None if the compaction call itself fails,
/// leaving the original messages intact so the loop can still attempt one more iteration.
async fn compact_messages(
    provider: &AiProvider,
    messages: &[ChatMessage],
    system: Option<&str>,
    max_tokens: u32,
) -> Option<Vec<ChatMessage>> {
    let history: String = messages.iter().map(|m| {
        let role = &m.role;
        let text = match &m.content {
            MessageContent::Text(t) => t.clone(),
            MessageContent::ContentBlocks(blocks) => blocks.iter().filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                ContentBlock::ToolResult { content, .. } => content.as_str(),
                _ => None,
            }).collect::<Vec<_>>().join(" "),
        };
        format!("[{}]: {}", role, &text[..text.len().min(500)])
    }).collect::<Vec<_>>().join("\n");

    let compaction_prompt = format!(
        "Summarise the following task execution history. Preserve: the original task objective, \
         items already processed, items still pending, and key decisions made. \
         Discard intermediate tool call details that are no longer needed.\n\n{}",
        history
    );

    let req = GenerateRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(compaction_prompt),
        }],
        max_tokens: Some((max_tokens / 4).max(512)),
        temperature: Some(0.1),
        system: system.map(str::to_string),
        blackboard_context: None,
        tools: None,
        supports_web_tools: false,
        thinking: None,
        tool_choice: None,
    };

    let summary = provider.generate(req).await.ok()?.content;
    if summary.is_empty() { return None; }

    Some(vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::Text(format!("[Resumo do progresso anterior]\n{}", summary)),
    }])
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
