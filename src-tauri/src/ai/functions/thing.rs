use serde_json::Value;
use crate::eavto::Connection;
use crate::owl::{Individual, Object, Property, PropertyType};
use super::ToolResult;
use std::sync::atomic::{AtomicU64, Ordering};

static IRI_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_iri_id() -> u64 {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    loop {
        let current = IRI_COUNTER.load(Ordering::Relaxed);
        let next = if now > current { now } else { current + 1 };
        if IRI_COUNTER.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            return next;
        }
    }
}

fn build_objects(conn: &Connection, detail_iri: &str, raw_values: &[Value]) -> Result<Vec<Object>, crate::owl::OwlError> {
    let prop = Property::get(conn, detail_iri)?;
    let is_iri = prop.as_ref().map(|p| p.property_type == PropertyType::ObjectProperty).unwrap_or(false);
    let datatype = prop.as_ref()
        .and_then(|p| p.ranges.first().cloned())
        .unwrap_or_else(|| "xsd:string".to_string());

    Ok(raw_values.iter()
        .filter_map(|v| v.as_str())
        .map(|value| {
            if is_iri {
                Object::Iri(value.to_string())
            } else {
                Object::Literal { value: value.to_string(), datatype: Some(datatype.clone()), language: None }
            }
        })
        .collect())
}

pub fn remember(conn: &Connection, args: &Value) -> ToolResult {
    let query_str = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let entity_type_filter = args.get("type").and_then(|v| v.as_str());
    let concept_iri = args.get("concept_iri").and_then(|v| v.as_str());
    let include_retracted = args.get("include_retracted").and_then(|v| v.as_bool()).unwrap_or(false);
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let filters_owned: Option<Vec<(String, String, String)>> =
        args.get("filters").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|item| {
                let detail = item.get("detail").and_then(|v| v.as_str())?.to_string();
                let value = item.get("value").and_then(|v| v.as_str())?.to_string();
                let operator = item.get("operator").and_then(|v| v.as_str()).unwrap_or("=").to_string();
                Some((detail, value, operator))
            }).collect()
        });

    let tokens: Vec<String> = if query_str.is_empty() {
        Vec::new()
    } else {
        query_str.split_whitespace().map(|s| s.to_lowercase()).collect()
    };

    let filters_ref: Option<&[(String, String, String)]> = filters_owned.as_deref();

    match crate::owl::search_rich(
        conn,
        &tokens,
        entity_type_filter,
        concept_iri,
        filters_ref,
        include_retracted,
        limit,
        offset,
    ) {
        Ok((results, total)) => {
            let entities: Vec<serde_json::Value> = results.into_iter()
                .map(|r| serde_json::json!({
                    "id": r.id,
                    "label": r.label,
                    "icon": r.icon,
                    "type": r.entity_type,
                    "matchedProperties": r.matched_properties,
                    "conceptType": r.concept_type,
                    "status": r.status,
                }))
                .collect();
            ToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "entities": entities,
                    "total": total,
                    "limit": limit,
                    "offset": offset,
                })),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn get_things(conn: &Connection, args: &Value) -> ToolResult {
    let iris = match args.get("iris").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iris".to_string()),
        },
    };

    let mut results = Vec::new();
    let mut errors = Vec::new();
    for v in iris {
        if let Some(iri) = v.as_str() {
            let r = get_thing_one(conn, &serde_json::json!({ "iri": iri }));
            if r.success {
                if let Some(val) = r.result {
                    results.push(val);
                }
            } else {
                errors.push(format!("{}: {}", iri, r.error.unwrap_or_default()));
            }
        }
    }

    ToolResult {
        success: errors.is_empty(),
        result: Some(serde_json::json!({ "things": results })),
        error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
    }
}

pub fn learn_thing(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, learn_thing_one)
}

fn learn_thing_one(conn: &mut Connection, args: &Value) -> ToolResult {
    if args.get("iri").is_some() {
        update_thing_one(conn, args)
    } else {
        create_thing_one(conn, args)
    }
}


