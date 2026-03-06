use serde_json::Value;
use crate::eavto::Connection;
use tauri::Emitter;
use crate::owl::{Class, Individual, Object};
use super::ToolResult;
use std::sync::atomic::{AtomicU64, Ordering};


const SCORE_LABEL_MATCH: usize = 3;
const SCORE_COMMENT_MATCH: usize = 2;
const SCORE_DETAIL_MATCH: usize = 1;

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

pub fn search_things(conn: &Connection, args: &Value) -> ToolResult {
    super::batch::run_multi_read(conn, args, search_things_one)
}

fn search_things_one(conn: &Connection, args: &Value) -> ToolResult {
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

    let from_millis = args.get("from_millis").and_then(|v| v.as_i64());
    let to_millis = args.get("to_millis").and_then(|v| v.as_i64());
    let include_retracted = args.get("include_retracted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let use_extended_query = from_millis.is_some() || to_millis.is_some() || include_retracted;

    match (|| {
        let thing_iris = if let Some(concept_iri) = concept_iri_opt {
            if use_extended_query {
                Individual::find_by_class_with_date_range(
                    conn, concept_iri, from_millis, to_millis, include_retracted,
                )?
            } else {
                Class::get_instances(conn, concept_iri)?
            }
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
                            match_count += SCORE_LABEL_MATCH;
                        } else if comment_lower.contains(token) {
                            match_count += SCORE_COMMENT_MATCH;
                        } else if detail_text.contains(token) {
                            match_count += SCORE_DETAIL_MATCH;
                        }
                    }

                    if match_count == 0 {
                        continue;
                    }

                    match_count
                } else {
                    0
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
    super::batch::run_multi_read(conn, args, get_thing_one)
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

        if include_retracted {
            let retracted_triples = crate::eavto::query::get_retracted_by_entity(conn, iri)
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
            for triple in &retracted_triples.triples {
                if triple.predicate == "rdfs:label"
                    || triple.predicate == "rdfs:comment"
                    || triple.predicate == "foundation:icon"
                    || triple.predicate == "foundation:hasIcon"
                {
                    continue;
                }
                let value: serde_json::Value = match &triple.object {
                    crate::eavto::Object::Iri(s) | crate::eavto::Object::Blank(s) => serde_json::json!(s),
                    crate::eavto::Object::Literal { value: v, .. } => serde_json::json!(v),
                    crate::eavto::Object::Integer(i) => serde_json::json!(i),
                    crate::eavto::Object::Number(n) => serde_json::json!(n),
                    crate::eavto::Object::Boolean(b) => serde_json::json!(b),
                    crate::eavto::Object::DateTime(dt) => serde_json::json!(dt),
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

pub fn create_thing(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, create_thing_one)
}

fn create_thing_one(
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
    let generated_iri = format!("foundation:{}_{}", concept_name, next_iri_id());

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

        if let Some(properties) = args.get("properties").and_then(|v| v.as_array()) {
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

                let value_type = prop_entry.get("value_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("literal");
                let datatype = prop_entry.get("datatype")
                    .and_then(|v| v.as_str())
                    .unwrap_or("xsd:string");

                if detail_iri == "foundation:hasStatus" {
                    if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                        crate::owl::validate_allowed_status(conn, concept_iri, status_iri)?;
                    }
                }

                let objects: Vec<Object> = raw_values.iter()
                    .filter_map(|v| v.as_str())
                    .map(|value| {
                        if value_type == "iri" {
                            Object::Iri(value.to_string())
                        } else {
                            Object::Literal {
                                value: value.to_string(),
                                datatype: Some(datatype.to_string()),
                                language: None,
                            }
                        }
                    })
                    .collect();

                if objects.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        format!("Property '{}' values contain no valid string entries", detail_iri)
                    ));
                }

                individual.add_property(conn, detail_iri, objects, "ai")?;
            }
        }

        // Validate all required fields were provided
        let restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, concept_iri)?;
        let required: Vec<&str> = restrictions.iter()
            .filter(|r| r.is_required())
            .map(|r| r.property_iri.as_str())
            .collect();
        if !required.is_empty() {
            let provided: std::collections::HashSet<&str> = args.get("properties")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|p| p.get("detail_iri").and_then(|v| v.as_str()))
                    .collect())
                .unwrap_or_default();
            for prop_iri in &required {
                if !provided.contains(*prop_iri) {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Required field '{}' must be provided when creating an instance of '{}'",
                        prop_iri, concept_iri,
                    )));
                }
            }
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
    super::batch::run_atomic(conn, args, app, update_thing_one)
}

