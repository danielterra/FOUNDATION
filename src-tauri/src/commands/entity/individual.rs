use std::collections::HashMap;
use crate::owl::{self, Class, Individual, Property, Connection};
use super::{EntityData, PropertyValue, GraphNode, GraphLink, StatusInfo, FileInfo,
            resolve_unit_label, resolve_entity_status, resolve_status_for_entity};

pub(super) fn get_individual_data(conn: &Connection, individual_id: &str, groups: (u8, u8, u8), individual: Individual) -> Result<EntityData, String> {
    let t0 = std::time::Instant::now();
    let (group_class, group_individual, group_literal) = groups;
    let n_props = individual.properties.len();
    let n_backlinks = individual.backlinks.len();
    let n_types = individual.types.len();
    crate::commands::logging::log_backend("DEBUG", &format!("[INSPECTOR] {individual_id}: Individual (props={n_props} backlinks={n_backlinks} types={n_types})"));

    let label = individual.label.unwrap_or_else(|| individual_id.to_string());
    let icon = individual.icon;
    let comment = individual.comment;

    let mut max_tx_per_predicate: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    for (idx, (property_iri, _)) in individual.properties.iter().enumerate() {
        let tx = individual.property_tx.get(idx).copied().unwrap_or(0);
        let entry = max_tx_per_predicate.entry(property_iri.clone()).or_insert(0);
        if tx > *entry {
            *entry = tx;
        }
    }

    let entity_class_iris: std::collections::HashSet<String> = individual.types.iter()
        .map(|t| t.iri.clone())
        .collect();

    // Pre-fetch all class objects to avoid redundant Class::get calls.
    let mut class_cache: HashMap<String, Class> = HashMap::new();
    for type_thing in &individual.types {
        if let Ok(Some(class)) = Class::get(conn, &type_thing.iri) {
            class_cache.insert(type_thing.iri.clone(), class);
        }
    }

    // Collect all property IRIs needed (filled + class-defined empty) for a single batch fetch.
    let filled_iris_set: std::collections::HashSet<&str> = individual.properties.iter()
        .map(|(iri, _)| iri.as_str())
        .collect();
    let mut all_prop_iris: Vec<String> = individual.properties.iter()
        .map(|(iri, _)| iri.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    {
        let mut seen = std::collections::HashSet::new();
        for type_thing in &individual.types {
            if let Some(class) = class_cache.get(&type_thing.iri) {
                for (prop_iri, _) in &class.properties {
                    if !filled_iris_set.contains(prop_iri.as_str()) && seen.insert(prop_iri.clone()) {
                        all_prop_iris.push(prop_iri.clone());
                    }
                }
            }
        }
    }
    let all_prop_iris_refs: Vec<&str> = all_prop_iris.iter().map(|s| s.as_str()).collect();
    let prop_cache = Property::get_batch(conn, &all_prop_iris_refs).unwrap_or_default();

    // Pre-fetch Thing metadata for all range/source class IRIs in one batch.
    let mut thing_iris: std::collections::HashSet<String> = std::collections::HashSet::new();
    for prop in prop_cache.values() {
        if let Some(range_iri) = prop.ranges.first() {
            thing_iris.insert(range_iri.clone());
        }
    }
    for type_thing in &individual.types {
        if let Some(class) = class_cache.get(&type_thing.iri) {
            for (_, source_class_iri) in &class.properties {
                if source_class_iri != &type_thing.iri {
                    thing_iris.insert(source_class_iri.clone());
                }
            }
        }
    }
    let thing_iris_vec: Vec<String> = thing_iris.into_iter().collect();
    let thing_cache = crate::owl::Thing::get_batch(conn, &thing_iris_vec);

    let mut properties = Vec::new();
    for (property_iri, value_obj) in &individual.properties {
        let prop_opt = prop_cache.get(property_iri.as_str());
        let (property_label, property_comment, unit, unit_label, is_object_property, prop_ranges,
            ai_behavior_rules) =
            if let Some(prop) = prop_opt {
            let label = prop.domain_labels.iter()
                .find(|dl| entity_class_iris.contains(&dl.domain))
                .map(|dl| dl.forward_label.clone())
                .unwrap_or_else(|| prop.label.clone().unwrap_or_else(|| property_iri.clone()));
            let comment = prop.comment.clone();

            let (unit, unit_label) = if let Some(unit_iri) = &prop.unit {
                let unit_display = resolve_unit_label(conn, unit_iri);
                (Some(unit_iri.clone()), unit_display)
            } else {
                (None, None)
            };

            let is_obj_prop = prop.property_type == crate::owl::PropertyType::ObjectProperty
                || value_obj.is_iri();
            (label, comment, unit, unit_label, is_obj_prop, prop.ranges.clone(), prop.ai_behavior_rules.clone())
        } else {
            (property_iri.clone(), None, None, None, value_obj.is_iri(), vec![], None)
        };

        let value = if is_object_property {
            value_obj.as_iri().unwrap_or("").to_string()
        } else {
            value_obj.as_literal().unwrap_or_default()
        };

        let (value_label, value_icon, datatype, value_status) = if is_object_property {
            let target_thing = crate::owl::Thing::get(conn, &value);
            let status = resolve_status_for_entity(conn, &value);
            (Some(target_thing.label), target_thing.icon, None, status)
        } else {
            let stored_dt = value_obj.datatype().map(|s| s.to_string());
            let declared_dt = prop_ranges.first()
                .filter(|r| r.as_str() != "xsd:string")
                .cloned();
            let effective_dt = if stored_dt.as_deref().map(|dt| dt != "xsd:string").unwrap_or(false) {
                stored_dt
            } else {
                declared_dt.or(stored_dt)
            };
            (None, None, effective_dt, None)
        };

        let (range_class_iri, range_class_label, range_class_icon) = if is_object_property {
            prop_ranges.first().map(|range_iri| {
                let range_thing = thing_cache.get(range_iri)
                    .cloned()
                    .unwrap_or_else(|| crate::owl::Thing::get(conn, range_iri));
                (Some(range_iri.clone()), Some(range_thing.label), range_thing.icon)
            }).unwrap_or((None, None, None))
        } else {
            (None, None, None)
        };

        let file_info = if range_class_iri.as_deref() == Some("foundation:File") && !value.is_empty() {
            let file_path = owl::get_literal_property(conn, &value, "foundation:filePath").ok().flatten();
            let file_name = owl::get_literal_property(conn, &value, "foundation:fileName").ok().flatten();
            let file_size = owl::get_literal_property(conn, &value, "foundation:fileSize").ok().flatten()
                .and_then(|s| s.parse::<i64>().ok());
            let file_type_iri = owl::get_iri_property(conn, &value, "foundation:hasFileType").ok().flatten();
            Some(FileInfo { file_path, file_name, file_size, file_type_iri })
        } else {
            None
        };

        let is_calculated = conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate IN ('foundation:formula', 'foundation:aggregation') AND retracted = 0",
            rusqlite::params![property_iri],
            |row| row.get::<_, i64>(0),
        ).unwrap_or(0) > 0;

        let formula_error: Option<String> = if is_calculated {
            conn.query_row(
                "SELECT error_message FROM formula_instance_errors WHERE instance_iri = ? AND property_iri = ?",
                rusqlite::params![individual_id, property_iri],
                |row| row.get(0),
            ).ok()
        } else {
            None
        };

        properties.push(PropertyValue {
            property: property_iri.clone(),
            property_label,
            property_comment,
            value,
            value_label,
            value_icon,
            is_object_property,
            source_class: None,
            source_class_label: None,
            source_class_icon: None,
            unit,
            unit_label,
            datatype,
            value_status,
            group_total: None,
            is_calculated,
            formula_error,
            is_empty: false,
            range_class_iri,
            range_class_label,
            range_class_icon,
            file_info,
            min_count: None,
            max_count: None,
            ai_behavior_rules,
        });
    }

    let t_props = std::time::Instant::now();
    crate::commands::logging::log_backend("DEBUG", &format!("[INSPECTOR] {individual_id}: property loop {}ms", t_props.duration_since(t0).as_millis()));

    properties.sort_by(|a, b| {
        let tx_a = max_tx_per_predicate.get(&a.property).copied().unwrap_or(0);
        let tx_b = max_tx_per_predicate.get(&b.property).copied().unwrap_or(0);
        tx_b.cmp(&tx_a)
    });

    {
        let filled_iris: std::collections::HashSet<String> = properties.iter()
            .map(|p| p.property.clone())
            .collect();
        let mut seen = std::collections::HashSet::new();

        for type_thing in &individual.types {
            if let Some(class) = class_cache.get(&type_thing.iri) {
                for (prop_iri, source_class_iri) in &class.properties {
                    if filled_iris.contains(prop_iri) { continue; }
                    if !seen.insert(prop_iri.clone()) { continue; }

                    let Some(prop) = prop_cache.get(prop_iri.as_str()) else { continue };

                    let property_label = prop.label.clone().unwrap_or_else(|| prop_iri.clone());
                    let property_comment = prop.comment.clone();
                    let is_object_property = prop.property_type == crate::owl::PropertyType::ObjectProperty;

                    let (source_class, source_class_label, source_class_icon) = if source_class_iri != &type_thing.iri {
                        let source_thing = thing_cache.get(source_class_iri)
                            .cloned()
                            .unwrap_or_else(|| crate::owl::Thing::get(conn, source_class_iri));
                        (Some(source_class_iri.clone()), Some(source_thing.label), source_thing.icon)
                    } else {
                        (None, None, None)
                    };

                    let (unit, unit_label) = if let Some(unit_iri) = &prop.unit {
                        (Some(unit_iri.clone()), resolve_unit_label(conn, unit_iri))
                    } else {
                        (None, None)
                    };

                    let (range_class_iri, range_class_label, range_class_icon) = if is_object_property {
                        prop.ranges.first().map(|range_iri| {
                            let range_thing = thing_cache.get(range_iri)
                                .cloned()
                                .unwrap_or_else(|| crate::owl::Thing::get(conn, range_iri));
                            (Some(range_iri.clone()), Some(range_thing.label), range_thing.icon)
                        }).unwrap_or((None, None, None))
                    } else {
                        (None, None, None)
                    };

                    let ai_behavior_rules = prop.ai_behavior_rules.clone();

                    properties.push(PropertyValue {
                        property: prop_iri.clone(),
                        property_label,
                        property_comment,
                        value: String::new(),
                        value_label: None,
                        value_icon: None,
                        is_object_property,
                        source_class,
                        source_class_label,
                        source_class_icon,
                        unit,
                        unit_label,
                        datatype: if !is_object_property {
                            prop.ranges.first()
                                .filter(|r| r.starts_with("xsd:"))
                                .cloned()
                        } else {
                            None
                        },
                        value_status: None,
                        group_total: None,
                        is_calculated: false,
                        formula_error: None,
                        is_empty: true,
                        range_class_iri,
                        range_class_label,
                        range_class_icon,
                        file_info: None,
                        min_count: None,
                        max_count: None,
                        ai_behavior_rules,
                    });
                }
            }
        }
    }

    let t_empty_props = std::time::Instant::now();
    crate::commands::logging::log_backend("DEBUG", &format!("[INSPECTOR] {individual_id}: empty-props loop {}ms", t_empty_props.duration_since(t_props).as_millis()));

    {
        let mut card_map: HashMap<String, (Option<u32>, Option<u32>)> = HashMap::new();
        for type_thing in &individual.types {
            if let Ok(restrictions) = crate::owl::cardinality::get_class_cardinality_restrictions(
                conn, &type_thing.iri,
            ) {
                for r in restrictions {
                    let entry = card_map.entry(r.property_iri).or_insert((None, None));
                    let min = r.exact.or(r.min);
                    let max = r.exact.or(r.max);
                    if let Some(m) = min {
                        entry.0 = Some(entry.0.map_or(m, |e: u32| e.max(m)));
                    }
                    if let Some(m) = max {
                        entry.1 = Some(entry.1.map_or(m, |e: u32| e.min(m)));
                    }
                }
            }
        }
        for prop in &mut properties {
            if let Some(&(min, max)) = card_map.get(&prop.property) {
                prop.min_count = min;
                prop.max_count = max;
            }
        }
    }

    let t_cardinality = std::time::Instant::now();
    crate::commands::logging::log_backend("DEBUG", &format!("[INSPECTOR] {individual_id}: cardinality {}ms", t_cardinality.duration_since(t_empty_props).as_millis()));

    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut added_node_ids = std::collections::HashSet::new();

    nodes.push(GraphNode {
        id: individual_id.to_string(),
        label: label.clone(),
        icon: icon.clone(),
        group: group_individual,
        is_broken_ref: None,
        is_literal: None,
    });
    added_node_ids.insert(individual_id.to_string());

    for class_thing in &individual.types {
        if !added_node_ids.contains(&class_thing.iri) {
            nodes.push(GraphNode {
                id: class_thing.iri.clone(),
                label: class_thing.label.clone(),
                icon: class_thing.icon.clone(),
                group: group_class,
                is_broken_ref: None,
                is_literal: None,
            });
            added_node_ids.insert(class_thing.iri.clone());
        }

        links.push(GraphLink {
            source: individual_id.to_string(),
            target: class_thing.iri.clone(),
            label: "type".to_string(),
        });
    }

    for prop in &properties {
        if prop.is_object_property {
            if !added_node_ids.contains(&prop.value) {
                let related_thing = crate::owl::Thing::get(conn, &prop.value);
                let entity_exists_flag = conn.query_row(
                    "SELECT 1 FROM triples WHERE subject = ?1 AND retracted = 0 LIMIT 1",
                    rusqlite::params![prop.value],
                    |_| Ok(()),
                ).is_ok();

                nodes.push(GraphNode {
                    id: prop.value.clone(),
                    label: related_thing.label,
                    icon: if entity_exists_flag {
                        related_thing.icon
                    } else {
                        Some("warning".to_string())
                    },
                    group: group_individual,
                    is_broken_ref: if entity_exists_flag { None } else { Some(true) },
                    is_literal: None,
                });
                added_node_ids.insert(prop.value.clone());
            }

            links.push(GraphLink {
                source: individual_id.to_string(),
                target: prop.value.clone(),
                label: prop.property_label.clone(),
            });
        } else {
            let literal_node_id = format!("{}#literal#{}", individual_id, &prop.property);

            if !added_node_ids.contains(&literal_node_id) {
                let display_value = if let Some(unit_label) = &prop.unit_label {
                    format!("{} {}", prop.value, unit_label)
                } else {
                    prop.value.clone()
                };

                nodes.push(GraphNode {
                    id: literal_node_id.clone(),
                    label: display_value,
                    icon: prop.value_icon.clone(),
                    group: group_literal,
                    is_broken_ref: None,
                    is_literal: Some(true),
                });
                added_node_ids.insert(literal_node_id.clone());
            }

            links.push(GraphLink {
                source: individual_id.to_string(),
                target: literal_node_id,
                label: prop.property_label.clone(),
            });
        }
    }

    let t_graph_nodes = std::time::Instant::now();
    crate::commands::logging::log_backend("DEBUG", &format!("[INSPECTOR] {individual_id}: graph nodes {}ms", t_graph_nodes.duration_since(t_cardinality).as_millis()));

    let backlink_source_iris: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        individual.backlinks.iter()
            .map(|b| b.subject.clone())
            .filter(|s| seen.insert(s.clone()))
            .collect()
    };

    let source_things = crate::owl::Thing::get_batch(conn, &backlink_source_iris);

    let unique_class_iris: Vec<String> = individual.backlinks.iter()
        .filter_map(|b| b.source_class.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let class_things = crate::owl::Thing::get_batch(conn, &unique_class_iris);

    let mut prop_cache: HashMap<
        String,
        (String, Option<String>, Vec<crate::owl::DomainLabel>),
    > = HashMap::new();
    {
        let unique_prop_iris: std::collections::HashSet<String> = individual.backlinks.iter()
            .map(|b| b.predicate.clone())
            .collect();
        for prop_iri in unique_prop_iris {
            let (label, comment, domain_labels) = if let Ok(Some(prop)) =
                Property::get(conn, &prop_iri)
            {
                (prop.label.unwrap_or_else(|| prop_iri.clone()), prop.comment, prop.domain_labels)
            } else {
                (prop_iri.clone(), None, vec![])
            };
            prop_cache.insert(prop_iri, (label, comment, domain_labels));
        }
    }

    let source_status_iris = crate::eavto::query::get_first_iri_property_batch(
        conn, &backlink_source_iris, "foundation:hasStatus",
    ).unwrap_or_default();

    let unique_status_iris: Vec<String> = source_status_iris.values()
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mut status_cache: HashMap<String, StatusInfo> = HashMap::new();
    for status_iri in unique_status_iris {
        if owl::is_instance_of(conn, &status_iri, "foundation:Status") {
            let status_thing = crate::owl::Thing::get(conn, &status_iri);
            let (icon, color) = owl::resolve_status_appearance(conn, &status_iri);
            status_cache.insert(status_iri.clone(), StatusInfo {
                iri: status_iri,
                label: status_thing.label,
                icon,
                color,
            });
        }
    }

    for b in &individual.backlinks {
        if !added_node_ids.contains(&b.subject) {
            let thing = source_things.get(&b.subject)
                .cloned()
                .unwrap_or_else(|| crate::owl::Thing::get(conn, &b.subject));
            nodes.push(GraphNode {
                id: b.subject.clone(),
                label: thing.label,
                icon: thing.icon,
                group: group_individual,
                is_broken_ref: None,
                is_literal: None,
            });
            added_node_ids.insert(b.subject.clone());
        }

        let prop_label = prop_cache.get(&b.predicate)
            .map(|(l, _, dls)| {
                b.source_class.as_deref()
                    .and_then(|cls| dls.iter().find(|dl| dl.domain == cls))
                    .map(|dl| dl.inverse_label.as_deref().unwrap_or(&dl.forward_label).to_string())
                    .unwrap_or_else(|| l.clone())
            })
            .unwrap_or_else(|| b.predicate.clone());

        links.push(GraphLink {
            source: b.subject.clone(),
            target: individual_id.to_string(),
            label: prop_label,
        });
    }

    let backlinks: Vec<PropertyValue> = Vec::new();
    for b in &individual.backlinks {
        let (property_label, property_comment) = {
            let cached = prop_cache.get(&b.predicate);
            let resolved_label = b.source_class.as_deref()
                .and_then(|cls| cached.and_then(|(_, _, dls)| {
                    dls.iter().find(|dl| dl.domain == cls)
                }))
                .map(|dl| dl.inverse_label.as_deref().unwrap_or(&dl.forward_label).to_string())
                .or_else(|| cached.map(|(l, _, _)| l.clone()))
                .unwrap_or_else(|| b.predicate.clone());
            let comment = cached.and_then(|(_, c, _)| c.clone());
            (resolved_label, comment)
        };

        let source_thing = source_things.get(&b.subject)
            .cloned()
            .unwrap_or_else(|| crate::owl::Thing::get(conn, &b.subject));

        let (source_class_iri, source_class_label) = match &b.source_class {
            Some(class_iri) => {
                let label = class_things.get(class_iri)
                    .map(|t| t.label.clone())
                    .unwrap_or_else(|| class_iri.clone());
                (Some(class_iri.clone()), Some(label))
            }
            None => (None, None),
        };

        let value_status = source_status_iris.get(&b.subject)
            .and_then(|status_iri| status_cache.get(status_iri))
            .cloned();

        properties.push(PropertyValue {
            property: b.predicate.clone(),
            property_label,
            property_comment,
            value: b.subject.clone(),
            value_label: Some(source_thing.label),
            value_icon: source_thing.icon,
            is_object_property: true,
            source_class: source_class_iri,
            source_class_label,
            source_class_icon: None,
            unit: None,
            unit_label: None,
            datatype: None,
            value_status,
            group_total: Some(b.group_total),
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
            min_count: None,
            max_count: None,
            ai_behavior_rules: None,
        });
    }

    let t_backlinks = std::time::Instant::now();
    crate::commands::logging::log_backend("DEBUG", &format!("[INSPECTOR] {individual_id}: backlinks section {}ms", t_backlinks.duration_since(t_graph_nodes).as_millis()));

    let status = resolve_entity_status(conn, &properties);

    let mut required_fields: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for type_thing in &individual.types {
        if let Ok(restrictions) = crate::owl::cardinality::get_class_cardinality_restrictions(conn, &type_thing.iri) {
            for r in restrictions {
                if r.is_required() && seen.insert(r.property_iri.clone()) {
                    required_fields.push(r.property_iri);
                }
            }
        }
    }

    let mut allowed_statuses: Vec<StatusInfo> = Vec::new();
    let mut seen_status_iris = std::collections::HashSet::new();
    for type_thing in &individual.types {
        let status_iris = crate::owl::get_all_iri_properties(conn, &type_thing.iri, "foundation:allowedStatus")
            .unwrap_or_default();
        for status_iri in status_iris {
            if seen_status_iris.insert(status_iri.clone()) {
                let thing = crate::owl::Thing::get(conn, &status_iri);
                let (icon, color) = crate::owl::resolve_status_appearance(conn, &status_iri);
                allowed_statuses.push(StatusInfo { iri: status_iri, label: thing.label, icon, color });
            }
        }
    }

    let t_tail = std::time::Instant::now();
    crate::commands::logging::log_backend("DEBUG", &format!(
        "[INSPECTOR] {individual_id}: tail (status+required+allowed) {}ms | TOTAL {}ms",
        t_tail.duration_since(t_backlinks).as_millis(),
        t_tail.duration_since(t0).as_millis(),
    ));

    Ok(EntityData {
        id: individual_id.to_string(),
        label,
        icon,
        comment,
        is_class: false,
        is_locked: crate::owl::is_system_locked(conn, individual_id),
        allowed_statuses,
        types: individual.types.clone(),
        super_classes: vec![],
        sub_classes: vec![],
        instances: vec![],
        properties,
        backlinks,
        status,
        required_fields,
        nodes,
        links,
    })
}
