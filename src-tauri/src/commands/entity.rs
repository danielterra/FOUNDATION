use serde::Serialize;
use tauri::State;
use rusqlite::Connection;

use crate::eavto::DbExecutor;
use crate::owl::{Class, Individual, Property};

/// Entity type in OWL ontology
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Class,
    Individual,
}

/// Search result for entities
#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    #[serde(rename = "type")]
    pub entity_type: String, // "class" or "individual"
}

/// Node in the graph (Class or Individual)
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub group: u8, // 1 = Class, 6 = Individual
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_broken_ref: Option<bool>, // true if entity doesn't exist in database
}

/// Link between nodes (ObjectProperty)
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphLink {
    pub source: String,
    pub target: String,
    pub label: String,
}

/// Complete entity data with its neighborhood
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityData {
    pub id: String,
    pub label: String,
    pub entity_type: EntityType,
    pub icon: Option<String>,
    pub comment: Option<String>,

    // For Classes
    pub super_classes: Vec<crate::owl::Thing>,
    pub sub_classes: Vec<crate::owl::Thing>,
    pub instances: Vec<crate::owl::Thing>,

    // For Individuals
    pub types: Vec<crate::owl::Thing>,
    pub properties: Vec<PropertyValue>,

    // Graph visualization data
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyValue {
    pub property: String,
    pub property_label: String,
    pub property_comment: Option<String>,
    pub value: String,
    pub value_label: Option<String>, // For object properties, the label of the target entity
    pub value_icon: Option<String>, // For object properties, the icon of the target entity
    pub is_object_property: bool,
    pub source_class: Option<String>, // For classes: which class defines this property (for grouping inherited properties)
    pub source_class_label: Option<String>,
    pub unit: Option<String>, // QUDT unit IRI (e.g., "unit:GigaBYTE")
    pub unit_label: Option<String>, // QUDT unit label (e.g., "Gigabyte")
}

/// Search for entities (classes and individuals) by label
#[tauri::command]
#[allow(non_snake_case)]
pub async fn entity__search(
    query: String,
    limit: Option<usize>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    // Use EAVTO executor for async read (won't block UI)
    executor.read(move |conn| {
        let limit = limit.unwrap_or(100);
        let mut results = Vec::new();

        // Search classes using OWL abstraction
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

        // Search individuals using OWL abstraction
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

        // Limit total results
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
    // Use EAVTO executor for async read (won't block UI)
    executor.read(move |conn| {
        // Determine entity type by checking what it is
        let entity_type = determine_entity_type(conn, &entity_id)?;

        let data = match entity_type {
            EntityType::Class => get_class_data(conn, &entity_id)?,
            EntityType::Individual => get_individual_data(conn, &entity_id)?,
        };

        serde_json::to_string(&data).map_err(|e| e.to_string())
    }).await
}

fn determine_entity_type(conn: &Connection, entity_id: &str) -> Result<EntityType, String> {
    // Check if it's a class (has rdf:type owl:Class)
    let class = Class::new(entity_id);
    if class.exists(conn).map_err(|e| e.to_string())? {
        return Ok(EntityType::Class);
    }

    // Check if it's an individual (has rdf:type pointing to something other than owl:Class)
    let individual = Individual::new(entity_id);
    if individual.exists(conn).map_err(|e| e.to_string())? {
        return Ok(EntityType::Individual);
    }

    Err(format!("Entity {} not found or unknown type", entity_id))
}

fn get_class_data(conn: &Connection, class_id: &str) -> Result<EntityData, String> {
    // Get complete class data using OWL abstraction
    let class = Class::get(conn, class_id)
        .map_err(|e| e.to_string())?;

    let label = class.label.unwrap_or_else(|| class_id.to_string());
    let icon = class.icon;
    let comment = class.comment;

    // Get instances separately (may be thousands)
    let instance_iris = Class::get_instances(conn, class_id)
        .map_err(|e| e.to_string())?;

    // Convert instances to Thing objects
    let instances: Vec<crate::owl::Thing> = instance_iris.iter()
        .map(|iri| crate::owl::Thing::get(conn, iri))
        .collect();

    // Build graph visualization
    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut added_node_ids = std::collections::HashSet::new();

    // Add the class itself
    nodes.push(GraphNode {
        id: class_id.to_string(),
        label: label.clone(),
        icon: icon.clone(),
        group: 1,
        is_broken_ref: None,
    });
    added_node_ids.insert(class_id.to_string());

    // Add super-classes as nodes
    for super_class in &class.super_classes {
        if !added_node_ids.contains(&super_class.iri) {
            nodes.push(GraphNode {
                id: super_class.iri.clone(),
                label: super_class.label.clone(),
                icon: super_class.icon.clone(),
                group: 1,
                is_broken_ref: None,
            });
            added_node_ids.insert(super_class.iri.clone());
        }

        links.push(GraphLink {
            source: class_id.to_string(),
            target: super_class.iri.clone(),
            label: "subClassOf".to_string(),
        });
    }

    // Add sub-classes as nodes
    for sub_class in &class.sub_classes {
        if !added_node_ids.contains(&sub_class.iri) {
            nodes.push(GraphNode {
                id: sub_class.iri.clone(),
                label: sub_class.label.clone(),
                icon: sub_class.icon.clone(),
                group: 1,
                is_broken_ref: None,
            });
            added_node_ids.insert(sub_class.iri.clone());
        }

        links.push(GraphLink {
            source: sub_class.iri.clone(),
            target: class_id.to_string(),
            label: "subClassOf".to_string(),
        });
    }

    // Add instances as nodes
    for instance in &instances {
        if !added_node_ids.contains(&instance.iri) {
            nodes.push(GraphNode {
                id: instance.iri.clone(),
                label: instance.label.clone(),
                icon: instance.icon.clone(),
                group: 6,
                is_broken_ref: None,
            });
            added_node_ids.insert(instance.iri.clone());
        }

        links.push(GraphLink {
            source: instance.iri.clone(),
            target: class_id.to_string(),
            label: "type".to_string(),
        });
    }

    // Build properties list from class.properties
    let mut properties = Vec::new();
    for (property_iri, source_class_iri) in &class.properties {
        // Get property data using OWL abstraction
        let prop = Property::get(conn, property_iri)
            .map_err(|e| e.to_string())?;

        let property_label = prop.label.unwrap_or_else(|| property_iri.clone());
        let property_comment = prop.comment;

        // Determine if it's an object property and get range
        let is_object_property = prop.property_type == crate::owl::PropertyType::ObjectProperty;
        let (value, value_label, value_icon) = prop.ranges.first()
            .map(|range_iri| {
                let range_thing = crate::owl::Thing::get(conn, range_iri);
                (range_iri.clone(), range_thing.label, range_thing.icon)
            })
            .unwrap_or_else(|| ("owl:Thing".to_string(), "Any".to_string(), None));

        // Get source class label (only if different from current class)
        let (source_class, source_class_label) = if source_class_iri != class_id {
            let source_thing = crate::owl::Thing::get(conn, source_class_iri);
            (Some(source_class_iri.clone()), Some(source_thing.label))
        } else {
            (None, None)
        };

        // Get unit symbol if property has a unit (e.g., "GB" instead of "GigaByte")
        let (unit, unit_label) = if let Some(unit_iri) = &prop.unit {
            // Try to get qudt:symbol first, fallback to rdfs:label
            let symbol_result = crate::eavto::query::get_by_entity_predicate(conn, unit_iri, "qudt:symbol");
            let unit_display = if let Ok(result) = symbol_result {
                result.triples.first()
                    .and_then(|t| t.object.as_literal())
                    .map(|s| s.to_string())
            } else {
                None
            };

            // Fallback to label if no symbol found
            let unit_display = unit_display.or_else(|| {
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
        });
    }

    Ok(EntityData {
        id: class_id.to_string(),
        label,
        entity_type: EntityType::Class,
        icon,
        comment,
        super_classes: class.super_classes.clone(),
        sub_classes: class.sub_classes.clone(),
        instances,
        types: vec![],
        properties,
        nodes,
        links,
    })
}

fn get_individual_data(conn: &Connection, individual_id: &str) -> Result<EntityData, String> {
    // Get complete individual data using OWL abstraction
    let individual = Individual::get(conn, individual_id)
        .map_err(|e| e.to_string())?;

    let label = individual.label.unwrap_or_else(|| individual_id.to_string());
    let icon = individual.icon;
    let comment = individual.comment;

    // Build properties list
    let mut properties = Vec::new();
    for (property_iri, value_obj) in &individual.properties {
        // Get property metadata using OWL abstraction
        let prop_result = Property::get(conn, property_iri);
        let (property_label, property_comment, unit, unit_label) = if let Ok(prop) = prop_result {
            let label = prop.label.clone().unwrap_or_else(|| property_iri.clone());
            let comment = prop.comment.clone();

            // Get unit symbol if property has a unit (e.g., "GB" instead of "GigaByte")
            let (unit, unit_label) = if let Some(unit_iri) = &prop.unit {
                // Try to get qudt:symbol first, fallback to rdfs:label
                let symbol_result = crate::eavto::query::get_by_entity_predicate(conn, unit_iri, "qudt:symbol");
                let unit_display = if let Ok(result) = symbol_result {
                    result.triples.first()
                        .and_then(|t| t.object.as_literal())
                        .map(|s| s.to_string())
                } else {
                    None
                };

                // Fallback to label if no symbol found
                let unit_display = unit_display.or_else(|| {
                    let unit_thing = crate::owl::Thing::get(conn, unit_iri);
                    Some(unit_thing.label)
                });

                (Some(unit_iri.clone()), unit_display)
            } else {
                (None, None)
            };

            (label, comment, unit, unit_label)
        } else {
            (property_iri.clone(), None, None, None)
        };

        // Determine type and extract value
        let is_object_property = value_obj.is_iri();
        let value = if is_object_property {
            value_obj.as_iri().unwrap_or("").to_string()
        } else {
            value_obj.as_literal().unwrap_or_default()
        };

        // For object properties, get the label and icon of the target entity
        // For datatype properties, get the icon of the datatype
        let (value_label, value_icon) = if is_object_property {
            let target_thing = crate::owl::Thing::get(conn, &value);
            (Some(target_thing.label), target_thing.icon)
        } else {
            // Get datatype icon for literal values
            let datatype_icon = if let Some(dt_iri) = value_obj.datatype() {
                let datatype_thing = crate::owl::Thing::get(conn, dt_iri);
                datatype_thing.icon
            } else {
                None
            };
            (None, datatype_icon)
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
        });
    }

    // Build graph visualization
    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut added_node_ids = std::collections::HashSet::new();

    // Add the individual itself
    nodes.push(GraphNode {
        id: individual_id.to_string(),
        label: label.clone(),
        icon: icon.clone(),
        group: 6,
        is_broken_ref: None,
    });
    added_node_ids.insert(individual_id.to_string());

    // Add its classes as nodes
    for class_thing in &individual.types {
        if !added_node_ids.contains(&class_thing.iri) {
            nodes.push(GraphNode {
                id: class_thing.iri.clone(),
                label: class_thing.label.clone(),
                icon: class_thing.icon.clone(),
                group: 1,
                is_broken_ref: None,
            });
            added_node_ids.insert(class_thing.iri.clone());
        }

        links.push(GraphLink {
            source: individual_id.to_string(),
            target: class_thing.iri.clone(),
            label: "type".to_string(),
        });
    }

    // Add related individuals via ObjectProperties (outgoing relationships)
    for prop in &properties {
        if prop.is_object_property {
            if !added_node_ids.contains(&prop.value) {
                let related_thing = crate::owl::Thing::get(conn, &prop.value);
                let entity_exists_flag = Individual::new(&prop.value).exists(conn).unwrap_or(false);

                nodes.push(GraphNode {
                    id: prop.value.clone(),
                    label: related_thing.label,
                    icon: if entity_exists_flag { related_thing.icon } else { Some("warning".to_string()) },
                    group: 6,
                    is_broken_ref: if entity_exists_flag { None } else { Some(true) },
                });
                added_node_ids.insert(prop.value.clone());
            }

            links.push(GraphLink {
                source: individual_id.to_string(),
                target: prop.value.clone(),
                label: prop.property_label.clone(),
            });
        }
    }

    // Add related individuals via incoming ObjectProperties (backlinks)
    // Still need raw query for reverse lookups
    let backlink_query = "SELECT subject, predicate
                          FROM triples
                          WHERE object = ? AND object_type = 'iri'
                          AND predicate != 'rdf:type'
                          AND retracted = 0";

    let mut stmt = conn.prepare(backlink_query).map_err(|e| e.to_string())?;
    let backlink_rows = stmt.query_map([individual_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
        ))
    }).map_err(|e| e.to_string())?;

    for row in backlink_rows {
        let (subject, predicate_iri) = row.map_err(|e| e.to_string())?;

        if !added_node_ids.contains(&subject) {
            let subject_thing = crate::owl::Thing::get(conn, &subject);
            let entity_exists_flag = Individual::new(&subject).exists(conn).unwrap_or(false);

            nodes.push(GraphNode {
                id: subject.clone(),
                label: subject_thing.label,
                icon: if entity_exists_flag { subject_thing.icon } else { Some("warning".to_string()) },
                group: 6,
                is_broken_ref: if entity_exists_flag { None } else { Some(true) },
            });
            added_node_ids.insert(subject.clone());
        }

        // Get property label
        let prop_label = Property::get(conn, &predicate_iri)
            .ok()
            .and_then(|p| p.label)
            .unwrap_or_else(|| predicate_iri.clone());

        links.push(GraphLink {
            source: subject,
            target: individual_id.to_string(),
            label: prop_label,
        });
    }

    Ok(EntityData {
        id: individual_id.to_string(),
        label,
        entity_type: EntityType::Individual,
        icon,
        comment,
        super_classes: vec![],
        sub_classes: vec![],
        instances: vec![],
        types: individual.types.clone(),
        properties,
        nodes,
        links,
    })
}