use serde_json::Value;
use rusqlite::Connection;
use tauri::Emitter;
use crate::owl::Class;
use crate::eavto::query;
use super::FunctionResult;

pub fn search_classes(conn: &Connection, args: &Value) -> FunctionResult {
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
        // Get all classes (entities that are rdf:type of owl:Class or rdfs:Class)
        let classes_result = query::get_by_predicate_object(conn, "rdf:type", "owl:Class")?;
        let rdfs_classes_result = query::get_by_predicate_object(conn, "rdf:type", "rdfs:Class")?;

        let mut all_class_iris: Vec<String> = classes_result.triples.into_iter()
            .chain(rdfs_classes_result.triples)
            .map(|t| t.subject)
            .collect();

        all_class_iris.sort();
        all_class_iris.dedup();

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

        // Get full class info for each and calculate relevance scores
        let mut classes_with_scores: Vec<(Value, usize)> = Vec::new();
        for iri in all_class_iris {
            if let Ok(class) = Class::get(conn, &iri) {
                // Calculate relevance score if query provided
                let score = if !search_tokens.is_empty() {
                    let label_lower = class.label.as_ref().map(|l| l.to_lowercase()).unwrap_or_default();
                    let comment_lower = class.comment.as_ref().map(|c| c.to_lowercase()).unwrap_or_default();

                    // Count how many tokens match (OR logic with scoring)
                    let mut match_count = 0;
                    for token in &search_tokens {
                        if label_lower.contains(token) {
                            match_count += 3; // Label matches are more important
                        } else if comment_lower.contains(token) {
                            match_count += 2; // Comment matches are medium importance
                        }
                    }

                    // Only include if at least one token matches
                    if match_count == 0 {
                        continue;
                    }

                    match_count
                } else {
                    0 // No query, all classes have equal score
                };

                classes_with_scores.push((serde_json::json!({
                    "iri": class.iri,
                    "label": class.label,
                    "icon": class.icon,
                    "superClasses": class.super_classes.iter().map(|t| t.iri.clone()).collect::<Vec<_>>(),
                    "subClasses": class.sub_classes.iter().map(|t| t.iri.clone()).collect::<Vec<_>>(),
                }), score));
            }
        }

        // Sort by relevance score (highest first)
        if !search_tokens.is_empty() {
            classes_with_scores.sort_by(|a, b| b.1.cmp(&a.1));
        }

        let classes: Vec<_> = classes_with_scores.into_iter().map(|(cls, _)| cls).collect();

        // Apply pagination
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

pub fn get_class(conn: &Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        let class = Class::get(conn, iri)?;

        // Get labels for owl:oneOf enumerated values
        let allowed_values: Vec<serde_json::Value> = if !class.one_of_values.is_empty() {
            use crate::eavto::query;
            class.one_of_values.iter().map(|value_iri| {
                let label_result = query::get_by_entity_predicate(conn, value_iri, "rdfs:label").ok();
                let label = label_result
                    .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
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

        // Add allowedValues only if the class has owl:oneOf constraint
        if !allowed_values.is_empty() {
            response["allowedValues"] = serde_json::json!(allowed_values);
        }

        Ok::<_, crate::owl::OwlError>(response)
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

pub fn create_class(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
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
    let super_class = args.get("super_class").and_then(|v| v.as_str());

    match (|| {
        let class = Class::new(iri);
        class.assert(conn, crate::owl::ClassType::OwlClass, label, icon, super_class, "ai")?;

        // Add comment if provided
        if let Some(comment_text) = comment {
            use crate::eavto::{store, Triple, Object};
            let comment_triple = Triple::new(iri, "rdfs:comment", Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[comment_triple], "ai")?;
        }

        // Emit entity-created event (which will auto-open an inspector)
        if let Some(app_handle) = app {
            app_handle.emit("entity-created", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": iri,
            "message": format!("Class {} created successfully", label),
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

pub fn update_class(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
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
            // Retract old label
            let old_labels = query::get_by_entity_predicate(conn, iri, rdfs::LABEL)?;
            for triple in old_labels.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::LABEL, triple.object)], "ai")?;
            }
            // Assert new label
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

        // Update super class
        if let Some(super_class) = args.get("super_class").and_then(|v| v.as_str()) {
            let old_supers = query::get_by_entity_predicate(conn, iri, rdfs::SUB_CLASS_OF)?;
            for triple in old_supers.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::SUB_CLASS_OF, triple.object)], "ai")?;
            }
            let new_super = Triple::new(iri, rdfs::SUB_CLASS_OF, Object::Iri(super_class.to_string()));
            store::assert_triples(conn, &[new_super], "ai")?;
            updated_fields.push("superClass");
        }

        // Emit entity-updated event
        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Class {} updated successfully", iri),
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

pub fn delete_class(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
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
            "message": format!("Class {} deleted successfully", iri),
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

#[allow(dead_code)]
pub fn get_class_hierarchy(conn: &Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    match (|| {
        use crate::owl::vocabulary::rdfs;

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
                    "label": Class::get(conn, class_iri).ok().and_then(|c| c.label),
                    "subClasses": [],
                    "truncated": depth >= max_depth,
                }));
            }

            visited.insert(class_iri.to_string());

            let class = Class::get(conn, class_iri)?;
            let mut sub_classes = Vec::new();

            // Get direct subclasses
            let sub_result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, class_iri)?;
            for triple in sub_result.triples {
                let sub_hierarchy = get_hierarchy_recursive(conn, &triple.subject, depth + 1, max_depth, visited)?;
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
