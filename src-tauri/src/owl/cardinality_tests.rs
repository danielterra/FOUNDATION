use super::*;

#[test]
fn test_exact_cardinality() {
    let restriction = CardinalityRestriction {
        property_iri: "foundation:name".to_string(),
        min: None,
        max: None,
        exact: Some(1),
    };

    assert!(!restriction.is_violated(1)); // OK
    assert!(restriction.is_violated(0));  // Too few
    assert!(restriction.is_violated(2));  // Too many
}

#[test]
fn test_min_cardinality() {
    let restriction = CardinalityRestriction {
        property_iri: "foundation:email".to_string(),
        min: Some(1),
        max: None,
        exact: None,
    };

    assert!(restriction.is_violated(0));  // Too few
    assert!(!restriction.is_violated(1)); // OK
    assert!(!restriction.is_violated(5)); // OK (no max)
}

#[test]
fn test_max_cardinality() {
    let restriction = CardinalityRestriction {
        property_iri: "foundation:phone".to_string(),
        min: None,
        max: Some(3),
        exact: None,
    };

    assert!(!restriction.is_violated(0)); // OK (no min)
    assert!(!restriction.is_violated(3)); // OK
    assert!(restriction.is_violated(4));  // Too many
}

#[test]
fn test_min_max_cardinality() {
    let restriction = CardinalityRestriction {
        property_iri: "foundation:hasPhoneNumber".to_string(),
        min: Some(0),
        max: Some(2),
        exact: None,
    };

    assert!(!restriction.is_violated(0)); // OK
    assert!(!restriction.is_violated(1)); // OK
    assert!(!restriction.is_violated(2)); // OK
    assert!(restriction.is_violated(3));  // Too many
}

#[test]
fn test_set_class_required_fields() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    // Set up class
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();

    // Mark two properties as required
    set_class_required_fields(
        &mut conn,
        "foundation:TestClass",
        &["foundation:name", "foundation:email"],
        "test",
    ).unwrap();

    // Verify restrictions exist
    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
    assert_eq!(restrictions.len(), 2);
    let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();
    assert!(props.contains(&"foundation:name"));
    assert!(props.contains(&"foundation:email"));
    for r in &restrictions {
        assert_eq!(r.min, Some(1));
    }

    // Replace with just one required field
    set_class_required_fields(
        &mut conn,
        "foundation:TestClass",
        &["foundation:name"],
        "test",
    ).unwrap();

    let restrictions2 = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
    assert_eq!(restrictions2.len(), 1);
    assert_eq!(restrictions2[0].property_iri, "foundation:name");

    // Clear all required fields
    set_class_required_fields(&mut conn, "foundation:TestClass", &[], "test").unwrap();
    let restrictions3 = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
    assert_eq!(restrictions3.len(), 0);
}

#[test]
fn test_set_required_fields_preserves_parent_class_subclass_link() {
    // Regression for Bug_1772765777624: set_class_required_fields must not retract
    // the real rdfs:subClassOf link to the parent class when inserting blank node
    // restriction links that share the same (subject, predicate).
    use crate::eavto::{store, query, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:ParentClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:ChildClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:ChildClass", "rdfs:subClassOf", Object::Iri("foundation:ParentClass".to_string())),
        Triple::new("foundation:childProp", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
        Triple::new("foundation:childProp", "rdfs:domain", Object::Iri("foundation:ChildClass".to_string())),
    ], "test").unwrap();

    set_class_required_fields(&mut conn, "foundation:ChildClass", &["foundation:childProp"], "test").unwrap();

    let subclass_result = query::get_by_entity_predicate(
        &conn, "foundation:ChildClass", "rdfs:subClassOf",
    ).unwrap();
    let real_parent_links: Vec<&str> = subclass_result.triples.iter()
        .filter_map(|t| t.object.as_iri())
        .filter(|iri| !iri.starts_with("_:"))
        .collect();

    assert!(
        real_parent_links.contains(&"foundation:ParentClass"),
        "rdfs:subClassOf foundation:ParentClass must survive set_class_required_fields; got: {:?}",
        real_parent_links
    );
}

