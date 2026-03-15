use super::*;
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};

async fn assert_triples(conn: &crate::eavto::Connection, triples: &[Triple]) {
    store::assert_triples(conn, triples, "test").await.expect("assert_triples failed");
}

#[tokio::test]
async fn test_replace_all_property_iris_saves_all_values() {
    let conn = setup_test_db().await;

    assert_triples(&conn, &[Triple::new(
        "foundation:TestConcept",
        "rdf:type",
        Object::Iri("owl:Class".to_string()),
    )]).await;

    for iri in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
        assert_triples(&conn, &[Triple::new(*iri, "rdf:type", Object::Iri("foundation:Status".to_string()))]).await;
    }

    replace_all_property_iris(
        &conn,
        "foundation:TestConcept",
        "foundation:allowedStatus",
        &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"],
        "test",
    ).await.unwrap();

    let mut stmt = conn.prepare(
        "SELECT COUNT(*) FROM triples \
         WHERE subject = 'foundation:TestConcept' \
           AND predicate = 'foundation:allowedStatus' \
           AND retracted = 0"
    ).await.unwrap();
    let row = stmt.query_row(()).await.unwrap();
    let active: i64 = row.get_value(0).unwrap().as_integer().copied().unwrap_or(0);
    assert_eq!(active, 3, "All three allowedStatus values must be stored");

    for status in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
        let sql = format!(
            "SELECT COUNT(*) FROM triples \
             WHERE subject = 'foundation:TestConcept' \
               AND predicate = 'foundation:allowedStatus' \
               AND object = '{}' AND retracted = 0",
            status
        );
        let mut stmt = conn.prepare(&sql).await.unwrap();
        let row = stmt.query_row(()).await.unwrap();
        let exists: i64 = row.get_value(0).unwrap().as_integer().copied().unwrap_or(0);
        assert!(exists > 0, "{status} must be stored as allowedStatus");
    }
}

#[tokio::test]
async fn test_replace_all_property_iris_replaces_existing_values() {
    let conn = setup_test_db().await;

    assert_triples(&conn, &[Triple::new(
        "foundation:TestConcept",
        "rdf:type",
        Object::Iri("owl:Class".to_string()),
    )]).await;

    for iri in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
        assert_triples(&conn, &[Triple::new(*iri, "rdf:type", Object::Iri("foundation:Status".to_string()))]).await;
    }

    replace_all_property_iris(
        &conn,
        "foundation:TestConcept",
        "foundation:allowedStatus",
        &["foundation:StatusA", "foundation:StatusB"],
        "test",
    ).await.unwrap();

    replace_all_property_iris(
        &conn,
        "foundation:TestConcept",
        "foundation:allowedStatus",
        &["foundation:StatusB", "foundation:StatusC"],
        "test",
    ).await.unwrap();

    let mut stmt = conn.prepare(
        "SELECT COUNT(*) FROM triples \
         WHERE subject = 'foundation:TestConcept' \
           AND predicate = 'foundation:allowedStatus' \
           AND retracted = 0"
    ).await.unwrap();
    let row = stmt.query_row(()).await.unwrap();
    let active: i64 = row.get_value(0).unwrap().as_integer().copied().unwrap_or(0);
    assert_eq!(active, 2, "Only the new set of values must remain");

    let mut stmt2 = conn.prepare(
        "SELECT COUNT(*) FROM triples \
         WHERE subject = 'foundation:TestConcept' \
           AND predicate = 'foundation:allowedStatus' \
           AND object = 'foundation:StatusA' AND retracted = 0"
    ).await.unwrap();
    let row2 = stmt2.query_row(()).await.unwrap();
    let status_a_active: i64 = row2.get_value(0).unwrap().as_integer().copied().unwrap_or(0);
    assert_eq!(status_a_active, 0, "StatusA must be retracted after replacement");
}

// ── search_classes ───────────────────────────────────────────────────────

