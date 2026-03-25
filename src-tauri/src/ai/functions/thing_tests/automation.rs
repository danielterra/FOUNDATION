use super::*;

#[test]
fn test_part_of_process_accessible_on_start_event() {
    let mut conn = setup_test_db();
    setup_automation_hierarchy(&mut conn);
    Individual::new("foundation:StartEvent_Reg")
        .assert(&mut conn, "foundation:automation_StartEvent", "Reg Start", ICON_URL, "test").unwrap();
    assert!(set_part_of_process(&mut conn, "foundation:StartEvent_Reg").is_ok(),
        "partOfProcess must be settable on automation_StartEvent via automation_FlowNode inheritance");
}

#[test]
fn test_part_of_process_accessible_on_agent_task() {
    let mut conn = setup_test_db();
    setup_automation_hierarchy(&mut conn);
    Individual::new("foundation:AgentTask_Reg")
        .assert(&mut conn, "foundation:automation_AgentTask", "Reg Agent", ICON_URL, "test").unwrap();
    assert!(set_part_of_process(&mut conn, "foundation:AgentTask_Reg").is_ok(),
        "partOfProcess must be settable on automation_AgentTask via automation_FlowNode inheritance");
}

#[test]
fn test_part_of_process_accessible_on_sequence_flow() {
    let mut conn = setup_test_db();
    setup_automation_hierarchy(&mut conn);
    // partOfProcess domain covers both automation_FlowNode and automation_SequenceFlow
    Property::new("foundation:partOfProcess")
        .assert(&mut conn, PropertyType::ObjectProperty, "Part of Process", None,
            &["foundation:automation_FlowNode", "foundation:automation_SequenceFlow"],
            Some("foundation:automation_Process"), None, "test")
        .unwrap();
    Class::new("foundation:automation_SequenceFlow")
        .assert(&mut conn, ClassType::OwlClass, "Sequence Flow", ICON_URL, None, "test").unwrap();
    Individual::new("foundation:SeqFlow_Reg")
        .assert(&mut conn, "foundation:automation_SequenceFlow", "Reg Flow", ICON_URL, "test").unwrap();
    assert!(set_part_of_process(&mut conn, "foundation:SeqFlow_Reg").is_ok(),
        "partOfProcess must be settable on automation_SequenceFlow (direct domain match)");
}

