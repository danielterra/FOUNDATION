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
use crate::ai::providers::{MessageContent, ToolDefinition, UsageInfo};

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

const LOCAL_CONTEXT_WINDOW: usize = 65536;
const LOCAL_MAX_OUTPUT_TOKENS: usize = 2048;
const MAX_PROMPT_CHARS: usize = (LOCAL_CONTEXT_WINDOW - LOCAL_MAX_OUTPUT_TOKENS) * 35 / 10;
const LOOKAHEAD_FLUSH_THRESHOLD: usize = 20;
const TOOL_DESC_MAX_LEN: usize = 80;

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

        let tool_calls = parse_all_tool_calls_from_output(&full_text);

        crate::commands::log_backend("info", &format!(
            "[LOCAL AI] Parsed {} tool call(s): {:?}",
            tool_calls.len(),
            tool_calls.iter().map(|tc| tc.name.as_str()).collect::<Vec<_>>()
        ));

        Ok(GenerateResponse {
            content: String::new(),
            tool_calls,
            thinking_blocks: vec![],
            stop_reason: Some("tool_use".to_string()),
            usage,
            model_used: None,
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
    /// First tokens: buffer until we can confirm or deny "<|tool_call>" prefix.
    InitialLookahead,
    /// Normal text: emit each token immediately.
    Text,
    /// Saw `<` — buffering until we know if this is a tool call tag.
    MaybeTool,
    /// Inside a tool call block — suppress all output.
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
        Self { state: StreamState::InitialLookahead, lookahead: String::new() }
    }

    /// Feed one token. Returns `(text_to_emit, should_stop_inference)`.
    fn feed(&mut self, token: &str) -> (Option<String>, bool) {
        match self.state {
            StreamState::InitialLookahead => {
                const NATIVE_MARKER: &str = "<|tool_call>";
                self.lookahead.push_str(token);
                if self.lookahead.starts_with(NATIVE_MARKER) {
                    // Confirmed native Gemma tool call — suppress entire block.
                    self.lookahead.clear();
                    self.state = StreamState::InToolCall;
                    return (None, false);
                }
                if !NATIVE_MARKER.starts_with(self.lookahead.as_str()) {
                    // Can't be a native tool call — flush buffer and continue normally.
                    let flushed = std::mem::take(&mut self.lookahead);
                    if let Some(lt_pos) = flushed.find('<') {
                        let before = if lt_pos > 0 { Some(flushed[..lt_pos].to_string()) } else { None };
                        self.lookahead = flushed[lt_pos..].to_string();
                        self.state = StreamState::MaybeTool;
                        let (post, stop) = self.resolve_lookahead();
                        return (combine_text(before, post), stop);
                    } else {
                        self.state = StreamState::Text;
                        return (if flushed.is_empty() { None } else { Some(flushed) }, false);
                    }
                }
                // Still a valid prefix of "<|tool_call>" — keep buffering.
                (None, false)
            }
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
                if self.lookahead.contains("<tool_call|>") || self.lookahead.contains("</tool_call>") {
                    // Tool call complete — clear buffer and go back to Text so the
                    // model can emit more content or additional tool call tags.
                    self.lookahead.clear();
                    self.state = StreamState::Text;
                    (None, false)
                } else {
                    (None, false)
                }
            }
        }
    }

    /// Checks the current lookahead buffer and either confirms a tool call tag,
    /// flushes as plain text (false alarm), or keeps buffering.
    fn resolve_lookahead(&mut self) -> (Option<String>, bool) {
        // Native Gemma 4 format: <|tool_call>
        if let Some(tag_pos) = self.lookahead.find("<|tool_call>") {
            let before = if tag_pos > 0 { Some(self.lookahead[..tag_pos].to_string()) } else { None };
            self.lookahead.clear();
            self.state = StreamState::InToolCall;
            return (before, false);
        }
        // Legacy XML format: <tool_call>
        if let Some(tag_pos) = self.lookahead.find("<tool_call>") {
            let before = if tag_pos > 0 { Some(self.lookahead[..tag_pos].to_string()) } else { None };
            self.lookahead.clear();
            self.state = StreamState::InToolCall;
            return (before, false);
        }
        if self.lookahead.len() > LOOKAHEAD_FLUSH_THRESHOLD {
            // Not a tool call tag — flush as plain text.
            let flushed = std::mem::take(&mut self.lookahead);
            self.state = StreamState::Text;
            (Some(flushed), false)
        } else {
            (None, false)
        }
    }

    /// Returns buffered text that was held in lookahead states but never confirmed as a tag.
    /// Should be called after the receive loop ends to flush any remaining lookahead.
    fn flush_pending_text(&self) -> Option<String> {
        if matches!(self.state, StreamState::InitialLookahead | StreamState::Text | StreamState::MaybeTool)
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
fn format_tools_for_prompt(tools: &[ToolDefinition]) -> String {
    if tools.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "\n\n---\n## TOOL USE\n\n\
        Call tools using the native Gemma 4 format:\n\
        <|tool_call>call:TOOL_NAME{arg1:<|\"|>string value<|\"|>,arg2:42}<tool_call|>\n\n\
        String values use <|\"|>...<|\"|> delimiters. Numbers and booleans are bare. \
        Arrays use [<|\"|>item<|\"|>,...] syntax.\n\n\
        Rules:\n\
        - Use ONLY tools from the list below — NEVER invent tool names.\n\
        - Never repeat a tool call with identical arguments.\n\n\
        Examples of correct tool use:\n\
        \n\
        User: Show me my tasks for today.\n\
        <|tool_call>call:search{query:<|\"|>tasks due today<|\"|>,class_iri:<|\"|>foundation:Task<|\"|>}<tool_call|>\n\
        [tool result: {\"entities\": [{\"label\": \"Pay rent\", \"iri\": \"foundation:Task_1\"}], \"total\": 1}]\n\
        \n\
        User: What is foundation:Project_42?\n\
        <|tool_call>call:describe_individual{iris:[<|\"|>foundation:Project_42<|\"|>]}<tool_call|>\n\
        [tool result: {\"label\": \"Website Redesign\", \"status\": \"InProgress\"}]\n\
        \n\
        ## AVAILABLE TOOLS\n\n",
    );

    for tool in tools {
        out.push_str(&compact_tool_signature(tool));
        out.push('\n');
    }

    out
}

