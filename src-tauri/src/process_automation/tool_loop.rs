use crate::ai::{AiProvider, GenerateOnce, GenerateRequest, ChatMessage, GenerateResponse};
use crate::ai::providers::{MessageContent, ContentBlock, ToolDefinition};
use crate::ai::functions::{execute_async_tool, is_async_tool, ToolCall as FunctionToolCall, execute_tool as execute_fn};
use crate::owl::DbExecutor;

type Result<T> = std::result::Result<T, String>;

const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMPERATURE: f32 = 0.3;
const DEFAULT_MAX_ITERATIONS: usize = 50;

pub struct ToolLoopConfig {
    pub system: Option<String>,
    pub tools: Vec<ToolDefinition>,
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
    /// Maximum number of retries when the provider returns an empty response (blank
    /// stop_reason with no tool calls). Defaults to 3.
    pub empty_response_max_retries: u32,
    /// Initial backoff duration in milliseconds before the first retry of an empty
    /// response. Doubles on each retry up to `empty_response_backoff_ceiling_ms`.
    /// Defaults to 500 ms.
    pub empty_response_backoff_initial_ms: u64,
    /// Ceiling for the exponential backoff in milliseconds. Defaults to 4 000 ms.
    pub empty_response_backoff_ceiling_ms: u64,
    /// Ratio of input_tokens / context_window above which an empty response is
    /// treated as a context-full failure rather than a transient one. Defaults to 0.90.
    pub context_full_threshold: f32,
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
            empty_response_max_retries: 3,
            empty_response_backoff_initial_ms: 500,
            empty_response_backoff_ceiling_ms: 4_000,
            context_full_threshold: 0.90,
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
    pub output_value: String,
    pub message: String,
}

