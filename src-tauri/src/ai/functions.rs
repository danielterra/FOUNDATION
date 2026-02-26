use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use rusqlite::Connection;
use crate::owl::{Class, Individual};
use crate::eavto::query;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

impl FunctionDefinition {
    pub fn to_claude_tool(&self) -> crate::ai::providers::ClaudeTool {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.parameters {
            properties.insert(
                param.name.clone(),
                json!({
                    "type": param.param_type,
                    "description": param.description,
                })
            );
            if param.required {
                required.push(param.name.clone());
            }
        }

        let input_schema = json!({
            "type": "object",
            "properties": properties,
            "required": required,
        });

        crate::ai::providers::ClaudeTool {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema,
        }
    }

    pub fn to_openai_function(&self) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.parameters {
            properties.insert(
                param.name.clone(),
                json!({
                    "type": param.param_type,
                    "description": param.description,
                })
            );
            if param.required {
                required.push(param.name.clone());
            }
        }

        json!({
            "name": self.name,
            "description": self.description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResult {
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
}

pub fn get_claude_tools() -> Vec<crate::ai::providers::ClaudeTool> {
    get_available_functions()
        .into_iter()
        .map(|f| f.to_claude_tool())
        .collect()
}

/// Get all available functions for the AI
pub fn get_available_functions() -> Vec<FunctionDefinition> {
    vec![
        FunctionDefinition {
            name: "remember_concepts".to_string(),
            description: "Search your memory for concepts you know about. Find what types of things you've learned.".to_string(),
            parameters: vec![
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "What to search for (partial match, case-insensitive)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "How many results to return (default: 100)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "number".to_string(),
                    description: "How many to skip for pagination (default: 0)".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_concept".to_string(),
            description: "Remember everything you know about a specific concept - what it's related to, what things belong to it.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the concept (e.g., 'foundation:Person')".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_things".to_string(),
            description: "Search your memory for specific things in a concept (e.g., which people, places, or organizations you remember).".to_string(),
            parameters: vec![
                Parameter {
                    name: "class_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Which concept to search in".to_string(),
                    required: true,
                },
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "What to search for (partial match, case-insensitive)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "How many results to return (default: 100)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "number".to_string(),
                    description: "How many to skip for pagination (default: 0)".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_thing".to_string(),
            description: "Remember everything you know about a specific thing - all its details and connections.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing you want to remember".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "learn_concept".to_string(),
            description: "Learn a new concept (e.g., when users mention a new type of thing you should remember).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Unique ID for this concept (e.g., 'foundation:Project')".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name for this concept".to_string(),
                    required: true,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Material icon name".to_string(),
                    required: true,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                },
                Parameter {
                    name: "super_class".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional parent concept ID".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "learn_thing".to_string(),
            description: "Learn about a new specific thing (person, place, organization, etc.). You'll get back an ID to reference it later.".to_string(),
            parameters: vec![
                Parameter {
                    name: "class_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "What concept this thing belongs to".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name of this thing".to_string(),
                    required: true,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Material icon name".to_string(),
                    required: true,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "learn_thing_detail".to_string(),
            description: "Learn a new detail or connection about a thing (e.g., learn that X works at Y, or that someone's birthday is May 15).".to_string(),
            parameters: vec![
                Parameter {
                    name: "instance_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing to add details to".to_string(),
                    required: true,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "What kind of connection or detail (e.g., 'foundation:worksAt')".to_string(),
                    required: true,
                },
                Parameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "The value (text, number, date, or ID of another thing)".to_string(),
                    required: true,
                },
                Parameter {
                    name: "value_type".to_string(),
                    param_type: "string".to_string(),
                    description: "'literal' for text/numbers/dates, 'iri' for connections to other things. Default: 'literal'".to_string(),
                    required: false,
                },
                Parameter {
                    name: "datatype".to_string(),
                    param_type: "string".to_string(),
                    description: "Data type: 'xsd:string', 'xsd:integer', 'xsd:dateTime', etc. Default: 'xsd:string'".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "learn_connection_type".to_string(),
            description: "Learn a new type of connection or detail you can remember about things (e.g., 'worksAt', 'bornOn', 'hasSkill').".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Unique ID for this connection type (e.g., 'foundation:hasAge')".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name for this connection type".to_string(),
                    required: true,
                },
                Parameter {
                    name: "property_type".to_string(),
                    param_type: "string".to_string(),
                    description: "'object' for connections to other things, 'datatype' for simple values (text, numbers, dates)".to_string(),
                    required: true,
                },
                Parameter {
                    name: "domain".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional: which concept this applies to".to_string(),
                    required: false,
                },
                Parameter {
                    name: "range".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional: for 'object' properties, what concept it connects to; for 'datatype', what type of value".to_string(),
                    required: false,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_connection_types".to_string(),
            description: "Search your memory for types of connections or details you can remember (e.g., find all relationship types you know).".to_string(),
            parameters: vec![
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "What to search for (partial match, case-insensitive)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "Maximum number of results to return (default: 100)".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_connection_type".to_string(),
            description: "Remember everything you know about a specific type of connection.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the connection type".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "forget_concept".to_string(),
            description: "Forget a concept (but not the specific things that belong to it).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "The IRI of the class to delete".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "forget_thing".to_string(),
            description: "Forget a specific thing completely (all its details and connections).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing to forget".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "forget_connection_type".to_string(),
            description: "Forget a type of connection.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the connection type to forget".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "update_concept".to_string(),
            description: "Update details about a concept (name, icon, description).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the concept".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "New name".to_string(),
                    required: false,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "New icon".to_string(),
                    required: false,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "New description".to_string(),
                    required: false,
                },
                Parameter {
                    name: "super_class".to_string(),
                    param_type: "string".to_string(),
                    description: "New parent concept ID".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "update_thing".to_string(),
            description: "Update details about a thing (name, icon, description).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "New name".to_string(),
                    required: false,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "New icon".to_string(),
                    required: false,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "New description".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "forget_thing_detail".to_string(),
            description: "Forget a specific detail or connection about a thing (e.g., forget that someone works at X).".to_string(),
            parameters: vec![
                Parameter {
                    name: "instance_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing".to_string(),
                    required: true,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "What kind of detail to forget".to_string(),
                    required: true,
                },
                Parameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "The specific value to forget".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_concept_tree".to_string(),
            description: "Remember the hierarchy of a concept and all its related concepts (parent and children concepts).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the root concept".to_string(),
                    required: true,
                },
                Parameter {
                    name: "max_depth".to_string(),
                    param_type: "number".to_string(),
                    description: "How deep to go (default: 10)".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_things_by_details".to_string(),
            description: "Remember things that have specific details or connections (e.g., remember all people who work at X).".to_string(),
            parameters: vec![
                Parameter {
                    name: "class_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Which concept to search in".to_string(),
                    required: true,
                },
                Parameter {
                    name: "properties".to_string(),
                    param_type: "array".to_string(),
                    description: "Details to match: array of {property: 'IRI', value: 'VALUE'}".to_string(),
                    required: true,
                },
            ],
        },
    ]
}

/// Execute a function call
pub fn execute_function(conn: &mut Connection, call: &FunctionCall) -> FunctionResult {
    match call.name.as_str() {
        "learn_concept" => create_class(conn, &call.arguments),
        "learn_thing" => create_instance(conn, &call.arguments),
        "learn_thing_detail" => add_property_value(conn, &call.arguments),
        "learn_connection_type" => create_property(conn, &call.arguments),
        "remember_concept" => get_class(conn, &call.arguments),
        "remember_thing" => get_instance(conn, &call.arguments),
        "remember_concepts" => search_classes(conn, &call.arguments),
        "remember_things" => search_instances(conn, &call.arguments),
        "remember_connection_type" => get_property(conn, &call.arguments),
        "remember_concept_tree" => get_class_hierarchy(conn, &call.arguments),
        "remember_connection_types" => search_properties(conn, &call.arguments),
        "remember_things_by_details" => find_instances_by_property(conn, &call.arguments),
        "forget_concept" => delete_class(conn, &call.arguments),
        "forget_thing" => delete_instance(conn, &call.arguments),
        "forget_connection_type" => delete_property(conn, &call.arguments),
        "forget_thing_detail" => remove_property_value(conn, &call.arguments),
        "update_concept" => update_class(conn, &call.arguments),
        "update_thing" => update_instance(conn, &call.arguments),
        _ => FunctionResult {
            success: false,
            result: None,
            error: Some(format!("Unknown function: {}", call.name)),
        },
    }
}

fn search_classes(conn: &Connection, args: &Value) -> FunctionResult {
    let query_str = args.get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let limit = args.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;

    let offset = args.get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    match (|| {
        // Get all classes (entities that are rdf:type of owl:Class or rdfs:Class)
        let classes_result = query::get_by_predicate_object(conn, "rdf:type", "owl:Class")?;
        let rdfs_classes_result = query::get_by_predicate_object(conn, "rdf:type", "rdfs:Class")?;

        let mut all_class_iris: Vec<String> = classes_result.triples.into_iter()
            .chain(rdfs_classes_result.triples)
            .map(|t| t.subject)
            .collect();

        all_class_iris.sort();
        all_class_iris.dedup();

        // Get full class info for each
        let mut classes = Vec::new();
        for iri in all_class_iris {
            if let Ok(class) = Class::get(conn, &iri) {
                // Filter by query if provided
                if !query_str.is_empty() {
                    if let Some(label) = &class.label {
                        if !label.to_lowercase().contains(&query_str.to_lowercase()) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                classes.push(serde_json::json!({
                    "iri": class.iri,
                    "label": class.label,
                    "icon": class.icon,
                    "superClasses": class.super_classes.iter().map(|t| t.iri.clone()).collect::<Vec<_>>(),
                    "subClasses": class.sub_classes.iter().map(|t| t.iri.clone()).collect::<Vec<_>>(),
                }));
            }
        }

        // Apply pagination
        let total = classes.len();
        let paginated: Vec<_> = classes.into_iter().skip(offset).take(limit).collect();

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "classes": paginated,
            "count": paginated.len(),
            "total": total,
            "limit": limit,
            "offset": offset,
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

fn get_class(conn: &Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        let class = Class::get(conn, iri)?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": class.iri,
            "label": class.label,
            "icon": class.icon,
            "comment": class.comment,
            "types": class.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "superClasses": class.super_classes.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "subClasses": class.sub_classes.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": class.properties.iter().map(|(prop, source)| serde_json::json!({
                "property": prop,
                "source": source,
            })).collect::<Vec<_>>(),
            "instanceCount": class.backlinks.len(),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

fn search_instances(conn: &Connection, args: &Value) -> FunctionResult {
    let class_iri = match args.get("class_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: class_iri".to_string()),
        },
    };

    let query_str = args.get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let limit = args.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;

    let offset = args.get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    match (|| {
        let instance_iris = Class::get_instances(conn, class_iri)?;

        let mut instances = Vec::new();
        for iri in instance_iris {
            if let Ok(individual) = Individual::get(conn, &iri) {
                // Filter by query if provided
                if !query_str.is_empty() {
                    if let Some(label) = &individual.label {
                        if !label.to_lowercase().contains(&query_str.to_lowercase()) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                instances.push(serde_json::json!({
                    "iri": individual.iri,
                    "label": individual.label,
                    "icon": individual.icon,
                }));
            }
        }

        // Apply pagination
        let total = instances.len();
        let paginated: Vec<_> = instances.into_iter().skip(offset).take(limit).collect();

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "instances": paginated,
            "count": paginated.len(),
            "total": total,
            "limit": limit,
            "offset": offset,
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

fn get_instance(conn: &Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        let individual = Individual::get(conn, iri)?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": individual.iri,
            "label": individual.label,
            "icon": individual.icon,
            "comment": individual.comment,
            "types": individual.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": individual.properties.iter().map(|(prop, value)| serde_json::json!({
                "property": prop,
                "value": value.as_literal().map(|s| s.to_string()).or_else(|| value.as_iri().map(|s| s.to_string())).unwrap_or_default(),
            })).collect::<Vec<_>>(),
            "backlinksCount": individual.backlinks.len(),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

fn create_class(conn: &mut Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    let label = match args.get("label").and_then(|v| v.as_str()) {
        Some(label) => label,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: label".to_string()),
        },
    };

    let icon = match args.get("icon").and_then(|v| v.as_str()) {
        Some(icon) => icon,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: icon".to_string()),
        },
    };

    let comment = args.get("comment").and_then(|v| v.as_str());
    let super_class = args.get("super_class").and_then(|v| v.as_str());

    match (|| {
        let class = Class::new(iri);
        class.assert(conn, crate::owl::ClassType::OwlClass, label, icon, super_class, "ai")?;

        // Add comment if provided
        if let Some(comment_text) = comment {
            use crate::eavto::{store, Triple, Object};
            let comment_triple = Triple::new(iri, "rdfs:comment", Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[comment_triple], "ai")?;
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": iri,
            "message": format!("Class {} created successfully", label),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

fn create_instance(conn: &mut Connection, args: &Value) -> FunctionResult {
    let class_iri = match args.get("class_iri").and_then(|v| v.as_str()) {
        Some(class_iri) => class_iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: class_iri".to_string()),
        },
    };

    let label = match args.get("label").and_then(|v| v.as_str()) {
        Some(label) => label,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: label".to_string()),
        },
    };

    let icon = match args.get("icon").and_then(|v| v.as_str()) {
        Some(icon) => icon,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: icon".to_string()),
        },
    };

    let comment = args.get("comment").and_then(|v| v.as_str());

    // Extract class name from IRI for generating unique instance IRI
    let class_name = class_iri.split(':').last().unwrap_or("Instance");
    let timestamp = chrono::Utc::now().timestamp_millis();
    let generated_iri = format!("foundation:{}_{}", class_name, timestamp);

    match (|| {
        let individual = Individual::new(&generated_iri);
        individual.assert(conn, class_iri, label, icon, "ai")?;

        // Add comment if provided
        if let Some(comment_text) = comment {
            use crate::eavto::{store, Triple, Object};
            let comment_triple = Triple::new(&generated_iri, "rdfs:comment", Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[comment_triple], "ai")?;
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": generated_iri,
            "message": format!("Instance {} created successfully with IRI: {}", label, generated_iri),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn add_property_value(conn: &mut Connection, args: &Value) -> FunctionResult {
    let instance_iri = match args.get("instance_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: instance_iri".to_string()),
        },
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_iri".to_string()),
        },
    };

    let value = match args.get("value").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: value".to_string()),
        },
    };

    let value_type = args.get("value_type").and_then(|v| v.as_str()).unwrap_or("literal");
    let datatype = args.get("datatype").and_then(|v| v.as_str()).unwrap_or("xsd:string");

    match (|| {
        use crate::eavto::Object;

        let object = if value_type == "iri" {
            Object::Iri(value.to_string())
        } else {
            Object::Literal {
                value: value.to_string(),
                datatype: Some(datatype.to_string()),
                language: None,
            }
        };

        let individual = Individual::new(instance_iri);
        individual.add_property(conn, property_iri, object, "ai")?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "message": format!("Property {} added to {}", property_iri, instance_iri),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn create_property(conn: &mut Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    let label = match args.get("label").and_then(|v| v.as_str()) {
        Some(label) => label,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: label".to_string()),
        },
    };

    let property_type_str = match args.get("property_type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_type".to_string()),
        },
    };

    let property_type = match property_type_str {
        "object" => crate::owl::PropertyType::ObjectProperty,
        "datatype" => crate::owl::PropertyType::DatatypeProperty,
        _ => return FunctionResult {
            success: false,
            result: None,
            error: Some("Invalid property_type. Must be 'object' or 'datatype'".to_string()),
        },
    };

    let comment = args.get("comment").and_then(|v| v.as_str());
    let domain = args.get("domain").and_then(|v| v.as_str());
    let range = args.get("range").and_then(|v| v.as_str());

    match (|| {
        use crate::owl::Property;

        let property = Property::new(iri);
        property.assert(conn, property_type, label, comment, domain, range, None, "ai")?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": iri,
            "message": format!("Property {} created successfully", label),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn search_properties(conn: &Connection, args: &Value) -> FunctionResult {
    let query_str = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;

    match (|| {
        use crate::owl::{Property, vocabulary::{rdf, owl}};

        // Get all object properties
        let obj_props_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::OBJECT_PROPERTY)?;
        // Get all datatype properties
        let data_props_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::DATATYPE_PROPERTY)?;

        let mut all_property_iris: Vec<String> = obj_props_result.triples.into_iter()
            .chain(data_props_result.triples)
            .map(|t| t.subject)
            .collect();

        all_property_iris.sort();
        all_property_iris.dedup();

        let mut properties = Vec::new();
        for iri in all_property_iris {
            if let Ok(property) = Property::get(conn, &iri) {
                // Filter by query if provided
                if !query_str.is_empty() {
                    if let Some(label) = &property.label {
                        if !label.to_lowercase().contains(&query_str.to_lowercase()) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                properties.push(serde_json::json!({
                    "iri": property.iri,
                    "label": property.label,
                    "type": format!("{:?}", property.property_type),
                    "domains": property.domains,
                    "ranges": property.ranges,
                }));

                if properties.len() >= limit {
                    break;
                }
            }
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "properties": properties,
            "count": properties.len(),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn get_property(conn: &Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::owl::Property;

        let property = Property::get(conn, iri)?;

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "iri": property.iri,
            "label": property.label,
            "comment": property.comment,
            "type": format!("{:?}", property.property_type),
            "domains": property.domains,
            "ranges": property.ranges,
            "superProperties": property.super_properties,
            "isFunctional": property.is_functional,
            "isTransitive": property.is_transitive,
            "isSymmetric": property.is_symmetric,
            "inverseOf": property.inverse_of,
            "unit": property.unit,
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn delete_class(conn: &mut Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple};

        // Get all triples where this IRI is the subject
        let triples_result = query::get_by_entity(conn, iri)?;

        // Retract all triples
        let triples_to_retract: Vec<Triple> = triples_result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();

        store::retract_triples(conn, &triples_to_retract, "ai")?;

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Class {} deleted successfully", iri),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn delete_instance(conn: &mut Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple};

        // Get all triples where this IRI is the subject
        let triples_result = query::get_by_entity(conn, iri)?;

        // Retract all triples
        let triples_to_retract: Vec<Triple> = triples_result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();

        store::retract_triples(conn, &triples_to_retract, "ai")?;

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Instance {} deleted successfully", iri),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn delete_property(conn: &mut Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple};

        // Get all triples where this IRI is the subject
        let triples_result = query::get_by_entity(conn, iri)?;

        // Retract all triples
        let triples_to_retract: Vec<Triple> = triples_result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();

        store::retract_triples(conn, &triples_to_retract, "ai")?;

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Property {} deleted successfully", iri),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn update_class(conn: &mut Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple, Object};
        use crate::owl::vocabulary::rdfs;

        let mut updated_fields = Vec::new();

        // Update label
        if let Some(label) = args.get("label").and_then(|v| v.as_str()) {
            // Retract old label
            let old_labels = query::get_by_entity_predicate(conn, iri, rdfs::LABEL)?;
            for triple in old_labels.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::LABEL, triple.object)], "ai")?;
            }
            // Assert new label
            let new_label = Triple::new(iri, rdfs::LABEL, Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_label], "ai")?;
            updated_fields.push("label");
        }

        // Update icon
        if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
            let old_icons = query::get_by_entity_predicate(conn, iri, "foundation:icon")?;
            for triple in old_icons.triples {
                store::retract_triples(conn, &[Triple::new(iri, "foundation:icon", triple.object)], "ai")?;
            }
            let new_icon = Triple::new(iri, "foundation:icon", Object::Literal {
                value: icon.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_icon], "ai")?;
            updated_fields.push("icon");
        }

        // Update comment
        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            let old_comments = query::get_by_entity_predicate(conn, iri, rdfs::COMMENT)?;
            for triple in old_comments.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::COMMENT, triple.object)], "ai")?;
            }
            let new_comment = Triple::new(iri, rdfs::COMMENT, Object::Literal {
                value: comment.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_comment], "ai")?;
            updated_fields.push("comment");
        }

        // Update super class
        if let Some(super_class) = args.get("super_class").and_then(|v| v.as_str()) {
            let old_supers = query::get_by_entity_predicate(conn, iri, rdfs::SUB_CLASS_OF)?;
            for triple in old_supers.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::SUB_CLASS_OF, triple.object)], "ai")?;
            }
            let new_super = Triple::new(iri, rdfs::SUB_CLASS_OF, Object::Iri(super_class.to_string()));
            store::assert_triples(conn, &[new_super], "ai")?;
            updated_fields.push("superClass");
        }

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Class {} updated successfully", iri),
            "updatedFields": updated_fields,
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn update_instance(conn: &mut Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple, Object};
        use crate::owl::vocabulary::rdfs;

        let mut updated_fields = Vec::new();

        // Update label
        if let Some(label) = args.get("label").and_then(|v| v.as_str()) {
            let old_labels = query::get_by_entity_predicate(conn, iri, rdfs::LABEL)?;
            for triple in old_labels.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::LABEL, triple.object)], "ai")?;
            }
            let new_label = Triple::new(iri, rdfs::LABEL, Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_label], "ai")?;
            updated_fields.push("label");
        }

        // Update icon
        if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
            let old_icons = query::get_by_entity_predicate(conn, iri, "foundation:icon")?;
            for triple in old_icons.triples {
                store::retract_triples(conn, &[Triple::new(iri, "foundation:icon", triple.object)], "ai")?;
            }
            let new_icon = Triple::new(iri, "foundation:icon", Object::Literal {
                value: icon.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_icon], "ai")?;
            updated_fields.push("icon");
        }

        // Update comment
        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            let old_comments = query::get_by_entity_predicate(conn, iri, rdfs::COMMENT)?;
            for triple in old_comments.triples {
                store::retract_triples(conn, &[Triple::new(iri, rdfs::COMMENT, triple.object)], "ai")?;
            }
            let new_comment = Triple::new(iri, rdfs::COMMENT, Object::Literal {
                value: comment.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[new_comment], "ai")?;
            updated_fields.push("comment");
        }

        Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
            "success": true,
            "message": format!("Instance {} updated successfully", iri),
            "updatedFields": updated_fields,
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn remove_property_value(conn: &mut Connection, args: &Value) -> FunctionResult {
    let instance_iri = match args.get("instance_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: instance_iri".to_string()),
        },
    };

    let property_iri = match args.get("property_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_iri".to_string()),
        },
    };

    let value = match args.get("value").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: value".to_string()),
        },
    };

    match (|| {
        use crate::eavto::{store, query, Triple, Object};

        // Find the triple to retract
        let triples_result = query::get_by_entity_predicate(conn, instance_iri, property_iri)?;

        for triple in triples_result.triples {
            let matches = match &triple.object {
                Object::Iri(iri) => iri == value,
                Object::Literal { value: v, .. } => v == value,
                _ => false,
            };

            if matches {
                store::retract_triples(conn, &[Triple::new(instance_iri, property_iri, triple.object)], "ai")?;
                return Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
                    "success": true,
                    "message": format!("Property value removed from {}", instance_iri),
                }));
            }
        }

        Ok(serde_json::json!({
            "success": false,
            "message": "Property value not found",
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn get_class_hierarchy(conn: &Connection, args: &Value) -> FunctionResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    match (|| {
        use crate::owl::vocabulary::rdfs;

        fn get_hierarchy_recursive(
            conn: &Connection,
            class_iri: &str,
            depth: usize,
            max_depth: usize,
            visited: &mut std::collections::HashSet<String>,
        ) -> Result<Value, crate::owl::OwlError> {
            if depth >= max_depth || visited.contains(class_iri) {
                return Ok(serde_json::json!({
                    "iri": class_iri,
                    "label": Class::get(conn, class_iri).ok().and_then(|c| c.label),
                    "subClasses": [],
                    "truncated": depth >= max_depth,
                }));
            }

            visited.insert(class_iri.to_string());

            let class = Class::get(conn, class_iri)?;
            let mut sub_classes = Vec::new();

            // Get direct subclasses
            let sub_result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, class_iri)?;
            for triple in sub_result.triples {
                let sub_hierarchy = get_hierarchy_recursive(conn, &triple.subject, depth + 1, max_depth, visited)?;
                sub_classes.push(sub_hierarchy);
            }

            Ok(serde_json::json!({
                "iri": class.iri,
                "label": class.label,
                "icon": class.icon,
                "subClasses": sub_classes,
                "truncated": false,
            }))
        }

        let mut visited = std::collections::HashSet::new();
        let hierarchy = get_hierarchy_recursive(conn, iri, 0, max_depth, &mut visited)?;

        Ok::<_, crate::owl::OwlError>(hierarchy)
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

pub fn find_instances_by_property(conn: &Connection, args: &Value) -> FunctionResult {
    let class_iri = match args.get("class_iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: class_iri".to_string()),
        },
    };

    let properties = match args.get("properties").and_then(|v| v.as_array()) {
        Some(props) => props,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: properties".to_string()),
        },
    };

    match (|| {
        // Convert properties array to tuple format
        let mut property_constraints = Vec::new();
        for prop in properties {
            let property_iri = prop.get("property")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::owl::OwlError::ValidationError("Missing 'property' field in constraint".to_string()))?;
            let value = prop.get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::owl::OwlError::ValidationError("Missing 'value' field in constraint".to_string()))?;

            property_constraints.push((property_iri, value));
        }

        // Use the OWL Individual method
        let instances = Individual::find_by_class_and_properties(conn, class_iri, &property_constraints)?;

        // Get full info for each instance
        let mut results = Vec::new();
        for iri in instances {
            if let Ok(individual) = Individual::get(conn, &iri) {
                results.push(serde_json::json!({
                    "iri": individual.iri,
                    "label": individual.label,
                    "icon": individual.icon,
                }));
            }
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "instances": results,
            "count": results.len(),
        }))
    })() {
        Ok(result) => FunctionResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}
