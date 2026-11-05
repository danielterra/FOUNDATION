use super::ToolResult;
use crate::eavto::Connection;
use crate::owl::{Individual, Object, Property, PropertyType, PropertyValueCursor};
use serde_json::Value;
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
        if IRI_COUNTER
            .compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return next;
        }
    }
}

/// Returns `"rdfs:comment"` when the caller supplies a `comment`-named alias
/// that is NOT already in the `rdfs` namespace (e.g. `"foundation:comment"` or
/// bare `"comment"`). All other IRIs pass through unchanged.
fn normalize_comment_alias(property_iri: &str) -> &str {
    let local_name = property_iri.split(':').last().unwrap_or(property_iri);
    if local_name == "comment" && !property_iri.starts_with("rdfs:") {
        "rdfs:comment"
    } else {
        property_iri
    }
}

fn build_objects(
    conn: &Connection,
    property_iri: &str,
    raw_values: &[Value],
) -> Result<Vec<Object>, crate::owl::OwlError> {
    let property_iri = normalize_comment_alias(property_iri);
    let prop = Property::get(conn, property_iri)?;
    let is_iri = prop
        .as_ref()
        .map(|p| p.property_type == PropertyType::ObjectProperty)
        .unwrap_or(false);
    let datatype = prop
        .as_ref()
        .and_then(|p| p.ranges.first().cloned())
        .unwrap_or_else(|| "xsd:string".to_string());

    raw_values
        .iter()
        .filter(|v| !v.is_null())
        .map(|v| coerce_value(v, is_iri, &datatype, property_iri))
        .collect()
}

fn coerce_value(
    v: &Value,
    is_iri: bool,
    datatype: &str,
    property_iri: &str,
) -> Result<Object, crate::owl::OwlError> {
    let lexical = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Property '{}': unsupported value type {}; expected string, number or boolean",
                property_iri,
                type_name(v),
            )))
        }
    };

    if is_iri {
        Ok(Object::Iri(lexical))
    } else {
        Ok(Object::Literal {
            value: lexical,
            datatype: Some(datatype.to_string()),
            language: None,
        })
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub fn search(conn: &Connection, args: &Value) -> ToolResult {
    let query_str = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let entity_type_filter = args.get("type").and_then(|v| v.as_str());
    let class_iri = args.get("class_iri").and_then(|v| v.as_str());
    let include_retracted = args
        .get("include_retracted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let filters_owned: Option<Vec<(String, String, String)>> =
        args.get("filters").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let detail = item.get("detail").and_then(|v| v.as_str())?.to_string();
                    let operator = item
                        .get("operator")
                        .and_then(|v| v.as_str())
                        .unwrap_or("=")
                        .to_string();
                    let value = if operator == "exists" || operator == "not_exists" {
                        String::new()
                    } else {
                        match item.get("value")? {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => return None,
                        }
                    };
                    Some((detail, value, operator))
                })
                .collect()
        });

    let iri_hit = if !query_str.is_empty() && filters_owned.is_none() {
        crate::core_ontology::search::try_iri_direct_lookup(conn, query_str)
    } else {
        None
    };

    let tokens: Vec<String> = if query_str.is_empty() {
        Vec::new()
    } else {
        query_str
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect()
    };

    let filters_ref: Option<&[(String, String, String)]> = filters_owned.as_deref();

    match crate::core_ontology::search::search(
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
            let entities: Vec<serde_json::Value> = results
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "label": r.label,
                        "icon": r.icon,
                        "type": r.entity_type,
                        "matchedProperties": r.matched_properties,
                        "classType": r.class_type,
                        "status": r.status,
                    })
                })
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
            concept: class_iri.and_then(|iri| load_class_context(conn, iri)),
        },
    }
}

pub fn read_property_page(conn: &Connection, args: &Value) -> ToolResult {
    let individual_iri = match args.get("individual_iri").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: individual_iri".to_string()),
                concept: None,
            }
        }
    };
    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: property_iri".to_string()),
                concept: None,
            }
        }
    };
    let page = args
        .get("page")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .max(1) as usize;
    let page_size = args
        .get("page_size")
        .and_then(|v| v.as_u64())
        .unwrap_or(2000)
        .max(1) as usize;
    let value_index = args
        .get("value_index")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let individual = match Individual::get(conn, individual_iri) {
        Ok(Some(ind)) => ind,
        Ok(None) => {
            return ToolResult {
                success: false,
                result: None,
                error: Some(format!("Individual '{}' not found.", individual_iri)),
                concept: None,
            }
        }
        Err(e) => {
            return ToolResult {
                success: false,
                result: None,
                error: Some(e.to_string()),
                concept: None,
            }
        }
    };

    let values: Vec<String> = individual
        .properties
        .iter()
        .filter(|(pred, _)| pred == property_iri)
        .enumerate()
        .filter(|(i, _)| *i == value_index)
        .map(|(_, (_, obj))| match obj {
            Object::Literal { value, .. } => value.clone(),
            Object::Iri(s) | Object::Blank(s) => s.clone(),
            Object::Integer(n) => n.to_string(),
            Object::Number(n) => n.to_string(),
            Object::Boolean(b) => b.to_string(),
            Object::DateTime(s) => s.clone(),
        })
        .collect();

    let text = match values.into_iter().next() {
        Some(v) => v,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some(format!(
                    "Property '{}' not found on '{}' at value_index {}.",
                    property_iri, individual_iri, value_index
                )),
                concept: None,
            }
        }
    };

    let chars: Vec<char> = text.chars().collect();
    let total_chars = chars.len();
    let total_pages = (total_chars + page_size - 1) / page_size;
    let total_pages = total_pages.max(1);

    if page > total_pages {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!(
                "Page {} out of range — total_pages is {}.",
                page, total_pages
            )),
            concept: None,
        };
    }

    let start = (page - 1) * page_size;
    let end = (start + page_size).min(total_chars);
    let page_text: String = chars[start..end].iter().collect();

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "value": page_text,
            "page": page,
            "total_chars": total_chars,
            "total_pages": total_pages,
            "has_next_page": page < total_pages,
        })),
        error: None,
        concept: None,
    }
}

