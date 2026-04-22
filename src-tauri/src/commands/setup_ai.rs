use tauri::State;

use crate::owl::{self, DbExecutor, Individual, Object};

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__list_ai_services(
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor.read(|conn| {
        let service_iris = owl::find_entities_with_property(conn, "rdf:type", "foundation:AIAPIService")
            .map_err(|e| format!("Failed to query services: {}", e))?;

        let mut result = Vec::new();
        for service_iri in service_iris {
            let service_iri = &service_iri;

            if let Ok(Some(service_ind)) = Individual::get(conn, service_iri) {
                let label = service_ind.label;
                let comment = service_ind.properties.iter()
                    .find(|(k, _)| k == "rdfs:comment")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                let is_local = !service_ind.properties.iter()
                    .any(|(k, _)| k == "foundation:apiKey");

                result.push(serde_json::json!({
                    "iri": service_iri,
                    "label": label,
                    "description": comment,
                    "isLocal": is_local,
                }));
            }
        }

        Ok(result)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__list_ai_models(
    service_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor.read(move |conn| {
        let model_iris = if let Some(ref service) = service_iri {
            owl::find_entities_with_property(conn, "foundation:offeredBy", service)
                .map_err(|e| format!("Failed to query models for service: {}", e))?
        } else {
            owl::find_entities_with_property(conn, "rdf:type", "foundation:AIModel")
                .map_err(|e| format!("Failed to query models: {}", e))?
        };

        let mut result = Vec::new();
        for model_iri in model_iris {
            let model_iri = &model_iri;

            if let Ok(Some(model_ind)) = Individual::get(conn, model_iri) {
                let label = model_ind.label;
                let comment = model_ind.properties.iter()
                    .find(|(k, _)| k == "rdfs:comment")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                let model_identifier = model_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:modelIdentifier")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                let is_default = model_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:isDefaultModel")
                    .and_then(|(_, v)| v.as_literal())
                    .map(|v| v == "true")
                    .unwrap_or(false);

                let model_version = model_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:modelVersion")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                result.push(serde_json::json!({
                    "iri": model_iri,
                    "label": label,
                    "description": comment,
                    "modelIdentifier": model_identifier,
                    "modelVersion": model_version,
                    "isDefault": is_default,
                }));
            }
        }

        result.sort_by(|a, b| {
            let a_default = a["isDefault"].as_bool().unwrap_or(false);
            let b_default = b["isDefault"].as_bool().unwrap_or(false);

            match (b_default, a_default) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a["label"].as_str().cmp(&b["label"].as_str()),
            }
        });

        Ok(result)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__get_current_ai_model(
    executor: State<'_, DbExecutor>,
) -> Result<Option<serde_json::Value>, String> {
    executor.read(|conn| {
        let model_iri = match crate::commands::chat::settings::get_ai_model_iri(conn)? {
            Some(iri) if !iri.is_empty() => iri,
            _ => return Ok(None),
        };

        if let Ok(Some(model_ind)) = Individual::get(conn, &model_iri) {
            let model_identifier = model_ind.properties.iter()
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
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__get_active_model_info(
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    executor.read(|conn| {
        let service_iri = crate::commands::chat::settings::get_ai_service_iri(conn)?
            .unwrap_or_else(|| "foundation:ClaudeAIService".to_string());
        let model_iri = crate::commands::chat::settings::get_ai_model_iri(conn)?
            .unwrap_or_default();

        let service_ind = Individual::get(conn, &service_iri).ok().flatten();
        let model_ind = if model_iri.is_empty() { None } else {
            Individual::get(conn, &model_iri).ok().flatten()
        };

        let service_label = service_ind.as_ref().and_then(|i| i.label.clone())
            .unwrap_or_else(|| service_iri.clone());
        let model_label = model_ind.as_ref().and_then(|i| i.label.clone())
            .unwrap_or_else(|| model_iri.clone());

        let is_local = service_ind.map(|i| {
            !i.properties.iter().any(|(k, _)| k == "foundation:apiKey")
        }).unwrap_or(false);

        Ok(serde_json::json!({
            "modelLabel": model_label,
            "serviceLabel": service_label,
            "isLocal": is_local,
        }))
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__get_current_ai_service(
    executor: State<'_, DbExecutor>,
) -> Result<Option<serde_json::Value>, String> {
    executor.read(|conn| {
        let service_iri = match crate::commands::chat::settings::get_ai_service_iri(conn)? {
            Some(iri) => iri,
            None => return Ok(None),
        };

        if let Ok(Some(service_ind)) = Individual::get(conn, &service_iri) {
            Ok(Some(serde_json::json!({
                "iri": service_iri,
                "label": service_ind.label,
            })))
        } else {
            Ok(None)
        }
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__save_ai_service(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        let setting_iris = owl::find_entities_with_property(conn, "rdf:type", "foundation:SoftwareSetting")
            .map_err(|e| format!("Failed to query settings: {}", e))?;

        for iri in setting_iris {
            if let Ok(Some(setting)) = Individual::get(conn, &iri) {
                let key = setting.properties.iter()
                    .find(|(k, _)| k == "foundation:settingKey")
                    .and_then(|(_, v)| v.as_literal());
                if key.as_deref() == Some("aiService") {
                    setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
                        value: service_iri.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }], "settings").map_err(|e| format!("Failed to update settingValue: {}", e))?;
                    return Ok(String::new());
                }
            }
        }
        Err("DefaultAIServiceSetting not found".to_string())
    }).await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__save_ai_model(
    model_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        let setting = Individual::get(conn, "foundation:DefaultAIModelSetting")
            .map_err(|e| format!("Failed to get DefaultAIModelSetting: {}", e))?
            .ok_or_else(|| "DefaultAIModelSetting not found".to_string())?;

        setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
            value: model_iri.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "settings").map_err(|e| format!("Failed to update settingValue: {}", e))?;

        Ok(String::new())
    }).await?;
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
                .unwrap_or_else(|| "foundation:ClaudeAIService".to_string());
            (fallback, false)
        };

        let (model_iri, model_overridden) = if let Some(iri) = own_model {
            (iri, true)
        } else {
            let fallback = crate::commands::chat::settings::get_ai_model_iri(conn)?
                .unwrap_or_default();
            (fallback, false)
        };

        let service_label = Individual::get(conn, &service_iri).ok().flatten()
            .and_then(|i| i.label).unwrap_or_else(|| service_iri.clone());
        let model_label = if model_iri.is_empty() { String::new() } else {
            Individual::get(conn, &model_iri).ok().flatten()
                .and_then(|i| i.label).unwrap_or_else(|| model_iri.clone())
        };

        Ok(serde_json::json!({
            "serviceIri": service_iri,
            "serviceLabel": service_label,
            "serviceOverridden": service_overridden,
            "modelIri": model_iri,
            "modelLabel": model_label,
            "modelOverridden": model_overridden,
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
    executor.write(move |conn| {
        if let Some(ref svc) = service_iri {
            if svc.is_empty() {
                owl::replace_all_property_iris(conn, &agent_iri, "foundation:usesService", &[], "agent_config")
                    .map_err(|e| format!("Failed to clear usesService: {}", e))?;
            } else {
                owl::replace_all_property_iris(conn, &agent_iri, "foundation:usesService", &[svc.as_str()], "agent_config")
                    .map_err(|e| format!("Failed to set usesService: {}", e))?;
            }
        }
        if let Some(ref mdl) = model_iri {
            if mdl.is_empty() {
                owl::replace_all_property_iris(conn, &agent_iri, "foundation:usesModel", &[], "agent_config")
                    .map_err(|e| format!("Failed to clear usesModel: {}", e))?;
            } else {
                owl::replace_all_property_iris(conn, &agent_iri, "foundation:usesModel", &[mdl.as_str()], "agent_config")
                    .map_err(|e| format!("Failed to set usesModel: {}", e))?;
            }
        }
        Ok(String::new())
    }).await?;
    Ok(())
}
