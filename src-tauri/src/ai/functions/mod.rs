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
        "learn_concept" => concept::create_concept(conn, args, app),
        "learn_thing" => thing::create_thing(conn, args, app),
        "learn_thing_detail" => detail::learn_detail_value(conn, args, app),
        "learn_connection_type" => detail::create_detail(conn, args, app),
        "remember_concept" => concept::get_concept(conn, args),
        "remember_thing" => thing::get_thing(conn, args),
        "remember_concepts" => concept::search_concepts(conn, args),
        "remember_things" => thing::search_things(conn, args),
        "remember_connection_type" => detail::get_detail(conn, args),
        "remember_things_by_details" => thing::find_things_by_detail(conn, args),
        "forget_concept" => concept::delete_concept(conn, args, app),
        "forget_thing" => thing::delete_thing(conn, args, app),
        "forget_connection_type" => detail::delete_detail(conn, args, app),
        "forget_thing_detail" => detail::forget_detail_value(conn, args, app),
        "update_concept" => concept::update_concept(conn, args, app),
        "update_thing" => thing::update_thing(conn, args, app),
        "blackboard_show" => blackboard::blackboard_show(conn),
        "blackboard_add_widget" => blackboard::blackboard_add_widget(conn, args, app),
        "blackboard_remove" => blackboard::blackboard_remove(conn, args, app),
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
    use crate::owl::{Class, ClassType, Property, PropertyType, vocabulary::{rdf, owl}};
    use crate::eavto::{store, Triple, Object};

    #[test]
    fn test_get_concept_with_one_of_returns_allowed_values() {
        let mut conn = setup_test_db();

        let priority_class = Class::new("foundation:TaskPriority");
        priority_class.assert(
            &mut conn, ClassType::OwlClass, "Task Priority", "https://example.com/priority.svg", None, "test",
        ).unwrap();

        let high = Triple::new(
            "foundation:HighPriority", rdf::TYPE,
            Object::Iri("foundation:TaskPriority".to_string()),
        );
        let high_label = Triple::new("foundation:HighPriority", "rdfs:label", Object::Literal {
            value: "High Priority".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
        let medium = Triple::new(
            "foundation:MediumPriority", rdf::TYPE,
            Object::Iri("foundation:TaskPriority".to_string()),
        );
        let medium_label = Triple::new("foundation:MediumPriority", "rdfs:label", Object::Literal {
            value: "Medium Priority".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
        store::assert_triples(
            &mut conn, &[high, high_label, medium, medium_label], "test",
        ).unwrap();

        let list2 = Triple::new(
            "_:list2", rdf::FIRST, Object::Iri("foundation:MediumPriority".to_string()),
        );
        let list2_rest = Triple::new("_:list2", rdf::REST, Object::Iri(rdf::NIL.to_string()));
        let list1 = Triple::new(
            "_:list1", rdf::FIRST, Object::Iri("foundation:HighPriority".to_string()),
        );
        let list1_rest = Triple::new("_:list1", rdf::REST, Object::Iri("_:list2".to_string()));
        store::assert_triples(&mut conn, &[list1, list1_rest, list2, list2_rest], "test").unwrap();

        let one_of = Triple::new(
            "foundation:TaskPriority", owl::ONE_OF, Object::Iri("_:list1".to_string()),
        );
        store::assert_triples(&mut conn, &[one_of], "test").unwrap();

        let args = serde_json::json!([{"iri": "foundation:TaskPriority"}]);
        let result = concept::get_concept(&conn, &args);

        assert!(result.success, "get_concept should succeed");
        let response = result.result.unwrap();
        let concept_result = &response["results"][0];
        assert!(concept_result["allowedValues"].is_array(), "Should have allowedValues array");
        let allowed = concept_result["allowedValues"].as_array().unwrap();
        assert_eq!(allowed.len(), 2, "Should have 2 allowed values");

        let high_value = allowed.iter().find(|v| v["iri"] == "foundation:HighPriority");
        assert!(high_value.is_some());
        assert_eq!(high_value.unwrap()["label"], "High Priority");
    }

    #[test]
    fn test_get_detail_with_one_of_range_returns_allowed_values() {
        let mut conn = setup_test_db();

        let task_class = Class::new("foundation:Task");
        task_class.assert(
            &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test",
        ).unwrap();

        let priority_class = Class::new("foundation:TaskPriority");
        priority_class.assert(
            &mut conn, ClassType::OwlClass, "Task Priority", "https://example.com/priority.svg", None, "test",
        ).unwrap();

        let high = Triple::new(
            "foundation:HighPriority", rdf::TYPE,
            Object::Iri("foundation:TaskPriority".to_string()),
        );
        let high_label = Triple::new("foundation:HighPriority", "rdfs:label", Object::Literal {
            value: "High".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
        store::assert_triples(&mut conn, &[high, high_label], "test").unwrap();

        let list1 = Triple::new(
            "_:list1", rdf::FIRST, Object::Iri("foundation:HighPriority".to_string()),
        );
        let list1_rest = Triple::new("_:list1", rdf::REST, Object::Iri(rdf::NIL.to_string()));
        store::assert_triples(&mut conn, &[list1, list1_rest], "test").unwrap();

        let one_of = Triple::new(
            "foundation:TaskPriority", owl::ONE_OF, Object::Iri("_:list1".to_string()),
        );
        store::assert_triples(&mut conn, &[one_of], "test").unwrap();

        let priority_prop = Property::new("foundation:priority");
        priority_prop.assert(
            &mut conn,
            PropertyType::ObjectProperty,
            "priority",
            None,
            &["foundation:Task"],
            Some("foundation:TaskPriority"),
            None,
            "test",
        ).unwrap();

        let args = serde_json::json!([{"iri": "foundation:priority"}]);
        let result = detail::get_detail(&conn, &args);

        assert!(result.success, "get_detail should succeed");
        let response = result.result.unwrap();
        let detail_result = &response["results"][0];
        assert!(detail_result["allowedValues"].is_array(), "Should have allowedValues array");
        let allowed = detail_result["allowedValues"].as_array().unwrap();
        assert_eq!(allowed.len(), 1, "Should have 1 allowed value");
        assert_eq!(allowed[0]["iri"], "foundation:HighPriority");
        assert_eq!(allowed[0]["label"], "High");
    }
}
