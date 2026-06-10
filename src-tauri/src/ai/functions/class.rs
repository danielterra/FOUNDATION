use serde_json::Value;
use rusqlite::Connection;
use crate::owl::Class;
use super::ToolResult;

#[cfg(test)]
mod tests {
    include!("class_tests.rs");
}

pub(super) fn load_class_context(conn: &Connection, iri: &str) -> Option<Value> {
    describe_class_one(conn, &serde_json::json!({ "iri": iri })).result
}

pub fn describe_class(conn: &Connection, args: &Value) -> ToolResult {
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
            let r = describe_class_one(conn, &serde_json::json!({ "iri": iri }));
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
        result: Some(serde_json::json!({ "classes": results })),
        error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
        concept: None,
    }
}

fn describe_class_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
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
            let status_iris = crate::owl::get_all_iri_properties(
                conn, iri, "foundation:allowedStatus",
            )?;
            status_iris.iter()
                .map(|status_iri| {
                    let thing = crate::owl::Thing::get(conn, status_iri);
                    let (icon, color) = crate::core_ontology::status::resolve_status_appearance(conn, status_iri);
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
            let restrictions =
                crate::owl::cardinality::get_class_cardinality_restrictions(conn, iri)?;
            restrictions.into_iter()
                .filter(|r| r.is_required())
                .map(|r| {
                    let label = crate::owl::get_literal_property(
                        conn, &r.property_iri, "rdfs:label",
                    )
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
                    let domain_iris = crate::owl::get_all_iri_properties(
                        conn, prop_iri, "rdfs:domain",
                    )
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

        let concept_properties: Vec<serde_json::Value> = concept.concept_properties.iter()
            .map(|(predicate, value_obj)| {
                let label = crate::owl::get_literal_property(conn, predicate, "rdfs:label")
                    .ok()
                    .flatten();
                let value = if value_obj.is_iri() {
                    value_obj.as_iri().unwrap_or("").to_string()
                } else {
                    value_obj.as_literal().unwrap_or_default()
                };
                serde_json::json!({
                    "property": predicate,
                    "label": label,
                    "value": value,
                    "isObjectProperty": value_obj.is_iri(),
                })
            })
            .collect();

        let applicable_automations = super::find_automations_for_types(conn, &[iri.to_string()]);
        let related_processes = super::find_related_processes_for_class(conn, iri);
        let is_locked = crate::owl::is_system_locked(conn, iri);

        let properties_array: Vec<serde_json::Value> = concept.properties.iter()
            .map(|(prop_iri, source)| {
                let prop = crate::owl::Property::get(conn, prop_iri).ok().flatten();
                let label = prop.as_ref().and_then(|p| p.label.clone());
                let ai_behavior_rules = prop.as_ref().and_then(|p| p.ai_behavior_rules.clone());
                let is_object_property = prop.as_ref().map(|p|
                    p.property_type == crate::owl::PropertyType::ObjectProperty
                ).unwrap_or(false);
                let property_type_str = prop.as_ref()
                    .map(|p| p.classification().as_str())
                    .unwrap_or("value");
                let range: Vec<serde_json::Value> = prop.as_ref()
                    .map(|p| p.ranges.iter().map(|r| {
                        let range_label = crate::owl::Thing::get(conn, r).label;
                        serde_json::json!({"iri": r, "label": range_label})
                    }).collect())
                    .unwrap_or_default();
                serde_json::json!({
                    "property": prop_iri,
                    "label": label,
                    "source": source,
                    "isObjectProperty": is_object_property,
                    "propertyType": property_type_str,
                    "range": range,
                    "aiBehaviorRules": ai_behavior_rules,
                })
            })
            .collect();

        let mut response = serde_json::json!({
            "iri": concept.iri,
            "label": concept.label,
            "icon": concept.icon,
            "comment": concept.comment,
            "isSystemLocked": is_locked,
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
            "disjointWith": concept.disjoint_with.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
                "icon": t.icon,
            })).collect::<Vec<_>>(),
            "properties": properties_array,
            "instanceCount": concept.backlinks.len(),
            "allowedStatuses": allowed_statuses,
            "requiredFields": required_fields,
            "incomingProperties": incoming_properties,
            "classProperties": concept_properties,
        });

        if !allowed_values.is_empty() {
            response["allowedValues"] = serde_json::json!(allowed_values);
        }
        if !applicable_automations.is_empty() {
            response["applicableAutomations"] = serde_json::json!(applicable_automations);
        }
        if !related_processes.is_empty() {
            response["relatedProcesses"] = serde_json::json!(related_processes);
        }

        {
            use crate::eavto::query;
            let mut cascade_rules: Vec<serde_json::Value> = Vec::new();
            for (predicate, direction) in &[
                ("foundation:cascadeDeleteRange", "range"),
                ("foundation:cascadeDeleteDomain", "domain"),
            ] {
                let triples = query::get_by_entity_predicate(conn, iri, predicate)
                    .map(|r| r.triples).unwrap_or_default();
                for t in triples {
                    if let Some(prop_iri) = t.object.as_iri() {
                        cascade_rules.push(serde_json::json!({
                            "property_iri": prop_iri,
                            "direction": direction,
                        }));
                    }
                }
            }
            if !cascade_rules.is_empty() {
                response["cascade_rules"] = serde_json::json!(cascade_rules);
            }
        }

        Ok::<_, crate::owl::OwlError>(response)
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

pub fn define_class(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, define_class_one)
}

