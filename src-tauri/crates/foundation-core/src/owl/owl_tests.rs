use super::*;
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};

// ── batch_insert_triples (append semantics) ───────────────────────────────────

#[test]
fn test_batch_insert_triples_appends_without_replacing() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:X", "test:tag", Object::Literal {
            value: "first".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    batch_insert_triples(&mut conn, &[
        Triple::new("test:X", "test:tag", Object::Literal {
            value: "second".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let vals = get_all_property_values(&conn, "test:X", "test:tag").unwrap();
    assert!(vals.contains(&"first".to_string()), "first value must still be present after append");
    assert!(vals.contains(&"second".to_string()), "second value must be appended");
}

#[test]
fn test_batch_insert_triples_empty_slice_is_noop() {
    let mut conn = setup_test_db();
    let result = batch_insert_triples(&mut conn, &[], "test");
    assert!(result.is_ok());
}

// ── assert_raw_triples (replace semantics) ────────────────────────────────────

#[test]
fn test_assert_raw_triples_replaces_existing_value() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Y", "test:name", Object::Literal {
            value: "old".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    assert_raw_triples(&mut conn, &[
        Triple::new("test:Y", "test:name", Object::Literal {
            value: "new".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let vals = get_all_property_values(&conn, "test:Y", "test:name").unwrap();
    assert!(!vals.contains(&"old".to_string()), "old value must be replaced");
    assert!(vals.contains(&"new".to_string()), "new value must be present");
}

#[test]
fn test_assert_raw_triples_read_confirms_written_triple() {
    let mut conn = setup_test_db();
    let triples = vec![
        Triple::new("test:Z", "rdf:type", Object::Iri("test:Thing".to_string())),
    ];

    assert_raw_triples(&mut conn, &triples, "test").unwrap();

    let vals = get_all_property_values(&conn, "test:Z", "rdf:type").unwrap();
    assert!(vals.contains(&"test:Thing".to_string()));
}

// ── try_iri_direct_lookup ─────────────────────────────────────────────────────

#[test]
fn test_try_iri_direct_lookup_existing_iri_returns_result() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:MyEntity", "rdf:type", Object::Iri("test:SomeClass".to_string())),
        Triple::new("test:MyEntity", "rdfs:label", Object::Literal {
            value: "My Entity".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let result = search::try_iri_direct_lookup(&conn, "test:MyEntity");
    assert!(result.is_some(), "existing IRI must return Some");
    let result = result.unwrap();
    assert_eq!(result.id, "test:MyEntity");
}

#[test]
fn test_try_iri_direct_lookup_nonexistent_iri_returns_none() {
    let conn = setup_test_db();
    let result = search::try_iri_direct_lookup(&conn, "test:Ghost");
    assert!(result.is_none(), "non-existent IRI must return None");
}

#[test]
fn test_try_iri_direct_lookup_query_with_space_returns_none() {
    let conn = setup_test_db();
    let result = search::try_iri_direct_lookup(&conn, "test:My Entity");
    assert!(result.is_none(), "query with space must return None");
}

#[test]
fn test_try_iri_direct_lookup_no_colon_returns_none() {
    let conn = setup_test_db();
    let result = search::try_iri_direct_lookup(&conn, "notAnIri");
    assert!(result.is_none(), "query without colon must return None");
}

// ── replace_all_property_iris ─────────────────────────────────────────────────

#[test]
fn test_replace_all_property_iris_saves_all_values() {
    let mut conn = setup_test_db();

    store::assert_triples(
        &mut conn,
        &[Triple::new(
            "foundation:TestConcept",
            "rdf:type",
            Object::Iri("owl:Class".to_string()),
        )],
        "test",
    ).unwrap();

    for iri in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
        store::assert_triples(
            &mut conn,
            &[Triple::new(*iri, "rdf:type", Object::Iri("foundation:Status".to_string()))],
            "test",
        ).unwrap();
    }

    replace_all_property_iris(
        &mut conn,
        "foundation:TestConcept",
        "foundation:allowedStatus",
        &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"],
        "test",
    ).unwrap();

    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples \
         WHERE subject = 'foundation:TestConcept' \
           AND predicate = 'foundation:allowedStatus' \
           AND retracted = 0",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(active, 3, "all three allowedStatus values must be stored");

    for status in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM triples \
             WHERE subject = 'foundation:TestConcept' \
               AND predicate = 'foundation:allowedStatus' \
               AND object = ? AND retracted = 0",
            [status],
            |row| row.get(0),
        ).unwrap();
        assert!(exists, "{status} must be stored as allowedStatus");
    }
}

#[test]
fn test_replace_all_property_iris_replaces_existing_values() {
    let mut conn = setup_test_db();

    store::assert_triples(
        &mut conn,
        &[Triple::new(
            "foundation:TestConcept",
            "rdf:type",
            Object::Iri("owl:Class".to_string()),
        )],
        "test",
    ).unwrap();

    for iri in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
        store::assert_triples(
            &mut conn,
            &[Triple::new(*iri, "rdf:type", Object::Iri("foundation:Status".to_string()))],
            "test",
        ).unwrap();
    }

    replace_all_property_iris(
        &mut conn,
        "foundation:TestConcept",
        "foundation:allowedStatus",
        &["foundation:StatusA", "foundation:StatusB"],
        "test",
    ).unwrap();

    replace_all_property_iris(
        &mut conn,
        "foundation:TestConcept",
        "foundation:allowedStatus",
        &["foundation:StatusB", "foundation:StatusC"],
        "test",
    ).unwrap();

    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current \
         WHERE subject = 'foundation:TestConcept' \
           AND predicate = 'foundation:allowedStatus'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(active, 2, "only the new set of values must remain");

    let status_a_active: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM triples_current \
         WHERE subject = 'foundation:TestConcept' \
           AND predicate = 'foundation:allowedStatus' \
           AND object = 'foundation:StatusA'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(!status_a_active, "StatusA must be retracted after replacement");
}

// ── helpers shared by search tests ───────────────────────────────────────────

fn create_class(conn: &mut crate::eavto::Connection, iri: &str, label: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new(iri, "rdfs:label", Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();
}

fn create_individual(conn: &mut crate::eavto::Connection, iri: &str, class_iri: &str, label: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri(class_iri.to_string())),
        Triple::new(iri, "rdfs:label", Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();
}

fn lit(value: &str) -> Object {
    Object::Literal { value: value.to_string(), datatype: Some("xsd:string".to_string()), language: None }
}

// ── search_classes ────────────────────────────────────────────────────────────

#[test]
fn test_search_classes_empty_db() {
    let conn = setup_test_db();
    let result = search::search_classes(&conn, "task", 10).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_search_classes_finds_matching_label() {
    let mut conn = setup_test_db();
    create_class(&mut conn, "foundation:Task", "Task");
    create_class(&mut conn, "foundation:Project", "Project");

    let result = search::search_classes(&conn, "task", 10).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Task");
    assert!(result[0].is_class);
}

#[test]
fn test_search_classes_case_insensitive() {
    let mut conn = setup_test_db();
    create_class(&mut conn, "foundation:Task", "Task");

    let result = search::search_classes(&conn, "TASK", 10).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Task");
}

#[test]
fn test_search_classes_respects_limit() {
    let mut conn = setup_test_db();
    create_class(&mut conn, "foundation:TaskA", "Task Alpha");
    create_class(&mut conn, "foundation:TaskB", "Task Beta");
    create_class(&mut conn, "foundation:TaskC", "Task Gamma");

    let result = search::search_classes(&conn, "task", 2).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_search_classes_ranks_exact_match_first() {
    let mut conn = setup_test_db();
    create_class(&mut conn, "foundation:Task", "Task");
    create_class(&mut conn, "foundation:TaskType", "Task Type");

    let result = search::search_classes(&conn, "task", 10).unwrap();
    assert_eq!(result[0].id, "foundation:Task");
}

#[test]
fn test_search_classes_empty_query_returns_all_classes() {
    let mut conn = setup_test_db();
    create_class(&mut conn, "foundation:Task", "Task");
    create_class(&mut conn, "foundation:Bug", "Bug");
    create_class(&mut conn, "foundation:Project", "Project");

    let result = search::search_classes(&conn, "", 100).unwrap();
    assert_eq!(result.len(), 3, "empty query must return all classes");
    let ids: Vec<&str> = result.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"foundation:Task"));
    assert!(ids.contains(&"foundation:Bug"));
    assert!(ids.contains(&"foundation:Project"));
}

// ── search_individuals ────────────────────────────────────────────────────────

#[test]
fn test_search_individuals_empty_db() {
    let conn = setup_test_db();
    let result = search::search_individuals(&conn, "alice", 10).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_search_individuals_finds_matching_label() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:Alice", "foundation:Person", "Alice Smith");
    create_individual(&mut conn, "foundation:Bob", "foundation:Person", "Bob Jones");

    let result = search::search_individuals(&conn, "alice", 10).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Alice");
    assert!(!result[0].is_class);
}

#[test]
fn test_search_individuals_case_insensitive() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:Alice", "foundation:Person", "Alice Smith");

    let result = search::search_individuals(&conn, "ALICE", 10).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_search_individuals_excludes_owl_classes() {
    let mut conn = setup_test_db();
    create_class(&mut conn, "foundation:Task", "Task");
    create_individual(&mut conn, "foundation:MyTask", "foundation:Task", "Task Alpha");

    let result = search::search_individuals(&conn, "task", 10).unwrap();
    assert!(result.iter().all(|r| !r.is_class));
    assert!(result.iter().any(|r| r.id == "foundation:MyTask"));
}

#[test]
fn test_search_individuals_respects_limit() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:P1", "foundation:Person", "Alice A");
    create_individual(&mut conn, "foundation:P2", "foundation:Person", "Alice B");
    create_individual(&mut conn, "foundation:P3", "foundation:Person", "Alice C");

    let result = search::search_individuals(&conn, "alice", 2).unwrap();
    assert_eq!(result.len(), 2);
}

// ── search_instances ──────────────────────────────────────────────────────────

#[test]
fn test_search_instances_empty_query_returns_all() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:Alice", "foundation:Person", "Alice");
    create_individual(&mut conn, "foundation:Bob", "foundation:Person", "Bob");

    let result = search::search_instances(&conn, "", 100).unwrap();
    assert!(result.len() >= 2);
}

#[test]
fn test_search_instances_matches_by_label() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:Alice", "foundation:Person", "Alice Smith");
    create_individual(&mut conn, "foundation:Bob", "foundation:Person", "Bob Jones");

    let result = search::search_instances(&conn, "alice", 10).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Alice");
    assert_eq!(result[0].entity_type, "individual");
}

#[test]
fn test_search_instances_matches_by_property_value() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Doc1", "rdf:type", Object::Iri("foundation:Document".to_string())),
        Triple::new("foundation:Doc1", "rdfs:label", Object::Literal {
            value: "Report Q1".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:Doc1", "foundation:description", Object::Literal {
            value: "quarterly financials".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = search::search_instances(&conn, "quarterly", 10).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Doc1");
    assert!(!result[0].matched_properties.is_empty());
}

#[test]
fn test_search_instances_respects_limit() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:A1", "foundation:Item", "Apple A");
    create_individual(&mut conn, "foundation:A2", "foundation:Item", "Apple B");
    create_individual(&mut conn, "foundation:A3", "foundation:Item", "Apple C");

    let result = search::search_instances(&conn, "apple", 2).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_search_instances_returns_classes() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Vehicle", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Vehicle", "rdfs:label", Object::Literal {
            value: "Vehicle".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = search::search_instances(&conn, "vehicle", 10).unwrap();
    assert!(!result.is_empty());
    let found = result.iter().find(|r| r.id == "foundation:Vehicle").unwrap();
    assert_eq!(found.entity_type, "class");
}

#[test]
fn test_search_instances_iri_match_scores_highest() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:Alice", "foundation:Person", "Alice");
    create_individual(&mut conn, "foundation:Bob", "foundation:Person", "Bob Alice Fan");

    let result = search::search_instances(&conn, "foundation:Alice", 10).unwrap();
    assert!(!result.is_empty());
    assert_eq!(result[0].id, "foundation:Alice");
}

#[test]
fn test_search_instances_label_scores_higher_than_property() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TaskA", "rdf:type", Object::Iri("foundation:Task".to_string())),
        Triple::new("foundation:TaskA", "rdfs:label", Object::Literal {
            value: "Deploy".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:TaskB", "rdf:type", Object::Iri("foundation:Task".to_string())),
        Triple::new("foundation:TaskB", "rdfs:label", Object::Literal {
            value: "Other Task".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:TaskB", "foundation:description", Object::Literal {
            value: "deploy configuration".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = search::search_instances(&conn, "deploy", 10).unwrap();
    assert!(result.len() >= 2);
    assert_eq!(result[0].id, "foundation:TaskA");
}

#[test]
fn test_search_instances_label_exact_beats_starts_with_beats_contains() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E1", "rdf:type", Object::Iri("foundation:Thing".to_string())),
        Triple::new("foundation:E1", "rdfs:label", Object::Literal {
            value: "rust".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:E2", "rdf:type", Object::Iri("foundation:Thing".to_string())),
        Triple::new("foundation:E2", "rdfs:label", Object::Literal {
            value: "rust lang".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:E3", "rdf:type", Object::Iri("foundation:Thing".to_string())),
        Triple::new("foundation:E3", "rdfs:label", Object::Literal {
            value: "the rust book".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = search::search_instances(&conn, "rust", 10).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].id, "foundation:E1", "exact match must be first");
    assert_eq!(result[1].id, "foundation:E2", "starts_with must be second");
    assert_eq!(result[2].id, "foundation:E3", "contains must be last");
}

#[test]
fn test_search_instances_comment_match_works() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Widget", "rdf:type", Object::Iri("foundation:Component".to_string())),
        Triple::new("foundation:Widget", "rdfs:label", Object::Literal {
            value: "Widget".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:Widget", "rdfs:comment", Object::Literal {
            value: "A reusable UI element for dashboards".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = search::search_instances(&conn, "dashboard", 10).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Widget");
    let comment_prop = result[0].matched_properties.iter()
        .find(|p| p["detail_iri"] == "rdfs:comment");
    assert!(comment_prop.is_some(), "rdfs:comment must appear in matched_properties");
}

#[test]
fn test_search_instances_comment_beats_property() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alpha", "rdf:type", Object::Iri("foundation:Thing".to_string())),
        Triple::new("foundation:Alpha", "rdfs:label", Object::Literal {
            value: "Alpha".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:Alpha", "rdfs:comment", Object::Literal {
            value: "contains widget".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:Beta", "rdf:type", Object::Iri("foundation:Thing".to_string())),
        Triple::new("foundation:Beta", "rdfs:label", Object::Literal {
            value: "Beta".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:Beta", "foundation:notes", Object::Literal {
            value: "uses widget pattern".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = search::search_instances(&conn, "widget", 10).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].id, "foundation:Alpha", "comment match (score 20) must beat property match (score 10)");
}

#[test]
fn test_search_instances_iri_local_part_match() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:ProjectAlpha", "foundation:Project", "Some Project");
    create_individual(&mut conn, "foundation:ProjectBeta", "foundation:Project", "Other Project");

    let result = search::search_instances(&conn, "ProjectAlpha", 10).unwrap();
    assert!(!result.is_empty());
    assert_eq!(result[0].id, "foundation:ProjectAlpha");
}

#[test]
fn test_search_instances_matched_properties_content() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Invoice1", "rdf:type", Object::Iri("foundation:Invoice".to_string())),
        Triple::new("foundation:Invoice1", "rdfs:label", Object::Literal {
            value: "Invoice 001".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:Invoice1", "foundation:reference", Object::Literal {
            value: "REF-2024-ACME".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = search::search_instances(&conn, "acme", 10).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].matched_properties.len(), 1);
    assert_eq!(result[0].matched_properties[0]["detail_iri"], "foundation:reference");
}

// ── search (unified, with class_iri filter) ───────────────────────────────────

#[test]
fn test_search_with_class_iri_returns_only_instances_of_that_class() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:TaskA", "foundation:Task", "Task A");
    create_individual(&mut conn, "foundation:TaskB", "foundation:Task", "Task B");
    create_individual(&mut conn, "foundation:BugX", "foundation:Bug", "Bug X");

    let tokens: Vec<String> = vec![];
    let (results, _) = search::search(&conn, &tokens, None, Some("foundation:Task"), None, false, 100, 0).unwrap();

    assert_eq!(results.len(), 2, "must return only instances of foundation:Task");
    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"foundation:TaskA"));
    assert!(ids.contains(&"foundation:TaskB"));
    assert!(!ids.contains(&"foundation:BugX"), "instances of another class must not appear");
}

#[test]
fn test_search_with_class_iri_and_text_tokens_filters_both() {
    let mut conn = setup_test_db();
    create_individual(&mut conn, "foundation:TaskFoo", "foundation:Task", "Task Foo");
    create_individual(&mut conn, "foundation:TaskBar", "foundation:Task", "Task Bar");
    create_individual(&mut conn, "foundation:BugFoo", "foundation:Bug", "Bug Foo");

    let tokens = vec!["foo".to_string()];
    let (results, _) = search::search(&conn, &tokens, None, Some("foundation:Task"), None, false, 100, 0).unwrap();

    assert_eq!(results.len(), 1, "must filter by class AND by text");
    assert_eq!(results[0].id, "foundation:TaskFoo");
}

// ── property helpers ──────────────────────────────────────────────────────────

#[test]
fn test_get_all_iri_properties_returns_all_iris() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:related", Object::Iri("foundation:A".to_string())),
        Triple::new("foundation:E", "foundation:related", Object::Iri("foundation:B".to_string())),
    ], "test").unwrap();
    let result = get_all_iri_properties(&conn, "foundation:E", "foundation:related").unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&"foundation:A".to_string()));
    assert!(result.contains(&"foundation:B".to_string()));
}