fn get_thing_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    let include_retracted = args.get("include_retracted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match (|| {
        let individual = Individual::get(conn, iri)?
            .ok_or_else(|| crate::owl::OwlError::NotFound(iri.to_string()))?;

        let mut concept_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut seen_groups = std::collections::HashSet::new();
        for b in &individual.backlinks {
            let concept = b.source_class.clone().unwrap_or_else(|| "owl:Thing".to_string());
            let group_key = format!("{}:{}", b.predicate, concept);
            if seen_groups.insert(group_key) {
                *concept_counts.entry(concept).or_insert(0) += b.group_total;
            }
        }

        let mut backlinks: Vec<serde_json::Value> = concept_counts
            .into_iter()
            .map(|(concept_iri, count)| {
                let concept_label = crate::owl::Thing::get(conn, &concept_iri).label;
                serde_json::json!({
                    "concept": concept_iri,
                    "conceptLabel": concept_label,
                    "count": count,
                })
            })
            .collect();
        backlinks.sort_by(|a, b| {
            let ca = a["count"].as_u64().unwrap_or(0);
            let cb = b["count"].as_u64().unwrap_or(0);
            cb.cmp(&ca)
        });

        let mut properties = individual.serializable_properties(conn);
        for (i, (_, obj)) in individual.properties.iter().enumerate() {
            if let Object::DateTime(rfc3339) = obj {
                if let Some(entry) = properties.get_mut(i) {
                    entry["value"] = serde_json::json!(rfc3339);
                }
            }
        }

        if include_retracted {
            let retracted_triples = Individual::get_retracted_properties(conn, iri)?;
            for triple in &retracted_triples {
                let value: serde_json::Value = match &triple.object {
                    Object::Iri(s) | Object::Blank(s) => serde_json::json!(s),
                    Object::Literal { value: v, .. } => serde_json::json!(v),
                    Object::Integer(i) => serde_json::json!(i),
                    Object::Number(n) => serde_json::json!(n),
                    Object::Boolean(b) => serde_json::json!(b),
                    Object::DateTime(rfc3339) => serde_json::json!(rfc3339),
                };
                properties.push(serde_json::json!({
                    "property": triple.predicate,
                    "value": value,
                    "retracted": true,
                }));
            }
        }

        let class_iris: Vec<String> = individual.types.iter()
            .map(|t| t.iri.clone())
            .filter(|iri| iri.starts_with("foundation:"))
            .collect();

        let mut allowed_statuses: Vec<serde_json::Value> = Vec::new();
        let mut required_fields: Vec<String> = Vec::new();
        let mut seen_required = std::collections::HashSet::new();

        for class_iri in &class_iris {
            let status_iris = crate::owl::get_all_iri_properties(conn, class_iri, "foundation:allowedStatus")?;
            for status_iri in status_iris {
                let thing = crate::owl::Thing::get(conn, &status_iri);
                let (icon, color) = crate::owl::resolve_status_appearance(conn, &status_iri);
                allowed_statuses.push(serde_json::json!({
                    "iri": status_iri,
                    "label": thing.label,
                    "icon": icon,
                    "color": color,
                }));
            }

            let restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, class_iri)?;
            for r in restrictions {
                if r.is_required() && seen_required.insert(r.property_iri.clone()) {
                    required_fields.push(r.property_iri);
                }
            }
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": individual.iri,
            "label": individual.label,
            "icon": individual.icon,
            "comment": individual.comment,
            "types": individual.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": properties,
            "backlinks": backlinks,
            "allowedStatuses": allowed_statuses,
            "requiredFields": required_fields,
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

fn create_thing_one(
    conn: &mut Connection,
    args: &Value,
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

    let icon = if let Some(icon_str) = args.get("icon").and_then(|v| v.as_str()) {
        icon_str.to_string()
    } else {
        let concept_thing = crate::owl::Thing::get(conn, concept_iri);
        match concept_thing.icon {
            Some(icon) => icon,
            None => return ToolResult {
                success: false,
                result: None,
                error: Some(format!(
                    "No icon provided and concept '{}' has no icon to inherit",
                    concept_iri
                )),
            },
        }
    };

    let comment = args.get("comment").and_then(|v| v.as_str());

    let concept_name = concept_iri.split(':').last().unwrap_or("Thing");
    let generated_iri = format!("foundation:{}_{}", concept_name, next_iri_id());

    match (|| {
        let individual = Individual::new(&generated_iri);
        individual.assert(conn, concept_iri, label, &icon, "ai")?;

        if let Some(comment_text) = comment {
            individual.add_property(conn, "rdfs:comment", vec![Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai")?;
        }

        if let Some(properties) = args.get("upsert_properties").and_then(|v| v.as_array()) {
            for prop_entry in properties {
                let detail_iri = prop_entry.get("detail_iri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Each property entry must have 'detail_iri'".to_string()
                    ))?;

                let raw_values = prop_entry.get("values")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        format!("Property '{}' must have 'values' array", detail_iri)
                    ))?;

                if raw_values.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        format!("Property '{}' values array must not be empty", detail_iri)
                    ));
                }

                if detail_iri == "foundation:hasStatus" {
                    if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                        crate::owl::validate_allowed_status(conn, concept_iri, status_iri)?;
                    }
                }

                let objects = build_objects(conn, detail_iri, raw_values)?;

                if objects.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        format!("Property '{}' values contain no valid string entries", detail_iri)
                    ));
                }

                individual.add_property(conn, detail_iri, objects, "ai")?;
            }
        }

        let restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, concept_iri)?;
        let required: Vec<&str> = restrictions.iter()
            .filter(|r| r.is_required())
            .map(|r| r.property_iri.as_str())
            .collect();
        if !required.is_empty() {
            let mut provided: std::collections::HashSet<String> = args.get("upsert_properties")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|p| p.get("detail_iri").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .collect())
                .unwrap_or_default();
            // label is always required for creation and is stored as rdfs:label
            provided.insert("rdfs:label".to_string());
            for prop_iri in &required {
                if !provided.contains(*prop_iri) {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Required field '{}' must be provided when creating an instance of '{}'",
                        prop_iri, concept_iri,
                    )));
                }
            }
        }

        super::batch::queue_event("entity-created", serde_json::json!({"entityId": generated_iri.clone()}));

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

