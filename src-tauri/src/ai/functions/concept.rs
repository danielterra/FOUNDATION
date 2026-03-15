use std::pin::Pin;
use std::future::Future;
use serde_json::Value;
use turso::Connection;
use crate::owl::Class;
use super::ToolResult;

#[cfg(test)]
mod tests {
    include!("concept_tests.rs");
}

pub(super) async fn load_concept_context(conn: &Connection, iri: &str) -> Option<Value> {
    get_concept_one(conn, &serde_json::json!({ "iri": iri })).await.result
}

pub async fn get_concepts(conn: &Connection, args: &Value) -> ToolResult {
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
            let r = get_concept_one(conn, &serde_json::json!({ "iri": iri })).await;
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
        result: Some(serde_json::json!({ "concepts": results })),
        error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
        concept: None,
    }
}

async fn get_concept_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    match (|| async move {
        let concept = Class::get(conn, iri).await?
            .ok_or_else(|| crate::owl::OwlError::NotFound(iri.to_string()))?;

        let allowed_values: Vec<serde_json::Value> = {
            let mut vals = Vec::new();
            for value_iri in &concept.one_of_values {
                let label = crate::owl::get_literal_property(conn, value_iri, "rdfs:label").await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| value_iri.clone());
                vals.push(serde_json::json!({ "iri": value_iri, "label": label }));
            }
            vals
        };

        let allowed_statuses: Vec<serde_json::Value> = {
            let status_iris = crate::owl::get_all_iri_properties(conn, iri, "foundation:allowedStatus").await?;
            let mut statuses = Vec::new();
            for status_iri in &status_iris {
                let thing = crate::owl::Thing::get(conn, status_iri).await;
                let (icon, color) = crate::owl::resolve_status_appearance(conn, status_iri).await;
                statuses.push(serde_json::json!({
                    "iri": status_iri,
                    "label": thing.label,
                    "icon": icon,
                    "color": color,
                }));
            }
            statuses
        };

        let required_fields: Vec<serde_json::Value> = {
            let restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, iri).await?;
            let mut fields = Vec::new();
            for r in restrictions.into_iter().filter(|r| r.is_required()) {
                let label = crate::owl::get_literal_property(conn, &r.property_iri, "rdfs:label").await
                    .ok()
                    .flatten();
                fields.push(serde_json::json!({ "property": r.property_iri, "label": label }));
            }
            fields
        };

        let incoming_properties: Vec<serde_json::Value> = {
            use crate::eavto::query;
            let result = query::get_by_predicate_object(conn, "rdfs:range", iri).await
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
            let mut props = Vec::new();
            for t in &result.triples {
                let prop_iri = &t.subject;
                let label = crate::owl::get_literal_property(conn, prop_iri, "rdfs:label").await
                    .ok()
                    .flatten();
                let domain_iris = crate::owl::get_all_iri_properties(conn, prop_iri, "rdfs:domain").await
                    .unwrap_or_default();
                let mut domains = Vec::new();
                for d in &domain_iris {
                    let d_label = crate::owl::Thing::get(conn, d).await.label;
                    domains.push(serde_json::json!({"iri": d, "label": d_label}));
                }
                props.push(serde_json::json!({
                    "property": prop_iri,
                    "label": label,
                    "domains": domains,
                }));
            }
            props
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
    })().await {
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

pub async fn learn_concept(
    conn: &Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, learn_concept_one).await
}