async fn create_class(conn: &crate::eavto::Connection, iri: &str, label: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new(iri, "rdfs:label", Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").await.unwrap();
}

async fn create_individual(conn: &crate::eavto::Connection, iri: &str, class_iri: &str, label: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri(class_iri.to_string())),
        Triple::new(iri, "rdfs:label", Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").await.unwrap();
}

#[tokio::test]
async fn test_search_classes_empty_db() {
    let conn = setup_test_db().await;
    let result = search_classes(&conn, "task", 10).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_search_classes_finds_matching_label() {
    let conn = setup_test_db().await;
    create_class(&conn, "foundation:Task", "Task").await;
    create_class(&conn, "foundation:Project", "Project").await;

    let result = search_classes(&conn, "task", 10).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Task");
    assert!(result[0].is_class);
}

#[tokio::test]
async fn test_search_classes_case_insensitive() {
    let conn = setup_test_db().await;
    create_class(&conn, "foundation:Task", "Task").await;

    let result = search_classes(&conn, "TASK", 10).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Task");
}

#[tokio::test]
async fn test_search_classes_respects_limit() {
    let conn = setup_test_db().await;
    create_class(&conn, "foundation:TaskA", "Task Alpha").await;
    create_class(&conn, "foundation:TaskB", "Task Beta").await;
    create_class(&conn, "foundation:TaskC", "Task Gamma").await;

    let result = search_classes(&conn, "task", 2).await.unwrap();
    assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn test_search_classes_ranks_exact_match_first() {
    let conn = setup_test_db().await;
    create_class(&conn, "foundation:Task", "Task").await;
    create_class(&conn, "foundation:TaskType", "Task Type").await;

    let result = search_classes(&conn, "task", 10).await.unwrap();
    assert_eq!(result[0].id, "foundation:Task");
}

// ── search_individuals ───────────────────────────────────────────────────

#[tokio::test]
async fn test_search_individuals_empty_db() {
    let conn = setup_test_db().await;
    let result = search_individuals(&conn, "alice", 10).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_search_individuals_finds_matching_label() {
    let conn = setup_test_db().await;
    create_individual(&conn, "foundation:Alice", "foundation:Person", "Alice Smith").await;
    create_individual(&conn, "foundation:Bob", "foundation:Person", "Bob Jones").await;

    let result = search_individuals(&conn, "alice", 10).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Alice");
    assert!(!result[0].is_class);
}

#[tokio::test]
async fn test_search_individuals_case_insensitive() {
    let conn = setup_test_db().await;
    create_individual(&conn, "foundation:Alice", "foundation:Person", "Alice Smith").await;

    let result = search_individuals(&conn, "ALICE", 10).await.unwrap();
    assert_eq!(result.len(), 1);
}

#[tokio::test]
async fn test_search_individuals_excludes_owl_classes() {
    let conn = setup_test_db().await;
    create_class(&conn, "foundation:Task", "Task").await;
    create_individual(&conn, "foundation:MyTask", "foundation:Task", "Task Alpha").await;

    let result = search_individuals(&conn, "task", 10).await.unwrap();
    assert!(result.iter().all(|r| !r.is_class));
    assert!(result.iter().any(|r| r.id == "foundation:MyTask"));
}

#[tokio::test]
async fn test_search_individuals_respects_limit() {
    let conn = setup_test_db().await;
    create_individual(&conn, "foundation:P1", "foundation:Person", "Alice A").await;
    create_individual(&conn, "foundation:P2", "foundation:Person", "Alice B").await;
    create_individual(&conn, "foundation:P3", "foundation:Person", "Alice C").await;

    let result = search_individuals(&conn, "alice", 2).await.unwrap();
    assert_eq!(result.len(), 2);
}

// ── search_instances_rich ─────────────────────────────────────────────────

#[tokio::test]
async fn test_search_instances_rich_empty_query_returns_all() {
    let conn = setup_test_db().await;
    create_individual(&conn, "foundation:Alice", "foundation:Person", "Alice").await;
    create_individual(&conn, "foundation:Bob", "foundation:Person", "Bob").await;

    let result = search_instances_rich(&conn, "", 100).await.unwrap();
    assert!(result.len() >= 2);
}

#[tokio::test]
async fn test_search_instances_rich_matches_by_label() {
    let conn = setup_test_db().await;
    create_individual(&conn, "foundation:Alice", "foundation:Person", "Alice Smith").await;
    create_individual(&conn, "foundation:Bob", "foundation:Person", "Bob Jones").await;

    let result = search_instances_rich(&conn, "alice", 10).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Alice");
    assert_eq!(result[0].entity_type, "individual");
}

#[tokio::test]
async fn test_search_instances_rich_matches_by_property_value() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
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
    ], "test").await.unwrap();

    let result = search_instances_rich(&conn, "quarterly", 10).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Doc1");
    assert!(!result[0].matched_properties.is_empty());
}

#[tokio::test]
async fn test_search_instances_rich_respects_limit() {
    let conn = setup_test_db().await;
    create_individual(&conn, "foundation:A1", "foundation:Item", "Apple A").await;
    create_individual(&conn, "foundation:A2", "foundation:Item", "Apple B").await;
    create_individual(&conn, "foundation:A3", "foundation:Item", "Apple C").await;

    let result = search_instances_rich(&conn, "apple", 2).await.unwrap();
    assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn test_search_instances_rich_returns_classes() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:Vehicle", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Vehicle", "rdfs:label", Object::Literal {
            value: "Vehicle".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").await.unwrap();

    let result = search_instances_rich(&conn, "vehicle", 10).await.unwrap();
    assert!(!result.is_empty());
    let found = result.iter().find(|r| r.id == "foundation:Vehicle").unwrap();
    assert_eq!(found.entity_type, "class");
}

#[tokio::test]
async fn test_search_instances_rich_iri_match_scores_highest() {
    let conn = setup_test_db().await;
    create_individual(&conn, "foundation:Alice", "foundation:Person", "Alice").await;
    create_individual(&conn, "foundation:Bob", "foundation:Person", "Bob Alice Fan").await;

    let result = search_instances_rich(&conn, "foundation:Alice", 10).await.unwrap();
    assert!(!result.is_empty());
    assert_eq!(result[0].id, "foundation:Alice");
}

#[tokio::test]
async fn test_search_instances_rich_label_scores_higher_than_property() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
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
    ], "test").await.unwrap();

    let result = search_instances_rich(&conn, "deploy", 10).await.unwrap();
    assert!(result.len() >= 2);
    assert_eq!(result[0].id, "foundation:TaskA");
}

