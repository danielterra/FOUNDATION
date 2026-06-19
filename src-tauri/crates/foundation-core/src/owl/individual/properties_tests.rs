use super::{
    find_entities_with_property_bounded, find_entities_with_property_keyset,
    find_entities_with_predicate, is_subclass_of, replace_all_property_literals,
    get_all_current_triples,
};
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};
use crate::owl::vocabulary::rdfs;

// ── helpers ───────────────────────────────────────────────────────────────────

fn seed_type(conn: &mut rusqlite::Connection, iri: &str, class: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri(class.to_string())),
    ], "test").unwrap();
}

fn seed_label(conn: &mut rusqlite::Connection, iri: &str, label: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdfs:label", Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();
}

// ── find_entities_with_property_bounded ──────────────────────────────────────

#[test]
fn test_find_entities_bounded_limit_applied_in_sql() {
    let mut conn = setup_test_db();
    for i in 0..5 {
        seed_type(&mut conn, &format!("test:E{}", i), "test:Widget");
    }
    let rows = find_entities_with_property_bounded(&conn, "rdf:type", "test:Widget", 3, 0, None)
        .unwrap();
    assert_eq!(rows.len(), 3, "limit deve ser aplicado em SQL");
}

#[test]
fn test_find_entities_bounded_has_more_semantics() {
    let mut conn = setup_test_db();
    for i in 0..4 {
        seed_type(&mut conn, &format!("test:F{}", i), "test:Widget");
    }
    let page1 = find_entities_with_property_bounded(&conn, "rdf:type", "test:Widget", 2, 0, None)
        .unwrap();
    let page2 = find_entities_with_property_bounded(&conn, "rdf:type", "test:Widget", 2, 2, None)
        .unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 2);
    // sem sobreposição
    assert!(!page1.iter().any(|x| page2.contains(x)));
}

#[test]
fn test_find_entities_bounded_empty_page() {
    let conn = setup_test_db();
    let rows = find_entities_with_property_bounded(&conn, "rdf:type", "test:Nonexistent", 10, 0, None)
        .unwrap();
    assert!(rows.is_empty());
}

#[test]
fn test_find_entities_bounded_order_by_subject_when_no_predicate() {
    let mut conn = setup_test_db();
    for label in &["Z", "A", "M"] {
        let iri = format!("test:{}", label);
        seed_type(&mut conn, &iri, "test:Thing");
    }
    let rows = find_entities_with_property_bounded(&conn, "rdf:type", "test:Thing", 10, 0, None)
        .unwrap();
    let labels: Vec<_> = rows.iter().map(|r| r.as_str()).collect();
    let mut sorted = labels.clone();
    sorted.sort();
    assert_eq!(labels, sorted, "sem order_predicate deve ordenar por subject ASC");
}

#[test]
fn test_find_entities_bounded_order_by_predicate() {
    let mut conn = setup_test_db();
    for (iri, label) in &[("test:P1", "Zulu"), ("test:P2", "Alpha"), ("test:P3", "Mike")] {
        seed_type(&mut conn, iri, "test:Product");
        seed_label(&mut conn, iri, label);
    }
    let rows = find_entities_with_property_bounded(
        &conn, "rdf:type", "test:Product", 10, 0, Some("rdfs:label"),
    ).unwrap();
    // order_predicate DESC (object_value DESC) — "Zulu" > "Mike" > "Alpha"
    assert_eq!(rows[0], "test:P1", "Zulu deve vir primeiro em DESC");
    assert_eq!(rows[1], "test:P3");
    assert_eq!(rows[2], "test:P2");
}

// ── find_entities_with_property_keyset ───────────────────────────────────────

#[test]
fn test_find_entities_keyset_cursor_advances_without_repeat_or_skip() {
    let mut conn = setup_test_db();
    for i in 0..6 {
        seed_type(&mut conn, &format!("test:K{}", i), "test:Item");
    }

    let page1 = find_entities_with_property_keyset(&conn, "rdf:type", "test:Item", None, 3)
        .unwrap();
    assert_eq!(page1.len(), 3);
    let cursor = page1.last().unwrap().1;

    let page2 = find_entities_with_property_keyset(&conn, "rdf:type", "test:Item", Some(cursor), 3)
        .unwrap();
    assert_eq!(page2.len(), 3);

    let p1_subjects: Vec<_> = page1.iter().map(|(s, _)| s.clone()).collect();
    let p2_subjects: Vec<_> = page2.iter().map(|(s, _)| s.clone()).collect();
    for s in &p2_subjects {
        assert!(!p1_subjects.contains(s), "sem repetição entre páginas");
    }
}

#[test]
fn test_find_entities_keyset_stability_under_insertion_after_cursor() {
    let mut conn = setup_test_db();
    for i in 0..4 {
        seed_type(&mut conn, &format!("test:S{}", i), "test:Item");
    }

    let page1 = find_entities_with_property_keyset(&conn, "rdf:type", "test:Item", None, 2)
        .unwrap();
    let cursor = page1.last().unwrap().1;

    // Nova tripla inserida APÓS o cursor — não deve entrar na página 2
    seed_type(&mut conn, "test:SNovo", "test:Item");

    let page2 = find_entities_with_property_keyset(&conn, "rdf:type", "test:Item", Some(cursor), 10)
        .unwrap();
    let p2_subjects: Vec<_> = page2.iter().map(|(s, _)| s.clone()).collect();
    assert!(!p2_subjects.contains(&"test:SNovo".to_string()),
        "entidade inserida após cursor não deve aparecer na página 2");
}

// ── find_entities_with_predicate ─────────────────────────────────────────────

