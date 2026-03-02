use crate::ai;
use crate::ai::{ChatMessage, GenerateRequest};
use crate::ai::functions::{self, FunctionCall, FunctionResult};
use crate::owl::DbExecutor;
use crate::owl::{self, Individual, Object};
use tauri::{AppHandle, State};
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

        let now = chrono::Utc::now().to_rfc3339();

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
            Object::Iri("foundation:ClaudeAIService".to_string()),
            "ai"
        ).map_err(|e| format!("Failed to link to service: {}", e))?;

        credential.add_property(
            conn,
            "foundation:ownedBy",
            Object::Iri("foundation:ThisUser".to_string()),
            "ai"
        ).map_err(|e| format!("Failed to set owner: {}", e))?;

        credential.add_property(
            conn,
            "foundation:credentialValue",
            Object::Literal {
                value: api_key,
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            "ai"
        ).map_err(|e| format!("Failed to set credential value: {}", e))?;

        credential.add_property(
            conn,
            "foundation:credentialCreatedAt",
            Object::Literal {
                value: now,
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            },
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
    _app: AppHandle,
    api_key: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {

    let model_identifier = executor.read(|conn| {
        let models = owl::find_entities_with_property(
            conn, "foundation:offeredBy", "foundation:ClaudeAIService",
        ).map_err(|e| format!("Failed to query models: {}", e))?;

        for model_iri in &models {
            if owl::has_property_literal(conn, model_iri, "foundation:isDefaultModel", "true") {
                if let Ok(Some(identifier)) = owl::get_literal_property(
                    conn, model_iri, "foundation:modelIdentifier",
                ) {
                    return Ok(Some(identifier));
                }
            }
        }

        Ok(None)
    }).await.map_err(|e: String| format!("Failed to query default model: {}", e))?;

    if model_identifier.is_none() {
        super::log_backend("warn", "No default model found in ontology, using hardcoded fallback");
    }

    ai::initialize_ai_with_model(api_key, model_identifier).await?;


    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__generate(
    _app: AppHandle,
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
pub async fn ai__get_available_functions() -> Result<Value, String> {
    let functions = functions::get_available_functions();
    serde_json::to_value(functions)
        .map_err(|e| format!("Failed to serialize functions: {}", e))
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__execute_function(
    app: AppHandle,
    name: String,
    arguments: Value,
    executor: State<'_, DbExecutor>,
) -> Result<FunctionResult, String> {

    let call = FunctionCall { name, arguments };
    let app_clone = app.clone();

    let result_json = executor.write(move |conn| {
        let result = functions::execute_function(conn, &call, Some(&app_clone));
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await.map_err(|e| format!("Failed to execute function: {}", e))?;

    let result: FunctionResult = serde_json::from_str(&result_json)
        .map_err(|e| format!("Failed to parse result: {}", e))?;


    Ok(result)
}
