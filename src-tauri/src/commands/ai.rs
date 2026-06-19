use crate::owl::DbExecutor;
use crate::owl::{self, Individual, Object, Triple};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__list_openrouter_models(
    api_key: String,
    base_url: String,
) -> Result<Vec<serde_json::Value>, String> {
    let url = format!("{}/models?supported_parameters=tools", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models from {}: {}", url, e))?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(format!("OpenRouter models endpoint returned HTTP {}", status));
    }

    let body: serde_json::Value = response.json()
        .await
        .map_err(|e| format!("Failed to parse models response: {}", e))?;

    let models = body["data"].as_array()
        .ok_or_else(|| "Unexpected response format from /models endpoint".to_string())?;

    let result = models.iter().map(|m| {
        let input_cost = m["pricing"]["prompt"].as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let output_cost = m["pricing"]["completion"].as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        serde_json::json!({
            "id": m["id"].as_str().unwrap_or(""),
            "name": m["name"].as_str().unwrap_or(""),
            "contextLength": m["context_length"].as_u64().unwrap_or(0),
            "inputCostPerToken": input_cost,
            "outputCostPerToken": output_cost,
        })
    }).collect();

    Ok(result)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__ensure_openrouter_model(
    service_iri: String,
    model_id: String,
    model_name: String,
    context_length: u64,
    input_cost_per_token: f64,
    output_cost_per_token: f64,
    supports_tools: bool,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.write(move |conn| {
        // Return existing IRI if this model was already registered for this service.
        // Query by offeredBy (IRI property) then filter by modelIdentifier (literal).
        let existing = owl::find_entities_with_property(conn, "foundation:offeredBy", &service_iri)
            .unwrap_or_default()
            .into_iter()
            .find(|iri| {
                owl::get_literal_property(conn, iri, "foundation:modelIdentifier")
                    .ok()
                    .flatten()
                    .as_deref() == Some(model_id.as_str())
            });

        if let Some(iri) = existing {
            return Ok(iri);
        }

        let timestamp = chrono::Utc::now().timestamp_millis();
        let model_iri = format!("foundation:AIModel_{}", timestamp);
        let ind = Individual::new(&model_iri);

        ind.assert(conn, "foundation:AIModel", &model_name, "smart_toy", "ai")
            .map_err(|e| format!("Failed to create AIModel: {}", e))?;

        let mut props: Vec<Triple> = vec![
            Triple::new(&model_iri, "foundation:modelIdentifier",
                Object::Literal { value: model_id, datatype: Some("xsd:string".to_string()), language: None }),
            Triple::new(&model_iri, "foundation:offeredBy", Object::Iri(service_iri)),
            Triple::new(&model_iri, "foundation:maxInputTokens",
                Object::Integer(context_length as i64)),
        ];

        // estimate_call_cost reads price per million tokens, so convert from OR's per-token values
        if input_cost_per_token > 0.0 {
            props.push(Triple::new(&model_iri, "foundation:inputPricePerMTok",
                Object::Number(input_cost_per_token * 1_000_000.0)));
        }
        if output_cost_per_token > 0.0 {
            props.push(Triple::new(&model_iri, "foundation:outputPricePerMTok",
                Object::Number(output_cost_per_token * 1_000_000.0)));
        }
        if supports_tools {
            props.push(Triple::new(&model_iri, "foundation:modelCapability",
                Object::Literal { value: "tool_calling".to_string(), datatype: Some("xsd:string".to_string()), language: None }));
        }

        crate::owl::assert_raw_triples(conn, &props, "ai")
            .map_err(|e| format!("Failed to persist model properties: {}", e))?;

        Ok(model_iri)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__list_api_calls(
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    after_tx: Option<i64>,
    limit: Option<i64>,
    snapshot_tx: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    executor.read(move |conn| {
        let page_size = limit.unwrap_or(100).max(1).min(500);

        let pinned_tx: i64 = match snapshot_tx {
            Some(tx) => tx,
            None => conn.query_row(
                "SELECT COALESCE(MAX(tx), 0) FROM triples",
                [],
                |row| row.get(0),
            ).map_err(|e| e.to_string())?,
        };

        let rows = owl::find_entities_with_property_keyset(
            conn, "rdf:type", "foundation:AIAPICall", after_tx, page_size + 1,
        ).unwrap_or_default();

        let has_more = rows.len() as i64 > page_size;
        let rows: Vec<(String, i64)> = rows.into_iter().take(page_size as usize).collect();
        let next_cursor: Option<i64> = if has_more { rows.last().map(|(_, tx)| *tx) } else { None };

        let items: Vec<serde_json::Value> = rows.into_iter()
            .filter_map(|(iri, _tx)| {
                let ind = Individual::get(conn, &iri).ok().flatten()?;

                let called_at = ind.properties.iter()
                    .find(|(k, _)| k == "foundation:calledAt")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                if let (Some(from), Ok(ts)) = (from_ms, chrono::DateTime::parse_from_rfc3339(&called_at)) {
                    if ts.timestamp_millis() < from { return None; }
                }
                if let (Some(to), Ok(ts)) = (to_ms, chrono::DateTime::parse_from_rfc3339(&called_at)) {
                    if ts.timestamp_millis() > to { return None; }
                }

                let get_int = |prop: &str| ind.properties.iter()
                    .find(|(k, _)| k == prop)
                    .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as u64) } else { None })
                    .unwrap_or(0);

                let get_num = |prop: &str| ind.properties.iter()
                    .find(|(k, _)| k == prop)
                    .and_then(|(_, v)| match v {
                        Object::Number(n) => Some(*n),
                        Object::Literal { value, .. } => value.parse::<f64>().ok(),
                        _ => None,
                    });

                let model = ind.properties.iter()
                    .find(|(k, _)| k == "foundation:model")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                let conv_iri = ind.properties.iter()
                    .find(|(k, _)| k == "foundation:generatedByConversation")
                    .and_then(|(_, v)| if let Object::Iri(s) = v { Some(s.clone()) } else { None });

                Some(serde_json::json!({
                    "iri": iri,
                    "model": model,
                    "calledAt": called_at,
                    "inputTokens": get_int("foundation:inputTokens"),
                    "outputTokens": get_int("foundation:outputTokens"),
                    "estimatedCost": get_num("foundation:estimatedCost"),
                    "conversationIri": conv_iri,
                }))
            })
            .collect();

        Ok(serde_json::json!({
            "items": items,
            "next_cursor": next_cursor,
            "has_more": has_more,
            "snapshot_tx": pinned_tx,
        }))
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__get_fallback_models(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<String>, String> {
    executor.read(move |conn| {
        let json = owl::get_literal_property(conn, &service_iri, "foundation:fallbackModelIdentifiers")
            .unwrap_or_default()
            .unwrap_or_default();
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_fallback_models(
    service_iri: String,
    model_ids: Vec<String>,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let json = serde_json::to_string(&model_ids)
        .map_err(|e| format!("Failed to serialize fallback models: {}", e))?;
    executor.write(move |conn| {
        Individual::new(&service_iri).add_property(
            conn,
            "foundation:fallbackModelIdentifiers",
            vec![Object::Literal {
                value: json,
                datatype: Some("xsd:string".to_string()),
                language: None,
            }],
            "ai",
        ).map_err(|e| format!("Failed to save fallback models: {}", e))?;
        Ok(String::new())
    }).await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__validate_provider_key(
    api_key: String,
    base_url: String,
) -> Result<(), String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("URL unreachable ({}): {}", url, e))?;

    match response.status().as_u16() {
        200..=299 => Ok(()),
        401 => Err("Invalid API key (401 Unauthorized).".to_string()),
        403 => Err("Access denied (403 Forbidden). Check the API key.".to_string()),
        code => Err(format!("Validation failed: HTTP {}.", code)),
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__get_service_base_url(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        Ok(owl::get_literal_property(conn, &service_iri, "foundation:apiBaseUrl")
            .unwrap_or_default()
            .unwrap_or_default())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_service_base_url(
    service_iri: String,
    base_url: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        Individual::new(&service_iri).add_property(
            conn,
            "foundation:apiBaseUrl",
            vec![Object::Literal {
                value: base_url,
                datatype: Some("xsd:anyURI".to_string()),
                language: None,
            }],
            "ai",
        ).map_err(|e| format!("Failed to save base URL: {}", e))?;
        Ok(String::new())
    }).await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_api_key(
    _app: AppHandle,
    api_key: String,
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {

    executor.write(move |conn| {

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
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<Option<String>, String> {
    executor.read(move |conn| {
        if let Ok(Some(api_key_iri)) = owl::get_iri_property(conn, &service_iri, "foundation:apiKey") {
            if let Ok(Some(value)) = owl::get_literal_property(conn, &api_key_iri, "foundation:credentialValue") {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__initialize(
    app: AppHandle,
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
