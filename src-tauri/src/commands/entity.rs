use serde::Serialize;
use tauri::{State, Emitter};
use std::collections::HashMap;
use crate::owl::{self, Class, Individual, Property, Connection, DbExecutor, Object};

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Class,
    Individual,
}


#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub group: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_broken_ref: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_literal: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphLink {
    pub source: String,
    pub target: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityData {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub comment: Option<String>,

    pub types: Vec<crate::owl::Thing>,
    pub super_classes: Vec<crate::owl::Thing>,
    pub sub_classes: Vec<crate::owl::Thing>,
    pub instances: Vec<crate::owl::Thing>,

    pub is_class: bool,
    pub allowed_statuses: Vec<StatusInfo>,

    pub properties: Vec<PropertyValue>,
    pub backlinks: Vec<PropertyValue>,
    pub status: Option<StatusInfo>,
    pub required_fields: Vec<String>,

    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub iri: String,
    pub label: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyValue {
    pub property: String,
    pub property_label: String,
    pub property_comment: Option<String>,
    pub value: String,
    pub value_label: Option<String>,
    pub value_icon: Option<String>,
    pub is_object_property: bool,
    pub source_class: Option<String>,
    pub source_class_label: Option<String>,
    pub unit: Option<String>,
    pub unit_label: Option<String>,
    pub datatype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_status: Option<StatusInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_total: Option<usize>,
    pub is_calculated: bool,
    pub formula_error: Option<String>,
    pub is_empty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_class_iri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_class_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_class_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_info: Option<FileInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    pub file_size: Option<i64>,
    pub file_type_iri: Option<String>,
}

/// Search for instances by label and property values
#[tauri::command]
#[allow(non_snake_case)]
pub async fn entity__search(
    query: String,
    limit: Option<usize>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let limit = limit.unwrap_or(100);
        let results = crate::owl::search_instances_rich(conn, &query, limit)
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&results).map_err(|e| e.to_string())
    }).await
}

/// Get entity data with its complete neighborhood for visualization
#[tauri::command]
#[allow(non_snake_case)]
pub async fn entity__get(
    entity_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let entity_type = determine_entity_type(conn, &entity_id)?;
        let groups = owl::load_graph_node_groups(conn);

        let data = match entity_type {
            EntityType::Class => get_class_data(conn, &entity_id, groups)?,
            EntityType::Individual => get_individual_data(conn, &entity_id, groups)?,
        };

        serde_json::to_string(&data).map_err(|e| e.to_string())
    }).await
}

