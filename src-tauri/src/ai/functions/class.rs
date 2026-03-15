use serde_json::Value;
use rusqlite::Connection;
use tauri::Emitter;
use crate::owl::Class;
use super::ToolResult;

const SCORE_LABEL_MATCH: usize = 3;
const SCORE_COMMENT_MATCH: usize = 2;

pub fn search_classes(conn: &Connection, args: &Value) -> ToolResult {
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
        let all_class_iris = Class::find_all_iris(conn)?;

        let search_tokens: Vec<String> = if query_str.is_empty() {
            Vec::new()
        } else {
            query_str
                .to_lowercase()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        };

        let mut classes_with_scores: Vec<(Value, usize)> = Vec::new();
        for iri in all_class_iris {
            if let Ok(Some(class)) = Class::get(conn, &iri) {
                let score = if !search_tokens.is_empty() {
                    let label_lower = class.label.as_ref()
                        .map(|l| l.to_lowercase())
                        .unwrap_or_default();
                    let comment_lower = class.comment.as_ref()
                        .map(|c| c.to_lowercase())
                        .unwrap_or_default();

                    let mut match_count = 0;
                    for token in &search_tokens {
                        if label_lower.contains(token) {
                            match_count += SCORE_LABEL_MATCH;
                        } else if comment_lower.contains(token) {
                            match_count += SCORE_COMMENT_MATCH;
                        }
                    }

                    if match_count == 0 {
                        continue;
                    }

                    match_count
                } else {
                    0
                };

                classes_with_scores.push((serde_json::json!({
                    "iri": class.iri,
                    "label": class.label,
                    "icon": class.icon,
                    "superClasses": class.super_classes.iter()
                        .map(|t| t.iri.clone()).collect::<Vec<_>>(),
                    "subClasses": class.sub_classes.iter()
                        .map(|t| t.iri.clone()).collect::<Vec<_>>(),
                }), score));
            }
        }

        if !search_tokens.is_empty() {
            classes_with_scores.sort_by(|a, b| b.1.cmp(&a.1));
        }

        let classes: Vec<_> = classes_with_scores.into_iter().map(|(cls, _)| cls).collect();

        let total = classes.len();
        let paginated: Vec<_> = classes.into_iter().skip(offset).take(limit).collect();

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "classes": paginated,
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

pub fn get_class(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        let class = Class::get(conn, iri)?
            .ok_or_else(|| crate::owl::OwlError::NotFound(iri.to_string()))?;

        let allowed_values: Vec<serde_json::Value> = if !class.one_of_values.is_empty() {
            class.one_of_values.iter().map(|value_iri| {
                let label = crate::owl::get_literal_property(conn, value_iri, "rdfs:label")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| value_iri.clone());
                serde_json::json!({
                    "iri": value_iri,
                    "label": label,
                })
            }).collect()
        } else {
            Vec::new()
        };

        let mut response = serde_json::json!({
            "iri": class.iri,
            "label": class.label,
            "icon": class.icon,
            "comment": class.comment,
            "types": class.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "superClasses": class.super_classes.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "subClasses": class.sub_classes.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": class.properties.iter().map(|(prop, source)| serde_json::json!({
                "property": prop,
                "source": source,
            })).collect::<Vec<_>>(),
            "instanceCount": class.backlinks.len(),
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

pub fn create_class(
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
    let super_class = args.get("super_class").and_then(|v| v.as_str());

    match (|| {
        let class = Class::new(iri);
        class.assert(conn, crate::owl::ClassType::OwlClass, label, icon, super_class, "ai")?;

        if let Some(comment_text) = comment {
            Class::set_comment(conn, iri, comment_text, "ai")?;
        }

        if let Some(app_handle) = app {
            app_handle.emit("entity-created", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": iri,
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

pub fn update_class(
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

        if let Some(label) = args.get("label").and_then(|v| v.as_str()) {
            Class::set_label(conn, iri, label, "ai")?;
            updated_fields.push("label");
        }

        if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
            Class::set_icon(conn, iri, icon, "ai")?;
            updated_fields.push("icon");
        }

        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            Class::set_comment(conn, iri, comment, "ai")?;
            updated_fields.push("comment");
        }

        if let Some(super_class) = args.get("super_class").and_then(|v| v.as_str()) {
            Class::set_super_class(conn, iri, super_class, "ai")?;
            updated_fields.push("superClass");
        }

        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
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

pub fn delete_class(
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
        Class::retract_all(conn, iri, "ai")?;

        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({}))
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

#[allow(dead_code)]
pub fn get_class_hierarchy(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    match (|| {
        fn get_hierarchy_recursive(
            conn: &Connection,
            class_iri: &str,
            depth: usize,
            max_depth: usize,
            visited: &mut std::collections::HashSet<String>,
        ) -> Result<Value, crate::owl::OwlError> {
            if depth >= max_depth || visited.contains(class_iri) {
                return Ok(serde_json::json!({
                    "iri": class_iri,
                    "label": Class::get(conn, class_iri).ok().flatten().and_then(|c| c.label),
                    "subClasses": [],
                    "truncated": depth >= max_depth,
                }));
            }

            visited.insert(class_iri.to_string());

            let class = Class::get(conn, class_iri)?
                .ok_or_else(|| crate::owl::OwlError::NotFound(class_iri.to_string()))?;
            let mut sub_classes = Vec::new();

            let sub_iris = Class::get_subclass_iris(conn, class_iri)?;
            for sub_iri in sub_iris {
                let sub_hierarchy = get_hierarchy_recursive(
                    conn, &sub_iri, depth + 1, max_depth, visited,
                )?;
                sub_classes.push(sub_hierarchy);
            }

            Ok(serde_json::json!({
                "iri": class.iri,
                "label": class.label,
                "icon": class.icon,
                "subClasses": sub_classes,
                "truncated": false,
            }))
        }

        let mut visited = std::collections::HashSet::new();
        let hierarchy = get_hierarchy_recursive(conn, iri, 0, max_depth, &mut visited)?;

        Ok::<_, crate::owl::OwlError>(hierarchy)
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
