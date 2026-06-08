use super::*;
use crate::owl::Individual;

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

#[test]
fn test_remove_domain_retracts_property_values_from_instances() {
    let mut conn = setup_test_db();

    // Test class and property
    Class::new("foundation:Widget")
        .assert(&mut conn, ClassType::OwlClass, "Widget", "https://example.com/widget.svg", None, "test")
        .unwrap();
    Property::new("foundation:widgetColor")
        .assert(&mut conn, PropertyType::DatatypeProperty, "Widget Color", None,
            &["foundation:Widget"], Some("xsd:string"), None, "test")
        .unwrap();

    // 3 instances with a widgetColor value
    for (iri, color) in [
        ("foundation:Widget_A", "red"),
        ("foundation:Widget_B", "green"),
        ("foundation:Widget_C", "blue"),
    ] {
        Individual::new(iri)
            .assert(&mut conn, "foundation:Widget", iri, "https://example.com/widget.svg", "test")
            .unwrap();
        Individual::new(iri)
            .add_property(
                &mut conn,
                "foundation:widgetColor",
                vec![Object::Literal { value: color.to_string(), datatype: Some("xsd:string".to_string()), language: None }],
                "test",
            )
            .unwrap();
    }

    // Confirm the values exist before the cascade
    for iri in ["foundation:Widget_A", "foundation:Widget_B", "foundation:Widget_C"] {
        let count = Individual::get_property_count(&conn, iri, "foundation:widgetColor").unwrap();
        assert_eq!(count, 1, "{} should have 1 widgetColor value before the cascade", iri);
    }

    // Remove foundation:Widget from the domain → should cascade and retract the values
    let args = serde_json::json!({
        "iri": "foundation:widgetColor",
        "property_iri": "rdfs:domain",
        "values": ["foundation:Widget"]
    });
    let result = remove_property_values_one(&mut conn, &args);
    assert!(result.success, "remove_property_values should succeed: {:?}", result.error);

    // Verify the values were retracted on all instances
    for iri in ["foundation:Widget_A", "foundation:Widget_B", "foundation:Widget_C"] {
        let count = Individual::get_property_count(&conn, iri, "foundation:widgetColor").unwrap();
        assert_eq!(count, 0, "{} should have no widgetColor values after the cascade", iri);
    }
}

fn setup_numeric_props(conn: &mut Connection) {
    Property::new("foundation:dueDayOfMonth")
        .assert(conn, PropertyType::DatatypeProperty, "Due Day", None,
            &["foundation:Task"], Some("xsd:integer"), Some("unit:Count"), "test")
        .unwrap();
    Property::new("foundation:contractValue")
        .assert(conn, PropertyType::DatatypeProperty, "Contract Value", None,
            &["foundation:Task"], Some("xsd:decimal"), Some("currency:USD"), "test")
        .unwrap();
    Property::new("foundation:isActive")
        .assert(conn, PropertyType::DatatypeProperty, "Is Active", None,
            &["foundation:Task"], Some("xsd:boolean"), None, "test")
        .unwrap();
}

#[test]
fn test_add_property_values_accepts_json_integer() {
    // Regression for Bug_1777034280520: numeric JSON values were silently dropped
    // by `.filter_map(as_str)`, producing "no valid string entries" errors.
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    setup_numeric_props(&mut conn);
    create_task(&mut conn, "foundation:Task_num_001");

    let args = serde_json::json!({
        "iri": "foundation:Task_num_001",
        "property_iri": "foundation:dueDayOfMonth",
        "values": [5]
    });

    let result = add_property_values_one(&mut conn, &args);
    assert!(result.success, "JSON integer must succeed: {:?}", result.error);

    let stored = crate::owl::get_literal_property(
        &conn, "foundation:Task_num_001", "foundation:dueDayOfMonth",
    ).unwrap().unwrap();
    assert_eq!(stored, "5", "Integer value must be stored as its lexical form");
}

#[test]
fn test_replace_property_values_accepts_json_decimal() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    setup_numeric_props(&mut conn);
    create_task(&mut conn, "foundation:Task_num_002");

    let args = serde_json::json!({
        "iri": "foundation:Task_num_002",
        "property_iri": "foundation:contractValue",
        "values": [1234.56]
    });

    let result = replace_property_values_one(&mut conn, &args);
    assert!(result.success, "JSON decimal must succeed: {:?}", result.error);

    let stored = crate::owl::get_literal_property(
        &conn, "foundation:Task_num_002", "foundation:contractValue",
    ).unwrap().unwrap();
    assert_eq!(stored, "1234.56", "Decimal value must be stored as its lexical form");
}

#[test]
fn test_assert_individual_accepts_numeric_and_boolean_properties() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    setup_numeric_props(&mut conn);

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "label": "Task with numeric fields",
        "properties": [
            {"property_iri": "foundation:dueDayOfMonth", "values": [15]},
            {"property_iri": "foundation:contractValue", "values": [99.9]},
            {"property_iri": "foundation:isActive", "values": [true]}
        ]
    });

    let result = assert_individual_one(&mut conn, &args);
    assert!(result.success, "mixed numeric/boolean values must succeed: {:?}", result.error);

    let iri = result.result.unwrap()["iri"].as_str().unwrap().to_string();
    assert_eq!(
        crate::owl::get_literal_property(&conn, &iri, "foundation:dueDayOfMonth").unwrap().unwrap(),
        "15"
    );
    assert_eq!(
        crate::owl::get_literal_property(&conn, &iri, "foundation:contractValue").unwrap().unwrap(),
        "99.9"
    );
    assert_eq!(
        crate::owl::get_literal_property(&conn, &iri, "foundation:isActive").unwrap().unwrap(),
        "true"
    );
}

#[test]
fn test_add_property_values_rejects_object_value_with_descriptive_error() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    setup_numeric_props(&mut conn);
    create_task(&mut conn, "foundation:Task_num_003");

    let args = serde_json::json!({
        "iri": "foundation:Task_num_003",
        "property_iri": "foundation:dueDayOfMonth",
        "values": [{"foo": "bar"}]
    });

    let result = add_property_values_one(&mut conn, &args);
    assert!(!result.success, "Object values must be rejected");
    let err = result.error.unwrap();
    assert!(err.contains("unsupported value type"), "Error must be descriptive: {}", err);
}

#[test]
fn test_assert_individual_rejects_nonexistent_class() {
    // Regression for Bug observed with foundation:Extracted_1777066198582: the agent
    // asserted individuals with invented class IRIs (e.g. foundation:PurchaseOrder)
    // because the backend did not validate class existence.
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let args = serde_json::json!({
        "class_iri": "foundation:ClassThatDoesNotExist",
        "label": "Should fail"
    });

    let result = assert_individual_one(&mut conn, &args);
    assert!(!result.success, "assert_individual with unknown class must fail");
    let err = result.error.unwrap();
    assert!(
        err.contains("foundation:ClassThatDoesNotExist") && err.contains("does not exist"),
        "Error must name the missing class: {}", err,
    );
    assert!(
        err.contains("define_class"),
        "Error must suggest creating the class: {}", err,
    );
}

#[test]
fn test_assert_individual_accepts_builtin_owl_thing() {
    // owl:Thing is the implicit root class; it may have no explicit triples
    // but must still be accepted as a valid type.
    let mut conn = setup_test_db();

    let args = serde_json::json!({
        "class_iri": "owl:Thing",
        "label": "Untyped thing",
        "icon": "https://example.com/icon.svg"
    });

    let result = assert_individual_one(&mut conn, &args);
    assert!(result.success, "owl:Thing must be accepted: {:?}", result.error);
}
