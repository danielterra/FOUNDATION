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

// ── describe_property IRI validation ─────────────────────────────────────────

#[test]
fn test_describe_property_valid_prefixed_iri_returns_property_details() {
    let mut conn = setup_test_db();

    execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:validPrefixedProp", "label": "Valid Prefixed Prop", "property_type": "datatype"}]
        }),
    }, None, None);

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "foundation:validPrefixedProp"}]}),
    }, None, None);
    assert!(result.success, "valid prefixed IRI should succeed: {:?}", result.error);
    let data = result.result.unwrap();
    assert_eq!(data["results"][0]["iri"].as_str().unwrap(), "foundation:validPrefixedProp");
}

#[test]
fn test_describe_property_valid_absolute_iri_passes_validation() {
    let mut conn = setup_test_db();

    execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "http://example.org/prop", "label": "Example Prop", "property_type": "datatype"}]
        }),
    }, None, None);

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "http://example.org/prop"}]}),
    }, None, None);
    assert!(result.success, "valid absolute IRI should succeed: {:?}", result.error);
    let data = result.result.unwrap();
    assert_eq!(data["results"][0]["iri"].as_str().unwrap(), "http://example.org/prop");
}

#[test]
fn test_describe_property_plain_text_iri_returns_invalid_iri_error() {
    let mut conn = setup_test_db();

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "part of product"}]}),
    }, None, None);
    assert!(!result.success, "plain text IRI should fail");
    let err = result.error.unwrap();
    assert!(err.contains("Invalid IRI"), "error should contain 'Invalid IRI', got: {err}");
    assert!(err.contains("query"), "error should suggest using 'query' field, got: {err}");
}

#[test]
fn test_describe_property_empty_string_iri_returns_invalid_iri_error() {
    let mut conn = setup_test_db();

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": ""}]}),
    }, None, None);
    assert!(!result.success, "empty string IRI should fail");
    let err = result.error.unwrap();
    assert!(err.contains("Invalid IRI"), "error should contain 'Invalid IRI', got: {err}");
}

#[test]
fn test_describe_property_numeric_string_iri_returns_invalid_iri_error() {
    let mut conn = setup_test_db();

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "123"}]}),
    }, None, None);
    assert!(!result.success, "numeric string IRI should fail");
    let err = result.error.unwrap();
    assert!(err.contains("Invalid IRI"), "error should contain 'Invalid IRI', got: {err}");
}

#[test]
fn test_describe_property_invalid_iri_error_is_immediate_with_no_lookup() {
    let mut conn = setup_test_db();

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "no colon here"}]}),
    }, None, None);
    assert!(!result.success, "invalid IRI should fail");
    let err = result.error.unwrap();
    assert!(err.contains("Invalid IRI"),
        "error must say 'Invalid IRI' (not 'not found'), confirming no lookup was attempted; got: {err}");
    assert!(!err.to_lowercase().contains("not found"),
        "error must not be a NotFound error — lookup must not have been attempted; got: {err}");
}

#[test]
fn test_describe_property_query_only_skips_iri_validation() {
    let mut conn = setup_test_db();

    execute_tool(&mut conn, &ToolCall {
        name: "define_property".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:statusQueryProp", "label": "StatusQueryLabel", "property_type": "datatype"}]
        }),
    }, None, None);

    let result = execute_tool(&mut conn, &ToolCall {
        name: "describe_property".to_string(),
        arguments: serde_json::json!({"operations": [{"query": "StatusQueryLabel"}]}),
    }, None, None);
    assert!(result.success, "query-only should succeed without IRI validation: {:?}", result.error);
    let data = result.result.unwrap();
    let props = data["results"][0]["properties"].as_array().unwrap();
    assert!(props.iter().any(|p| p["iri"] == "foundation:statusQueryProp"),
        "search should find the property by label");
}
