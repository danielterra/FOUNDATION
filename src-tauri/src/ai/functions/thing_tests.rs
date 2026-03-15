use super::*;
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};
use crate::owl::{Class, ClassType, Individual, Property, PropertyType};

async fn setup_task_class_with_statuses(conn: &Connection) {
    let task_class = Class::new("foundation:Task");
    task_class.assert(conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test").await.unwrap();

    let triples = vec![
        Triple::new("foundation:ActiveStatus", "rdf:type", Object::Iri("foundation:Status".to_string())),
        Triple::new("foundation:ActiveStatus", "rdfs:label", Object::Literal {
            value: "Active".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:DoneStatus", "rdf:type", Object::Iri("foundation:Status".to_string())),
        Triple::new("foundation:DoneStatus", "rdfs:label", Object::Literal {
            value: "Done".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:ActiveStatus".to_string())),
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:DoneStatus".to_string())),
    ];
    store::assert_triples(conn, &triples, "test").await.unwrap();

    Property::new("foundation:priority")
        .assert(conn, PropertyType::DatatypeProperty, "priority", None, &["foundation:Task"], Some("xsd:string"), None, "test")
        .await.unwrap();

    Property::new("foundation:hasStatus")
        .assert(conn, PropertyType::ObjectProperty, "hasStatus", None, &["foundation:Task"], None, None, "test")
        .await.unwrap();
}

async fn create_task(conn: &Connection, iri: &str) {
    let individual = Individual::new(iri);
    individual.assert(conn, "foundation:Task", "Test Task", "https://example.com/icon.svg", "test").await.unwrap();
}

#[tokio::test]
async fn test_update_thing_with_properties_updates_literal_property() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_001").await;

    let args = serde_json::json!({
        "iri": "foundation:Task_001",
        "upsert_properties": [
            {
                "detail_iri": "foundation:priority",
                "values": ["High"]
            }
        ]
    });

    let result = update_thing_one(&conn, &args).await;
    assert!(result.success, "update_thing should succeed: {:?}", result.error);
    let response = result.result.unwrap();
    let updated = response["updatedFields"].as_array().unwrap();
    assert!(
        updated.iter().any(|v| v == "foundation:priority"),
        "Should report foundation:priority as updated"
    );
}

#[tokio::test]
async fn test_update_thing_with_valid_status_succeeds() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_002").await;

    let args = serde_json::json!({
        "iri": "foundation:Task_002",
        "upsert_properties": [
            {
                "detail_iri": "foundation:hasStatus",
                "values": ["foundation:ActiveStatus"]
            }
        ]
    });

    let result = update_thing_one(&conn, &args).await;
    assert!(result.success, "update_thing with valid status should succeed: {:?}", result.error);
}

#[tokio::test]
async fn test_update_thing_with_invalid_status_returns_descriptive_error() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_003").await;

    let args = serde_json::json!({
        "iri": "foundation:Task_003",
        "upsert_properties": [
            {
                "detail_iri": "foundation:hasStatus",
                "values": ["foundation:InvalidStatus"]
            }
        ]
    });

    let result = update_thing_one(&conn, &args).await;
    assert!(!result.success, "update_thing with invalid status should fail");
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

#[tokio::test]
async fn test_update_thing_partial_update_with_only_label() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_004").await;

    let args = serde_json::json!({
        "iri": "foundation:Task_004",
        "label": "Updated Task Name"
    });

    let result = update_thing_one(&conn, &args).await;
    assert!(result.success, "Partial update with only label should succeed: {:?}", result.error);
    let response = result.result.unwrap();
    let updated = response["updatedFields"].as_array().unwrap();
    assert_eq!(updated.len(), 1, "Only label should be reported as updated");
    assert_eq!(updated[0], "label");
}

#[tokio::test]
async fn test_create_thing_without_icon_inherits_from_concept() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "label": "My Inherited Icon Task"
    });

    let result = create_thing_one(&conn, &args).await;
    assert!(result.success, "create_thing without icon should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let iri = response["iri"].as_str().expect("result should have iri");

    let individual = crate::owl::Individual::get(&conn, iri).await
        .expect("should be able to get individual")
        .expect("individual should exist");

    assert_eq!(
        individual.icon.as_deref(),
        Some("https://example.com/task.svg"),
        "Individual should inherit the concept's icon"
    );
}

