use std::num::NonZeroU32;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, LlamaChatMessage};
use llama_cpp_2::sampling::LlamaSampler;

use crate::ai::{GenerateRequest, GenerateResponse, ToolCall};
use crate::ai::providers::{MessageContent, ClaudeTool, UsageInfo};

// LlamaContext wraps a raw C pointer that is safe to send across threads
// (llama.cpp uses no thread-local state; access is serialised by the Mutex below).
struct SendableContext(LlamaContext<'static>);
unsafe impl Send for SendableContext {}

lazy_static::lazy_static! {
    static ref LOADED_MODEL: Mutex<Option<Arc<LlamaModel>>> = Mutex::new(None);
    static ref LLAMA_BACKEND: std::sync::Mutex<Option<Arc<LlamaBackend>>> =
        std::sync::Mutex::new(None);
    // Cached context: never dropped to avoid ggml_metal_rsets_free assertion crash.
    // (Known llama.cpp Metal bug: keepalive thread still holds resource sets when the
    // context destructor fires.) Model and backend live in statics, so 'static is sound.
    static ref LOADED_CONTEXT: std::sync::Mutex<Option<SendableContext>> =
        std::sync::Mutex::new(None);
}

/// Context window of the local model. Matches `with_n_ctx` in `run_inference`.
const LOCAL_CONTEXT_WINDOW: usize = 65536;
/// Maximum output tokens per turn — must match the limit inside `run_inference`.
const LOCAL_MAX_OUTPUT_TOKENS: usize = 2048;
/// Input budget: context minus output headroom, expressed as estimated chars (≈3.5 chars/token).
const MAX_PROMPT_CHARS: usize = (LOCAL_CONTEXT_WINDOW - LOCAL_MAX_OUTPUT_TOKENS) * 35 / 10;

fn get_or_init_backend() -> Result<Arc<LlamaBackend>, String> {
    let mut guard = LLAMA_BACKEND.lock().map_err(|e| format!("Backend lock poisoned: {}", e))?;
    if let Some(b) = guard.as_ref() {
        return Ok(Arc::clone(b));
    }
    let backend = LlamaBackend::init().map_err(|e| format!("Failed to init llama backend: {}", e))?;
    let backend = Arc::new(backend);
    *guard = Some(Arc::clone(&backend));
    Ok(backend)
}

pub struct LocalProvider {
    model_path: String,
}

impl LocalProvider {
    pub fn new(model_path: String) -> Self {
        Self { model_path }
    }

    pub async fn generate_stream(
        &self,
        request: GenerateRequest,
        app: &tauri::AppHandle,
        conversation_id: &str,
    ) -> Result<GenerateResponse, String> {
        let model = load_model(&self.model_path).await?;

        let tools = request.tools.as_deref().unwrap_or(&[]);
        let tool_injection = format_tools_for_prompt(tools);

        let mut messages: Vec<(String, String)> = Vec::new();
        if let Some(ref system) = request.system {
            messages.push(("system".to_string(), format!("{}{}", system, tool_injection)));
        }
        for msg in &request.messages {
            let role = match msg.role.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                _ => continue,
            };
            let text = extract_text_content(&msg.content);
            if text.is_empty() { continue; }
            // Merge consecutive same-role messages so tool results (which arrive as a
            // separate user turn) are folded into the next real user message rather
            // than creating a pattern the model might reproduce.
            if let Some(last) = messages.last_mut() {
                if last.0 == role && role != "system" {
                    last.1.push('\n');
                    last.1.push_str(&text);
                    continue;
                }
            }
            messages.push((role.to_string(), text));
        }

        truncate_messages_if_needed(&mut messages);

        let conv_id = conversation_id.to_string();
        let app_clone = app.clone();

        crate::commands::log_backend("info", "[LOCAL AI] Starting inference...");

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (stats_tx, stats_rx) = tokio::sync::oneshot::channel::<InferenceStats>();

        let model_clone = Arc::clone(&model);
        tokio::task::spawn_blocking(move || {
            match run_inference(&model_clone, &messages, |piece| tx.send(piece).is_ok()) {
                Ok(stats) => { stats_tx.send(stats).ok(); }
                Err(e) => {
                    crate::commands::log_backend("error", &format!("[LOCAL AI] Inference error: {}", e));
                }
            }
        });

        let mut full_text = String::new();
        let mut processor = StreamProcessor::new();

        loop {
            match rx.recv().await {
                None => break,
                Some(piece) if piece.is_empty() => {}
                Some(piece) => {
                    full_text.push_str(&piece);
                    let (to_emit, should_stop) = processor.feed(&piece);
                    if let Some(text) = to_emit {
                        app_clone.emit("chat-ai-delta", serde_json::json!({
                            "conversationId": conv_id,
                            "type": "text",
                            "text": text,
                        })).ok();
                    }
                    if should_stop {
                        drop(rx); // causes on_token to return false → inference loop breaks
                        break;
                    }
                }
            }
        }

        // Flush any buffered lookahead that turned out not to be a tool call.
        if let Some(remaining) = processor.flush_pending_text() {
            if !remaining.is_empty() {
                app_clone.emit("chat-ai-delta", serde_json::json!({
                    "conversationId": conv_id,
                    "type": "text",
                    "text": remaining,
                })).ok();
            }
        }

        crate::commands::log_backend("info", &format!(
            "[LOCAL AI] Inference complete. {} chars. Raw: {:?}",
            full_text.len(),
            full_text.chars().take(200).collect::<String>(),
        ));

        let stats = stats_rx.await.ok();
        let usage = stats.map(|s| {
            crate::commands::log_backend("info", &format!(
                "[LOCAL AI] Tokens — prompt: {}, completion: {}",
                s.prompt_tokens, s.completion_tokens,
            ));
            UsageInfo {
                input_tokens: s.prompt_tokens as u32,
                output_tokens: s.completion_tokens as u32,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }
        });

        // Parse all tool calls from full output. Try <tool_call> XML first, then Python speak().
        let mut tool_calls = parse_all_tool_calls_from_output(&full_text);

        if tool_calls.is_empty() {
            let message = parse_python_speak_output(&full_text);
            crate::commands::log_backend("info", &format!(
                "[LOCAL AI] Fallback parsed message: {:?}", message
            ));
            app_clone.emit("chat-ai-delta", serde_json::json!({
                "conversationId": conv_id,
                "type": "text",
                "text": message,
            })).ok();
            let tool_id = format!("local_{}", chrono::Utc::now().timestamp_millis());
            tool_calls.push(ToolCall {
                id: tool_id,
                name: "speak".to_string(),
                input: serde_json::json!({ "message": message }),
            });
        } else {
            crate::commands::log_backend("info", &format!(
                "[LOCAL AI] Parsed {} tool call(s): {:?}",
                tool_calls.len(),
                tool_calls.iter().map(|tc| tc.name.as_str()).collect::<Vec<_>>()
            ));
            // Emit speak messages that weren't already streamed via StreamProcessor
            for tc in &tool_calls {
                if tc.name == "speak" {
                    if let Some(msg) = tc.input.get("message").and_then(|m| m.as_str()) {
                        app_clone.emit("chat-ai-delta", serde_json::json!({
                            "conversationId": conv_id,
                            "type": "text",
                            "text": msg,
                        })).ok();
                    }
                }
            }
        }

        Ok(GenerateResponse {
            content: String::new(),
            tool_calls,
            thinking_blocks: vec![],
            stop_reason: Some("tool_use".to_string()),
            usage,
        })
    }
}

