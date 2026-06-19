use super::*;
use crate::eavto::test_helpers::setup_test_db;

#[test]
fn test_assert_numeric_property_requires_unit() {
    let mut conn = setup_test_db();
    let prop = Property::new("foundation:hasAge");

    let result = prop.assert(
        &mut conn,
        PropertyType::DatatypeProperty,
        "has age",
        Some("The age of a person"),
        &["foundation:Person"],
        Some("xsd:integer"),
        None,
        "test"
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("qudt:unit"));
}

#[test]
fn test_assert_numeric_property_with_unit() {
    let mut conn = setup_test_db();
    let prop = Property::new("foundation:hasAge");

    let result = prop.assert(
        &mut conn,
        PropertyType::DatatypeProperty,
        "has age",
        Some("The age of a person"),
        &["foundation:Person"],
        Some("xsd:integer"),
        Some("unit:YR"),
        "test"
    );
    assert!(result.is_ok());

    let property = Property::get(&conn, "foundation:hasAge").unwrap().unwrap();
    assert_eq!(property.iri, "foundation:hasAge");
    assert_eq!(property.label, Some("has age".to_string()));
    assert_eq!(property.comment, Some("The age of a person".to_string()));
    assert_eq!(property.property_type, PropertyType::DatatypeProperty);
    assert_eq!(property.domains.len(), 1);
    assert_eq!(property.domains[0], "foundation:Person");
    assert_eq!(property.ranges.len(), 1);
    assert_eq!(property.ranges[0], "xsd:integer");
}

#[test]
fn test_object_property() {
    let mut conn = setup_test_db();
    let prop = Property::new("foundation:hasParent");

    prop.assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "has parent",
        None,
        &["foundation:Person"],
        Some("foundation:Person"),
        None,
        "test"
    ).unwrap();

    let property = Property::get(&conn, "foundation:hasParent").unwrap().unwrap();
    assert_eq!(property.property_type, PropertyType::ObjectProperty);
    assert!(Property::get(&conn, "foundation:hasParent").unwrap().is_some());
}

#[test]
fn test_non_numeric_property_cannot_have_unit() {
    let mut conn = setup_test_db();
    let prop = Property::new("foundation:hasName");

    let result = prop.assert(
        &mut conn,
        PropertyType::DatatypeProperty,
        "has name",
        None,
        &["foundation:Person"],
        Some("xsd:string"),
        Some("unit:GigaBYTE"),
        "test"
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("non-numeric"));
}

#[test]
fn test_all_numeric_types_require_unit() {
    let mut conn = setup_test_db();

    let numeric_types = vec![
        ("xsd:decimal", "unit:Meter"),
        ("xsd:integer", "unit:YR"),
        ("xsd:float", "unit:KiloGM"),
        ("xsd:double", "unit:Second"),
    ];

    for (i, (xsd_type, unit)) in numeric_types.iter().enumerate() {
        let prop = Property::new(&format!("test:prop{}", i));

        let result = prop.assert(
            &mut conn,
            PropertyType::DatatypeProperty,
            "test prop",
            None,
            &[],
            Some(xsd_type),
            None,
            "test"
        );
        assert!(result.is_err(), "Should fail for {} without unit", xsd_type);

        let result = prop.assert(
            &mut conn,
            PropertyType::DatatypeProperty,
            "test prop",
            None,
            &[],
            Some(xsd_type),
            Some(unit),
            "test"
        );
        assert!(result.is_ok(), "Should succeed for {} with unit", xsd_type);
    }
}

fn assert_object_property(conn: &mut Connection, iri: &str) {
    Property::new(iri).assert(
        conn,
        PropertyType::ObjectProperty,
        "Test Property",
        Some("A test property"),
        &["foundation:Person"],
        Some("foundation:Person"),
        None,
        "test",
    ).unwrap();
}

#[test]
fn test_retract_removes_property_definition() {
    let mut conn = setup_test_db();
    assert_object_property(&mut conn, "foundation:hasParent");

    assert!(Property::get(&conn, "foundation:hasParent").unwrap().is_some());

    Property::retract(&mut conn, "foundation:hasParent", "test").unwrap();

    assert!(Property::get(&conn, "foundation:hasParent").unwrap().is_none(),
        "property should no longer exist after retraction");
}

