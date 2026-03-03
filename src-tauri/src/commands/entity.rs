use serde::Serialize;
use tauri::State;
use crate::owl::{self, Class, Individual, Property, Connection, DbExecutor};

const GROUP_CLASS: u8 = 1;
const GROUP_INDIVIDUAL: u8 = 6;
const GROUP_LITERAL: u8 = 7;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Class,
    Individual,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    #[serde(rename = "type")]
    pub entity_type: String,
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

    pub properties: Vec<PropertyValue>,
    pub backlinks: Vec<PropertyValue>,
    pub status: Option<StatusInfo>,

    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub iri: String,
    pub label: String,
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
}

/// Search for entities (classes and individuals) by label
#[tauri::command]
#[allow(non_snake_case)]
pub async fn entity__search(
    query: String,
    limit: Option<usize>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let limit = limit.unwrap_or(100);
        let mut results = Vec::new();

        let class_results = crate::owl::search_classes(conn, &query, limit)
            .map_err(|e| e.to_string())?;

        for class_result in class_results {
            results.push(SearchResult {
                id: class_result.id,
                label: class_result.label,
                icon: class_result.icon,
                entity_type: "class".to_string(),
            });
        }

        let remaining_limit = limit.saturating_sub(results.len());
        if remaining_limit > 0 {
            let individual_results = crate::owl::search_individuals(conn, &query, remaining_limit)
                .map_err(|e| e.to_string())?;

            for individual_result in individual_results {
                results.push(SearchResult {
                    id: individual_result.id,
                    label: individual_result.label,
                    icon: individual_result.icon,
                    entity_type: "individual".to_string(),
                });
            }
        }

        results.truncate(limit);

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

        let data = match entity_type {
            EntityType::Class => get_class_data(conn, &entity_id)?,
            EntityType::Individual => get_individual_data(conn, &entity_id)?,
        };

        serde_json::to_string(&data).map_err(|e| e.to_string())
    }).await
}

fn resolve_entity_status(conn: &Connection, properties: &[PropertyValue]) -> Option<StatusInfo> {
    for prop in properties {
        if !prop.is_object_property || prop.value.is_empty() {
            continue;
        }
        if owl::is_instance_of(conn, &prop.value, "foundation:Status") {
            let color = owl::get_literal_property(conn, &prop.value, "foundation:color")
                .ok()
                .flatten();
            return Some(StatusInfo {
                iri: prop.value.clone(),
                label: prop.value_label.clone().unwrap_or_else(|| prop.value.clone()),
                color,
            });
        }
    }
    None
}

fn resolve_status_for_entity(conn: &Connection, entity_iri: &str) -> Option<StatusInfo> {
    owl::get_entity_status_info(conn, entity_iri)
        .map(|(iri, label, color)| StatusInfo { iri, label, color })
}

fn determine_entity_type(conn: &Connection, entity_id: &str) -> Result<EntityType, String> {
    let class = Class::new(entity_id);
    if class.exists(conn).map_err(|e| e.to_string())? {
        return Ok(EntityType::Class);
    }

    let individual = Individual::new(entity_id);
    if individual.exists(conn).map_err(|e| e.to_string())? {
        return Ok(EntityType::Individual);
    }

    Err(format!("Entity {} not found or unknown type", entity_id))
}

fn get_class_data(conn: &Connection, class_id: &str) -> Result<EntityData, String> {
    let class = Class::get(conn, class_id)
        .map_err(|e| e.to_string())?;

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
        group: GROUP_CLASS,
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
                group: GROUP_CLASS,
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
                group: GROUP_CLASS,
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
            .map_err(|e| e.to_string())?;

        let property_label = prop.label.clone().unwrap_or_else(|| property_iri.clone());

        if prop.property_type == crate::owl::PropertyType::ObjectProperty {
            for range_iri in &prop.ranges {
                if !added_node_ids.contains(range_iri) {
                    let range_thing = crate::owl::Thing::get(conn, range_iri);
                    nodes.push(GraphNode {
                        id: range_iri.clone(),
                        label: range_thing.label,
                        icon: range_thing.icon,
                        group: GROUP_CLASS,
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
                        group: GROUP_LITERAL,
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
        });
    }

    for (property_iri, source_class_iri) in &class.properties {
        let prop = Property::get(conn, property_iri)
            .map_err(|e| e.to_string())?;

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
        });
    }

    let mut backlinks = Vec::new();
    for (source_entity, property_iri, _value_obj) in &class.backlinks {
        let prop_result = Property::get(conn, property_iri);
        let (property_label, property_comment) = if let Ok(prop) = prop_result {
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
        });
    }

    let status = resolve_entity_status(conn, &properties);

    Ok(EntityData {
        id: class_id.to_string(),
        label,
        icon,
        comment,
        types: class.types.clone(),
        super_classes: class.super_classes.clone(),
        sub_classes: class.sub_classes.clone(),
        instances: vec![],
        properties,
        backlinks,
        status,
        nodes,
        links,
    })
}

