use super::*;

#[test]
fn test_search_things_returns_subclass_instances() {
    let mut conn = setup_test_db();
    setup_event_hierarchy(&mut conn);

    create_event(&mut conn, "foundation:Event_001", "foundation:Event");
    create_event(&mut conn, "foundation:Vacation_001", "foundation:Vacation");

    let args = serde_json::json!({
        "class_iri": "foundation:Event"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(
        iris.contains(&"foundation:Event_001"),
        "Results should include direct Event instance"
    );
    assert!(
        iris.contains(&"foundation:Vacation_001"),
        "Results should include Vacation (subclass) instance"
    );
    assert_eq!(things.len(), 2, "Should return exactly both instances");
}

#[test]
fn test_find_things_by_detail_returns_subclass_instances() {
    let mut conn = setup_test_db();
    setup_event_hierarchy(&mut conn);

    create_event(&mut conn, "foundation:Event_002", "foundation:Event");
    create_event(&mut conn, "foundation:Vacation_002", "foundation:Vacation");

    let status_iri = "foundation:PlannedStatus";
    let status_triple = crate::eavto::Triple::new(
        status_iri, "rdf:type", crate::eavto::Object::Iri("foundation:Status".to_string()),
    );
    crate::eavto::store::assert_triples(&mut conn, &[status_triple], "test").unwrap();

    for iri in ["foundation:Event_002", "foundation:Vacation_002"] {
        let individual = Individual::new(iri);
        individual.add_property(
            &mut conn,
            "foundation:hasStatus",
            vec![crate::owl::Object::Iri(status_iri.to_string())],
            "test",
        ).unwrap();
    }

    let args = serde_json::json!({
        "class_iri": "foundation:Event",
        "filters": [
            {"detail": "foundation:hasStatus", "value": status_iri}
        ]
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(
        iris.contains(&"foundation:Event_002"),
        "Results should include direct Event instance matching the filter"
    );
    assert!(
        iris.contains(&"foundation:Vacation_002"),
        "Results should include Vacation (subclass) instance matching the filter"
    );
    assert_eq!(things.len(), 2, "Should return exactly both instances");
}

#[test]
fn test_search_things_returns_instances_across_three_level_hierarchy() {
    let mut conn = setup_test_db();
    setup_animal_hierarchy(&mut conn);

    let animal = Individual::new("foundation:Animal_001");
    animal.assert(&mut conn, "foundation:Animal", "Test Animal", "https://example.com/animal.svg", "test").unwrap();

    let mammal = Individual::new("foundation:Mammal_001");
    mammal.assert(&mut conn, "foundation:Mammal", "Test Mammal", "https://example.com/mammal.svg", "test").unwrap();

    let dog = Individual::new("foundation:Dog_001");
    dog.assert(&mut conn, "foundation:Dog", "Test Dog", "https://example.com/dog.svg", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Animal"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Animal_001"), "Results should include direct Animal instance");
    assert!(iris.contains(&"foundation:Mammal_001"), "Results should include Mammal (subclass) instance");
    assert!(iris.contains(&"foundation:Dog_001"), "Results should include Dog (sub-subclass) instance");
    assert_eq!(things.len(), 3, "Should return exactly all three instances");
}

#[test]
fn test_search_things_returns_subclass_instances_when_parent_has_no_direct_instances() {
    let mut conn = setup_test_db();
    setup_event_hierarchy(&mut conn);

    create_event(&mut conn, "foundation:Vacation_003", "foundation:Vacation");
    create_event(&mut conn, "foundation:Vacation_004", "foundation:Vacation");

    let args = serde_json::json!({
        "class_iri": "foundation:Event"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Vacation_003"), "Results should include first Vacation instance");
    assert!(iris.contains(&"foundation:Vacation_004"), "Results should include second Vacation instance");
    assert_eq!(things.len(), 2, "Should return exactly both Vacation instances with no direct Event instances");
}

#[test]
fn test_search_things_filters_by_label_with_class_iri() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let individual1 = Individual::new("foundation:Task_S001");
    individual1.assert(&mut conn, "foundation:Task", "Ingresse Contract", "https://example.com/task.svg", "test").unwrap();

    let individual2 = Individual::new("foundation:Task_S002");
    individual2.assert(&mut conn, "foundation:Task", "Buy groceries", "https://example.com/task.svg", "test").unwrap();

    let individual3 = Individual::new("foundation:Task_S003");
    individual3.assert(&mut conn, "foundation:Task", "Read book", "https://example.com/task.svg", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "query": "Ingresse"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_S001"), "Should include matching instance");
    assert!(!iris.contains(&"foundation:Task_S002"), "Should not include non-matching instance");
    assert!(!iris.contains(&"foundation:Task_S003"), "Should not include non-matching instance");
    assert_eq!(things.len(), 1, "Should return exactly one matching instance");
}

#[test]
fn test_search_things_filters_globally_without_class_iri() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let event_class = Class::new("foundation:Event");
    event_class.assert(&mut conn, ClassType::OwlClass, "Event", "https://example.com/event.svg", None, "test").unwrap();

    let task = Individual::new("foundation:Task_S010");
    task.assert(&mut conn, "foundation:Task", "Ingresse contract", "https://example.com/task.svg", "test").unwrap();

    let event = Individual::new("foundation:Event_S010");
    event.assert(&mut conn, "foundation:Event", "Birthday party", "https://example.com/event.svg", "test").unwrap();

    let args = serde_json::json!({
        "query": "Ingresse"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_S010"), "Should include the matching Task instance");
    assert!(!iris.contains(&"foundation:Event_S010"), "Should not include the non-matching Event instance");
}

#[test]
fn test_search_things_label_match_is_case_insensitive() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let individual = Individual::new("foundation:Task_S020");
    individual.assert(&mut conn, "foundation:Task", "Ingresse Contract", "https://example.com/task.svg", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_S020"), "Case-insensitive match should return the instance");
}

#[test]
fn test_search_things_multi_token_query() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let individual1 = Individual::new("foundation:Task_S030");
    individual1.assert(&mut conn, "foundation:Task", "Ingresse Service Contract", "https://example.com/task.svg", "test").unwrap();

    let individual2 = Individual::new("foundation:Task_S031");
    individual2.assert(&mut conn, "foundation:Task", "Ingresse Proposal", "https://example.com/task.svg", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "query": "Ingresse contract"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_S030"), "Instance matching all tokens should be returned");
    assert!(!iris.contains(&"foundation:Task_S031"), "Instance not matching all tokens (AND) should be excluded");
}

#[test]
fn test_search_things_matches_by_comment() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let task_one = Individual::new("foundation:Task_comment_001");
    task_one.assert(&mut conn, "foundation:Task", "Task One", "https://example.com/task.svg", "test").unwrap();
    task_one.add_property(&mut conn, "rdfs:comment", vec![Object::Literal {
        value: "Ingresse contract details".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").unwrap();

    let task_two = Individual::new("foundation:Task_comment_002");
    task_two.assert(&mut conn, "foundation:Task", "Task Two", "https://example.com/task.svg", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter().filter_map(|t| t["id"].as_str()).collect();

    assert!(iris.contains(&"foundation:Task_comment_001"), "Task One should be returned");
    assert!(!iris.contains(&"foundation:Task_comment_002"), "Task Two should not be returned");
}

#[test]
fn test_search_things_matches_by_property_value() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let task_alpha = Individual::new("foundation:Task_prop_001");
    task_alpha.assert(&mut conn, "foundation:Task", "Task Alpha", "https://example.com/task.svg", "test").unwrap();
    task_alpha.add_property(&mut conn, "foundation:priority", vec![Object::Literal {
        value: "Ingresse project".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").unwrap();

    let task_beta = Individual::new("foundation:Task_prop_002");
    task_beta.assert(&mut conn, "foundation:Task", "Task Beta", "https://example.com/task.svg", "test").unwrap();
    task_beta.add_property(&mut conn, "foundation:priority", vec![Object::Literal {
        value: "other".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter().filter_map(|t| t["id"].as_str()).collect();

    assert!(iris.contains(&"foundation:Task_prop_001"), "Task Alpha should be returned");
    assert!(!iris.contains(&"foundation:Task_prop_002"), "Task Beta should not be returned");
}

#[test]
fn test_search_things_matched_properties_label_match() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let individual = Individual::new("foundation:Task_label_match_001");
    individual.assert(&mut conn, "foundation:Task", "Ingresse Contract", "https://example.com/task.svg", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let thing = things.iter().find(|t| t["id"].as_str() == Some("foundation:Task_label_match_001"))
        .expect("Task should be in results");

    assert_eq!(thing["label"].as_str(), Some("Ingresse Contract"), "label should appear at root level");
    let matched = thing["matchedProperties"].as_array().unwrap();
    assert!(
        matched.iter().all(|p| p["detail_iri"].as_str() != Some("rdfs:label")),
        "matchedProperties must not contain rdfs:label (label is already at root level)"
    );
}

#[test]
fn test_search_things_matched_properties_property_match() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let individual = Individual::new("foundation:Task_prop_match_001");
    individual.assert(&mut conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").unwrap();
    individual.add_property(&mut conn, "foundation:priority", vec![Object::Literal {
        value: "Ingresse project".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let thing = things.iter().find(|t| t["id"].as_str() == Some("foundation:Task_prop_match_001"))
        .expect("Task should be in results");

    let matched = thing["matchedProperties"].as_array().unwrap();
    assert!(
        matched.iter().any(|p| p["detail_iri"].as_str() == Some("foundation:priority")),
        "matchedProperties should contain foundation:priority entry"
    );
}

#[test]
fn test_search_things_score_ordering() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let label_match = Individual::new("foundation:Task_score_001");
    label_match.assert(&mut conn, "foundation:Task", "Ingresse", "https://example.com/task.svg", "test").unwrap();

    let comment_match = Individual::new("foundation:Task_score_002");
    comment_match.assert(&mut conn, "foundation:Task", "Task B", "https://example.com/task.svg", "test").unwrap();
    comment_match.add_property(&mut conn, "rdfs:comment", vec![Object::Literal {
        value: "Ingresse related".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").unwrap();

    let prop_match = Individual::new("foundation:Task_score_003");
    prop_match.assert(&mut conn, "foundation:Task", "Task C", "https://example.com/task.svg", "test").unwrap();
    prop_match.add_property(&mut conn, "foundation:priority", vec![Object::Literal {
        value: "Ingresse work".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter().filter_map(|t| t["id"].as_str()).collect();

    assert_eq!(iris.len(), 3, "All three tasks should be returned");

    let label_pos = iris.iter().position(|&iri| iri == "foundation:Task_score_001").unwrap();
    let comment_pos = iris.iter().position(|&iri| iri == "foundation:Task_score_002").unwrap();
    let prop_pos = iris.iter().position(|&iri| iri == "foundation:Task_score_003").unwrap();

    assert!(label_pos < comment_pos, "Label match should rank before comment match");
    assert!(comment_pos < prop_pos, "Comment match should rank before property match");
}

#[test]
fn test_search_things_include_retracted_false_excludes_retracted() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_101");
    create_task(&mut conn, "foundation:Task_102");

    Individual::retract(&mut conn, "foundation:Task_102", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "include_retracted": false
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_101"), "Results should include the non-retracted instance");
    assert!(!iris.contains(&"foundation:Task_102"), "Results should not include the retracted instance");
}

#[test]
fn test_search_things_include_retracted_true_includes_retracted() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_103");
    create_task(&mut conn, "foundation:Task_104");

    Individual::retract(&mut conn, "foundation:Task_104", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "include_retracted": true
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_103"), "Results should include the active instance");
    assert!(iris.contains(&"foundation:Task_104"), "Results should include the retracted instance");
}

#[test]
fn test_search_things_default_excludes_retracted() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);
    create_task(&mut conn, "foundation:Task_105");
    create_task(&mut conn, "foundation:Task_106");

    Individual::retract(&mut conn, "foundation:Task_106", "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_105"), "Results should include the active instance");
    assert!(!iris.contains(&"foundation:Task_106"), "Results should not include the retracted instance");
}

#[test]
fn test_search_things_limit_restricts_results() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    for i in 1..=5 {
        let iri = format!("foundation:Task_pag_{}", i);
        let label = format!("Task {}", i);
        let individual = Individual::new(&iri);
        individual.assert(&mut conn, "foundation:Task", &label, "https://example.com/icon.svg", "test").unwrap();
    }

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "limit": 3
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    assert_eq!(things.len(), 3, "Should return 3 things with limit 3");
    assert_eq!(response["total"].as_u64().unwrap(), 5, "Total should be 5");
}

#[test]
fn test_search_things_offset_skips_results() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    for i in 1..=5 {
        let iri = format!("foundation:Task_off_{}", i);
        let label = format!("Task {}", i);
        let individual = Individual::new(&iri);
        individual.assert(&mut conn, "foundation:Task", &label, "https://example.com/icon.svg", "test").unwrap();
    }

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "limit": 10,
        "offset": 3
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    assert_eq!(things.len(), 2, "Should return 2 things after skipping 3");
    assert_eq!(response["total"].as_u64().unwrap(), 5, "Total should be 5");
}

#[test]
fn test_search_things_default_limit_returns_all_when_under_limit() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    for i in 1..=3 {
        let iri = format!("foundation:Task_def_{}", i);
        let label = format!("Task {}", i);
        let individual = Individual::new(&iri);
        individual.assert(&mut conn, "foundation:Task", &label, "https://example.com/icon.svg", "test").unwrap();
    }

    let args = serde_json::json!({
        "class_iri": "foundation:Task"
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    assert_eq!(things.len(), 3, "Should return all 3 things");
    assert_eq!(response["total"].as_u64().unwrap(), 3, "Total should be 3");
}

#[test]
fn test_search_things_response_fields_are_correct() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    for i in 1..=2 {
        let iri = format!("foundation:Task_fields_{}", i);
        let label = format!("Task {}", i);
        let individual = Individual::new(&iri);
        individual.assert(&mut conn, "foundation:Task", &label, "https://example.com/icon.svg", "test").unwrap();
    }

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "limit": 10,
        "offset": 0
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    assert_eq!(response["limit"].as_u64().unwrap(), 10, "limit field should be 10");
    assert_eq!(response["offset"].as_u64().unwrap(), 0, "offset field should be 0");
    assert_eq!(response["total"].as_u64().unwrap(), 2, "total field should be 2");
}

fn setup_payment_class(conn: &mut Connection) {
    use crate::owl::{Class, ClassType, Property, PropertyType};
    Class::new("foundation:Payment")
        .assert(conn, ClassType::OwlClass, "Payment", ICON_URL, None, "test").unwrap();
    Property::new("foundation:transactionAmount")
        .assert(conn, PropertyType::DatatypeProperty, "Amount", None,
            &["foundation:Payment"], Some("xsd:decimal"), None, "test")
        .unwrap();
    Property::new("foundation:transactionDate")
        .assert(conn, PropertyType::DatatypeProperty, "Date", None,
            &["foundation:Payment"], Some("xsd:date"), None, "test")
        .unwrap();
}

fn create_payment(conn: &mut Connection, iri: &str, amount: f64) {
    let ind = Individual::new(iri);
    ind.assert(conn, "foundation:Payment", &format!("Payment {iri}"), ICON_URL, "test").unwrap();
    ind.add_property(conn, "foundation:transactionAmount",
        vec![Object::Number(amount)], "test").unwrap();
}

#[test]
fn test_filter_exists_without_value_is_not_silently_dropped() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let with_priority = Individual::new("foundation:Task_ex_001");
    with_priority.assert(&mut conn, "foundation:Task", "With Priority", ICON_URL, "test").unwrap();
    with_priority.add_property(&mut conn, "foundation:priority", vec![Object::Literal {
        value: "high".to_string(), datatype: Some("xsd:string".to_string()), language: None,
    }], "test").unwrap();

    let without_priority = Individual::new("foundation:Task_ex_002");
    without_priority.assert(&mut conn, "foundation:Task", "Without Priority", ICON_URL, "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "filters": [{"detail": "foundation:priority", "operator": "exists"}]
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);
    let entities = result.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris: Vec<&str> = entities.iter().filter_map(|e| e["id"].as_str()).collect();

    assert_eq!(iris.len(), 1, "only the task with priority should match");
    assert!(iris.contains(&"foundation:Task_ex_001"));
    assert!(!iris.contains(&"foundation:Task_ex_002"));
}

#[test]
fn test_filter_not_exists_without_value_is_not_silently_dropped() {
    let mut conn = setup_test_db();
    setup_task_class_with_statuses(&mut conn);

    let with_priority = Individual::new("foundation:Task_nex_001");
    with_priority.assert(&mut conn, "foundation:Task", "With Priority", ICON_URL, "test").unwrap();
    with_priority.add_property(&mut conn, "foundation:priority", vec![Object::Literal {
        value: "high".to_string(), datatype: Some("xsd:string".to_string()), language: None,
    }], "test").unwrap();

    let without_priority = Individual::new("foundation:Task_nex_002");
    without_priority.assert(&mut conn, "foundation:Task", "Without Priority", ICON_URL, "test").unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "filters": [{"detail": "foundation:priority", "operator": "not_exists"}]
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);
    let entities = result.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris: Vec<&str> = entities.iter().filter_map(|e| e["id"].as_str()).collect();

    assert_eq!(iris.len(), 1, "only the task without priority should match");
    assert!(!iris.contains(&"foundation:Task_nex_001"));
    assert!(iris.contains(&"foundation:Task_nex_002"));
}

#[test]
fn test_filter_numeric_json_number_is_not_silently_dropped() {
    // Regression: value: 128.7 (JSON number) was silently dropped because
    // the filter parser called .as_str() which returns None for numbers.
    let mut conn = setup_test_db();
    setup_payment_class(&mut conn);

    create_payment(&mut conn, "foundation:Payment_num_001", 128.7);
    create_payment(&mut conn, "foundation:Payment_num_002", 200.0);
    create_payment(&mut conn, "foundation:Payment_num_003", 128.7);

    let args = serde_json::json!({
        "class_iri": "foundation:Payment",
        "filters": [{"detail": "foundation:transactionAmount", "operator": "=", "value": 128.7}]
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);
    let entities = result.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris: Vec<&str> = entities.iter().filter_map(|e| e["id"].as_str()).collect();

    assert_eq!(iris.len(), 2, "exactly 2 payments of 128.7 should match");
    assert!(iris.contains(&"foundation:Payment_num_001"));
    assert!(!iris.contains(&"foundation:Payment_num_002"));
    assert!(iris.contains(&"foundation:Payment_num_003"));
}

#[test]
fn test_filter_numeric_gte_uses_numeric_not_lexicographic_comparison() {
    // Regression: "9" > "128.7" lexicographically, but 9.0 < 128.7 numerically.
    let mut conn = setup_test_db();
    setup_payment_class(&mut conn);

    create_payment(&mut conn, "foundation:Payment_gte_001", 9.0);
    create_payment(&mut conn, "foundation:Payment_gte_002", 100.0);
    create_payment(&mut conn, "foundation:Payment_gte_003", 200.0);

    let args = serde_json::json!({
        "class_iri": "foundation:Payment",
        "filters": [{"detail": "foundation:transactionAmount", "operator": ">=", "value": "100.0"}]
    });

    let result = search(&conn, &args);
    assert!(result.success, "search should succeed: {:?}", result.error);
    let entities = result.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris: Vec<&str> = entities.iter().filter_map(|e| e["id"].as_str()).collect();

    assert!(!iris.contains(&"foundation:Payment_gte_001"), "9.0 < 100.0 must be excluded");
    assert!(iris.contains(&"foundation:Payment_gte_002"), "100.0 >= 100.0 must be included");
    assert!(iris.contains(&"foundation:Payment_gte_003"), "200.0 >= 100.0 must be included");
}

// Performance tests — run with a copy of the real DB:
//   sqlite3 ~/Documents/Foundation/FOUNDATION.db "VACUUM INTO '/tmp/foundation_bench.db'"
//   FOUNDATION_BENCH_DB=/tmp/foundation_bench.db cargo test --lib perf_ -- --ignored --nocapture

#[test]
#[ignore = "performance test, requires FOUNDATION_BENCH_DB env var"]
fn perf_search_things_global_query() {
    let db_path = std::env::var("FOUNDATION_BENCH_DB")
        .expect("Set FOUNDATION_BENCH_DB to path of a DB copy (use VACUUM INTO to create it)");

    let conn = Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ).expect("Failed to open bench DB");

    let args = serde_json::json!({"query": "contract"});

    let _ = search(&conn, &args);

    let runs = 3u32;
    let mut total = std::time::Duration::ZERO;
    for i in 0..runs {
        let start = std::time::Instant::now();
        let result = search(&conn, &args);
        let elapsed = start.elapsed();
        total += elapsed;
        let response = result.result.unwrap();
        let n = response["total"].as_u64().unwrap_or(0);
        println!("[global query run {}] {} results in {:?}", i + 1, n, elapsed);
    }
    println!("[global query] average: {:?}", total / runs);
}

#[test]
#[ignore = "performance test, requires FOUNDATION_BENCH_DB env var"]
fn perf_search_things_with_class_iri() {
    let db_path = std::env::var("FOUNDATION_BENCH_DB")
        .expect("Set FOUNDATION_BENCH_DB to path of a DB copy (use VACUUM INTO to create it)");

    let conn = Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ).expect("Failed to open bench DB");

    let args = serde_json::json!({"class_iri": "foundation:Contract", "query": "Ingresse"});

    let _ = search(&conn, &args);

    let runs = 3u32;
    let mut total = std::time::Duration::ZERO;
    for i in 0..runs {
        let start = std::time::Instant::now();
        let result = search(&conn, &args);
        let elapsed = start.elapsed();
        total += elapsed;
        let response = result.result.unwrap();
        let n = response["total"].as_u64().unwrap_or(0);
        println!("[concept query run {}] {} results in {:?}", i + 1, n, elapsed);
    }
    println!("[concept query] average: {:?}", total / runs);
}
