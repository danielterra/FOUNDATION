use tauri::{AppHandle, Emitter, Manager};

use crate::owl::{DbExecutor, get_literal_property, get_iri_property};

use super::executor::ExecutionContext;

type Result<T> = std::result::Result<T, String>;

pub async fn execute_nova_message_task(
    app: &AppHandle,
    node_iri: &str,
    ctx: &ExecutionContext,
) -> Result<()> {
    let executor = app.state::<DbExecutor>();

    let (payload_template, target_conversation_iri) = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                let payload = get_literal_property(conn, &node_iri, "foundation:messagePayload")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();

                let target_conv = get_iri_property(conn, &node_iri, "foundation:targetConversation")
                    .map_err(|e| e.to_string())?;

                Ok((payload, target_conv))
            }
        })
        .await?;

    let message_text = super::executor::interpolate_with_db(&payload_template, ctx, &executor).await;

    let conv_id = match target_conversation_iri {
        Some(iri) => iri,
        None => executor
            .read(|conn| {
                crate::core_ontology::conversation::find_conversation_by_last_user_message(conn)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "No active conversation found".to_string())
            })
            .await?,
    };

    crate::commands::create_user_message(&executor, &conv_id, &message_text).await?;
    app.emit("chat-message-added", serde_json::json!({"conversationId": conv_id})).ok();

    let app_spawn = app.clone();
    tokio::spawn(async move {
        let executor = app_spawn.state::<DbExecutor>().inner().clone();
        let cancellation = app_spawn.state::<crate::commands::AiCancellationState>();
        if let Err(e) = crate::commands::run_conversation_from_current_state(
            app_spawn.clone(), executor, conv_id, &cancellation, false,
        ).await {
            crate::commands::log_backend("warn", &format!("[nova_message_task] Reply failed: {}", e));
        }
    });

    Ok(())
}