/// Pure tool execution loop — no streaming, no persistence, no side effects beyond tool calls.
/// The caller is responsible for persisting messages and emitting events.
pub async fn run_tool_loop(
    executor: &DbExecutor,
    app: Option<&tauri::AppHandle>,
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
        let response = call_with_empty_retry(provider, request, iteration, &config).await?;
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

        let wants_tools = stop_reason == "tool_use" || stop_reason == "tool_calls";

        // A blank or unrecognised stop_reason is an anomalous provider response.
        // If the model accumulated tool calls regardless (inferred by providers.rs),
        // continue executing them. Otherwise surface the anomaly as an error so the
        // task_manager records a real failure instead of masking it as "Completed.".
        // Empty responses (blank stop_reason + no tool_calls) are handled inside
        // call_with_empty_retry before reaching this point; if we arrive here with
        // a blank stop_reason it means tool calls were present (inferred path).
        if !wants_tools {
            if stop_reason.is_empty() && !response.tool_calls.is_empty() {
                crate::commands::log_backend("warn", &format!(
                    "[tool_loop] iter={} blank stop_reason but {} tool call(s) present — continuing",
                    iteration, response.tool_calls.len()
                ));
            } else {
                crate::commands::log_backend("info", &format!(
                    "[tool_loop] iter={} stop_reason={} — loop ended",
                    iteration, stop_reason
                ));
                break;
            }
        }

        let mut result_blocks: Vec<ContentBlock> = Vec::new();

        for tc in &response.tool_calls {
            if let Some(ref ct) = config.completion_tool {
                if tc.name == ct.tool_name {
                    let output_iri = tc.input.get("output_iri")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let output_value = tc.input.get("output_value")
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
                    completion = Some(CompletionResult { output_iri, output_value, message: msg });
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
            let result_json = if is_async_tool(&call.name) {
                let r = execute_async_tool(&call, app).await;
                serde_json::to_string(&r).unwrap_or_else(|e| format!("\"{}\"", e))
            } else {
                executor
                    .write(move |conn| {
                        let r = execute_fn(conn, &call, None, None);
                        serde_json::to_string(&r).map_err(|e| e.to_string())
                    })
                    .await
                    .unwrap_or_else(|e| format!("\"{}\"", e))
            };

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

/// Issue one `provider.generate` call for the given `request`, retrying transparently
/// when the provider returns a blank stop_reason with no tool calls (transient empty
/// response). Uses exponential backoff seeded by `config.empty_response_backoff_initial_ms`,
/// doubling up to `config.empty_response_backoff_ceiling_ms`, for up to
/// `config.empty_response_max_retries` additional attempts.
///
/// Returns `Err` immediately (without spending retries) when the response is empty
/// AND `input_tokens >= context_window * context_full_threshold` — the model is out
/// of space, not merely glitching.
async fn call_with_empty_retry(
    provider: &impl GenerateOnce,
    request: GenerateRequest,
    iteration: usize,
    config: &ToolLoopConfig,
) -> Result<GenerateResponse> {
    let mut backoff_ms = config.empty_response_backoff_initial_ms;

    let mut attempt: u32 = 0;
    loop {
        let resp = provider.generate_once(request.clone()).await.map_err(|e| {
            crate::commands::log_backend("error", &format!(
                "[tool_loop] iter={} provider error: {}", iteration, e
            ));
            format!("tool loop: {}", e)
        })?;

        let is_empty_response = resp.stop_reason.as_deref().unwrap_or("").is_empty()
            && resp.tool_calls.is_empty();

        if !is_empty_response {
            return Ok(resp);
        }

        let input_tokens = resp.usage.as_ref().map(|u| u.input_tokens).unwrap_or(0);
        let context_full = input_tokens > 0
            && (input_tokens as f32 / config.context_window as f32) >= config.context_full_threshold;

        if context_full {
            crate::commands::log_backend("error", &format!(
                "[tool_loop] iter={} empty response with context near full \
                 (input_tokens={} >= {:.0}% of context_window={}) — failing immediately",
                iteration, input_tokens,
                config.context_full_threshold * 100.0,
                config.context_window,
            ));
            return Err(format!(
                "O provedor retornou resposta vazia na iteração {} porque o contexto está quase \
                 cheio (input_tokens={}, limiar={:.0}% de {}). Reduza o histórico ou aumente o \
                 context_window.",
                iteration, input_tokens,
                config.context_full_threshold * 100.0,
                config.context_window,
            ));
        }

        attempt += 1;
        if attempt > config.empty_response_max_retries {
            crate::commands::log_backend("error", &format!(
                "[tool_loop] iter={} provider retornou respostas vazias repetidamente \
                 ({} tentativas) — abortando item",
                iteration, attempt,
            ));
            return Err(format!(
                "O provedor retornou respostas vazias repetidamente na iteração {} \
                 ({} tentativas no total). Isso indica problema transitório persistente \
                 do lado do provedor. O item não foi concluído.",
                iteration, attempt,
            ));
        }

        crate::commands::log_backend("warn", &format!(
            "[tool_loop] iter={} resposta vazia (tentativa {} de {}) — aguardando {}ms antes de retentar",
            iteration, attempt, config.empty_response_max_retries, backoff_ms,
        ));
        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(config.empty_response_backoff_ceiling_ms);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{GenerateOnce, GenerateRequest, GenerateResponse};
    use crate::ai::providers::UsageInfo;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn empty_response(input_tokens: u32) -> GenerateResponse {
        GenerateResponse {
            content: String::new(),
            tool_calls: vec![],
            thinking_blocks: vec![],
            stop_reason: None,
            usage: Some(UsageInfo { input_tokens, output_tokens: 0, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 }),
            model_used: None,
        }
    }

    fn config_with_retries(max_retries: u32, context_window: u32) -> ToolLoopConfig {
        ToolLoopConfig {
            empty_response_max_retries: max_retries,
            empty_response_backoff_initial_ms: 1,
            empty_response_backoff_ceiling_ms: 4,
            context_full_threshold: 0.90,
            context_window,
            ..ToolLoopConfig::default()
        }
    }

    fn make_request() -> GenerateRequest {
        GenerateRequest {
            messages: vec![],
            max_tokens: Some(100),
            temperature: Some(0.3),
            system: None,
            blackboard_context: None,
            tools: None,
            supports_web_tools: false,
            thinking: None,
            tool_choice: None,
        }
    }

    // Shared mock: returns empty_response(token_count) for the first `fail_for` calls,
    // then switches to `final_response()`. Used by all 4 tests.
    struct SequencedMock {
        call_count: Arc<AtomicUsize>,
        fail_for: usize,
        final_tokens: u32,
        final_stop_reason: Option<String>,
    }

    impl GenerateOnce for SequencedMock {
        async fn generate_once(&self, _req: GenerateRequest) -> std::result::Result<GenerateResponse, String> {
            let n = self.call_count.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_for {
                Ok(empty_response(self.final_tokens))
            } else {
                Ok(GenerateResponse {
                    content: "resposta válida".to_string(),
                    tool_calls: vec![],
                    thinking_blocks: vec![],
                    stop_reason: self.final_stop_reason.clone(),
                    usage: Some(UsageInfo { input_tokens: 100, output_tokens: 10, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 }),
                    model_used: None,
                })
            }
        }
    }

    /// (a) Retry transitório: vazio nas primeiras 2 tentativas, válida depois.
    /// Exercita `call_with_empty_retry` real — falha aqui significa regressão no algoritmo.
    #[tokio::test]
    async fn test_retry_succeeds_after_transient_empties() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock = SequencedMock {
            call_count: call_count.clone(),
            fail_for: 2,
            final_tokens: 100,
            final_stop_reason: Some("end_turn".to_string()),
        };
        let config = config_with_retries(3, 180_000);

        let result = call_with_empty_retry(&mock, make_request(), 0, &config).await;

        assert!(result.is_ok(), "deve ter sucedido após retries: {:?}", result);
        assert_eq!(call_count.load(Ordering::SeqCst), 3, "2 vazias + 1 válida = 3 chamadas");
        assert_eq!(result.unwrap().stop_reason.as_deref(), Some("end_turn"));
    }

    /// (b) Esgotamento: sempre vazio — Err claro com "respostas vazias repetidamente".
    /// `fail_for = usize::MAX` faz o mock retornar sempre empty_response(100).
    #[tokio::test]
    async fn test_retry_exhaustion_returns_clear_error() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock = SequencedMock {
            call_count: call_count.clone(),
            fail_for: usize::MAX,
            final_tokens: 100,
            final_stop_reason: None,
        };
        let config = config_with_retries(3, 180_000);

        let result = call_with_empty_retry(&mock, make_request(), 0, &config).await;

        assert!(result.is_err(), "deve ter falhado após esgotar retries");
        let msg = result.unwrap_err();
        assert!(msg.contains("respostas vazias repetidamente"), "mensagem deve mencionar respostas vazias: {}", msg);
        assert_eq!(call_count.load(Ordering::SeqCst), 4, "1 inicial + 3 retries = 4 chamadas");
    }

    /// (c) Contexto quase cheio: empty + input_tokens >= context_window * 0.90 — falha imediata sem gastar retries.
    /// `final_tokens = 162_001` fica acima de 90 % de 180_000.
    #[tokio::test]
    async fn test_context_full_fails_immediately_without_retries() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock = SequencedMock {
            call_count: call_count.clone(),
            fail_for: usize::MAX,
            final_tokens: 162_001,
            final_stop_reason: None,
        };
        let config = config_with_retries(3, 180_000);

        let result = call_with_empty_retry(&mock, make_request(), 0, &config).await;

        assert!(result.is_err(), "deve ter falhado imediatamente");
        let msg = result.unwrap_err();
        assert!(msg.contains("contexto"), "mensagem deve mencionar contexto: {}", msg);
        assert_eq!(call_count.load(Ordering::SeqCst), 1, "1 chamada — sem retries");
    }

    /// (d) Caminho de sucesso: resposta válida na primeira — sem latência nem chamada extra.
    #[tokio::test]
    async fn test_success_path_no_extra_calls() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock = SequencedMock {
            call_count: call_count.clone(),
            fail_for: 0,
            final_tokens: 100,
            final_stop_reason: Some("end_turn".to_string()),
        };
        let config = config_with_retries(3, 180_000);

        let result = call_with_empty_retry(&mock, make_request(), 0, &config).await;

        assert!(result.is_ok(), "deve ter retornado imediatamente: {:?}", result);
        assert_eq!(call_count.load(Ordering::SeqCst), 1, "exatamente 1 chamada");
    }
}
