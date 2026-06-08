use crate::owl::{DbExecutor, Individual, Object};
use crate::commands::chat_storage::{ContentBlock, create_message, load_conversation_history};
use crate::commands::chat::settings::AgentConfig;
use crate::ai::{AiProvider, ChatMessage, GenerateRequest};
use crate::ai::providers::{ClaudeProvider, OpenRouterProvider};
use crate::ai::local::LocalProvider;
use super::super::log_backend;
use tauri::Emitter;

const COMPACTION_THRESHOLD: f64 = 0.80;

/// Checks if compaction is needed and runs it synchronously before the first LLM call.
/// Called at the start of run_conversation_loop so the model always sees the compacted
/// context from the first iteration of the turn. Non-critical: failures are logged only.
pub async fn run_compaction_if_needed(
    executor: DbExecutor,
    app: tauri::AppHandle,
    conversation_id: String,
    cfg: CompactionConfig,
    total_tokens: usize,
) {
    let threshold = (cfg.max_tokens as f64 * COMPACTION_THRESHOLD) as usize;
    if total_tokens < threshold {
        return;
    }

    log_backend("info", &format!(
        "[COMPACTION] Triggering compaction for {} ({}/{} tokens)",
        conversation_id, total_tokens, cfg.max_tokens
    ));

    if let Err(e) = run_compaction(executor, app, conversation_id, cfg).await {
        log_backend("warn", &format!("[COMPACTION] Failed (non-critical): {}", e));
    }
}

/// Minimal config snapshot passed to the background task — avoids holding AgentConfig across await.
pub struct CompactionConfig {
    pub max_tokens: usize,
    pub api_key: String,
    pub base_url: String,
    pub model_identifier: String,
    pub fallback_models: Vec<String>,
    pub timeout_secs: u64,
}

impl CompactionConfig {
    pub fn from_agent_config(cfg: &AgentConfig) -> Self {
        Self {
            max_tokens: cfg.max_tokens,
            api_key: cfg.api_key.clone(),
            base_url: cfg.base_url.clone(),
            model_identifier: cfg.model_identifier.clone(),
            fallback_models: cfg.fallback_models.clone(),
            timeout_secs: cfg.timeout_secs,
        }
    }
}

async fn run_compaction(
    executor: DbExecutor,
    app: tauri::AppHandle,
    conversation_id: String,
    cfg: CompactionConfig,
) -> Result<(), String> {
    app.emit("ai-status", serde_json::json!({
        "status": "Compacting history...",
        "conversationId": conversation_id
    })).ok();

    // Load full history (no token budget — we want everything)
    let (history, _) = load_conversation_history(&executor, &conversation_id, usize::MAX).await?;

    // Only compact real persisted messages — exclude compaction summaries and synthetic
    // messages that load_conversation_history may inject on the compacted path.
    let normal_msgs: Vec<_> = history.iter()
        .filter(|m| m.role != "compaction" && !m.iri.starts_with("synthetic:"))
        .collect();

    // Keep the most recent 10 messages intact so they remain as "live" context
    const TAIL_SIZE: usize = 10;
    if normal_msgs.len() <= TAIL_SIZE {
        log_backend("info", "[COMPACTION] Not enough messages to compact, skipping");
        app.emit("ai-status", serde_json::json!({"conversationId": conversation_id})).ok();
        return Ok(());
    }

    let to_compact = &normal_msgs[..normal_msgs.len() - TAIL_SIZE];
    let summary_up_to_iri = to_compact.last()
        .map(|m| m.iri.clone())
        .ok_or("No messages to compact")?;

    // Build the summarization prompt from the messages to compact
    let history_text: String = to_compact.iter().map(|msg| {
        let text: String = msg.content.iter().filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        }).collect::<Vec<_>>().join(" ");
        format!("[{}]: {}", msg.role, text)
    }).filter(|s| !s.trim_end_matches(|c| c == '[' || c == ']' || c == ' ' || c == ':').is_empty())
    .collect::<Vec<_>>().join("\n");

    if history_text.trim().is_empty() {
        log_backend("info", "[COMPACTION] Nothing meaningful to summarize, skipping");
        app.emit("ai-status", serde_json::json!({"conversationId": conversation_id})).ok();
        return Ok(());
    }

    let prompt = format!(
        "Create a compact summary of this conversation preserving: the user's goals, \
         decisions made, important technical context, and relevant IRIs/entities mentioned. \
         Write densely and factually, without introduction or conclusion.\n\n\
         CONVERSATION:\n{}", history_text
    );

    let provider = build_provider(&cfg);
    let request = GenerateRequest {
        messages: vec![ChatMessage::text("user", &prompt)],
        max_tokens: Some(2048),
        temperature: Some(0.3),
        system: None,
        blackboard_context: None,
        tools: None,
        supports_web_tools: false,
        thinking: None,
        tool_choice: None,
    };

    let response = provider.generate(request).await?;
    let summary_text = response.content.trim().to_string();

    if summary_text.is_empty() {
        return Err("Model returned empty summary".to_string());
    }

    // Save compaction message to conversation
    let compaction_iri = create_message(
        &executor,
        &conversation_id,
        "compaction",
        vec![ContentBlock::CompactionSummary { text: summary_text }],
        None,
        None,
        None,
    ).await?;

    log_backend("info", &format!(
        "[COMPACTION] Saved compaction message {} (up to {})",
        compaction_iri, summary_up_to_iri
    ));

    // Replace (not append) summaryUpToMessage so exactly one value exists at all times.
    let conv_id_for_write = conversation_id.clone();
    let summary_up_to = summary_up_to_iri.clone();
    executor.write(move |conn| {
        Individual::clear_property(conn, &conv_id_for_write, "foundation:summaryUpToMessage", "ai")
            .map_err(|e| format!("Failed to clear summaryUpToMessage: {}", e))?;
        Individual::new(&conv_id_for_write)
            .add_property(conn, "foundation:summaryUpToMessage", vec![Object::Iri(summary_up_to)], "ai")
            .map_err(|e| format!("Failed to set summaryUpToMessage: {}", e))?;
        Ok(String::new())
    }).await?;

    app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();
    app.emit("ai-status", serde_json::json!({"conversationId": conversation_id})).ok();

    Ok(())
}

fn build_provider(cfg: &CompactionConfig) -> AiProvider {
    if cfg.base_url.contains("openrouter.ai") {
        AiProvider::OpenRouter(OpenRouterProvider::with_model(
            cfg.api_key.clone(),
            cfg.base_url.clone(),
            cfg.model_identifier.clone(),
            cfg.fallback_models.clone(),
            cfg.timeout_secs,
        ))
    } else if cfg.api_key.is_empty() {
        AiProvider::Local(LocalProvider::new(String::new()))
    } else {
        AiProvider::Claude(ClaudeProvider::with_model(
            cfg.api_key.clone(),
            cfg.model_identifier.clone(),
            cfg.timeout_secs,
        ))
    }
}