fn update_thing_one(
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
        let mut updated_fields: Vec<String> = Vec::new();

        let individual = Individual::new(iri);

        if let Some(concept_iri) = args.get("concept_iri").and_then(|v| v.as_str()) {
            let has_type = crate::owl::get_iri_property(conn, iri, "rdf:type")
                .ok().flatten().is_some();
            if !has_type {
                let existing_label = crate::owl::get_literal_property(conn, iri, "rdfs:label")
                    .ok().flatten();
                let label = args.get("label").and_then(|v| v.as_str())
                    .or_else(|| existing_label.as_deref())
                    .unwrap_or("Unknown");
                let icon = args.get("icon").and_then(|v| v.as_str()).unwrap_or("category");
                individual.assert(conn, concept_iri, label, icon, "ai")?;
                updated_fields.push("rdf:type".to_string());
            }
        }

        if let Some(label) = args.get("label").and_then(|v| v.as_str()) {
            individual.add_property(conn, "rdfs:label", vec![Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai")?;
            updated_fields.push("label".to_string());
        }

        if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
            crate::owl::validate_icon(conn, icon)?;
            let (icon_pred, icon_obj) = crate::owl::icon_store_value(icon);
            individual.add_property(conn, icon_pred, vec![icon_obj], "ai")?;
            updated_fields.push("icon".to_string());
        }

        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            individual.add_property(conn, "rdfs:comment", vec![Object::Literal {
                value: comment.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai")?;
            updated_fields.push("comment".to_string());
        }

        if let Some(remove_props) = args.get("remove_properties").and_then(|v| v.as_array()) {
            for item in remove_props {
                if let Some(detail_iri) = item.as_str() {
                    Individual::clear_property(conn, iri, detail_iri, "ai")?;
                    updated_fields.push(format!("-{}", detail_iri));
                }
            }
        }

        let mut referenced_iris: std::collections::HashSet<String> = std::collections::HashSet::new();

        if let Some(properties) = args.get("upsert_properties").and_then(|v| v.as_array()) {
            let concept_iri = if let Ok(Some(c)) = crate::owl::get_iri_property(conn, iri, "rdf:type") {
                Some(c)
            } else {
                None
            };

            for prop_entry in properties {
                let detail_iri = prop_entry.get("detail_iri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Each property entry must have 'detail_iri'".to_string()
                    ))?;

                let raw_values = prop_entry.get("values")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        format!("Property '{}' must have 'values' array", detail_iri)
                    ))?;

                if raw_values.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        format!("Property '{}' values array must not be empty", detail_iri)
                    ));
                }

                if detail_iri == "foundation:hasStatus" {
                    if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                        if let Some(ref concept) = concept_iri {
                            crate::owl::validate_allowed_status(conn, concept, status_iri)?;
                        }
                    }
                }

                let objects = build_objects(conn, detail_iri, raw_values)?;

                if objects.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        format!("Property '{}' values contain no valid string entries", detail_iri)
                    ));
                }

                for obj in &objects {
                    if let Object::Iri(ref_iri) = obj {
                        referenced_iris.insert(ref_iri.clone());
                    }
                }

                individual.add_property(conn, detail_iri, objects, "ai")?;
                updated_fields.push(detail_iri.to_string());
            }
        }

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        for ref_iri in &referenced_iris {
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": ref_iri}));
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
    super::batch::run_atomic(conn, args, app, delete_thing_one)
}

fn delete_thing_one(
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

    let detail_iri = args.get("detail_iri").and_then(|v| v.as_str());
    let value = args.get("value").and_then(|v| v.as_str());

    match (|| {
        match (detail_iri, value) {
            (Some(detail), Some(val)) => {
                let current_count = Individual::get_property_count(conn, iri, detail)?;
                crate::owl::cardinality::validate_property_cardinality(
                    conn, iri, detail, current_count.saturating_sub(1),
                )?;
                match Individual::remove_property_value(conn, iri, detail, val, "ai")? {
                    Some(removed) => {
                        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
                        if let Object::Iri(ref_iri) = removed {
                            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": ref_iri}));
                        }
                        Ok::<_, crate::owl::OwlError>(serde_json::json!({
                            "success": true,
                            "message": format!("Value removed from {} on {}", detail, iri),
                        }))
                    }
                    None => Ok(serde_json::json!({
                        "success": false,
                        "message": "Value not found",
                    }))
                }
            }
            (Some(detail), None) => {
                Individual::clear_property(conn, iri, detail, "ai")?;
                super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
                Ok::<_, crate::owl::OwlError>(serde_json::json!({
                    "success": true,
                    "message": format!("All values of {} removed from {}", detail, iri),
                }))
            }
            _ => {
                Individual::retract(conn, iri, "ai")?;
                super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
                Ok::<_, crate::owl::OwlError>(serde_json::json!({
                    "success": true,
                    "message": format!("Thing {} deleted successfully", iri),
                }))
            }
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




#[cfg(test)]
#[path = "thing_tests.rs"]
mod tests;