#[test]
fn test_inherited_properties_accessible_after_set_required_fields() {
    // Regression for Bug_1772765777624: Class::get_properties must return inherited
    // properties from the real parent class after set_class_required_fields has run.
    // Previously, the parent rdfs:subClassOf link was destroyed, breaking inheritance.
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::Class;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:FinancialTransaction", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:transactionDate", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
        Triple::new("foundation:transactionDate", "rdfs:domain", Object::Iri("foundation:FinancialTransaction".to_string())),
        Triple::new("foundation:InstallmentPayment", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:InstallmentPayment", "rdfs:subClassOf", Object::Iri("foundation:FinancialTransaction".to_string())),
        Triple::new("foundation:scheduledAt", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
        Triple::new("foundation:scheduledAt", "rdfs:domain", Object::Iri("foundation:InstallmentPayment".to_string())),
    ], "test").unwrap();

    set_class_required_fields(
        &mut conn, "foundation:InstallmentPayment", &["foundation:scheduledAt"], "test",
    ).unwrap();

    let class = Class::get(&conn, "foundation:InstallmentPayment")
        .unwrap()
        .expect("InstallmentPayment class must exist after set_class_required_fields");

    let prop_iris: Vec<&str> = class.properties.iter().map(|(iri, _)| iri.as_str()).collect();
    assert!(
        prop_iris.contains(&"foundation:transactionDate"),
        "Inherited property foundation:transactionDate must remain accessible \
         after set_class_required_fields; got: {:?}",
        prop_iris
    );
}

// ── is_required ─────────────────────────────────────────────────────────

#[test]
fn test_is_required_exact_one() {
    let r = CardinalityRestriction { property_iri: "p".to_string(), min: None, max: None, exact: Some(1) };
    assert!(r.is_required());
}

#[test]
fn test_is_required_exact_zero_is_not_required() {
    let r = CardinalityRestriction { property_iri: "p".to_string(), min: None, max: None, exact: Some(0) };
    assert!(!r.is_required());
}

#[test]
fn test_is_required_min_one() {
    let r = CardinalityRestriction { property_iri: "p".to_string(), min: Some(1), max: None, exact: None };
    assert!(r.is_required());
}

#[test]
fn test_is_required_min_zero_is_not_required() {
    let r = CardinalityRestriction { property_iri: "p".to_string(), min: Some(0), max: None, exact: None };
    assert!(!r.is_required());
}

#[test]
fn test_is_required_no_constraints_is_not_required() {
    let r = CardinalityRestriction { property_iri: "p".to_string(), min: None, max: None, exact: None };
    assert!(!r.is_required());
}

#[test]
fn test_is_required_max_only_is_not_required() {
    let r = CardinalityRestriction { property_iri: "p".to_string(), min: None, max: Some(5), exact: None };
    assert!(!r.is_required());
}

// ── violation_message ───────────────────────────────────────────────────

#[test]
fn test_violation_message_exact_uses_property_label() {
    let r = CardinalityRestriction { property_iri: "foundation:name".to_string(), min: None, max: None, exact: Some(1) };
    let msg = r.violation_message(0, Some("Name"));
    assert_eq!(msg, "Property 'Name' requires exactly 1 value(s), but has 0");
}

#[test]
fn test_violation_message_exact_falls_back_to_iri() {
    let r = CardinalityRestriction { property_iri: "foundation:name".to_string(), min: None, max: None, exact: Some(2) };
    let msg = r.violation_message(3, None);
    assert_eq!(msg, "Property 'foundation:name' requires exactly 2 value(s), but has 3");
}

#[test]
fn test_violation_message_min_below_threshold() {
    let r = CardinalityRestriction { property_iri: "foundation:email".to_string(), min: Some(1), max: None, exact: None };
    let msg = r.violation_message(0, Some("Email"));
    assert_eq!(msg, "Property 'Email' requires at least 1 value(s), but has 0");
}

#[test]
fn test_violation_message_max_exceeded() {
    let r = CardinalityRestriction { property_iri: "foundation:phone".to_string(), min: None, max: Some(3), exact: None };
    let msg = r.violation_message(5, Some("Phone"));
    assert_eq!(msg, "Property 'Phone' allows at most 3 value(s), but has 5");
}

