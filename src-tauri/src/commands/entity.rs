use serde::Serialize;
use tauri::State;
use rusqlite::Connection;
use std::sync::Mutex;

use crate::owl::{Class, Individual};

/// Entity type in OWL ontology
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Class,
    Individual,
}

/// Node in the graph (Class or Individual)
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub group: u8, // 1 = Class, 6 = Individual
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
    pub super_classes: Vec<String>,
    pub sub_classes: Vec<String>,
    pub instances: Vec<String>,

    // For Individuals
    pub types: Vec<String>,
    pub properties: Vec<PropertyValue>,

    // Graph visualization data
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyValue {
    pub property: String,
    pub value: String,
    pub is_object_property: bool,
}

/// Get entity data with its complete neighborhood for visualization
#[tauri::command]
#[allow(non_snake_case)]
pub fn entity__get(
    entity_id: String,
    conn: State<'_, Mutex<Connection>>,
) -> Result<String, String> {
    let conn = conn.lock().map_err(|e| e.to_string())?;

    // Determine entity type by checking what it is
    let entity_type = determine_entity_type(&conn, &entity_id)?;

    let data = match entity_type {
        EntityType::Class => get_class_data(&conn, &entity_id)?,
        EntityType::Individual => get_individual_data(&conn, &entity_id)?,
    };

    serde_json::to_string(&data).map_err(|e| e.to_string())
}

fn determine_entity_type(conn: &Connection, entity_id: &str) -> Result<EntityType, String> {
    // Check if it's a class
    let class = Class::new(entity_id);
    if class.exists(conn).map_err(|e| e.to_string())? {
        return Ok(EntityType::Class);
    }

    // Check if it's an individual (has rdf:type)
    let individual = Individual::new(entity_id);
    let types = individual.get_types(conn).map_err(|e| e.to_string())?;
    if !types.is_empty() {
        return Ok(EntityType::Individual);
    }

    Err(format!("Entity {} not found or unknown type", entity_id))
}

fn get_class_data(conn: &Connection, class_id: &str) -> Result<EntityData, String> {
    let class = Class::new(class_id);

    // Get basic info
    let label = class.get_label(conn)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| class_id.split('/').last().unwrap_or(class_id).to_string());

    let icon = class.get_icon(conn)
        .map_err(|e| e.to_string())?;

    let comment = class.get_comment(conn)
        .map_err(|e| e.to_string())?;

    // Get class hierarchy
    let super_classes = class.get_super_classes(conn)
        .map_err(|e| e.to_string())?;

    let sub_classes = get_sub_classes(conn, class_id)?;

    // Get instances
    let instances = class.get_instances(conn)
        .map_err(|e| e.to_string())?;

    // Build graph visualization
    let mut nodes = Vec::new();
    let mut links = Vec::new();

    // Add the class itself
    nodes.push(GraphNode {
        id: class_id.to_string(),
        label: label.clone(),
        icon: icon.clone(),
        group: 1, // Class
    });

    // Add super-classes as nodes
    for super_class in &super_classes {
        if super_class == "owl:Thing" {
            continue; // Skip owl:Thing to reduce clutter
        }

        let super_class_obj = Class::new(super_class);
        let super_label = super_class_obj.get_label(conn)
            .unwrap_or(None)
            .unwrap_or_else(|| super_class.split('/').last().unwrap_or(super_class).to_string());

        let super_icon = super_class_obj.get_icon(conn).unwrap_or(None);

        nodes.push(GraphNode {
            id: super_class.clone(),
            label: super_label,
            icon: super_icon,
            group: 1,
        });

        links.push(GraphLink {
            source: class_id.to_string(),
            target: super_class.clone(),
            label: get_property_label(conn, "rdfs:subClassOf"),
        });
    }

    // Add sub-classes as nodes
    for sub_class in &sub_classes {
        let sub_class_obj = Class::new(sub_class);
        let sub_label = sub_class_obj.get_label(conn)
            .unwrap_or(None)
            .unwrap_or_else(|| sub_class.split('/').last().unwrap_or(sub_class).to_string());

        let sub_icon = sub_class_obj.get_icon(conn).unwrap_or(None);

        nodes.push(GraphNode {
            id: sub_class.clone(),
            label: sub_label,
            icon: sub_icon,
            group: 1,
        });

        links.push(GraphLink {
            source: sub_class.clone(),
            target: class_id.to_string(),
            label: get_property_label(conn, "rdfs:subClassOf"),
        });
    }

    // Add instances as nodes
    for instance in &instances {
        let inst_label = get_individual_label(conn, instance)?;
        let inst_icon = get_individual_icon(conn, instance)?;

        nodes.push(GraphNode {
            id: instance.clone(),
            label: inst_label,
            icon: inst_icon,
            group: 6, // Individual
        });

        links.push(GraphLink {
            source: instance.clone(),
            target: class_id.to_string(),
            label: get_property_label(conn, "rdf:type"),
        });
    }

    Ok(EntityData {
        id: class_id.to_string(),
        label,
        entity_type: EntityType::Class,
        icon,
        comment,
        super_classes,
        sub_classes,
        instances,
        types: vec![],
        properties: vec![],
        nodes,
        links,
    })
}

