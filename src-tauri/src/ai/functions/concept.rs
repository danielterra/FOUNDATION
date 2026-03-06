use serde_json::Value;
use rusqlite::Connection;
use crate::owl::Class;
use super::ToolResult;

#[cfg(test)]
mod tests {
    use super::update_concept_one;
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    #[test]
    fn test_update_concept_required_fields_rejects_nonexistent_property() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        ], "test").unwrap();

        let args = serde_json::json!({
            "iri": "foundation:TestClass",
            "required_fields": ["foundation:nonExistent"]
        });

        let result = update_concept_one(&mut conn, &args);

        assert!(!result.success);
        let error = result.error.unwrap();
        assert!(
            error.contains("foundation:nonExistent") && error.contains("not defined in this ontology"),
            "Expected error about undefined property, got: {}",
            error
        );
    }

    #[test]
    fn test_update_concept_required_fields_accepts_valid_datatype_property() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:myProp", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
        ], "test").unwrap();

        let args = serde_json::json!({
            "iri": "foundation:TestClass",
            "required_fields": ["foundation:myProp"]
        });

        let result = update_concept_one(&mut conn, &args);
        assert!(result.success, "Expected success, got error: {:?}", result.error);
    }

    #[test]
    fn test_update_concept_required_fields_accepts_valid_object_property() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:myRef", "rdf:type", Object::Iri("owl:ObjectProperty".to_string())),
        ], "test").unwrap();

        let args = serde_json::json!({
            "iri": "foundation:TestClass",
            "required_fields": ["foundation:myRef"]
        });

        let result = update_concept_one(&mut conn, &args);
        assert!(result.success, "Expected success, got error: {:?}", result.error);
    }
}

const SCORE_LABEL_MATCH: usize = 3;
const SCORE_COMMENT_MATCH: usize = 2;

pub fn search_concepts(conn: &Connection, args: &Value) -> ToolResult {
    super::batch::run_multi_read(conn, args, search_concepts_one)
}