/// Returns the GraphNodeType configuration stored in the ontology.
/// The frontend uses this to map group discriminators to visual styles.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn entity__get_node_type_config(
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let configs = owl::get_graph_node_type_config(conn);
        serde_json::to_string(&configs).map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn entity__update_literal(
    entity_id: String,
    property_iri: String,
    value: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    executor.write(move |conn| {
        let individual = Individual::new(&entity_id_clone);
        individual.add_property(conn, &property_iri, vec![Object::Literal {
            value,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "user").map_err(|e| e.to_string())?;
        Ok("updated".to_string())
    }).await?;
    app.emit("entity-updated", serde_json::json!({ "entityId": entity_id })).ok();
    Ok(())
}

fn sort_backlinks_by_recency(conn: &Connection, backlinks: &mut Vec<PropertyValue>) {
    let entity_iris: Vec<String> = backlinks.iter().map(|b| b.value.clone()).collect();
    let max_tx_map = crate::eavto::query::get_entities_max_tx(conn, &entity_iris)
        .unwrap_or_default();
    backlinks.sort_by(|a, b| {
        let tx_a = max_tx_map.get(&a.value).copied().unwrap_or(0);
        let tx_b = max_tx_map.get(&b.value).copied().unwrap_or(0);
        tx_b.cmp(&tx_a)
    });
}

fn resolve_unit_label(conn: &Connection, unit_iri: &str) -> Option<String> {
    owl::get_literal_property(conn, unit_iri, "qudt:currencyCode")
        .ok()
        .flatten()
        .or_else(|| Some(crate::owl::Thing::get(conn, unit_iri).label))
}

fn resolve_entity_status(conn: &Connection, properties: &[PropertyValue]) -> Option<StatusInfo> {
    for prop in properties {
        if !prop.is_object_property || prop.value.is_empty() {
            continue;
        }
        if owl::is_instance_of(conn, &prop.value, "foundation:Status") {
            let (icon, color) = owl::resolve_status_appearance(conn, &prop.value);
            return Some(StatusInfo {
                iri: prop.value.clone(),
                label: prop.value_label.clone().unwrap_or_else(|| prop.value.clone()),
                icon,
                color,
            });
        }
    }
    None
}

fn resolve_status_for_entity(conn: &Connection, entity_iri: &str) -> Option<StatusInfo> {
    owl::get_entity_status_info(conn, entity_iri)
        .map(|(iri, label, color, icon)| StatusInfo { iri, label, icon, color })
}

fn determine_entity_type(conn: &Connection, entity_id: &str) -> Result<EntityType, String> {
    if Class::get(conn, entity_id).map_err(|e| e.to_string())?.is_some() {
        return Ok(EntityType::Class);
    }

    if Individual::get(conn, entity_id).map_err(|e| e.to_string())?.is_some() {
        return Ok(EntityType::Individual);
    }

    Err(format!("Entity {} not found or unknown type", entity_id))
}

fn get_class_data(conn: &Connection, class_id: &str, groups: (u8, u8, u8)) -> Result<EntityData, String> {
    let (group_class, _group_individual, group_literal) = groups;
    let class = Class::get(conn, class_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Class {} not found", class_id))?;

    let label = class.label.unwrap_or_else(|| class_id.to_string());
    let icon = class.icon;
    let comment = class.comment;


    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut added_node_ids = std::collections::HashSet::new();

    nodes.push(GraphNode {
        id: class_id.to_string(),
        label: label.clone(),
        icon: icon.clone(),
        group: group_class,
        is_broken_ref: None,
        is_literal: None,
    });
    added_node_ids.insert(class_id.to_string());

    for super_class in &class.super_classes {
        if !added_node_ids.contains(&super_class.iri) {
            nodes.push(GraphNode {
                id: super_class.iri.clone(),
                label: super_class.label.clone(),
                icon: super_class.icon.clone(),
                group: group_class,
                is_broken_ref: None,
                is_literal: None,
            });
            added_node_ids.insert(super_class.iri.clone());
        }

        links.push(GraphLink {
            source: class_id.to_string(),
            target: super_class.iri.clone(),
            label: "subClassOf".to_string(),
        });
    }

    for sub_class in &class.sub_classes {
        if !added_node_ids.contains(&sub_class.iri) {
            nodes.push(GraphNode {
                id: sub_class.iri.clone(),
                label: sub_class.label.clone(),
                icon: sub_class.icon.clone(),
                group: group_class,
                is_broken_ref: None,
                is_literal: None,
            });
            added_node_ids.insert(sub_class.iri.clone());
        }

        links.push(GraphLink {
            source: sub_class.iri.clone(),
            target: class_id.to_string(),
            label: "subClassOf".to_string(),
        });
    }

    for (property_iri, _source_class_iri) in &class.properties {
        let prop = Property::get(conn, property_iri)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Property {} not found", property_iri))?;

        let property_label = prop.label.clone().unwrap_or_else(|| property_iri.clone());

        if prop.property_type == crate::owl::PropertyType::ObjectProperty {
            for range_iri in &prop.ranges {
                if !added_node_ids.contains(range_iri) {
                    let range_thing = crate::owl::Thing::get(conn, range_iri);
                    nodes.push(GraphNode {
                        id: range_iri.clone(),
                        label: range_thing.label,
                        icon: range_thing.icon,
                        group: group_class,
                        is_broken_ref: None,
                        is_literal: None,
                    });
                    added_node_ids.insert(range_iri.clone());
                }

                links.push(GraphLink {
                    source: class_id.to_string(),
                    target: range_iri.clone(),
                    label: property_label.clone(),
                });
            }
        } else {
            for range_iri in &prop.ranges {
                let literal_node_id = format!("{}#datatype#{}", class_id, range_iri);

                if !added_node_ids.contains(&literal_node_id) {
                    let range_thing = crate::owl::Thing::get(conn, range_iri);

                    nodes.push(GraphNode {
                        id: literal_node_id.clone(),
                        label: range_thing.label,
                        icon: range_thing.icon,
                        group: group_literal,
                        is_broken_ref: None,
                        is_literal: Some(true),
                    });
                    added_node_ids.insert(literal_node_id.clone());
                }

                links.push(GraphLink {
                    source: class_id.to_string(),
                    target: literal_node_id,
                    label: property_label.clone(),
                });
            }
        }
    }


    let mut properties = Vec::new();

    for type_thing in &class.types {
        properties.push(PropertyValue {
            property: "rdf:type".to_string(),
            property_label: "type".to_string(),
            property_comment: Some("The type of this entity".to_string()),
            value: type_thing.iri.clone(),
            value_label: Some(type_thing.label.clone()),
            value_icon: type_thing.icon.clone(),
            is_object_property: true,
            source_class: None,
            source_class_label: None,
            unit: None,
            unit_label: None,
            datatype: None,
            value_status: None,
            group_total: None,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
        });
    }

    for super_class in &class.super_classes {
        properties.push(PropertyValue {
            property: "rdfs:subClassOf".to_string(),
            property_label: "subClassOf".to_string(),
            property_comment: Some("Parent class of this class".to_string()),
            value: super_class.iri.clone(),
            value_label: Some(super_class.label.clone()),
            value_icon: super_class.icon.clone(),
            is_object_property: true,
            source_class: None,
            source_class_label: None,
            unit: None,
            unit_label: None,
            datatype: None,
            value_status: None,
            group_total: None,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
        });
    }

    for (property_iri, source_class_iri) in &class.properties {
        let prop = Property::get(conn, property_iri)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Property {} not found", property_iri))?;

        let property_label = prop.label.unwrap_or_else(|| property_iri.clone());
        let property_comment = prop.comment;

        let is_object_property = prop.property_type == crate::owl::PropertyType::ObjectProperty;
        let (value, value_label, value_icon) = prop.ranges.first()
            .map(|range_iri| {
                let range_thing = crate::owl::Thing::get(conn, range_iri);
                (range_iri.clone(), range_thing.label, range_thing.icon)
            })
            .unwrap_or_else(|| ("owl:Thing".to_string(), "Any".to_string(), None));

        let (source_class, source_class_label) = if source_class_iri != class_id {
            let source_thing = crate::owl::Thing::get(conn, source_class_iri);
            (Some(source_class_iri.clone()), Some(source_thing.label))
        } else {
            (None, None)
        };

        let (unit, unit_label) = if let Some(unit_iri) = &prop.unit {
            let unit_display = owl::get_literal_property(conn, unit_iri, "qudt:symbol")
                .ok()
                .flatten()
                .or_else(|| {
                    let unit_thing = crate::owl::Thing::get(conn, unit_iri);
                    Some(unit_thing.label)
                });

            (Some(unit_iri.clone()), unit_display)
        } else {
            (None, None)
        };

        properties.push(PropertyValue {
            property: property_iri.clone(),
            property_label,
            property_comment,
            value,
            value_label: Some(value_label),
            value_icon,
            is_object_property,
            source_class,
            source_class_label,
            unit,
            unit_label,
            datatype: None,
            value_status: None,
            group_total: None,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
        });
    }

    let mut backlinks = Vec::new();
    for (source_entity, property_iri, _value_obj) in &class.backlinks {
        let prop_result = Property::get(conn, property_iri);
        let (property_label, property_comment) = if let Ok(Some(prop)) = prop_result {
            (prop.label.unwrap_or_else(|| property_iri.clone()), prop.comment)
        } else {
            (property_iri.clone(), None)
        };

        let source_thing = crate::owl::Thing::get(conn, source_entity);

        let (source_class_iri, source_class_label) =
            match owl::get_iri_property(conn, source_entity, "rdf:type") {
                Ok(Some(class_iri)) => {
                    let class_thing = crate::owl::Thing::get(conn, &class_iri);
                    (Some(class_iri), Some(class_thing.label))
                }
                _ => (None, None),
            };

        backlinks.push(PropertyValue {
            property: property_iri.clone(),
            property_label,
            property_comment,
            value: source_entity.clone(),
            value_label: Some(source_thing.label),
            value_icon: source_thing.icon,
            is_object_property: true,
            source_class: source_class_iri,
            source_class_label,
            unit: None,
            unit_label: None,
            datatype: None,
            value_status: resolve_status_for_entity(conn, source_entity),
            group_total: None,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
        });
    }

    sort_backlinks_by_recency(conn, &mut backlinks);

    let status = resolve_entity_status(conn, &properties);

    let required_fields = crate::owl::cardinality::get_class_cardinality_restrictions(conn, class_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.is_required())
        .map(|r| r.property_iri)
        .collect();

    let allowed_statuses = crate::owl::get_all_iri_properties(conn, class_id, "foundation:allowedStatus")
        .unwrap_or_default()
        .into_iter()
        .map(|status_iri| {
            let thing = crate::owl::Thing::get(conn, &status_iri);
            let (icon, color) = crate::owl::resolve_status_appearance(conn, &status_iri);
            StatusInfo { iri: status_iri, label: thing.label, icon, color }
        })
        .collect();

    Ok(EntityData {
        id: class_id.to_string(),
        label,
        icon,
        comment,
        is_class: true,
        allowed_statuses,
        types: class.types.clone(),
        super_classes: class.super_classes.clone(),
        sub_classes: class.sub_classes.clone(),
        instances: vec![],
        properties,
        backlinks,
        status,
        required_fields,
        nodes,
        links,
    })
}

fn get_individual_data(conn: &Connection, individual_id: &str, groups: (u8, u8, u8)) -> Result<EntityData, String> {
    let (group_class, group_individual, group_literal) = groups;
    let individual = Individual::get(conn, individual_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Individual {} not found", individual_id))?;

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

    let mut properties = Vec::new();
    for (property_iri, value_obj) in &individual.properties {
        let prop_result = Property::get(conn, property_iri);
        let (property_label, property_comment, unit, unit_label, is_object_property, prop_ranges) =
            if let Ok(Some(prop)) = prop_result {
            let label = prop.label.clone().unwrap_or_else(|| property_iri.clone());
            let comment = prop.comment.clone();

            let (unit, unit_label) = if let Some(unit_iri) = &prop.unit {
                let unit_display = resolve_unit_label(conn, unit_iri);

                (Some(unit_iri.clone()), unit_display)
            } else {
                (None, None)
            };

            let is_obj_prop = prop.property_type == crate::owl::PropertyType::ObjectProperty
                || value_obj.is_iri();
            (label, comment, unit, unit_label, is_obj_prop, prop.ranges)
        } else {
            (property_iri.clone(), None, None, None, value_obj.is_iri(), vec![])
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
            (None, None, value_obj.datatype().map(|s| s.to_string()), None)
        };

        let (range_class_iri, range_class_label, range_class_icon) = if is_object_property {
            prop_ranges.first().map(|range_iri| {
                let range_thing = crate::owl::Thing::get(conn, range_iri);
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
            "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = 'foundation:formula' AND retracted = 0",
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
        });
    }

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
            if let Ok(Some(class)) = Class::get(conn, &type_thing.iri) {
                for (prop_iri, source_class_iri) in &class.properties {
                    if filled_iris.contains(prop_iri) { continue; }
                    if !seen.insert(prop_iri.clone()) { continue; }

                    let Ok(Some(prop)) = Property::get(conn, prop_iri) else { continue };

                    let property_label = prop.label.unwrap_or_else(|| prop_iri.clone());
                    let property_comment = prop.comment;
                    let is_object_property = prop.property_type == crate::owl::PropertyType::ObjectProperty;

                    let (source_class, source_class_label) = if source_class_iri != &type_thing.iri {
                        let source_thing = crate::owl::Thing::get(conn, source_class_iri);
                        (Some(source_class_iri.clone()), Some(source_thing.label))
                    } else {
                        (None, None)
                    };

                    let (unit, unit_label) = if let Some(unit_iri) = &prop.unit {
                        (Some(unit_iri.clone()), resolve_unit_label(conn, unit_iri))
                    } else {
                        (None, None)
                    };

                    let (range_class_iri, range_class_label, range_class_icon) = if is_object_property {
                        prop.ranges.first().map(|range_iri| {
                            let range_thing = crate::owl::Thing::get(conn, range_iri);
                            (Some(range_iri.clone()), Some(range_thing.label), range_thing.icon)
                        }).unwrap_or((None, None, None))
                    } else {
                        (None, None, None)
                    };

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
                        unit,
                        unit_label,
                        datatype: None,
                        value_status: None,
                        group_total: None,
                        is_calculated: false,
                        formula_error: None,
                        is_empty: true,
                        range_class_iri,
                        range_class_label,
                        range_class_icon,
                        file_info: None,
                    });
                }
            }
        }
    }

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
                let entity_exists_flag =
                    Individual::get(conn, &prop.value).ok().flatten().is_some();

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

    // Batch-load all metadata needed for backlink nodes and the backlinks list.
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

    let mut prop_cache: HashMap<String, (String, Option<String>)> = HashMap::new();
    {
        let unique_prop_iris: std::collections::HashSet<String> = individual.backlinks.iter()
            .map(|b| b.predicate.clone())
            .collect();
        for prop_iri in unique_prop_iris {
            let (label, comment) = if let Ok(Some(prop)) = Property::get(conn, &prop_iri) {
                (prop.label.unwrap_or_else(|| prop_iri.clone()), prop.comment)
            } else {
                (prop_iri.clone(), None)
            };
            prop_cache.insert(prop_iri, (label, comment));
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
            .map(|(l, _)| l.clone())
            .unwrap_or_else(|| b.predicate.clone());

        links.push(GraphLink {
            source: b.subject.clone(),
            target: individual_id.to_string(),
            label: prop_label,
        });
    }

    let mut backlinks = Vec::new();
    for b in &individual.backlinks {
        let (property_label, property_comment) = prop_cache.get(&b.predicate)
            .cloned()
            .unwrap_or_else(|| (b.predicate.clone(), None));

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

        backlinks.push(PropertyValue {
            property: b.predicate.clone(),
            property_label,
            property_comment,
            value: b.subject.clone(),
            value_label: Some(source_thing.label),
            value_icon: source_thing.icon,
            is_object_property: true,
            source_class: source_class_iri,
            source_class_label,
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
        });
    }

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

    Ok(EntityData {
        id: individual_id.to_string(),
        label,
        icon,
        comment,
        is_class: false,
        allowed_statuses: vec![],
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