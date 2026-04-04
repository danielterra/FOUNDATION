use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use rusqlite::Connection;

mod batch;
mod class;
mod class_graph;
mod definitions;
mod file;
mod property;
mod individual;

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

pub fn find_automations_for_types(conn: &Connection, type_iris: &[String]) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for type_iri in type_iris {
        let mut stmt = match conn.prepare(
            "SELECT DISTINCT t.subject FROM triples t
             WHERE t.predicate = 'foundation:inputClass'
               AND t.object = ?1
               AND t.retracted = 0"
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let iris: Vec<String> = stmt
            .query_map([type_iri], |row| row.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        for iri in iris {
            if seen.contains(&iri) { continue; }
            let rdf_type = crate::owl::get_iri_property(conn, &iri, "rdf:type")
                .ok()
                .flatten();
            if rdf_type.as_deref() != Some("foundation:Automation") { continue; }
            let label = crate::owl::get_literal_property(conn, &iri, "rdfs:label")
                .ok()
                .flatten()
                .unwrap_or_else(|| iri.clone());
            seen.insert(iri.clone());
            results.push(serde_json::json!({"iri": iri, "label": label}));
        }
    }
    results
}

pub fn execute_tool(
    conn: &mut Connection,
    call: &ToolCall,
    app: Option<&tauri::AppHandle>,
    _conversation_id: Option<&str>,
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
        "define_class" => class::define_class(conn, args, app),
        "define_property" => property::define_property(conn, args, app),
        "assert_individual" => individual::assert_individual(conn, args, app),
        "add_property_values" => individual::add_property_values(conn, args, app),
        "replace_property_values" => individual::replace_property_values(conn, args, app),
        "remove_property_values" => individual::remove_property_values(conn, args, app),
        "clear_property" => individual::clear_property(conn, args, app),
        "retract_individual" => individual::retract_individual(conn, args, app),
        "retract_class" => class::retract_class(conn, args, app),
        "retract_property" => property::retract_property(conn, args, app),
        "search" => individual::search(conn, args),
        "describe_class" => class::describe_class(conn, args),
        "describe_individual" => individual::describe_individual(conn, args),
        "describe_property" => property::describe_property(conn, args),
        "class_graph" => class_graph::get_class_graph(conn, args),
        "head_file" => file::head_file(conn, args),
        "read_lines" => file::read_lines(conn, args),
        "get_automation" => get_automation_tool(conn, args),
        "run_automation" => run_automation_tool(conn, args, app),
        _ => ToolResult {
            success: false,
            result: None,
            error: Some(format!("Unknown tool: {}", call.name)),
            concept: None,
        },
    }
}

fn get_automation_tool(conn: &rusqlite::Connection, args: &Value) -> ToolResult {
    let automation_iri = match args["automation_iri"].as_str() {
        Some(iri) if !iri.is_empty() => iri,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("automation_iri is required".to_string()),
            concept: None,
        },
    };

    let graph = match crate::commands::automation::build_automation_graph(conn, automation_iri) {
        Ok(g) => g,
        Err(e) => return ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
    };

    let mut lines = vec![
        format!("# Automation: {}", graph.process_label),
        format!("IRI: {}", automation_iri),
        String::new(),
        "## Flow Nodes".to_string(),
    ];

    for node in &graph.nodes {
        lines.push(format!("- [{}] {} ({})", node.node_type, node.label, node.id));
        if let Some(ref agent) = node.assigned_agent {
            lines.push(format!("  assigned_agent: {}", agent));
        }
        for role in &node.assigned_to_roles {
            lines.push(format!("  assigned_to_role: {}", role));
        }
        for user in &node.assigned_to_users {
            lines.push(format!("  assigned_to_user: {}", user));
        }
        if !node.uses_tools.is_empty() {
            lines.push(format!("  uses_tools: {}", node.uses_tools.join(", ")));
        }
        if let Some(ref lbl) = node.input_class_label {
            lines.push(format!("  input: {}", lbl));
        }
        if let Some(ref lbl) = node.output_class_label {
            lines.push(format!("  output: {}", lbl));
        }
        for c in &node.output_classes {
            lines.push(format!("  output: {}", c.label));
        }
        if let Some(ref proc_iri) = node.invokes_process {
            lines.push(format!("  calls: {}", proc_iri));
        }
    }

    lines.push(String::new());
    lines.push("## Sequence Flows".to_string());

    for edge in &graph.edges {
        let condition = edge.condition_expression.as_deref()
            .map(|c| format!(" [if: {}]", c))
            .unwrap_or_default();
        lines.push(format!("- {} → {}{}", edge.source, edge.target, condition));
    }

    ToolResult {
        success: true,
        result: Some(Value::String(lines.join("\n"))),
        error: None,
        concept: None,
    }
}

