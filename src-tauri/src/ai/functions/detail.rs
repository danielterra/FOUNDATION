use serde_json::Value;
use crate::eavto::Connection;
use tauri::Emitter;
use super::ToolResult;

pub fn create_detail(
    conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, create_detail_one)
}

fn create_detail_one(
    conn: &mut Connection, args: &Value, _app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    let label = match args.get("label").and_then(|v| v.as_str()) {
        Some(label) => label,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: label".to_string()),
        },
    };

    let detail_type_str = match args.get("detail_type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: detail_type".to_string()),
        },
    };

    let detail_type = match detail_type_str {
        "object" => crate::owl::PropertyType::ObjectProperty,
        "datatype" => crate::owl::PropertyType::DatatypeProperty,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Invalid detail_type. Must be 'object' or 'datatype'".to_string()),
        },
    };

    let comment = args.get("comment").and_then(|v| v.as_str());
    let domain_strings: Vec<String> = match args.get("domain") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
        _ => vec![],
    };
    let domains: Vec<&str> = domain_strings.iter().map(|s| s.as_str()).collect();
    let range = args.get("range").and_then(|v| v.as_str());
    let unit = args.get("unit").and_then(|v| v.as_str());

    match (|| {
        use crate::owl::Property;

        let detail = Property::new(iri);
        detail.assert(conn, detail_type, label, comment, &domains, range, unit, "ai")?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": iri,
            "message": format!("Detail {} created successfully", label),
        }))
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn get_detail(conn: &Connection, args: &Value) -> ToolResult {
    super::batch::run_multi_read(conn, args, get_detail_one)
}

fn get_detail_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::owl::{Property, Class, Individual};

        let detail = Property::get(conn, iri)?
            .ok_or_else(|| crate::owl::OwlError::NotFound(iri.to_string()))?;

        let mut allowed_values: Vec<serde_json::Value> = Vec::new();
        for range_iri in &detail.ranges {
            if let Ok(Some(range_concept)) = Class::get(conn, range_iri) {
                if !range_concept.one_of_values.is_empty() {
                    for value_iri in &range_concept.one_of_values {
                        let label = Individual::get(conn, value_iri)
                            .ok()
                            .flatten()
                            .and_then(|ind| ind.label)
                            .unwrap_or_else(|| value_iri.clone());

                        allowed_values.push(serde_json::json!({
                            "iri": value_iri,
                            "label": label,
                        }));
                    }
                }
            }
        }

        let mut response = serde_json::json!({
            "iri": detail.iri,
            "label": detail.label,
            "comment": detail.comment,
            "type": format!("{:?}", detail.property_type),
            "domains": detail.domains,
            "ranges": detail.ranges,
            "superProperties": detail.super_properties,
            "isFunctional": detail.is_functional,
            "isTransitive": detail.is_transitive,
            "isSymmetric": detail.is_symmetric,
            "inverseOf": detail.inverse_of,
            "unit": detail.unit,
        });

        if !allowed_values.is_empty() {
            response["allowedValues"] = serde_json::json!(allowed_values);
        }

        Ok::<_, crate::owl::OwlError>(response)
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn delete_detail(
    conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, delete_detail_one)
}

fn delete_detail_one(
    conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::owl::Property;

        let affected_entities = Property::retract(conn, iri, "ai")?;
        let affected_count = affected_entities.len();

        if let Some(app_handle) = app {
            for entity_id in &affected_entities {
                app_handle.emit("entity-updated", serde_json::json!({"entityId": entity_id})).ok();
            }
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "message": format!("Detail {} deleted successfully", iri),
            "affectedEntities": affected_count,
        }))
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn forget_detail_value(
    conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, forget_detail_value_one)
}

fn forget_detail_value_one(
    conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let thing_iri = match args.get("thing_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: thing_iri".to_string()),
        },
    };

    let detail_iri = match args.get("detail_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: detail_iri".to_string()),
        },
    };

    let value = match args.get("value").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: value".to_string()),
        },
    };

    match (|| {
        use crate::owl::{Individual, Object};
        use crate::eavto::query;

        // Validate removal won't violate minCardinality
        let current_count = query::get_by_entity_predicate(conn, thing_iri, detail_iri)
            .map(|r| r.triples.len())
            .unwrap_or(0);
        crate::owl::cardinality::validate_property_cardinality(
            conn, thing_iri, detail_iri, current_count.saturating_sub(1),
        )?;

        match Individual::remove_property_value(conn, thing_iri, detail_iri, value, "ai")? {
            Some(removed) => {
                if let Some(app_handle) = app {
                    app_handle.emit(
                        "entity-updated", serde_json::json!({"entityId": thing_iri}),
                    ).ok();
                    if let Object::Iri(iri) = removed {
                        app_handle.emit(
                            "entity-updated", serde_json::json!({"entityId": iri}),
                        ).ok();
                    }
                }
                Ok::<_, crate::owl::OwlError>(serde_json::json!({
                    "success": true,
                    "message": format!("Detail value removed from {}", thing_iri),
                }))
            }
            None => Ok(serde_json::json!({
                "success": false,
                "message": "Detail value not found",
            }))
        }
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}