#[tokio::test]
async fn test_search_instances_rich_label_exact_beats_starts_with_beats_contains() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
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
    ], "test").await.unwrap();

    let result = search_instances_rich(&conn, "rust", 10).await.unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].id, "foundation:E1", "exact match must be first");
    assert_eq!(result[1].id, "foundation:E2", "starts_with must be second");
    assert_eq!(result[2].id, "foundation:E3", "contains must be last");
}

#[tokio::test]
async fn test_search_instances_rich_comment_match_works() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
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
    ], "test").await.unwrap();

    let result = search_instances_rich(&conn, "dashboard", 10).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "foundation:Widget");
    let comment_prop = result[0].matched_properties.iter()
        .find(|p| p["detail_iri"] == "rdfs:comment");
    assert!(comment_prop.is_some(), "rdfs:comment must appear in matched_properties");
}

#[tokio::test]
async fn test_search_instances_rich_comment_beats_property() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
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
    ], "test").await.unwrap();

    let result = search_instances_rich(&conn, "widget", 10).await.unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].id, "foundation:Alpha", "comment match (score 20) must beat property match (score 10)");
}

#[tokio::test]
async fn test_search_instances_rich_iri_local_part_match() {
    let conn = setup_test_db().await;
    create_individual(&conn, "foundation:ProjectAlpha", "foundation:Project", "Some Project").await;
    create_individual(&conn, "foundation:ProjectBeta", "foundation:Project", "Other Project").await;

    let result = search_instances_rich(&conn, "ProjectAlpha", 10).await.unwrap();
    assert!(!result.is_empty());
    assert_eq!(result[0].id, "foundation:ProjectAlpha");
}

#[tokio::test]
async fn test_search_instances_rich_matched_properties_content() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
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
    ], "test").await.unwrap();

    let result = search_instances_rich(&conn, "acme", 10).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].matched_properties.len(), 1);
    assert_eq!(result[0].matched_properties[0]["detail_iri"], "foundation:reference");
}

// ── property helpers ─────────────────────────────────────────────────────

fn lit(value: &str) -> Object {
    Object::Literal { value: value.to_string(), datatype: Some("xsd:string".to_string()), language: None }
}

#[tokio::test]
async fn test_get_all_iri_properties_returns_all_iris() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:E", "foundation:related", Object::Iri("foundation:A".to_string())),
        Triple::new("foundation:E", "foundation:related", Object::Iri("foundation:B".to_string())),
    ], "test").await.unwrap();
    let result = get_all_iri_properties(&conn, "foundation:E", "foundation:related").await.unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&"foundation:A".to_string()));
    assert!(result.contains(&"foundation:B".to_string()));
}