#[test]
fn test_violation_message_fallback_when_no_branch_matches() {
    // Fallback: called with min+max but count is within bounds (defensive path)
    let r = CardinalityRestriction { property_iri: "foundation:tag".to_string(), min: Some(1), max: Some(5), exact: None };
    let msg = r.violation_message(3, Some("Tag"));
    assert_eq!(msg, "Property 'Tag' cardinality constraint violated");
}

// ── get_class_cardinality_restrictions ──────────────────────────────────

#[test]
fn test_get_class_cardinality_restrictions_empty_for_no_restrictions() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Task", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Task").unwrap();
    assert!(restrictions.is_empty());
}

#[test]
fn test_get_class_cardinality_restrictions_reads_min_cardinality() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Project", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Project", "rdfs:subClassOf", Object::Blank("_:r1".to_string())),
        Triple::new("_:r1", "rdf:type", Object::Iri("owl:Restriction".to_string())),
        Triple::new("_:r1", "owl:onProperty", Object::Iri("foundation:title".to_string())),
        Triple::new("_:r1", "owl:minCardinality", Object::Integer(1)),
    ], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Project").unwrap();
    assert_eq!(restrictions.len(), 1);
    assert_eq!(restrictions[0].property_iri, "foundation:title");
    assert_eq!(restrictions[0].min, Some(1));
    assert_eq!(restrictions[0].max, None);
    assert_eq!(restrictions[0].exact, None);
}

#[test]
fn test_get_class_cardinality_restrictions_reads_exact_cardinality() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Invoice", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Invoice", "rdfs:subClassOf", Object::Blank("_:r2".to_string())),
        Triple::new("_:r2", "rdf:type", Object::Iri("owl:Restriction".to_string())),
        Triple::new("_:r2", "owl:onProperty", Object::Iri("foundation:invoiceNumber".to_string())),
        Triple::new("_:r2", "owl:cardinality", Object::Integer(1)),
    ], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Invoice").unwrap();
    assert_eq!(restrictions.len(), 1);
    assert_eq!(restrictions[0].exact, Some(1));
}

#[test]
fn test_get_class_cardinality_restrictions_reads_max_cardinality() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Task", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Task", "rdfs:subClassOf", Object::Blank("_:r3".to_string())),
        Triple::new("_:r3", "rdf:type", Object::Iri("owl:Restriction".to_string())),
        Triple::new("_:r3", "owl:onProperty", Object::Iri("foundation:assignedTo".to_string())),
        Triple::new("_:r3", "owl:maxCardinality", Object::Integer(5)),
    ], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Task").unwrap();
    assert_eq!(restrictions.len(), 1);
    assert_eq!(restrictions[0].max, Some(5));
}

#[test]
fn test_get_class_cardinality_restrictions_ignores_non_restriction_subclasses() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Child", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Child", "rdfs:subClassOf", Object::Iri("foundation:Parent".to_string())),
    ], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Child").unwrap();
    assert!(restrictions.is_empty(), "plain IRI subClassOf must not be treated as restriction");
}

// ── validate_property_cardinality ───────────────────────────────────────

#[test]
fn test_validate_property_cardinality() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Person", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Person", "rdfs:subClassOf", Object::Blank("_:r1".to_string())),
        Triple::new("_:r1", "rdf:type", Object::Iri("owl:Restriction".to_string())),
        Triple::new("_:r1", "owl:onProperty", Object::Iri("foundation:name".to_string())),
        Triple::new("_:r1", "owl:minCardinality", Object::Integer(1)),
    ], "test").unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:john", "rdf:type", Object::Iri("foundation:Person".to_string())),
    ], "test").unwrap();

    validate_property_cardinality(&conn, "foundation:john", "foundation:name", 1).unwrap();

    let result = validate_property_cardinality(&conn, "foundation:john", "foundation:name", 0);
    assert!(result.is_err(), "Should fail with 0 values for a required field");
}