#[test]
fn test_get_all_iri_properties_ignores_literals() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:tag", lit("hello")),
    ], "test").unwrap();
    let result = get_all_iri_properties(&conn, "foundation:E", "foundation:tag").unwrap();
    assert!(result.is_empty());
}

// ── get_all_property_values ───────────────────────────────────────────────────

#[test]
fn test_get_all_property_values_three_mixed_in_order() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:items", Object::Iri("foundation:A".to_string())),
        Triple::new("foundation:E", "foundation:items", Object::Iri("foundation:B".to_string())),
        Triple::new("foundation:E", "foundation:items", lit("literal-c")),
    ], "test").unwrap();
    let result = get_all_property_values(&conn, "foundation:E", "foundation:items").unwrap();
    assert_eq!(result.len(), 3);
    assert!(result.contains(&"foundation:A".to_string()));
    assert!(result.contains(&"foundation:B".to_string()));
    assert!(result.contains(&"literal-c".to_string()));
}

#[test]
fn test_get_all_property_values_single_iri() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:ref", Object::Iri("foundation:Target".to_string())),
    ], "test").unwrap();
    let result = get_all_property_values(&conn, "foundation:E", "foundation:ref").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "foundation:Target");
}

#[test]
fn test_get_all_property_values_absent_returns_empty() {
    let conn = setup_test_db();
    let result = get_all_property_values(&conn, "foundation:E", "foundation:ref").unwrap();
    assert!(result.is_empty());

    let json = serde_json::to_string(&result).unwrap();
    assert_eq!(json, "[]");
}