fn update_thing_one(
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
        let mut updated_fields: Vec<String> = Vec::new();

        let individual = Individual::new(iri);

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

        let mut iri_values_emitted: Vec<String> = Vec::new();

        if let Some(properties) = args.get("properties").and_then(|v| v.as_array()) {
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

                let value_type = prop_entry.get("value_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("literal");
                let datatype = prop_entry.get("datatype")
                    .and_then(|v| v.as_str())
                    .unwrap_or("xsd:string");

                if detail_iri == "foundation:hasStatus" {
                    if let Some(status_iri) = raw_values.first().and_then(|v| v.as_str()) {
                        if let Some(ref concept) = concept_iri {
                            crate::owl::validate_allowed_status(conn, concept, status_iri)?;
                        }
                    }
                }

                let objects: Vec<Object> = raw_values.iter()
                    .filter_map(|v| v.as_str())
                    .map(|value| {
                        if value_type == "iri" {
                            Object::Iri(value.to_string())
                        } else {
                            Object::Literal {
                                value: value.to_string(),
                                datatype: Some(datatype.to_string()),
                                language: None,
                            }
                        }
                    })
                    .collect();

                if objects.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        format!("Property '{}' values contain no valid string entries", detail_iri)
                    ));
                }

                if value_type == "iri" {
                    for v in raw_values.iter().filter_map(|v| v.as_str()) {
                        iri_values_emitted.push(v.to_string());
                    }
                }

                individual.add_property(conn, detail_iri, objects, "ai")?;
                updated_fields.push(detail_iri.to_string());
            }
        }

        if let Some(app_handle) = app {
            app_handle.emit("entity-updated", serde_json::json!({"entityId": iri})).ok();
            for ref_iri in &iri_values_emitted {
                app_handle.emit("entity-updated", serde_json::json!({"entityId": ref_iri})).ok();
            }
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
    super::batch::run_multi_read(conn, args, find_things_by_detail_one)
}

