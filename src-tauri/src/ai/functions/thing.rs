use serde_json::Value;
use crate::eavto::Connection;
use tauri::Emitter;
use crate::owl::{Class, Individual, Object};
use super::ToolResult;

pub fn search_things(conn: &Connection, args: &Value) -> ToolResult {
    let concept_iri_opt = args.get("concept_iri")
        .and_then(|v| v.as_str());

    let query_str = args.get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let limit = args.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;

    let offset = args.get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    match (|| {
        let thing_iris = if let Some(concept_iri) = concept_iri_opt {
            Class::get_instances(conn, concept_iri)?
        } else {
            Individual::search(conn)?
        };

        let search_tokens: Vec<String> = if query_str.is_empty() {
            Vec::new()
        } else {
            query_str
                .to_lowercase()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        };

        let mut things_with_scores: Vec<(Value, usize)> = Vec::new();
        for iri in thing_iris {
            if let Ok(Some(individual)) = Individual::get(conn, &iri) {
                let score = if !search_tokens.is_empty() {
                    let label_lower = individual.label.as_ref()
                        .map(|l| l.to_lowercase())
                        .unwrap_or_default();
                    let comment_lower = individual.comment.as_ref()
                        .map(|c| c.to_lowercase())
                        .unwrap_or_default();

                    let mut detail_text = String::new();
                    for (_, value) in &individual.properties {
                        if let Some(val_str) = value.as_literal() {
                            detail_text.push_str(&val_str.to_lowercase());
                            detail_text.push(' ');
                        }
                    }

                    let mut match_count = 0;
                    for token in &search_tokens {
                        if label_lower.contains(token) {
                            match_count += 3;
                        } else if comment_lower.contains(token) {
                            match_count += 2;
                        } else if detail_text.contains(token) {
                            match_count += 1;
                        }
                    }

                    if match_count == 0 {
                        continue;
                    }

                    match_count
                } else {
                    0 // No query, all things have equal score
                };

                things_with_scores.push((serde_json::json!({
                    "iri": individual.iri,
                    "label": individual.label,
                    "icon": individual.icon,
                }), score));
            }
        }

        if !search_tokens.is_empty() {
            things_with_scores.sort_by(|a, b| b.1.cmp(&a.1));
        }

        let things: Vec<_> = things_with_scores.into_iter().map(|(t, _)| t).collect();

        let total = things.len();
        let paginated: Vec<_> = things.into_iter().skip(offset).take(limit).collect();

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "things": paginated,
            "count": paginated.len(),
            "total": total,
            "limit": limit,
            "offset": offset,
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

pub fn get_thing(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        let individual = Individual::get(conn, iri)?
            .ok_or_else(|| crate::owl::OwlError::NotFound(iri.to_string()))?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": individual.iri,
            "label": individual.label,
            "icon": individual.icon,
            "comment": individual.comment,
            "types": individual.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": individual.serializable_properties(conn),
            "backlinksCount": individual.backlinks.len(),
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

pub fn create_thing(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let concept_iri = match args.get("concept_iri").and_then(|v| v.as_str()) {
        Some(concept_iri) => concept_iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: concept_iri".to_string()),
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

    let icon = match args.get("icon").and_then(|v| v.as_str()) {
        Some(icon) => icon,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: icon".to_string()),
        },
    };

    let comment = args.get("comment").and_then(|v| v.as_str());

    let concept_name = concept_iri.split(':').last().unwrap_or("Thing");
    let timestamp = chrono::Utc::now().timestamp_millis();
    let generated_iri = format!("foundation:{}_{}", concept_name, timestamp);

    match (|| {
        let individual = Individual::new(&generated_iri);
        individual.assert(conn, concept_iri, label, icon, "ai")?;

        if let Some(comment_text) = comment {
            individual.add_property(conn, "rdfs:comment", vec![Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai")?;
        }

        if let Some(app_handle) = app {
            app_handle.emit(
                "entity-created", serde_json::json!({"entityId": generated_iri.clone()}),
            ).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": generated_iri,
            "message": format!("Thing {} created successfully with IRI: {}", label, generated_iri),
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

pub fn update_thing(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
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
        let mut updated_fields = Vec::new();

        let individual = Individual::new(iri);

        if let Some(label) = args.get("label").and_then(|v| v.as_str()) {
            individual.add_property(conn, "rdfs:label", vec![Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai")?;
            updated_fields.push("label");
        }

        if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
            individual.add_property(conn, "foundation:icon", vec![Object::Literal {
                value: icon.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai")?;
            updated_fields.push("icon");
        }

        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            individual.add_property(conn, "rdfs:comment", vec![Object::Literal {
                value: comment.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai")?;
            updated_fields.push("comment");
        }

        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "message": format!("Thing {} updated successfully", iri),
            "updatedFields": updated_fields,
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

pub fn delete_thing(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
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
        Individual::retract(conn, iri, "ai")?;

        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "message": format!("Thing {} deleted successfully", iri),
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

pub fn find_things_by_detail(conn: &Connection, args: &Value) -> ToolResult {
    let concept_iri = match args.get("concept_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: concept_iri".to_string()),
        },
    };

    let properties = match args.get("properties").and_then(|v| v.as_array()) {
        Some(props) => props,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: properties".to_string()),
        },
    };

    match (|| {
        let mut detail_constraints = Vec::new();
        for prop in properties {
            let detail_iri = prop.get("detail")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::owl::OwlError::ValidationError(
                    "Missing 'detail' field in constraint".to_string(),
                ))?;
            let value = prop.get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::owl::OwlError::ValidationError(
                    "Missing 'value' field in constraint".to_string(),
                ))?;

            detail_constraints.push((detail_iri, value));
        }

        let things = Individual::find_by_class_and_properties(
            conn, concept_iri, &detail_constraints,
        )?;

        let mut results = Vec::new();
        for iri in things {
            if let Ok(Some(individual)) = Individual::get(conn, &iri) {
                results.push(serde_json::json!({
                    "iri": individual.iri,
                    "label": individual.label,
                    "icon": individual.icon,
                }));
            }
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "things": results,
            "count": results.len(),
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