fn define_class_one(
    conn: &mut Connection,
    args: &Value,
) -> ToolResult {
    let orig_iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    match (|| {
        let new_iri_arg = args.get("new_iri").and_then(|v| v.as_str());
        let iri: &str = if let Some(new_iri) = new_iri_arg {
            if new_iri != orig_iri {
                if Class::get(conn, orig_iri)?.is_none() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Class '{}' not found. Cannot rename a non-existent class.", orig_iri
                    )));
                }
                if Class::get(conn, new_iri)?.is_some() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Class '{}' already exists. Cannot rename to an existing IRI.", new_iri
                    )));
                }
                crate::eavto::store::rename_iri(conn, orig_iri, new_iri, "ai")
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

        let existing = Class::get(conn, iri)?;
        let is_new = existing.is_none();

        let label_arg = args.get("label").and_then(|v| v.as_str());
        let icon_arg = args.get("icon").and_then(|v| v.as_str());

        if is_new {
            let label = label_arg
                .ok_or_else(|| crate::owl::OwlError::ValidationError(
                    "Missing required parameter: label \
                     (required when creating a new class)".to_string()
                ))?;
            let icon = icon_arg
                .ok_or_else(|| crate::owl::OwlError::ValidationError(
                    "Missing required parameter: icon \
                     (required when creating a new class)".to_string()
                ))?;
            let concept = Class::new(iri);
            concept.assert(conn, crate::owl::ClassType::OwlClass, label, icon, None, "ai")?;
        } else {
            if let Some(label) = label_arg {
                Class::set_label(conn, iri, label, "ai")?;
            }
            if let Some(icon) = icon_arg {
                Class::set_icon(conn, iri, icon, "ai")?;
            }
        }

        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            Class::set_comment(conn, iri, comment, "ai")?;
        }

        let super_classes_val = args.get("super_classes");

        if is_new {
            let iris: Vec<&str> = super_classes_val
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            if iris.is_empty() {
                return Err(crate::owl::OwlError::ValidationError(
                    "Missing required parameter: super_classes \
                     (at least one superclass is required when creating a class)"
                        .to_string()
                ));
            }
            Class::validate_super_classes_not_disjoint(conn, &iris)?;
            Class::set_super_classes(conn, iri, &iris, "ai")?;
        } else if let Some(val) = super_classes_val {
            let iris: Vec<&str> = val
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            if iris.is_empty() {
                return Err(crate::owl::OwlError::ValidationError(
                    "super_classes must contain at least one superclass".to_string()
                ));
            }
            Class::validate_super_classes_not_disjoint(conn, &iris)?;
            Class::set_super_classes(conn, iri, &iris, "ai")?;
        }

        if let Some(disjoint_val) = args.get("disjoint_with") {
            let iris: Vec<&str> = disjoint_val
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            Class::set_disjoint_with(conn, iri, &iris, "ai")?;
        }

        if let Some(all_disjoint_val) = args.get("all_disjoint_classes") {
            let members: Vec<&str> = all_disjoint_val
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            if members.len() < 3 {
                return Err(crate::owl::OwlError::ValidationError(
                    "all_disjoint_classes requires at least 3 distinct class IRIs \
                     — for pairs use disjoint_with instead".to_string()
                ));
            }
            if !members.contains(&iri) {
                return Err(crate::owl::OwlError::ValidationError(format!(
                    "all_disjoint_classes must include the class being defined ('{}')",
                    iri,
                )));
            }
            Class::assert_all_disjoint_classes(conn, &members, "ai")?;
        }

        if let Some(allowed_statuses) = args.get("allowed_statuses").and_then(|v| v.as_array()) {
            let status_iris: Vec<&str> = allowed_statuses.iter()
                .filter_map(|v| v.as_str())
                .collect();

            for status_iri in &status_iris {
                let individual = crate::owl::Individual::get(conn, *status_iri)?;
                if individual.is_none() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Status '{}' does not exist. \
                         Use search to query existing Status instances \
                         before setting allowedStatuses.",
                        status_iri
                    )));
                }
                let (icon, _) = crate::core_ontology::status::resolve_status_appearance(conn, status_iri);
                if icon.is_none() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Status '{}' exists but has no icon. All statuses must have a valid icon.",
                        status_iri
                    )));
                }
            }

            crate::owl::replace_all_property_iris(
                conn, iri, "foundation:allowedStatus", &status_iris, "ai",
            )?;
        }

        if let Some(remove_properties) = args.get("remove_properties").and_then(|v| v.as_array()) {
            for item in remove_properties {
                if let Some(prop_iri) = item.as_str() {
                    if crate::owl::Property::get(conn, prop_iri)?.is_none() {
                        continue;
                    }
                    let domain_triple = crate::eavto::Triple::new(
                        prop_iri,
                        "rdfs:domain",
                        crate::eavto::Object::Iri(iri.to_string()),
                    );
                    crate::eavto::store::retract_triples(conn, &[domain_triple], "ai")?;
                    let existing =
                        crate::owl::cardinality::get_class_cardinality_restrictions(conn, iri)?;
                    let updated: Vec<crate::owl::cardinality::PropertyRestriction<'_>> =
                        existing.iter()
                        .filter(|r| r.property_iri != prop_iri)
                        .map(|r| crate::owl::cardinality::PropertyRestriction {
                            property_iri: r.property_iri.as_str(),
                            min: r.min,
                            max: r.max,
                        })
                        .collect();
                    crate::owl::cardinality::set_class_cardinality_restrictions(
                        conn, iri, &updated, "ai",
                    )?;
                    retract_cascade_rule(conn, iri, prop_iri, "ai")?;
                    super::batch::queue_event(
                        "entity-updated",
                        serde_json::json!({"entityId": prop_iri}),
                    );
                }
            }
        }

        if let Some(cascade_rules) = args.get("cascade_rules").and_then(|v| v.as_array()) {
            use crate::eavto::{store, query, Triple, Object};

            let range_triples = query::get_by_entity_predicate(
                conn, iri, "foundation:cascadeDeleteRange",
            )
                .map(|r| r.triples).unwrap_or_default();
            let domain_triples = query::get_by_entity_predicate(
                conn, iri, "foundation:cascadeDeleteDomain",
            )
                .map(|r| r.triples).unwrap_or_default();
            let to_retract: Vec<Triple> = range_triples.into_iter().chain(domain_triples)
                .map(|t| Triple::new(t.subject, t.predicate, t.object))
                .collect();
            if !to_retract.is_empty() {
                store::retract_triples(conn, &to_retract, "ai")?;
            }

            let mut new_triples = Vec::new();
            for entry in cascade_rules {
                let prop_iri = entry.get("property_iri").and_then(|v| v.as_str())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Each cascade_rules entry must have 'property_iri'".to_string()
                    ))?;
                let direction = entry.get("direction").and_then(|v| v.as_str())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Each cascade_rules entry must have 'direction' \
                         ('range' or 'domain')".to_string()
                    ))?;
                let predicate = match direction {
                    "range" => "foundation:cascadeDeleteRange",
                    "domain" => "foundation:cascadeDeleteDomain",
                    other => return Err(crate::owl::OwlError::ValidationError(format!(
                        "Invalid direction '{}'. Must be 'range' or 'domain'", other
                    ))),
                };
                new_triples.push(Triple::new(iri, predicate, Object::Iri(prop_iri.to_string())));
            }
            if !new_triples.is_empty() {
                store::assert_triples(conn, &new_triples, "ai")?;
            }
        }

        if let Some(details) = args.get("add_properties").and_then(|v| v.as_array()) {
            for detail in details {
                let prop_iri = detail.as_str()
                    .or_else(|| detail.get("iri").and_then(|v| v.as_str()))
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Each add_properties item must be a property IRI string".to_string()
                    ))?;

                let mut prop = crate::owl::Property::get(conn, prop_iri)?
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        format!(
                            "Property '{}' not found. Define it first with define_property.",
                            prop_iri,
                        )
                    ))?;

                if !prop.domains.contains(&iri.to_string()) {
                    prop.domains.push(iri.to_string());
                    let domains: Vec<&str> = prop.domains.iter().map(|s| s.as_str()).collect();
                    prop.assert(
                        conn,
                        prop.property_type,
                        prop.label.as_deref().unwrap_or(""),
                        None,
                        &domains,
                        prop.ranges.first().map(|s| s.as_str()),
                        prop.unit.as_deref(),
                        "ai",
                    )?;

                    if prop.formula.is_some() {
                        super::batch::queue_formula_recalc(prop_iri.to_string());
                    }
                }

                super::batch::queue_event(
                    "entity-updated",
                    serde_json::json!({"entityId": prop_iri}),
                );
                super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
            }
        }

        if let Some(property_restrictions) =
            args.get("property_restrictions").and_then(|v| v.as_array())
        {
            struct RestrictionEntry {
                prop_iri: String,
                min: Option<u32>,
                max: Option<u32>,
            }

            let mut entries: Vec<RestrictionEntry> = Vec::new();

            for entry in property_restrictions {
                let prop_iri = entry.get("property_iri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Each property_restrictions entry must have 'property_iri'".to_string()
                    ))?;

                let prop = crate::owl::Property::get(conn, prop_iri)?;
                let is_valid = prop.map(|p| matches!(
                    p.property_type,
                    crate::owl::PropertyType::ObjectProperty
                        | crate::owl::PropertyType::DatatypeProperty
                )).unwrap_or(false);
                if !is_valid {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Property '{}' is not defined in this ontology",
                        prop_iri
                    )));
                }

                entries.push(RestrictionEntry {
                    prop_iri: prop_iri.to_string(),
                    min: entry.get("cardinality_min").and_then(|v| v.as_u64()).map(|v| v as u32),
                    max: entry.get("cardinality_max").and_then(|v| v.as_u64()).map(|v| v as u32),
                });
            }

            let restrictions: Vec<crate::owl::cardinality::PropertyRestriction<'_>> = entries.iter()
                .map(|e| crate::owl::cardinality::PropertyRestriction {
                    property_iri: e.prop_iri.as_str(),
                    min: e.min,
                    max: e.max,
                })
                .collect();

            crate::owl::cardinality::set_class_cardinality_restrictions(
                conn, iri, &restrictions, "ai",
            )?;
        }

        if is_new {
            super::batch::queue_event("entity-created", serde_json::json!({"entityId": iri}));
        } else {
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": iri,
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
            result: None,
            error: Some(e.to_string()),
            concept: load_class_context(conn, orig_iri),
        },
    }
}

