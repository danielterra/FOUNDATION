use serde_json::Value;
use rusqlite::Connection;
use tauri::Emitter;
use crate::eavto::query;
use super::FunctionResult;

pub fn create_property(conn: &mut Connection, args: &Value) -> FunctionResult {
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

    let property_type_str = match args.get("property_type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_type".to_string()),
        },
    };

    let property_type = match property_type_str {
        "object" => crate::owl::PropertyType::ObjectProperty,
        "datatype" => crate::owl::PropertyType::DatatypeProperty,
        _ => return FunctionResult {
            success: false,
            result: None,
            error: Some("Invalid property_type. Must be 'object' or 'datatype'".to_string()),
        },
    };

    let comment = args.get("comment").and_then(|v| v.as_str());
    let domain = args.get("domain").and_then(|v| v.as_str());
    let range = args.get("range").and_then(|v| v.as_str());

    match (|| {
        use crate::owl::Property;

        let property = Property::new(iri);
        property.assert(conn, property_type, label, comment, domain, range, None, "ai")?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": iri,
            "message": format!("Property {} created successfully", label),
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
pub fn search_properties(conn: &Connection, args: &Value) -> FunctionResult {
    let query_str = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;

    match (|| {
        use crate::owl::{Property, vocabulary::{rdf, owl}};

        // Get all object properties
        let obj_props_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::OBJECT_PROPERTY)?;
        // Get all datatype properties
        let data_props_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::DATATYPE_PROPERTY)?;

        let mut all_property_iris: Vec<String> = obj_props_result.triples.into_iter()
            .chain(data_props_result.triples)
            .map(|t| t.subject)
            .collect();

        all_property_iris.sort();
        all_property_iris.dedup();

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

        let mut properties = Vec::new();
        for iri in all_property_iris {
            if let Ok(property) = Property::get(conn, &iri) {
                // Filter by query if provided
                if !search_tokens.is_empty() {
                    if let Some(label) = &property.label {
                        let label_lower = label.to_lowercase();
                        let comment_lower = property.comment.as_ref().map(|c| c.to_lowercase());

                        // Check if ALL search tokens appear in label or comment
                        let matches = search_tokens.iter().all(|token| {
                            label_lower.contains(token) ||
                            comment_lower.as_ref().map(|c| c.contains(token)).unwrap_or(false)
                        });

                        if !matches {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                properties.push(serde_json::json!({
                    "iri": property.iri,
                    "label": property.label,
                    "type": format!("{:?}", property.property_type),
                    "domains": property.domains,
                    "ranges": property.ranges,
                }));

                if properties.len() >= limit {
                    break;
                }
            }
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "properties": properties,
            "count": properties.len(),
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

pub fn get_property(conn: &Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::owl::{Property, Class};

        let property = Property::get(conn, iri)?;

        // Check if any of the ranges has owl:oneOf enumeration
        let mut allowed_values: Vec<serde_json::Value> = Vec::new();
        for range_iri in &property.ranges {
            if let Ok(range_class) = Class::get(conn, range_iri) {
                if !range_class.one_of_values.is_empty() {
                    // Get labels for each enumerated value
                    for value_iri in &range_class.one_of_values {
                        use crate::eavto::query;
                        let label_result = query::get_by_entity_predicate(conn, value_iri, "rdfs:label")?;
                        let label = label_result.triples.first()
                            .and_then(|t| t.object.as_literal())
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
            "iri": property.iri,
            "label": property.label,
            "comment": property.comment,
            "type": format!("{:?}", property.property_type),
            "domains": property.domains,
            "ranges": property.ranges,
            "superProperties": property.super_properties,
            "isFunctional": property.is_functional,
            "isTransitive": property.is_transitive,
            "isSymmetric": property.is_symmetric,
            "inverseOf": property.inverse_of,
            "unit": property.unit,
        });

        // Add allowedValues only if the range has owl:oneOf constraint
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

pub fn delete_property(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
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

        // IMPORTANT: First, retract all triples that use this property as predicate
        // This ensures that when a property is removed from a class, all facts using it are also removed
        let facts_using_property = query::get_by_predicate(conn, iri)?;
        let mut affected_entities = std::collections::HashSet::new();

        let facts_to_retract: Vec<Triple> = facts_using_property.triples.into_iter()
            .map(|t| {
                affected_entities.insert(t.subject.clone());
                Triple::new(t.subject, t.predicate, t.object)
            })
            .collect();

        if !facts_to_retract.is_empty() {
            store::retract_triples(conn, &facts_to_retract, "ai")?;
        }

        // Then, get all triples where this property IRI is the subject (property definition)
        let triples_result = query::get_by_entity(conn, iri)?;

        // Retract property definition triples
        let triples_to_retract: Vec<Triple> = triples_result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();

        store::retract_triples(conn, &triples_to_retract, "ai")?;

        // Emit entity-updated events for all affected entities
        let affected_count = affected_entities.len();
        if let Some(app_handle) = app {
            for entity_id in affected_entities {
                app_handle.emit("entity-updated", serde_json::json!({"entityId": entity_id})).ok();
            }
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
        }

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Property {} deleted successfully", iri),
            "affectedEntities": affected_count,
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

pub fn add_property_value(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
    let instance_iri = match args.get("instance_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: instance_iri".to_string()),
        },
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_iri".to_string()),
        },
    };

    let value = match args.get("value").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: value".to_string()),
        },
    };

    let value_type = args.get("value_type").and_then(|v| v.as_str()).unwrap_or("literal");
    let datatype = args.get("datatype").and_then(|v| v.as_str()).unwrap_or("xsd:string");

    match (|| {
        use crate::eavto::Object;
        use crate::owl::Individual;

        let object = if value_type == "iri" {
            Object::Iri(value.to_string())
        } else {
            Object::Literal {
                value: value.to_string(),
                datatype: Some(datatype.to_string()),
                language: None,
            }
        };

        let individual = Individual::new(instance_iri);
        individual.add_property(conn, property_iri, object, "ai")?;

        // Emit entity-updated event
        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": instance_iri})).ok();
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "message": format!("Property {} added to {}", property_iri, instance_iri),
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

pub fn remove_property_value(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
    let instance_iri = match args.get("instance_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: instance_iri".to_string()),
        },
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_iri".to_string()),
        },
    };

    let value = match args.get("value").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: value".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple, Object};

        // Find the triple to retract
        let triples_result = query::get_by_entity_predicate(conn, instance_iri, property_iri)?;

        for triple in triples_result.triples {
            let matches = match &triple.object {
                Object::Iri(iri) => iri == value,
                Object::Blank(blank) => blank == value,
                Object::Literal { value: v, .. } => v == value,
                Object::Integer(i) => i.to_string() == value,
                Object::Number(n) => {
                    // Try to parse the input value as f64 for numeric comparison
                    if let Ok(input_num) = value.parse::<f64>() {
                        (n - input_num).abs() < f64::EPSILON
                    } else {
                        n.to_string() == value
                    }
                },
                Object::Boolean(b) => b.to_string() == value,
                Object::DateTime(dt) => dt.to_string() == value,
            };

            if matches {
                store::retract_triples(conn, &[Triple::new(instance_iri, property_iri, triple.object)], "ai")?;

                // Emit entity-updated event
                if let Some(app_handle) = app {
                    app_handle.emit("entity-updated", serde_json::json!({"entityId": instance_iri})).ok();
                }

                return Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
                    "success": true,
                    "message": format!("Property value removed from {}", instance_iri),
                }));
            }
        }

        Ok(serde_json::json!({
            "success": false,
            "message": "Property value not found",
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