#[test]
fn test_get_all_property_values_prefers_iri_over_literal_coalesce() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:mixed", Object::Iri("foundation:IriVal".to_string())),
        Triple::new("foundation:E", "foundation:mixed", lit("lit-val")),
    ], "test").unwrap();
    let result = get_all_property_values(&conn, "foundation:E", "foundation:mixed").unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&"foundation:IriVal".to_string()));
    assert!(result.contains(&"lit-val".to_string()));
}

#[test]
fn test_get_literal_property_returns_value() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:name", lit("Hello")),
    ], "test").unwrap();
    let result = get_literal_property(&conn, "foundation:E", "foundation:name").unwrap();
    assert_eq!(result, Some("Hello".to_string()));
}

#[test]
fn test_get_literal_property_returns_none_when_absent() {
    let conn = setup_test_db();
    let result = get_literal_property(&conn, "foundation:E", "foundation:name").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_get_literal_property_ignores_iri_values() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:ref", Object::Iri("foundation:Other".to_string())),
    ], "test").unwrap();
    let result = get_literal_property(&conn, "foundation:E", "foundation:ref").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_get_iri_property_returns_iri() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:ref", Object::Iri("foundation:Target".to_string())),
    ], "test").unwrap();
    let result = get_iri_property(&conn, "foundation:E", "foundation:ref").unwrap();
    assert_eq!(result, Some("foundation:Target".to_string()));
}

