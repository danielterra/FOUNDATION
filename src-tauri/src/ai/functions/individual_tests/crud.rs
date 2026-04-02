use super::*;

#[test]
fn test_update_thing_with_properties_updates_literal_property() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_001");

    let args = serde_json::json!({
        "iri": "foundation:Task_001",
        "property_iri": "foundation:priority",
        "values": ["High"]
    });

    let result = replace_property_values_one(&mut conn, &args);
    assert!(result.success, "replace_property_values should succeed: {:?}", result.error);
    assert_eq!(result.result.unwrap()["iri"].as_str(), Some("foundation:Task_001"));
}

#[test]
fn test_update_thing_with_valid_status_succeeds() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_002");

    let args = serde_json::json!({
        "iri": "foundation:Task_002",
        "property_iri": "foundation:hasStatus",
        "values": ["foundation:ActiveStatus"]
    });

    let result = replace_property_values_one(&mut conn, &args);
    assert!(result.success, "replace_property_values with valid status should succeed: {:?}", result.error);
}

#[test]
fn test_update_thing_with_invalid_status_returns_descriptive_error() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_003");

    let args = serde_json::json!({
        "iri": "foundation:Task_003",
        "property_iri": "foundation:hasStatus",
        "values": ["foundation:InvalidStatus"]
    });

    let result = replace_property_values_one(&mut conn, &args);
    assert!(!result.success, "replace_property_values with invalid status should fail");
    let error = result.error.unwrap();
    assert!(
        error.contains("foundation:InvalidStatus"),
        "Error should mention the invalid status: {}", error
    );
    assert!(
        error.contains("Allowed") || error.contains("allowed"),
        "Error should list allowed statuses: {}", error
    );
}

#[test]
fn test_update_thing_partial_update_with_only_label() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_004");

    let args = serde_json::json!({
        "iri": "foundation:Task_004",
        "property_iri": "rdfs:label",
        "values": ["Updated Task Name"]
    });

    let result = replace_property_values_one(&mut conn, &args);
    assert!(result.success, "Replacing rdfs:label should succeed: {:?}", result.error);
    assert_eq!(result.result.unwrap()["iri"].as_str(), Some("foundation:Task_004"));
}

#[test]
fn test_create_thing_without_icon_inherits_from_concept() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "label": "My Inherited Icon Task"
    });

    let result = assert_individual_one(&mut conn, &args);
    assert!(result.success, "assert_individual without icon should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let iri = response["iri"].as_str().expect("result should have iri");

    let individual = crate::owl::Individual::get(&conn, iri)
        .expect("should be able to get individual")
        .expect("individual should exist");

    assert_eq!(
        individual.icon.as_deref(),
        Some("https://example.com/task.svg"),
        "Individual should inherit the concept's icon"
    );
}

#[test]
fn test_create_thing_label_satisfies_rdfs_label_required_field() {
    // Regression for Bug_1772766219258: creating an instance with label should satisfy
    // an rdfs:label cardinality restriction, since label is always stored as rdfs:label.
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    crate::owl::cardinality::set_class_required_fields(
        &mut conn, "foundation:Task", &["foundation:priority"], "test",
    ).unwrap();

    // Simulate how rdfs:label ends up as a required field (legacy data or direct store)
    crate::eavto::store::append_triples(&mut conn, &[
        {
            let blank_id = "_:restriction_rdfs_label_test".to_string();
            crate::eavto::Triple::new(
                "foundation:Task", "rdfs:subClassOf",
                crate::eavto::Object::Blank(blank_id.clone()),
            )
        },
    ], "test").unwrap();
    crate::eavto::store::assert_triples(&mut conn, &[
        crate::eavto::Triple::new(
            "_:restriction_rdfs_label_test", "rdf:type",
            crate::eavto::Object::Iri("owl:Restriction".to_string()),
        ),
        crate::eavto::Triple::new(
            "_:restriction_rdfs_label_test", "owl:onProperty",
            crate::eavto::Object::Iri("rdfs:label".to_string()),
        ),
        crate::eavto::Triple::new(
            "_:restriction_rdfs_label_test", "owl:minCardinality",
            crate::eavto::Object::Integer(1),
        ),
    ], "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "label": "My Task",
        "properties": [
            {"property_iri": "foundation:priority", "values": ["High"]}
        ]
    });

    let result = assert_individual_one(&mut conn, &args);
    assert!(
        result.success,
        "Creating a thing with label should satisfy rdfs:label required field; got: {:?}",
        result.error
    );
}

