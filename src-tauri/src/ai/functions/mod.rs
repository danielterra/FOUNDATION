use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use rusqlite::Connection;

mod blackboard;
mod class;
mod definitions;
mod instance;
mod property;

pub use definitions::{get_available_functions, get_claude_tools};

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

    #[allow(dead_code)]
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

/// Execute a function call
pub fn execute_function(conn: &mut Connection, call: &FunctionCall, app: Option<&tauri::AppHandle>) -> FunctionResult {
    match call.name.as_str() {
        "learn_concept" => class::create_class(conn, &call.arguments, app),
        "learn_thing" => instance::create_instance(conn, &call.arguments, app),
        "learn_thing_detail" => property::add_property_value(conn, &call.arguments, app),
        "learn_connection_type" => property::create_property(conn, &call.arguments),
        "remember_concept" => class::get_class(conn, &call.arguments),
        "remember_thing" => instance::get_instance(conn, &call.arguments),
        "remember_concepts" => class::search_classes(conn, &call.arguments),
        "remember_things" => instance::search_instances(conn, &call.arguments),
        "remember_connection_type" => property::get_property(conn, &call.arguments),
        "remember_things_by_details" => instance::find_instances_by_property(conn, &call.arguments),
        "forget_concept" => class::delete_class(conn, &call.arguments, app),
        "forget_thing" => instance::delete_instance(conn, &call.arguments, app),
        "forget_connection_type" => property::delete_property(conn, &call.arguments, app),
        "forget_thing_detail" => property::remove_property_value(conn, &call.arguments, app),
        "update_concept" => class::update_class(conn, &call.arguments, app),
        "update_thing" => instance::update_instance(conn, &call.arguments, app),
        "blackboard_show" => blackboard::blackboard_show(conn),
        "blackboard_add_widget" => blackboard::blackboard_add_widget(conn, &call.arguments, app),
        "blackboard_remove" => blackboard::blackboard_remove(conn, &call.arguments, app),
        "blackboard_clear" => blackboard::blackboard_clear(conn, app),
        _ => FunctionResult {
            success: false,
            result: None,
            error: Some(format!("Unknown function: {}", call.name)),
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
    fn test_get_class_with_one_of_returns_allowed_values() {
        let mut conn = setup_test_db();

        // Create TaskPriority enumeration class
        let priority_class = Class::new("foundation:TaskPriority");
        priority_class.assert(&mut conn, ClassType::OwlClass, "Task Priority", "priority-icon", None, "test").unwrap();

        // Create enumerated individuals with labels
        let high = Triple::new("foundation:HighPriority", rdf::TYPE, Object::Iri("foundation:TaskPriority".to_string()));
        let high_label = Triple::new("foundation:HighPriority", "rdfs:label", Object::Literal {
            value: "High Priority".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
        let medium = Triple::new("foundation:MediumPriority", rdf::TYPE, Object::Iri("foundation:TaskPriority".to_string()));
        let medium_label = Triple::new("foundation:MediumPriority", "rdfs:label", Object::Literal {
            value: "Medium Priority".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
        store::assert_triples(&mut conn, &[high, high_label, medium, medium_label], "test").unwrap();

        // Create RDF list
        let list2 = Triple::new("_:list2", rdf::FIRST, Object::Iri("foundation:MediumPriority".to_string()));
        let list2_rest = Triple::new("_:list2", rdf::REST, Object::Iri(rdf::NIL.to_string()));
        let list1 = Triple::new("_:list1", rdf::FIRST, Object::Iri("foundation:HighPriority".to_string()));
        let list1_rest = Triple::new("_:list1", rdf::REST, Object::Iri("_:list2".to_string()));
        store::assert_triples(&mut conn, &[list1, list1_rest, list2, list2_rest], "test").unwrap();

        // Add owl:oneOf
        let one_of = Triple::new("foundation:TaskPriority", owl::ONE_OF, Object::Iri("_:list1".to_string()));
        store::assert_triples(&mut conn, &[one_of], "test").unwrap();

        // Call get_class
        let args = serde_json::json!({"iri": "foundation:TaskPriority"});
        let result = class::get_class(&conn, &args);

        // Verify result
        assert!(result.success, "get_class should succeed");
        let response = result.result.unwrap();
        assert!(response["allowedValues"].is_array(), "Should have allowedValues array");
        let allowed = response["allowedValues"].as_array().unwrap();
        assert_eq!(allowed.len(), 2, "Should have 2 allowed values");

        // Check that labels are included
        let high_value = allowed.iter().find(|v| v["iri"] == "foundation:HighPriority");
        assert!(high_value.is_some());
        assert_eq!(high_value.unwrap()["label"], "High Priority");
    }

    #[test]
    fn test_get_property_with_one_of_range_returns_allowed_values() {
        let mut conn = setup_test_db();

        // Create Task class
        let task_class = Class::new("foundation:Task");
        task_class.assert(&mut conn, ClassType::OwlClass, "Task", "task-icon", None, "test").unwrap();

        // Create TaskPriority enumeration class with owl:oneOf
        let priority_class = Class::new("foundation:TaskPriority");
        priority_class.assert(&mut conn, ClassType::OwlClass, "Task Priority", "priority-icon", None, "test").unwrap();

        // Create enumerated individuals
        let high = Triple::new("foundation:HighPriority", rdf::TYPE, Object::Iri("foundation:TaskPriority".to_string()));
        let high_label = Triple::new("foundation:HighPriority", "rdfs:label", Object::Literal {
            value: "High".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
        store::assert_triples(&mut conn, &[high, high_label], "test").unwrap();

        // Create RDF list
        let list1 = Triple::new("_:list1", rdf::FIRST, Object::Iri("foundation:HighPriority".to_string()));
        let list1_rest = Triple::new("_:list1", rdf::REST, Object::Iri(rdf::NIL.to_string()));
        store::assert_triples(&mut conn, &[list1, list1_rest], "test").unwrap();

        // Add owl:oneOf
        let one_of = Triple::new("foundation:TaskPriority", owl::ONE_OF, Object::Iri("_:list1".to_string()));
        store::assert_triples(&mut conn, &[one_of], "test").unwrap();

        // Create priority property
        let priority_prop = Property::new("foundation:priority");
        priority_prop.assert(&mut conn, PropertyType::ObjectProperty, "priority", None, Some("foundation:Task"), Some("foundation:TaskPriority"), None, "test").unwrap();

        // Call get_property
        let args = serde_json::json!({"iri": "foundation:priority"});
        let result = property::get_property(&conn, &args);

        // Verify result
        assert!(result.success, "get_property should succeed");
        let response = result.result.unwrap();
        assert!(response["allowedValues"].is_array(), "Should have allowedValues array");
        let allowed = response["allowedValues"].as_array().unwrap();
        assert_eq!(allowed.len(), 1, "Should have 1 allowed value");
        assert_eq!(allowed[0]["iri"], "foundation:HighPriority");
        assert_eq!(allowed[0]["label"], "High");
    }
}
