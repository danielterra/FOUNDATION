use serde_json::Value;
use rusqlite::Connection;
use tauri::Emitter;
use crate::owl::Class;
use crate::eavto::query;
use super::ToolResult;

pub fn search_concepts(conn: &Connection, args: &Value) -> ToolResult {
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
        let concepts_result = query::get_by_predicate_object(conn, "rdf:type", "owl:Class")?;
        let rdfs_concepts_result = query::get_by_predicate_object(conn, "rdf:type", "rdfs:Class")?;

        let mut all_concept_iris: Vec<String> = concepts_result.triples.into_iter()
            .chain(rdfs_concepts_result.triples)
            .map(|t| t.subject)
            .collect();

        all_concept_iris.sort();
        all_concept_iris.dedup();

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
                            match_count += 3;
                        } else if comment_lower.contains(token) {
                            match_count += 2;
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
            use crate::eavto::query;
            concept.one_of_values.iter().map(|value_iri| {
                let label_result = query::get_by_entity_predicate(
                    conn, value_iri, "rdfs:label",
                ).ok();
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
            use crate::eavto::{store, Triple, Object};
            let comment_triple = Triple::new(iri, "rdfs:comment", Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[comment_triple], "ai")?;
        }

        if let Some(app_handle) = app {
            app_handle.emit("entity-created", serde_json::json!({"entityId": iri})).ok();
        }

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
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple, Object};
        use crate::owl::vocabulary::rdfs;

        let mut updated_fields = Vec::new();

        if let Some(label) = args.get("label").and_then(|v| v.as_str()) {
            let old_labels = query::get_by_entity_predicate(conn, iri, rdfs::LABEL)?;
            for triple in old_labels.triples {
                let t = Triple::new(iri, rdfs::LABEL, triple.object);
                store::retract_triples(conn, &[t], "ai")?;
            }
            let new_label = Triple::new(iri, rdfs::LABEL, Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_label], "ai")?;
            updated_fields.push("label");
        }

        if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
            let old_icons = query::get_by_entity_predicate(conn, iri, "foundation:icon")?;
            for triple in old_icons.triples {
                let t = Triple::new(iri, "foundation:icon", triple.object);
                store::retract_triples(conn, &[t], "ai")?;
            }
            let new_icon = Triple::new(iri, "foundation:icon", Object::Literal {
                value: icon.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_icon], "ai")?;
            updated_fields.push("icon");
        }

        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            let old_comments = query::get_by_entity_predicate(conn, iri, rdfs::COMMENT)?;
            for triple in old_comments.triples {
                let t = Triple::new(iri, rdfs::COMMENT, triple.object);
                store::retract_triples(conn, &[t], "ai")?;
            }
            let new_comment = Triple::new(iri, rdfs::COMMENT, Object::Literal {
                value: comment.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_comment], "ai")?;
            updated_fields.push("comment");
        }

        if let Some(super_concept) = args.get("super_concept").and_then(|v| v.as_str()) {
            let old_supers = query::get_by_entity_predicate(conn, iri, rdfs::SUB_CLASS_OF)?;
            for triple in old_supers.triples {
                let t = Triple::new(iri, rdfs::SUB_CLASS_OF, triple.object);
                store::retract_triples(conn, &[t], "ai")?;
            }
            let new_super = Triple::new(
                iri, rdfs::SUB_CLASS_OF, Object::Iri(super_concept.to_string()),
            );
            store::assert_triples(conn, &[new_super], "ai")?;
            updated_fields.push("superConcept");
        }

        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
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
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple};

        let triples_result = query::get_by_entity(conn, iri)?;

        let triples_to_retract: Vec<Triple> = triples_result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();

        store::retract_triples(conn, &triples_to_retract, "ai")?;

        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
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

#[allow(dead_code)]
pub fn get_concept_hierarchy(conn: &Connection, args: &Value) -> ToolResult {
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
        use crate::owl::vocabulary::rdfs;

        fn get_hierarchy_recursive(
            conn: &Connection,
            concept_iri: &str,
            depth: usize,
            max_depth: usize,
            visited: &mut std::collections::HashSet<String>,
        ) -> Result<Value, crate::owl::OwlError> {
            if depth >= max_depth || visited.contains(concept_iri) {
                return Ok(serde_json::json!({
                    "iri": concept_iri,
                    "label": Class::get(conn, concept_iri).ok().flatten().and_then(|c| c.label),
                    "subClasses": [],
                    "truncated": depth >= max_depth,
                }));
            }

            visited.insert(concept_iri.to_string());

            let concept = Class::get(conn, concept_iri)?
                .ok_or_else(|| crate::owl::OwlError::NotFound(concept_iri.to_string()))?;
            let mut sub_concepts = Vec::new();

            let sub_result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, concept_iri)?;
            for triple in sub_result.triples {
                let sub_hierarchy = get_hierarchy_recursive(
                    conn, &triple.subject, depth + 1, max_depth, visited,
                )?;
                sub_concepts.push(sub_hierarchy);
            }

            Ok(serde_json::json!({
                "iri": concept.iri,
                "label": concept.label,
                "icon": concept.icon,
                "subClasses": sub_concepts,
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