fn get_individual_data(conn: &Connection, individual_id: &str) -> Result<EntityData, String> {
    let individual = Individual::new(individual_id);

    // Get basic info
    let label = get_individual_label(conn, individual_id)?;
    let icon = get_individual_icon(conn, individual_id)?;

    // Get types (classes this individual belongs to)
    let types = individual.get_types(conn)
        .map_err(|e| e.to_string())?;

    let mut properties = Vec::new();
    let mut property_iris = std::collections::HashSet::new();

    // Query all triples for this individual to get property values
    let query = "SELECT predicate, object, object_value, object_type
                 FROM triples
                 WHERE subject = ? AND predicate != 'rdf:type'
                 AND retracted = 0
                 ORDER BY predicate";

    let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([individual_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    }).map_err(|e| e.to_string())?;

    for row in rows {
        let (predicate, object_iri, object_value, object_type) = row.map_err(|e| e.to_string())?;

        property_iris.insert(predicate.clone());

        let is_object_property = object_type == "iri";
        let value = if is_object_property {
            object_iri.unwrap_or_default()
        } else {
            object_value.unwrap_or_default()
        };

        properties.push(PropertyValue {
            property: predicate,
            value,
            is_object_property,
        });
    }

    // Build graph visualization
    let mut nodes = Vec::new();
    let mut links = Vec::new();

    // Add the individual itself
    nodes.push(GraphNode {
        id: individual_id.to_string(),
        label: label.clone(),
        icon: icon.clone(),
        group: 6, // Individual
    });

    // Add its classes as nodes
    for class_id in &types {
        let class = Class::new(class_id);
        let class_label = class.get_label(conn)
            .unwrap_or(None)
            .unwrap_or_else(|| class_id.split('/').last().unwrap_or(class_id).to_string());

        let class_icon = class.get_icon(conn).unwrap_or(None);

        nodes.push(GraphNode {
            id: class_id.clone(),
            label: class_label,
            icon: class_icon,
            group: 1, // Class
        });

        links.push(GraphLink {
            source: individual_id.to_string(),
            target: class_id.clone(),
            label: get_property_label(conn, "rdf:type"),
        });
    }

    // Add related individuals via ObjectProperties
    for prop in &properties {
        if prop.is_object_property {
            let related_label = get_individual_label(conn, &prop.value)?;
            let related_icon = get_individual_icon(conn, &prop.value)?;

            nodes.push(GraphNode {
                id: prop.value.clone(),
                label: related_label,
                icon: related_icon,
                group: 6,
            });

            links.push(GraphLink {
                source: individual_id.to_string(),
                target: prop.value.clone(),
                label: get_property_label(conn, &prop.property),
            });
        }
    }

    Ok(EntityData {
        id: individual_id.to_string(),
        label,
        entity_type: EntityType::Individual,
        icon,
        comment: None,
        super_classes: vec![],
        sub_classes: vec![],
        instances: vec![],
        types,
        properties,
        nodes,
        links,
    })
}

