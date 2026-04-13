use crate::owl::{Individual, DbExecutor};
use crate::ai::functions::ToolCall;
use crate::commands::chat_storage::{AIConversationMessage, ContentBlock, load_message, create_message};
use super::super::log_backend;
use tauri::Emitter;

/// Returns `(tool_result_msg_iri, had_successful_speak)`.
/// `had_successful_speak` is true when a `speak` tool was called and delivered without error,
/// signalling the loop to break — the AI has communicated with the user.
pub async fn execute_tools_from_message(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    assistant_message: &AIConversationMessage,
) -> Result<(String, bool), String> {
    let tool_use_ids: Vec<String> = assistant_message.content.iter()
        .filter_map(|b| {
            if let ContentBlock::ToolUse { id, .. } = b { Some(id.clone()) } else { None }
        })
        .collect();

    if tool_use_ids.is_empty() {
        return Err("No tool use blocks found in message".to_string());
    }

    // Guard against storing duplicate tool_results for the same tool_use_id.
    // This can happen when the recovery path re-executes tools that already ran.
    let conv_id_check = conversation_id.to_string();
    let ids_to_check = tool_use_ids;
    let existing_iri = executor.read(move |conn| {
        let message_iris = Individual::find_by_class_and_properties(
            conn,
            "foundation:AIConversationMessage",
            &[("foundation:partOfConversation", &conv_id_check)],
        ).map_err(|e| format!("Failed to query messages: {}", e))?;

        for iri in message_iris {
            if let Ok(msg) = load_message(conn, &iri) {
                let is_duplicate = msg.content.iter().any(|b| {
                    if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                        ids_to_check.contains(tool_use_id)
                    } else {
                        false
                    }
                });
                if is_duplicate {
                    return Ok(Some(iri));
                }
            }
        }
        Ok(None)
    }).await?;

    if let Some(iri) = existing_iri {
        log_backend("warn", &format!(
            "[CHAT] Skipping duplicate tool_result — results already stored in: {}", iri
        ));
        return Ok((iri, false));
    }

    let mut tool_results = Vec::new();
    let mut had_successful_speak = false;

    for block in &assistant_message.content {
        if let ContentBlock::ToolUse { id, name, input } = block {
            let (content, is_error) = execute_tool(executor, app, conversation_id, name, input).await;
            if name == "speak" && !is_error {
                had_successful_speak = true;
            }
            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: id.clone(),
                content,
                is_error: Some(is_error),
            });
        }
    }

    let content_json = serde_json::to_string(&tool_results)
        .map_err(|e| format!("Failed to serialize tool results: {}", e))?;

    let iri = create_message(executor, conversation_id, "user", &content_json, None, None, None).await?;
    app.emit("chat-message-added", serde_json::json!({"conversationId": conversation_id})).ok();
    Ok((iri, had_successful_speak))
}

use super::SPEAK_MAX_CHARS;
const WIDGET_CASCADE_OFFSET_PX: f64 = 50.0;

/// Execute a speak tool call before saving any assistant message.
/// Returns `(spoken_text, is_error)`.
/// On success: shows widgets (if iris provided) and returns the text to store as a clean message.
/// On failure: returns the error description.
pub async fn try_speak(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    input: &serde_json::Value,
) -> (String, bool) {
    let message = input.get("message").and_then(|v| v.as_str()).unwrap_or("");
    if message.chars().count() > SPEAK_MAX_CHARS {
        return (format!(
            "Too long: {} chars (max {}). Shorten to {} chars or fewer and retry.",
            message.chars().count(), SPEAK_MAX_CHARS, SPEAK_MAX_CHARS
        ), true);
    }
    let iris: Vec<String> = if let Some(arr) = input.get("iris").and_then(|v| v.as_array()) {
        arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
    } else if let Some(s) = input.get("iris").and_then(|v| v.as_str()) {
        serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
    } else {
        Vec::new()
    };
    if !iris.is_empty() {
        show_widgets_for_iris(executor, app, conversation_id, iris).await;
    }
    (message.to_string(), false)
}

