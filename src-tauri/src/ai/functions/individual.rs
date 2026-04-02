use serde_json::Value;
use crate::eavto::Connection;
use crate::owl::{Individual, Object, Property, PropertyType};
use super::ToolResult;
use std::sync::atomic::{AtomicU64, Ordering};

fn load_class_context(conn: &Connection, class_iri: &str) -> Option<Value> {
    super::class::load_class_context(conn, class_iri)
}

fn load_range_contexts(conn: &Connection, args: &Value) -> Option<Value> {
    let properties = args.get("properties").and_then(|v| v.as_array())?;

    let mut range_contexts: Vec<Value> = Vec::new();

    for prop_entry in properties {
        let property_iri = match prop_entry.get("property_iri").and_then(|v| v.as_str()) {
            Some(iri) => iri,
            None => continue,
        };

        let prop = match Property::get(conn, property_iri).ok().flatten() {
            Some(p) => p,
            None => continue,
        };

        if prop.property_type != PropertyType::ObjectProperty {
            continue;
        }

        let range_iri = match prop.ranges.first() {
            Some(r) => r.clone(),
            None => continue,
        };

        if range_iri == "owl:Thing" || range_iri.starts_with("xsd:") {
            continue;
        }

        range_contexts.push(serde_json::json!({
            "property": property_iri,
            "range": range_iri,
        }));
    }

    if range_contexts.is_empty() {
        None
    } else {
        Some(serde_json::json!({ "rangeContexts": range_contexts }))
    }
}

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

