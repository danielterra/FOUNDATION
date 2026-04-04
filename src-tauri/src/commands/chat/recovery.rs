use crate::owl::{Individual, Connection, DbExecutor};
use super::settings::load_agent_config;

pub fn delete_messages_from_timestamp(
    conn: &mut Connection,
    conversation_iri: &str,
    from_timestamp: i64,
    exclude_exact: bool,
) -> Result<(), String> {
    let message_iris = Individual::find_by_class_and_properties(
        conn,
        "foundation:AIConversationMessage",
        &[("foundation:partOfConversation", conversation_iri)],
    ).map_err(|e| format!("Failed to query messages: {}", e))?;

    for iri in message_iris {
        let ind = match Individual::get(conn, &iri) {
            Ok(Some(i)) => i,
            _ => continue,
        };

        let ts = match ind.properties.iter()
            .find(|(k, _)| k == "foundation:sentAt")
            .and_then(|(_, v)| super::parse_timestamp(v))
        {
            Some(t) => t,
            None => continue,
        };

        let should_delete = if exclude_exact {
            ts > from_timestamp
        } else {
            ts >= from_timestamp
        };

        if should_delete {
            Individual::retract(conn, &iri, "chat")
                .map_err(|e| format!("Failed to retract message {}: {}", iri, e))?;
        }
    }

    Ok(())
}

pub async fn run_conversation_from_current_state(
    app: tauri::AppHandle,
    executor: DbExecutor,
    conversation_id: String,
    cancellation: &super::cancellation::AiCancellationState,
    silent: bool,
) -> Result<(), String> {
    let conv_id_for_config = conversation_id.clone();
    let agent_config = executor.read(move |conn| {
        load_agent_config(conn, &conv_id_for_config)
    }).await?;

    super::engine::run_conversation_loop(
        &app, &executor, &conversation_id,
        &agent_config, None, silent, cancellation,
    ).await?;

    if !silent {
        use tauri::Emitter;
        app.emit("ai-status", serde_json::json!({ "status": null, "conversationId": conversation_id })).ok();
    }

    Ok(())
}