fn run_automation_tool(conn: &Connection, args: &Value, app: Option<&tauri::AppHandle>) -> ToolResult {
    use crate::owl::{find_entities_with_property, get_iri_property, get_literal_property, Individual};

    let app = match app {
        Some(a) => a.clone(),
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("run_automation requires a running Foundation app".to_string()),
            concept: None,
        },
    };

    let automation_iri = match args["automation_iri"].as_str() {
        Some(iri) if !iri.is_empty() => iri.to_string(),
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("automation_iri is required".to_string()),
            concept: None,
        },
    };

    let input_iri = args["input_iri"].as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let input_class_iri: Option<String> = (|| -> Option<String> {
        let node_iris = find_entities_with_property(conn, "foundation:partOfProcess", &automation_iri)
            .ok()?;

        for node_iri in &node_iris {
            let node_type = get_iri_property(conn, node_iri, "rdf:type").ok()??;
            if node_type.ends_with("automation_StartEvent") {
                if let Some(class_iri) = get_iri_property(conn, node_iri, "foundation:inputClass").ok()? {
                    return Some(class_iri);
                }
            }
        }
        None
    })();

    if let Some(ref class_iri) = input_class_iri {
        let class_label = get_literal_property(conn, class_iri, "rdfs:label")
            .unwrap_or_default()
            .unwrap_or_else(|| class_iri.clone());

        let provided_iri = match &input_iri {
            Some(iri) => iri.clone(),
            None => return ToolResult {
                success: false,
                result: None,
                error: Some(format!(
                    "This automation requires an input of type '{}' but no input_iri was provided.",
                    class_label
                )),
                concept: None,
            },
        };

        let individual = match Individual::get(conn, &provided_iri) {
            Ok(Some(ind)) => ind,
            Ok(None) => return ToolResult {
                success: false,
                result: None,
                error: Some(format!("Input individual '{}' not found in the ontology.", provided_iri)),
                concept: None,
            },
            Err(e) => return ToolResult {
                success: false,
                result: None,
                error: Some(format!("Failed to look up input individual '{}': {}", provided_iri, e)),
                concept: None,
            },
        };

        let actual_type = individual.properties.iter()
            .find(|(k, _)| k == "rdf:type")
            .map(|(_, v)| match v {
                crate::owl::Object::Iri(iri) => iri.clone(),
                _ => String::new(),
            })
            .unwrap_or_default();

        if actual_type != *class_iri {
            let actual_label = if actual_type.is_empty() {
                "unknown".to_string()
            } else {
                get_literal_property(conn, &actual_type, "rdfs:label")
                    .unwrap_or_default()
                    .unwrap_or_else(|| actual_type.clone())
            };
            return ToolResult {
                success: false,
                result: None,
                error: Some(format!(
                    "Type mismatch: automation expects input of type '{}' but '{}' is of type '{}'.",
                    class_label, provided_iri, actual_label
                )),
                concept: None,
            };
        }
    }

    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::process_automation::executor::run_process(&app, &automation_iri, input_iri).await {
            crate::commands::log_backend(
                "error",
                &format!("[run_automation] Error running {}: {}", automation_iri, e),
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

    // ---- connections via define_property + define_class ----

    #[test]
    fn test_learn_concept_with_connections_creates_object_properties() {
        let mut conn = setup_test_db();

        let props_call = ToolCall {
            name: "define_property".to_string(),
            arguments: serde_json::json!({
                "operations": [
                    {"iri": "foundation:worksAt", "label": "works at", "property_type": "object"},
                    {"iri": "foundation:reportsTo", "label": "reports to", "property_type": "object", "range": "foundation:Employee"}
                ]
            }),
        };
        assert!(execute_tool(&mut conn, &props_call, None, None).success);

        let call = ToolCall {
            name: "define_class".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Employee",
                    "label": "Employee",
                    "icon": "https://example.com/icon.svg",
                    "super_classes": ["owl:Thing"],
                    "add_properties": ["foundation:worksAt", "foundation:reportsTo"]
                }]
            }),
        };
        let result = execute_tool(&mut conn, &call, None, None);
        assert!(result.success, "define_class with connections should succeed: {:?}", result.error);

        let works_at = Property::get(&conn, "foundation:worksAt").unwrap().unwrap();
        assert_eq!(works_at.property_type, PropertyType::ObjectProperty);
        assert!(works_at.domains.iter().any(|d| d == "foundation:Employee"));

        let reports_to = Property::get(&conn, "foundation:reportsTo").unwrap().unwrap();
        assert_eq!(reports_to.property_type, PropertyType::ObjectProperty);
        assert!(reports_to.ranges.iter().any(|r| r == "foundation:Employee"));
    }

    // ---- calculated fields via define_property + define_class ----

    #[test]
    fn test_learn_concept_with_calculated_fields_creates_properties() {
        let mut conn = setup_test_db();

        let props_call = ToolCall {
            name: "define_property".to_string(),
            arguments: serde_json::json!({
                "operations": [
                    {"iri": "foundation:width", "label": "width", "property_type": "datatype"},
                    {"iri": "foundation:height", "label": "height", "property_type": "datatype"},
                    {"iri": "foundation:area", "label": "area", "property_type": "datatype", "formula": "{{foundation:width}} * {{foundation:height}}"}
                ]
            }),
        };
        assert!(execute_tool(&mut conn, &props_call, None, None).success);

        let call = ToolCall {
            name: "define_class".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Rectangle",
                    "label": "Rectangle",
                    "icon": "https://example.com/icon.svg",
                    "super_classes": ["owl:Thing"],
                    "add_properties": ["foundation:width", "foundation:height", "foundation:area"]
                }]
            }),
        };
        let result = execute_tool(&mut conn, &call, None, None);
        assert!(result.success, "define_class with calculated fields should succeed: {:?}", result.error);

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
    fn test_learn_concept_property_domain_set_to_concept() {
        let mut conn = setup_test_db();

        let props_call = ToolCall {
            name: "define_property".to_string(),
            arguments: serde_json::json!({
                "operations": [{"iri": "foundation:boxSize", "label": "size", "property_type": "datatype"}]
            }),
        };
        assert!(execute_tool(&mut conn, &props_call, None, None).success);

        let call = ToolCall {
            name: "define_class".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Box",
                    "label": "Box",
                    "icon": "https://example.com/icon.svg",
                    "super_classes": ["owl:Thing"],
                    "add_properties": ["foundation:boxSize"]
                }]
            }),
        };
        execute_tool(&mut conn, &call, None, None);

        let prop = Property::get(&conn, "foundation:boxSize").unwrap().unwrap();
        assert!(
            prop.domains.iter().any(|d| d == "foundation:Box"),
            "Domain should be set to concept IRI, got: {:?}",
            prop.domains
        );
    }

    #[test]
    fn test_define_property_circular_formula_is_rejected() {
        let mut conn = setup_test_db();

        let call = ToolCall {
            name: "define_property".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:selfRef",
                    "label": "self ref",
                    "property_type": "datatype",
                    "range": "xsd:decimal",
                    "unit": "unit:Meter",
                    "formula": "{{foundation:selfRef}} + 1"
                }]
            }),
        };
        let result = execute_tool(&mut conn, &call, None, None);
        assert!(!result.success, "Circular formula should be rejected");
        let err = result.error.unwrap();
        assert!(err.contains("Circular"), "Expected circular dependency error, got: {err}");
    }

    // ---- define_class upsert: adding properties to existing class ----

    #[test]
    fn test_update_concept_adds_calculated_fields() {
        let mut conn = setup_test_db();

        let radius_call = ToolCall {
            name: "define_property".to_string(),
            arguments: serde_json::json!({
                "operations": [{"iri": "foundation:radius", "label": "radius", "property_type": "datatype"}]
            }),
        };
        assert!(execute_tool(&mut conn, &radius_call, None, None).success);

        let create_call = ToolCall {
            name: "define_class".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Circle",
                    "label": "Circle",
                    "icon": "https://example.com/icon.svg",
                    "super_classes": ["owl:Thing"],
                    "add_properties": ["foundation:radius"]
                }]
            }),
        };
        execute_tool(&mut conn, &create_call, None, None);

        let circ_call = ToolCall {
            name: "define_property".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:circumference",
                    "label": "circumference",
                    "property_type": "datatype",
                    "formula": "{{foundation:radius}} * 6"
                }]
            }),
        };
        assert!(execute_tool(&mut conn, &circ_call, None, None).success);

        let update_call = ToolCall {
            name: "define_class".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Circle",
                    "add_properties": ["foundation:circumference"]
                }]
            }),
        };
        let result = execute_tool(&mut conn, &update_call, None, None);
        assert!(result.success, "updating concept with new property should succeed: {:?}", result.error);

        let prop = Property::get(&conn, "foundation:circumference").unwrap().unwrap();
        assert_eq!(prop.property_type, PropertyType::DatatypeProperty);

        let stored = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:circumference", "foundation:formula"
        ).unwrap();
        assert!(!stored.triples.is_empty(), "Formula triple should be stored");
    }

}