fn get_individual_data(conn: &Connection, individual_id: &str) -> Result<EntityData, String> {
    let individual = Individual::get(conn, individual_id)
        .map_err(|e| e.to_string())?;

    let label = individual.label.unwrap_or_else(|| individual_id.to_string());
    let icon = individual.icon;
    let comment = individual.comment;

    let mut properties = Vec::new();
    for (property_iri, value_obj) in &individual.properties {
        let prop_result = Property::get(conn, property_iri);
        let (property_label, property_comment, unit, unit_label, is_object_property) =
            if let Ok(prop) = prop_result {
            let label = prop.label.clone().unwrap_or_else(|| property_iri.clone());
            let comment = prop.comment.clone();

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

            let is_obj_prop = prop.property_type == crate::owl::PropertyType::ObjectProperty
                || value_obj.is_iri();
            (label, comment, unit, unit_label, is_obj_prop)
        } else {
            (property_iri.clone(), None, None, None, value_obj.is_iri())
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
        });
    }

    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut added_node_ids = std::collections::HashSet::new();

    nodes.push(GraphNode {
        id: individual_id.to_string(),
        label: label.clone(),
        icon: icon.clone(),
        group: GROUP_INDIVIDUAL,
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
                group: GROUP_CLASS,
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
                let entity_exists_flag = Individual::new(&prop.value).exists(conn).unwrap_or(false);

                nodes.push(GraphNode {
                    id: prop.value.clone(),
                    label: related_thing.label,
                    icon: if entity_exists_flag {
                        related_thing.icon
                    } else {
                        Some("warning".to_string())
                    },
                    group: GROUP_INDIVIDUAL,
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
                    group: GROUP_LITERAL,
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

    for (subject, predicate_iri, _) in &individual.backlinks {
        if !added_node_ids.contains(subject) {
            let subject_thing = crate::owl::Thing::get(conn, subject);
            let entity_exists_flag = Individual::new(subject).exists(conn).unwrap_or(false);

            nodes.push(GraphNode {
                id: subject.clone(),
                label: subject_thing.label,
                icon: if entity_exists_flag {
                    subject_thing.icon
                } else {
                    Some("warning".to_string())
                },
                group: GROUP_INDIVIDUAL,
                is_broken_ref: if entity_exists_flag { None } else { Some(true) },
                is_literal: None,
            });
            added_node_ids.insert(subject.clone());
        }

        let prop_label = Property::get(conn, predicate_iri)
            .ok()
            .and_then(|p| p.label)
            .unwrap_or_else(|| predicate_iri.clone());

        links.push(GraphLink {
            source: subject.clone(),
            target: individual_id.to_string(),
            label: prop_label,
        });
    }

    let mut backlinks = Vec::new();
    for (source_entity, property_iri, _value_obj) in &individual.backlinks {
        let prop_result = Property::get(conn, property_iri);
        let (property_label, property_comment) = if let Ok(prop) = prop_result {
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
        });
    }

    let status = resolve_entity_status(conn, &properties);

    Ok(EntityData {
        id: individual_id.to_string(),
        label,
        icon,
        comment,
        types: individual.types.clone(),
        super_classes: vec![],
        sub_classes: vec![],
        instances: vec![],
        properties,
        backlinks,
        status,
        nodes,
        links,
    })
}