#[test]
fn test_retract_removes_fact_triples_using_property() {
    let mut conn = setup_test_db();
    assert_object_property(&mut conn, "foundation:hasParent");

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:alice", "foundation:hasParent", Object::Iri("foundation:bob".to_string())),
        Triple::new("foundation:carol", "foundation:hasParent", Object::Iri("foundation:dave".to_string())),
    ], "test").unwrap();

    Property::retract(&mut conn, "foundation:hasParent", "test").unwrap();

    let alice_facts = crate::eavto::query::get_by_entity_predicate(
        &conn, "foundation:alice", "foundation:hasParent"
    ).unwrap();
    assert!(alice_facts.triples.is_empty(), "fact triple for alice must be retracted");

    let carol_facts = crate::eavto::query::get_by_entity_predicate(
        &conn, "foundation:carol", "foundation:hasParent"
    ).unwrap();
    assert!(carol_facts.triples.is_empty(), "fact triple for carol must be retracted");
}

#[test]
fn test_retract_returns_affected_subjects() {
    let mut conn = setup_test_db();
    assert_object_property(&mut conn, "foundation:hasParent");

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:alice", "foundation:hasParent", Object::Iri("foundation:bob".to_string())),
        Triple::new("foundation:carol", "foundation:hasParent", Object::Iri("foundation:dave".to_string())),
    ], "test").unwrap();

    let mut affected = Property::retract(&mut conn, "foundation:hasParent", "test").unwrap();
    affected.sort();

    assert_eq!(affected, vec!["foundation:alice", "foundation:carol"]);
}

#[test]
fn test_retract_nonexistent_property_returns_empty() {
    let mut conn = setup_test_db();

    let affected = Property::retract(&mut conn, "foundation:ghost", "test").unwrap();
    assert!(affected.is_empty(), "retracting a non-existent property must not error");
}

#[test]
fn test_retract_with_no_usages_returns_empty_affected() {
    let mut conn = setup_test_db();
    assert_object_property(&mut conn, "foundation:hasParent");

    let affected = Property::retract(&mut conn, "foundation:hasParent", "test").unwrap();
    assert!(affected.is_empty(), "no instances used the property, so affected must be empty");
}

#[test]
fn test_retract_datatype_property() {
    let mut conn = setup_test_db();

    Property::new("foundation:birthDate").assert(
        &mut conn,
        PropertyType::DatatypeProperty,
        "birth date",
        None,
        &["foundation:Person"],
        Some("xsd:string"),
        None,
        "test",
    ).unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:alice", "foundation:birthDate", Object::Literal {
            value: "1990-01-01".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let affected = Property::retract(&mut conn, "foundation:birthDate", "test").unwrap();
    assert!(affected.contains(&"foundation:alice".to_string()));
    assert!(Property::get(&conn, "foundation:birthDate").unwrap().is_none());
}

#[test]
fn test_property_characteristics() {
    let mut conn = setup_test_db();
    let prop = Property::new("foundation:hasParent");

    prop.assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "has parent",
        None,
        &[],
        None,
        None,
        "test"
    ).unwrap();

    let functional_triple = Triple::new(
        "foundation:hasParent",
        rdf::TYPE,
        Object::Iri(owl::FUNCTIONAL_PROPERTY.to_string())
    );
    store::assert_triples(&mut conn, &[functional_triple], "test").unwrap();

    let property = Property::get(&conn, "foundation:hasParent").unwrap().unwrap();
    assert!(property.is_functional);
}

#[test]
fn test_transitive_property_detection() {
    let mut conn = setup_test_db();

    Property::new("foundation:isAncestorOf").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "is ancestor of",
        None,
        &[],
        None,
        None,
        "test",
    ).unwrap();

    store::append_triples(&mut conn, &[
        Triple::new("foundation:isAncestorOf", rdf::TYPE, Object::Iri(owl::TRANSITIVE_PROPERTY.to_string())),
    ], "test").unwrap();

    let property = Property::get(&conn, "foundation:isAncestorOf").unwrap().unwrap();
    assert!(property.is_transitive, "property should be detected as transitive");
    assert!(!property.is_symmetric);
    assert!(!property.is_functional);
    assert_eq!(property.property_type, PropertyType::ObjectProperty);
}