fn build_objects(conn: &Connection, property_iri: &str, raw_values: &[Value]) -> Result<Vec<Object>, crate::owl::OwlError> {
    let prop = Property::get(conn, property_iri)?;
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

pub fn search(conn: &Connection, args: &Value) -> ToolResult {
    let query_str = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let entity_type_filter = args.get("type").and_then(|v| v.as_str());
    let class_iri = args.get("class_iri").and_then(|v| v.as_str());
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

    let iri_hit = if !query_str.is_empty() && filters_owned.is_none() {
        crate::owl::try_iri_direct_lookup(conn, query_str)
    } else {
        None
    };

    let tokens: Vec<String> = if query_str.is_empty() {
        Vec::new()
    } else {
        query_str.split_whitespace().map(|s| s.to_lowercase()).collect()
    };

    let filters_ref: Option<&[(String, String, String)]> = filters_owned.as_deref();

    match crate::owl::search(
        conn,
        &tokens,
        entity_type_filter,
        class_iri,
        filters_ref,
        include_retracted,
        limit,
        offset,
    ) {
        Ok((mut results, mut total)) => {
            if let Some(hit) = iri_hit {
                if !results.iter().any(|r| r.id == hit.id) {
                    total += 1;
                    results.insert(0, hit);
                    results.truncate(limit);
                } else if let Some(pos) = results.iter().position(|r| r.id == hit.id) {
                    let hit = results.remove(pos);
                    results.insert(0, hit);
                }
            }
            let entities: Vec<serde_json::Value> = results.into_iter()
                .map(|r| serde_json::json!({
                    "id": r.id,
                    "label": r.label,
                    "icon": r.icon,
                    "type": r.entity_type,
                    "matchedProperties": r.matched_properties,
                    "classType": r.class_type,
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
                concept: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: class_iri.and_then(|iri| load_class_context(conn,iri)),
        },
    }
}

pub fn describe_individual(conn: &Connection, args: &Value) -> ToolResult {
    let iris = match args.get("iris").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iris".to_string()),
            concept: None,
        },
    };

    let mut results = Vec::new();
    let mut errors = Vec::new();
    for v in iris {
        if let Some(iri) = v.as_str() {
            crate::search::track_access(conn, iri);
            let r = describe_individual_one(conn, &serde_json::json!({ "iri": iri }));
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
        result: Some(serde_json::json!({ "individuals": results })),
        error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
        concept: None,
    }
}

fn describe_individual_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
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
            .map(|(class_iri, count)| {
                let class_label = crate::owl::Thing::get(conn, &class_iri).label;
                serde_json::json!({
                    "class": class_iri,
                    "conceptLabel": class_label,
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

        const CONTENT_MAX: usize = 500;
        for prop in &mut properties {
            if prop.get("property").and_then(|p| p.as_str()) == Some("foundation:content") {
                if let Some(value) = prop.get("value").and_then(|v| v.as_str()) {
                    if value.len() > CONTENT_MAX {
                        let truncated = format!("{}…[{} chars total]", &value[..CONTENT_MAX], value.len());
                        prop["value"] = serde_json::json!(truncated);
                    }
                }
            }
        }

        let class_iris: Vec<String> = individual.types.iter()
            .map(|t| t.iri.clone())
            .filter(|iri| iri.starts_with("foundation:"))
            .collect();

        let applicable_automations = super::find_automations_for_types(conn, &class_iris);

        let mut allowed_statuses: Vec<serde_json::Value> = Vec::new();
        let mut required_fields: Vec<String> = Vec::new();
        let mut seen_required = std::collections::HashSet::new();

        for class_iri in &class_iris {
            let status_iris = crate::owl::get_all_iri_properties(conn, class_iri, "foundation:allowedStatus")?;
            for status_iri in status_iris {
                let thing = crate::owl::Thing::get(conn, &status_iri);
                let comment = crate::owl::get_literal_property(conn, &status_iri, "rdfs:comment")
                    .unwrap_or_default();
                allowed_statuses.push(serde_json::json!({
                    "iri": status_iri,
                    "label": thing.label,
                    "comment": comment,
                }));
            }

            let restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, class_iri)?;
            for r in restrictions {
                if r.is_required() && seen_required.insert(r.property_iri.clone()) {
                    required_fields.push(r.property_iri);
                }
            }
        }

        let is_locked = crate::owl::is_system_locked(conn, iri);
        let mut result = serde_json::json!({
            "iri": individual.iri,
            "label": individual.label,
            "icon": individual.icon,
            "comment": individual.comment,
            "isSystemLocked": is_locked,
            "types": individual.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": properties,
            "backlinks": backlinks,
            "allowedStatuses": allowed_statuses,
            "requiredFields": required_fields,
        });
        if !applicable_automations.is_empty() {
            result["applicableAutomations"] = serde_json::json!(applicable_automations);
        }
        Ok::<_, crate::owl::OwlError>(result)
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
            concept: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }
}

pub fn assert_individual(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, assert_individual_one)
}

fn assert_individual_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let class_iri = match args.get("class_iri").and_then(|v| v.as_str()) {
        Some(class_iri) => class_iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: class_iri".to_string()),
            concept: None,
        },
    };

    let label = match args.get("label").and_then(|v| v.as_str()) {
        Some(label) => label,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: label".to_string()),
            concept: load_class_context(conn,class_iri),
        },
    };

    let icon = if let Some(icon_str) = args.get("icon").and_then(|v| v.as_str()) {
        icon_str.to_string()
    } else {
        let concept_thing = crate::owl::Thing::get(conn, class_iri);
        match concept_thing.icon {
            Some(icon) => icon,
            None => return ToolResult {
                success: false,
                result: None,
                error: Some(format!(
                    "No icon provided and class '{}' has no icon to inherit",
                    class_iri
                )),
                concept: load_class_context(conn,class_iri),
            },
        }
    };

    let comment = args.get("comment").and_then(|v| v.as_str());

    let class_name = class_iri.split(':').last().unwrap_or("Thing");
    let generated_iri = format!("foundation:{}_{}", class_name, next_iri_id());

    match (|| {
        let individual = Individual::new(&generated_iri);
        individual.assert(conn, class_iri, label, &icon, "ai")?;

        if let Some(comment_text) = comment {
            individual.add_property(conn, "rdfs:comment", vec![Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "ai")?;
        }

        if let Some(properties) = args.get("properties").and_then(|v| v.as_array()) {
            for prop_entry in properties {
                let property_iri = prop_entry.get("property_iri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Each property entry must have 'property_iri'".to_string()
                    ))?;

                let raw_values = prop_entry.get("values")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        format!("Property '{}' must have 'values' array", property_iri)
                    ))?;

                if raw_values.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        format!("Property '{}' values array must not be empty", property_iri)
                    ));
                }

                if property_iri == "foundation:hasStatus" {
                    if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                        crate::owl::validate_allowed_status(conn, class_iri, status_iri)?;
                    }
                }

                let objects = build_objects(conn, property_iri, raw_values)?;

                if objects.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        format!("Property '{}' values contain no valid string entries", property_iri)
                    ));
                }

                individual.add_property(conn, property_iri, objects, "ai")?;
            }
        }

        let restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, class_iri)?;
        let required: Vec<&str> = restrictions.iter()
            .filter(|r| r.is_required())
            .map(|r| r.property_iri.as_str())
            .collect();
        if !required.is_empty() {
            let mut provided: std::collections::HashSet<String> = args.get("properties")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|p| p.get("property_iri").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .collect())
                .unwrap_or_default();
            provided.insert("rdfs:label".to_string());
            if comment.is_some() {
                provided.insert("rdfs:comment".to_string());
            }
            for prop_iri in &required {
                if !provided.contains(*prop_iri) {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Required property '{}' must be provided when creating an instance of '{}'",
                        prop_iri, class_iri,
                    )));
                }
            }
        }

        super::batch::queue_event("entity-created", serde_json::json!({"entityId": generated_iri.clone()}));

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": generated_iri,
        }))
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
            concept: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: load_range_contexts(conn, args),
            error: Some(e.to_string()),
            concept: load_class_context(conn,class_iri),
        },
    }
}

pub fn add_property_values(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, add_property_values_one)
}