#[test]
fn test_find_things_by_detail_date_filter_with_operators() {
    let mut conn = setup_test_db();

    let task_class = Class::new("foundation:Task");
    task_class.assert(&mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test").unwrap();

    Property::new("foundation:dueDate")
        .assert(&mut conn, PropertyType::DatatypeProperty, "dueDate", None, &["foundation:Task"], Some("xsd:date"), None, "test")
        .unwrap();

    Individual::new("foundation:TaskDue0307")
        .assert(&mut conn, "foundation:Task", "Task Due 2026-03-07", "https://example.com/task.svg", "test").unwrap();
    Individual::new("foundation:TaskDue0308")
        .assert(&mut conn, "foundation:Task", "Task Due 2026-03-08", "https://example.com/task.svg", "test").unwrap();
    Individual::new("foundation:TaskDue0309")
        .assert(&mut conn, "foundation:Task", "Task Due 2026-03-09", "https://example.com/task.svg", "test").unwrap();

    for (iri, date_str) in [
        ("foundation:TaskDue0307", "2026-03-07"),
        ("foundation:TaskDue0308", "2026-03-08"),
        ("foundation:TaskDue0309", "2026-03-09"),
    ] {
        Individual::new(iri).add_property(
            &mut conn,
            "foundation:dueDate",
            vec![crate::owl::Object::Literal {
                value: date_str.to_string(),
                datatype: Some("xsd:date".to_string()),
                language: None,
            }],
            "test",
        ).unwrap();
    }

    let args = serde_json::json!({
        "class_iri": "foundation:Task",
        "filters": [
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": "="}
        ]
    });
    let result = search(&conn, &args);
    assert!(result.success, "date '=' filter should succeed: {:?}", result.error);
    let things = result.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris: Vec<&str> = things.iter().filter_map(|t| t["id"].as_str()).collect();
    assert_eq!(iris, vec!["foundation:TaskDue0308"], "date '=' should return only the matching task");

    let args_gte = serde_json::json!({
        "class_iri": "foundation:Task",
        "filters": [
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": ">="}
        ]
    });
    let result_gte = search(&conn, &args_gte);
    assert!(result_gte.success, "date '>=' filter should succeed: {:?}", result_gte.error);
    let things_gte = result_gte.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris_gte: Vec<&str> = things_gte.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(iris_gte.contains(&"foundation:TaskDue0308"), "'>=' should include 2026-03-08");
    assert!(iris_gte.contains(&"foundation:TaskDue0309"), "'>=' should include 2026-03-09");
    assert!(!iris_gte.contains(&"foundation:TaskDue0307"), "'>=' should exclude 2026-03-07");

    let args_lte = serde_json::json!({
        "class_iri": "foundation:Task",
        "filters": [
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": "<="}
        ]
    });
    let result_lte = search(&conn, &args_lte);
    assert!(result_lte.success, "date '<=' filter should succeed: {:?}", result_lte.error);
    let things_lte = result_lte.result.unwrap()["entities"].as_array().unwrap().clone();
    let iris_lte: Vec<&str> = things_lte.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(iris_lte.contains(&"foundation:TaskDue0307"), "'<=' should include 2026-03-07");
    assert!(iris_lte.contains(&"foundation:TaskDue0308"), "'<=' should include 2026-03-08");
    assert!(!iris_lte.contains(&"foundation:TaskDue0309"), "'<=' should exclude 2026-03-09");

    let args_range = serde_json::json!({
        "class_iri": "foundation:Task",
        "filters": [
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": ">="},
            {"detail": "foundation:dueDate", "value": "2026-03-08", "operator": "<="}
        ]
    });
    let result_range = search(&conn, &args_range);
    assert!(result_range.success, "date range filter should succeed: {:?}", result_range.error);
    let things_range = result_range.result.unwrap()["entities"].as_array().unwrap().clone();
    assert_eq!(things_range.len(), 1, "range >=2026-03-08 AND <=2026-03-08 should return exactly 1");
    assert_eq!(things_range[0]["id"].as_str().unwrap(), "foundation:TaskDue0308");
}

#[test]
fn test_create_thing_with_invalid_object_property_returns_range_contexts() {
    let mut conn = setup_test_db();

    let persona_class = Class::new("foundation:Persona");
    persona_class.assert(&mut conn, ClassType::OwlClass, "Persona", "https://example.com/persona.svg", None, "test").unwrap();

    let user_story_class = Class::new("foundation:UserStory");
    user_story_class.assert(&mut conn, ClassType::OwlClass, "User Story", "https://example.com/story.svg", None, "test").unwrap();

    Property::new("foundation:userRole")
        .assert(&mut conn, PropertyType::ObjectProperty, "user role", None, &["foundation:UserStory"], Some("foundation:Persona"), None, "test")
        .unwrap();

    let args = serde_json::json!({
        "class_iri": "foundation:UserStory",
        "label": "As a user",
        "properties": [
            {
                "property_iri": "foundation:userRole",
                "values": ["foundation:INVALID_PERSONA"]
            }
        ]
    });

    let result = assert_individual_one(&mut conn, &args);
    assert!(!result.success, "assert_individual with invalid IRI should fail");

    let range_ctx = result.result.expect("result should contain range contexts on failure");
    let range_contexts = range_ctx["rangeContexts"].as_array().expect("should have rangeContexts");
    assert!(!range_contexts.is_empty(), "should include range context for the failing property");

    let ctx = &range_contexts[0];
    assert_eq!(ctx["property"].as_str().unwrap(), "foundation:userRole");
    assert_eq!(ctx["range"].as_str().unwrap(), "foundation:Persona");

    assert!(result.concept.is_some(), "concept (UserStory) should be included in the error response");
}