#[tokio::test]
async fn test_get_all_iri_properties_ignores_literals() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:E", "foundation:tag", lit("hello")),
    ], "test").await.unwrap();
    let result = get_all_iri_properties(&conn, "foundation:E", "foundation:tag").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_literal_property_returns_value() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:E", "foundation:name", lit("Hello")),
    ], "test").await.unwrap();
    let result = get_literal_property(&conn, "foundation:E", "foundation:name").await.unwrap();
    assert_eq!(result, Some("Hello".to_string()));
}

#[tokio::test]
async fn test_get_literal_property_returns_none_when_absent() {
    let conn = setup_test_db().await;
    let result = get_literal_property(&conn, "foundation:E", "foundation:name").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_literal_property_ignores_iri_values() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:E", "foundation:ref", Object::Iri("foundation:Other".to_string())),
    ], "test").await.unwrap();
    let result = get_literal_property(&conn, "foundation:E", "foundation:ref").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_iri_property_returns_iri() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:E", "foundation:ref", Object::Iri("foundation:Target".to_string())),
    ], "test").await.unwrap();
    let result = get_iri_property(&conn, "foundation:E", "foundation:ref").await.unwrap();
    assert_eq!(result, Some("foundation:Target".to_string()));
}

#[tokio::test]
async fn test_get_iri_property_returns_none_when_absent() {
    let conn = setup_test_db().await;
    let result = get_iri_property(&conn, "foundation:E", "foundation:ref").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_has_property_iri_true_when_present() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:E", "rdf:type", Object::Iri("foundation:Task".to_string())),
    ], "test").await.unwrap();
    assert!(has_property_iri(&conn, "foundation:E", "rdf:type", "foundation:Task").await);
}

#[tokio::test]
async fn test_has_property_iri_false_when_absent() {
    let conn = setup_test_db().await;
    assert!(!has_property_iri(&conn, "foundation:E", "rdf:type", "foundation:Task").await);
}

#[tokio::test]
async fn test_has_property_literal_true_when_present() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:E", "foundation:name", lit("Alice")),
    ], "test").await.unwrap();
    assert!(has_property_literal(&conn, "foundation:E", "foundation:name", "Alice").await);
}

#[tokio::test]
async fn test_has_property_literal_false_when_absent() {
    let conn = setup_test_db().await;
    assert!(!has_property_literal(&conn, "foundation:E", "foundation:name", "Alice").await);
}

#[tokio::test]
async fn test_is_instance_of_true() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:E", "rdf:type", Object::Iri("foundation:Task".to_string())),
    ], "test").await.unwrap();
    assert!(is_instance_of(&conn, "foundation:E", "foundation:Task").await);
}

#[tokio::test]
async fn test_is_instance_of_false() {
    let conn = setup_test_db().await;
    assert!(!is_instance_of(&conn, "foundation:E", "foundation:Task").await);
}

#[tokio::test]
async fn test_find_entities_with_property_returns_subjects() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:A", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
        Triple::new("foundation:B", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
        Triple::new("foundation:C", "foundation:hasStatus", Object::Iri("foundation:Done".to_string())),
    ], "test").await.unwrap();
    let mut result = find_entities_with_property(&conn, "foundation:hasStatus", "foundation:Active").await.unwrap();
    result.sort();
    assert_eq!(result, vec!["foundation:A".to_string(), "foundation:B".to_string()]);
}

#[tokio::test]
async fn test_find_entities_with_property_empty_when_no_match() {
    let conn = setup_test_db().await;
    let result = find_entities_with_property(&conn, "foundation:hasStatus", "foundation:Active").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_validate_allowed_status_passes_when_no_restriction() {
    let conn = setup_test_db().await;
    validate_allowed_status(&conn, "foundation:Task", "foundation:Active").await.unwrap();
}

#[tokio::test]
async fn test_validate_allowed_status_passes_when_in_allowed_list() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:Active".to_string())),
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:Done".to_string())),
    ], "test").await.unwrap();
    validate_allowed_status(&conn, "foundation:Task", "foundation:Active").await.unwrap();
}

#[tokio::test]
async fn test_validate_allowed_status_fails_when_not_in_list() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:Active".to_string())),
    ], "test").await.unwrap();
    let result = validate_allowed_status(&conn, "foundation:Task", "foundation:Archived").await;
    assert!(result.is_err());
}

// ── status helpers ────────────────────────────────────────────────────────

async fn create_status(conn: &crate::eavto::Connection, iri: &str, label: &str, color: &str, icon: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri("foundation:Status".to_string())),
        Triple::new(iri, "rdfs:label", lit(label)),
        Triple::new(iri, "foundation:color", lit(color)),
        Triple::new(iri, "foundation:icon", lit(icon)),
    ], "test").await.unwrap();
}