pub fn describe_individual(conn: &Connection, args: &Value) -> ToolResult {
    let continuation_fields = ["individual_iri", "property_iri", "snapshot_tx", "cursor"];
    let continuation_requested = continuation_fields
        .iter()
        .any(|field| args.get(field).is_some());
    if continuation_requested {
        return describe_individual_property_page(conn, args);
    }

    let iris = match args.get("iris").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iris".to_string()),
                concept: None,
            }
        }
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
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        },
        concept: None,
    }
}

fn describe_object_value(object: &Object) -> Value {
    match object {
        Object::Iri(value) | Object::Blank(value) => serde_json::json!({
            "kind": object.object_type(),
            "value": value,
        }),
        Object::Literal {
            value,
            datatype,
            language,
        } => serde_json::json!({
            "kind": "literal",
            "value": value,
            "datatype": datatype,
            "language": language,
        }),
        Object::Integer(value) => {
            serde_json::json!({ "kind": "literal", "value": value, "datatype": "xsd:integer" })
        }
        Object::Number(value) => {
            serde_json::json!({ "kind": "literal", "value": value, "datatype": "xsd:decimal" })
        }
        Object::Boolean(value) => {
            serde_json::json!({ "kind": "literal", "value": value, "datatype": "xsd:boolean" })
        }
        Object::DateTime(value) => {
            serde_json::json!({ "kind": "literal", "value": value, "datatype": "xsd:dateTime" })
        }
    }
}

fn describe_property_cursor(
    cursor: &PropertyValueCursor,
    individual_iri: &str,
    property_iri: &str,
) -> Value {
    serde_json::json!({
        "individual_iri": individual_iri,
        "property_iri": property_iri,
        "snapshot_tx": cursor.snapshot_tx,
        "value_key": cursor.value_key,
        "value_tx": cursor.value_tx,
        "datatype": cursor.datatype,
        "language": cursor.language,
        "object_type": cursor.object_type,
    })
}

