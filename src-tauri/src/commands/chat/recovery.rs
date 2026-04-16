use tauri::Manager;
use crate::owl::{Individual, Connection, DbExecutor, get_literal_property};
use super::settings::load_agent_config;
use super::cancellation::{AiCancellationState, ConversationProcessingState};

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

/// Returns true if the newest message in the conversation has role "user",
/// meaning there is a pending user message the engine has not yet responded to.
pub fn has_pending_user_message(conn: &Connection, conversation_iri: &str) -> bool {
    let iris = match Individual::find_messages_by_conversation(conn, conversation_iri, 1, 0) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let newest_iri = match iris.first() {
        Some(iri) => iri,
        None => return false,
    };
    matches!(
        get_literal_property(conn, newest_iri, "foundation:role"),
        Ok(Some(role)) if role == "user"
    )
}

pub async fn run_conversation_from_current_state(
    app: tauri::AppHandle,
    executor: DbExecutor,
    conversation_id: String,
    cancellation: &AiCancellationState,
    silent: bool,
) -> Result<(), String> {
    let conv_id_for_config = conversation_id.clone();
    let agent_config = executor.read(move |conn| {
        load_agent_config(conn, &conv_id_for_config)
    }).await?;

    super::engine::run_conversation_loop(
        &app, &executor, &conversation_id,
        &agent_config, None, silent, false, cancellation,
    ).await?;

    if !silent {
        use tauri::Emitter;
        app.emit("ai-status", serde_json::json!({ "status": null, "conversationId": conversation_id })).ok();
    }

    Ok(())
}

/// Process a conversation until no more pending user messages remain.
/// Must be called only after `queue_state.try_acquire(conv_id)` returned true.
pub async fn process_conversation_queue(
    app: tauri::AppHandle,
    executor: DbExecutor,
    conversation_id: String,
) {
    let queue_state = match app.try_state::<ConversationProcessingState>() {
        Some(s) => s,
        None => {
            crate::commands::log_backend("warn", "[conversation_queue] ConversationProcessingState not registered");
            return;
        }
    };
    let cancellation = match app.try_state::<AiCancellationState>() {
        Some(s) => s,
        None => {
            crate::commands::log_backend("warn", "[conversation_queue] AiCancellationState not registered");
            return;
        }
    };

    loop {
        if let Err(e) = run_conversation_from_current_state(
            app.clone(),
            executor.clone(),
            conversation_id.clone(),
            &cancellation,
            false,
        ).await {
            crate::commands::log_backend("warn", &format!(
                "[conversation_queue] Failed for {}: {}", conversation_id, e
            ));
        }

        queue_state.release(&conversation_id);

        let has_more = executor
            .read({
                let conv_id = conversation_id.clone();
                move |conn| Ok(has_pending_user_message(conn, &conv_id))
            })
            .await
            .unwrap_or(false);

        if !has_more || !queue_state.try_acquire(&conversation_id) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    fn insert_msg(conn: &mut rusqlite::Connection, iri: &str, conv: &str, role: &str, ts: i64) {
        let rfc3339 = chrono::DateTime::from_timestamp_millis(ts)
            .unwrap_or_default()
            .to_rfc3339();
        store::assert_triples(conn, &[
            Triple::new(iri, "rdf:type", Object::Iri("foundation:AIConversationMessage".to_string())),
            Triple::new(iri, "foundation:partOfConversation", Object::Iri(conv.to_string())),
            Triple::new(iri, "foundation:role", Object::Literal {
                value: role.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new(iri, "foundation:sentAt", Object::DateTime(rfc3339)),
        ], "test").unwrap();
    }

    #[test]
    fn has_pending_user_message_retorna_true_quando_ultimo_e_user() {
        let mut conn = setup_test_db();
        insert_msg(&mut conn, "foundation:Msg1", "foundation:ConvA", "assistant", 1_000);
        insert_msg(&mut conn, "foundation:Msg2", "foundation:ConvA", "user", 2_000);
        assert!(has_pending_user_message(&conn, "foundation:ConvA"));
    }

    #[test]
    fn has_pending_user_message_retorna_false_quando_ultimo_e_assistant() {
        let mut conn = setup_test_db();
        insert_msg(&mut conn, "foundation:Msg1", "foundation:ConvA", "user", 1_000);
        insert_msg(&mut conn, "foundation:Msg2", "foundation:ConvA", "assistant", 2_000);
        assert!(!has_pending_user_message(&conn, "foundation:ConvA"));
    }

    #[test]
    fn has_pending_user_message_retorna_false_em_conversa_vazia() {
        let conn = setup_test_db();
        assert!(!has_pending_user_message(&conn, "foundation:ConvEmpty"));
    }
}
