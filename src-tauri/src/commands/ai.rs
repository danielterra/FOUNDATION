use crate::owl::DbExecutor;
use crate::owl::{self, Individual, Object};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_api_key(
    _app: AppHandle,
    api_key: String,
    service_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {

    executor.write(move |conn| {
        let service_iri = if let Some(iri) = service_iri {
            iri
        } else {
            owl::get_iri_property(conn, "foundation:LocalAIAssistant", "foundation:usesService")
                .map_err(|e| format!("Failed to get service: {}", e))?
                .ok_or_else(|| "LocalAIAssistant has no usesService".to_string())?
        };

        // Retract previous API key if one is linked to the service
        if let Ok(Some(old_key_iri)) = owl::get_iri_property(conn, &service_iri, "foundation:apiKey") {
            Individual::retract(conn, &old_key_iri, "ai")
                .map_err(|e| format!("Failed to retract old credential: {}", e))?;
        }

        let timestamp = chrono::Utc::now().timestamp_millis();
        let api_key_iri = format!("foundation:ClaudeAPIKey_{}", timestamp);
        let credential = Individual::new(&api_key_iri);

        credential.assert(
            conn,
            "foundation:APIKey",
            "Claude AI API Key",
            "vpn_key",
            "ai"
        ).map_err(|e| format!("Failed to create APIKey: {}", e))?;

        credential.add_property(
            conn,
            "foundation:credentialValue",
            vec![Object::Literal {
                value: api_key,
                datatype: Some("xsd:string".to_string()),
                language: None,
            }],
            "ai"
        ).map_err(|e| format!("Failed to set credential value: {}", e))?;

        credential.add_property(
            conn,
            "foundation:credentialCreatedAt",
            vec![Object::DateTime(chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339())],
            "ai"
        ).map_err(|e| format!("Failed to set created timestamp: {}", e))?;

        // Link the new key to the service
        let service = Individual::new(&service_iri);
        service.add_property(
            conn,
            "foundation:apiKey",
            vec![Object::Iri(api_key_iri)],
            "ai"
        ).map_err(|e| format!("Failed to link key to service: {}", e))?;

        Ok("saved".to_string())
    }).await?;

    super::log_backend("info", "API key saved successfully");

    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__delete_api_key(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        if let Ok(Some(old_key_iri)) = owl::get_iri_property(conn, &service_iri, "foundation:apiKey") {
            Individual::retract(conn, &old_key_iri, "ai")
                .map_err(|e| format!("Failed to retract credential: {}", e))?;
        }
        Individual::clear_property(conn, &service_iri, "foundation:apiKey", "ai")
            .map_err(|e| format!("Failed to clear apiKey link: {}", e))?;
        Ok(String::new())
    }).await?;

    super::log_backend("info", "API key deleted");
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__get_api_key(
    service_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<Option<String>, String> {
    executor.read(move |conn| {
        let resolved = service_iri.or_else(|| {
            owl::get_iri_property(conn, "foundation:LocalAIAssistant", "foundation:usesService")
                .ok().flatten()
        });

        if let Some(svc_iri) = resolved {
            if let Ok(Some(api_key_iri)) = owl::get_iri_property(conn, &svc_iri, "foundation:apiKey") {
                if let Ok(Some(value)) = owl::get_literal_property(conn, &api_key_iri, "foundation:credentialValue") {
                    return Ok(Some(value));
                }
            }
        }

        Ok(None)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__initialize(
    app: AppHandle,
    _api_key: String,
    _executor: State<'_, DbExecutor>,
) -> Result<(), String> {

    let app_for_recovery = app.clone();
    tauri::async_runtime::spawn(async move {
        let executor_state = app_for_recovery.state::<DbExecutor>();
        let cancellation_state = app_for_recovery.state::<super::chat::AiCancellationState>();
        match super::chat::chat__recover_pending_tools(
            app_for_recovery.clone(), executor_state, cancellation_state,
        ).await {
            Ok(count) if count > 0 => {
                super::log_backend(
                    "info",
                    &format!("[RECOVERY] Recovered {} pending tool execution(s)", count),
                );
            }
            Ok(_) => {}
            Err(e) => {
                super::log_backend("warn", &format!("[RECOVERY] Failed to check pending tools: {}", e));
                app_for_recovery.emit("ai-error", serde_json::json!({ "message": e })).ok();
            }
        }
    });

    Ok(())
}