pub fn retract_class(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, retract_class_one)
}

fn retract_class_one(
    conn: &mut Connection,
    args: &Value,
) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    match (|| {
        if let Some(class) = Class::get(conn, iri)? {
            let child_iris: Vec<String> = class.sub_classes.iter()
                .map(|t| t.iri.clone())
                .filter(|s| !s.starts_with("_:"))
                .collect();
            if !child_iris.is_empty() {
                return Err(crate::owl::OwlError::ValidationError(format!(
                    "Cannot retract class '{}': it has {} subclass(es) that depend on it: {}. \
                     Remove the superclass reference from each subclass first.",
                    iri, child_iris.len(), child_iris.join(", ")
                )));
            }
        }

        let instance_iris: Vec<String> = {
            let result = crate::eavto::query::get_by_predicate_object(conn, "rdf:type", iri)
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
            result.triples.into_iter().map(|t| t.subject).collect()
        };

        let deleted_instances = instance_iris.len();

        Class::retract_all(conn, iri, "ai")?;

        for instance_iri in &instance_iris {
            crate::owl::Individual::retract(conn, instance_iri, "ai")?;
            super::batch::queue_event(
                "entity-updated",
                serde_json::json!({"entityId": instance_iri}),
            );
        }

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "deleted_instances": deleted_instances,
            "message": format!("Class '{}' retracted with {} instance(s).", iri, deleted_instances),
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
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }
}

fn retract_cascade_rule(
    conn: &mut Connection,
    class_iri: &str,
    prop_iri: &str,
    origin: &str,
) -> Result<(), crate::owl::OwlError> {
    use crate::eavto::{store, query, Triple};
    for predicate in &["foundation:cascadeDeleteRange", "foundation:cascadeDeleteDomain"] {
        let triples = query::get_by_entity_predicate(conn, class_iri, predicate)
            .map(|r| r.triples).unwrap_or_default();
        let to_retract: Vec<Triple> = triples.into_iter()
            .filter(|t| t.object.as_iri() == Some(prop_iri))
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        if !to_retract.is_empty() {
            store::retract_triples(conn, &to_retract, origin)?;
        }
    }
    Ok(())
}