#[test]
fn test_find_entities_with_predicate_returns_subjects() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:A", "test:myPred", Object::Literal {
            value: "v1".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("test:B", "test:myPred", Object::Literal {
            value: "v2".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("test:C", "test:otherPred", Object::Literal {
            value: "v3".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let results = find_entities_with_predicate(&conn, "test:myPred").unwrap();
    assert!(results.contains(&"test:A".to_string()));
    assert!(results.contains(&"test:B".to_string()));
    assert!(!results.contains(&"test:C".to_string()));
}

#[test]
fn test_find_entities_with_predicate_deduplicates() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:A", "test:tag", Object::Literal {
            value: "x".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("test:A", "test:tag", Object::Literal {
            value: "y".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let results = find_entities_with_predicate(&conn, "test:tag").unwrap();
    let count = results.iter().filter(|s| s.as_str() == "test:A").count();
    assert_eq!(count, 1, "must deduplicate subjects with multiple values");
}

#[test]
fn test_find_entities_with_predicate_empty_when_no_match() {
    let conn = setup_test_db();
    let results = find_entities_with_predicate(&conn, "test:ghost").unwrap();
    assert!(results.is_empty());
}

// ── is_subclass_of ────────────────────────────────────────────────────────────

#[test]
fn test_is_subclass_of_direct_parent() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Dog", rdfs::SUB_CLASS_OF, Object::Iri("test:Animal".to_string())),
    ], "test").unwrap();

    assert!(is_subclass_of(&conn, "test:Dog", "test:Animal"));
}

#[test]
fn test_is_subclass_of_transitive() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Poodle", rdfs::SUB_CLASS_OF, Object::Iri("test:Dog".to_string())),
        Triple::new("test:Dog", rdfs::SUB_CLASS_OF, Object::Iri("test:Animal".to_string())),
    ], "test").unwrap();

    assert!(is_subclass_of(&conn, "test:Poodle", "test:Animal"),
        "transitive chain Poodle→Dog→Animal must resolve");
}

#[test]
fn test_is_subclass_of_self_is_true() {
    let conn = setup_test_db();
    assert!(is_subclass_of(&conn, "test:Any", "test:Any"), "class is subclass of itself");
}

#[test]
fn test_is_subclass_of_false_when_not_related() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Cat", rdfs::SUB_CLASS_OF, Object::Iri("test:Animal".to_string())),
    ], "test").unwrap();

    assert!(!is_subclass_of(&conn, "test:Cat", "test:Dog"),
        "Cat is not a subclass of Dog");
}

// ── replace_all_property_literals ────────────────────────────────────────────

#[test]
fn test_replace_all_property_literals_replaces_set() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Ent", "test:tag", Object::Literal {
            value: "old1".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("test:Ent", "test:tag", Object::Literal {
            value: "old2".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    replace_all_property_literals(
        &mut conn, "test:Ent", "test:tag", &["new1", "new2", "new3"], "test",
    ).unwrap();

    let conn_ref: &rusqlite::Connection = &conn;
    let count: i64 = conn_ref.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject='test:Ent' AND predicate='test:tag'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(count, 3, "must have exactly the new 3 values");
}

#[test]
fn test_replace_all_property_literals_immutability_old_values_remain_retracted() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Ent", "test:tag", Object::Literal {
            value: "old".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    replace_all_property_literals(
        &mut conn, "test:Ent", "test:tag", &["new"], "test",
    ).unwrap();

    let conn_ref: &rusqlite::Connection = &conn;
    let old_count: i64 = conn_ref.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='test:Ent' AND predicate='test:tag' AND object_value='old'",
        [], |r| r.get(0),
    ).unwrap();
    assert!(old_count >= 1, "old value row must still exist in history (immutable store)");

    let current_old: i64 = conn_ref.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject='test:Ent' AND predicate='test:tag' AND object_value='old'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(current_old, 0, "old value must not appear in current view");
}

#[test]
fn test_replace_all_property_literals_empty_clears_existing() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Ent", "test:tag", Object::Literal {
            value: "v".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    replace_all_property_literals(&mut conn, "test:Ent", "test:tag", &[], "test").unwrap();

    let conn_ref: &rusqlite::Connection = &conn;
    let count: i64 = conn_ref.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject='test:Ent' AND predicate='test:tag'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(count, 0, "clearing with empty list must remove all current values");
}

// ── get_all_current_triples ───────────────────────────────────────────────────

#[test]
fn test_get_all_current_triples_returns_active_triples() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:X", "rdf:type", Object::Iri("test:Cls".to_string())),
        Triple::new("test:X", "rdfs:label", Object::Literal {
            value: "X".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let triples = get_all_current_triples(&conn, "test:X").unwrap();
    assert_eq!(triples.len(), 2);
    assert!(triples.iter().any(|t| t.predicate == "rdf:type"));
    assert!(triples.iter().any(|t| t.predicate == "rdfs:label"));
}

#[test]
fn test_get_all_current_triples_empty_for_nonexistent() {
    let conn = setup_test_db();
    let triples = get_all_current_triples(&conn, "test:Ghost").unwrap();
    assert!(triples.is_empty());
}

#[test]
fn test_get_all_current_triples_excludes_retracted() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Y", "rdf:type", Object::Iri("test:Cls".to_string())),
        Triple::new("test:Y", "test:prop", Object::Literal {
            value: "old".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();
    store::retract_triples(&mut conn, &[
        Triple::new("test:Y", "test:prop", Object::Literal {
            value: "old".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let triples = get_all_current_triples(&conn, "test:Y").unwrap();
    assert!(!triples.iter().any(|t| t.predicate == "test:prop"),
        "retracted triple must not appear in current triples");
}