async fn execute_tool(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    name: &str,
    input: &serde_json::Value,
) -> (String, bool) {
    if name == "speak" {
        let message = input.get("message").and_then(|v| v.as_str()).unwrap_or("");
        if message.chars().count() > SPEAK_MAX_CHARS {
            return (format!(
                "Too long: {} chars (max {}). Shorten to a single message of {} chars or fewer and retry once.",
                message.chars().count(), SPEAK_MAX_CHARS, SPEAK_MAX_CHARS
            ), true);
        }
        let iris: Vec<String> = if let Some(arr) = input.get("iris").and_then(|v| v.as_array()) {
            arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
        } else if let Some(s) = input.get("iris").and_then(|v| v.as_str()) {
            serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
        } else {
            Vec::new()
        };
        if !iris.is_empty() {
            show_widgets_for_iris(executor, app, conversation_id, iris).await;
        }
        return ("Delivered.".to_string(), false);
    }

    let call = ToolCall {
        name: name.to_string(),
        arguments: input.clone(),
    };

    let app_clone = app.clone();
    let conv_id = conversation_id.to_string();
    let result_json = match executor.write(move |conn| {
        let result = crate::ai::functions::execute_tool(conn, &call, Some(&app_clone), Some(&conv_id));
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await {
        Ok(json) => json,
        Err(e) => return (format!("{{\"success\":false,\"error\":\"{}\"}}", e), true),
    };

    let tool_result: crate::ai::functions::ToolResult = match serde_json::from_str(&result_json) {
        Ok(r) => r,
        Err(e) => return (format!("{{\"success\":false,\"error\":\"{}\"}}", e), true),
    };

    if tool_result.success {
        let content = tool_result.result
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| v.to_string()))
            .unwrap_or_default();
        (content, false)
    } else {
        (result_json, true)
    }
}

async fn show_widgets_for_iris(
    executor: &DbExecutor,
    app: &tauri::AppHandle,
    conversation_id: &str,
    iris: Vec<String>,
) {
    use crate::commands::widget::{self, Widget, Position, Size, WindowState};
    use tauri::Emitter;

    let conv_id = conversation_id.to_string();
    let app_clone = app.clone();

    let _ = executor.write(move |conn| {
        let widget_types = widget::blackboard__list_widget_types(conn);

        let entity_types = crate::eavto::query::get_first_iri_property_batch(
            conn, &iris, "rdf:type",
        ).unwrap_or_default();

        let mut offset_count = 0usize;

        for entity_id in &iris {
            let class_iri = entity_types.get(entity_id).cloned().unwrap_or_default();

            let matched = widget_types.iter()
                .find(|t| t.supported_class != "owl:Thing" && t.supported_class == class_iri)
                .or_else(|| widget_types.iter().find(|t| t.id == "inspector"));

            let (widget_type_id, width, height) = match matched {
                Some(t) => (t.id.clone(), t.default_size.width, t.default_size.height),
                None => ("inspector".to_string(), 480.0, 600.0),
            };

            let offset = offset_count as f64 * WIDGET_CASCADE_OFFSET_PX;
            let sanitized_entity = entity_id.replace([':', '/', '#', ' '], "_");
            let conv_suffix = conv_id.replace([':', '/', '#', ' '], "_");

            let w = Widget {
                id: format!("foundation:Widget_{}_{}_{}", widget_type_id, sanitized_entity, conv_suffix),
                widget_type: widget_type_id,
                entity_id: entity_id.clone(),
                position: Position { x: 100.0 + offset, y: 100.0 + offset },
                size: Size { width, height },
                window_state: WindowState::Normal,
                conversation_iri: Some(conv_id.clone()),
            };

            if widget::owl_insert_widget(conn, &w).is_ok() {
                app_clone.emit("widget-added", w).ok();
                offset_count += 1;
            }
        }

        Ok(String::new())
    }).await;
}

#[cfg(test)]
mod tests {
    use super::SPEAK_MAX_CHARS;

    #[test]
    fn speak_rejects_messages_over_288_characters() {
        let long_message = "a".repeat(SPEAK_MAX_CHARS + 1);
        let input = serde_json::json!({ "message": long_message });
        let message = input.get("message").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            message.chars().count() > SPEAK_MAX_CHARS,
            "message of {} chars must exceed SPEAK_MAX_CHARS={}",
            message.chars().count(), SPEAK_MAX_CHARS
        );
        let is_too_long = message.chars().count() > SPEAK_MAX_CHARS;
        assert!(is_too_long, "speak must return an error for messages over {} chars", SPEAK_MAX_CHARS);
    }

    #[test]
    fn speak_accepts_messages_exactly_288_characters() {
        let exact_message = "a".repeat(SPEAK_MAX_CHARS);
        let input = serde_json::json!({ "message": exact_message });
        let message = input.get("message").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(
            message.chars().count(), SPEAK_MAX_CHARS,
            "message must be exactly {} chars", SPEAK_MAX_CHARS
        );
        let is_valid = message.chars().count() <= SPEAK_MAX_CHARS;
        assert!(is_valid, "speak must accept messages of exactly {} chars", SPEAK_MAX_CHARS);
    }

    #[test]
    fn speak_max_chars_is_288() {
        assert_eq!(SPEAK_MAX_CHARS, 288);
    }
}
