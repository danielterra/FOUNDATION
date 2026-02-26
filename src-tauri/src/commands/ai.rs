use crate::ai;
use crate::ai::{ChatMessage, GenerateRequest};
use crate::ai::functions::{self, FunctionCall, FunctionResult};
use crate::eavto::{DbExecutor, query};
use crate::owl::{Individual, Object};
use tauri::{AppHandle, Manager, State};
use serde_json::Value;

#[tauri::command]
pub async fn ai__save_api_key(
    app: AppHandle,
    api_key: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    super::log_backend(&app, "info", "Saving API key to ontology");

    executor.write(move |conn| {
        // Find existing Claude API key credential owned by ThisUser
        let existing_keys = query::get_by_predicate_object(conn, "foundation:ownedBy", "foundation:ThisUser");

        if let Ok(result) = existing_keys {
            // Filter to find Claude API key specifically
            for triple in result.triples {
                let credential_iri = &triple.subject;

                // Check if this credential is for Claude AI Service
                if let Ok(cred_for) = query::get_by_entity_predicate(conn, credential_iri, "foundation:credentialFor") {
                    if cred_for.triples.iter().any(|t| {
                        t.object.as_iri().map(|iri| iri == "foundation:ClaudeAIService").unwrap_or(false)
                    }) {
                        // Check if it's an APIKey type
                        if let Ok(types) = query::get_by_entity_predicate(conn, credential_iri, "rdf:type") {
                            if types.triples.iter().any(|t| {
                                t.object.as_iri().map(|iri| iri == "foundation:APIKey").unwrap_or(false)
                            }) {
                                // Found existing Claude API key, remove it
                                let all_triples = query::get_by_entity(conn, credential_iri)
                                    .map_err(|e| format!("Failed to get credential triples: {}", e))?;

                                if !all_triples.triples.is_empty() {
                                    crate::eavto::store::retract_triples(conn, &all_triples.triples, "ai")
                                        .map_err(|e| format!("Failed to retract old credential: {}", e))?;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Create new APIKey credential
        let timestamp = chrono::Utc::now().timestamp_millis();
        let api_key_iri = format!("foundation:ClaudeAPIKey_{}", timestamp);
        let credential = Individual::new(&api_key_iri);

        let now = chrono::Utc::now().to_rfc3339();

        // Assert as APIKey
        credential.assert(
            conn,
            "foundation:APIKey",
            "Claude AI API Key",
            "vpn_key",
            "ai"
        ).map_err(|e| format!("Failed to create APIKey: {}", e))?;

        // Link to service
        credential.add_property(
            conn,
            "foundation:credentialFor",
            Object::Iri("foundation:ClaudeAIService".to_string()),
            "ai"
        ).map_err(|e| format!("Failed to link to service: {}", e))?;

        // Link to owner
        credential.add_property(
            conn,
            "foundation:ownedBy",
            Object::Iri("foundation:ThisUser".to_string()),
            "ai"
        ).map_err(|e| format!("Failed to set owner: {}", e))?;

        // Set the actual key value
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

        // Set created timestamp
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

        Ok("API key saved".to_string())
    }).await?;

    super::log_backend(&app, "info", "API key saved successfully");

    Ok(())
}

#[tauri::command]
pub async fn ai__get_api_key(
    executor: State<'_, DbExecutor>,
) -> Result<Option<String>, String> {
    executor.read(|conn| {
        // Find credentials owned by ThisUser
        let owned_creds = query::get_by_predicate_object(conn, "foundation:ownedBy", "foundation:ThisUser")
            .ok();

        if let Some(result) = owned_creds {
            // Look for Claude API key
            for triple in result.triples {
                let credential_iri = &triple.subject;

                // Check if it's for Claude AI Service
                if let Ok(cred_for) = query::get_by_entity_predicate(conn, credential_iri, "foundation:credentialFor") {
                    if cred_for.triples.iter().any(|t| {
                        t.object.as_iri().map(|iri| iri == "foundation:ClaudeAIService").unwrap_or(false)
                    }) {
                        // Check if it's an APIKey
                        if let Ok(types) = query::get_by_entity_predicate(conn, credential_iri, "rdf:type") {
                            if types.triples.iter().any(|t| {
                                t.object.as_iri().map(|iri| iri == "foundation:APIKey").unwrap_or(false)
                            }) {
                                // Get the credential value
                                if let Ok(value_result) = query::get_by_entity_predicate(conn, credential_iri, "foundation:credentialValue") {
                                    if let Some(value) = value_result.triples.first().and_then(|t| t.object.as_literal()) {
                                        return Ok(Some(value));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }).await
}

#[tauri::command]
pub async fn ai__initialize(
    app: AppHandle,
    api_key: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    super::log_backend(&app, "info", "Initializing AI with Claude API");

    // Get default model from ontology
    let model_identifier = executor.read(|conn| {
        // Find models offered by ClaudeAIService where isDefaultModel = true
        let models = query::get_by_predicate_object(conn, "foundation:offeredBy", "foundation:ClaudeAIService")
            .ok();

        if let Some(result) = models {
            for triple in result.triples {
                let model_iri = &triple.subject;

                // Check if this is the default model
                if let Ok(is_default) = query::get_by_entity_predicate(conn, model_iri, "foundation:isDefaultModel") {
                    if is_default.triples.iter().any(|t| {
                        t.object.as_literal().map(|v| v == "true").unwrap_or(false)
                    }) {
                        // Get the model identifier
                        if let Ok(identifier_result) = query::get_by_entity_predicate(conn, model_iri, "foundation:modelIdentifier") {
                            if let Some(identifier) = identifier_result.triples.first().and_then(|t| t.object.as_literal()) {
                                return Ok(Some(identifier));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }).await.map_err(|e: String| format!("Failed to query default model: {}", e))?;

    if let Some(model) = &model_identifier {
        super::log_backend(&app, "info", &format!("Using model from ontology: {}", model));
    } else {
        super::log_backend(&app, "warn", "No default model found in ontology, using hardcoded fallback");
    }

    ai::initialize_ai_with_model(api_key, model_identifier).await?;

    super::log_backend(&app, "info", "AI initialized successfully");

    Ok(())
}

#[tauri::command]
pub async fn ai__generate(
    app: AppHandle,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    system: Option<String>,
) -> Result<String, String> {
    super::log_backend(&app, "info", &format!("Generating AI response with {} messages", messages.len()));

    let request = GenerateRequest {
        messages,
        max_tokens,
        temperature,
        system,
    };

    let response = ai::generate_response(request).await?;

    super::log_backend(&app, "info", &format!("AI response generated: {} chars", response.len()));

    Ok(response)
}

#[tauri::command]
pub async fn ai__list_available_models(
    app: AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Value, String> {
    super::log_backend(&app, "info", "Listing available models from Claude API");

    // Get API endpoint from ontology
    let api_info = executor.read(|conn| {
        let models_endpoint = query::get_by_entity_predicate(conn, "foundation:ClaudeAIService", "foundation:apiModelsEndpoint")
            .ok()
            .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()));

        let api_version = query::get_by_entity_predicate(conn, "foundation:ClaudeAIService", "foundation:apiVersion")
            .ok()
            .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()));

        Ok((models_endpoint, api_version))
    }).await?;

    let (endpoint, version) = api_info;
    let endpoint = endpoint.ok_or("API models endpoint not found in ontology")?;
    let version = version.ok_or("API version not found in ontology")?;

    // Get API key
    let api_key_result = ai__get_api_key(executor).await?;
    let api_key = api_key_result.ok_or("API key not found. Please configure your Claude API key first.")?;

    // Call the API
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

    super::log_backend(&app, "info", &format!("Retrieved models: {}", models_json));

    Ok(models_json)
}

#[tauri::command]
pub async fn ai__get_available_functions() -> Result<Value, String> {
    let functions = functions::get_available_functions();
    serde_json::to_value(functions)
        .map_err(|e| format!("Failed to serialize functions: {}", e))
}

#[tauri::command]
pub async fn ai__execute_function(
    app: AppHandle,
    name: String,
    arguments: Value,
    executor: State<'_, DbExecutor>,
) -> Result<FunctionResult, String> {
    super::log_backend(&app, "info", &format!("Executing function: {} with args: {}", name, arguments));

    let call = FunctionCall { name, arguments };

    let result = executor.read(move |conn| {
        Ok(functions::execute_function(conn, &call))
    }).await.map_err(|e| format!("Failed to execute function: {}", e))?;

    super::log_backend(&app, "info", &format!("Function result: success={}", result.success));

    Ok(result)
}