// ---------------------------------------------------------------------------
// Streaming buffer — lookahead for <tool_call> tags
// ---------------------------------------------------------------------------

fn combine_text(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(x), Some(y)) => Some(format!("{}{}", x, y)),
        (Some(x), None) => Some(x),
        (None, y) => y,
    }
}

#[derive(Debug, PartialEq)]
enum StreamState {
    /// Normal text: emit each token immediately.
    Text,
    /// Saw `<` — buffering until we know if this is a `<tool_call>` tag.
    MaybeTool,
    /// Inside `<tool_call>…</tool_call>` — suppress all output.
    InToolCall,
}

struct StreamProcessor {
    state: StreamState,
    /// Lookahead buffer: holds tokens when we might be entering a tag.
    /// Reused as accumulator in `InToolCall` state to detect the closing tag.
    lookahead: String,
}

impl StreamProcessor {
    fn new() -> Self {
        Self { state: StreamState::Text, lookahead: String::new() }
    }

    /// Feed one token. Returns `(text_to_emit, should_stop_inference)`.
    fn feed(&mut self, token: &str) -> (Option<String>, bool) {
        match self.state {
            StreamState::Text => {
                if let Some(lt_pos) = token.find('<') {
                    // Emit text before `<`, buffer the rest, then immediately try to
                    // resolve — handles the case where `<tool_call>` arrives as one token.
                    let pre = if lt_pos > 0 { Some(token[..lt_pos].to_string()) } else { None };
                    self.lookahead.push_str(&token[lt_pos..]);
                    self.state = StreamState::MaybeTool;
                    let (post, stop) = self.resolve_lookahead();
                    (combine_text(pre, post), stop)
                } else {
                    (Some(token.to_string()), false)
                }
            }
            StreamState::MaybeTool => {
                self.lookahead.push_str(token);
                self.resolve_lookahead()
            }
            StreamState::InToolCall => {
                self.lookahead.push_str(token);
                if self.lookahead.contains("</tool_call>") {
                    // Tool call complete — clear buffer and go back to Text so the
                    // model can emit more content or additional <tool_call> tags.
                    self.lookahead.clear();
                    self.state = StreamState::Text;
                    (None, false)
                } else {
                    (None, false)
                }
            }
        }
    }

    /// Checks the current lookahead buffer and either confirms a `<tool_call>` tag,
    /// flushes as plain text (false alarm), or keeps buffering.
    fn resolve_lookahead(&mut self) -> (Option<String>, bool) {
        if let Some(tag_pos) = self.lookahead.find("<tool_call>") {
            let before = if tag_pos > 0 {
                Some(self.lookahead[..tag_pos].to_string())
            } else {
                None
            };
            self.lookahead.clear();
            self.state = StreamState::InToolCall;
            (before, false)
        } else if self.lookahead.len() > 20 {
            // Not a tool_call tag — flush as plain text.
            let flushed = std::mem::take(&mut self.lookahead);
            self.state = StreamState::Text;
            (Some(flushed), false)
        } else {
            (None, false)
        }
    }

