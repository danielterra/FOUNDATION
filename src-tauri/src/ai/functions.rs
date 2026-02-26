use serde::{Deserialize, Serialize};
use serde_json::Value;
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

/// Get system prompt with function definitions for the AI
pub fn get_system_prompt_with_functions() -> String {
    let functions = get_available_functions();

    let mut prompt = String::from("Available functions:\n");

    for func in &functions {
        prompt.push_str(&format!("- {}: {}\n", func.name, func.description));
    }

    prompt.push_str(
        "\nTo call a function, use this EXACT format:\n\
        <json>{\"function\": \"name\", \"arguments\": {...}}</json>\n\
        Otherwise respond in plain text.\n\n"
    );

    prompt
}

/// Get all available functions for the AI
pub fn get_available_functions() -> Vec<FunctionDefinition> {
    vec![
        FunctionDefinition {
            name: "search_classes".to_string(),
            description: "Search for OWL/RDFS classes in the knowledge base. Returns paginated results.".to_string(),
            parameters: vec![
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search query to match against class labels (case-insensitive, partial match)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "Maximum number of results to return (default: 100)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "number".to_string(),
                    description: "Number of results to skip for pagination (default: 0)".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "get_class".to_string(),
            description: "Get complete information about a specific class by IRI, including label, icon, comment, super classes, sub classes, properties, and instances.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "The IRI of the class (e.g., 'foundation:Person')".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "search_instances".to_string(),
            description: "Search for instances (individuals) of a specific class. Returns paginated results.".to_string(),
            parameters: vec![
                Parameter {
                    name: "class_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "The IRI of the class to search instances for".to_string(),
                    required: true,
                },
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search query to match against instance labels (case-insensitive, partial match)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "Maximum number of results to return (default: 100)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "number".to_string(),
                    description: "Number of results to skip for pagination (default: 0)".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "get_instance".to_string(),
            description: "Get complete information about a specific instance by IRI, including label, icon, comment, types, properties, and backlinks.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "The IRI of the instance (e.g., 'foundation:John')".to_string(),
                    required: true,
                },
            ],
        },
    ]
}

/// Execute a function call
pub fn execute_function(conn: &Connection, call: &FunctionCall) -> FunctionResult {
    match call.name.as_str() {
        "search_classes" => search_classes(conn, &call.arguments),
        "get_class" => get_class(conn, &call.arguments),
        "search_instances" => search_instances(conn, &call.arguments),
        "get_instance" => get_instance(conn, &call.arguments),
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