fn learn_concept_one<'a>(
    conn: &'a Connection,
    args: &'a Value,
) -> Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>> {
    Box::pin(async move {
        let orig_iri = match args.get("iri").and_then(|v| v.as_str()) {
            Some(iri) => iri,
            None => return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            },
        };

        match (|| async move {
            let new_iri_arg = args.get("new_iri").and_then(|v| v.as_str());
            let iri: &str = if let Some(new_iri) = new_iri_arg {
                if new_iri != orig_iri {
                    if Class::get(conn, orig_iri).await?.is_none() {
                        return Err(crate::owl::OwlError::ValidationError(format!(
                            "Concept '{}' not found. Cannot rename a non-existent concept.", orig_iri
                        )));
                    }
                    if Class::get(conn, new_iri).await?.is_some() {
                        return Err(crate::owl::OwlError::ValidationError(format!(
                            "Concept '{}' already exists. Cannot rename to an existing IRI.", new_iri
                        )));
                    }
                    crate::eavto::store::rename_iri(conn, orig_iri, new_iri, "ai").await
                        .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
                    super::batch::queue_event(
                        "entity-updated",
                        serde_json::json!({"entityId": orig_iri}),
                    );
                    new_iri
                } else {
                    orig_iri
                }
            } else {
                orig_iri
            };

            let existing = Class::get(conn, iri).await?;
            let is_new = existing.is_none();

            let label_arg = args.get("label").and_then(|v| v.as_str());
            let icon_arg = args.get("icon").and_then(|v| v.as_str());

            let needs_assert = is_new || label_arg.is_some() || icon_arg.is_some();

            if needs_assert {
                let label = label_arg
                    .or_else(|| existing.as_ref().and_then(|c| c.label.as_deref()))
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Missing required parameter: label (required when creating a new concept)".to_string()
                    ))?;
                let icon = icon_arg
                    .or_else(|| existing.as_ref().and_then(|c| c.icon.as_deref()))
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Missing required parameter: icon (required when creating a new concept)".to_string()
                    ))?;
                let concept = Class::new(iri);
                concept.assert(conn, crate::owl::ClassType::OwlClass, label, icon, None, "ai").await?;
            }

            if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
                Class::set_comment(conn, iri, comment, "ai").await?;
            }

            let super_concepts_val = args.get("super_concepts");

            if is_new {
                let iris: Vec<&str> = super_concepts_val
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                if iris.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        "Missing required parameter: super_concepts (at least one superclass is required when creating a concept)".to_string()
                    ));
                }
                Class::set_super_classes(conn, iri, &iris, "ai").await?;
            } else if let Some(val) = super_concepts_val {
                let iris: Vec<&str> = val
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                if iris.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(
                        "super_concepts must contain at least one superclass".to_string()
                    ));
                }
                Class::set_super_classes(conn, iri, &iris, "ai").await?;
            }

            if let Some(allowed_statuses) = args.get("allowed_statuses").and_then(|v| v.as_array()) {
                let status_iris: Vec<&str> = allowed_statuses.iter()
                    .filter_map(|v| v.as_str())
                    .collect();

                for status_iri in &status_iris {
                    let individual = crate::owl::Individual::get(conn, *status_iri).await?;
                    if individual.is_none() {
                        return Err(crate::owl::OwlError::ValidationError(format!(
                            "Status '{}' does not exist. Use remember_concepts to query existing Status instances before setting allowedStatuses.",
                            status_iri
                        )));
                    }
                    let (icon, _) = crate::owl::resolve_status_appearance(conn, status_iri).await;
                    if icon.is_none() {
                        return Err(crate::owl::OwlError::ValidationError(format!(
                            "Status '{}' exists but has no icon. All statuses must have a valid icon.",
                            status_iri
                        )));
                    }
                }

                crate::owl::replace_all_property_iris(
                    conn, iri, "foundation:allowedStatus", &status_iris, "ai",
                ).await?;
            }

            if let Some(remove_details) = args.get("remove_details").and_then(|v| v.as_array()) {
                for item in remove_details {
                    if let Some(prop_iri) = item.as_str() {
                        let mut prop = match crate::owl::Property::get(conn, prop_iri).await? {
                            Some(p) => p,
                            None => continue,
                        };
                        prop.domains.retain(|d| d != iri);
                        if prop.domains.is_empty() {
                            crate::owl::Property::retract(conn, prop_iri, "ai").await?;
                        } else {
                            let domains: Vec<&str> = prop.domains.iter().map(|s| s.as_str()).collect();
                            prop.assert(conn, prop.property_type, prop.label.as_deref().unwrap_or(""), None, &domains, prop.ranges.first().map(|s| s.as_str()), prop.unit.as_deref(), "ai").await?;
                        }
                        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": prop_iri}));
                    }
                }
            }

            if let Some(details) = args.get("upsert_details").and_then(|v| v.as_array()) {
                for detail in details {
                    let prop_iri = detail.as_str()
                        .or_else(|| detail.get("iri").and_then(|v| v.as_str()))
                        .ok_or_else(|| crate::owl::OwlError::ValidationError(
                            "Each upsert_details item must be a property IRI string".to_string()
                        ))?;

                    let mut prop = crate::owl::Property::get(conn, prop_iri).await?
                        .ok_or_else(|| crate::owl::OwlError::ValidationError(
                            format!("Property '{}' not found. Define it first with learn_properties.", prop_iri)
                        ))?;

                    if !prop.domains.contains(&iri.to_string()) {
                        prop.domains.push(iri.to_string());
                        let domains: Vec<&str> = prop.domains.iter().map(|s| s.as_str()).collect();
                        prop.assert(conn, prop.property_type, prop.label.as_deref().unwrap_or(""), None, &domains, prop.ranges.first().map(|s| s.as_str()), prop.unit.as_deref(), "ai").await?;
                    }

                    super::batch::queue_event("entity-updated", serde_json::json!({"entityId": prop_iri}));
                    super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
                }
            }

            if let Some(required_fields) = args.get("required_fields").and_then(|v| v.as_array()) {
                let prop_iris: Vec<&str> = required_fields.iter()
                    .filter_map(|v| v.as_str())
                    .collect();

                for prop_iri in &prop_iris {
                    let prop = crate::owl::Property::get(conn, *prop_iri).await?;
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

                crate::owl::cardinality::set_class_required_fields(conn, iri, &prop_iris, "ai").await?;
            }

            if is_new {
                super::batch::queue_event("entity-created", serde_json::json!({"entityId": iri}));
            } else {
                super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
            }

            Ok::<_, crate::owl::OwlError>(serde_json::json!({
                "iri": iri,
            }))
        })().await {
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
                concept: load_concept_context(conn, orig_iri).await,
            },
        }
    })
}

pub async fn delete_concept(
    conn: &Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, delete_concept_one).await
}

fn delete_concept_one<'a>(
    conn: &'a Connection,
    args: &'a Value,
) -> Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>> {
    Box::pin(async move {
        let iri = match args.get("iri").and_then(|v| v.as_str()) {
            Some(iri) => iri,
            None => return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            },
        };

        match (|| async move {
            if let Some(class) = Class::get(conn, iri).await? {
                let child_iris: Vec<String> = class.sub_classes.iter()
                    .map(|t| t.iri.clone())
                    .filter(|s| !s.starts_with("_:"))
                    .collect();
                if !child_iris.is_empty() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Cannot delete concept '{}': it has {} subclass(es) that depend on it: {}. \
                         Remove the superclass reference from each subclass first.",
                        iri, child_iris.len(), child_iris.join(", ")
                    )));
                }
            }

            Class::retract_all(conn, iri, "ai").await?;

            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));

            Ok::<_, crate::owl::OwlError>(serde_json::json!({}))
        })().await {
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
    })
}
