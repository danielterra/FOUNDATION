use super::*;
use crate::eavto::query;
use crate::ai::functions::{ToolCall, execute_tool};

fn setup_person_class(conn: &mut Connection) {
    let class = Class::new("foundation:Person");
    class.assert(conn, ClassType::OwlClass, "Person", "https://example.com/person.svg", None, "test").unwrap();
    Property::new("foundation:nick")
        .assert(conn, PropertyType::DatatypeProperty, "nick", None, &["foundation:Person"], Some("xsd:string"), None, "test")
        .unwrap();
    Property::new("foundation:role")
        .assert(conn, PropertyType::DatatypeProperty, "role", None, &["foundation:Person"], Some("xsd:string"), None, "test")
        .unwrap();
}

fn retract_op(name: &str, iri: &str) -> ToolCall {
    ToolCall {
        name: name.to_string(),
        arguments: serde_json::json!({ "operations": [{ "iri": iri }] }),
    }
}

fn restore_op(name: &str, iri: &str) -> ToolCall {
    ToolCall {
        name: name.to_string(),
        arguments: serde_json::json!({ "operations": [{ "iri": iri }] }),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// TestCase_1775309506892 — restore_individual restores all property values
// ────────────────────────────────────────────────────────────────────────────
#[test]
fn restore_individual_restores_all_property_values() {
    let mut conn = setup_test_db();
    setup_person_class(&mut conn);

    // Single assert_triples call → all triples share one tx
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "rdf:type",   Object::Iri("foundation:Person".to_string())),
        Triple::new("foundation:Alice", "rdfs:label", Object::Literal { value: "Alice".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        Triple::new("foundation:Alice", "foundation:hasIcon", Object::Literal { value: "https://example.com/person.svg".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        Triple::new("foundation:Alice", "foundation:nick", Object::Literal { value: "ali".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        Triple::new("foundation:Alice", "foundation:role", Object::Literal { value: "admin".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
    ], "test").unwrap();

    Individual::retract(&mut conn, "foundation:Alice", "test").unwrap();
    assert!(Individual::get(&conn, "foundation:Alice").unwrap().is_none(), "should be gone after retract");

    Individual::restore(&mut conn, "foundation:Alice", "test").unwrap();

    let ind = Individual::get(&conn, "foundation:Alice").unwrap()
        .expect("individual must reappear after restore");
    assert_eq!(ind.label.as_deref(), Some("Alice"), "label must be restored");

    let nick = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:nick").unwrap();
    assert!(!nick.triples.is_empty(), "foundation:nick must be restored");
    assert_eq!(nick.triples[0].object.as_literal(), Some("ali".to_string()));

    let role = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:role").unwrap();
    assert!(!role.triples.is_empty(), "foundation:role must be restored");
    assert_eq!(role.triples[0].object.as_literal(), Some("admin".to_string()));
}

// ────────────────────────────────────────────────────────────────────────────
// TestCase_1775309507679 — restore_individual on non-retracted IRI returns error
// ────────────────────────────────────────────────────────────────────────────
#[test]
fn restore_individual_on_non_retracted_iri_returns_error() {
    let mut conn = setup_test_db();
    setup_person_class(&mut conn);

    Individual::new("foundation:Bob")
        .assert(&mut conn, "foundation:Person", "Bob", "https://example.com/person.svg", "test")
        .unwrap();

    let result = Individual::restore(&mut conn, "foundation:Bob", "test");
    assert!(result.is_err(), "restore on a live individual must return an error");
}

// ────────────────────────────────────────────────────────────────────────────
// TestCase_1775309508523 — restore_class restores class and cascade-deleted instances
// ────────────────────────────────────────────────────────────────────────────
#[test]
fn restore_class_restores_class_and_cascade_deleted_instances() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Widget", "rdf:type",      Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Widget", "rdfs:label",    Object::Literal { value: "Widget".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        Triple::new("foundation:Widget", "foundation:hasIcon", Object::Literal { value: "https://example.com/widget.svg".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
    ], "test").unwrap();

    for name in ["A", "B", "C"] {
        let iri = format!("foundation:Widget_{}", name);
        store::assert_triples(&mut conn, &[
            Triple::new(&iri, "rdf:type",   Object::Iri("foundation:Widget".to_string())),
            Triple::new(&iri, "rdfs:label", Object::Literal { value: name.to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        ], "test").unwrap();
    }

    let r = execute_tool(&mut conn, &retract_op("retract_class", "foundation:Widget"), None, None);
    assert!(r.success, "retract_class should succeed: {:?}", r.error);

    assert!(Class::get(&conn, "foundation:Widget").unwrap().is_none(), "class should be gone");
    for name in ["A", "B", "C"] {
        let iri = format!("foundation:Widget_{}", name);
        assert!(Individual::get(&conn, &iri).unwrap().is_none(), "instance {} should be gone", name);
    }

    let r = execute_tool(&mut conn, &restore_op("restore_class", "foundation:Widget"), None, None);
    assert!(r.success, "restore_class should succeed: {:?}", r.error);

    assert!(Class::get(&conn, "foundation:Widget").unwrap().is_some(), "class must reappear");
    for name in ["A", "B", "C"] {
        let iri = format!("foundation:Widget_{}", name);
        assert!(Individual::get(&conn, &iri).unwrap().is_some(), "instance {} must reappear", name);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// TestCase_1775309509355 — restore_class does not restore independently-retracted instances
// ────────────────────────────────────────────────────────────────────────────
#[test]
fn restore_class_does_not_restore_independently_retracted_instances() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Gadget", "rdf:type",   Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Gadget", "rdfs:label", Object::Literal { value: "Gadget".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        Triple::new("foundation:Gadget", "foundation:hasIcon", Object::Literal { value: "https://example.com/gadget.svg".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
    ], "test").unwrap();

    for name in ["X", "Y", "Z"] {
        let iri = format!("foundation:Gadget_{}", name);
        store::assert_triples(&mut conn, &[
            Triple::new(&iri, "rdf:type",   Object::Iri("foundation:Gadget".to_string())),
            Triple::new(&iri, "rdfs:label", Object::Literal { value: name.to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        ], "test").unwrap();
    }

    // Independently retract Gadget_X before retracting the class
    Individual::retract(&mut conn, "foundation:Gadget_X", "test").unwrap();

    // Retract the class (cascade-deletes Y and Z, but not X which was already gone)
    let r = execute_tool(&mut conn, &retract_op("retract_class", "foundation:Gadget"), None, None);
    assert!(r.success, "retract_class should succeed: {:?}", r.error);

    // Restore the class
    let r = execute_tool(&mut conn, &restore_op("restore_class", "foundation:Gadget"), None, None);
    assert!(r.success, "restore_class should succeed: {:?}", r.error);

    assert!(Class::get(&conn, "foundation:Gadget").unwrap().is_some(), "class must reappear");
    assert!(Individual::get(&conn, "foundation:Gadget_Y").unwrap().is_some(), "Y must reappear");
    assert!(Individual::get(&conn, "foundation:Gadget_Z").unwrap().is_some(), "Z must reappear");

    // X was retracted before the class → tx < class_retract_tx → must remain retracted
    assert!(
        Individual::get(&conn, "foundation:Gadget_X").unwrap().is_none(),
        "X was independently retracted before class retraction and must remain retracted"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// TestCase_1775309510202 — restore_property restores definition and all asserted facts
// ────────────────────────────────────────────────────────────────────────────
#[test]
fn restore_property_restores_definition_and_all_asserted_facts() {
    let mut conn = setup_test_db();

    let class = Class::new("foundation:Thing");
    class.assert(&mut conn, ClassType::OwlClass, "Thing", "https://example.com/thing.svg", None, "test").unwrap();

    let prop = Property::new("foundation:grade");
    prop.assert(&mut conn, PropertyType::DatatypeProperty, "grade", None, &["foundation:Thing"], Some("xsd:string"), None, "test").unwrap();

    // Assert the property on two individuals in separate txs
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:T1", "rdf:type",      Object::Iri("foundation:Thing".to_string())),
        Triple::new("foundation:T1", "rdfs:label",    Object::Literal { value: "T1".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        Triple::new("foundation:T1", "foundation:grade", Object::Literal { value: "B".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
    ], "test").unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:T2", "rdf:type",      Object::Iri("foundation:Thing".to_string())),
        Triple::new("foundation:T2", "rdfs:label",    Object::Literal { value: "T2".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
        Triple::new("foundation:T2", "foundation:grade", Object::Literal { value: "C".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
    ], "test").unwrap();

    // Update T1's grade (retracts "B", inserts "A" at a higher tx)
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:T1", "foundation:grade", Object::Literal { value: "A".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
    ], "test").unwrap();

    let before = query::get_by_entity_predicate(&conn, "foundation:T1", "foundation:grade").unwrap();
    assert_eq!(before.triples[0].object.as_literal(), Some("A".to_string()), "T1 grade should be A before retraction");

    // Retract the property (retracts definition + current facts)
    let r = execute_tool(&mut conn, &retract_op("retract_property", "foundation:grade"), None, None);
    assert!(r.success, "retract_property should succeed: {:?}", r.error);

    assert!(Property::get(&conn, "foundation:grade").unwrap().is_none(), "property should be gone");
    assert!(query::get_by_entity_predicate(&conn, "foundation:T1", "foundation:grade").unwrap().triples.is_empty(), "T1 grade should be gone");
    assert!(query::get_by_entity_predicate(&conn, "foundation:T2", "foundation:grade").unwrap().triples.is_empty(), "T2 grade should be gone");

    // Restore the property
    let r = execute_tool(&mut conn, &restore_op("restore_property", "foundation:grade"), None, None);
    assert!(r.success, "restore_property should succeed: {:?}", r.error);

    // Property definition must be back
    assert!(Property::get(&conn, "foundation:grade").unwrap().is_some(), "property definition must be restored");

    // T2 grade must be restored (C, never updated)
    let t2 = query::get_by_entity_predicate(&conn, "foundation:T2", "foundation:grade").unwrap();
    assert!(!t2.triples.is_empty(), "T2 grade must be restored");
    assert_eq!(t2.triples[0].object.as_literal(), Some("C".to_string()), "T2 grade must be C");

    // T1 grade must be restored to the last active value (A), NOT the superseded value (B)
    let t1 = query::get_by_entity_predicate(&conn, "foundation:T1", "foundation:grade").unwrap();
    assert!(!t1.triples.is_empty(), "T1 grade must be restored");
    assert_eq!(t1.triples[0].object.as_literal(), Some("A".to_string()), "T1 grade must be A (last active), not B (superseded)");
    assert_eq!(t1.triples.len(), 1, "only one grade for T1 — superseded B must remain retracted");
}
