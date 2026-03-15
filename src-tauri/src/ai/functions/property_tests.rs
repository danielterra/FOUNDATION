use crate::ai::functions::{execute_tool, ToolCall};
use crate::eavto::{Connection, test_helpers::setup_test_db};
use crate::owl::{Class, ClassType};

const ICON: &str = "https://example.com/icon.svg";

async fn setup_classes(conn: &Connection) {
    Class::new("foundation:Process")
        .assert(conn, ClassType::OwlClass, "Process", ICON, None, "test").await.unwrap();
    Class::new("foundation:Task")
        .assert(conn, ClassType::OwlClass, "Task", ICON, None, "test").await.unwrap();
    Class::new("foundation:Event")
        .assert(conn, ClassType::OwlClass, "Event", ICON, None, "test").await.unwrap();
}

// ── learn_properties ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_learn_property_creates_object_property() {
    let conn = setup_test_db().await;

    let result = execute_tool(&conn, &ToolCall {
        name: "learn_properties".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:belongsTo",
                "label": "Belongs To",
                "property_type": "object",
                "range": "foundation:Process"
            }]
        }),
    }, None).await;
    assert!(result.success, "learn_properties should create property: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:belongsTo").await.unwrap().unwrap();
    assert_eq!(prop.property_type, crate::owl::PropertyType::ObjectProperty);
    assert!(prop.ranges.contains(&"foundation:Process".to_string()));
}

#[tokio::test]
async fn test_learn_property_creates_datatype_property() {
    let conn = setup_test_db().await;

    let result = execute_tool(&conn, &ToolCall {
        name: "learn_properties".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:taskName",
                "label": "Task Name",
                "property_type": "datatype",
                "range": "xsd:string"
            }]
        }),
    }, None).await;
    assert!(result.success, "learn_properties should create datatype property: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:taskName").await.unwrap().unwrap();
    assert_eq!(prop.property_type, crate::owl::PropertyType::DatatypeProperty);
}

#[tokio::test]
async fn test_learn_property_does_not_change_domains() {
    let conn = setup_test_db().await;
    setup_classes(&conn).await;

    execute_tool(&conn, &ToolCall {
        name: "learn_properties".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:stableProp", "label": "Stable Prop", "property_type": "object"}]
        }),
    }, None).await;

    execute_tool(&conn, &ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:Task", "upsert_details": ["foundation:stableProp"]}]
        }),
    }, None).await;

    let result = execute_tool(&conn, &ToolCall {
        name: "learn_properties".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:stableProp", "comment": "Updated comment"}]
        }),
    }, None).await;
    assert!(result.success, "update via learn_properties should succeed: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:stableProp").await.unwrap().unwrap();
    assert!(prop.domains.contains(&"foundation:Task".to_string()), "domain must not be removed by learn_properties");
}

#[tokio::test]
async fn test_learn_property_requires_label_for_new() {
    let conn = setup_test_db().await;
    let result = execute_tool(&conn, &ToolCall {
        name: "learn_properties".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:noLabel", "property_type": "object"}]
        }),
    }, None).await;
    assert!(!result.success, "should fail without label");
}

#[tokio::test]
async fn test_learn_property_requires_property_type_for_new() {
    let conn = setup_test_db().await;
    let result = execute_tool(&conn, &ToolCall {
        name: "learn_properties".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:noType", "label": "No Type"}]
        }),
    }, None).await;
    assert!(!result.success, "should fail without property_type");
}

// ── remember_properties ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_remember_property_by_iri() {
    let conn = setup_test_db().await;

    execute_tool(&conn, &ToolCall {
        name: "learn_properties".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:fetchMe",
                "label": "Fetch Me",
                "property_type": "object",
                "range": "foundation:Process"
            }]
        }),
    }, None).await;

    let result = execute_tool(&conn, &ToolCall {
        name: "remember_properties".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "foundation:fetchMe"}]}),
    }, None).await;
    assert!(result.success, "remember_property by iri should succeed: {:?}", result.error);
    let data = result.result.unwrap();
    assert_eq!(data["results"][0]["iri"].as_str().unwrap(), "foundation:fetchMe");
    assert_eq!(data["results"][0]["property_type"].as_str().unwrap(), "object");
}

#[tokio::test]
async fn test_remember_property_search_by_label() {
    let conn = setup_test_db().await;

    execute_tool(&conn, &ToolCall {
        name: "learn_properties".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:uniqueNameProp",
                "label": "UniqueSearchLabel",
                "property_type": "datatype",
                "range": "xsd:string"
            }]
        }),
    }, None).await;

    let result = execute_tool(&conn, &ToolCall {
        name: "remember_properties".to_string(),
        arguments: serde_json::json!({"operations": [{"query": "UniqueSearchLabel"}]}),
    }, None).await;
    assert!(result.success, "search should succeed: {:?}", result.error);
    let props = result.result.unwrap();
    let found = props["results"][0]["properties"].as_array().unwrap();
    assert!(found.iter().any(|p| p["iri"] == "foundation:uniqueNameProp"),
        "search should find the property by label");
}

#[tokio::test]
async fn test_remember_property_not_found_returns_error() {
    let conn = setup_test_db().await;
    let result = execute_tool(&conn, &ToolCall {
        name: "remember_properties".to_string(),
        arguments: serde_json::json!({"operations": [{"iri": "foundation:doesNotExist"}]}),
    }, None).await;
    assert!(!result.success, "should fail for unknown property IRI");
}