fn describe_individual_property_page(conn: &Connection, args: &Value) -> ToolResult {
    let individual_iri = args.get("individual_iri").and_then(Value::as_str);
    let property_iri = args.get("property_iri").and_then(Value::as_str);
    let snapshot_tx = args.get("snapshot_tx").and_then(Value::as_i64);
    let cursor = args.get("cursor");
    let (Some(individual_iri), Some(property_iri), Some(snapshot_tx), Some(cursor)) =
        (individual_iri, property_iri, snapshot_tx, cursor)
    else {
        return ToolResult {
            success: false,
            result: None,
            error: Some("Continuation requires individual_iri, property_iri, snapshot_tx, and cursor from the previous page".to_string()),
            concept: None,
        };
    };
    if args.get("iris").is_some() {
        return ToolResult {
            success: false,
            result: None,
            error: Some("Use either iris for an initial description or continuation parameters for the next property page, not both".to_string()),
            concept: None,
        };
    }

    let cursor_snapshot_tx = cursor.get("snapshot_tx").and_then(Value::as_i64);
    let value_key = cursor.get("value_key").and_then(Value::as_str);
    let value_tx = cursor.get("value_tx").and_then(Value::as_i64);
    let object_type = cursor.get("object_type").and_then(Value::as_str);
    let datatype = match cursor.get("datatype") {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) => None,
        _ => return invalid_property_cursor(),
    };
    let language = match cursor.get("language") {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) => None,
        _ => return invalid_property_cursor(),
    };
    let (Some(cursor_snapshot_tx), Some(value_key), Some(value_tx), Some(object_type)) =
        (cursor_snapshot_tx, value_key, value_tx, object_type)
    else {
        return invalid_property_cursor();
    };
    if !matches!(object_type, "iri" | "blank" | "literal") {
        return invalid_property_cursor();
    }
    if cursor.get("individual_iri").and_then(Value::as_str) != Some(individual_iri)
        || cursor.get("property_iri").and_then(Value::as_str) != Some(property_iri)
    {
        return ToolResult {
            success: false,
            result: None,
            error: Some(
                "Property cursor must belong to the requested individual_iri and property_iri"
                    .to_string(),
            ),
            concept: None,
        };
    }
    if cursor_snapshot_tx != snapshot_tx {
        return ToolResult {
            success: false,
            result: None,
            error: Some(
                "Property cursor snapshot_tx must match the requested snapshot_tx".to_string(),
            ),
            concept: None,
        };
    }

    const PAGE_SIZE: usize = 5;
    let after = PropertyValueCursor {
        entity_iri: individual_iri.to_string(),
        predicate: property_iri.to_string(),
        snapshot_tx: cursor_snapshot_tx,
        value_key: value_key.to_string(),
        value_tx,
        datatype,
        language,
        object_type: object_type.to_string(),
    };
    match crate::owl::get_property_values_page(
        conn,
        individual_iri,
        property_iri,
        PAGE_SIZE,
        snapshot_tx,
        Some(&after),
    ) {
        Ok(page) => {
            let secret_props =
                match crate::core_ontology::property_configuration::secret_property_iris(conn) {
                    Ok(properties) => properties,
                    Err(error) => {
                        return ToolResult {
                            success: false,
                            result: None,
                            error: Some(error.to_string()),
                            concept: None,
                        }
                    }
                };
            let is_secret = secret_props.contains(property_iri);
            let values = if is_secret {
                vec![serde_json::json!(
                    crate::core_ontology::property_configuration::SECRET_MASK
                )]
            } else {
                page.items
                    .iter()
                    .map(|triple| describe_object_value(&triple.object))
                    .collect::<Vec<_>>()
            };
            let result = serde_json::json!({
                "individual_iri": individual_iri,
                "property": property_iri,
                "values": values,
                "pagination": {
                    "total": if is_secret { Some(1) } else { None::<usize> },
                    "loaded": if is_secret { 1 } else { page.items.len() },
                    "has_more": if is_secret { false } else { page.has_more },
                    "snapshot_tx": page.snapshot_tx,
                    "cursor": if is_secret { None } else { page.next_cursor.as_ref().map(|cursor| describe_property_cursor(cursor, individual_iri, property_iri)) },
                },
                "isSecret": is_secret,
            });
            ToolResult {
                success: true,
                result: Some(result),
                error: None,
                concept: None,
            }
        }
        Err(error) => ToolResult {
            success: false,
            result: None,
            error: Some(error.to_string()),
            concept: None,
        },
    }
}

fn invalid_property_cursor() -> ToolResult {
    ToolResult {
        success: false,
        result: None,
        error: Some("cursor must contain individual_iri, property_iri, snapshot_tx, value_key, value_tx, datatype, language, and object_type from the previous property page".to_string()),
        concept: None,
    }
}