    /// Returns buffered text that was held in `MaybeTool` state but never confirmed as a tag.
    /// Should be called after the receive loop ends to flush any remaining lookahead.
    fn flush_pending_text(&self) -> Option<String> {
        if matches!(self.state, StreamState::Text | StreamState::MaybeTool)
            && !self.lookahead.is_empty()
        {
            Some(self.lookahead.clone())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Context truncation
// ---------------------------------------------------------------------------

/// Removes the oldest non-system messages until the total estimated prompt size
/// fits within the input token budget. Keeps system message (index 0) and the
/// last message (most recent user turn) intact.
fn truncate_messages_if_needed(messages: &mut Vec<(String, String)>) {
    let total_chars: usize = messages.iter().map(|(_, c)| c.len()).sum();
    if total_chars <= MAX_PROMPT_CHARS {
        return;
    }

    crate::commands::log_backend("warn", &format!(
        "[LOCAL AI] Prompt too long ({} chars, limit {}). Truncating history.",
        total_chars, MAX_PROMPT_CHARS,
    ));

    let mut removed = 0;
    while messages.len() > 2 {
        let current: usize = messages.iter().map(|(_, c)| c.len()).sum();
        if current <= MAX_PROMPT_CHARS {
            break;
        }
        // Remove oldest non-system message (index 1), preserving system at 0
        // and the last message always.
        messages.remove(1);
        removed += 1;
    }

    crate::commands::log_backend("info", &format!(
        "[LOCAL AI] Removed {} message(s) from history to fit context.",
        removed,
    ));
}

// ---------------------------------------------------------------------------
// Token stats returned from run_inference
// ---------------------------------------------------------------------------

struct InferenceStats {
    prompt_tokens: usize,
    completion_tokens: usize,
}

// ---------------------------------------------------------------------------
// Tool prompt formatting
// ---------------------------------------------------------------------------

/// Generates tool-calling instructions injected into the system prompt of local models.
/// Claude receives tool definitions via the API; local models (Gemma 4 E4B via llama.cpp)
/// require the format to be described in the system prompt.
///
/// Gemma 4 was trained with native <tool_call> token support. The examples below match
/// the format used during training so the model recognises them reliably.
fn format_tools_for_prompt(tools: &[ClaudeTool]) -> String {
    if tools.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "\n\n---\n## TOOL USE\n\n\
        You were trained to call tools using <tool_call> tags. \
        You may emit one or more tool calls per response:\n\
        <tool_call>{\"name\": \"TOOL_NAME\", \"input\": {ARGUMENTS_AS_JSON}}</tool_call>\n\n\
        Rules:\n\
        - Use ONLY tools from the list below — NEVER invent tool names.\n\
        - Execute all knowledge-graph actions first, then call speak() last.\n\
        - Never repeat a tool call with identical arguments.\n\n\
        Examples of correct tool use:\n\
        \n\
        User: What is 12 × 8?\n\
        <tool_call>{\"name\": \"speak\", \"input\": {\"message\": \"12 × 8 = 96.\"}}</tool_call>\n\
        \n\
        User: Show me my tasks for today.\n\
        <tool_call>{\"name\": \"search\", \"input\": {\"query\": \"tasks due today\", \"class_iri\": \"foundation:Task\"}}</tool_call>\n\
        [tool result: {\"entities\": [{\"label\": \"Pay rent\", \"iri\": \"foundation:Task_1\"}], \"total\": 1}]\n\
        <tool_call>{\"name\": \"speak\", \"input\": {\"message\": \"You have 1 task today: Pay rent.\", \"iris\": [\"foundation:Task_1\"]}}</tool_call>\n\
        \n\
        User: What is foundation:Project_42?\n\
        <tool_call>{\"name\": \"describe_individual\", \"input\": {\"iris\": [\"foundation:Project_42\"]}}</tool_call>\n\
        [tool result: {\"label\": \"Website Redesign\", \"status\": \"InProgress\"}]\n\
        <tool_call>{\"name\": \"speak\", \"input\": {\"message\": \"Project 42 is 'Website Redesign', currently in progress.\"}}</tool_call>\n\
        \n\
        ## AVAILABLE TOOLS\n\n",
    );

    for tool in tools {
        out.push_str(&compact_tool_signature(tool));
        out.push('\n');
    }

    out
}

fn compact_tool_signature(tool: &ClaudeTool) -> String {
    let params = schema_params_signature(&tool.input_schema);
    let desc = tool.description.trim();
    let desc = if desc.len() > 80 { &desc[..80] } else { desc };
    format!("- {}({}) — {}", tool.name, params, desc)
}

fn schema_params_signature(schema: &serde_json::Value) -> String {
    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return String::new(),
    };
    let required: Vec<&str> = schema.get("required")
        .and_then(|r| r.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    props.iter()
        .map(|(name, prop)| {
            let type_str = prop.get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("any");
            let opt = if required.contains(&name.as_str()) { "" } else { "?" };
            format!("{}{}: {}", name, opt, type_str)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Output parsers
// ---------------------------------------------------------------------------

/// Parses ALL `<tool_call>` tags from the model output in order.
/// Falls back to a single Python-style call when no XML tags are found.
fn parse_all_tool_calls_from_output(text: &str) -> Vec<ToolCall> {
    let text = strip_eot_markers(text);
    let mut calls: Vec<ToolCall> = Vec::new();
    let mut remaining = text;

    while let Some(start) = remaining.find("<tool_call>") {
        let content_start = start + "<tool_call>".len();
        let rest = &remaining[content_start..];
        if let Some(rel_end) = rest.find("</tool_call>") {
            let json_str = rest[..rel_end].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                    let input = v.get("input").cloned().unwrap_or(serde_json::json!({}));
                    let id = format!("local_{}_{}", chrono::Utc::now().timestamp_millis(), calls.len());
                    calls.push(ToolCall { id, name: name.to_string(), input });
                }
            }
            remaining = &remaining[content_start + rel_end + "</tool_call>".len()..];
        } else {
            break;
        }
    }

    if calls.is_empty() {
        if let Some(tc) = parse_json_call_syntax(text) {
            calls.push(tc);
        }
    }

    calls
}

/// Parses `name({"key": "val"})` syntax, walking braces to handle nested JSON.
fn parse_json_call_syntax(text: &str) -> Option<ToolCall> {
    // Find a `(` preceded by a word that looks like a tool name.
    let paren_pos = text.find("({")?;
    let before = text[..paren_pos].trim();
    // Extract the tool name — last word before `(`
    let name = before.split_whitespace().last()
        .or_else(|| before.split(':').last())
        .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
        .filter(|s| !s.is_empty())?
        .to_string();

    let json_start = paren_pos + 1; // points at `{`
    let rest = &text[json_start..];

    // Walk braces to find the matching `}`.
    let mut depth = 0usize;
    let mut end = None;
    for (i, c) in rest.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let json_str = &rest[..end?];
    let input: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let id = format!("local_{}", chrono::Utc::now().timestamp_millis());
    Some(ToolCall { id, name, input })
}

fn strip_eot_markers(text: &str) -> &str {
    text.trim_end_matches("<end_of_turn>")
        .trim_end_matches("<eos>")
        .trim_end_matches('\n')
        .trim()
}

/// Extracts the message from Python-style `speak(message="...")` output.
/// Fallback for models that still produce the old format.
fn parse_python_speak_output(text: &str) -> String {
    let text = strip_eot_markers(text);
    if let Some(pos) = text.rfind("speak(") {
        let after = &text[pos + 6..];
        if let Some(msg) = extract_quoted_arg(after, "message") {
            return msg;
        }
    }
    text.to_string()
}

/// Extracts a named string argument from Python-style function call arguments.
/// Handles both double-quoted and single-quoted values with backslash escaping.
fn extract_quoted_arg(args: &str, key: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{}={}", key, quote);
        if let Some(start) = args.find(&needle) {
            let content = &args[start + needle.len()..];
            let mut result = String::new();
            let mut escaped = false;
            for c in content.chars() {
                if escaped {
                    result.push(c);
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    return Some(result);
                } else {
                    result.push(c);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// llama.cpp core
// ---------------------------------------------------------------------------

fn detect_chat_template(model: &LlamaModel) -> Result<llama_cpp_2::model::LlamaChatTemplate, String> {
    if let Ok(tmpl) = model.chat_template(None) {
        let test_msg = vec![
            LlamaChatMessage::new("user".to_string(), "hi".to_string())
                .map_err(|e| format!("Failed to create test message: {}", e))?,
        ];
        if model.apply_chat_template(&tmpl, &test_msg, true).is_ok() {
            return Ok(tmpl);
        }
    }
    llama_cpp_2::model::LlamaChatTemplate::new("gemma")
        .map_err(|e| format!("Failed to create gemma template: {}", e))
}

/// `on_token` returns `false` to stop generation early (e.g. tool call complete).
fn run_inference<F>(
    model: &LlamaModel,
    messages: &[(String, String)],
    mut on_token: F,
) -> Result<InferenceStats, String>
where
    F: FnMut(String) -> bool,
{
    let backend = get_or_init_backend()?;
    let tmpl = detect_chat_template(model)?;

    let chat_messages: Vec<LlamaChatMessage> = messages.iter()
        .map(|(role, content)| {
            LlamaChatMessage::new(role.clone(), content.clone())
                .map_err(|e| format!("Failed to create chat message: {}", e))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let prompt = model.apply_chat_template(&tmpl, &chat_messages, true)
        .map_err(|e| format!("Failed to apply chat template: {}", e))?;

    let tokens = model.str_to_token(&prompt, AddBos::Always)
        .map_err(|e| format!("Tokenization failed: {}", e))?;

    let n_input = tokens.len();

    let mut ctx_guard = LOADED_CONTEXT.lock()
        .map_err(|e| format!("Context lock poisoned: {}", e))?;

    if ctx_guard.is_none() {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(LOCAL_CONTEXT_WINDOW as u32));
        let ctx = model.new_context(&backend, ctx_params)
            .map_err(|e| format!("Failed to create context: {}", e))?;
        // Safety: model lives in LOADED_MODEL (static) and backend lives in LLAMA_BACKEND
        // (static), both of which are 'static. The context borrows from them, so extending
        // the lifetime to 'static is sound.
        let ctx: LlamaContext<'static> = unsafe { std::mem::transmute(ctx) };
        *ctx_guard = Some(SendableContext(ctx));
    }

    let ctx = &mut ctx_guard.as_mut().unwrap().0;
    ctx.clear_kv_cache();

    // Decode prompt in 512-token chunks to avoid n_tokens_all > n_batch assertion.
    const PROMPT_CHUNK: usize = 512;
    let mut batch = LlamaBatch::new(PROMPT_CHUNK.max(1), 1);
    let mut chunk_start = 0;
    while chunk_start < n_input {
        let chunk_end = (chunk_start + PROMPT_CHUNK).min(n_input);
        batch.clear();
        for j in chunk_start..chunk_end {
            let is_last_token = j == n_input - 1;
            batch.add(tokens[j], j as i32, &[0], is_last_token)
                .map_err(|e| format!("Failed to add token to batch: {}", e))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| format!("Failed to decode prompt chunk {}-{}: {}", chunk_start, chunk_end, e))?;
        chunk_start = chunk_end;
    }

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(42);
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(64, 1.1, 0.0, 0.0),
        LlamaSampler::temp(1.0),
        LlamaSampler::top_p(0.95, 1),
        LlamaSampler::dist(seed),
    ]);
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let n_input_i32 = n_input as i32;
    let mut n_cur = n_input_i32;

    loop {
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, false, None)
            .unwrap_or_default();
        if !piece.is_empty() && !on_token(piece) {
            // Caller requested stop (e.g. tool call complete, channel closed).
            n_cur += 1;
            break;
        }

        batch.clear();
        batch.add(token, n_cur, &[0], true)
            .map_err(|e| format!("Failed to add generated token: {}", e))?;
        ctx.decode(&mut batch)
            .map_err(|e| format!("Failed to decode generated token: {}", e))?;
        n_cur += 1;

        if n_cur - n_input_i32 >= LOCAL_MAX_OUTPUT_TOKENS as i32 {
            break;
        }
        if n_cur >= LOCAL_CONTEXT_WINDOW as i32 {
            break;
        }
    }

    // Context stays in LOADED_CONTEXT — never dropped, avoiding the Metal crash.
    Ok(InferenceStats {
        prompt_tokens: n_input,
        completion_tokens: (n_cur - n_input_i32) as usize,
    })
}

pub async fn load_model(model_path: &str) -> Result<Arc<LlamaModel>, String> {
    // Check path existence before the cache: a non-existent path should always
    // return Err, even if a different model is already cached.
    if !std::path::Path::new(model_path).exists() {
        let filename = std::path::Path::new(model_path)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| model_path.to_string());
        return Err(format!(
            "Model file not found: {}. Download {} from HuggingFace and place it in src-tauri/resources/models/.",
            model_path, filename
        ));
    }

    let mut guard = LOADED_MODEL.lock().await;
    if let Some(model) = guard.as_ref() {
        return Ok(Arc::clone(model));
    }

    crate::commands::log_backend("info", &format!("[LOCAL AI] Loading model from: {}", model_path));

    let backend = get_or_init_backend()?;
    let path = std::path::PathBuf::from(model_path);
    let model = LlamaModel::load_from_file(&backend, path, &LlamaModelParams::default())
        .map_err(|e| format!("Failed to load local AI model: {}", e))?;

    crate::commands::log_backend("info", "[LOCAL AI] Model loaded successfully");
    let model = Arc::new(model);
    *guard = Some(Arc::clone(&model));
    Ok(model)
}

pub(crate) fn extract_text_content(content: &MessageContent) -> String {
    use crate::ai::providers::ContentBlock;
    match content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::ContentBlocks(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.clone()),
                ContentBlock::ToolUse { name, input, .. } => {
                    if name == "speak" {
                        // Prior speaks are the model's own words — render as plain text.
                        input.get("message").and_then(|m| m.as_str()).map(|s| s.to_string())
                    } else {
                        // Include args so the model knows exactly what it called before,
                        // helping it avoid repeating the same call with the same arguments.
                        Some(format!("[called {}({})]", name, input))
                    }
                }
                ContentBlock::ToolResult { content, is_error, .. } => {
                    let text = match content {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Array(arr) => arr.iter()
                            .filter_map(|v| v.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        other => other.to_string(),
                    };
                    if text.is_empty() {
                        None
                    } else if is_error.unwrap_or(false) {
                        // Include error results so the model knows about failures and can recover.
                        Some(format!("[error: {}]", text))
                    } else {
                        Some(text)
                    }
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::{ContentBlock, MessageContent};

    const MODEL_E4B: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/models/gemma-4-E4B-it-Q4_K_M.gguf"
    );


    fn speak_tool() -> ClaudeTool {
        ClaudeTool {
            name: "speak".to_string(),
            description: "Responde ao usuário com texto".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"message": {"type": "string"}},
                "required": ["message"]
            }),
        }
    }

    fn describe_tool() -> ClaudeTool {
        ClaudeTool {
            name: "describe_individual".to_string(),
            description: "Fetch full details of an entity by IRI".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"iris": {"type": "array", "items": {"type": "string"}}},
                "required": ["iris"]
            }),
        }
    }

    fn search_tool() -> ClaudeTool {
        ClaudeTool {
            name: "search".to_string(),
            description: "Search for entities by label or type".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "class_iri": {"type": "string"}
                },
                "required": []
            }),
        }
    }

    /// Builds messages replicating the generate_stream pipeline:
    /// applies extract_text_content and merges consecutive same-role turns.
    fn build_turns(
        system: &str,
        tools: &[ClaudeTool],
        turns: Vec<(&str, MessageContent)>,
    ) -> Vec<(String, String)> {
        let mut messages: Vec<(String, String)> = vec![
            ("system".to_string(), format!("{}{}", system, format_tools_for_prompt(tools))),
        ];
        for (role, content) in turns {
            let text = extract_text_content(&content);
            if text.is_empty() { continue; }
            if let Some(last) = messages.last_mut() {
                if last.0 == role && role != "system" {
                    last.1.push('\n');
                    last.1.push_str(&text);
                    continue;
                }
            }
            messages.push((role.to_string(), text));
        }
        messages
    }

    /// Runs inference and returns (raw_output, tool_call_name, speak_message).
    async fn run_and_parse(model_path: &str, messages: &[(String, String)])
        -> (String, Option<String>, Option<String>)
    {
        let model = load_model(model_path).await.expect("falha ao carregar modelo");
        let mut raw = String::new();
        run_inference(&model, messages, |p| { raw.push_str(&p); true })
            .expect("inferência falhou");
        println!("\n--- raw output ---\n{:?}\n---", raw);
        let tc = parse_all_tool_calls_from_output(&raw).into_iter().next();
        let name = tc.as_ref().map(|t| t.name.clone());
        let msg = tc.as_ref().and_then(|t| t.input["message"].as_str().map(|s| s.to_string()));
        (raw, name, msg)
    }

    /// Runs inference and returns (raw_output, all tool calls in order).
    async fn run_and_parse_all(model_path: &str, messages: &[(String, String)])
        -> (String, Vec<ToolCall>)
    {
        let model = load_model(model_path).await.expect("falha ao carregar modelo");
        let mut raw = String::new();
        run_inference(&model, messages, |p| { raw.push_str(&p); true })
            .expect("inferência falhou");
        println!("\n--- raw output (all calls) ---\n{:?}\n---", raw);
        let calls = parse_all_tool_calls_from_output(&raw);
        (raw, calls)
    }

    /// Builds messages with tool injection exactly as `generate_stream` does.
    /// Includes the mandatory "speak is your only output channel" constraint so
    /// the model doesn't bypass tools on trivial prompts.
    fn messages_with_injection(system: &str, user: &str) -> Vec<(String, String)> {
        let tools = vec![speak_tool()];
        let full_system = format!(
            "{}\n\nYou operate in a tool loop. Text responses are NEVER shown to the user \
             — use the speak tool to respond. NEVER output raw text.{}",
            system, format_tools_for_prompt(&tools)
        );
        vec![
            ("system".to_string(), full_system),
            ("user".to_string(), user.to_string()),
        ]
    }

    // =========================================================================
    // Extração de texto
    // =========================================================================

    #[test]
    fn extract_text_from_text_variant() {
        let content = MessageContent::Text("Olá mundo".to_string());
        assert_eq!(extract_text_content(&content), "Olá mundo");
    }

    #[test]
    fn extract_text_from_content_blocks_inclui_text_e_tool_use() {
        let content = MessageContent::ContentBlocks(vec![
            ContentBlock::Text { text: "primeira linha".to_string() },
            ContentBlock::ToolUse {
                id: "tu_1".to_string(),
                name: "search".to_string(),
                input: serde_json::json!({"query": "test"}),
            },
            ContentBlock::Text { text: "segunda linha".to_string() },
        ]);
        let result = extract_text_content(&content);
        assert!(result.contains("primeira linha"));
        assert!(result.contains("search"));
        assert!(result.contains("segunda linha"));
    }

    #[test]
    fn extract_text_tool_use_gera_texto_legivel() {
        let content = MessageContent::ContentBlocks(vec![
            ContentBlock::ToolUse {
                id: "tu_1".to_string(),
                name: "describe_individual".to_string(),
                input: serde_json::json!({"iris": ["foundation:X"]}),
            },
        ]);
        let result = extract_text_content(&content);
        assert!(result.contains("describe_individual"), "deve incluir nome da ferramenta");
    }

    #[test]
    fn extract_text_tool_result_gera_texto_legivel() {
        let content = MessageContent::ContentBlocks(vec![
            ContentBlock::ToolResult {
                tool_use_id: "tu_1".to_string(),
                content: serde_json::json!("Resultado da ferramenta aqui"),
                is_error: None,
            },
        ]);
        let result = extract_text_content(&content);
        assert!(result.contains("Resultado da ferramenta aqui"), "deve incluir o conteúdo do resultado");
    }

    // =========================================================================
    // Parser Python speak()
    // =========================================================================

    #[test]
    fn parser_python_extrai_mensagem_aspas_duplas() {
        let raw = r#"speak(message="Olá! Como posso ajudar?")"#;
        assert_eq!(parse_python_speak_output(raw), "Olá! Como posso ajudar?");
    }

    #[test]
    fn parser_python_extrai_mensagem_aspas_simples() {
        let raw = "speak(message='Olá mundo')";
        assert_eq!(parse_python_speak_output(raw), "Olá mundo");
    }

    #[test]
    fn parser_python_remove_marcador_end_of_turn() {
        let raw = "speak(message=\"Resposta\")\n<end_of_turn>";
        assert_eq!(parse_python_speak_output(raw), "Resposta");
    }

    #[test]
    fn parser_python_remove_marcador_eos() {
        let raw = "speak(message=\"Resposta\")<eos>";
        assert_eq!(parse_python_speak_output(raw), "Resposta");
    }

    #[test]
    fn parser_python_trata_aspas_escapadas() {
        let raw = r#"speak(message="Ele disse \"olá\" para mim")"#;
        assert_eq!(parse_python_speak_output(raw), r#"Ele disse "olá" para mim"#);
    }

    #[test]
    fn parser_python_usa_ultimo_speak_quando_modelo_escreve_raciocinio_antes() {
        let raw = "Deixa eu pensar...\nspeak(message=\"Resposta final\")";
        assert_eq!(parse_python_speak_output(raw), "Resposta final");
    }

    #[test]
    fn parser_python_fallback_para_texto_limpo_sem_speak() {
        let raw = "Resposta sem wrapper\n<end_of_turn>";
        assert_eq!(parse_python_speak_output(raw), "Resposta sem wrapper");
    }

    #[test]
    fn parser_python_fallback_string_vazia() {
        assert_eq!(parse_python_speak_output(""), "");
    }

    #[test]
    fn parser_python_speak_com_iris_none_extrai_so_message() {
        let raw = r#"speak(message="Olá!", iris=None)"#;
        assert_eq!(parse_python_speak_output(raw), "Olá!");
    }

    // =========================================================================
    // Parser <tool_call>
    // =========================================================================

    #[test]
    fn parse_tool_call_speak_basico() {
        let raw = r#"<tool_call>{"name": "speak", "input": {"message": "Olá!"}}</tool_call>"#;
        let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("deve parsear speak");
        assert_eq!(tc.name, "speak");
        assert_eq!(tc.input["message"], "Olá!");
    }

    #[test]
    fn parse_tool_call_search() {
        let raw = r#"<tool_call>{"name": "search", "input": {"query": "capital do Brasil"}}</tool_call>"#;
        let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("deve parsear search");
        assert_eq!(tc.name, "search");
        assert_eq!(tc.input["query"], "capital do Brasil");
    }

    #[test]
    fn parse_tool_call_ignora_marcadores_eot() {
        let raw = "<tool_call>{\"name\": \"speak\", \"input\": {\"message\": \"ok\"}}</tool_call>\n<end_of_turn>";
        let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("deve parsear com EOT");
        assert_eq!(tc.name, "speak");
        assert_eq!(tc.input["message"], "ok");
    }

    #[test]
    fn parse_tool_call_retorna_vazio_sem_tag() {
        assert!(parse_all_tool_calls_from_output("texto sem tag").is_empty());
        assert!(parse_all_tool_calls_from_output("").is_empty());
    }

    #[test]
    fn parse_tool_call_retorna_vazio_json_invalido() {
        assert!(parse_all_tool_calls_from_output("<tool_call>não é json</tool_call>").is_empty());
    }

    #[test]
    fn parse_tool_call_multiplos_retorna_todos() {
        let raw = concat!(
            r#"<tool_call>{"name": "search", "input": {"query": "x"}}</tool_call>"#,
            r#"<tool_call>{"name": "speak", "input": {"message": "ok"}}</tool_call>"#,
        );
        let calls = parse_all_tool_calls_from_output(raw);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "search");
        assert_eq!(calls[1].name, "speak");
    }

    // =========================================================================
    // StreamProcessor
    // =========================================================================

    #[test]
    fn stream_processor_emite_texto_normal() {
        let mut p = StreamProcessor::new();
        let (emit, stop) = p.feed("olá mundo");
        assert_eq!(emit.as_deref(), Some("olá mundo"));
        assert!(!stop);
    }

    #[test]
    fn stream_processor_suprime_tool_call_completo() {
        let mut p = StreamProcessor::new();
        let tokens = ["<tool", "_call>", "{\"name\":", "\"speak\",", "\"input\":", "{\"message\":\"ok\"}}", "</tool_call>"];
        let mut emitted = String::new();
        let mut stopped = false;
        for tok in tokens {
            let (emit, stop) = p.feed(tok);
            if let Some(t) = emit { emitted.push_str(&t); }
            if stop { stopped = true; break; }
        }
        assert!(emitted.is_empty(), "nenhum texto deve ter sido emitido: {:?}", emitted);
        assert!(!stopped, "não deve parar após </tool_call> — processamento continua");
    }

    #[test]
    fn stream_processor_emite_texto_antes_de_tool_call() {
        let mut p = StreamProcessor::new();
        // Texto antes do <tool_call> deve ser emitido
        let (emit, _) = p.feed("texto antes");
        assert_eq!(emit.as_deref(), Some("texto antes"));
        // <tool_call> chega em seguida
        let (emit2, _) = p.feed("<tool_call>");
        assert!(emit2.is_none());
        assert_eq!(p.state, StreamState::InToolCall);
    }

    #[test]
    fn stream_processor_falso_alarme_emite_lookahead() {
        let mut p = StreamProcessor::new();
        // '<' sozinho não é tool_call — após >20 chars deve flush
        let (e1, _) = p.feed("<");
        assert!(e1.is_none()); // buffering
        let (e2, _) = p.feed("p>algum html que nao é tool call longo o suficiente");
        // Após o lookahead ultrapassar 20 chars, faz flush
        assert!(e2.is_some(), "deve ter feito flush do lookahead");
        assert_eq!(p.state, StreamState::Text);
    }

    #[test]
    fn stream_processor_flush_pending_text_retorna_lookahead_nao_emitido() {
        let mut p = StreamProcessor::new();
        p.feed("<poss");
        // Ainda em MaybeTool com lookahead não resolvido
        let pending = p.flush_pending_text();
        assert!(pending.is_some());
        assert!(pending.unwrap().contains("<poss"));
    }

    // =========================================================================
    // Truncamento de contexto
    // =========================================================================

    #[test]
    fn truncate_nao_remove_quando_cabem() {
        let mut msgs = vec![
            ("system".to_string(), "system prompt curto".to_string()),
            ("user".to_string(), "mensagem curta".to_string()),
        ];
        truncate_messages_if_needed(&mut msgs);
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn truncate_remove_mensagens_antigas_mantendo_sistema_e_ultima() {
        let large = "x".repeat(MAX_PROMPT_CHARS / 2 + 100);
        let mut msgs = vec![
            ("system".to_string(), "system".to_string()),
            ("user".to_string(), large.clone()),
            ("assistant".to_string(), "resposta".to_string()),
            ("user".to_string(), "ultima mensagem".to_string()),
        ];
        truncate_messages_if_needed(&mut msgs);
        // sistema e última mensagem devem estar presentes
        assert_eq!(msgs[0].0, "system");
        assert_eq!(msgs.last().unwrap().1, "ultima mensagem");
        // total deve estar abaixo do limite
        let total: usize = msgs.iter().map(|(_, c)| c.len()).sum();
        assert!(total <= MAX_PROMPT_CHARS, "total {} > limite {}", total, MAX_PROMPT_CHARS);
    }

    // =========================================================================
    // format_tools_for_prompt
    // =========================================================================

    #[test]
    fn format_tools_vazio_retorna_vazio() {
        assert_eq!(format_tools_for_prompt(&[]), "");
    }

    #[test]
    fn format_tools_contem_nome_e_formato() {
        let tools = vec![speak_tool()];
        let prompt = format_tools_for_prompt(&tools);
        assert!(prompt.contains("speak"));
        assert!(prompt.contains("<tool_call>"));
        assert!(prompt.contains("message"));
    }

    #[test]
    fn format_tools_multiplas_ferramentas() {
        let tools = vec![
            speak_tool(),
            ClaudeTool {
                name: "search".to_string(),
                description: "Busca no grafo".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "required": ["query"]
                }),
            },
        ];
        let prompt = format_tools_for_prompt(&tools);
        assert!(prompt.contains("speak"));
        assert!(prompt.contains("search"));
        assert!(prompt.contains("query: string"));
    }

    // =========================================================================
    // Testes de integração — requerem o modelo GGUF em disco
    //
    // Execute com:
    //   cargo test --manifest-path src-tauri/Cargo.toml --lib -- local::tests \
    //     --include-ignored --test-threads=1 2>&1 | grep -E "test |FAILED|ok|raw|parsed"
    // =========================================================================

    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn integracao_modelo_carrega_e_cache_funciona() {
        let m1 = load_model(MODEL_E4B).await.expect("falha ao carregar modelo");
        let m2 = load_model(MODEL_E4B).await.expect("falha no cache");
        assert!(Arc::ptr_eq(&m1, &m2), "segunda chamada deve retornar o mesmo Arc");
    }

    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn integracao_modelo_usa_formato_tool_call() {
        let model = load_model(MODEL_E4B).await.expect("falha ao carregar modelo");
        let msgs = messages_with_injection("Você é um assistente útil.", "Diga apenas 'olá'.");

        let mut raw = String::new();
        run_inference(&model, &msgs, |p| { raw.push_str(&p); true }).expect("inferência falhou");

        println!("raw output: {:?}", raw);

        let tc = parse_all_tool_calls_from_output(&raw).into_iter().next();
        assert!(tc.is_some(), "modelo não usou formato <tool_call>. Output: {:?}", raw);
        assert_eq!(tc.unwrap().name, "speak");
    }

    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn integracao_parser_extrai_mensagem_limpa() {
        let model = load_model(MODEL_E4B).await.expect("falha ao carregar modelo");
        let msgs = messages_with_injection("Você é um assistente útil.", "Diga apenas 'olá'.");

        let mut raw = String::new();
        run_inference(&model, &msgs, |p| { raw.push_str(&p); true }).expect("inferência falhou");

        println!("raw output: {:?}", raw);

        let tc = parse_all_tool_calls_from_output(&raw).into_iter().next();
        assert!(tc.is_some(), "modelo não gerou <tool_call>. Output: {:?}", raw);
        let message = tc.unwrap().input["message"].as_str().unwrap_or("").to_string();

        println!("parsed message: {:?}", message);
        assert!(!message.is_empty(), "mensagem parseada está vazia");
        assert!(!message.contains("<tool_call>"), "tag tool_call vazou para a mensagem");
        assert!(!message.contains("<end_of_turn>"), "marcador EOT não foi removido");
    }

    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn integracao_modelo_retorna_stats_de_tokens() {
        let model = load_model(MODEL_E4B).await.expect("falha ao carregar modelo");
        let msgs = messages_with_injection("Você é um assistente útil.", "Diga apenas 'olá'.");

        let stats = run_inference(&model, &msgs, |_| true).expect("inferência falhou");

        assert!(stats.prompt_tokens > 0, "prompt_tokens deve ser > 0");
        assert!(stats.completion_tokens > 0, "completion_tokens deve ser > 0");
        println!("prompt: {} tokens, completion: {} tokens", stats.prompt_tokens, stats.completion_tokens);
    }

    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn integracao_modelo_responde_em_portugues() {
        let model = load_model(MODEL_E4B).await.expect("falha ao carregar modelo");
        let msgs = messages_with_injection(
            "Você é um assistente útil. Responda sempre em português.",
            "Qual é a capital do Brasil?",
        );

        let mut raw = String::new();
        run_inference(&model, &msgs, |p| { raw.push_str(&p); true }).expect("inferência falhou");

        let message = parse_all_tool_calls_from_output(&raw)
            .into_iter()
            .next()
            .and_then(|tc| tc.input["message"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| parse_python_speak_output(&raw));

        println!("parsed: {:?}", message);
        let lower = message.to_lowercase();
        assert!(
            lower.contains("brasília") || lower.contains("brasilia"),
            "resposta não mencionou Brasília. Got: {:?}", message
        );
    }

    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn integracao_modelo_nao_encontrado_retorna_erro_claro() {
        let result = load_model("/tmp/modelo_inexistente.gguf").await;
        let err = result.expect_err("esperava Err, mas load_model retornou Ok");
        assert!(err.contains("not found"), "erro deveria mencionar 'not found': {}", err);
    }

    // =========================================================================
    // Testes de cenário: tool loop, artefatos de sistema, multi-turn
    //
    // Execute com:
    //   cargo test --manifest-path src-tauri/Cargo.toml --lib \
    //     -- local::tests::cenario --include-ignored --test-threads=1 --nocapture
    // =========================================================================

    /// Cenário: pergunta simples sem histórico → modelo deve chamar speak.
    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn cenario_speak_simples() {
        let tools = vec![speak_tool()];
        let msgs = build_turns(
            "You are a helpful assistant.",
            &tools,
            vec![("user", MessageContent::Text("What is 2 + 2?".into()))],
        );
        let (_, name, msg) = run_and_parse(MODEL_E4B, &msgs).await;
        assert_eq!(name.as_deref(), Some("speak"), "deve chamar speak");
        let msg = msg.expect("speak deve ter message");
        assert!(msg.contains('4'), "resposta deve conter '4'. Got: {:?}", msg);
    }

    /// Cenário: resultado de ferramenta no histórico → modelo deve chamar speak, não repetir a ferramenta.
    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn cenario_speak_apos_resultado_de_ferramenta() {
        let tools = vec![speak_tool(), search_tool()];
        let msgs = build_turns(
            "You are a helpful assistant.",
            &tools,
            vec![
                ("user", MessageContent::Text("Find all tasks due today.".into())),
                ("assistant", MessageContent::ContentBlocks(vec![
                    ContentBlock::ToolUse {
                        id: "t1".into(),
                        name: "search".into(),
                        input: serde_json::json!({"query": "tasks due today"}),
                    },
                ])),
                ("user", MessageContent::ContentBlocks(vec![
                    ContentBlock::ToolResult {
                        tool_use_id: "t1".into(),
                        content: serde_json::json!({"entities": [{"label": "Pay rent", "scheduledAt": "2026-04-15"}], "total": 1}),
                        is_error: None,
                    },
                ])),
            ],
        );
        let (_, name, msg) = run_and_parse(MODEL_E4B, &msgs).await;
        assert_eq!(name.as_deref(), Some("speak"), "deve chamar speak após ter o resultado");
        let msg = msg.expect("speak deve ter message");
        println!("speak message: {:?}", msg);
        assert!(!msg.is_empty(), "mensagem não pode estar vazia");
    }

    /// Cenário: ferramenta já chamada no histórico → modelo não deve repeti-la com os mesmos args.
    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn cenario_sem_loop_de_ferramenta() {
        let tools = vec![speak_tool(), describe_tool()];
        let msgs = build_turns(
            "You are a helpful assistant.",
            &tools,
            vec![
                ("user", MessageContent::Text("Tell me about foundation:Task_123.".into())),
                ("assistant", MessageContent::ContentBlocks(vec![
                    ContentBlock::ToolUse {
                        id: "t1".into(),
                        name: "describe_individual".into(),
                        input: serde_json::json!({"iris": ["foundation:Task_123"]}),
                    },
                ])),
                ("user", MessageContent::ContentBlocks(vec![
                    ContentBlock::ToolResult {
                        tool_use_id: "t1".into(),
                        content: serde_json::json!({"label": "Buy groceries", "status": "Pending", "scheduledAt": "2026-04-15"}),
                        is_error: None,
                    },
                ])),
            ],
        );
        let (_, name, msg) = run_and_parse(MODEL_E4B, &msgs).await;
        assert_ne!(name.as_deref(), Some("describe_individual"),
            "não deve repetir describe_individual. Got: {:?}", name);
        assert_eq!(name.as_deref(), Some("speak"), "deve chamar speak com o resumo");
        let msg = msg.expect("speak deve ter message");
        println!("speak message: {:?}", msg);
        assert!(!msg.is_empty());
    }

    /// Cenário: modelo deve emitir múltiplos tool_calls na mesma resposta.
    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn cenario_multitool_na_mesma_resposta() {
        let tools = vec![speak_tool(), describe_tool()];
        let msgs = build_turns(
            "You are a helpful assistant. When the user asks to describe multiple entities, \
             call describe_individual for each entity in a single response — emit one <tool_call> \
             per entity, all in the same reply.",
            &tools,
            vec![
                ("user", MessageContent::Text(
                    "Describe both foundation:Task_A and foundation:Task_B right now. \
                     Call describe_individual twice in this response.".into()
                )),
            ],
        );
        let (raw, calls) = run_and_parse_all(MODEL_E4B, &msgs).await;
        println!("tool calls ({} total): {:?}", calls.len(),
            calls.iter().map(|c| &c.name).collect::<Vec<_>>());
        assert!(
            calls.len() >= 2,
            "deve emitir pelo menos 2 tool_calls na mesma resposta. Obteve {} — raw: {:?}",
            calls.len(), raw
        );
        let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names.contains(&"describe_individual"),
            "deve chamar describe_individual. Obteve: {:?}", names
        );
    }

    /// Cenário: mensagem do speak não deve conter artefatos do template Gemma.
    #[tokio::test]
    #[ignore = "requer resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
    async fn cenario_speak_sem_artefatos_de_sistema() {
        let tools = vec![speak_tool(), search_tool()];
        let msgs = build_turns(
            "You are a helpful assistant.",
            &tools,
            vec![
                ("user", MessageContent::Text("List my projects.".into())),
                ("assistant", MessageContent::ContentBlocks(vec![
                    ContentBlock::ToolUse {
                        id: "t1".into(),
                        name: "search".into(),
                        input: serde_json::json!({"class_iri": "foundation:Project"}),
                    },
                ])),
                ("user", MessageContent::ContentBlocks(vec![
                    ContentBlock::ToolResult {
                        tool_use_id: "t1".into(),
                        content: serde_json::json!({"entities": [{"label": "FOUNDATION"}, {"label": "Website"}], "total": 2}),
                        is_error: None,
                    },
                ])),
            ],
        );
        let (_, _, msg) = run_and_parse(MODEL_E4B, &msgs).await;
        let msg = msg.unwrap_or_default();
        println!("speak message: {:?}", msg);
        assert!(!msg.contains("<start_of_turn>"), "artefato <start_of_turn> no speak");
        assert!(!msg.contains("<end_of_turn>"), "artefato <end_of_turn> no speak");
        assert!(!msg.contains("[called "), "prefixo [called] vazou para o speak");
        assert!(!msg.contains("<result>"), "tag <result> vazou para o speak");
        assert!(!msg.contains("<action>"), "tag <action> vazou para o speak");
    }
}