#[test]
fn test_get_iri_property_returns_none_when_absent() {
    let conn = setup_test_db();
    let result = get_iri_property(&conn, "foundation:E", "foundation:ref").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_has_property_iri_true_when_present() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "rdf:type", Object::Iri("foundation:Task".to_string())),
    ], "test").unwrap();
    assert!(has_property_iri(&conn, "foundation:E", "rdf:type", "foundation:Task"));
}

#[test]
fn test_has_property_iri_false_when_absent() {
    let conn = setup_test_db();
    assert!(!has_property_iri(&conn, "foundation:E", "rdf:type", "foundation:Task"));
}

#[test]
fn test_has_property_literal_true_when_present() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "foundation:name", lit("Alice")),
    ], "test").unwrap();
    assert!(has_property_literal(&conn, "foundation:E", "foundation:name", "Alice"));
}

#[test]
fn test_has_property_literal_false_when_absent() {
    let conn = setup_test_db();
    assert!(!has_property_literal(&conn, "foundation:E", "foundation:name", "Alice"));
}

#[test]
fn test_is_instance_of_true() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:E", "rdf:type", Object::Iri("foundation:Task".to_string())),
    ], "test").unwrap();
    assert!(is_instance_of(&conn, "foundation:E", "foundation:Task"));
}

