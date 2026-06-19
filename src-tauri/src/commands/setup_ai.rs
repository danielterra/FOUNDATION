use tauri::State;

use crate::owl::{self, DbExecutor, Individual, Object};

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__list_ai_services(
    limit: Option<i64>,
    offset: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor.read(move |conn| {
        // bounded-por-natureza; keyset-tx não agrega (não é append-no-topo)
        let page_size = limit.unwrap_or(100).max(1).min(500);
        let page_offset = offset.unwrap_or(0).max(0);
        let service_iris = owl::find_entities_with_property_bounded(
            conn, "rdf:type", "foundation:AIAPIService", page_size, page_offset, Some("rdfs:label"),
        ).map_err(|e| format!("Failed to query services: {}", e))?;

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
    }).await
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
    executor.read(move |conn| {
        let page_size = limit.unwrap_or(100).max(1).min(500);
        let page_offset = offset.unwrap_or(0).max(0);

        let pinned_tx: i64 = match snapshot_tx {
            Some(tx) => tx,
            None => conn.query_row(
                "SELECT COALESCE(MAX(tx), 0) FROM triples",
                [],
                |row| row.get(0),
            ).map_err(|e| e.to_string())?,
        };

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
        ).map_err(|e| format!("Failed to query models: {}", e))?;

        let items: Vec<serde_json::Value> = rows.into_iter().map(|row| {
            serde_json::json!({
                "iri": row.subject,
                "label": row.label,
                "description": row.description,
                "modelIdentifier": row.model_identifier,
                "modelVersion": row.model_version,
                "isDefault": row.is_default,
            })
        }).collect();

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
            !i.properties.iter().any(|(k, _)| k == "foundation:apiBaseUrl")
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
            let is_local = !service_ind.properties.iter()
                .any(|(k, _)| k == "foundation:apiBaseUrl");
            Ok(Some(serde_json::json!({
                "iri": service_iri,
                "label": service_ind.label,
                "isLocal": is_local,
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