#[test]
fn test_symmetric_property_detection() {
    let mut conn = setup_test_db();

    Property::new("foundation:isSiblingOf").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "is sibling of",
        None,
        &[],
        None,
        None,
        "test",
    ).unwrap();

    store::append_triples(&mut conn, &[
        Triple::new("foundation:isSiblingOf", rdf::TYPE, Object::Iri(owl::SYMMETRIC_PROPERTY.to_string())),
    ], "test").unwrap();

    let property = Property::get(&conn, "foundation:isSiblingOf").unwrap().unwrap();
    assert!(property.is_symmetric, "property should be detected as symmetric");
    assert!(!property.is_transitive);
    assert!(!property.is_functional);
    assert_eq!(property.property_type, PropertyType::ObjectProperty);
}

#[test]
fn test_annotation_property_detection() {
    let mut conn = setup_test_db();

    Property::new("foundation:seeAlso").assert(
        &mut conn,
        PropertyType::AnnotationProperty,
        "see also",
        None,
        &[],
        None,
        None,
        "test",
    ).unwrap();

    let property = Property::get(&conn, "foundation:seeAlso").unwrap().unwrap();
    assert_eq!(property.property_type, PropertyType::AnnotationProperty);
    assert!(!property.is_functional);
    assert!(!property.is_transitive);
    assert!(!property.is_symmetric);
}

#[test]
fn test_transitive_and_symmetric_combined() {
    let mut conn = setup_test_db();

    Property::new("foundation:equals").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "equals",
        None,
        &[],
        None,
        None,
        "test",
    ).unwrap();

    store::append_triples(&mut conn, &[
        Triple::new("foundation:equals", rdf::TYPE, Object::Iri(owl::TRANSITIVE_PROPERTY.to_string())),
        Triple::new("foundation:equals", rdf::TYPE, Object::Iri(owl::SYMMETRIC_PROPERTY.to_string())),
    ], "test").unwrap();

    let property = Property::get(&conn, "foundation:equals").unwrap().unwrap();
    assert!(property.is_transitive);
    assert!(property.is_symmetric);
    assert_eq!(property.property_type, PropertyType::ObjectProperty);
}

#[test]
fn test_property_without_characteristics_has_all_false() {
    let mut conn = setup_test_db();

    Property::new("foundation:hasName").assert(
        &mut conn,
        PropertyType::DatatypeProperty,
        "has name",
        None,
        &[],
        Some("xsd:string"),
        None,
        "test",
    ).unwrap();

    let property = Property::get(&conn, "foundation:hasName").unwrap().unwrap();
    assert!(!property.is_functional);
    assert!(!property.is_transitive);
    assert!(!property.is_symmetric);
    assert_eq!(property.property_type, PropertyType::DatatypeProperty);
}

#[test]
fn test_domain_labels_loaded_from_store() {
    use crate::eavto::{store, Triple, Object};
    let mut conn = setup_test_db();

    Property::new("foundation:hasFather").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "has father",
        None,
        &["foundation:Person"],
        Some("foundation:Person"),
        None,
        "test",
    ).unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new(
            "test:DomainLabel_1", "rdf:type",
            Object::Iri("foundation:DomainLabel".to_string()),
        ),
        Triple::new(
            "test:DomainLabel_1", "foundation:onProperty",
            Object::Iri("foundation:hasFather".to_string()),
        ),
        Triple::new(
            "test:DomainLabel_1", "foundation:forDomain",
            Object::Iri("foundation:Person".to_string()),
        ),
        Triple::new(
            "test:DomainLabel_1", "foundation:forwardLabel",
            Object::Literal {
                value: "has father".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
        ),
        Triple::new(
            "test:DomainLabel_1", "foundation:inverseLabel",
            Object::Literal {
                value: "has child".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
        ),
    ], "test").unwrap();

    let prop = Property::get(&conn, "foundation:hasFather").unwrap().unwrap();
    assert_eq!(prop.domain_labels.len(), 1);
    assert_eq!(prop.domain_labels[0].domain, "foundation:Person");
    assert_eq!(prop.domain_labels[0].forward_label, "has father");
    assert_eq!(prop.domain_labels[0].inverse_label, Some("has child".to_string()));
}