fn describe_individual_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args
        .get("iri")
        .or_else(|| args.get("IRI"))
        .and_then(|v| v.as_str())
    {
        Some(iri) => iri,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };

    let include_retracted = args
        .get("include_retracted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match (|| {
        let snapshot_tx =
            crate::owl::current_snapshot_tx(conn).map_err(crate::owl::OwlError::DatabaseError)?;
        const PROPERTY_PAGE_SIZE: usize = 5;

        let individual = Individual::get_with_property_page(
            conn,
            iri,
            snapshot_tx,
            foundation_core::owl::vocabulary::rdfs::LABEL,
            foundation_core::owl::vocabulary::rdfs::COMMENT,
            "foundation:hasIcon",
            foundation_core::owl::vocabulary::rdf::TYPE,
            PROPERTY_PAGE_SIZE,
        )?
            .ok_or_else(|| crate::owl::OwlError::NotFound(format!(
                "'{}' not found. Do NOT retry this IRI — use the `search` tool to find the correct IRI first.",
                iri
            )))?;

        // PAGE_SIZE must match `inspector__get_backlink_page` so the cursor emitted here
        // is valid for the next paginated call.
        const PAGE_SIZE: usize = 15;

        // Composite cursor (last_tx, subject) of the PAGE_SIZE-th item in
        // `last_tx DESC, subject ASC` order. Uses the same logic as individual.rs:
        // track min last_tx among included items; among items at that last_tx, track
        // the max subject (last in ASC order).
        let group_next_cursor: std::collections::HashMap<String, Option<(i64, String)>> = {
            let mut counts: std::collections::HashMap<String, (usize, i64, String, usize)> =
                std::collections::HashMap::new();
            for b in &individual.backlinks {
                let key = format!(
                    "{}:{}",
                    b.predicate,
                    b.source_class.as_deref().unwrap_or("owl:Thing")
                );
                let entry =
                    counts
                        .entry(key)
                        .or_insert((0, i64::MAX, String::new(), b.group_total));
                if entry.0 < PAGE_SIZE {
                    entry.0 += 1;
                    if b.last_tx < entry.1 {
                        entry.1 = b.last_tx;
                        entry.2 = b.subject.clone();
                    } else if b.last_tx == entry.1 && b.subject > entry.2 {
                        entry.2 = b.subject.clone();
                    }
                }
            }
            counts
                .into_iter()
                .map(|(key, (_, min_tx, boundary_subject, total))| {
                    let cursor = if total > PAGE_SIZE {
                        Some((min_tx, boundary_subject))
                    } else {
                        None
                    };
                    (key, cursor)
                })
                .collect()
        };

        let mut seen_groups = std::collections::HashSet::new();
        let mut backlinks: Vec<serde_json::Value> = Vec::new();
        for b in &individual.backlinks {
            let source_class = b
                .source_class
                .clone()
                .unwrap_or_else(|| "owl:Thing".to_string());
            let group_key = format!("{}:{}", b.predicate, source_class);
            if !seen_groups.insert(group_key.clone()) {
                continue;
            }
            let prop_label = crate::owl::Property::get(conn, &b.predicate)
                .ok()
                .flatten()
                .map(|prop| {
                    b.source_class
                        .as_deref()
                        .and_then(|cls| prop.domain_labels.iter().find(|dl| dl.domain == cls))
                        .map(|dl| {
                            dl.inverse_label
                                .as_deref()
                                .unwrap_or(&dl.forward_label)
                                .to_string()
                        })
                        .or_else(|| prop.label.clone())
                        .unwrap_or_else(|| b.predicate.clone())
                })
                .unwrap_or_else(|| b.predicate.clone());
            let class_label = crate::owl::Thing::get(conn, &source_class).label;
            let next_cursor = group_next_cursor
                .get(&group_key)
                .and_then(|c| c.as_ref())
                .map(|(tx, subj)| serde_json::json!({ "last_tx": tx, "subject": subj }));
            backlinks.push(serde_json::json!({
                "class": source_class,
                "conceptLabel": class_label,
                "propertyLabel": prop_label,
                "count": b.group_total,
                "nextCursor": next_cursor,
            }));
        }
        backlinks.sort_by(|a, b| {
            let ca = a["count"].as_u64().unwrap_or(0);
            let cb = b["count"].as_u64().unwrap_or(0);
            cb.cmp(&ca)
        });

        let mut property_iris: Vec<String> = individual
            .properties
            .iter()
            .map(|(predicate, _)| predicate.clone())
            .collect();
        property_iris.sort();
        property_iris.dedup();
        let mut properties = Vec::new();
        for property_iri in property_iris {
            let page = crate::owl::get_property_values_page(
                conn,
                iri,
                &property_iri,
                PROPERTY_PAGE_SIZE,
                snapshot_tx,
                None,
            )
            .map_err(crate::owl::OwlError::DatabaseError)?;
            let property = Property::get(conn, &property_iri).ok().flatten();
            let property_type = property
                .as_ref()
                .map(|value| value.classification().as_str())
                .unwrap_or("value");
            let mut entry = serde_json::json!({
                "property": property_iri,
                "propertyType": property_type,
                "values": page.items.iter().map(|triple| describe_object_value(&triple.object)).collect::<Vec<_>>(),
                "pagination": {
                    "total": Option::<usize>::None,
                    "loaded": page.items.len(),
                    "has_more": page.has_more,
                    "snapshot_tx": page.snapshot_tx,
                    "cursor": page.next_cursor.as_ref().map(|cursor| describe_property_cursor(cursor, iri, &property_iri)),
                },
            });
            if let Some(unit) = property.as_ref().and_then(|value| value.unit.as_ref()) {
                entry["unit"] = serde_json::json!(unit);
            }
            if let Some(rules) = property.and_then(|value| value.ai_behavior_rules) {
                entry["aiBehaviorRules"] = serde_json::json!(rules);
            }
            properties.push(entry);
        }

        if include_retracted {
            let retracted_triples = Individual::get_retracted_properties(conn, iri)?;
            for triple in &retracted_triples {
                properties.push(serde_json::json!({
                    "property": triple.predicate,
                    "values": [describe_object_value(&triple.object)],
                    "retracted": true,
                    "pagination": {
                        "total": 1,
                        "loaded": 1,
                        "has_more": false,
                        "snapshot_tx": snapshot_tx,
                        "cursor": null,
                    },
                }));
            }
        }

        // Secrets are replaced before anything else looks at the value: an agent
        // reading an entity must never receive a credential, and the value would
        // otherwise land in its transcript.
        let secret_props =
            crate::core_ontology::property_configuration::secret_property_iris(conn)?;
        for prop in &mut properties {
            let is_secret = prop
                .get("property")
                .and_then(Value::as_str)
                .map(|predicate| secret_props.contains(predicate))
                .unwrap_or(false);
            if is_secret {
                prop["values"] =
                    serde_json::json!([crate::core_ontology::property_configuration::SECRET_MASK]);
                prop["isSecret"] = serde_json::json!(true);
                prop["pagination"]["total"] = serde_json::json!(1);
                prop["pagination"]["loaded"] = serde_json::json!(1);
                prop["pagination"]["has_more"] = serde_json::json!(false);
                prop["pagination"]["cursor"] = Value::Null;
            }
        }

        let class_iris: Vec<String> = individual
            .types
            .iter()
            .map(|t| t.iri.clone())
            .filter(|iri| iri.starts_with("foundation:"))
            .collect();

        let applicable_automations = super::find_automations_for_types(conn, &class_iris);

        let mut allowed_statuses: Vec<serde_json::Value> = Vec::new();
        let mut required_fields: Vec<String> = Vec::new();
        let mut seen_required = std::collections::HashSet::new();
        let mut class_ai_behavior_rules: Vec<String> = Vec::new();

        for class_iri in &class_iris {
            let status_iris =
                crate::owl::get_all_iri_properties(conn, class_iri, "foundation:allowedStatus")?;
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

            let restrictions =
                crate::owl::cardinality::get_class_cardinality_restrictions(conn, class_iri)?;
            for r in restrictions {
                if r.is_required() && seen_required.insert(r.property_iri.clone()) {
                    required_fields.push(r.property_iri);
                }
            }

            if let Ok(Some(class)) = crate::owl::Class::get(conn, class_iri) {
                for (pred, val) in &class.concept_properties {
                    if pred == "foundation:aiBehaviorRules" {
                        if let Some(rule) = val.as_literal() {
                            class_ai_behavior_rules.push(rule);
                        }
                    }
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
            "classAiBehaviorRules": class_ai_behavior_rules,
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

pub(crate) fn assert_individual_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let class_iri = match args.get("class_iri").and_then(|v| v.as_str()) {
        Some(class_iri) => class_iri,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: class_iri".to_string()),
                concept: None,
            }
        }
    };

    let label = match args.get("label").and_then(|v| v.as_str()) {
        Some(label) => label,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: label".to_string()),
                concept: load_class_context(conn, class_iri),
            }
        }
    };

    if !crate::owl::Class::exists(conn, class_iri) {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!(
                "Class '{}' does not exist in the ontology. \
                 Create it first with define_class (check class_graph and search for an \
                 existing equivalent before creating), or use an existing class IRI.",
                class_iri,
            )),
            concept: None,
        };
    }

    let icon = if let Some(icon_str) = args.get("icon").and_then(|v| v.as_str()) {
        icon_str.to_string()
    } else {
        let concept_thing = crate::owl::Thing::get(conn, class_iri);
        match concept_thing.icon {
            Some(icon) => icon,
            None => {
                return ToolResult {
                    success: false,
                    result: None,
                    error: Some(format!(
                        "No icon provided and class '{}' has no icon to inherit",
                        class_iri
                    )),
                    concept: load_class_context(conn, class_iri),
                }
            }
        }
    };

    let comment = args.get("comment").and_then(|v| v.as_str());

    let class_name = class_iri.split(':').last().unwrap_or("Thing");
    let generated_iri = format!("foundation:{}_{}", class_name, next_iri_id());

    match (|| {
        let individual = Individual::new(&generated_iri);
        individual.assert(conn, class_iri, label, &icon, "ai")?;

        if let Some(comment_text) = comment {
            individual.add_property(
                conn,
                "rdfs:comment",
                vec![Object::Literal {
                    value: comment_text.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "ai",
            )?;
        }

        if let Some(properties) = args.get("properties").and_then(|v| v.as_array()) {
            for prop_entry in properties {
                let property_iri = prop_entry
                    .get("property_iri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        crate::owl::OwlError::ValidationError(
                            "Each property entry must have 'property_iri'".to_string(),
                        )
                    })?;

                let raw_values = prop_entry
                    .get("values")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        crate::owl::OwlError::ValidationError(format!(
                            "Property '{}' must have 'values' array",
                            property_iri
                        ))
                    })?;

                if raw_values.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Property '{}' values array must not be empty",
                        property_iri
                    )));
                }

                if property_iri == "foundation:hasStatus" {
                    if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                        crate::core_ontology::status::validate_allowed_status(
                            conn, class_iri, status_iri,
                        )?;
                    }
                }

                let objects = build_objects(conn, property_iri, raw_values)?;

                if objects.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Property '{}' values array contains only null entries",
                        property_iri,
                    )));
                }

                individual.add_property(conn, property_iri, objects, "ai")?;
            }
        }

        let restrictions =
            crate::owl::cardinality::get_class_cardinality_restrictions(conn, class_iri)?;
        let required: Vec<&str> = restrictions
            .iter()
            .filter(|r| r.is_required())
            .map(|r| r.property_iri.as_str())
            .collect();
        if !required.is_empty() {
            let mut provided: std::collections::HashSet<String> = args
                .get("properties")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| p.get("property_iri").and_then(|v| v.as_str()))
                        .map(|s| s.to_string())
                        .collect()
                })
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

        super::batch::queue_event(
            "entity-created",
            serde_json::json!({"entityId": generated_iri.clone()}),
        );

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
            concept: load_class_context(conn, class_iri),
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
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };

    let property_iri_raw = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: property_iri".to_string()),
                concept: None,
            }
        }
    };
    let property_iri = normalize_comment_alias(property_iri_raw);

    let raw_values = match args.get("values").and_then(|v| v.as_array()) {
        Some(v) if !v.is_empty() => v,
        Some(_) => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("values array must not be empty".to_string()),
                concept: None,
            }
        }
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: values".to_string()),
                concept: None,
            }
        }
    };

    match (|| {
        if property_iri == "foundation:hasStatus" {
            if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                let class_iri = crate::owl::get_iri_property(conn, iri, "rdf:type")
                    .ok()
                    .flatten()
                    .ok_or_else(|| {
                        let msg = format!("Individual '{}' has no rdf:type", iri);
                        crate::owl::OwlError::NotFound(msg)
                    })?;
                crate::core_ontology::status::validate_allowed_status(
                    conn, &class_iri, status_iri,
                )?;
            }
        }

        let objects = build_objects(conn, property_iri, raw_values)?;
        if objects.is_empty() {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Property '{}' values array contains only null entries",
                property_iri
            )));
        }

        let individual = Individual::new(iri);
        individual.append_property(conn, property_iri, objects, "ai")?;

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        super::batch::queue_instance_recalc(iri.to_string(), property_iri.to_string());

        Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}))
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
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };

    let property_iri_raw = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: property_iri".to_string()),
                concept: None,
            }
        }
    };
    let property_iri = normalize_comment_alias(property_iri_raw);

    let raw_values = match args.get("values").and_then(|v| v.as_array()) {
        Some(v) => v,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: values".to_string()),
                concept: None,
            }
        }
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
                    .ok()
                    .flatten()
                    .ok_or_else(|| {
                        let msg = format!("Individual '{}' has no rdf:type", iri);
                        crate::owl::OwlError::NotFound(msg)
                    })?;
                crate::core_ontology::status::validate_allowed_status(
                    conn, &class_iri, status_iri,
                )?;
            }
        }

        let objects = build_objects(conn, property_iri, raw_values)?;
        if objects.is_empty() {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Property '{}' values array contains only null entries",
                property_iri
            )));
        }

        Individual::clear_property(conn, iri, property_iri, "ai")?;

        let individual = Individual::new(iri);
        individual.add_property(conn, property_iri, objects, "ai")?;

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        super::batch::queue_instance_recalc(iri.to_string(), property_iri.to_string());

        Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}))
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
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: property_iri".to_string()),
                concept: None,
            }
        }
    };

    let values_to_remove = match args.get("values").and_then(|v| v.as_array()) {
        Some(v) if !v.is_empty() => v,
        Some(_) => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("values array must not be empty".to_string()),
                concept: None,
            }
        }
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: values".to_string()),
                concept: None,
            }
        }
    };

    match (|| {
        let current_count = Individual::get_property_count(conn, iri, property_iri)?;
        let remove_count = values_to_remove.len();
        crate::owl::cardinality::validate_property_cardinality(
            conn,
            iri,
            property_iri,
            current_count.saturating_sub(remove_count),
        )?;

        // Collect class IRIs being removed from the domain before mutating.
        let domain_classes_removed: Vec<String> = if property_iri == "rdfs:domain" {
            values_to_remove
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        } else {
            Vec::new()
        };

        for val in values_to_remove {
            if let Some(val_str) = val.as_str() {
                Individual::remove_property_value(conn, iri, property_iri, val_str, "ai")?;
            }
        }

        // Cascade: retract all values of this property from instances of the removed domain class.
        for class_iri in &domain_classes_removed {
            let instances = crate::owl::find_entities_with_property(conn, "rdf:type", class_iri)
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
            for instance_iri in instances {
                Individual::clear_property(conn, &instance_iri, iri, "ai")?;
            }
        }

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));

        Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}))
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
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: property_iri".to_string()),
                concept: None,
            }
        }
    };

    match (|| {
        crate::owl::cardinality::validate_property_cardinality(conn, iri, property_iri, 0)?;
        Individual::clear_property(conn, iri, property_iri, "ai")?;
        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        Ok::<_, crate::owl::OwlError>(serde_json::json!({"iri": iri}))
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
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };

    match Individual::retract_collecting_with_summary(conn, iri, "ai") {
        Ok((all_retracted, cascade_summary)) => {
            crate::search::remove_from_index(&all_retracted);
            for retracted_iri in &all_retracted {
                super::batch::queue_event(
                    "entity-deleted",
                    serde_json::json!({"entityId": retracted_iri}),
                );
            }
            let message = if cascade_summary.is_empty() {
                format!("Individual '{}' retracted.", iri)
            } else {
                let parts: Vec<String> = cascade_summary
                    .iter()
                    .map(|(prop, dir, count)| format!("{} via {} ({})", count, prop, dir))
                    .collect();
                format!(
                    "Individual '{}' retracted. Cascade: {}.",
                    iri,
                    parts.join("; ")
                )
            };
            ToolResult {
                success: true,
                result: Some(serde_json::json!({"iri": iri, "message": message})),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }
}

pub fn restore_individual(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, restore_individual_one)
}

fn restore_individual_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };
    match Individual::restore(conn, iri, "ai") {
        Ok(()) => {
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
            ToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "iri": iri,
                    "message": format!("Individual '{}' restored.", iri),
                })),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }
}