#[tokio::test]
async fn test_resolve_status_appearance_direct_color_and_icon() {
    let conn = setup_test_db().await;
    create_status(&conn, "foundation:ActiveStatus", "Active", "#00FF00", "check").await;

    let (icon, color) = resolve_status_appearance(&conn, "foundation:ActiveStatus").await;
    assert_eq!(icon, Some("check".to_string()));
    assert_eq!(color, Some("#00FF00".to_string()));
}

#[tokio::test]
async fn test_resolve_status_appearance_falls_back_to_parent() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:ParentStatus", "foundation:color", lit("#0000FF")),
        Triple::new("foundation:ParentStatus", "foundation:icon", lit("star")),
    ], "test").await.unwrap();
    store::assert_triples(&conn, &[
        Triple::new("foundation:ChildStatus", "foundation:parentStatus",
            Object::Iri("foundation:ParentStatus".to_string())),
    ], "test").await.unwrap();

    let (icon, color) = resolve_status_appearance(&conn, "foundation:ChildStatus").await;
    assert_eq!(icon, Some("star".to_string()));
    assert_eq!(color, Some("#0000FF".to_string()));
}

#[tokio::test]
async fn test_resolve_status_appearance_returns_none_when_absent() {
    let conn = setup_test_db().await;
    let (icon, color) = resolve_status_appearance(&conn, "foundation:Unknown").await;
    assert!(icon.is_none());
    assert!(color.is_none());
}

#[tokio::test]
async fn test_get_entity_status_info_finds_status() {
    let conn = setup_test_db().await;
    create_status(&conn, "foundation:ActiveStatus", "Active", "#00FF00", "check").await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:MyTask", "foundation:hasStatus",
            Object::Iri("foundation:ActiveStatus".to_string())),
    ], "test").await.unwrap();

    let result = get_entity_status_info(&conn, "foundation:MyTask").await;
    assert!(result.is_some());
    let (iri, label, _color, _icon) = result.unwrap();
    assert_eq!(iri, "foundation:ActiveStatus");
    assert_eq!(label, "Active");
}

#[tokio::test]
async fn test_get_entity_status_info_returns_none_when_no_status() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:MyTask", "rdf:type", Object::Iri("foundation:Task".to_string())),
    ], "test").await.unwrap();

    let result = get_entity_status_info(&conn, "foundation:MyTask").await;
    assert!(result.is_none());
}

// ── graph helpers ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_load_graph_node_groups_returns_defaults_when_no_data() {
    let conn = setup_test_db().await;
    let (class_group, individual_group, literal_group) = load_graph_node_groups(&conn).await;
    assert_eq!(class_group, 1);
    assert_eq!(individual_group, 6);
    assert_eq!(literal_group, 7);
}

#[tokio::test]
async fn test_get_graph_node_type_config_empty_when_no_data() {
    let conn = setup_test_db().await;
    let configs = get_graph_node_type_config(&conn).await;
    assert!(configs.is_empty());
}

#[tokio::test]
async fn test_get_graph_node_type_config_loads_entries() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:ClassNode", "rdf:type",
            Object::Iri("foundation:GraphNodeType".to_string())),
        Triple::new("foundation:ClassNode", "rdfs:label", lit("Class Node")),
        Triple::new("foundation:ClassNode", "foundation:graphGroup", lit("1")),
    ], "test").await.unwrap();

    let configs = get_graph_node_type_config(&conn).await;
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].label, "Class Node");
    assert_eq!(configs[0].group, 1);
}

#[tokio::test]
async fn test_get_graph_node_type_config_sorted_by_group() {
    let conn = setup_test_db().await;
    store::assert_triples(&conn, &[
        Triple::new("foundation:NodeB", "rdf:type", Object::Iri("foundation:GraphNodeType".to_string())),
        Triple::new("foundation:NodeB", "rdfs:label", lit("B Node")),
        Triple::new("foundation:NodeB", "foundation:graphGroup", lit("5")),
        Triple::new("foundation:NodeA", "rdf:type", Object::Iri("foundation:GraphNodeType".to_string())),
        Triple::new("foundation:NodeA", "rdfs:label", lit("A Node")),
        Triple::new("foundation:NodeA", "foundation:graphGroup", lit("2")),
    ], "test").await.unwrap();

    let configs = get_graph_node_type_config(&conn).await;
    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].group, 2);
    assert_eq!(configs[1].group, 5);
}
