use crate::ai;
use crate::owl::DbExecutor;
use crate::owl::{self, Individual, Object};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_api_key(
    _app: AppHandle,
    api_key: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {

    executor.write(move |conn| async move {
        let service_iri = owl::get_iri_property(&conn, "foundation:LocalAIAssistant", "foundation:usesService").await
            .map_err(|e| format!("Failed to get service: {}", e))?
            .ok_or_else(|| "LocalAIAssistant has no usesService".to_string())?;

        // Retract previous API key if one is linked to the service
        if let Ok(Some(old_key_iri)) = owl::get_iri_property(&conn, &service_iri, "foundation:apiKey").await {
            Individual::retract(&conn, &old_key_iri, "ai").await
                .map_err(|e| format!("Failed to retract old credential: {}", e))?;
        }

        let timestamp = chrono::Utc::now().timestamp_millis();
        let api_key_iri = format!("foundation:ClaudeAPIKey_{}", timestamp);
        let credential = Individual::new(&api_key_iri);

        credential.assert(
            &conn,
            "foundation:APIKey",
            "Claude AI API Key",
            "vpn_key",
            "ai"
        ).await.map_err(|e| format!("Failed to create APIKey: {}", e))?;

        credential.add_property(
            &conn,
            "foundation:credentialValue",
            vec![Object::Literal {
                value: api_key,
                datatype: Some("xsd:string".to_string()),
                language: None,
            }],
            "ai"
        ).await.map_err(|e| format!("Failed to set credential value: {}", e))?;

        credential.add_property(
            &conn,
            "foundation:credentialCreatedAt",
            vec![Object::DateTime(chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339())],
            "ai"
        ).await.map_err(|e| format!("Failed to set created timestamp: {}", e))?;

        // Link the new key to the service
        let service = Individual::new(&service_iri);
        service.add_property(
            &conn,
            "foundation:apiKey",
            vec![Object::Iri(api_key_iri)],
            "ai"
        ).await.map_err(|e| format!("Failed to link key to service: {}", e))?;

        Ok("saved".to_string())
    }).await?;

    super::log_backend("info", "API key saved successfully");

    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__get_api_key(
    executor: State<'_, DbExecutor>,
) -> Result<Option<String>, String> {
    executor.read(|conn| async move {
        let service_iri = owl::get_iri_property(&conn, "foundation:LocalAIAssistant", "foundation:usesService").await
            .ok().flatten();

        if let Some(service_iri) = service_iri {
            if let Ok(Some(api_key_iri)) = owl::get_iri_property(&conn, &service_iri, "foundation:apiKey").await {
                if let Ok(Some(value)) = owl::get_literal_property(&conn, &api_key_iri, "foundation:credentialValue").await {
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
    api_key: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {

    let (model_identifier, timeout_secs) = executor.read(|conn| async move {
        // Check user's preferred model from DefaultAIModelSetting first
        let preferred_model_iri = if let Ok(Some(setting)) =
            Individual::get(&conn, "foundation:DefaultAIModelSetting").await
        {
            setting.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| v.as_literal())
        } else {
            None
        };

        let model = if let Some(ref iri) = preferred_model_iri {
            owl::get_literal_property(&conn, iri, "foundation:modelIdentifier").await
                .ok()
                .flatten()
        } else {
            // Fall back to the model on LocalAIAssistant
            let model_iri = owl::get_iri_property(&conn, "foundation:LocalAIAssistant", "foundation:usesModel").await
                .ok()
                .flatten();
            if let Some(iri) = model_iri {
                owl::get_literal_property(&conn, &iri, "foundation:modelIdentifier").await
                    .ok()
                    .flatten()
            } else {
                None
            }
        };

        let timeout = if let Ok(Some(setting)) =
            Individual::get(&conn, "foundation:DefaultAPIRequestTimeoutSetting").await
        {
            setting.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| v.as_literal())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(180)
        } else {
            180
        };

        Ok((model, timeout))
    }).await.map_err(|e: String| format!("Failed to query AI settings: {}", e))?;

    if model_identifier.is_none() {
        super::log_backend(
            "warn",
            "No default model found in ontology. AI generation will fail \
            until a model is configured in Settings.",
        );
    }

    super::log_backend("info", &format!(
        "[AI] Initializing with model={:?}, timeout={}s",
        model_identifier, timeout_secs
    ));
    ai::initialize_ai_with_model(api_key, model_identifier, timeout_secs).await?;

    let executor_state = app.state::<DbExecutor>();
    let cancellation_state = app.state::<super::chat::AiCancellationState>();
    match super::chat::chat__recover_pending_tools(app.clone(), executor_state, cancellation_state).await {
        Ok(count) if count > 0 => {
            super::log_backend(
                "info",
                &format!("[RECOVERY] Recovered {} pending tool execution(s)", count),
            );
        }
        Ok(_) => {}
        Err(e) => {
            super::log_backend("warn", &format!("[RECOVERY] Failed to check pending tools: {}", e));
            app.emit("ai-error", serde_json::json!({ "message": e })).ok();
        }
    }

    Ok(())
}