async fn setup_event_hierarchy(conn: &Connection) {
    let event_class = Class::new("foundation:Event");
    event_class.assert(conn, ClassType::OwlClass, "Event", "https://example.com/event.svg", None, "test").await.unwrap();

    let vacation_class = Class::new("foundation:Vacation");
    vacation_class.assert(
        conn, ClassType::OwlClass, "Vacation", "https://example.com/vacation.svg",
        Some("foundation:Event"), "test",
    ).await.unwrap();

    Property::new("foundation:hasStatus")
        .assert(conn, PropertyType::ObjectProperty, "hasStatus", None, &["foundation:Event"], None, None, "test")
        .await.unwrap();
}

async fn create_event(conn: &Connection, iri: &str, concept_iri: &str) {
    let individual = Individual::new(iri);
    individual.assert(conn, concept_iri, "Test Event", "https://example.com/event.svg", "test").await.unwrap();
}

#[tokio::test]
async fn test_search_things_returns_subclass_instances() {
    let conn = setup_test_db().await;
    setup_event_hierarchy(&conn).await;

    create_event(&conn, "foundation:Event_001", "foundation:Event").await;
    create_event(&conn, "foundation:Vacation_001", "foundation:Vacation").await;

    let args = serde_json::json!({
        "concept_iri": "foundation:Event"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

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

#[tokio::test]
async fn test_find_things_by_detail_returns_subclass_instances() {
    let conn = setup_test_db().await;
    setup_event_hierarchy(&conn).await;

    create_event(&conn, "foundation:Event_002", "foundation:Event").await;
    create_event(&conn, "foundation:Vacation_002", "foundation:Vacation").await;

    let status_iri = "foundation:PlannedStatus";
    let status_triple = crate::eavto::Triple::new(
        status_iri, "rdf:type", crate::eavto::Object::Iri("foundation:Status".to_string()),
    );
    crate::eavto::store::assert_triples(&conn, &[status_triple], "test").await.unwrap();

    for iri in ["foundation:Event_002", "foundation:Vacation_002"] {
        let individual = Individual::new(iri);
        individual.add_property(
            &conn,
            "foundation:hasStatus",
            vec![crate::owl::Object::Iri(status_iri.to_string())],
            "test",
        ).await.unwrap();
    }

    let args = serde_json::json!({
        "concept_iri": "foundation:Event",
        "filters": [
            {"detail": "foundation:hasStatus", "value": status_iri}
        ]
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "find_things_by_detail should succeed: {:?}", result.error);

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

async fn setup_animal_hierarchy(conn: &Connection) {
    let animal_class = Class::new("foundation:Animal");
    animal_class.assert(conn, ClassType::OwlClass, "Animal", "https://example.com/animal.svg", None, "test").await.unwrap();

    let mammal_class = Class::new("foundation:Mammal");
    mammal_class.assert(
        conn, ClassType::OwlClass, "Mammal", "https://example.com/mammal.svg",
        Some("foundation:Animal"), "test",
    ).await.unwrap();

    let dog_class = Class::new("foundation:Dog");
    dog_class.assert(
        conn, ClassType::OwlClass, "Dog", "https://example.com/dog.svg",
        Some("foundation:Mammal"), "test",
    ).await.unwrap();
}

#[tokio::test]
async fn test_search_things_returns_instances_across_three_level_hierarchy() {
    let conn = setup_test_db().await;
    setup_animal_hierarchy(&conn).await;

    let animal = Individual::new("foundation:Animal_001");
    animal.assert(&conn, "foundation:Animal", "Test Animal", "https://example.com/animal.svg", "test").await.unwrap();

    let mammal = Individual::new("foundation:Mammal_001");
    mammal.assert(&conn, "foundation:Mammal", "Test Mammal", "https://example.com/mammal.svg", "test").await.unwrap();

    let dog = Individual::new("foundation:Dog_001");
    dog.assert(&conn, "foundation:Dog", "Test Dog", "https://example.com/dog.svg", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Animal"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

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

#[tokio::test]
async fn test_search_things_returns_subclass_instances_when_parent_has_no_direct_instances() {
    let conn = setup_test_db().await;
    setup_event_hierarchy(&conn).await;

    create_event(&conn, "foundation:Vacation_003", "foundation:Vacation").await;
    create_event(&conn, "foundation:Vacation_004", "foundation:Vacation").await;

    let args = serde_json::json!({
        "concept_iri": "foundation:Event"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Vacation_003"), "Results should include first Vacation instance");
    assert!(iris.contains(&"foundation:Vacation_004"), "Results should include second Vacation instance");
    assert_eq!(things.len(), 2, "Should return exactly both Vacation instances with no direct Event instances");
}

#[tokio::test]
async fn test_search_things_filters_by_label_with_concept_iri() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let individual1 = Individual::new("foundation:Task_S001");
    individual1.assert(&conn, "foundation:Task", "Ingresse Contract", "https://example.com/task.svg", "test").await.unwrap();

    let individual2 = Individual::new("foundation:Task_S002");
    individual2.assert(&conn, "foundation:Task", "Buy groceries", "https://example.com/task.svg", "test").await.unwrap();

    let individual3 = Individual::new("foundation:Task_S003");
    individual3.assert(&conn, "foundation:Task", "Read book", "https://example.com/task.svg", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "query": "Ingresse"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

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

#[tokio::test]
async fn test_search_things_filters_globally_without_concept_iri() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let event_class = Class::new("foundation:Event");
    event_class.assert(&conn, ClassType::OwlClass, "Event", "https://example.com/event.svg", None, "test").await.unwrap();

    let task = Individual::new("foundation:Task_S010");
    task.assert(&conn, "foundation:Task", "Ingresse contract", "https://example.com/task.svg", "test").await.unwrap();

    let event = Individual::new("foundation:Event_S010");
    event.assert(&conn, "foundation:Event", "Birthday party", "https://example.com/event.svg", "test").await.unwrap();

    let args = serde_json::json!({
        "query": "Ingresse"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_S010"), "Should include the matching Task instance");
    assert!(!iris.contains(&"foundation:Event_S010"), "Should not include the non-matching Event instance");
}

#[tokio::test]
async fn test_search_things_label_match_is_case_insensitive() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let individual = Individual::new("foundation:Task_S020");
    individual.assert(&conn, "foundation:Task", "Ingresse Contract", "https://example.com/task.svg", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_S020"), "Case-insensitive match should return the instance");
}

#[tokio::test]
async fn test_search_things_multi_token_query() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let individual1 = Individual::new("foundation:Task_S030");
    individual1.assert(&conn, "foundation:Task", "Ingresse Service Contract", "https://example.com/task.svg", "test").await.unwrap();

    let individual2 = Individual::new("foundation:Task_S031");
    individual2.assert(&conn, "foundation:Task", "Ingresse Proposal", "https://example.com/task.svg", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "query": "Ingresse contract"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_S030"), "Instance matching all tokens should be returned");
    assert!(!iris.contains(&"foundation:Task_S031"), "Instance not matching all tokens (AND) should be excluded");
}

#[tokio::test]
async fn test_search_things_matches_by_comment() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let task_one = Individual::new("foundation:Task_comment_001");
    task_one.assert(&conn, "foundation:Task", "Task One", "https://example.com/task.svg", "test").await.unwrap();
    task_one.add_property(&conn, "rdfs:comment", vec![Object::Literal {
        value: "Ingresse contract details".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").await.unwrap();

    let task_two = Individual::new("foundation:Task_comment_002");
    task_two.assert(&conn, "foundation:Task", "Task Two", "https://example.com/task.svg", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter().filter_map(|t| t["id"].as_str()).collect();

    assert!(iris.contains(&"foundation:Task_comment_001"), "Task One should be returned");
    assert!(!iris.contains(&"foundation:Task_comment_002"), "Task Two should not be returned");
}

#[tokio::test]
async fn test_search_things_matches_by_property_value() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let task_alpha = Individual::new("foundation:Task_prop_001");
    task_alpha.assert(&conn, "foundation:Task", "Task Alpha", "https://example.com/task.svg", "test").await.unwrap();
    task_alpha.add_property(&conn, "foundation:priority", vec![Object::Literal {
        value: "Ingresse project".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").await.unwrap();

    let task_beta = Individual::new("foundation:Task_prop_002");
    task_beta.assert(&conn, "foundation:Task", "Task Beta", "https://example.com/task.svg", "test").await.unwrap();
    task_beta.add_property(&conn, "foundation:priority", vec![Object::Literal {
        value: "other".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter().filter_map(|t| t["id"].as_str()).collect();

    assert!(iris.contains(&"foundation:Task_prop_001"), "Task Alpha should be returned");
    assert!(!iris.contains(&"foundation:Task_prop_002"), "Task Beta should not be returned");
}

#[tokio::test]
async fn test_search_things_matched_properties_label_match() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let individual = Individual::new("foundation:Task_label_match_001");
    individual.assert(&conn, "foundation:Task", "Ingresse Contract", "https://example.com/task.svg", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

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

#[tokio::test]
async fn test_search_things_matched_properties_property_match() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let individual = Individual::new("foundation:Task_prop_match_001");
    individual.assert(&conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").await.unwrap();
    individual.add_property(&conn, "foundation:priority", vec![Object::Literal {
        value: "Ingresse project".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

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

#[tokio::test]
async fn test_search_things_score_ordering() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    let label_match = Individual::new("foundation:Task_score_001");
    label_match.assert(&conn, "foundation:Task", "Ingresse", "https://example.com/task.svg", "test").await.unwrap();

    let comment_match = Individual::new("foundation:Task_score_002");
    comment_match.assert(&conn, "foundation:Task", "Task B", "https://example.com/task.svg", "test").await.unwrap();
    comment_match.add_property(&conn, "rdfs:comment", vec![Object::Literal {
        value: "Ingresse related".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").await.unwrap();

    let prop_match = Individual::new("foundation:Task_score_003");
    prop_match.assert(&conn, "foundation:Task", "Task C", "https://example.com/task.svg", "test").await.unwrap();
    prop_match.add_property(&conn, "foundation:priority", vec![Object::Literal {
        value: "Ingresse work".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "query": "ingresse"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

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

#[tokio::test]
async fn test_search_things_include_retracted_false_excludes_retracted() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_101").await;
    create_task(&conn, "foundation:Task_102").await;

    Individual::retract(&conn, "foundation:Task_102", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "include_retracted": false
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_101"), "Results should include the non-retracted instance");
    assert!(!iris.contains(&"foundation:Task_102"), "Results should not include the retracted instance");
}

#[tokio::test]
async fn test_search_things_include_retracted_true_includes_retracted() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_103").await;
    create_task(&conn, "foundation:Task_104").await;

    Individual::retract(&conn, "foundation:Task_104", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "include_retracted": true
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_103"), "Results should include the active instance");
    assert!(iris.contains(&"foundation:Task_104"), "Results should include the retracted instance");
}

#[tokio::test]
async fn test_search_things_default_excludes_retracted() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_105").await;
    create_task(&conn, "foundation:Task_106").await;

    Individual::retract(&conn, "foundation:Task_106", "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    let iris: Vec<&str> = things.iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(iris.contains(&"foundation:Task_105"), "Results should include the active instance");
    assert!(!iris.contains(&"foundation:Task_106"), "Results should not include the retracted instance");
}

#[tokio::test]
async fn test_search_things_limit_restricts_results() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    for i in 1..=5 {
        let iri = format!("foundation:Task_pag_{}", i);
        let label = format!("Task {}", i);
        let individual = Individual::new(&iri);
        individual.assert(&conn, "foundation:Task", &label, "https://example.com/icon.svg", "test").await.unwrap();
    }

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "limit": 3
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things_one should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    assert_eq!(things.len(), 3, "Should return 3 things with limit 3");
    assert_eq!(response["total"].as_u64().unwrap(), 5, "Total should be 5");
}

#[tokio::test]
async fn test_search_things_offset_skips_results() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    for i in 1..=5 {
        let iri = format!("foundation:Task_off_{}", i);
        let label = format!("Task {}", i);
        let individual = Individual::new(&iri);
        individual.assert(&conn, "foundation:Task", &label, "https://example.com/icon.svg", "test").await.unwrap();
    }

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "limit": 10,
        "offset": 3
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things_one should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    assert_eq!(things.len(), 2, "Should return 2 things after skipping 3");
    assert_eq!(response["total"].as_u64().unwrap(), 5, "Total should be 5");
}

#[tokio::test]
async fn test_search_things_default_limit_returns_all_when_under_limit() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    for i in 1..=3 {
        let iri = format!("foundation:Task_def_{}", i);
        let label = format!("Task {}", i);
        let individual = Individual::new(&iri);
        individual.assert(&conn, "foundation:Task", &label, "https://example.com/icon.svg", "test").await.unwrap();
    }

    let args = serde_json::json!({
        "concept_iri": "foundation:Task"
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things_one should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    let things = response["entities"].as_array().unwrap();
    assert_eq!(things.len(), 3, "Should return all 3 things");
    assert_eq!(response["total"].as_u64().unwrap(), 3, "Total should be 3");
}

#[tokio::test]
async fn test_search_things_response_fields_are_correct() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    for i in 1..=2 {
        let iri = format!("foundation:Task_fields_{}", i);
        let label = format!("Task {}", i);
        let individual = Individual::new(&iri);
        individual.assert(&conn, "foundation:Task", &label, "https://example.com/icon.svg", "test").await.unwrap();
    }

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "limit": 10,
        "offset": 0
    });

    let result = remember(&conn, &args).await;
    assert!(result.success, "search_things_one should succeed: {:?}", result.error);

    let response = result.result.unwrap();
    assert_eq!(response["limit"].as_u64().unwrap(), 10, "limit field should be 10");
    assert_eq!(response["offset"].as_u64().unwrap(), 0, "offset field should be 0");
    assert_eq!(response["total"].as_u64().unwrap(), 2, "total field should be 2");
}

#[tokio::test]
async fn test_remove_before_add_in_same_operation_preserves_new_value() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_migrate_001").await;

    let individual = Individual::new("foundation:Task_migrate_001");
    individual.add_property(&conn, "foundation:priority", vec![Object::Literal {
        value: "Low".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:Task_migrate_001",
        "remove_properties": ["foundation:priority"],
        "upsert_properties": [
            {
                "detail_iri": "foundation:priority",
                "values": ["High"]
            }
        ]
    });

    let result = update_thing_one(&conn, &args).await;
    assert!(result.success, "update_thing with remove+add should succeed: {:?}", result.error);

    let priority = crate::owl::get_literal_property(&conn, "foundation:Task_migrate_001", "foundation:priority").await
        .expect("query should succeed")
        .expect("priority should have a value after the operation");

    assert_eq!(priority, "High", "New value must survive: remove_properties wiped it (wrong order)");
}

#[tokio::test]
async fn test_update_thing_icon_file_url_is_accepted() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_icon_001").await;

    let args = serde_json::json!({
        "iri": "foundation:Task_icon_001",
        "icon": "file:///Users/daniel/Documents/Foundation/attachments/icon.png"
    });

    let result = update_thing_one(&conn, &args).await;
    assert!(result.success, "update_thing with file:// icon should succeed: {:?}", result.error);

    let individual = Individual::get(&conn, "foundation:Task_icon_001").await
        .unwrap().unwrap();
    assert_eq!(
        individual.icon.as_deref(),
        Some("file:///Users/daniel/Documents/Foundation/attachments/icon.png"),
        "Icon should be stored and readable as the file:// URL"
    );
}

#[tokio::test]
async fn test_update_thing_icon_https_url_is_accepted() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;
    create_task(&conn, "foundation:Task_icon_002").await;

    let args = serde_json::json!({
        "iri": "foundation:Task_icon_002",
        "icon": "https://example.com/icon.png"
    });

    let result = update_thing_one(&conn, &args).await;
    assert!(result.success, "update_thing with https:// icon should succeed: {:?}", result.error);
}

#[tokio::test]
async fn test_create_thing_label_satisfies_rdfs_label_required_field() {
    let conn = setup_test_db().await;
    setup_task_class_with_statuses(&conn).await;

    crate::owl::cardinality::set_class_required_fields(
        &conn, "foundation:Task", &["foundation:priority"], "test",
    ).await.unwrap();

    crate::eavto::store::append_triples(&conn, &[
        {
            let blank_id = "_:restriction_rdfs_label_test".to_string();
            crate::eavto::Triple::new(
                "foundation:Task", "rdfs:subClassOf",
                crate::eavto::Object::Blank(blank_id.clone()),
            )
        },
    ], "test").await.unwrap();
    crate::eavto::store::assert_triples(&conn, &[
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
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "label": "My Task",
        "upsert_properties": [
            {"detail_iri": "foundation:priority", "values": ["High"]}
        ]
    });

    let result = create_thing_one(&conn, &args).await;
    assert!(
        result.success,
        "Creating a thing with label should satisfy rdfs:label required field; got: {:?}",
        result.error
    );
}

const ICON_URL: &str = "https://example.com/icon.svg";

async fn setup_bpmn_hierarchy(conn: &Connection) {
    let i = ICON_URL;
    Class::new("foundation:bpmn_Process")
        .assert(conn, ClassType::OwlClass, "BPMN Process", i, None, "test").await.unwrap();
    Class::new("foundation:bpmn_FlowNode")
        .assert(conn, ClassType::OwlClass, "Flow Node", i, None, "test").await.unwrap();
    Property::new("foundation:partOfProcess")
        .assert(conn, PropertyType::ObjectProperty, "Part of Process", None,
            &["foundation:bpmn_FlowNode"], Some("foundation:bpmn_Process"), None, "test")
        .await.unwrap();
    Class::new("foundation:bpmn_Event")
        .assert(conn, ClassType::OwlClass, "BPMN Event", i,
            Some("foundation:bpmn_FlowNode"), "test").await.unwrap();
    Class::new("foundation:bpmn_StartEvent")
        .assert(conn, ClassType::OwlClass, "Start Event", i,
            Some("foundation:bpmn_Event"), "test").await.unwrap();
    Class::new("foundation:bpmn_Task")
        .assert(conn, ClassType::OwlClass, "BPMN Task", i,
            Some("foundation:bpmn_FlowNode"), "test").await.unwrap();
    Class::new("foundation:bpmn_AgentTask")
        .assert(conn, ClassType::OwlClass, "Agent Task", i,
            Some("foundation:bpmn_Task"), "test").await.unwrap();
    Individual::new("foundation:Process_Reg")
        .assert(conn, "foundation:bpmn_Process", "Reg Process", i, "test").await.unwrap();
}

async fn set_part_of_process(
    conn: &Connection, ind_iri: &str,
) -> Result<(), crate::owl::OwlError> {
    Individual::new(ind_iri).add_property(
        conn, "foundation:partOfProcess",
        vec![Object::Iri("foundation:Process_Reg".to_string())], "test",
    ).await
}

#[tokio::test]
async fn test_part_of_process_accessible_on_start_event() {
    let conn = setup_test_db().await;
    setup_bpmn_hierarchy(&conn).await;
    Individual::new("foundation:StartEvent_Reg")
        .assert(&conn, "foundation:bpmn_StartEvent", "Reg Start", ICON_URL, "test").await.unwrap();
    assert!(set_part_of_process(&conn, "foundation:StartEvent_Reg").await.is_ok(),
        "partOfProcess must be settable on bpmn_StartEvent via bpmn_FlowNode inheritance");
}

#[tokio::test]
async fn test_part_of_process_accessible_on_agent_task() {
    let conn = setup_test_db().await;
    setup_bpmn_hierarchy(&conn).await;
    Individual::new("foundation:AgentTask_Reg")
        .assert(&conn, "foundation:bpmn_AgentTask", "Reg Agent", ICON_URL, "test").await.unwrap();
    assert!(set_part_of_process(&conn, "foundation:AgentTask_Reg").await.is_ok(),
        "partOfProcess must be settable on bpmn_AgentTask via bpmn_FlowNode inheritance");
}

#[tokio::test]
async fn test_part_of_process_accessible_on_sequence_flow() {
    let conn = setup_test_db().await;
    setup_bpmn_hierarchy(&conn).await;
    Property::new("foundation:partOfProcess")
        .assert(&conn, PropertyType::ObjectProperty, "Part of Process", None,
            &["foundation:bpmn_FlowNode", "foundation:bpmn_SequenceFlow"],
            Some("foundation:bpmn_Process"), None, "test")
        .await.unwrap();
    Class::new("foundation:bpmn_SequenceFlow")
        .assert(&conn, ClassType::OwlClass, "Sequence Flow", ICON_URL, None, "test").await.unwrap();
    Individual::new("foundation:SeqFlow_Reg")
        .assert(&conn, "foundation:bpmn_SequenceFlow", "Reg Flow", ICON_URL, "test").await.unwrap();
    assert!(set_part_of_process(&conn, "foundation:SeqFlow_Reg").await.is_ok(),
        "partOfProcess must be settable on bpmn_SequenceFlow (direct domain match)");
}

#[tokio::test]
async fn test_find_things_by_detail_date_filter_with_operators() {
    let conn = setup_test_db().await;

    let task_class = Class::new("foundation:Task");
    task_class.assert(&conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test").await.unwrap();

    Property::new("foundation:dueDate")
        .assert(&conn, PropertyType::DatatypeProperty, "dueDate", None, &["foundation:Task"], Some("xsd:date"), None, "test")
        .await.unwrap();

    Individual::new("foundation:TaskDue0307")
        .assert(&conn, "foundation:Task", "Task Due 2026-03-07", "https://example.com/task.svg", "test").await.unwrap();
    Individual::new("foundation:TaskDue0308")
        .assert(&conn, "foundation:Task", "Task Due 2026-03-08", "https://example.com/task.svg", "test").await.unwrap();
    Individual::new("foundation:TaskDue0309")
        .assert(&conn, "foundation:Task", "Task Due 2026-03-09", "https://example.com/task.svg", "test").await.unwrap();

    for (iri, date_str) in [
        ("foundation:TaskDue0307", "2026-03-07"),
        ("foundation:TaskDue0308", "2026-03-08"),
        ("foundation:TaskDue0309", "2026-03-09"),
    ] {
        Individual::new(iri).add_property(
            &conn,
            "foundation:dueDate",
            vec![crate::owl::Object::Literal {
                value: date_str.to_string(),
                datatype: Some("xsd:date".to_string()),
                language: None,
            }],
            "test",
        ).await.unwrap();
    }

    let args = serde_json::json!({
        "concept_iri": "foundation:Task",
        "filters": [
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": "="}
        ]
    });
    let result = remember(&conn, &args).await;
    assert!(result.success, "date '=' filter should succeed: {:?}", result.error);
    let things = result.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris: Vec<&str> = things.iter().filter_map(|t| t["id"].as_str()).collect();
    assert_eq!(iris, vec!["foundation:TaskDue0308"], "date '=' should return only the matching task");

    let args_gte = serde_json::json!({
        "concept_iri": "foundation:Task",
        "filters": [
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": ">="}
        ]
    });
    let result_gte = remember(&conn, &args_gte).await;
    assert!(result_gte.success, "date '>=' filter should succeed: {:?}", result_gte.error);
    let things_gte = result_gte.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris_gte: Vec<&str> = things_gte.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(iris_gte.contains(&"foundation:TaskDue0308"), "'>=' should include 2026-03-08");
    assert!(iris_gte.contains(&"foundation:TaskDue0309"), "'>=' should include 2026-03-09");
    assert!(!iris_gte.contains(&"foundation:TaskDue0307"), "'>=' should exclude 2026-03-07");

    let args_lte = serde_json::json!({
        "concept_iri": "foundation:Task",
        "filters": [
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": "<="}
        ]
    });
    let result_lte = remember(&conn, &args_lte).await;
    assert!(result_lte.success, "date '<=' filter should succeed: {:?}", result_lte.error);
    let things_lte = result_lte.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris_lte: Vec<&str> = things_lte.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(iris_lte.contains(&"foundation:TaskDue0307"), "'<=' should include 2026-03-07");
    assert!(iris_lte.contains(&"foundation:TaskDue0308"), "'<=' should include 2026-03-08");
    assert!(!iris_lte.contains(&"foundation:TaskDue0309"), "'<=' should exclude 2026-03-09");

    let args_range = serde_json::json!({
        "concept_iri": "foundation:Task",
        "filters": [
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": ">="},
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": "<="}
        ]
    });
    let result_range = remember(&conn, &args_range).await;
    assert!(result_range.success, "date range filter should succeed: {:?}", result_range.error);
    let things_range = result_range.result.unwrap()["entities"].as_array().unwrap().clone();
    assert_eq!(things_range.len(), 1, "range >=2026-03-08 AND <=2026-03-08 should return exactly 1");
    assert_eq!(things_range[0]["id"].as_str().unwrap(), "foundation:TaskDue0308");
}

#[tokio::test]
async fn test_create_thing_with_invalid_object_property_returns_range_contexts() {
    let conn = setup_test_db().await;

    let persona_class = Class::new("foundation:Persona");
    persona_class.assert(&conn, ClassType::OwlClass, "Persona", "https://example.com/persona.svg", None, "test").await.unwrap();

    let user_story_class = Class::new("foundation:UserStory");
    user_story_class.assert(&conn, ClassType::OwlClass, "User Story", "https://example.com/story.svg", None, "test").await.unwrap();

    Property::new("foundation:userRole")
        .assert(&conn, PropertyType::ObjectProperty, "user role", None, &["foundation:UserStory"], Some("foundation:Persona"), None, "test")
        .await.unwrap();

    let args = serde_json::json!({
        "concept_iri": "foundation:UserStory",
        "label": "As a user",
        "upsert_properties": [
            {
                "detail_iri": "foundation:userRole",
                "values": ["foundation:INVALID_PERSONA"]
            }
        ]
    });

    let result = create_thing_one(&conn, &args).await;
    assert!(!result.success, "create_thing with invalid IRI should fail");

    let range_ctx = result.result.expect("result should contain range contexts on failure");
    let range_contexts = range_ctx["rangeContexts"].as_array().expect("should have rangeContexts");
    assert!(!range_contexts.is_empty(), "should include range context for the failing property");

    let ctx = &range_contexts[0];
    assert_eq!(ctx["property"].as_str().unwrap(), "foundation:userRole");
    assert_eq!(ctx["range"].as_str().unwrap(), "foundation:Persona");

    assert!(result.concept.is_some(), "concept (UserStory) should be included in the error response");
}