#[test]
fn test_domain_label_without_inverse_is_loaded() {
    use crate::eavto::{store, Triple, Object};
    let mut conn = setup_test_db();

    Property::new("foundation:hasMember").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "has member",
        None,
        &[],
        None,
        None,
        "test",
    ).unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new(
            "test:DomainLabel_2", "rdf:type",
            Object::Iri("foundation:DomainLabel".to_string()),
        ),
        Triple::new(
            "test:DomainLabel_2", "foundation:onProperty",
            Object::Iri("foundation:hasMember".to_string()),
        ),
        Triple::new(
            "test:DomainLabel_2", "foundation:forDomain",
            Object::Iri("foundation:Team".to_string()),
        ),
        Triple::new(
            "test:DomainLabel_2", "foundation:forwardLabel",
            Object::Literal {
                value: "member of team".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
        ),
    ], "test").unwrap();

    let prop = Property::get(&conn, "foundation:hasMember").unwrap().unwrap();
    assert_eq!(prop.domain_labels.len(), 1);
    assert_eq!(prop.domain_labels[0].inverse_label, None);
}

#[test]
fn test_ac2_property_without_domain_labels_falls_back_to_rdfs_label() {
    let mut conn = setup_test_db();

    Property::new("foundation:hasRole").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "has role",
        None,
        &[],
        None,
        None,
        "test",
    ).unwrap();

    let prop = Property::get(&conn, "foundation:hasRole").unwrap().unwrap();
    assert!(prop.domain_labels.is_empty(),
        "AC2: property with no DomainLabel entries must have empty domain_labels");
    assert_eq!(prop.label, Some("has role".to_string()),
        "AC2: rdfs:label must be available as fallback when domain_labels is empty");
}