#[test]
fn test_update_thing_icon_file_url_is_accepted() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_icon_001");

    let args = serde_json::json!({
        "iri": "foundation:Task_icon_001",
        "property_iri": "foundation:hasIcon",
        "values": ["file:///Users/daniel/Documents/Foundation/attachments/icon.png"]
    });

    let result = replace_property_values_one(&mut conn, &args);
    assert!(result.success, "replace_property_values with file:// icon should succeed: {:?}", result.error);

    let individual = Individual::get(&conn, "foundation:Task_icon_001")
        .unwrap().unwrap();
    assert_eq!(
        individual.icon.as_deref(),
        Some("file:///Users/daniel/Documents/Foundation/attachments/icon.png"),
        "Icon should be stored and readable as the file:// URL"
    );
}

#[test]
fn test_update_thing_icon_https_url_is_accepted() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_icon_002");

    let args = serde_json::json!({
        "iri": "foundation:Task_icon_002",
        "property_iri": "foundation:hasIcon",
        "values": ["https://example.com/icon.png"]
    });

    let result = replace_property_values_one(&mut conn, &args);
    assert!(result.success, "replace_property_values with https:// icon should succeed: {:?}", result.error);
}

#[test]
fn test_remove_before_add_in_same_operation_preserves_new_value() {
    // Regression for Bug_1772970415230: remove must execute before replace
    // so the new value written by replace is not wiped by the subsequent remove.
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_migrate_001");

    let individual = Individual::new("foundation:Task_migrate_001");
    individual.add_property(&mut conn, "foundation:priority", vec![Object::Literal {
        value: "Low".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").unwrap();

    let remove_args = serde_json::json!({
        "iri": "foundation:Task_migrate_001",
        "property_iri": "foundation:priority",
        "values": ["Low"]
    });
    let remove_result = remove_property_values_one(&mut conn, &remove_args);
    assert!(remove_result.success, "remove_property_values should succeed: {:?}", remove_result.error);

    let replace_args = serde_json::json!({
        "iri": "foundation:Task_migrate_001",
        "property_iri": "foundation:priority",
        "values": ["High"]
    });
    let replace_result = replace_property_values_one(&mut conn, &replace_args);
    assert!(replace_result.success, "replace_property_values should succeed: {:?}", replace_result.error);

    let priority = crate::owl::get_literal_property(&conn, "foundation:Task_migrate_001", "foundation:priority")
        .expect("query should succeed")
        .expect("priority should have a value after the operation");

    assert_eq!(priority, "High", "New value must survive after remove+replace");
}

#[test]
fn test_retract_individual_response_includes_cascade_summary() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("test:Parent", "rdf:type", Object::Iri("test:ParentClass".to_string())),
        Triple::new("test:Child1", "rdf:type", Object::Iri("test:ChildClass".to_string())),
        Triple::new("test:Child2", "rdf:type", Object::Iri("test:ChildClass".to_string())),
        Triple::new("test:Parent", "test:owns", Object::Iri("test:Child1".to_string())),
        Triple::new("test:Parent", "test:owns", Object::Iri("test:Child2".to_string())),
        Triple::new("test:ParentClass", "foundation:cascadeDeleteRange",
            Object::Iri("test:owns".to_string())),
    ], "test").unwrap();

    let result = retract_individual_one(&mut conn, &serde_json::json!({"iri": "test:Parent"}));
    assert!(result.success, "retract_individual must succeed: {:?}", result.error);

    let message = result.result.unwrap()["message"].as_str().unwrap().to_string();
    assert!(message.contains("Cascade"), "Response must include cascade summary; got: {}", message);
    assert!(message.contains("2"), "Cascade count must be 2; got: {}", message);
    assert!(message.contains("test:owns"), "Cascade must name the property; got: {}", message);
    assert!(message.contains("range"), "Cascade must name the direction; got: {}", message);
}

#[test]
fn test_retract_individual_response_no_cascade_no_summary() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("test:Standalone", "rdf:type", Object::Iri("test:SomeClass".to_string())),
        Triple::new("test:Standalone", "rdfs:label", Object::Literal {
            value: "standalone".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = retract_individual_one(&mut conn, &serde_json::json!({"iri": "test:Standalone"}));
    assert!(result.success, "retract_individual must succeed: {:?}", result.error);

    let message = result.result.unwrap()["message"].as_str().unwrap().to_string();
    assert!(!message.contains("Cascade"), "No cascade must mean no summary; got: {}", message);
}