fn search_concepts_one(conn: &Connection, args: &Value) -> ToolResult {
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
        let all_concept_iris = Class::find_all_iris(conn)?;

        let search_tokens: Vec<String> = if query_str.is_empty() {
            Vec::new()
        } else {
            query_str
                .to_lowercase()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        };

        let mut concepts_with_scores: Vec<(Value, usize)> = Vec::new();
        for iri in all_concept_iris {
            if let Ok(Some(concept)) = Class::get(conn, &iri) {
                let score = if !search_tokens.is_empty() {
                    let label_lower = concept.label.as_ref()
                        .map(|l| l.to_lowercase())
                        .unwrap_or_default();
                    let comment_lower = concept.comment.as_ref()
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

                let super_classes: Vec<_> = concept.super_classes.iter()
                    .map(|t| t.iri.clone())
                    .collect();
                let sub_classes: Vec<_> = concept.sub_classes.iter()
                    .map(|t| t.iri.clone())
                    .collect();
                concepts_with_scores.push((serde_json::json!({
                    "iri": concept.iri,
                    "label": concept.label,
                    "icon": concept.icon,
                    "comment": concept.comment,
                    "superClasses": super_classes,
                    "subClasses": sub_classes,
                }), score));
            }
        }

        if !search_tokens.is_empty() {
            concepts_with_scores.sort_by(|a, b| b.1.cmp(&a.1));
        }

        let concepts: Vec<_> = concepts_with_scores.into_iter().map(|(c, _)| c).collect();

        let total = concepts.len();
        let paginated: Vec<_> = concepts.into_iter().skip(offset).take(limit).collect();

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "concepts": paginated,
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

pub fn get_concept(conn: &Connection, args: &Value) -> ToolResult {
    super::batch::run_multi_read(conn, args, get_concept_one)
}

fn get_concept_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        let concept = Class::get(conn, iri)?
            .ok_or_else(|| crate::owl::OwlError::NotFound(iri.to_string()))?;

        let allowed_values: Vec<serde_json::Value> = if !concept.one_of_values.is_empty() {
            concept.one_of_values.iter().map(|value_iri| {
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

        let allowed_statuses: Vec<serde_json::Value> = {
            let status_iris = crate::owl::get_all_iri_properties(conn, iri, "foundation:allowedStatus")?;
            status_iris.iter()
                .map(|status_iri| {
                    let thing = crate::owl::Thing::get(conn, status_iri);
                    let (icon, color) = crate::owl::resolve_status_appearance(conn, status_iri);
                    serde_json::json!({
                        "iri": status_iri,
                        "label": thing.label,
                        "icon": icon,
                        "color": color,
                    })
                })
                .collect()
        };

        let required_fields: Vec<serde_json::Value> = {
            let restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, iri)?;
            restrictions.into_iter()
                .filter(|r| r.is_required())
                .map(|r| {
                    let label = crate::owl::get_literal_property(conn, &r.property_iri, "rdfs:label")
                        .ok()
                        .flatten();
                    serde_json::json!({
                        "property": r.property_iri,
                        "label": label,
                    })
                })
                .collect()
        };

        let incoming_properties: Vec<serde_json::Value> = {
            use crate::eavto::query;
            let result = query::get_by_predicate_object(conn, "rdfs:range", iri)
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
            result.triples.iter()
                .map(|t| {
                    let prop_iri = &t.subject;
                    let label = crate::owl::get_literal_property(conn, prop_iri, "rdfs:label")
                        .ok()
                        .flatten();
                    let domain_iris = crate::owl::get_all_iri_properties(conn, prop_iri, "rdfs:domain")
                        .unwrap_or_default();
                    let domains: Vec<serde_json::Value> = domain_iris.iter()
                        .map(|d| {
                            let d_label = crate::owl::Thing::get(conn, d).label;
                            serde_json::json!({"iri": d, "label": d_label})
                        })
                        .collect();
                    serde_json::json!({
                        "property": prop_iri,
                        "label": label,
                        "domains": domains,
                    })
                })
                .collect()
        };

        let mut response = serde_json::json!({
            "iri": concept.iri,
            "label": concept.label,
            "icon": concept.icon,
            "comment": concept.comment,
            "types": concept.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "superClasses": concept.super_classes.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "subClasses": concept.sub_classes.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": concept.properties.iter().map(|(prop, source)| serde_json::json!({
                "property": prop,
                "source": source,
            })).collect::<Vec<_>>(),
            "instanceCount": concept.backlinks.len(),
            "allowedStatuses": allowed_statuses,
            "requiredFields": required_fields,
            "incomingProperties": incoming_properties,
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

pub fn create_concept(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, create_concept_one)
}

fn create_concept_one(
    conn: &mut Connection,
    args: &Value,
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
    let super_concept = args.get("super_concept").and_then(|v| v.as_str());

    match (|| {
        let concept = Class::new(iri);
        concept.assert(conn, crate::owl::ClassType::OwlClass, label, icon, super_concept, "ai")?;

        if let Some(comment_text) = comment {
            Class::set_comment(conn, iri, comment_text, "ai")?;
        }

        super::batch::queue_event("entity-created", serde_json::json!({"entityId": iri}));

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": iri,
            "message": format!("Concept {} created successfully", label),
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

pub fn update_concept(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, update_concept_one)
}

fn update_concept_one(
    conn: &mut Connection,
    args: &Value,
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

        if let Some(super_concept) = args.get("super_concept").and_then(|v| v.as_str()) {
            Class::set_super_class(conn, iri, super_concept, "ai")?;
            updated_fields.push("superConcept");
        }

        if let Some(allowed_statuses) = args.get("allowed_statuses").and_then(|v| v.as_array()) {
            let status_iris: Vec<&str> = allowed_statuses.iter()
                .filter_map(|v| v.as_str())
                .collect();
            crate::owl::replace_all_property_iris(
                conn, iri, "foundation:allowedStatus", &status_iris, "ai",
            )?;
            updated_fields.push("allowedStatuses");
        }

        if let Some(required_fields) = args.get("required_fields").and_then(|v| v.as_array()) {
            let prop_iris: Vec<&str> = required_fields.iter()
                .filter_map(|v| v.as_str())
                .collect();

            for prop_iri in &prop_iris {
                let prop = crate::owl::Property::get(conn, *prop_iri)?;
                let is_valid = prop.map(|p| matches!(
                    p.property_type,
                    crate::owl::PropertyType::ObjectProperty | crate::owl::PropertyType::DatatypeProperty
                )).unwrap_or(false);
                if !is_valid {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Property '{}' is not defined in this ontology",
                        prop_iri
                    )));
                }
            }

            crate::owl::cardinality::set_class_required_fields(conn, iri, &prop_iris, "ai")?;
            updated_fields.push("requiredFields");
        }

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "message": format!("Concept {} updated successfully", iri),
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

pub fn delete_concept(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, delete_concept_one)
}

fn delete_concept_one(
    conn: &mut Connection,
    args: &Value,
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

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "message": format!("Concept {} deleted successfully", iri),
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

