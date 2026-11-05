use tauri::State;

use crate::owl::{self, Connection, DbExecutor, Individual, Object};

fn has_setup_runtime_route(conn: &Connection, service: &Individual) -> Result<bool, String> {
    let has_api_key = service
        .properties
        .iter()
        .any(|(property, value)| property == "foundation:apiKey" && value.as_iri().is_some());
    let api_base_url = service
        .properties
        .iter()
        .find(|(property, _)| property == "foundation:apiBaseUrl")
        .and_then(|(_, value)| value.as_literal())
        .filter(|url| !url.trim().is_empty());

    match (has_api_key, api_base_url) {
        (true, Some(url)) if url.contains("openrouter.ai") => Ok(true),
        (true, Some(url)) if url.contains("api.anthropic.com") => Ok(true),
        (false, None) => {
            let model_iris =
                owl::find_entities_with_property(conn, "foundation:offeredBy", &service.iri)
                    .map_err(|error| {
                        format!("Failed to inspect local AI service models: {}", error)
                    })?;
            for model_iri in model_iris {
                let Some(model) = Individual::get(conn, &model_iri).map_err(|error| {
                    format!(
                        "Failed to inspect local AI model '{}': {}",
                        model_iri, error
                    )
                })?
                else {
                    continue;
                };
                let model_identifier = model
                    .properties
                    .iter()
                    .find(|(property, _)| property == "foundation:modelIdentifier")
                    .and_then(|(_, value)| value.as_literal());
                if model_identifier.as_deref()
                    == Some(crate::commands::local_model::MODEL_IDENTIFIER)
                {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupAIIntegrationMode {
    Chat,
    Mcp,
}

impl SetupAIIntegrationMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "chat" => Ok(Self::Chat),
            "mcp" => Ok(Self::Mcp),
            _ => Err(format!("Unsupported AI integration mode '{}'", value)),
        }
    }
}

pub fn validate_setup_ai_selection(
    conn: &Connection,
    mode: SetupAIIntegrationMode,
    service_iri: Option<&str>,
    model_iri: Option<&str>,
) -> Result<(), String> {
    match mode {
        SetupAIIntegrationMode::Chat => {
            let service_iri = service_iri
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Chat mode requires an explicit AI service".to_string())?;
            let model_iri = model_iri
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Chat mode requires an explicit AI model".to_string())?;
            validate_setup_ai_model(conn, model_iri, service_iri)?;
            validate_setup_ai_service_credentials(conn, service_iri)
        }
        SetupAIIntegrationMode::Mcp => {
            if service_iri.is_some() || model_iri.is_some() {
                return Err("MCP mode must not include a Chat service or model".to_string());
            }
            Ok(())
        }
    }
}

fn resolve_setup_model_snapshot(
    offset: i64,
    requested_snapshot: Option<i64>,
    current_tx: i64,
) -> Result<i64, String> {
    if offset < 0 {
        return Err("Model page offset cannot be negative".to_string());
    }
    if offset > 0 && requested_snapshot.is_none() {
        return Err("A model catalog continuation requires its original snapshot_tx".to_string());
    }
    let snapshot = requested_snapshot.unwrap_or(current_tx);
    if snapshot < 0 {
        return Err("Model catalog snapshot_tx cannot be negative".to_string());
    }
    if snapshot > current_tx {
        return Err(format!(
            "Model catalog snapshot_tx {} is newer than the current database transaction {}",
            snapshot, current_tx
        ));
    }
    Ok(snapshot)
}

fn validate_setup_ai_service_credentials(
    conn: &Connection,
    service_iri: &str,
) -> Result<(), String> {
    let service = Individual::get(conn, service_iri)
        .map_err(|error| format!("Failed to load AI service '{}': {}", service_iri, error))?
        .ok_or_else(|| format!("AI service '{}' not found", service_iri))?;
    if !service
        .properties
        .iter()
        .any(|(property, value)| property == "foundation:apiKey" && value.as_iri().is_some())
    {
        return Ok(());
    }
    let has_endpoint = service
        .properties
        .iter()
        .find(|(property, _)| property == "foundation:apiBaseUrl")
        .and_then(|(_, value)| value.as_literal())
        .filter(|url| !url.trim().is_empty())
        .is_some();
    if !has_endpoint {
        return Err(format!(
            "AI service '{}' has no configured API endpoint",
            service_iri
        ));
    }

    let credential_iri = service
        .properties
        .iter()
        .find(|(property, _)| property == "foundation:apiKey")
        .and_then(|(_, value)| value.as_iri())
        .ok_or_else(|| format!("AI service '{}' requires an API key", service_iri))?;
    let credential_value =
        owl::get_literal_property(conn, credential_iri, "foundation:credentialValue")
            .map_err(|error| format!("Failed to load API key for '{}': {}", service_iri, error))?;
    if credential_value.map_or(true, |value| value.trim().is_empty()) {
        return Err(format!(
            "AI service '{}' has no configured API key",
            service_iri
        ));
    }
    Ok(())
}

pub fn persist_setup_ai_mode(
    conn: &mut Connection,
    mode: SetupAIIntegrationMode,
) -> Result<(), String> {
    let setting_iris =
        owl::find_entities_with_property(conn, "rdf:type", "foundation:SoftwareSetting")
            .map_err(|error| format!("Failed to find setup mode settings: {}", error))?;
    let mut mode_setting = None;
    for iri in setting_iris {
        if let Some(setting) = Individual::get(conn, &iri)
            .map_err(|error| format!("Failed to load setting '{}': {}", iri, error))?
        {
            let key = setting
                .properties
                .iter()
                .find(|(property, _)| property == "foundation:settingKey")
                .and_then(|(_, value)| value.as_literal());
            if key.as_deref() == Some("aiIntegrationMode") {
                if mode_setting.replace(iri).is_some() {
                    return Err("Multiple AI integration mode settings exist".to_string());
                }
            }
        }
    }

    let setting_iri = match mode_setting {
        Some(iri) => iri,
        None => {
            let iri = format!(
                "foundation:AIIntegrationModeSetting_{}",
                chrono::Utc::now().timestamp_millis()
            );
            let setting = Individual::new(&iri);
            setting
                .assert(
                    conn,
                    "foundation:SoftwareSetting",
                    "AI Integration Mode",
                    "settings",
                    "setup",
                )
                .map_err(|error| {
                    format!("Failed to create AI integration mode setting: {}", error)
                })?;
            setting
                .add_property(
                    conn,
                    "foundation:settingKey",
                    vec![Object::Literal {
                        value: "aiIntegrationMode".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "setup",
                )
                .map_err(|error| format!("Failed to set AI integration mode key: {}", error))?;
            setting
                .add_property(
                    conn,
                    "foundation:origin",
                    vec![Object::Iri("foundation:ThisFoundationInstance".to_string())],
                    "setup",
                )
                .map_err(|error| format!("Failed to set AI integration mode origin: {}", error))?;
            iri
        }
    };

    let mode_value = match mode {
        SetupAIIntegrationMode::Chat => "chat",
        SetupAIIntegrationMode::Mcp => "mcp",
    };
    let setting = Individual::new(&setting_iri);
    setting
        .add_property(
            conn,
            "foundation:settingValue",
            vec![Object::Literal {
                value: mode_value.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }],
            "setup",
        )
        .map_err(|error| format!("Failed to save AI integration mode: {}", error))?;
    setting
        .add_property(
            conn,
            "foundation:hasStatus",
            vec![Object::Iri("foundation:Completed".to_string())],
            "setup",
        )
        .map_err(|error| format!("Failed to set AI integration mode status: {}", error))?;

    if mode == SetupAIIntegrationMode::Mcp {
        clear_setup_ai_service_setting(conn)?;
        clear_setup_ai_model_setting(conn)?;
    }
    Ok(())
}

pub fn persist_setup_ai_service(conn: &mut Connection, service_iri: &str) -> Result<(), String> {
    let setting_iris =
        owl::find_entities_with_property(conn, "rdf:type", "foundation:SoftwareSetting")
            .map_err(|error| format!("Failed to find AI service settings: {}", error))?;
    let mut setting_iri = None;
    for iri in setting_iris {
        let setting = Individual::get(conn, &iri)
            .map_err(|error| format!("Failed to load setting '{}': {}", iri, error))?;
        let key = setting
            .as_ref()
            .and_then(|item| {
                item.properties
                    .iter()
                    .find(|(p, _)| p == "foundation:settingKey")
            })
            .and_then(|(_, value)| value.as_literal());
        if key.as_deref() == Some("aiService") {
            if setting_iri.replace(iri).is_some() {
                return Err("Multiple AI service settings exist".to_string());
            }
        }
    }
    let setting_iri = setting_iri.ok_or_else(|| "AI service setting not found".to_string())?;
    owl::replace_all_property_literals(
        conn,
        &setting_iri,
        "foundation:settingValue",
        &[service_iri],
        "setup",
    )
    .map_err(|error| format!("Failed to persist selected AI service: {}", error))
}

pub fn get_setup_ai_mode(conn: &Connection) -> Result<Option<String>, String> {
    let setting_iris =
        owl::find_entities_with_property(conn, "rdf:type", "foundation:SoftwareSetting")
            .map_err(|error| format!("Failed to find AI integration mode: {}", error))?;
    let mut mode = None;
    for iri in setting_iris {
        let setting = Individual::get(conn, &iri)
            .map_err(|error| format!("Failed to load setting '{}': {}", iri, error))?;
        let Some(setting) = setting else {
            continue;
        };
        let key = setting
            .properties
            .iter()
            .find(|(property, _)| property == "foundation:settingKey")
            .and_then(|(_, value)| value.as_literal());
        if key.as_deref() == Some("aiIntegrationMode") {
            let value = setting
                .properties
                .iter()
                .find(|(property, _)| property == "foundation:settingValue")
                .and_then(|(_, value)| value.as_literal());
            if mode.replace(value.unwrap_or_default()).is_some() {
                return Err("Multiple AI integration mode settings exist".to_string());
            }
        }
    }
    Ok(mode)
}

fn clear_setup_ai_service_setting(conn: &mut Connection) -> Result<(), String> {
    let setting_iris =
        owl::find_entities_with_property(conn, "rdf:type", "foundation:SoftwareSetting")
            .map_err(|error| format!("Failed to find AI service settings: {}", error))?;
    for iri in setting_iris {
        let setting = Individual::get(conn, &iri)
            .map_err(|error| format!("Failed to load setting '{}': {}", iri, error))?;
        let key = setting
            .as_ref()
            .and_then(|item| {
                item.properties
                    .iter()
                    .find(|(p, _)| p == "foundation:settingKey")
            })
            .and_then(|(_, value)| value.as_literal());
        if key.as_deref() == Some("aiService") {
            owl::replace_all_property_literals(conn, &iri, "foundation:settingValue", &[], "setup")
                .map_err(|error| format!("Failed to clear AI service setting: {}", error))?;
        }
    }
    Ok(())
}

fn clear_setup_ai_model_setting(conn: &mut Connection) -> Result<(), String> {
    let setting = Individual::get(conn, "foundation:DefaultAIModelSetting")
        .map_err(|error| format!("Failed to load DefaultAIModelSetting: {}", error))?;
    if setting.is_some() {
        owl::replace_all_property_literals(
            conn,
            "foundation:DefaultAIModelSetting",
            "foundation:settingValue",
            &[],
            "setup",
        )
        .map_err(|error| format!("Failed to clear AI model setting: {}", error))?;
        owl::replace_all_property_iris(
            conn,
            "foundation:DefaultAIModelSetting",
            "foundation:appliedTo",
            &[],
            "setup",
        )
        .map_err(|error| format!("Failed to clear AI model service: {}", error))?;
    }
    Ok(())
}

pub fn validate_setup_ai_service(conn: &Connection, service_iri: &str) -> Result<(), String> {
    let service = Individual::get(conn, service_iri)
        .map_err(|e| format!("Failed to load AI service '{}': {}", service_iri, e))?
        .ok_or_else(|| format!("AI service '{}' not found", service_iri))?;
    let service_iris =
        owl::find_entities_with_property(conn, "rdf:type", "foundation:AIAPIService")
            .map_err(|e| format!("Failed to validate AI service '{}': {}", service_iri, e))?;
    if !service_iris.iter().any(|iri| iri == service_iri) {
        return Err(format!("Entity '{}' is not an AI API service", service_iri));
    }
    if !has_setup_runtime_route(conn, &service)? {
        return Err(format!(
            "AI service '{}' has no configured runtime route for setup",
            service_iri
        ));
    }

    Ok(())
}

pub fn validate_setup_ai_model(
    conn: &Connection,
    model_iri: &str,
    service_iri: &str,
) -> Result<(), String> {
    validate_setup_ai_service(conn, service_iri)?;

    let model = Individual::get(conn, model_iri)
        .map_err(|e| format!("Failed to load AI model '{}': {}", model_iri, e))?
        .ok_or_else(|| format!("Selected AI model '{}' not found", model_iri))?;
    let model_iris = owl::find_entities_with_property(conn, "rdf:type", "foundation:AIModel")
        .map_err(|e| format!("Failed to validate AI model '{}': {}", model_iri, e))?;
    if !model_iris.iter().any(|iri| iri == model_iri) {
        return Err(format!("Entity '{}' is not an AI model", model_iri));
    }

    let model_service = model
        .properties
        .iter()
        .find(|(predicate, _)| predicate == "foundation:offeredBy")
        .and_then(|(_, value)| value.as_iri());
    if model_service != Some(service_iri) {
        return Err(format!(
            "Selected AI model '{}' is not offered by service '{}'",
            model_iri, service_iri
        ));
    }

    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__list_ai_services(
    limit: Option<i64>,
    offset: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor
        .read(move |conn| {
            // bounded-por-natureza; keyset-tx não agrega (não é append-no-topo)
            let page_size = limit.unwrap_or(100).max(1).min(500);
            let page_offset = offset.unwrap_or(0).max(0);
            let service_iris = owl::find_entities_with_property_bounded(
                conn,
                "rdf:type",
                "foundation:AIAPIService",
                page_size,
                page_offset,
                Some("rdfs:label"),
            )
            .map_err(|e| format!("Failed to query services: {}", e))?;

            let mut result = Vec::new();
            for service_iri in service_iris {
                let service_iri = &service_iri;

                if validate_setup_ai_service(conn, service_iri).is_err() {
                    continue;
                }

                if let Ok(Some(service_ind)) = Individual::get(conn, service_iri) {
                    let label = service_ind.label;
                    let comment = service_ind
                        .properties
                        .iter()
                        .find(|(k, _)| k == "rdfs:comment")
                        .and_then(|(_, v)| v.as_literal())
                        .unwrap_or_default();

                    let is_local = !service_ind
                        .properties
                        .iter()
                        .any(|(k, _)| k == "foundation:apiBaseUrl");

                    result.push(serde_json::json!({
                        "iri": service_iri,
                        "label": label,
                        "description": comment,
                        "isLocal": is_local,
                    }));
                }
            }

            Ok(result)
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__list_ai_models(
    service_iri: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
    snapshot_tx: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    executor
        .read(move |conn| {
            if let Some(service_iri) = service_iri.as_deref() {
                validate_setup_ai_service(conn, service_iri)?;
            }
            let page_size = limit.unwrap_or(100).max(1).min(500);
            let page_offset = offset.unwrap_or(0).max(0);

            let current_tx = owl::current_snapshot_tx(conn)
                .map_err(|e| format!("Failed to read model catalog snapshot: {}", e))?;
            let pinned_tx =
                resolve_setup_model_snapshot(offset.unwrap_or(0), snapshot_tx, current_tx)?;

            let (rows, has_more) = owl::list_ai_models_as_of(
                conn,
                service_iri.as_deref(),
                "foundation:offeredBy",
                "rdf:type",
                "foundation:AIModel",
                "foundation:isDefaultModel",
                "rdfs:label",
                "foundation:modelIdentifier",
                "foundation:modelVersion",
                "rdfs:comment",
                pinned_tx,
                page_size,
                page_offset,
            )
            .map_err(|e| format!("Failed to query models: {}", e))?;

            let items: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|row| {
                    serde_json::json!({
                        "iri": row.subject,
                        "label": row.label,
                        "description": row.description,
                        "modelIdentifier": row.model_identifier,
                        "modelVersion": row.model_version,
                        "isDefault": row.is_default,
                    })
                })
                .collect();

            let next_cursor = if has_more {
                serde_json::Value::Number((page_offset + page_size).into())
            } else {
                serde_json::Value::Null
            };

            Ok(serde_json::json!({
                "items": items,
                "next_cursor": next_cursor,
                "has_more": has_more,
                "snapshot_tx": pinned_tx,
            }))
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__get_current_ai_model(
    executor: State<'_, DbExecutor>,
) -> Result<Option<serde_json::Value>, String> {
    executor
        .read(|conn| {
            let model_iri = match crate::commands::chat::settings::get_ai_model_iri(conn)? {
                Some(iri) if !iri.is_empty() => iri,
                _ => return Ok(None),
            };

            if let Ok(Some(model_ind)) = Individual::get(conn, &model_iri) {
                let model_identifier = model_ind
                    .properties
                    .iter()
                    .find(|(k, _)| k == "foundation:modelIdentifier")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                Ok(Some(serde_json::json!({
                    "iri": model_iri,
                    "label": model_ind.label,
                    "modelIdentifier": model_identifier,
                })))
            } else {
                Ok(None)
            }
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__get_active_model_info(
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    executor
        .read(|conn| {
            let integration_mode = get_setup_ai_mode(conn)?.unwrap_or_default();
            let service_iri =
                crate::commands::chat::settings::get_ai_service_iri(conn)?.unwrap_or_default();
            let model_iri =
                crate::commands::chat::settings::get_ai_model_iri(conn)?.unwrap_or_default();

            let service_ind = if service_iri.is_empty() {
                None
            } else {
                Individual::get(conn, &service_iri).ok().flatten()
            };
            let model_ind = if model_iri.is_empty() {
                None
            } else {
                Individual::get(conn, &model_iri).ok().flatten()
            };

            let service_label = service_ind
                .as_ref()
                .and_then(|i| i.label.clone())
                .unwrap_or_else(|| match integration_mode.as_str() {
                    "mcp" => "MCP client".to_string(),
                    _ => "Not configured".to_string(),
                });
            let model_label = model_ind
                .as_ref()
                .and_then(|i| i.label.clone())
                .unwrap_or_else(|| "Not configured".to_string());

            let is_local = service_ind
                .map(|i| {
                    !i.properties
                        .iter()
                        .any(|(k, _)| k == "foundation:apiBaseUrl")
                })
                .unwrap_or(false);

            Ok(serde_json::json!({
                "integrationMode": integration_mode,
                "modelLabel": model_label,
                "serviceLabel": service_label,
                "isLocal": is_local,
            }))
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__get_current_ai_service(
    executor: State<'_, DbExecutor>,
) -> Result<Option<serde_json::Value>, String> {
    executor
        .read(|conn| {
            let service_iri = match crate::commands::chat::settings::get_ai_service_iri(conn)? {
                Some(iri) => iri,
                None => return Ok(None),
            };

            if let Ok(Some(service_ind)) = Individual::get(conn, &service_iri) {
                let is_local = !service_ind
                    .properties
                    .iter()
                    .any(|(k, _)| k == "foundation:apiBaseUrl");
                Ok(Some(serde_json::json!({
                    "iri": service_iri,
                    "label": service_ind.label,
                    "isLocal": is_local,
                })))
            } else {
                Ok(None)
            }
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__save_ai_service(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(move |conn| {
            validate_setup_ai_service(conn, &service_iri)?;
            let setting_iris =
                owl::find_entities_with_property(conn, "rdf:type", "foundation:SoftwareSetting")
                    .map_err(|e| format!("Failed to query settings: {}", e))?;

            for iri in setting_iris {
                if let Ok(Some(setting)) = Individual::get(conn, &iri) {
                    let key = setting
                        .properties
                        .iter()
                        .find(|(k, _)| k == "foundation:settingKey")
                        .and_then(|(_, v)| v.as_literal());
                    if key.as_deref() == Some("aiService") {
                        setting
                            .add_property(
                                conn,
                                "foundation:settingValue",
                                vec![Object::Literal {
                                    value: service_iri.clone(),
                                    datatype: Some("xsd:string".to_string()),
                                    language: None,
                                }],
                                "settings",
                            )
                            .map_err(|e| format!("Failed to update settingValue: {}", e))?;
                        return Ok(String::new());
                    }
                }
            }
            Err("DefaultAIServiceSetting not found".to_string())
        })
        .await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__save_ai_model(
    model_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(move |conn| {
            let service_iri = crate::commands::chat::settings::get_ai_service_iri(conn)?
                .ok_or_else(|| "AI service must be selected before saving a model".to_string())?;
            validate_setup_ai_model(conn, &model_iri, &service_iri)?;
            let setting = Individual::get(conn, "foundation:DefaultAIModelSetting")
                .map_err(|e| format!("Failed to get DefaultAIModelSetting: {}", e))?
                .ok_or_else(|| "DefaultAIModelSetting not found".to_string())?;

            setting
                .add_property(
                    conn,
                    "foundation:settingValue",
                    vec![Object::Literal {
                        value: model_iri.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "settings",
                )
                .map_err(|e| format!("Failed to update settingValue: {}", e))?;

            Ok(String::new())
        })
        .await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn agent__get_ai_config(
    agent_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    executor.read(move |conn| {
        let own_service = owl::get_iri_property(conn, &agent_iri, "foundation:usesService")
            .map_err(|e| format!("Failed to get usesService: {}", e))?;
        let own_model = owl::get_iri_property(conn, &agent_iri, "foundation:usesModel")
            .map_err(|e| format!("Failed to get usesModel: {}", e))?;

        let (service_iri, service_overridden) = if let Some(iri) = own_service {
            (iri, true)
        } else {
            let fallback = crate::commands::chat::settings::get_ai_service_iri(conn)?
                .filter(|iri| !iri.trim().is_empty())
                .ok_or_else(|| "No Chat AI service is configured. Choose Chat mode and select a service.".to_string())?;
            (fallback, false)
        };

        let (model_iri, model_overridden) = if let Some(iri) = own_model {
            (iri, true)
        } else {
            let fallback = crate::commands::chat::settings::get_ai_model_iri(conn)?
                .filter(|iri| !iri.trim().is_empty())
                .ok_or_else(|| "No Chat AI model is configured. Choose a model before starting Chat.".to_string())?;
            (fallback, false)
        };

        let service_label = Individual::get(conn, &service_iri).ok().flatten()
            .and_then(|i| i.label).unwrap_or_else(|| service_iri.clone());

        let (model_label, supports_tool_calling) = if model_iri.is_empty() {
            (String::new(), false)
        } else {
            let model_ind = Individual::get(conn, &model_iri).ok().flatten();
            let label = model_ind.as_ref().and_then(|i| i.label.clone())
                .unwrap_or_else(|| model_iri.clone());
            let has_tools = model_ind.map(|i| i.properties.iter().any(|(k, v)| {
                k == "foundation:modelCapability"
                    && matches!(v, crate::owl::Object::Literal { value, .. } if value == "tool_calling")
            })).unwrap_or(false);
            (label, has_tools)
        };

        Ok(serde_json::json!({
            "serviceIri": service_iri,
            "serviceLabel": service_label,
            "serviceOverridden": service_overridden,
            "modelIri": model_iri,
            "modelLabel": model_label,
            "modelOverridden": model_overridden,
            "supportsToolCalling": supports_tool_calling,
        }))
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn agent__set_ai_config(
    agent_iri: String,
    service_iri: Option<String>,
    model_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(move |conn| {
            if let Some(ref svc) = service_iri {
                if svc.is_empty() {
                    owl::replace_all_property_iris(
                        conn,
                        &agent_iri,
                        "foundation:usesService",
                        &[],
                        "agent_config",
                    )
                    .map_err(|e| format!("Failed to clear usesService: {}", e))?;
                } else {
                    owl::replace_all_property_iris(
                        conn,
                        &agent_iri,
                        "foundation:usesService",
                        &[svc.as_str()],
                        "agent_config",
                    )
                    .map_err(|e| format!("Failed to set usesService: {}", e))?;
                }
            }
            if let Some(ref mdl) = model_iri {
                if mdl.is_empty() {
                    owl::replace_all_property_iris(
                        conn,
                        &agent_iri,
                        "foundation:usesModel",
                        &[],
                        "agent_config",
                    )
                    .map_err(|e| format!("Failed to clear usesModel: {}", e))?;
                } else {
                    owl::replace_all_property_iris(
                        conn,
                        &agent_iri,
                        "foundation:usesModel",
                        &[mdl.as_str()],
                        "agent_config",
                    )
                    .map_err(|e| format!("Failed to set usesModel: {}", e))?;
                }
            }
            Ok(String::new())
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod setup_ai_mode_tests {
    use super::{
        resolve_setup_model_snapshot, validate_setup_ai_selection, validate_setup_ai_service,
        SetupAIIntegrationMode,
    };
    use crate::owl::{Class, ClassType, Individual, Object, Triple};

    fn setup_ai_test_db() -> crate::owl::Connection {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        Class::new("foundation:AIAPIService")
            .assert(
                &mut conn,
                ClassType::OwlClass,
                "AI API service",
                "https://example.com/icon.svg",
                None,
                "test",
            )
            .unwrap();
        Class::new("foundation:AIModel")
            .assert(
                &mut conn,
                ClassType::OwlClass,
                "AI model",
                "https://example.com/icon.svg",
                None,
                "test",
            )
            .unwrap();
        conn
    }

    fn add_test_service(conn: &mut crate::owl::Connection, iri: &str, endpoint: Option<&str>) {
        let service = Individual::new(iri);
        service
            .assert_without_icon(conn, "foundation:AIAPIService", "Test service", "test")
            .unwrap();
        if let Some(endpoint) = endpoint {
            crate::eavto::store::assert_triples(
                conn,
                &[
                    Triple::new(
                        iri,
                        "foundation:apiKey",
                        Object::Iri("test:Credential".to_string()),
                    ),
                    Triple::new(
                        iri,
                        "foundation:apiBaseUrl",
                        Object::Literal {
                            value: endpoint.to_string(),
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        },
                    ),
                ],
                "test",
            )
            .unwrap();
        }
    }

    #[test]
    fn parses_only_supported_integration_modes() {
        assert_eq!(
            SetupAIIntegrationMode::parse("chat").unwrap(),
            SetupAIIntegrationMode::Chat
        );
        assert_eq!(
            SetupAIIntegrationMode::parse("mcp").unwrap(),
            SetupAIIntegrationMode::Mcp
        );
        assert!(SetupAIIntegrationMode::parse("claude").is_err());
    }

    #[test]
    fn chat_requires_explicit_service_and_model() {
        let mut conn = setup_ai_test_db();
        add_test_service(
            &mut conn,
            "test:ClaudeService",
            Some("https://api.anthropic.com/v1"),
        );
        assert!(
            validate_setup_ai_selection(&conn, SetupAIIntegrationMode::Chat, None, None)
                .unwrap_err()
                .contains("requires an explicit AI service")
        );
        assert!(validate_setup_ai_selection(
            &conn,
            SetupAIIntegrationMode::Chat,
            Some("test:ClaudeService"),
            None
        )
        .unwrap_err()
        .contains("requires an explicit AI model"));
    }

    #[test]
    fn chat_rejects_an_unrouted_service_and_model_pair() {
        let mut conn = setup_ai_test_db();
        add_test_service(
            &mut conn,
            "test:UnsupportedService",
            Some("https://example.com/v1"),
        );
        assert!(validate_setup_ai_selection(
            &conn,
            SetupAIIntegrationMode::Chat,
            Some("test:UnsupportedService"),
            Some("foundation:UnknownAIModel"),
        )
        .unwrap_err()
        .contains("no configured runtime route"));
    }

    #[test]
    fn setup_accepts_an_ontology_service_with_a_supported_runtime_route() {
        let mut conn = setup_ai_test_db();
        add_test_service(
            &mut conn,
            "test:OpenRouterService",
            Some("https://openrouter.ai/api/v1"),
        );

        assert!(validate_setup_ai_service(&conn, "test:OpenRouterService").is_ok());
    }

    #[test]
    fn setup_rejects_an_ontology_service_without_a_runtime_route() {
        let mut conn = setup_ai_test_db();
        add_test_service(
            &mut conn,
            "test:UnsupportedService",
            Some("https://example.com/v1"),
        );

        assert!(validate_setup_ai_service(&conn, "test:UnsupportedService")
            .unwrap_err()
            .contains("no configured runtime route"));
    }

    #[test]
    fn setup_rejects_a_credentialless_service_without_a_local_runtime_model() {
        let mut conn = setup_ai_test_db();
        add_test_service(&mut conn, "test:UnconfiguredLocalService", None);

        assert!(
            validate_setup_ai_service(&conn, "test:UnconfiguredLocalService")
                .unwrap_err()
                .contains("no configured runtime route")
        );
    }

    #[test]
    fn setup_accepts_a_local_service_offering_the_runtime_model() {
        let mut conn = setup_ai_test_db();
        add_test_service(&mut conn, "test:LocalService", None);
        Individual::new("test:LocalModel")
            .assert_without_icon(&mut conn, "foundation:AIModel", "Local model", "test")
            .unwrap();
        crate::eavto::store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "test:LocalModel",
                    "foundation:offeredBy",
                    Object::Iri("test:LocalService".to_string()),
                ),
                Triple::new(
                    "test:LocalModel",
                    "foundation:modelIdentifier",
                    Object::Literal {
                        value: crate::commands::local_model::MODEL_IDENTIFIER.to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
            ],
            "test",
        )
        .unwrap();

        assert!(validate_setup_ai_service(&conn, "test:LocalService").is_ok());
    }

    #[test]
    fn setup_reports_an_unknown_service_as_not_found() {
        let conn = setup_ai_test_db();

        assert!(validate_setup_ai_service(&conn, "test:MissingService")
            .unwrap_err()
            .contains("not found"));
    }

    #[test]
    fn mcp_rejects_chat_service_or_model() {
        let conn = crate::eavto::test_helpers::setup_test_db();
        assert!(validate_setup_ai_selection(
            &conn,
            SetupAIIntegrationMode::Mcp,
            Some("test:ClaudeService"),
            None
        )
        .unwrap_err()
        .contains("must not include a Chat service or model"));
        assert!(
            validate_setup_ai_selection(&conn, SetupAIIntegrationMode::Mcp, None, None).is_ok()
        );
    }

    #[test]
    fn model_catalog_continuations_keep_the_original_snapshot() {
        assert_eq!(resolve_setup_model_snapshot(0, None, 12).unwrap(), 12);
        assert_eq!(resolve_setup_model_snapshot(50, Some(12), 18).unwrap(), 12);
        assert!(resolve_setup_model_snapshot(50, None, 18).is_err());
        assert!(resolve_setup_model_snapshot(-1, Some(12), 18).is_err());
        assert!(resolve_setup_model_snapshot(50, Some(19), 18).is_err());
    }

    #[test]
    fn model_catalog_pages_remain_stable_when_new_models_are_added() {
        let mut conn = setup_ai_test_db();
        add_test_service(&mut conn, "test:LocalService", None);
        let initial_models = [("test:ModelA", "Alpha"), ("test:ModelB", "Bravo")]
            .map(|(iri, label)| {
                [
                    Triple::new(
                        iri,
                        "foundation:offeredBy",
                        Object::Iri("test:LocalService".to_string()),
                    ),
                    Triple::new(
                        iri,
                        "rdfs:label",
                        Object::Literal {
                            value: label.to_string(),
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        },
                    ),
                ]
            })
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        crate::eavto::store::assert_triples(&mut conn, &initial_models, "test").unwrap();

        let snapshot_tx = crate::owl::current_snapshot_tx(&conn).unwrap();
        let first_page = crate::owl::list_ai_models_as_of(
            &conn,
            Some("test:LocalService"),
            "foundation:offeredBy",
            "rdf:type",
            "foundation:AIModel",
            "foundation:isDefaultModel",
            "rdfs:label",
            "foundation:modelIdentifier",
            "foundation:modelVersion",
            "rdfs:comment",
            snapshot_tx,
            1,
            0,
        )
        .unwrap();
        assert!(first_page.1);
        assert_eq!(first_page.0[0].subject, "test:ModelA");

        crate::eavto::store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "test:Model0",
                    "foundation:offeredBy",
                    Object::Iri("test:LocalService".to_string()),
                ),
                Triple::new(
                    "test:Model0",
                    "rdfs:label",
                    Object::Literal {
                        value: "0 New model".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
            ],
            "test",
        )
        .unwrap();

        let second_page = crate::owl::list_ai_models_as_of(
            &conn,
            Some("test:LocalService"),
            "foundation:offeredBy",
            "rdf:type",
            "foundation:AIModel",
            "foundation:isDefaultModel",
            "rdfs:label",
            "foundation:modelIdentifier",
            "foundation:modelVersion",
            "rdfs:comment",
            snapshot_tx,
            1,
            1,
        )
        .unwrap();
        assert!(!second_page.1);
        assert_eq!(second_page.0[0].subject, "test:ModelB");
    }
}