#[test]
fn test_validate_property_cardinality_exact_violation() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Invoice", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Invoice", "rdfs:subClassOf", Object::Blank("_:r1".to_string())),
        Triple::new("_:r1", "rdf:type", Object::Iri("owl:Restriction".to_string())),
        Triple::new("_:r1", "owl:onProperty", Object::Iri("foundation:invoiceNumber".to_string())),
        Triple::new("_:r1", "owl:cardinality", Object::Integer(1)),
        Triple::new("foundation:inv1", "rdf:type", Object::Iri("foundation:Invoice".to_string())),
    ], "test").unwrap();

    assert!(validate_property_cardinality(&conn, "foundation:inv1", "foundation:invoiceNumber", 1).is_ok());
    assert!(validate_property_cardinality(&conn, "foundation:inv1", "foundation:invoiceNumber", 0).is_err());
    assert!(validate_property_cardinality(&conn, "foundation:inv1", "foundation:invoiceNumber", 2).is_err());
}

#[test]
fn test_validate_property_cardinality_max_violation() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Task", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Task", "rdfs:subClassOf", Object::Blank("_:r2".to_string())),
        Triple::new("_:r2", "rdf:type", Object::Iri("owl:Restriction".to_string())),
        Triple::new("_:r2", "owl:onProperty", Object::Iri("foundation:assignedTo".to_string())),
        Triple::new("_:r2", "owl:maxCardinality", Object::Integer(3)),
        Triple::new("foundation:task1", "rdf:type", Object::Iri("foundation:Task".to_string())),
    ], "test").unwrap();

    assert!(validate_property_cardinality(&conn, "foundation:task1", "foundation:assignedTo", 3).is_ok());
    assert!(validate_property_cardinality(&conn, "foundation:task1", "foundation:assignedTo", 4).is_err());
}

#[test]
fn test_validate_property_cardinality_error_message_contains_property_label() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Person", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Person", "rdfs:subClassOf", Object::Blank("_:r3".to_string())),
        Triple::new("_:r3", "rdf:type", Object::Iri("owl:Restriction".to_string())),
        Triple::new("_:r3", "owl:onProperty", Object::Iri("foundation:fullName".to_string())),
        Triple::new("_:r3", "owl:minCardinality", Object::Integer(1)),
        Triple::new("foundation:fullName", "rdfs:label", Object::Literal {
            value: "Full Name".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:alice", "rdf:type", Object::Iri("foundation:Person".to_string())),
    ], "test").unwrap();

    let err = validate_property_cardinality(&conn, "foundation:alice", "foundation:fullName", 0)
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Full Name"), "error message should use property label, got: {msg}");
    assert!(msg.contains("0"), "error message should mention the count, got: {msg}");
}

#[test]
fn test_validate_property_cardinality_no_type_skips_validation() {
    use crate::eavto::test_helpers::setup_test_db;

    let conn = setup_test_db();
    // Individual with no rdf:type — should pass without error
    assert!(validate_property_cardinality(&conn, "foundation:orphan", "foundation:name", 0).is_ok());
}

// ── inherited required fields ────────────────────────────────────────────

#[test]
fn test_child_inherits_required_fields_from_parent() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    // ParentClass has required field "foundation:title"
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:ParentClass", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();
    set_class_required_fields(&mut conn, "foundation:ParentClass", &["foundation:title"], "test").unwrap();

    // ChildClass extends ParentClass with its own required field "foundation:scheduledAt"
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:ChildClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:ChildClass", "rdfs:subClassOf", Object::Iri("foundation:ParentClass".to_string())),
    ], "test").unwrap();
    set_class_required_fields(&mut conn, "foundation:ChildClass", &["foundation:scheduledAt"], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:ChildClass").unwrap();
    let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();

    assert!(props.contains(&"foundation:scheduledAt"), "own required field must be present");
    assert!(props.contains(&"foundation:title"), "inherited required field from parent must be present");
    assert_eq!(restrictions.len(), 2);
}

#[test]
fn test_grandparent_required_fields_are_inherited() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:GrandParent", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Parent", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Parent", "rdfs:subClassOf", Object::Iri("foundation:GrandParent".to_string())),
        Triple::new("foundation:Child", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Child", "rdfs:subClassOf", Object::Iri("foundation:Parent".to_string())),
    ], "test").unwrap();

    set_class_required_fields(&mut conn, "foundation:GrandParent", &["foundation:name"], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Child").unwrap();
    let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();

    assert!(props.contains(&"foundation:name"), "grandparent required field must be inherited by grandchild");
}