#[test]
fn test_is_instance_of_false() {
    let conn = setup_test_db();
    assert!(!is_instance_of(&conn, "foundation:E", "foundation:Task"));
}

#[test]
fn test_find_entities_with_property_returns_subjects() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:A", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
        Triple::new("foundation:B", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
        Triple::new("foundation:C", "foundation:hasStatus", Object::Iri("foundation:Done".to_string())),
    ], "test").unwrap();
    let mut result = find_entities_with_property(&conn, "foundation:hasStatus", "foundation:Active").unwrap();
    result.sort();
    assert_eq!(result, vec!["foundation:A".to_string(), "foundation:B".to_string()]);
}

#[test]
fn test_find_entities_with_property_empty_when_no_match() {
    let conn = setup_test_db();
    let result = find_entities_with_property(&conn, "foundation:hasStatus", "foundation:Active").unwrap();
    assert!(result.is_empty());
}

// ── validate_allowed_status ───────────────────────────────────────────────────

#[test]
fn test_validate_allowed_status_fails_when_no_statuses_configured() {
    let conn = setup_test_db();
    let result = crate::owl::individual::status::validate_allowed_status(&conn, "foundation:Task", "foundation:Active");
    assert!(result.is_err(), "concept with no allowedStatus must return an error");
}

#[test]
fn test_validate_allowed_status_passes_when_in_allowed_list() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:Active".to_string())),
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:Done".to_string())),
    ], "test").unwrap();
    crate::owl::individual::status::validate_allowed_status(&conn, "foundation:Task", "foundation:Active").unwrap();
}

