use serde_json::Value;
use rusqlite::Connection;
use tauri::Emitter;
use crate::owl::{Class, Individual};
use super::FunctionResult;

pub fn search_instances(conn: &Connection, args: &Value) -> FunctionResult {
    let class_iri_opt = args.get("concept_iri")
        .or_else(|| args.get("class_iri"))
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
        // Get instances: either from specific class or all instances
        let instance_iris = if let Some(class_iri) = class_iri_opt {
            Class::get_instances(conn, class_iri)?
        } else {
            // Get all instances across all classes
            use crate::eavto::query;
            let result = query::get_by_predicate(conn, "rdf:type")?;
            result.triples.iter()
                .filter_map(|t| {
                    // Only include instances (not classes themselves)
                    if let Some(class_iri) = t.object.as_iri() {
                        if !class_iri.starts_with("owl:") &&
                           !class_iri.starts_with("rdfs:") &&
                           !class_iri.starts_with("rdf:") &&
                           class_iri != "owl:Class" {
                            Some(t.subject.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };

        // Parse query into search tokens (words)
        let search_tokens: Vec<String> = if query_str.is_empty() {
            Vec::new()
        } else {
            query_str
                .to_lowercase()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        };

        let mut instances_with_scores: Vec<(Value, usize)> = Vec::new();
        for iri in instance_iris {
            if let Ok(individual) = Individual::get(conn, &iri) {
                // Calculate relevance score if query provided
                let score = if !search_tokens.is_empty() {
                    let label_lower = individual.label.as_ref().map(|l| l.to_lowercase()).unwrap_or_default();
                    let comment_lower = individual.comment.as_ref().map(|c| c.to_lowercase()).unwrap_or_default();

                    // Get all property values for deeper search
                    let mut property_text = String::new();
                    for (_, value) in &individual.properties {
                        if let Some(val_str) = value.as_literal() {
                            property_text.push_str(&val_str.to_lowercase());
                            property_text.push(' ');
                        }
                    }

                    // Count how many tokens match (OR logic with scoring)
                    let mut match_count = 0;
                    for token in &search_tokens {
                        if label_lower.contains(token) {
                            match_count += 3; // Label matches are more important
                        } else if comment_lower.contains(token) {
                            match_count += 2; // Comment matches are medium importance
                        } else if property_text.contains(token) {
                            match_count += 1; // Property matches are least important
                        }
                    }

                    // Only include if at least one token matches
                    if match_count == 0 {
                        continue;
                    }

                    match_count
                } else {
                    0 // No query, all instances have equal score
                };

                instances_with_scores.push((serde_json::json!({
                    "iri": individual.iri,
                    "label": individual.label,
                    "icon": individual.icon,
                }), score));
            }
        }

        // Sort by relevance score (highest first)
        if !search_tokens.is_empty() {
            instances_with_scores.sort_by(|a, b| b.1.cmp(&a.1));
        }

        let instances: Vec<_> = instances_with_scores.into_iter().map(|(inst, _)| inst).collect();

        // Apply pagination
        let total = instances.len();
        let paginated: Vec<_> = instances.into_iter().skip(offset).take(limit).collect();

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "instances": paginated,
            "count": paginated.len(),
            "total": total,
            "limit": limit,
            "offset": offset,
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn get_instance(conn: &Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        let individual = Individual::get(conn, iri)?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": individual.iri,
            "label": individual.label,
            "icon": individual.icon,
            "comment": individual.comment,
            "types": individual.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": individual.properties.iter().map(|(prop, value)| serde_json::json!({
                "property": prop,
                "value": value.as_literal().map(|s| s.to_string()).or_else(|| value.as_iri().map(|s| s.to_string())).unwrap_or_default(),
            })).collect::<Vec<_>>(),
            "backlinksCount": individual.backlinks.len(),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn create_instance(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
    let class_iri = match args.get("concept_iri").or_else(|| args.get("class_iri")).and_then(|v| v.as_str()) {
        Some(class_iri) => class_iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: concept_iri".to_string()),
        },
    };

    let label = match args.get("label").and_then(|v| v.as_str()) {
        Some(label) => label,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: label".to_string()),
        },
    };

    let icon = match args.get("icon").and_then(|v| v.as_str()) {
        Some(icon) => icon,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: icon".to_string()),
        },
    };

    let comment = args.get("comment").and_then(|v| v.as_str());

    // Extract class name from IRI for generating unique instance IRI
    let class_name = class_iri.split(':').last().unwrap_or("Instance");
    let timestamp = chrono::Utc::now().timestamp_millis();
    let generated_iri = format!("foundation:{}_{}", class_name, timestamp);

    match (|| {
        let individual = Individual::new(&generated_iri);
        individual.assert(conn, class_iri, label, icon, "ai")?;

        // Add comment if provided
        if let Some(comment_text) = comment {
            use crate::eavto::{store, Triple, Object};
            let comment_triple = Triple::new(&generated_iri, "rdfs:comment", Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[comment_triple], "ai")?;
        }

        // Emit entity-created event (which will auto-open an inspector)
        if let Some(app_handle) = app {
            app_handle.emit("entity-created", serde_json::json!({"entityId": generated_iri.clone()})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": generated_iri,
            "message": format!("Instance {} created successfully with IRI: {}", label, generated_iri),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn update_instance(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple, Object};
        use crate::owl::vocabulary::rdfs;

        let mut updated_fields = Vec::new();

        // Update label
        if let Some(label) = args.get("label").and_then(|v| v.as_str()) {
            let old_labels = query::get_by_entity_predicate(conn, iri, rdfs::LABEL)?;
            for triple in old_labels.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::LABEL, triple.object)], "ai")?;
            }
            let new_label = Triple::new(iri, rdfs::LABEL, Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_label], "ai")?;
            updated_fields.push("label");
        }

        // Update icon
        if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
            let old_icons = query::get_by_entity_predicate(conn, iri, "foundation:icon")?;
            for triple in old_icons.triples {
                store::retract_triples(conn, &[Triple::new(iri, "foundation:icon", triple.object)], "ai")?;
            }
            let new_icon = Triple::new(iri, "foundation:icon", Object::Literal {
                value: icon.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_icon], "ai")?;
            updated_fields.push("icon");
        }

        // Update comment
        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            let old_comments = query::get_by_entity_predicate(conn, iri, rdfs::COMMENT)?;
            for triple in old_comments.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::COMMENT, triple.object)], "ai")?;
            }
            let new_comment = Triple::new(iri, rdfs::COMMENT, Object::Literal {
                value: comment.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_comment], "ai")?;
            updated_fields.push("comment");
        }

        // Emit entity-updated event
        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Instance {} updated successfully", iri),
            "updatedFields": updated_fields,
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn delete_instance(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple};

        // Get all triples where this IRI is the subject
        let triples_result = query::get_by_entity(conn, iri)?;

        // Retract all triples
        let triples_to_retract: Vec<Triple> = triples_result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();

        store::retract_triples(conn, &triples_to_retract, "ai")?;

        // Emit entity-updated event
        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Instance {} deleted successfully", iri),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn find_instances_by_property(conn: &Connection, args: &Value) -> FunctionResult {
    let class_iri = match args.get("concept_iri").or_else(|| args.get("class_iri")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: concept_iri".to_string()),
        },
    };

    let properties = match args.get("properties").and_then(|v| v.as_array()) {
        Some(props) => props,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: properties".to_string()),
        },
    };

    match (|| {
        // Convert properties array to tuple format
        let mut property_constraints = Vec::new();
        for prop in properties {
            let property_iri = prop.get("property")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::owl::OwlError::ValidationError("Missing 'property' field in constraint".to_string()))?;
            let value = prop.get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::owl::OwlError::ValidationError("Missing 'value' field in constraint".to_string()))?;

            property_constraints.push((property_iri, value));
        }

        // Use the OWL Individual method
        let instances = Individual::find_by_class_and_properties(conn, class_iri, &property_constraints)?;

        // Get full info for each instance
        let mut results = Vec::new();
        for iri in instances {
            if let Ok(individual) = Individual::get(conn, &iri) {
                results.push(serde_json::json!({
                    "iri": individual.iri,
                    "label": individual.label,
                    "icon": individual.icon,
                }));
            }
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "instances": results,
            "count": results.len(),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}
