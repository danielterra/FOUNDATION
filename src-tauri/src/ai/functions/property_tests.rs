use crate::ai::functions::{execute_tool, ToolCall};
use crate::eavto::{Connection, test_helpers::setup_test_db};
use crate::owl::{Class, ClassType};

const ICON: &str = "https://example.com/icon.svg";

fn setup_classes(conn: &mut Connection) {
    Class::new("foundation:Process")
        .assert(conn, ClassType::OwlClass, "Process", ICON, None, "test").unwrap();
    Class::new("foundation:Task")
        .assert(conn, ClassType::OwlClass, "Task", ICON, None, "test").unwrap();
    Class::new("foundation:Event")
        .assert(conn, ClassType::OwlClass, "Event", ICON, None, "test").unwrap();
}

// ── define_property ──────────────────────────────────────────────────────────

#[test]
fn test_define_property_creates_object_property() {
    let mut conn = setup_test_db();

    let result = execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:belongsTo",
                "label": "Belongs To",
                "property_type": "object",
                "range": "foundation:Process"
            }]
        }),
    }, None, None);
    assert!(result.success, "define_property should create property: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:belongsTo").unwrap().unwrap();
    assert_eq!(prop.property_type, crate::owl::PropertyType::ObjectProperty);
    assert!(prop.ranges.contains(&"foundation:Process".to_string()));
}

#[test]
fn test_define_property_creates_datatype_property() {
    let mut conn = setup_test_db();

    let result = execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:taskName",
                "label": "Task Name",
                "property_type": "datatype",
                "range": "xsd:string"
            }]
        }),
    }, None, None);
    assert!(result.success, "define_property should create datatype property: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:taskName").unwrap().unwrap();
    assert_eq!(prop.property_type, crate::owl::PropertyType::DatatypeProperty);
}

#[test]
fn test_define_property_does_not_change_domains() {
    let mut conn = setup_test_db();
    setup_classes(&mut conn);

    execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:stableProp", "label": "Stable Prop", "property_type": "object"}]
        }),
    }, None, None);

    execute_tool(&mut conn, &ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:Task", "add_properties": ["foundation:stableProp"]}]
        }),
    }, None, None);

    let result = execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:stableProp", "comment": "Updated comment"}]
        }),
    }, None, None);
    assert!(result.success, "update via define_property should succeed: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:stableProp").unwrap().unwrap();
    assert!(prop.domains.contains(&"foundation:Task".to_string()), "domain must not be removed by define_property");
}

#[test]
fn test_define_property_requires_label_for_new() {
    let mut conn = setup_test_db();
    let result = execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:noLabel", "property_type": "object"}]
        }),
    }, None, None);
    assert!(!result.success, "should fail without label");
}

#[test]
fn test_define_property_requires_property_type_for_new() {
    let mut conn = setup_test_db();
    let result = execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:noType", "label": "No Type"}]
        }),
    }, None, None);
    assert!(!result.success, "should fail without property_type");
}

// ── describe_property ────────────────────────────────────────────────────────

#[test]
fn test_describe_property_by_iri() {
    let mut conn = setup_test_db();

    execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:fetchMe",
                "label": "Fetch Me",
                "property_type": "object",
                "range": "foundation:Process"
            }]
        }),
    }, None, None);

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "foundation:fetchMe"}]}),
    }, None, None);
    assert!(result.success, "describe_property by iri should succeed: {:?}", result.error);
    let data = result.result.unwrap();
    assert_eq!(data["results"][0]["iri"].as_str().unwrap(), "foundation:fetchMe");
    assert_eq!(data["results"][0]["property_type"].as_str().unwrap(), "object");
}

#[test]
fn test_describe_property_search_by_label() {
    let mut conn = setup_test_db();

    execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:uniqueNameProp",
                "label": "UniqueSearchLabel",
                "property_type": "datatype",
                "range": "xsd:string"
            }]
        }),
    }, None, None);

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"query": "UniqueSearchLabel"}]}),
    }, None, None);
    assert!(result.success, "search should succeed: {:?}", result.error);
    let props = result.result.unwrap();
    let found = props["results"][0]["properties"].as_array().unwrap();
    assert!(found.iter().any(|p| p["iri"] == "foundation:uniqueNameProp"),
        "search should find the property by label");
}

#[test]
fn test_describe_property_not_found_returns_error() {
    let mut conn = setup_test_db();
    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "foundation:doesNotExist"}]}),
    }, None, None);
    assert!(!result.success, "should fail for unknown property IRI");
}