#[test]
fn test_child_own_required_field_takes_precedence_over_inherited() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Base", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Derived", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Derived", "rdfs:subClassOf", Object::Iri("foundation:Base".to_string())),
    ], "test").unwrap();

    // Both parent and child define a restriction on the same property
    set_class_required_fields(&mut conn, "foundation:Base", &["foundation:name"], "test").unwrap();
    set_class_required_fields(&mut conn, "foundation:Derived", &["foundation:name"], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Derived").unwrap();
    let name_restrictions: Vec<_> = restrictions.iter()
        .filter(|r| r.property_iri == "foundation:name")
        .collect();

    assert_eq!(name_restrictions.len(), 1, "same property must not appear twice; got {:?}", name_restrictions);
}

#[test]
fn test_class_without_explicit_parent_inherits_from_owl_thing() {
    // Regression: classes with no rdfs:subClassOf IRI link must still inherit
    // restrictions from owl:Thing (all OWL classes implicitly extend owl:Thing).
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    // owl:Thing has a required field restriction
    store::assert_triples(&mut conn, &[
        Triple::new("owl:Thing", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();
    set_class_required_fields(&mut conn, "owl:Thing", &["foundation:hasStatus"], "test").unwrap();

    // MyClass has NO explicit rdfs:subClassOf IRI — only blank-node restrictions
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:MyClass", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();
    set_class_required_fields(&mut conn, "foundation:MyClass", &["foundation:label"], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:MyClass").unwrap();
    let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();

    assert!(props.contains(&"foundation:label"), "own required field must be present");
    assert!(
        props.contains(&"foundation:hasStatus"),
        "owl:Thing required field must be inherited by classes with no explicit parent; got: {:?}",
        props,
    );
}

#[test]
fn test_no_required_fields_on_parent_returns_only_child_fields() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Base", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Derived", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Derived", "rdfs:subClassOf", Object::Iri("foundation:Base".to_string())),
    ], "test").unwrap();

    set_class_required_fields(&mut conn, "foundation:Derived", &["foundation:email"], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Derived").unwrap();
    assert_eq!(restrictions.len(), 1);
    assert_eq!(restrictions[0].property_iri, "foundation:email");
}

// ── set_class_cardinality_restrictions ──────────────────────────────────

#[test]
fn test_set_class_cardinality_restrictions_min_and_max() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();

    set_class_cardinality_restrictions(
        &mut conn,
        "foundation:TestClass",
        &[
            PropertyRestriction { property_iri: "foundation:name", min: Some(1), max: None },
            PropertyRestriction { property_iri: "foundation:tags", min: Some(0), max: Some(5) },
        ],
        "test",
    ).unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
    let name_r = restrictions.iter().find(|r| r.property_iri == "foundation:name").unwrap();
    assert_eq!(name_r.min, Some(1));
    assert_eq!(name_r.max, None);

    let tags_r = restrictions.iter().find(|r| r.property_iri == "foundation:tags").unwrap();
    assert_eq!(tags_r.min, None);
    assert_eq!(tags_r.max, Some(5));
}

#[test]
fn test_set_class_cardinality_restrictions_skips_none_none() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();

    set_class_cardinality_restrictions(
        &mut conn,
        "foundation:TestClass",
        &[
            PropertyRestriction { property_iri: "foundation:name", min: None, max: None },
        ],
        "test",
    ).unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
    assert!(restrictions.is_empty(), "None/None restriction should be skipped");
}

#[test]
fn test_set_class_required_fields_delegates_to_set_cardinality_restrictions() {
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();

    set_class_required_fields(&mut conn, "foundation:TestClass", &["foundation:name"], "test").unwrap();

    let restrictions = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
    assert_eq!(restrictions.len(), 1);
    assert_eq!(restrictions[0].property_iri, "foundation:name");
    assert_eq!(restrictions[0].min, Some(1));
    assert_eq!(restrictions[0].max, None);
}