fn compact_tool_signature(tool: &ToolDefinition) -> String {
    let params = schema_params_signature(&tool.input_schema);
    let desc = tool.description.trim();
    let desc = if desc.len() > TOOL_DESC_MAX_LEN { &desc[..TOOL_DESC_MAX_LEN] } else { desc };
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

    // Array-mode tools have a single "operations" key wrapping an array of objects.
    // Expand the item schema so the model sees the actual field names instead of just "array".
    if props.len() == 1 {
        if let Some(ops) = props.get("operations") {
            if let Some(items) = ops.get("items") {
                if items.get("type").and_then(|t| t.as_str()) == Some("object") {
                    let inner = schema_params_signature(items);
                    return format!("operations: [{{{}}}]", inner);
                }
            }
        }
    }

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
// Gemma 4 native argument parser
// ---------------------------------------------------------------------------

/// Gemma 4 string delimiter token (rendered with special=true).
const GEMMA_STR_DELIM: &str = "<|\"|>";

/// Parses Gemma 4 native tool call args: `{key:<|"|>val<|"|>,key2:42}` → JSON Value.
fn parse_gemma_native_args(args_str: &str) -> serde_json::Value {
    let s = args_str.trim();
    if s.starts_with('{') {
        let inner = if s.ends_with('}') { &s[1..s.len() - 1] } else { &s[1..] };
        parse_gemma_object(inner)
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    }
}

fn parse_gemma_object(content: &str) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let mut remaining = content.trim();
    while !remaining.is_empty() {
        let colon = match remaining.find(':') {
            Some(p) => p,
            None => break,
        };
        let key = remaining[..colon].trim().to_string();
        remaining = remaining[colon + 1..].trim_start();
        if key.is_empty() { break; }
        let (val, rest) = parse_gemma_value(remaining);
        map.insert(key, val);
        remaining = rest.trim_start_matches(',').trim();
    }
    serde_json::Value::Object(map)
}

fn parse_gemma_value(s: &str) -> (serde_json::Value, &str) {
    if s.starts_with(GEMMA_STR_DELIM) {
        let rest = &s[GEMMA_STR_DELIM.len()..];
        if let Some(end) = rest.find(GEMMA_STR_DELIM) {
            (serde_json::Value::String(rest[..end].to_string()), &rest[end + GEMMA_STR_DELIM.len()..])
        } else {
            (serde_json::Value::String(rest.to_string()), "")
        }
    } else if s.starts_with('[') {
        let mut depth = 0usize;
        let mut end = s.len();
        for (i, c) in s.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 { end = i + 1; break; }
                }
                _ => {}
            }
        }
        let inner = &s[1..end.saturating_sub(1)];
        (parse_gemma_array(inner), &s[end..])
    } else if s.starts_with('{') {
        let mut depth = 0usize;
        let mut end = s.len();
        for (i, c) in s.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 { end = i + 1; break; }
                }
                _ => {}
            }
        }
        let inner = &s[1..end.saturating_sub(1)];
        (parse_gemma_object(inner), &s[end..])
    } else {
        let end = s.find(|c: char| c == ',' || c == '}' || c == ']').unwrap_or(s.len());
        let bare = s[..end].trim();
        let val = match bare {
            "true" => serde_json::Value::Bool(true),
            "false" => serde_json::Value::Bool(false),
            "null" => serde_json::Value::Null,
            n => {
                if let Ok(i) = n.parse::<i64>() { serde_json::json!(i) }
                else if let Ok(f) = n.parse::<f64>() { serde_json::json!(f) }
                else { serde_json::Value::String(n.to_string()) }
            }
        };
        (val, &s[end..])
    }
}

