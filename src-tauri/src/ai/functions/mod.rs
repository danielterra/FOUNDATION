use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use turso::Connection;

mod batch;
mod blackboard;
mod concept;
mod concept_graph;
mod definitions;
mod meta_process;
mod property;
mod thing;

pub use definitions::{get_available_tools, get_claude_tools};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTemplate {
    pub name: String,
    pub description: String,
    pub parameters: Vec<Parameter>,
    #[serde(default)]
    pub array_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

impl ToolTemplate {
    pub fn to_claude_tool(&self) -> crate::ai::providers::ClaudeTool {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.parameters {
            let mut schema = if let Some(custom) = &param.schema {
                custom.clone()
            } else {
                json!({ "type": param.param_type })
            };
            if let Some(obj) = schema.as_object_mut() {
                obj.insert("description".to_string(), json!(param.description));
            }
            properties.insert(param.name.clone(), schema);
            if param.required {
                required.push(param.name.clone());
            }
        }

        let item_schema = json!({
            "type": "object",
            "properties": properties,
            "required": required,
        });

        let input_schema = if self.array_mode {
            json!({
                "type": "object",
                "properties": {
                    "operations": {
                        "type": "array",
                        "items": item_schema,
                        "minItems": 1,
                    }
                },
                "required": ["operations"],
            })
        } else {
            item_schema
        };

        crate::ai::providers::ClaudeTool {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema,
        }
    }

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concept: Option<Value>,
}

pub async fn execute_tool(
    conn: &Connection,
    call: &ToolCall,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let is_array_mode = get_available_tools()
        .iter()
        .any(|t| t.name == call.name && t.array_mode);
    let args = if is_array_mode {
        &call.arguments["operations"]
    } else {
        &call.arguments
    };

    match call.name.as_str() {
        "learn_concepts" => concept::learn_concept(conn, args, app).await,
        "learn_things" => thing::learn_thing(conn, args, app).await,
        "learn_properties" => property::learn_property(conn, args, app).await,
        "remember" => thing::remember(conn, args).await,
        "get_concepts" => concept::get_concepts(conn, args).await,
        "get_things" => thing::get_things(conn, args).await,
        "remember_properties" => property::remember_property(conn, args).await,
        "forget_concepts" => concept::delete_concept(conn, args, app).await,
        "forget_things" => thing::delete_thing(conn, args, app).await,
        "forget_properties" => property::forget_property(conn, args, app).await,
        "get_process" => meta_process::get_process(conn, args).await,
        "get_concept_graph" => concept_graph::get_concept_graph(conn, args).await,
        "blackboard_state" => blackboard::blackboard_state(conn).await,
        "blackboard_update" => blackboard::blackboard_update(conn, args, app).await,
        "run_process" => run_process_tool(args, app),
        _ => ToolResult {
            success: false,
            result: None,
            error: Some(format!("Unknown tool: {}", call.name)),
            concept: None,
        },
    }
}

fn run_process_tool(args: &Value, app: Option<&tauri::AppHandle>) -> ToolResult {
    let app = match app {
        Some(a) => a.clone(),
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("run_process requires a running Foundation app".to_string()),
            concept: None,
        },
    };

    let process_iri = match args["process_iri"].as_str() {
        Some(iri) if !iri.is_empty() => iri.to_string(),
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("process_iri is required".to_string()),
            concept: None,
        },
    };

    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::process_automation::executor::run_process(&app, &process_iri).await {
            crate::commands::log_backend(
                "error",
                &format!("[run_process] Error running {}: {}", process_iri, e),
            );
        }
    });

    ToolResult {
        success: true,
        result: None,
        error: None,
        concept: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Property, PropertyType};

    // ---- connections via learn_properties + learn_concepts ----

    #[tokio::test]
    async fn test_learn_concept_with_connections_creates_object_properties() {
        let conn = setup_test_db().await;

        let props_call = ToolCall {
            name: "learn_properties".to_string(),
            arguments: serde_json::json!({
                "operations": [
                    {"iri": "foundation:worksAt", "label": "works at", "property_type": "object"},
                    {"iri": "foundation:reportsTo", "label": "reports to", "property_type": "object", "range": "foundation:Employee"}
                ]
            }),
        };
        assert!(execute_tool(&conn, &props_call, None).await.success);

        let call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Employee",
                    "label": "Employee",
                    "icon": "https://example.com/icon.svg",
                    "super_concepts": ["owl:Thing"],
                    "upsert_details": ["foundation:worksAt", "foundation:reportsTo"]
                }]
            }),
        };
        let result = execute_tool(&conn, &call, None).await;
        assert!(result.success, "learn_concept with connections should succeed: {:?}", result.error);

        let works_at = Property::get(&conn, "foundation:worksAt").await.unwrap().unwrap();
        assert_eq!(works_at.property_type, PropertyType::ObjectProperty);
        assert!(works_at.domains.iter().any(|d| d == "foundation:Employee"));

        let reports_to = Property::get(&conn, "foundation:reportsTo").await.unwrap().unwrap();
        assert_eq!(reports_to.property_type, PropertyType::ObjectProperty);
        assert!(reports_to.ranges.iter().any(|r| r == "foundation:Employee"));
    }

    // ---- calculated fields via learn_properties + learn_concepts ----

    #[tokio::test]
    async fn test_learn_concept_with_calculated_fields_creates_properties() {
        let conn = setup_test_db().await;

        let props_call = ToolCall {
            name: "learn_properties".to_string(),
            arguments: serde_json::json!({
                "operations": [
                    {"iri": "foundation:width", "label": "width", "property_type": "datatype"},
                    {"iri": "foundation:height", "label": "height", "property_type": "datatype"},
                    {"iri": "foundation:area", "label": "area", "property_type": "datatype", "formula": "{{foundation:width}} * {{foundation:height}}"}
                ]
            }),
        };
        assert!(execute_tool(&conn, &props_call, None).await.success);

        let call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Rectangle",
                    "label": "Rectangle",
                    "icon": "https://example.com/icon.svg",
                    "super_concepts": ["owl:Thing"],
                    "upsert_details": ["foundation:width", "foundation:height", "foundation:area"]
                }]
            }),
        };
        let result = execute_tool(&conn, &call, None).await;
        assert!(result.success, "learn_concept with calculated fields should succeed: {:?}", result.error);

        let width = Property::get(&conn, "foundation:width").await.unwrap().unwrap();
        assert_eq!(width.property_type, PropertyType::DatatypeProperty);

        let area = Property::get(&conn, "foundation:area").await.unwrap().unwrap();
        assert_eq!(area.property_type, PropertyType::DatatypeProperty);

        let stored = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:area", "foundation:formula"
        ).await.unwrap();
        assert!(!stored.triples.is_empty(), "Formula triple should be stored");
        assert_eq!(
            stored.triples[0].object.as_literal().unwrap(),
            "{{foundation:width}} * {{foundation:height}}"
        );
    }

    #[tokio::test]
    async fn test_learn_concept_property_domain_set_to_concept() {
        let conn = setup_test_db().await;

        let props_call = ToolCall {
            name: "learn_properties".to_string(),
            arguments: serde_json::json!({
                "operations": [{"iri": "foundation:boxSize", "label": "size", "property_type": "datatype"}]
            }),
        };
        assert!(execute_tool(&conn, &props_call, None).await.success);

        let call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Box",
                    "label": "Box",
                    "icon": "https://example.com/icon.svg",
                    "super_concepts": ["owl:Thing"],
                    "upsert_details": ["foundation:boxSize"]
                }]
            }),
        };
        execute_tool(&conn, &call, None).await;

        let prop = Property::get(&conn, "foundation:boxSize").await.unwrap().unwrap();
        assert!(
            prop.domains.iter().any(|d| d == "foundation:Box"),
            "Domain should be set to concept IRI, got: {:?}",
            prop.domains
        );
    }

    #[tokio::test]
    async fn test_learn_property_circular_formula_is_rejected() {
        let conn = setup_test_db().await;

        let call = ToolCall {
            name: "learn_properties".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:selfRef",
                    "label": "self ref",
                    "property_type": "datatype",
                    "formula": "{{foundation:selfRef}} + 1"
                }]
            }),
        };
        let result = execute_tool(&conn, &call, None).await;
        assert!(!result.success, "Circular formula should be rejected");
        let err = result.error.unwrap();
        assert!(err.contains("Circular"), "Expected circular dependency error, got: {err}");
    }

    // ---- learn_concept upsert: adding properties to existing concept ----

    #[tokio::test]
    async fn test_update_concept_adds_calculated_fields() {
        let conn = setup_test_db().await;

        let radius_call = ToolCall {
            name: "learn_properties".to_string(),
            arguments: serde_json::json!({
                "operations": [{"iri": "foundation:radius", "label": "radius", "property_type": "datatype"}]
            }),
        };
        assert!(execute_tool(&conn, &radius_call, None).await.success);

        let create_call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Circle",
                    "label": "Circle",
                    "icon": "https://example.com/icon.svg",
                    "super_concepts": ["owl:Thing"],
                    "upsert_details": ["foundation:radius"]
                }]
            }),
        };
        execute_tool(&conn, &create_call, None).await;

        let circ_call = ToolCall {
            name: "learn_properties".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:circumference",
                    "label": "circumference",
                    "property_type": "datatype",
                    "formula": "{{foundation:radius}} * 6"
                }]
            }),
        };
        assert!(execute_tool(&conn, &circ_call, None).await.success);

        let update_call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Circle",
                    "upsert_details": ["foundation:circumference"]
                }]
            }),
        };
        let result = execute_tool(&conn, &update_call, None).await;
        assert!(result.success, "updating concept with new property should succeed: {:?}", result.error);

        let prop = Property::get(&conn, "foundation:circumference").await.unwrap().unwrap();
        assert_eq!(prop.property_type, PropertyType::DatatypeProperty);

        let stored = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:circumference", "foundation:formula"
        ).await.unwrap();
        assert!(!stored.triples.is_empty(), "Formula triple should be stored");
    }

}
