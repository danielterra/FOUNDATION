use tauri::{AppHandle, Emitter, Manager};

use crate::owl::{DbExecutor, get_literal_property, get_iri_property, Individual};

use super::executor::ExecutionContext;

type Result<T> = std::result::Result<T, String>;

fn find_most_recent_conversation(conn: &rusqlite::Connection) -> Result<String> {
    conn.query_row(
        r#"
        SELECT t.subject
        FROM triples t
        WHERE t.predicate = 'rdf:type'
          AND t.object = 'foundation:AIConversation'
          AND t.retracted = 0
          AND EXISTS (
              SELECT 1 FROM triples h
              WHERE h.subject = t.subject
                AND h.predicate = 'foundation:handledBy'
                AND h.retracted = 0
          )
        ORDER BY t.rowid DESC
        LIMIT 1
        "#,
        [],
        |row| row.get::<_, String>(0),
    )
    .map_err(|e| format!("No active conversation found: {}", e))
}

pub async fn execute_nova_message_task(
    app: &AppHandle,
    node_iri: &str,
    ctx: &ExecutionContext,
) -> Result<()> {
    let executor = app.state::<DbExecutor>();

    let (payload_template, context_entity_iri) = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                let payload = get_literal_property(conn, &node_iri, "foundation:messagePayload")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();

                let context_entity = get_iri_property(conn, &node_iri, "foundation:contextEntity")
                    .map_err(|e| e.to_string())?;

                Ok((payload, context_entity))
            }
        })
        .await?;

    let message_text = super::executor::interpolate_with_db(&payload_template, ctx, &executor).await;

    let final_message = if let Some(ref entity_iri) = context_entity_iri {
        let entity_summary = executor
            .read({
                let entity_iri = entity_iri.clone();
                move |conn| {
                    let label = get_literal_property(conn, &entity_iri, "rdfs:label")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| entity_iri.clone());

                    let comment = get_literal_property(conn, &entity_iri, "rdfs:comment")
                        .ok()
                        .flatten();

                    let type_label = Individual::get(conn, &entity_iri)
                        .ok()
                        .flatten()
                        .and_then(|ind| ind.types.first().map(|t| t.label.clone()))
                        .unwrap_or_default();

                    Ok::<_, String>((label, comment, type_label))
                }
            })
            .await
            .unwrap_or_else(|_| (entity_iri.clone(), None, String::new()));

        let (label, comment, type_label) = entity_summary;
        let context_block = if let Some(desc) = comment {
            format!("\n\n**Context — {} ({})**\n{}", label, type_label, desc)
        } else {
            format!("\n\n**Context — {} ({})**", label, type_label)
        };
        format!("{}{}", message_text, context_block)
    } else {
        message_text
    };

    let conv_id = executor
        .read(|conn| find_most_recent_conversation(conn))
        .await?;

    crate::commands::create_user_message(&executor, &conv_id, &final_message).await?;
    app.emit("chat-message-added", ()).ok();

    let app_spawn = app.clone();
    tokio::spawn(async move {
        let executor = app_spawn.state::<DbExecutor>().inner().clone();
        let cancellation = crate::commands::AiCancellationState::new();
        if let Err(e) = crate::commands::continue_conversation_after_recovery(
            app_spawn, executor, conv_id, &cancellation,
        ).await {
            crate::commands::log_backend("warn", &format!("[nova_message_task] Reply failed: {}", e));
        }
    });

    Ok(())
}