#[test]
fn test_validate_allowed_status_fails_when_not_in_list() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:Active".to_string())),
    ], "test").unwrap();
    let result = crate::owl::individual::status::validate_allowed_status(&conn, "foundation:Task", "foundation:Archived");
    assert!(result.is_err());
}

// ── resolve_status_appearance ─────────────────────────────────────────────────

fn create_status(conn: &mut crate::eavto::Connection, iri: &str, label: &str, color: &str, icon: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri("foundation:Status".to_string())),
        Triple::new(iri, "rdfs:label", lit(label)),
        Triple::new(iri, "foundation:color", lit(color)),
        Triple::new(iri, "foundation:hasIcon", Object::Iri(crate::owl::icon_name_to_iri(icon))),
    ], "test").unwrap();
}

#[test]
fn test_resolve_status_appearance_direct_color_and_icon() {
    let mut conn = setup_test_db();
    create_status(&mut conn, "foundation:ActiveStatus", "Active", "#00FF00", "check");

    let (icon, color) = crate::owl::individual::status::resolve_status_appearance(&conn, "foundation:ActiveStatus");
    assert_eq!(icon, Some("check".to_string()));
    assert_eq!(color, Some("#00FF00".to_string()));
}

#[test]
fn test_resolve_status_appearance_falls_back_to_parent() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:ParentStatus", "foundation:color", lit("#0000FF")),
        Triple::new("foundation:ParentStatus", "foundation:hasIcon", Object::Iri(crate::owl::icon_name_to_iri("star"))),
    ], "test").unwrap();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:ChildStatus", "foundation:parentStatus",
            Object::Iri("foundation:ParentStatus".to_string())),
    ], "test").unwrap();

    let (icon, color) = crate::owl::individual::status::resolve_status_appearance(&conn, "foundation:ChildStatus");
    assert_eq!(icon, Some("star".to_string()));
    assert_eq!(color, Some("#0000FF".to_string()));
}

