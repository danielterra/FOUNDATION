use crate::ai;
use crate::ai::{ChatMessage, GenerateRequest};
use crate::ai::functions::{self, ToolCall, ToolResult};
use crate::owl::DbExecutor;
use crate::owl::{self, Individual, Object};
use tauri::{AppHandle, Emitter, Manager, State};
use serde_json::Value;

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_api_key(
    _app: AppHandle,
    api_key: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {

    executor.write(move |conn| {
        let owned = owl::find_entities_with_property(
            conn, "foundation:ownedBy", "foundation:ThisUser",
        ).map_err(|e| format!("Failed to query credentials: {}", e))?;

        for credential_iri in &owned {
            if owl::has_property_iri(
                conn, credential_iri,
                "foundation:credentialFor", "foundation:ClaudeAIService",
            ) && owl::has_property_iri(conn, credential_iri, "rdf:type", "foundation:APIKey")
            {
                Individual::retract(conn, credential_iri, "ai")
                    .map_err(|e| format!("Failed to retract old credential: {}", e))?;
            }
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
            "foundation:credentialFor",
            vec![Object::Iri("foundation:ClaudeAIService".to_string())],
            "ai"
        ).map_err(|e| format!("Failed to link to service: {}", e))?;

        credential.add_property(
            conn,
            "foundation:ownedBy",
            vec![Object::Iri("foundation:ThisUser".to_string())],
            "ai"
        ).map_err(|e| format!("Failed to set owner: {}", e))?;

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
            vec![Object::DateTime(timestamp)],
            "ai"
        ).map_err(|e| format!("Failed to set created timestamp: {}", e))?;

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
    executor.read(|conn| {
        let owned = owl::find_entities_with_property(
            conn, "foundation:ownedBy", "foundation:ThisUser",
        ).map_err(|e| format!("Failed to query credentials: {}", e))?;

        for credential_iri in &owned {
            if owl::has_property_iri(
                conn, credential_iri,
                "foundation:credentialFor", "foundation:ClaudeAIService",
            ) && owl::has_property_iri(conn, credential_iri, "rdf:type", "foundation:APIKey")
            {
                if let Ok(Some(value)) = owl::get_literal_property(
                    conn, credential_iri, "foundation:credentialValue",
                ) {
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

    let (model_identifier, timeout_secs) = executor.read(|conn| {
        let models = owl::find_entities_with_property(
            conn, "foundation:offeredBy", "foundation:ClaudeAIService",
        ).map_err(|e| format!("Failed to query models: {}", e))?;

        let mut model = None;
        for model_iri in &models {
            if owl::has_property_literal(conn, model_iri, "foundation:isDefaultModel", "true") {
                if let Ok(Some(identifier)) = owl::get_literal_property(
                    conn, model_iri, "foundation:modelIdentifier",
                ) {
                    model = Some(identifier);
                    break;
                }
            }
        }

        let timeout = if let Ok(Some(setting)) =
            Individual::get(conn, "foundation:DefaultAPIRequestTimeoutSetting")
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

    super::log_backend("info", &format!("[AI] Initializing with timeout={}s", timeout_secs));
    ai::initialize_ai_with_model(api_key, model_identifier, timeout_secs).await?;

    let executor_state = app.state::<DbExecutor>();
    match super::chat::chat__recover_pending_tools(app.clone(), executor_state).await {
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

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__generate(
    _app: AppHandle,
    executor: State<'_, DbExecutor>,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    system: Option<String>,
) -> Result<String, String> {

    let request = GenerateRequest {
        messages,
        max_tokens,
        temperature,
        system,
        tools: None,
    };

    let response = ai::generate_response(request).await?;

    if let Some(usage) = &response.usage {
        let model = ai::get_current_model().unwrap_or_else(|_| "unknown".to_string());
        super::chat_storage::log_api_call(
            &executor,
            &model,
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_creation_input_tokens,
            usage.cache_read_input_tokens,
        ).await.unwrap_or_else(|e| super::log_backend("warn", &format!("[AI] Failed to log API call: {}", e)));
    }

    Ok(response.content)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__list_available_models(
    _app: AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Value, String> {

    let api_info = executor.read(|conn| {
        let models_endpoint = owl::get_literal_property(
            conn, "foundation:ClaudeAIService", "foundation:apiModelsEndpoint",
        ).ok().flatten();

        let api_version = owl::get_literal_property(
            conn, "foundation:ClaudeAIService", "foundation:apiVersion",
        ).ok().flatten();

        Ok((models_endpoint, api_version))
    }).await?;

    let (endpoint, version) = api_info;
    let endpoint = endpoint.ok_or("API models endpoint not found in ontology")?;
    let version = version.ok_or("API version not found in ontology")?;

    let api_key_result = ai__get_api_key(executor).await?;
    let api_key = api_key_result
        .ok_or("API key not found. Please configure your Claude API key first.")?;

    let client = reqwest::Client::new();
    let response = client
        .get(&endpoint)
        .header("x-api-key", api_key)
        .header("anthropic-version", version)
        .send()
        .await
        .map_err(|e| format!("Failed to call models API: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("API request failed with status {}: {}", status, error_text));
    }

    let models_json: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;


    Ok(models_json)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__get_available_tools() -> Result<Value, String> {
    let tools = functions::get_available_tools();
    serde_json::to_value(tools)
        .map_err(|e| format!("Failed to serialize tools: {}", e))
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__execute_tool(
    app: AppHandle,
    name: String,
    arguments: Value,
    executor: State<'_, DbExecutor>,
) -> Result<ToolResult, String> {

    let call = ToolCall { name, arguments };
    let app_clone = app.clone();

    let result_json = executor.write(move |conn| {
        let result = functions::execute_tool(conn, &call, Some(&app_clone));
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await.map_err(|e| format!("Failed to execute tool: {}", e))?;

    let result: ToolResult = serde_json::from_str(&result_json)
        .map_err(|e| format!("Failed to parse result: {}", e))?;


    Ok(result)
}
