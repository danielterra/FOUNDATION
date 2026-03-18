use serde::Serialize;
use tauri::{State, Emitter};
use std::collections::HashMap;
use crate::owl::{self, Class, Individual, Property, Connection, DbExecutor, Object};

mod individual;

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
pub async fn graph__search_entities(
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
pub async fn inspector__get_entity(
    entity_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        crate::search::track_access(conn, &entity_id);
        let entity_type = determine_entity_type(conn, &entity_id)?;
        let groups = owl::load_graph_node_groups(conn);

        let data = match entity_type {
            EntityType::Class => get_class_data(conn, &entity_id, groups)?,
            EntityType::Individual => individual::get_individual_data(conn, &entity_id, groups)?,
        };

        serde_json::to_string(&data).map_err(|e| e.to_string())
    }).await
}

/// Returns the GraphNodeType configuration stored in the ontology.
/// The frontend uses this to map group discriminators to visual styles.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn graph__get_node_type_config(
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let configs = owl::get_graph_node_type_config(conn);
        serde_json::to_string(&configs).map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__update_property(
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

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__update_status(
    entity_id: String,
    status_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    executor.write(move |conn| {
        let individual = Individual::new(&entity_id_clone);
        individual.add_property(conn, "foundation:hasStatus", vec![Object::Iri(status_iri)], "user")
            .map_err(|e| e.to_string())?;
        Ok("updated".to_string())
    }).await?;
    app.emit("entity-updated", serde_json::json!({ "entityId": entity_id })).ok();
    Ok(())
}

pub(super) fn sort_backlinks_by_recency(conn: &Connection, backlinks: &mut Vec<PropertyValue>) {
    let entity_iris: Vec<String> = backlinks.iter().map(|b| b.value.clone()).collect();
    let max_tx_map = crate::eavto::query::get_entities_max_tx(conn, &entity_iris)
        .unwrap_or_default();
    backlinks.sort_by(|a, b| {
        let tx_a = max_tx_map.get(&a.value).copied().unwrap_or(0);
        let tx_b = max_tx_map.get(&b.value).copied().unwrap_or(0);
        tx_b.cmp(&tx_a)
    });
}

pub(super) fn resolve_unit_label(conn: &Connection, unit_iri: &str) -> Option<String> {
    owl::get_literal_property(conn, unit_iri, "qudt:currencyCode")
        .ok()
        .flatten()
        .or_else(|| Some(crate::owl::Thing::get(conn, unit_iri).label))
}

pub(super) fn resolve_entity_status(conn: &Connection, properties: &[PropertyValue]) -> Option<StatusInfo> {
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

pub(super) fn resolve_status_for_entity(conn: &Connection, entity_iri: &str) -> Option<StatusInfo> {
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

    for (predicate, value_obj) in &class.concept_properties {
        let (property_label, property_comment) = if let Ok(Some(prop)) = Property::get(conn, predicate) {
            (prop.label.unwrap_or_else(|| predicate.clone()), prop.comment)
        } else {
            (predicate.clone(), None)
        };

        let is_object_property = value_obj.is_iri();
        let value = if is_object_property {
            value_obj.as_iri().unwrap_or("").to_string()
        } else {
            value_obj.as_literal().unwrap_or_default()
        };

        let value_label = if is_object_property {
            Some(crate::owl::Thing::get(conn, &value).label)
        } else {
            None
        };

        properties.push(PropertyValue {
            property: predicate.clone(),
            property_label,
            property_comment,
            value,
            value_label,
            value_icon: None,
            is_object_property,
            source_class: None,
            source_class_label: None,
            unit: None,
            unit_label: None,
            datatype: value_obj.datatype().map(|s| s.to_string()),
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

    let instance_iris: Vec<String> = class.backlinks.iter()
        .map(|(s, _, _)| s.clone())
        .collect();

    let source_things = crate::owl::Thing::get_batch(conn, &instance_iris);

    let source_status_iris = crate::eavto::query::get_first_iri_property_batch(
        conn, &instance_iris, "foundation:hasStatus",
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

    let group_total = if class.backlink_total > class.backlinks.len() {
        Some(class.backlink_total)
    } else {
        None
    };

    let mut backlinks = Vec::new();
    for (source_entity, property_iri, _) in &class.backlinks {
        let source_thing = source_things.get(source_entity)
            .cloned()
            .unwrap_or_else(|| crate::owl::Thing::get(conn, source_entity));

        let value_status = source_status_iris.get(source_entity)
            .and_then(|s| status_cache.get(s))
            .cloned();

        backlinks.push(PropertyValue {
            property: property_iri.clone(),
            property_label: "type of".to_string(),
            property_comment: None,
            value: source_entity.clone(),
            value_label: Some(source_thing.label),
            value_icon: source_thing.icon,
            is_object_property: true,
            source_class: Some(class_id.to_string()),
            source_class_label: None,
            unit: None,
            unit_label: None,
            datatype: None,
            value_status,
            group_total,
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