fn find_things_by_detail_one(conn: &Connection, args: &Value) -> ToolResult {
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

    let include_retracted = args.get("include_retracted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let limit = args.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;

    let offset = args.get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    match (|| {
        let mut detail_constraints: Vec<(String, String, String)> = Vec::new();
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
            let operator = prop.get("operator")
                .and_then(|v| v.as_str())
                .unwrap_or("=");

            detail_constraints.push((detail_iri.to_string(), value.to_string(), operator.to_string()));
        }

        let constraint_refs: Vec<(&str, &str, &str)> = detail_constraints
            .iter()
            .map(|(d, v, o)| (d.as_str(), v.as_str(), o.as_str()))
            .collect();

        let (paginated_iris, total) = Individual::find_by_class_and_properties_with_options(
            conn, concept_iri, &constraint_refs, include_retracted, limit, offset,
        )?;

        let mut results = Vec::new();
        for iri in paginated_iris {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::eavto::{store, Triple, Object};
    use crate::owl::{Class, ClassType, Individual, Property, PropertyType};

    fn setup_task_class_with_statuses(conn: &mut Connection) {
        let task_class = Class::new("foundation:Task");
        task_class.assert(conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test").unwrap();

        let triples = vec![
            Triple::new("foundation:ActiveStatus", "rdf:type", Object::Iri("foundation:Status".to_string())),
            Triple::new("foundation:ActiveStatus", "rdfs:label", Object::Literal {
                value: "Active".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:DoneStatus", "rdf:type", Object::Iri("foundation:Status".to_string())),
            Triple::new("foundation:DoneStatus", "rdfs:label", Object::Literal {
                value: "Done".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:ActiveStatus".to_string())),
            Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:DoneStatus".to_string())),
        ];
        store::assert_triples(conn, &triples, "test").unwrap();

        Property::new("foundation:priority")
            .assert(conn, PropertyType::DatatypeProperty, "priority", None, &["foundation:Task"], Some("xsd:string"), None, "test")
            .unwrap();

        Property::new("foundation:hasStatus")
            .assert(conn, PropertyType::ObjectProperty, "hasStatus", None, &["foundation:Task"], None, None, "test")
            .unwrap();
    }

    fn create_task(conn: &mut Connection, iri: &str) {
        let individual = Individual::new(iri);
        individual.assert(conn, "foundation:Task", "Test Task", "https://example.com/icon.svg", "test").unwrap();
    }

    #[test]
    fn test_update_thing_with_properties_updates_literal_property() {
        let mut conn = setup_test_db();
        setup_task_class_with_statuses(&mut conn);
        create_task(&mut conn, "foundation:Task_001");

        let args = serde_json::json!({
            "iri": "foundation:Task_001",
            "properties": [
                {
                    "detail_iri": "foundation:priority",
                    "values": ["High"],
                    "value_type": "literal",
                    "datatype": "xsd:string"
                }
            ]
        });

        let result = update_thing_one(&mut conn, &args, None);
        assert!(result.success, "update_thing should succeed: {:?}", result.error);
        let response = result.result.unwrap();
        let updated = response["updatedFields"].as_array().unwrap();
        assert!(
            updated.iter().any(|v| v == "foundation:priority"),
            "Should report foundation:priority as updated"
        );
    }

    #[test]
    fn test_update_thing_with_valid_status_succeeds() {
        let mut conn = setup_test_db();
        setup_task_class_with_statuses(&mut conn);
        create_task(&mut conn, "foundation:Task_002");

        let args = serde_json::json!({
            "iri": "foundation:Task_002",
            "properties": [
                {
                    "detail_iri": "foundation:hasStatus",
                    "values": ["foundation:ActiveStatus"],
                    "value_type": "iri"
                }
            ]
        });

        let result = update_thing_one(&mut conn, &args, None);
        assert!(result.success, "update_thing with valid status should succeed: {:?}", result.error);
    }

    #[test]
    fn test_update_thing_with_invalid_status_returns_descriptive_error() {
        let mut conn = setup_test_db();
        setup_task_class_with_statuses(&mut conn);
        create_task(&mut conn, "foundation:Task_003");

        let args = serde_json::json!({
            "iri": "foundation:Task_003",
            "properties": [
                {
                    "detail_iri": "foundation:hasStatus",
                    "values": ["foundation:InvalidStatus"],
                    "value_type": "iri"
                }
            ]
        });

        let result = update_thing_one(&mut conn, &args, None);
        assert!(!result.success, "update_thing with invalid status should fail");
        let error = result.error.unwrap();
        assert!(
            error.contains("foundation:InvalidStatus"),
            "Error should mention the invalid status: {}", error
        );
        assert!(
            error.contains("Allowed") || error.contains("allowed"),
            "Error should list allowed statuses: {}", error
        );
    }

    #[test]
    fn test_update_thing_partial_update_with_only_label() {
        let mut conn = setup_test_db();
        setup_task_class_with_statuses(&mut conn);
        create_task(&mut conn, "foundation:Task_004");

        let args = serde_json::json!({
            "iri": "foundation:Task_004",
            "label": "Updated Task Name"
        });

        let result = update_thing_one(&mut conn, &args, None);
        assert!(result.success, "Partial update with only label should succeed: {:?}", result.error);
        let response = result.result.unwrap();
        let updated = response["updatedFields"].as_array().unwrap();
        assert_eq!(updated.len(), 1, "Only label should be reported as updated");
        assert_eq!(updated[0], "label");
    }
}