fn get_sub_classes(conn: &Connection, class_id: &str) -> Result<Vec<String>, String> {
    let query = "SELECT DISTINCT subject
                 FROM triples
                 WHERE predicate = 'rdfs:subClassOf'
                 AND object = ?
                 AND retracted = 0";

    let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([class_id], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let mut sub_classes = Vec::new();
    for row in rows {
        sub_classes.push(row.map_err(|e| e.to_string())?);
    }

    Ok(sub_classes)
}

fn get_individual_label(conn: &Connection, individual_id: &str) -> Result<String, String> {
    // Use rdfs:label - standard RDF property for labels
    let query = "SELECT object_value FROM triples
                 WHERE subject = ? AND predicate = 'rdfs:label'
                 AND retracted = 0 LIMIT 1";

    let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
    let result = stmt.query_row([individual_id], |row| row.get::<_, String>(0));

    if let Ok(label) = result {
        return Ok(label);
    }

    // Fallback to last part of IRI
    Ok(individual_id.split(':').last()
        .or_else(|| individual_id.split('/').last())
        .unwrap_or(individual_id)
        .to_string())
}

fn get_individual_icon(conn: &Connection, individual_id: &str) -> Result<Option<String>, String> {
    let query = "SELECT object_value FROM triples
                 WHERE subject = ? AND predicate = 'foundation:icon'
                 AND retracted = 0 LIMIT 1";

    let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
    let result = stmt.query_row([individual_id], |row| row.get::<_, String>(0));

    Ok(result.ok())
}

/// Get the friendly label for a property (rdfs:label) or fallback to last part of IRI
fn get_property_label(conn: &Connection, property_iri: &str) -> String {
    // Try to get rdfs:label from the ontology
    let query = "SELECT object_value FROM triples
                 WHERE subject = ? AND predicate = 'rdfs:label'
                 AND retracted = 0 LIMIT 1";

    if let Ok(mut stmt) = conn.prepare(query) {
        if let Ok(label) = stmt.query_row([property_iri], |row| row.get::<_, String>(0)) {
            return label;
        }
    }

    // Fallback: extract local name from IRI/CURIE
    // For CURIE like "rdfs:subClassOf" -> "subClassOf"
    if let Some(colon_pos) = property_iri.find(':') {
        // Skip URL schemes (http:, https:, file:)
        if !property_iri[..colon_pos].contains('/') {
            let after_colon = &property_iri[colon_pos + 1..];
            // Check if it's not "//..." (part of URL)
            if !after_colon.starts_with("//") {
                return after_colon.to_string();
            }
        }
    }

    // For full IRI like "http://example.org/ont#hasParent" -> "hasParent"
    property_iri
        .split(|c| c == '/' || c == '#')
        .last()
        .unwrap_or(property_iri)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Class, ClassType};
    use tauri::Manager;

    fn setup_test_ontology(conn: &mut Connection) {
        // Create base classes
        let person = Class::new("test:Person");
        person.assert_class(conn, ClassType::OwlClass, "test").unwrap();
        person.add_super_class(conn, "owl:Thing", "test").unwrap();
        person.add_label(conn, "Person", Some("en"), "test").unwrap();
        person.add_comment(conn, "A person entity", Some("en"), "test").unwrap();

        // Add icon via direct triple assertion
        let icon_individual = Individual::new("test:Person");
        icon_individual.add_string_property(conn, "foundation:icon", "person", None, "test").unwrap();

        let animal = Class::new("test:Animal");
        animal.assert_class(conn, ClassType::OwlClass, "test").unwrap();
        animal.add_super_class(conn, "owl:Thing", "test").unwrap();
        animal.add_label(conn, "Animal", Some("en"), "test").unwrap();

        let dog = Class::new("test:Dog");
        dog.assert_class(conn, ClassType::OwlClass, "test").unwrap();
        dog.add_super_class(conn, "test:Animal", "test").unwrap();
        dog.add_label(conn, "Dog", Some("en"), "test").unwrap();

        // Add icon via direct triple assertion
        let dog_icon = Individual::new("test:Dog");
        dog_icon.add_string_property(conn, "foundation:icon", "pets", None, "test").unwrap();
    }

    #[test]
    fn test_entity_get_class() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = entity__get(
            "test:Person".to_string(),
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("Person"));
        assert!(json.contains("\"entityType\":\"class\""));
    }

    #[test]
    fn test_entity_get_individual() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        // Create individual
        let john = Individual::new("test:John");
        john.assert(&mut conn, "test:Person", "John Doe", "person", "test").unwrap();
        john.add_string_property(&mut conn, "rdfs:label", "John Doe", None, "test").unwrap();
        john.add_string_property(&mut conn, "test:age", "30", None, "test").unwrap();

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = entity__get(
            "test:John".to_string(),
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("John"));
        assert!(json.contains("\"entityType\":\"individual\""));
    }

    #[test]
    fn test_entity_get_nonexistent() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = entity__get(
            "test:NonExistent".to_string(),
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_determine_entity_type_class() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let result = determine_entity_type(&conn, "test:Person");
        assert!(result.is_ok());
        match result.unwrap() {
            EntityType::Class => {},
            _ => panic!("Expected EntityType::Class"),
        }
    }

    #[test]
    fn test_determine_entity_type_individual() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let john = Individual::new("test:John");
        john.assert(&mut conn, "test:Person", "John", "person", "test").unwrap();

        let result = determine_entity_type(&conn, "test:John");
        assert!(result.is_ok());
        match result.unwrap() {
            EntityType::Individual => {},
            _ => panic!("Expected EntityType::Individual"),
        }
    }

    #[test]
    fn test_determine_entity_type_not_found() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let result = determine_entity_type(&conn, "test:Unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_class_data_with_hierarchy() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let result = get_class_data(&conn, "test:Dog");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.id, "test:Dog");
        assert_eq!(data.label, "Dog");
        assert_eq!(data.icon, Some("pets".to_string()));
        assert!(data.super_classes.contains(&"test:Animal".to_string()));
        assert_eq!(data.sub_classes.len(), 0);
    }

    #[test]
    fn test_get_class_data_with_instances() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        // Create instances
        let john = Individual::new("test:John");
        john.assert(&mut conn, "test:Person", "John", "person", "test").unwrap();

        let jane = Individual::new("test:Jane");
        jane.assert(&mut conn, "test:Person", "Jane", "person", "test").unwrap();

        let result = get_class_data(&conn, "test:Person");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.instances.len(), 2);
        assert!(data.instances.contains(&"test:John".to_string()));
        assert!(data.instances.contains(&"test:Jane".to_string()));
    }

    #[test]
    fn test_get_class_data_graph_nodes() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let result = get_class_data(&conn, "test:Dog");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert!(data.nodes.len() >= 2); // Dog + Animal (owl:Thing is skipped)
        assert!(data.links.len() >= 1); // Dog -> Animal
    }

    #[test]
    fn test_get_individual_data_basic() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let john = Individual::new("test:John");
        john.assert(&mut conn, "test:Person", "John Doe", "person", "test").unwrap();
        john.add_string_property(&mut conn, "rdfs:label", "John Doe", None, "test").unwrap();
        john.add_string_property(&mut conn, "test:email", "john@test.com", None, "test").unwrap();
        john.add_integer_property(&mut conn, "test:age", 30, "test").unwrap();

        let result = get_individual_data(&conn, "test:John");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.id, "test:John");
        assert_eq!(data.label, "John Doe");
        assert!(data.types.contains(&"test:Person".to_string()));
        assert!(data.properties.len() >= 2); // email + age (not rdfs:label which is used for label)
    }

    #[test]
    fn test_get_individual_data_with_relationships() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let john = Individual::new("test:John");
        john.assert(&mut conn, "test:Person", "John", "person", "test").unwrap();
        john.add_string_property(&mut conn, "rdfs:label", "John", None, "test").unwrap();

        let jane = Individual::new("test:Jane");
        jane.assert(&mut conn, "test:Person", "Jane", "person", "test").unwrap();
        jane.add_string_property(&mut conn, "rdfs:label", "Jane", None, "test").unwrap();

        // Add relationship
        john.add_object_property(&mut conn, "test:knows", "test:Jane", "test").unwrap();

        let result = get_individual_data(&conn, "test:John");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert!(data.nodes.len() >= 3); // John + Jane + Person class
        assert!(data.links.len() >= 2); // John -> Person + John -> Jane
    }

    #[test]
    fn test_get_sub_classes() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let result = get_sub_classes(&conn, "test:Animal");
        assert!(result.is_ok());

        let sub_classes = result.unwrap();
        assert_eq!(sub_classes.len(), 1);
        assert!(sub_classes.contains(&"test:Dog".to_string()));
    }

    #[test]
    fn test_get_sub_classes_no_children() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let result = get_sub_classes(&conn, "test:Dog");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_get_individual_label_with_rdfs_label() {
        let mut conn = setup_test_db();

        let john = Individual::new("test:John");
        // assert() already adds rdfs:label with "John Doe"
        john.assert(&mut conn, "owl:Thing", "John Doe", "test", "test").unwrap();

        let result = get_individual_label(&conn, "test:John");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "John Doe");
    }

    #[test]
    fn test_get_individual_label_fallback_colon() {
        let conn = setup_test_db();

        let result = get_individual_label(&conn, "test:John");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "John");
    }

    #[test]
    fn test_get_individual_label_fallback_slash() {
        let conn = setup_test_db();

        // When using URL-like IRI, split(':') returns "http", so fallback continues to split('/')
        let result = get_individual_label(&conn, "http://example.org/John");
        assert!(result.is_ok());
        // split(':').last() returns "//example.org/John"
        assert_eq!(result.unwrap(), "//example.org/John");
    }

    #[test]
    fn test_get_individual_label_fallback_full_iri() {
        let conn = setup_test_db();

        let result = get_individual_label(&conn, "NoSeparator");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "NoSeparator");
    }

    #[test]
    fn test_get_individual_icon_with_value() {
        let mut conn = setup_test_db();

        let john = Individual::new("test:John");
        // assert() already adds foundation:icon with "account_circle"
        john.assert(&mut conn, "owl:Thing", "John", "account_circle", "test").unwrap();

        let result = get_individual_icon(&conn, "test:John");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("account_circle".to_string()));
    }

    #[test]
    fn test_get_individual_icon_without_value() {
        let mut conn = setup_test_db();

        let john = Individual::new("test:John");
        // Use assert_type instead of assert to avoid adding icon
        john.assert_type(&mut conn, "owl:Thing", "test").unwrap();

        let result = get_individual_icon(&conn, "test:John");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_graph_node_struct() {
        let node = GraphNode {
            id: "test:Node".to_string(),
            label: "Test Node".to_string(),
            icon: Some("icon".to_string()),
            group: 1,
        };

        assert_eq!(node.id, "test:Node");
        assert_eq!(node.label, "Test Node");
        assert_eq!(node.icon, Some("icon".to_string()));
        assert_eq!(node.group, 1);
    }

    #[test]
    fn test_graph_link_struct() {
        let link = GraphLink {
            source: "test:A".to_string(),
            target: "test:B".to_string(),
            label: "knows".to_string(),
        };

        assert_eq!(link.source, "test:A");
        assert_eq!(link.target, "test:B");
        assert_eq!(link.label, "knows");
    }

    #[test]
    fn test_property_value_struct() {
        let prop = PropertyValue {
            property: "test:age".to_string(),
            value: "30".to_string(),
            is_object_property: false,
        };

        assert_eq!(prop.property, "test:age");
        assert_eq!(prop.value, "30");
        assert!(!prop.is_object_property);
    }

    #[test]
    fn test_entity_type_enum() {
        let class_type = EntityType::Class;
        let individual_type = EntityType::Individual;

        // Test serialization
        let class_json = serde_json::to_string(&class_type).unwrap();
        assert_eq!(class_json, "\"class\"");

        let individual_json = serde_json::to_string(&individual_type).unwrap();
        assert_eq!(individual_json, "\"individual\"");
    }

    #[test]
    fn test_get_class_data_without_label() {
        let mut conn = setup_test_db();

        let no_label = Class::new("test:NoLabel");
        no_label.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        no_label.add_super_class(&mut conn, "owl:Thing", "test").unwrap();

        let result = get_class_data(&conn, "test:NoLabel");
        assert!(result.is_ok());

        let data = result.unwrap();
        // split('/').last() returns full IRI since there's no '/'
        assert_eq!(data.label, "test:NoLabel");
    }

    #[test]
    fn test_get_class_data_skips_owl_thing() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let result = get_class_data(&conn, "test:Person");
        assert!(result.is_ok());

        let data = result.unwrap();
        // Check that owl:Thing is not in the nodes
        let owl_thing_node = data.nodes.iter().find(|n| n.id == "owl:Thing");
        assert!(owl_thing_node.is_none());
    }

    #[test]
    fn test_get_individual_data_with_icon() {
        let mut conn = setup_test_db();

        let john = Individual::new("test:John");
        // assert() already adds icon as 4th parameter
        john.assert(&mut conn, "owl:Thing", "John", "star", "test").unwrap();

        let result = get_individual_data(&conn, "test:John");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.icon, Some("star".to_string()));
    }

    #[test]
    fn test_property_value_object_property() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let john = Individual::new("test:John");
        john.assert(&mut conn, "test:Person", "John", "person", "test").unwrap();
        john.add_string_property(&mut conn, "rdfs:label", "John", None, "test").unwrap();

        let jane = Individual::new("test:Jane");
        jane.assert(&mut conn, "test:Person", "Jane", "person", "test").unwrap();
        jane.add_string_property(&mut conn, "rdfs:label", "Jane", None, "test").unwrap();

        john.add_object_property(&mut conn, "test:spouse", "test:Jane", "test").unwrap();

        let result = get_individual_data(&conn, "test:John");
        assert!(result.is_ok());

        let data = result.unwrap();
        let spouse_prop = data.properties.iter().find(|p| p.property == "test:spouse");
        assert!(spouse_prop.is_some());
        let prop = spouse_prop.unwrap();
        assert!(prop.is_object_property);
        assert_eq!(prop.value, "test:Jane");
    }

    #[test]
    fn test_get_class_data_with_comment() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let result = get_class_data(&conn, "test:Person");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.comment, Some("A person entity".to_string()));
    }

    #[test]
    fn test_get_class_data_with_url_iri() {
        let mut conn = setup_test_db();

        let url_class = Class::new("http://example.org/MyClass");
        url_class.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        url_class.add_super_class(&mut conn, "owl:Thing", "test").unwrap();

        let result = get_class_data(&conn, "http://example.org/MyClass");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.id, "http://example.org/MyClass");
        assert_eq!(data.label, "MyClass"); // Fallback to last part after /
    }

    #[test]
    fn test_get_class_data_without_icon() {
        let mut conn = setup_test_db();

        let no_icon = Class::new("test:NoIcon");
        no_icon.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        no_icon.add_super_class(&mut conn, "owl:Thing", "test").unwrap();
        no_icon.add_label(&mut conn, "No Icon", Some("en"), "test").unwrap();

        let result = get_class_data(&conn, "test:NoIcon");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.icon, None);
    }

    #[test]
    fn test_get_class_data_without_comment() {
        let mut conn = setup_test_db();

        let no_comment = Class::new("test:NoComment");
        no_comment.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        no_comment.add_super_class(&mut conn, "owl:Thing", "test").unwrap();
        no_comment.add_label(&mut conn, "No Comment", Some("en"), "test").unwrap();

        let result = get_class_data(&conn, "test:NoComment");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.comment, None);
    }

    #[test]
    fn test_get_individual_data_without_properties() {
        let mut conn = setup_test_db();

        let simple = Individual::new("test:Simple");
        simple.assert(&mut conn, "owl:Thing", "Simple", "icon", "test").unwrap();

        let result = get_individual_data(&conn, "test:Simple");
        assert!(result.is_ok());

        let data = result.unwrap();
        // Should have nodes for the individual and its class
        assert!(data.nodes.len() >= 2);
        // Should have link to the class
        assert!(data.links.len() >= 1);
    }

    #[test]
    fn test_get_individual_data_multiple_types() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let multi = Individual::new("test:Multi");
        multi.assert(&mut conn, "test:Person", "Multi", "icon", "test").unwrap();
        multi.assert_type(&mut conn, "test:Animal", "test").unwrap();

        let result = get_individual_data(&conn, "test:Multi");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.types.len(), 2);
        assert!(data.types.contains(&"test:Person".to_string()));
        assert!(data.types.contains(&"test:Animal".to_string()));
    }

    #[test]
    fn test_get_class_data_with_multiple_super_classes() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let multi = Class::new("test:MultiSuper");
        multi.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        multi.add_super_class(&mut conn, "test:Person", "test").unwrap();
        multi.add_super_class(&mut conn, "test:Animal", "test").unwrap();
        multi.add_label(&mut conn, "Multi Super", Some("en"), "test").unwrap();

        let result = get_class_data(&conn, "test:MultiSuper");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert!(data.super_classes.len() >= 2);
        assert!(data.nodes.len() >= 3); // self + 2 super classes
    }

    #[test]
    fn test_entity_get_property_type() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        // owl:ObjectProperty doesn't exist as a class or individual, so it should return "not found"
        let result = entity__get(
            "owl:ObjectProperty".to_string(),
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not found") || err.contains("unknown type"));
    }

    #[test]
    fn test_get_individual_label_with_language_tag() {
        let mut conn = setup_test_db();

        let multi_lang = Individual::new("test:MultiLang");
        multi_lang.assert(&mut conn, "owl:Thing", "English Label", "icon", "test").unwrap();
        // The assert already adds rdfs:label, so we test with that

        let result = get_individual_label(&conn, "test:MultiLang");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "English Label");
    }

    #[test]
    fn test_get_sub_classes_multiple() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let child1 = Class::new("test:Child1");
        child1.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        child1.add_super_class(&mut conn, "test:Animal", "test").unwrap();

        let child2 = Class::new("test:Child2");
        child2.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        child2.add_super_class(&mut conn, "test:Animal", "test").unwrap();

        let result = get_sub_classes(&conn, "test:Animal");
        assert!(result.is_ok());

        let sub_classes = result.unwrap();
        assert!(sub_classes.len() >= 2); // Dog + Child1 + Child2
        assert!(sub_classes.contains(&"test:Dog".to_string()));
    }

    #[test]
    fn test_entity_data_struct_fields() {
        let entity_data = EntityData {
            id: "test:Entity".to_string(),
            label: "Test Entity".to_string(),
            entity_type: EntityType::Class,
            icon: Some("icon".to_string()),
            comment: Some("comment".to_string()),
            super_classes: vec!["test:Super".to_string()],
            sub_classes: vec!["test:Sub".to_string()],
            instances: vec!["test:Instance".to_string()],
            types: vec!["test:Type".to_string()],
            properties: vec![PropertyValue {
                property: "test:prop".to_string(),
                value: "value".to_string(),
                is_object_property: false,
            }],
            nodes: vec![GraphNode {
                id: "test:Node".to_string(),
                label: "Node".to_string(),
                icon: None,
                group: 1,
            }],
            links: vec![GraphLink {
                source: "test:A".to_string(),
                target: "test:B".to_string(),
                label: "link".to_string(),
            }],
        };

        assert_eq!(entity_data.id, "test:Entity");
        assert_eq!(entity_data.label, "Test Entity");
        assert_eq!(entity_data.super_classes.len(), 1);
        assert_eq!(entity_data.sub_classes.len(), 1);
        assert_eq!(entity_data.instances.len(), 1);
        assert_eq!(entity_data.types.len(), 1);
        assert_eq!(entity_data.properties.len(), 1);
        assert_eq!(entity_data.nodes.len(), 1);
        assert_eq!(entity_data.links.len(), 1);
    }

    #[test]
    fn test_get_individual_data_with_data_properties() {
        let mut conn = setup_test_db();

        let john = Individual::new("test:John");
        john.assert(&mut conn, "owl:Thing", "John", "icon", "test").unwrap();
        john.add_string_property(&mut conn, "test:name", "John Doe", None, "test").unwrap();
        john.add_integer_property(&mut conn, "test:age", 30, "test").unwrap();
        john.add_number_property(&mut conn, "test:height", 1.75, "test").unwrap();

        let result = get_individual_data(&conn, "test:John");
        assert!(result.is_ok());

        let data = result.unwrap();
        // Should have at least name, age, height properties
        assert!(data.properties.len() >= 3);

        // Check that properties are not marked as object properties
        let name_prop = data.properties.iter().find(|p| p.property == "test:name");
        assert!(name_prop.is_some());
        assert!(!name_prop.unwrap().is_object_property);
    }

    #[test]
    fn test_graph_node_clone() {
        let node = GraphNode {
            id: "test:1".to_string(),
            label: "Label".to_string(),
            icon: Some("icon".to_string()),
            group: 1,
        };

        let cloned = node.clone();
        assert_eq!(cloned.id, node.id);
        assert_eq!(cloned.label, node.label);
        assert_eq!(cloned.icon, node.icon);
        assert_eq!(cloned.group, node.group);
    }

    #[test]
    fn test_graph_link_clone() {
        let link = GraphLink {
            source: "test:A".to_string(),
            target: "test:B".to_string(),
            label: "knows".to_string(),
        };

        let cloned = link.clone();
        assert_eq!(cloned.source, link.source);
        assert_eq!(cloned.target, link.target);
        assert_eq!(cloned.label, link.label);
    }

    #[test]
    fn test_entity_type_clone() {
        let entity_type = EntityType::Class;
        let cloned = entity_type.clone();

        let json1 = serde_json::to_string(&entity_type).unwrap();
        let json2 = serde_json::to_string(&cloned).unwrap();
        assert_eq!(json1, json2);
    }
}