#[test]
fn test_ac4_domain_label_without_inverse_falls_back_to_forward_label() {
    use crate::eavto::{store, Triple, Object};
    let mut conn = setup_test_db();

    Property::new("foundation:contains").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "contains",
        None,
        &[],
        None,
        None,
        "test",
    ).unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("test:DL_ac4", "rdf:type",
            Object::Iri("foundation:DomainLabel".to_string())),
        Triple::new("test:DL_ac4", "foundation:onProperty",
            Object::Iri("foundation:contains".to_string())),
        Triple::new("test:DL_ac4", "foundation:forDomain",
            Object::Iri("foundation:Container".to_string())),
        Triple::new("test:DL_ac4", "foundation:forwardLabel", Object::Literal {
            value: "contains".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let prop = Property::get(&conn, "foundation:contains").unwrap().unwrap();
    let dl = prop.domain_labels.iter()
        .find(|dl| dl.domain == "foundation:Container")
        .expect("AC4: DomainLabel entry must be present");

    assert!(dl.inverse_label.is_none(),
        "AC4: inverse_label must be None when not specified");
    let resolved_backlink = dl.inverse_label.as_deref().unwrap_or(&dl.forward_label);
    assert_eq!(resolved_backlink, "contains",
        "AC4: absent inverse_label falls back to forward_label in backlink resolution");
}

fn setup_properties_with_domain_labels(conn: &mut Connection, n: usize) -> Vec<String> {
    use crate::eavto::{store, Triple, Object};
    let iris: Vec<String> = (0..n).map(|i| format!("foundation:perfProp{}", i)).collect();
    for (i, prop_iri) in iris.iter().enumerate() {
        Property::new(prop_iri).assert(
            conn,
            PropertyType::ObjectProperty,
            &format!("Perf Prop {}", i),
            None,
            &["foundation:Thing"],
            Some("foundation:Thing"),
            None,
            "test",
        ).unwrap();
        let dl_iri = format!("test:DL_perf_{}", i);
        store::assert_triples(conn, &[
            Triple::new(&dl_iri, "rdf:type",
                Object::Iri("foundation:DomainLabel".to_string())),
            Triple::new(&dl_iri, "foundation:onProperty",
                Object::Iri(prop_iri.clone())),
            Triple::new(&dl_iri, "foundation:forDomain",
                Object::Iri("foundation:Thing".to_string())),
            Triple::new(&dl_iri, "foundation:forwardLabel", Object::Literal {
                value: format!("perf prop {}", i),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new(&dl_iri, "foundation:inverseLabel", Object::Literal {
                value: format!("is perf prop {} of", i),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();
    }
    iris
}

#[test]
fn test_get_batch_correctness_matches_individual_get() {
    let mut conn = setup_test_db();
    let iris = setup_properties_with_domain_labels(&mut conn, 5);
    let iris_ref: Vec<&str> = iris.iter().map(|s| s.as_str()).collect();

    let batch = Property::get_batch(&conn, &iris_ref).unwrap();

    for iri in &iris {
        let single = Property::get(&conn, iri).unwrap().unwrap();
        let from_batch = batch.get(iri.as_str())
            .unwrap_or_else(|| panic!("get_batch must contain {iri}"));

        assert_eq!(single.iri, from_batch.iri);
        assert_eq!(single.label, from_batch.label);
        assert_eq!(single.comment, from_batch.comment);
        assert_eq!(single.property_type, from_batch.property_type);
        assert_eq!(single.domains, from_batch.domains);
        assert_eq!(single.ranges, from_batch.ranges);
        assert_eq!(single.is_functional, from_batch.is_functional);
        assert_eq!(single.domain_labels.len(), from_batch.domain_labels.len(),
            "domain_labels count must match for {iri}");
        if !single.domain_labels.is_empty() {
            assert_eq!(single.domain_labels[0].domain, from_batch.domain_labels[0].domain);
            assert_eq!(single.domain_labels[0].forward_label, from_batch.domain_labels[0].forward_label);
            assert_eq!(single.domain_labels[0].inverse_label, from_batch.domain_labels[0].inverse_label);
        }
    }
}

#[test]
fn test_get_batch_is_faster_than_individual_gets() {
    use std::time::Instant;
    const N: usize = 20;
    let mut conn = setup_test_db();
    let iris = setup_properties_with_domain_labels(&mut conn, N);
    let iris_ref: Vec<&str> = iris.iter().map(|s| s.as_str()).collect();

    let t_individual = Instant::now();
    for iri in &iris {
        Property::get(&conn, iri).unwrap().unwrap();
    }
    let elapsed_individual = t_individual.elapsed();

    let t_batch = Instant::now();
    let batch_result = Property::get_batch(&conn, &iris_ref).unwrap();
    let elapsed_batch = t_batch.elapsed();

    assert_eq!(batch_result.len(), N, "get_batch must return all {N} properties");

    let individual_us = elapsed_individual.as_micros().max(1);
    let batch_us = elapsed_batch.as_micros().max(1);
    let speedup = individual_us as f64 / batch_us as f64;

    eprintln!(
        "[perf] {} × Property::get(): {}µs  |  1 × get_batch(): {}µs  |  speedup: {:.1}×",
        N, individual_us, batch_us, speedup
    );

    assert!(
        elapsed_batch < elapsed_individual,
        "get_batch ({elapsed_batch:?}) must be faster than {N} individual get() calls ({elapsed_individual:?})"
    );
}

// ── search_filtered ──────────────────────────────────────────────────────────

fn seed_searchable_prop(
    conn: &mut Connection,
    iri: &str,
    label: &str,
    comment: Option<&str>,
    prop_type: PropertyType,
) {
    let range = if prop_type == PropertyType::DatatypeProperty {
        Some("xsd:string")
    } else {
        Some("owl:Thing")
    };
    Property::new(iri).assert(conn, prop_type, label, comment, &[], range, None, "test").unwrap();
}

#[test]
fn test_search_filtered_match_by_iri_token() {
    let mut conn = setup_test_db();
    seed_searchable_prop(&mut conn, "test:hasColor", "has color", None, PropertyType::DatatypeProperty);
    seed_searchable_prop(&mut conn, "test:hasSize", "has size", None, PropertyType::DatatypeProperty);

    let (items, total) = Property::search_filtered(&conn, "color", 10, 0).unwrap();
    assert_eq!(total, 1);
    assert_eq!(items[0].0, "test:hasColor");
}

#[test]
fn test_search_filtered_match_by_label_token() {
    let mut conn = setup_test_db();
    seed_searchable_prop(&mut conn, "test:p1", "blue widget", None, PropertyType::DatatypeProperty);
    seed_searchable_prop(&mut conn, "test:p2", "red thing", None, PropertyType::DatatypeProperty);

    let (items, total) = Property::search_filtered(&conn, "blue", 10, 0).unwrap();
    assert_eq!(total, 1);
    assert_eq!(items[0].0, "test:p1");
}

#[test]
fn test_search_filtered_match_by_comment_token() {
    let mut conn = setup_test_db();
    seed_searchable_prop(&mut conn, "test:pCommented", "labeled", Some("unique_xyz_comment"), PropertyType::DatatypeProperty);
    seed_searchable_prop(&mut conn, "test:pNoComment", "other", None, PropertyType::DatatypeProperty);

    let (items, _) = Property::search_filtered(&conn, "unique_xyz", 10, 0).unwrap();
    let found: Vec<_> = items.iter().map(|(iri, _)| iri.as_str()).collect();
    assert!(found.contains(&"test:pCommented"), "deve encontrar via rdfs:comment");
}

#[test]
fn test_search_filtered_empty_query_returns_all_bounded() {
    let mut conn = setup_test_db();
    for i in 0..5 {
        seed_searchable_prop(&mut conn, &format!("test:ep{}", i), &format!("Ep {}", i), None, PropertyType::DatatypeProperty);
    }

    let (items, total) = Property::search_filtered(&conn, "", 10, 0).unwrap();
    assert_eq!(total, 5);
    assert_eq!(items.len(), 5);
}

#[test]
fn test_search_filtered_pagination_offset_limit() {
    let mut conn = setup_test_db();
    for i in 0..6 {
        seed_searchable_prop(&mut conn, &format!("test:pg{}", i), &format!("Pg {}", i), None, PropertyType::DatatypeProperty);
    }

    let (page1, total1) = Property::search_filtered(&conn, "", 3, 0).unwrap();
    let (page2, total2) = Property::search_filtered(&conn, "", 3, 3).unwrap();

    assert_eq!(total1, 6);
    assert_eq!(total2, 6, "total must be stable across pages");
    assert_eq!(page1.len(), 3);
    assert_eq!(page2.len(), 3);
    let p1: Vec<_> = page1.iter().map(|(iri, _)| iri.clone()).collect();
    for (iri, _) in &page2 {
        assert!(!p1.contains(iri), "sem sobreposição entre páginas");
    }
}

#[test]
fn test_search_filtered_empty_result() {
    let conn = setup_test_db();
    let (items, total) = Property::search_filtered(&conn, "zzznomatch", 10, 0).unwrap();
    assert_eq!(total, 0);
    assert!(items.is_empty());
}

#[test]
fn test_search_filtered_no_properties_empty_query() {
    let conn = setup_test_db();
    let (items, total) = Property::search_filtered(&conn, "", 10, 0).unwrap();
    assert_eq!(total, 0);
    assert!(items.is_empty());
}

// ── Property::find_all_iris ───────────────────────────────────────────────────

#[test]
fn test_find_all_iris_returns_object_and_datatype_properties() {
    let mut conn = setup_test_db();

    let obj = Property::new("test:linkedTo");
    obj.assert(&mut conn, PropertyType::ObjectProperty, "linked to",
        None, &["test:Thing"], Some("test:OtherThing"), None, "test").unwrap();

    let dat = Property::new("test:hasCount");
    dat.assert(&mut conn, PropertyType::DatatypeProperty, "has count",
        None, &["test:Thing"], Some("xsd:integer"), Some("unit:Count"), "test").unwrap();

    let all = Property::find_all_iris(&conn).unwrap();
    assert!(all.contains(&"test:linkedTo".to_string()), "ObjectProperty must be included");
    assert!(all.contains(&"test:hasCount".to_string()), "DatatypeProperty must be included");
}

#[test]
fn test_find_all_iris_empty_when_no_properties() {
    let conn = setup_test_db();
    let all = Property::find_all_iris(&conn).unwrap();
    assert!(all.is_empty());
}

#[test]
fn test_find_all_iris_is_sorted_and_deduped() {
    let mut conn = setup_test_db();
    let p1 = Property::new("test:zProp");
    p1.assert(&mut conn, PropertyType::ObjectProperty, "z prop",
        None, &[], Some("test:T"), None, "test").unwrap();
    let p2 = Property::new("test:aProp");
    p2.assert(&mut conn, PropertyType::ObjectProperty, "a prop",
        None, &[], Some("test:T"), None, "test").unwrap();

    let all = Property::find_all_iris(&conn).unwrap();
    let mut sorted = all.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(all, sorted, "find_all_iris must return sorted, deduped list");
}

// ── Property::restore ─────────────────────────────────────────────────────────

#[test]
fn test_restore_retracted_property_comes_back() {
    let mut conn = setup_test_db();

    let prop = Property::new("test:recoverMe");
    prop.assert(&mut conn, PropertyType::ObjectProperty, "recover me",
        None, &["test:Task"], Some("test:Other"), None, "test").unwrap();

    Property::retract(&mut conn, "test:recoverMe", "test").unwrap();
    assert!(
        Property::get(&conn, "test:recoverMe").unwrap().is_none(),
        "pre-condition: property must be retracted"
    );

    Property::restore(&mut conn, "test:recoverMe", "test").unwrap();

    let restored = Property::get(&conn, "test:recoverMe").unwrap();
    assert!(restored.is_some(), "after restore property must be visible again");
    let restored = restored.unwrap();
    assert_eq!(restored.label, Some("recover me".to_string()));
}

#[test]
fn test_restore_errors_when_not_retracted() {
    let mut conn = setup_test_db();
    let prop = Property::new("test:liveProperty");
    prop.assert(&mut conn, PropertyType::ObjectProperty, "live property",
        None, &[], Some("test:T"), None, "test").unwrap();

    let result = Property::restore(&mut conn, "test:liveProperty", "test");
    assert!(result.is_err(), "restore on an active (non-retracted) property must error");
}

// ── Property::is_functional (field) ──────────────────────────────────────────

#[test]
fn test_is_functional_field_true_when_declared() {
    use crate::eavto::{store, Triple, Object};
    use crate::owl::vocabulary::{rdf, owl};

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:uniqueProp", rdf::TYPE, Object::Iri(owl::OBJECT_PROPERTY.to_string())),
        Triple::new("test:uniqueProp", rdf::TYPE, Object::Iri(owl::FUNCTIONAL_PROPERTY.to_string())),
        Triple::new("test:uniqueProp", "rdfs:label", Object::Literal {
            value: "unique prop".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let prop = Property::get(&conn, "test:uniqueProp").unwrap().unwrap();
    assert!(prop.is_functional, "is_functional field must be true when owl:FunctionalProperty is asserted");
}

#[test]
fn test_is_functional_field_false_when_not_declared() {
    use crate::eavto::{store, Triple, Object};
    use crate::owl::vocabulary::{rdf, owl};

    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:normalProp", rdf::TYPE, Object::Iri(owl::OBJECT_PROPERTY.to_string())),
        Triple::new("test:normalProp", "rdfs:label", Object::Literal {
            value: "normal prop".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let prop = Property::get(&conn, "test:normalProp").unwrap().unwrap();
    assert!(!prop.is_functional, "is_functional field must be false without owl:FunctionalProperty");
}
