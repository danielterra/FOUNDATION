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

/// Returns true if the newest message in the conversation is a real user-typed message
/// that the engine has not yet responded to.
///
/// Tool result messages also carry role="user" in the protocol, but they consist
/// exclusively of ToolResultBlock content. Those must NOT be treated as pending user
/// messages — doing so causes an infinite re-run loop when the model returns an empty
/// stop after executing tools (engine terminates → last message is tool result →
/// has_pending_user_message=true → engine starts again → repeat forever).
pub fn has_pending_user_message(conn: &Connection, conversation_iri: &str) -> bool {
    let iris = match crate::core_ontology::chat::find_messages_by_conversation(conn, conversation_iri, 1, 0) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let newest_iri = match iris.first() {
        Some(iri) => iri,
        None => return false,
    };

    let role_ok = matches!(
        get_literal_property(conn, newest_iri, "foundation:role"),
        Ok(Some(role)) if role == "user"
    );
    if !role_ok {
        return false;
    }

    // Exclude pure tool-result messages: they have only ToolResultBlock content blocks.
    // A real user message has at least one non-ToolResult block (Text, Image, Document, etc.).
    crate::core_ontology::chat::message_has_non_tool_result_block(conn, newest_iri.as_str())
}

fn build_recovery_first_turn_ctx(
    conn: &Connection,
    conversation_id: &str,
) -> Option<super::engine::FirstTurnContext> {
    let last_user_msg: String = conn.query_row(
        "SELECT t_msg.subject FROM triples t_msg
         JOIN triples t_role ON t_role.subject = t_msg.subject
             AND t_role.predicate = 'foundation:role'
             AND t_role.object_value = 'user' AND t_role.retracted = 0
         JOIN triples t_ts ON t_ts.subject = t_msg.subject
             AND t_ts.predicate = 'foundation:sentAt' AND t_ts.retracted = 0
         WHERE t_msg.predicate = 'foundation:partOfConversation'
             AND t_msg.object = ?1 AND t_msg.retracted = 0
         ORDER BY t_ts.object_datetime DESC LIMIT 1",
        [conversation_id],
        |row| row.get(0),
    ).ok()?;

    let json: String = conn.query_row(
        "SELECT object_value FROM triples
         WHERE subject = ?1 AND predicate = 'foundation:subconsciousContext'
           AND retracted = 0
         ORDER BY tx DESC LIMIT 1",
        [last_user_msg.as_str()],
        |row| row.get(0),
    ).ok()?;

    let entities: Vec<super::subconscious::SubconsciousEntity> =
        serde_json::from_str(&json).ok()?;
    let formatted = super::subconscious::format_context(&entities)?;

    Some(super::engine::FirstTurnContext {
        camera_images: None,
        attachment_binaries: vec![],
        files_needing_summary: vec![],
        subconscious_context: Some(formatted),
        blackboard_context: None,
    })
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

    // Re-inject stored subconscious context so retry/recovery behaves like a fresh send.
    let conv_id_for_sc = conversation_id.clone();
    let first_turn_ctx = executor.read(move |conn| {
        Ok(build_recovery_first_turn_ctx(conn, &conv_id_for_sc))
    }).await.ok().flatten();

    super::engine::run_conversation_loop(
        &app, &executor, &conversation_id,
        &agent_config, first_turn_ctx, silent, false, cancellation,
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

    fn attach_text_block(conn: &mut rusqlite::Connection, msg_iri: &str, block_iri: &str) {
        store::assert_triples(conn, &[
            Triple::new(block_iri, "rdf:type", Object::Iri("anthropic:TextBlock".to_string())),
            Triple::new(msg_iri, "foundation:hasContentBlock", Object::Iri(block_iri.to_string())),
        ], "test").unwrap();
    }

    fn attach_tool_result_block(conn: &mut rusqlite::Connection, msg_iri: &str, block_iri: &str) {
        store::assert_triples(conn, &[
            Triple::new(block_iri, "rdf:type", Object::Iri("anthropic:ToolResultBlock".to_string())),
            Triple::new(msg_iri, "foundation:hasContentBlock", Object::Iri(block_iri.to_string())),
        ], "test").unwrap();
    }

    #[test]
    fn has_pending_user_message_retorna_true_quando_ultimo_e_user_com_texto() {
        let mut conn = setup_test_db();
        insert_msg(&mut conn, "foundation:Msg1", "foundation:ConvA", "assistant", 1_000);
        insert_msg(&mut conn, "foundation:Msg2", "foundation:ConvA", "user", 2_000);
        attach_text_block(&mut conn, "foundation:Msg2", "anthropic:Block1");
        assert!(has_pending_user_message(&conn, "foundation:ConvA"));
    }

    #[test]
    fn has_pending_user_message_retorna_false_quando_ultimo_e_tool_result() {
        let mut conn = setup_test_db();
        insert_msg(&mut conn, "foundation:Msg1", "foundation:ConvA", "assistant", 1_000);
        insert_msg(&mut conn, "foundation:Msg2", "foundation:ConvA", "user", 2_000);
        attach_tool_result_block(&mut conn, "foundation:Msg2", "anthropic:Block1");
        assert!(!has_pending_user_message(&conn, "foundation:ConvA"),
            "mensagem de tool result não deve ser tratada como mensagem pendente do usuário");
    }

    #[test]
    fn has_pending_user_message_retorna_false_quando_ultimo_e_assistant() {
        let mut conn = setup_test_db();
        insert_msg(&mut conn, "foundation:Msg1", "foundation:ConvA", "user", 1_000);
        attach_text_block(&mut conn, "foundation:Msg1", "anthropic:Block1");
        insert_msg(&mut conn, "foundation:Msg2", "foundation:ConvA", "assistant", 2_000);
        assert!(!has_pending_user_message(&conn, "foundation:ConvA"));
    }

    #[test]
    fn has_pending_user_message_retorna_false_em_conversa_vazia() {
        let conn = setup_test_db();
        assert!(!has_pending_user_message(&conn, "foundation:ConvEmpty"));
    }
}