fn add_property_values_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_iri".to_string()),
            concept: None,
        },
    };

    let raw_values = match args.get("values").and_then(|v| v.as_array()) {
        Some(v) if !v.is_empty() => v,
        Some(_) => return ToolResult {
            success: false,
            result: None,
            error: Some("values array must not be empty".to_string()),
            concept: None,
        },
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: values".to_string()),
            concept: None,
        },
    };

    match (|| {
        if property_iri == "foundation:hasStatus" {
            if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                let class_iri = crate::owl::get_iri_property(conn, iri, "rdf:type")
                    .ok().flatten()
                    .ok_or_else(|| crate::owl::OwlError::NotFound(format!("Individual '{}' has no rdf:type", iri)))?;
                crate::owl::validate_allowed_status(conn, &class_iri, status_iri)?;
            }
        }

        let objects = build_objects(conn, property_iri, raw_values)?;
        if objects.is_empty() {
            return Err(crate::owl::OwlError::ValidationError(
                format!("Property '{}' values contain no valid string entries", property_iri)
            ));
        }

        let individual = Individual::new(iri);
        individual.append_property(conn, property_iri, objects, "ai")?;

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        super::batch::queue_instance_recalc(iri.to_string(), property_iri.to_string());

        Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}))
    })() {
        Ok(result) => ToolResult { success: true, result: Some(result), error: None, concept: None },
        Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
    }
}

pub fn replace_property_values(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, replace_property_values_one)
}

fn replace_property_values_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_iri".to_string()),
            concept: None,
        },
    };

    let raw_values = match args.get("values").and_then(|v| v.as_array()) {
        Some(v) => v,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: values".to_string()),
            concept: None,
        },
    };

    match (|| {
        if raw_values.is_empty() {
            crate::owl::cardinality::validate_property_cardinality(conn, iri, property_iri, 0)?;
            Individual::clear_property(conn, iri, property_iri, "ai")?;
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
            return Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}));
        }

        if property_iri == "foundation:hasStatus" {
            if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                let class_iri = crate::owl::get_iri_property(conn, iri, "rdf:type")
                    .ok().flatten()
                    .ok_or_else(|| crate::owl::OwlError::NotFound(format!("Individual '{}' has no rdf:type", iri)))?;
                crate::owl::validate_allowed_status(conn, &class_iri, status_iri)?;
            }
        }

        let objects = build_objects(conn, property_iri, raw_values)?;
        if objects.is_empty() {
            return Err(crate::owl::OwlError::ValidationError(
                format!("Property '{}' values contain no valid string entries", property_iri)
            ));
        }

        let individual = Individual::new(iri);
        individual.add_property(conn, property_iri, objects, "ai")?;

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        super::batch::queue_instance_recalc(iri.to_string(), property_iri.to_string());

        Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}))
    })() {
        Ok(result) => ToolResult { success: true, result: Some(result), error: None, concept: None },
        Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
    }
}

pub fn remove_property_values(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, remove_property_values_one)
}

fn remove_property_values_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_iri".to_string()),
            concept: None,
        },
    };

    let values_to_remove = match args.get("values").and_then(|v| v.as_array()) {
        Some(v) if !v.is_empty() => v,
        Some(_) => return ToolResult {
            success: false,
            result: None,
            error: Some("values array must not be empty".to_string()),
            concept: None,
        },
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: values".to_string()),
            concept: None,
        },
    };

    match (|| {
        let current_count = Individual::get_property_count(conn, iri, property_iri)?;
        let remove_count = values_to_remove.len();
        crate::owl::cardinality::validate_property_cardinality(
            conn, iri, property_iri, current_count.saturating_sub(remove_count),
        )?;

        for val in values_to_remove {
            if let Some(val_str) = val.as_str() {
                Individual::remove_property_value(conn, iri, property_iri, val_str, "ai")?;
            }
        }

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));

        Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}))
    })() {
        Ok(result) => ToolResult { success: true, result: Some(result), error: None, concept: None },
        Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
    }
}

pub fn clear_property(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, clear_property_one)
}

fn clear_property_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_iri".to_string()),
            concept: None,
        },
    };

    match (|| {
        crate::owl::cardinality::validate_property_cardinality(conn, iri, property_iri, 0)?;
        Individual::clear_property(conn, iri, property_iri, "ai")?;
        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}))
    })() {
        Ok(result) => ToolResult { success: true, result: Some(result), error: None, concept: None },
        Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
    }
}

pub fn retract_individual(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, retract_individual_one)
}

fn retract_individual_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    match Individual::retract_with_summary(conn, iri, "ai") {
        Ok(cascade) => {
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
            let message = if cascade.is_empty() {
                format!("Individual '{}' retracted.", iri)
            } else {
                let parts: Vec<String> = cascade.iter()
                    .map(|(prop, dir, count)| format!("{} via {} ({})", count, prop, dir))
                    .collect();
                format!("Individual '{}' retracted. Cascade: {}.", iri, parts.join(", "))
            };
            ToolResult {
                success: true,
                result: Some(serde_json::json!({"iri": iri, "message": message})),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
    }
}

#[cfg(test)]
#[path = "individual_tests/mod.rs"]
mod tests;
