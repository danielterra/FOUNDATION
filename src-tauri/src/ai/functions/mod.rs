use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use rusqlite::Connection;

mod batch;
mod blackboard;
mod concept;
mod definitions;
mod thing;
mod detail;

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
}

pub fn execute_tool(
    conn: &mut Connection,
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
        "learn_concepts" => concept::learn_concept(conn, args, app),
        "learn_things" => thing::learn_thing(conn, args, app),
        "remember_concepts" => concept::remember_concept(conn, args),
        "remember_things" => thing::remember_thing(conn, args),
        "forget_concepts" => concept::delete_concept(conn, args, app),
        "forget_things" => thing::delete_thing(conn, args, app),
        "blackboard_update" => blackboard::blackboard_update(conn, args, app),
        _ => ToolResult {
            success: false,
            result: None,
            error: Some(format!("Unknown tool: {}", call.name)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Property, PropertyType};

    // ---- connections via learn_concept ----

    #[test]
    fn test_learn_concept_with_connections_creates_object_properties() {
        let mut conn = setup_test_db();
        let call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Employee",
                    "label": "Employee",
                    "icon": "person",
                    "connections": [
                        {"iri": "foundation:worksAt", "label": "works at"},
                        {"iri": "foundation:reportsTo", "label": "reports to", "range": "foundation:Employee"}
                    ]
                }]
            }),
        };
        let result = execute_tool(&mut conn, &call, None);
        assert!(result.success, "learn_concept with connections should succeed: {:?}", result.error);

        let works_at = Property::get(&conn, "foundation:worksAt").unwrap().unwrap();
        assert_eq!(works_at.property_type, PropertyType::ObjectProperty);
        assert!(works_at.domains.iter().any(|d| d == "foundation:Employee"));

        let reports_to = Property::get(&conn, "foundation:reportsTo").unwrap().unwrap();
        assert_eq!(reports_to.property_type, PropertyType::ObjectProperty);
        assert!(reports_to.ranges.iter().any(|r| r == "foundation:Employee"));
    }

    // ---- calculated_fields via learn_concept ----

    #[test]
    fn test_learn_concept_with_calculated_fields_creates_properties() {
        let mut conn = setup_test_db();

        let call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Rectangle",
                    "label": "Rectangle",
                    "icon": "rectangle",
                    "calculated_fields": [
                        {"iri": "foundation:width", "label": "width"},
                        {"iri": "foundation:height", "label": "height"},
                        {
                            "iri": "foundation:area",
                            "label": "area",
                            "formula": "{{foundation:width}} * {{foundation:height}}"
                        }
                    ]
                }]
            }),
        };
        let result = execute_tool(&mut conn, &call, None);
        assert!(result.success, "learn_concept with calculated_fields should succeed: {:?}", result.error);

        let width = Property::get(&conn, "foundation:width").unwrap().unwrap();
        assert_eq!(width.property_type, PropertyType::DatatypeProperty);

        let area = Property::get(&conn, "foundation:area").unwrap().unwrap();
        assert_eq!(area.property_type, PropertyType::DatatypeProperty);

        let stored = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:area", "foundation:formula"
        ).unwrap();
        assert!(!stored.triples.is_empty(), "Formula triple should be stored");
        assert_eq!(
            stored.triples[0].object.as_literal().unwrap(),
            "{{foundation:width}} * {{foundation:height}}"
        );
    }

    #[test]
    fn test_learn_concept_calculated_field_domain_defaults_to_concept() {
        let mut conn = setup_test_db();

        let call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Box",
                    "label": "Box",
                    "icon": "box",
                    "calculated_fields": [
                        {"iri": "foundation:boxSize", "label": "size"}
                    ]
                }]
            }),
        };
        execute_tool(&mut conn, &call, None);

        let prop = Property::get(&conn, "foundation:boxSize").unwrap().unwrap();
        assert!(
            prop.domains.iter().any(|d| d == "foundation:Box"),
            "Domain should default to concept IRI, got: {:?}",
            prop.domains
        );
    }

    #[test]
    fn test_learn_concept_calculated_field_circular_formula_is_rejected() {
        let mut conn = setup_test_db();

        let call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Thing",
                    "label": "Thing",
                    "icon": "thing",
                    "calculated_fields": [
                        {
                            "iri": "foundation:selfRef",
                            "label": "self ref",
                            "formula": "{{foundation:selfRef}} + 1"
                        }
                    ]
                }]
            }),
        };
        let result = execute_tool(&mut conn, &call, None);
        assert!(!result.success, "Circular formula should be rejected");
        let err = result.error.unwrap();
        assert!(err.contains("Circular"), "Expected circular dependency error, got: {err}");
    }

    // ---- learn_concept upsert: adding calculated_fields to existing concept ----

    #[test]
    fn test_update_concept_adds_calculated_fields() {
        let mut conn = setup_test_db();

        let create_call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Circle",
                    "label": "Circle",
                    "icon": "circle",
                    "calculated_fields": [
                        {"iri": "foundation:radius", "label": "radius"}
                    ]
                }]
            }),
        };
        execute_tool(&mut conn, &create_call, None);

        let update_call = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Circle",
                    "calculated_fields": [
                        {
                            "iri": "foundation:circumference",
                            "label": "circumference",
                            "formula": "{{foundation:radius}} * 6"
                        }
                    ]
                }]
            }),
        };
        let result = execute_tool(&mut conn, &update_call, None);
        assert!(result.success, "learn_concept upsert with calculated_fields should succeed: {:?}", result.error);

        let prop = Property::get(&conn, "foundation:circumference").unwrap().unwrap();
        assert_eq!(prop.property_type, PropertyType::DatatypeProperty);

        let stored = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:circumference", "foundation:formula"
        ).unwrap();
        assert!(!stored.triples.is_empty(), "Formula triple should be stored");
    }

}