pub fn restore_class(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, restore_class_one)
}

fn restore_class_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };
    match crate::owl::Class::restore(conn, iri, "ai") {
        Ok(count) => {
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
            ToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "iri": iri,
                    "message": format!("Class '{}' restored with {} instance(s).", iri, count),
                })),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }
}

pub fn restore_property(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, restore_property_one)
}

fn restore_property_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            }
        }
    };
    match crate::owl::Property::restore(conn, iri, "ai") {
        Ok(count) => {
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
            ToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "iri": iri,
                    "message": format!("Property '{}' restored with {} fact(s).", iri, count),
                })),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }
}

#[cfg(test)]
mod property_page_tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Triple};

    const SUBJECT: &str = "foundation:PaginationTestEntity";
    const PROPERTY: &str = "foundation:paginationTestValue";

    fn seed_values(conn: &mut Connection, values: &[String]) {
        let mut triples = vec![
            Triple::new(SUBJECT, "rdf:type", Object::Iri("owl:Thing".to_string())),
            Triple::new(
                SUBJECT,
                "rdfs:label",
                Object::Literal {
                    value: "Pagination test entity".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            ),
        ];
        triples.extend(values.iter().map(|value| {
            Triple::new(
                SUBJECT,
                PROPERTY,
                Object::Literal {
                    value: value.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            )
        }));
        store::assert_triples(conn, &triples, "pagination-test").unwrap();
    }

    fn first_property(result: &ToolResult) -> Value {
        result.result.as_ref().unwrap()["individuals"][0]["properties"][0].clone()
    }

    fn property(result: &ToolResult, property_iri: &str) -> Value {
        result.result.as_ref().unwrap()["individuals"][0]["properties"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["property"] == property_iri)
            .unwrap()
            .clone()
    }

    #[test]
    // Covers adapter pagination and snapshot stability only; it does not test replacement/removal semantics.
    fn describe_individual_paginates_all_values_at_one_stable_snapshot() {
        let mut conn = setup_test_db();
        let expected: Vec<String> = (0..12).map(|index| format!("value-{index:02}")).collect();
        seed_values(&mut conn, &expected);

        let initial = describe_individual(&conn, &serde_json::json!({ "iris": [SUBJECT] }));
        assert!(initial.success, "{:?}", initial.error);
        let first = first_property(&initial);
        assert_eq!(first["values"].as_array().unwrap().len(), 5);
        assert!(first["pagination"]["total"].is_null());
        assert_eq!(first["pagination"]["loaded"], 5);
        assert_eq!(first["pagination"]["has_more"], true);
        let snapshot_tx = first["pagination"]["snapshot_tx"].as_i64().unwrap();
        let mut cursor = first["pagination"]["cursor"].clone();

        seed_values(&mut conn, &["value-after-snapshot".to_string()]);
        let mut values: Vec<String> = first["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value["value"].as_str().unwrap().to_string())
            .collect();

        loop {
            let page = describe_individual(
                &conn,
                &serde_json::json!({
                    "individual_iri": SUBJECT,
                    "property_iri": PROPERTY,
                    "snapshot_tx": snapshot_tx,
                    "cursor": cursor,
                }),
            );
            assert!(page.success, "{:?}", page.error);
            let result = page.result.unwrap();
            assert_eq!(result["pagination"]["snapshot_tx"], snapshot_tx);
            values.extend(
                result["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value["value"].as_str().unwrap().to_string()),
            );
            if !result["pagination"]["has_more"].as_bool().unwrap() {
                break;
            }
            cursor = result["pagination"]["cursor"].clone();
        }

        assert_eq!(values, expected);
    }

    #[test]
    fn property_continuation_rejects_snapshot_or_scope_mismatch() {
        let mut conn = setup_test_db();
        seed_values(
            &mut conn,
            &(0..7)
                .map(|index| format!("value-{index:02}"))
                .collect::<Vec<_>>(),
        );
        let first = describe_individual(&conn, &serde_json::json!({ "iris": [SUBJECT] }));
        let cursor = first_property(&first)["pagination"]["cursor"].clone();
        let snapshot_tx = cursor["snapshot_tx"].as_i64().unwrap();

        let mismatch = describe_individual(
            &conn,
            &serde_json::json!({
                "individual_iri": SUBJECT,
                "property_iri": PROPERTY,
                "snapshot_tx": snapshot_tx + 1,
                "cursor": cursor,
            }),
        );
        assert!(!mismatch.success);
        assert!(mismatch.error.unwrap().contains("snapshot_tx"));

        let mismatch = describe_individual(
            &conn,
            &serde_json::json!({
                "individual_iri": SUBJECT,
                "property_iri": "foundation:anotherProperty",
                "snapshot_tx": snapshot_tx,
                "cursor": first_property(&first)["pagination"]["cursor"],
            }),
        );
        assert!(!mismatch.success);
        assert!(mismatch.error.unwrap().contains("belong"));

        let mismatch = describe_individual(
            &conn,
            &serde_json::json!({
                "individual_iri": "foundation:anotherIndividual",
                "property_iri": PROPERTY,
                "snapshot_tx": snapshot_tx,
                "cursor": first_property(&first)["pagination"]["cursor"],
            }),
        );
        assert!(!mismatch.success);
        assert!(mismatch.error.unwrap().contains("belong"));
    }

    #[test]
    fn property_cursor_round_trips_unchanged_and_requires_value_tx() {
        let mut conn = setup_test_db();
        seed_values(
            &mut conn,
            &(0..7)
                .map(|index| format!("value-{index:02}"))
                .collect::<Vec<_>>(),
        );
        let first = describe_individual(&conn, &serde_json::json!({ "iris": [SUBJECT] }));
        let cursor = first_property(&first)["pagination"]["cursor"].clone();
        assert!(cursor["value_tx"].as_i64().is_some());

        let continuation = describe_individual(
            &conn,
            &serde_json::json!({
                "individual_iri": SUBJECT,
                "property_iri": PROPERTY,
                "snapshot_tx": cursor["snapshot_tx"],
                "cursor": cursor,
            }),
        );
        assert!(continuation.success, "{:?}", continuation.error);

        let mut incomplete_cursor = first_property(&first)["pagination"]["cursor"].clone();
        incomplete_cursor
            .as_object_mut()
            .unwrap()
            .remove("value_tx");
        let rejected = describe_individual(
            &conn,
            &serde_json::json!({
                "individual_iri": SUBJECT,
                "property_iri": PROPERTY,
                "snapshot_tx": incomplete_cursor["snapshot_tx"],
                "cursor": incomplete_cursor,
            }),
        );
        assert!(!rejected.success);
        assert!(rejected.error.unwrap().contains("value_tx"));

        let cursor_schema = crate::ai::functions::get_available_tools()
            .into_iter()
            .find(|tool| tool.name == "describe_individual")
            .unwrap()
            .parameters
            .into_iter()
            .find(|parameter| parameter.name == "cursor")
            .unwrap()
            .schema
            .unwrap();
        assert_eq!(cursor_schema["additionalProperties"], false);
        assert!(cursor_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "value_tx"));
        assert_eq!(cursor_schema["properties"]["value_tx"]["type"], "integer");
    }

    #[test]
    fn secret_property_has_same_mask_shape_on_initial_and_continuation_pages() {
        const SECRET_SUBJECT: &str = "foundation:PaginationSecretTestEntity";
        const SECRET_PROPERTY: &str = "foundation:paginationSecretTestValue";
        let mut conn = setup_test_db();
        let mut triples = vec![
            Triple::new(
                SECRET_SUBJECT,
                "rdf:type",
                Object::Iri("owl:Thing".to_string()),
            ),
            Triple::new(
                SECRET_SUBJECT,
                "rdfs:label",
                Object::Literal {
                    value: "Pagination secret test entity".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            ),
            Triple::new(
                SECRET_PROPERTY,
                "foundation:isSecret",
                Object::Boolean(true),
            ),
        ];
        triples.extend((0..7).map(|index| {
            Triple::new(
                SECRET_SUBJECT,
                SECRET_PROPERTY,
                Object::Literal {
                    value: format!("secret-value-{index}"),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            )
        }));
        store::assert_triples(&mut conn, &triples, "pagination-secret-test").unwrap();

        let initial = describe_individual(&conn, &serde_json::json!({ "iris": [SECRET_SUBJECT] }));
        assert!(initial.success, "{:?}", initial.error);
        let initial_values = property(&initial, SECRET_PROPERTY)["values"].clone();
        assert_eq!(
            initial_values,
            serde_json::json!([crate::core_ontology::property_configuration::SECRET_MASK])
        );

        let page = crate::owl::get_property_values_page(
            &conn,
            SECRET_SUBJECT,
            SECRET_PROPERTY,
            5,
            property(&initial, SECRET_PROPERTY)["pagination"]["snapshot_tx"]
                .as_i64()
                .unwrap(),
            None,
        )
        .unwrap();
        let cursor = describe_property_cursor(
            page.next_cursor.as_ref().unwrap(),
            SECRET_SUBJECT,
            SECRET_PROPERTY,
        );
        let continuation = describe_individual(
            &conn,
            &serde_json::json!({
                "individual_iri": SECRET_SUBJECT,
                "property_iri": SECRET_PROPERTY,
                "snapshot_tx": page.snapshot_tx,
                "cursor": cursor,
            }),
        );
        assert!(continuation.success, "{:?}", continuation.error);
        let continuation_result = continuation.result.unwrap();
        assert_eq!(continuation_result["values"], initial_values);
    }
}

#[cfg(test)]
#[path = "individual_tests/mod.rs"]
mod tests;