fn parse_gemma_array(content: &str) -> serde_json::Value {
    let mut items = Vec::new();
    let mut remaining = content.trim();
    while !remaining.is_empty() {
        let (val, rest) = parse_gemma_value(remaining);
        items.push(val);
        remaining = rest.trim_start_matches(',').trim();
    }
    serde_json::Value::Array(items)
}

// ---------------------------------------------------------------------------
// Output parsers
// ---------------------------------------------------------------------------

/// Parses ALL tool calls from the model output in order.
/// Primary: native Gemma 4 format `<|tool_call>call:name{args}<tool_call|>`.
/// Secondary: bare "tool_call\n{JSON}\n</tool_call>" (legacy fallback).
/// Tertiary: "<tool_call>{JSON}</tool_call>" (XML fallback).
fn parse_all_tool_calls_from_output(text: &str) -> Vec<ToolCall> {
    let text = strip_eot_markers(text);
    let mut calls: Vec<ToolCall> = Vec::new();

    // Primary: native Gemma 4 format — <|tool_call>call:name{args}<tool_call|>
    {
        const OPEN: &str = "<|tool_call>";
        const CLOSE: &str = "<tool_call|>";
        let mut offset = 0usize;
        while offset < text.len() {
            let Some(open_rel) = text[offset..].find(OPEN) else { break };
            let after_open = offset + open_rel + OPEN.len();
            let rest = &text[after_open..];
            let (call_content, new_offset) = if let Some(close_rel) = rest.find(CLOSE) {
                (&rest[..close_rel], after_open + close_rel + CLOSE.len())
            } else {
                (rest, text.len())
            };
            if let Some(name_args) = call_content.strip_prefix("call:") {
                if let Some(brace_pos) = name_args.find('{') {
                    let name = name_args[..brace_pos].trim();
                    if !name.is_empty() {
                        let args_str = &name_args[brace_pos..];
                        let input = parse_gemma_native_args(args_str);
                        let id = format!("local_{}_{}", chrono::Utc::now().timestamp_millis(), calls.len());
                        calls.push(ToolCall { id, name: name.to_string(), input });
                    }
                }
            }
            offset = new_offset;
        }
    }

    // Secondary: "tool_call\n{JSON}[\n</tool_call>]" (legacy format fallback)
    if calls.is_empty() {
        let marker = "tool_call\n";
        let mut offset = 0usize;
        while offset < text.len() {
            let Some(rel) = text[offset..].find(marker) else { break };
            let json_start = offset + rel + marker.len();
            if json_start >= text.len() { break; }
            let json_area = &text[json_start..];
            let json_len = json_area.find("\n</tool_call>")
                .or_else(|| json_area.find("</tool_call>"))
                .unwrap_or(json_area.len());
            let json_str = json_area[..json_len].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                    let input = v.get("input").cloned().unwrap_or(serde_json::json!({}));
                    let id = format!("local_{}_{}", chrono::Utc::now().timestamp_millis(), calls.len());
                    calls.push(ToolCall { id, name: name.to_string(), input });
                }
            }
            offset = (json_start + json_len + 1).min(text.len());
        }
    }

    // Tertiary: "<tool_call>{JSON}</tool_call>" (XML fallback)
    if calls.is_empty() {
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
    text.trim_end_matches("<tool_call|>")
        .trim_end_matches("<end_of_turn>")
        .trim_end_matches("<eos>")
        .trim_end_matches('\n')
        .trim()
}

/// Extracts the message from Python-style `speak(message="...")` output.
#[cfg(test)]
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
#[cfg(test)]
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

    let ctx = &mut ctx_guard.as_mut().expect("ctx_guard was just set to Some above").0;
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
            .token_to_piece(token, &mut decoder, true, None)
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

/// Converts a serde_json Value to Gemma 4 native arg format:
/// strings → `<|"|>value<|"|>`, arrays → `[item,...]`, objects → `{k:v,...}`.
fn format_gemma_native_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("{}{}{}", GEMMA_STR_DELIM, s, GEMMA_STR_DELIM),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_gemma_native_value).collect();
            format!("[{}]", items.join(","))
        }
        serde_json::Value::Object(map) => {
            let pairs: Vec<String> = map.iter()
                .map(|(k, v)| format!("{}:{}", k, format_gemma_native_value(v)))
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
        other => other.to_string(),
    }
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
                        // Render previous tool calls in the native Gemma 4 format so the model
                        // recognises them as past tool calls and uses the same format for new ones.
                        // Using [called name(args)] caused the model to hallucinate the entire
                        // conversation loop rather than emitting a new <|tool_call> block.
                        Some(format!("<|tool_call>call:{}{}<tool_call|>",
                            name,
                            format_gemma_native_value(input)
                        ))
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

#[cfg(test)]
mod tests;