#[test]
fn test_resolve_status_appearance_returns_none_when_absent() {
    let conn = setup_test_db();
    let (icon, color) = crate::owl::individual::status::resolve_status_appearance(&conn, "foundation:Unknown");
    assert!(icon.is_none());
    assert!(color.is_none());
}

// ── get_entity_status_info ────────────────────────────────────────────────────

#[test]
fn test_get_entity_status_info_finds_status() {
    let mut conn = setup_test_db();
    create_status(&mut conn, "foundation:ActiveStatus", "Active", "#00FF00", "check");
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:MyTask", "foundation:hasStatus",
            Object::Iri("foundation:ActiveStatus".to_string())),
    ], "test").unwrap();

    let result = get_entity_status_info(&conn, "foundation:MyTask");
    assert!(result.is_some());
    let (iri, label, _color, _icon) = result.unwrap();
    assert_eq!(iri, "foundation:ActiveStatus");
    assert_eq!(label, "Active");
}

#[test]
fn test_get_entity_status_info_returns_none_when_no_status() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:MyTask", "rdf:type", Object::Iri("foundation:Task".to_string())),
    ], "test").unwrap();

    let result = get_entity_status_info(&conn, "foundation:MyTask");
    assert!(result.is_none());
}

// ── graph helpers ─────────────────────────────────────────────────────────────

#[test]
fn test_load_graph_node_groups_returns_defaults_when_no_data() {
    let conn = setup_test_db();
    let (class_group, individual_group, literal_group) = load_graph_node_groups(&conn);
    assert_eq!(class_group, 1);
    assert_eq!(individual_group, 6);
    assert_eq!(literal_group, 7);
}

#[test]
fn test_get_graph_node_type_config_empty_when_no_data() {
    let conn = setup_test_db();
    let configs = get_graph_node_type_config(&conn);
    assert!(configs.is_empty());
}

#[test]
fn test_get_graph_node_type_config_loads_entries() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:ClassNode", "rdf:type",
            Object::Iri("foundation:GraphNodeType".to_string())),
        Triple::new("foundation:ClassNode", "rdfs:label", lit("Class Node")),
        Triple::new("foundation:ClassNode", "foundation:graphGroup", lit("1")),
    ], "test").unwrap();

    let configs = get_graph_node_type_config(&conn);
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].label, "Class Node");
    assert_eq!(configs[0].group, 1);
}

#[test]
fn test_get_graph_node_type_config_sorted_by_group() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:NodeB", "rdf:type", Object::Iri("foundation:GraphNodeType".to_string())),
        Triple::new("foundation:NodeB", "rdfs:label", lit("B Node")),
        Triple::new("foundation:NodeB", "foundation:graphGroup", lit("5")),
        Triple::new("foundation:NodeA", "rdf:type", Object::Iri("foundation:GraphNodeType".to_string())),
        Triple::new("foundation:NodeA", "rdfs:label", lit("A Node")),
        Triple::new("foundation:NodeA", "foundation:graphGroup", lit("2")),
    ], "test").unwrap();

    let configs = get_graph_node_type_config(&conn);
    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].group, 2);
    assert_eq!(configs[1].group, 5);
}
