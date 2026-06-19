use super::*;
use crate::eavto::test_helpers::setup_test_db;
use crate::owl::{Class, ClassType, Property, PropertyType, vocabulary::rdf};

#[test]
fn test_write_to_calculated_property_is_rejected() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:Rectangle");
    c.assert(&mut conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").unwrap();

    let width_prop = Property::new("foundation:hasWidth");
    width_prop.assert(&mut conn, PropertyType::DatatypeProperty, "has width", None,
        &["foundation:Rectangle"], Some("xsd:integer"), Some("unit:Meter"), "test").unwrap();

    let area_prop = Property::new("foundation:hasArea");
    area_prop.assert(&mut conn, PropertyType::DatatypeProperty, "has area", None,
        &["foundation:Rectangle"], Some("xsd:decimal"), Some("unit:SquareMeter"), "test").unwrap();

    let ind = Individual::new("foundation:MyRect");
    ind.assert(&mut conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").unwrap();

    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
        [],
    ).unwrap();
    let tx_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params!["foundation:hasArea", "{{foundation:hasWidth}} * 2", tx_id],
    ).unwrap();

    let result = ind.add_property(
        &mut conn,
        "foundation:hasArea",
        vec![Object::Literal { value: "100".to_string(), datatype: Some("xsd:decimal".to_string()), language: None }],
        "test",
    );

    assert!(result.is_err(), "Should reject write to calculated property");
    if let Err(OwlError::ValidationError(msg)) = result {
        assert!(msg.contains("calculated via a formula"));
    } else {
        panic!("Expected ValidationError");
    }
}

#[test]
fn test_write_to_non_calculated_property_succeeds() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:Rectangle");
    c.assert(&mut conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").unwrap();

    let width_prop = Property::new("foundation:hasWidth");
    width_prop.assert(&mut conn, PropertyType::DatatypeProperty, "has width", None,
        &["foundation:Rectangle"], Some("xsd:integer"), Some("unit:Meter"), "test").unwrap();

    let ind = Individual::new("foundation:MyRect");
    ind.assert(&mut conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").unwrap();

    let result = ind.add_property(
        &mut conn,
        "foundation:hasWidth",
        vec![Object::Literal { value: "5".to_string(), datatype: Some("xsd:integer".to_string()), language: None }],
        "test",
    );

    assert!(result.is_ok(), "Should accept write to non-calculated property");
}

// Regression: Bug_1773352703259 — foundation:hasIcon is an ObjectProperty but must accept
// literal values when set to a URL (file://, https://, etc.).
// The meta-property bypass must cover the full validation pipeline, not just formula checks.
#[test]
fn test_add_property_has_icon_file_url_literal_is_accepted() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:Item");
    c.assert(&mut conn, ClassType::OwlClass, "Item", "https://example.com/item.svg", None, "test").unwrap();

    Property::new("foundation:hasIcon")
        .assert(&mut conn, PropertyType::ObjectProperty, "has icon", None,
            &[], Some("foundation:Icon"), None, "test")
        .unwrap();

    let ind = Individual::new("foundation:MyItem");
    ind.assert(&mut conn, "foundation:Item", "My Item", "https://example.com/item.svg", "test").unwrap();

    let result = ind.add_property(
        &mut conn,
        "foundation:hasIcon",
        vec![Object::Literal {
            value: "file:///path/to/icon.png".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }],
        "test",
    );
    assert!(result.is_ok(), "foundation:hasIcon must accept file:// literal values: {:?}", result.err());
}

#[test]
fn test_meta_property_bypasses_formula_protection() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:Rectangle");
    c.assert(&mut conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").unwrap();

    let ind = Individual::new("foundation:MyRect");
    ind.assert(&mut conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").unwrap();

    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
        [],
    ).unwrap();
    let tx_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params!["rdfs:label", "some formula", tx_id],
    ).unwrap();

    let result = ind.add_property(
        &mut conn,
        "rdfs:label",
        vec![Object::Literal { value: "Updated Label".to_string(), datatype: Some("xsd:string".to_string()), language: None }],
        "test",
    );

    assert!(result.is_ok(), "Meta properties should bypass formula protection");
}

#[test]
fn test_calculated_property_error_message_is_descriptive() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:Rectangle");
    c.assert(&mut conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").unwrap();

    let area_prop = Property::new("foundation:hasArea");
    area_prop.assert(&mut conn, PropertyType::DatatypeProperty, "has area", None,
        &["foundation:Rectangle"], Some("xsd:decimal"), Some("unit:SquareMeter"), "test").unwrap();

    let ind = Individual::new("foundation:MyRect");
    ind.assert(&mut conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").unwrap();

    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
        [],
    ).unwrap();
    let tx_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params!["foundation:hasArea", "{{foundation:hasWidth}} * 2", tx_id],
    ).unwrap();

    let result = ind.add_property(
        &mut conn,
        "foundation:hasArea",
        vec![Object::Literal { value: "100".to_string(), datatype: Some("xsd:decimal".to_string()), language: None }],
        "test",
    );

    if let Err(OwlError::ValidationError(msg)) = result {
        assert!(msg.contains("foundation:hasArea"), "Error should contain the property IRI");
        assert!(msg.contains("calculated via a formula"), "Error should mention formula");
        assert!(msg.contains("cannot be set directly"), "Error should say cannot be set directly");
    } else {
        panic!("Expected ValidationError");
    }
}

#[test]
fn test_serializable_properties_integer() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None,
        icon: None,
        comment: None,
        types: vec![],
        properties: vec![("foundation:age".to_string(), Object::Integer(30))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["property"], "foundation:age");
    assert_eq!(props[0]["value"], 30);
}

#[test]
fn test_serializable_properties_number() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:score".to_string(), Object::Number(9.5))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["value"], 9.5);
}

#[test]
fn test_serializable_properties_boolean() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:active".to_string(), Object::Boolean(true))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["value"], true);
}

#[test]
fn test_serializable_properties_string_literal() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:name".to_string(), Object::Literal {
            value: "Alice".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["value"], "Alice");
}

#[test]
fn test_serializable_properties_decimal_literal_parsed_as_number() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:ratio".to_string(), Object::Literal {
            value: "3.14".to_string(),
            datatype: Some("xsd:decimal".to_string()),
            language: None,
        })],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["value"], 3.14);
}

#[test]
fn test_serializable_properties_integer_literal_parsed_as_number() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:count".to_string(), Object::Literal {
            value: "99".to_string(),
            datatype: Some("xsd:integer".to_string()),
            language: None,
        })],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["value"], 99);
}

#[test]
fn test_serializable_properties_iri_value() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:knows".to_string(), Object::Iri("foundation:Bob".to_string()))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["value"], "foundation:Bob");
}

#[test]
fn test_serializable_properties_includes_unit_when_property_has_one() {
    let mut conn = setup_test_db();

    Property::new("foundation:height").assert(
        &mut conn,
        PropertyType::DatatypeProperty,
        "height",
        None,
        &[],
        Some("xsd:decimal"),
        Some("unit:Meter"),
        "test",
    ).unwrap();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:height".to_string(), Object::Number(1.75))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["unit"], "unit:Meter");
}

#[test]
fn test_serializable_properties_no_unit_key_when_property_has_none() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:nickname".to_string(), Object::Literal {
            value: "Ally".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert!(props[0].get("unit").is_none(), "No unit key when property has no unit");
}

#[test]
fn test_serializable_properties_multiple() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "foundation:Alice".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![
            ("foundation:age".to_string(), Object::Integer(30)),
            ("foundation:name".to_string(), Object::Literal {
                value: "Alice".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            ("foundation:active".to_string(), Object::Boolean(false)),
        ],
        property_tx: vec![0, 0, 0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props.len(), 3);
}

#[test]
fn test_remove_property_value_iri_happy_path() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", rdf::TYPE, Object::Iri("foundation:Person".to_string())),
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
    ], "test").unwrap();

    let result = Individual::remove_property_value(
        &mut conn,
        "foundation:Alice",
        "foundation:knows",
        "foundation:Bob",
        "test",
    ).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), Object::Iri("foundation:Bob".to_string()));

    let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert!(after.triples.is_empty(), "Triple should have been retracted");
}

#[test]
fn test_remove_property_value_integer() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:age", Object::Integer(30)),
    ], "test").unwrap();

    let result = Individual::remove_property_value(
        &mut conn,
        "foundation:Alice",
        "foundation:age",
        "30",
        "test",
    ).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), Object::Integer(30));

    let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:age").unwrap();
    assert!(after.triples.is_empty(), "Integer triple should have been retracted");
}

#[test]
fn test_remove_property_value_string_literal() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:nickname", Object::Literal {
            value: "Ally".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let result = Individual::remove_property_value(
        &mut conn,
        "foundation:Alice",
        "foundation:nickname",
        "Ally",
        "test",
    ).unwrap();

    assert!(result.is_some());

    let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:nickname").unwrap();
    assert!(after.triples.is_empty(), "String literal triple should have been retracted");
}

#[test]
fn test_remove_property_value_nonexistent_value_returns_none() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
    ], "test").unwrap();

    let result = Individual::remove_property_value(
        &mut conn,
        "foundation:Alice",
        "foundation:knows",
        "foundation:Charlie",
        "test",
    ).unwrap();

    assert!(result.is_none(), "Should return None when value does not match");

    let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert_eq!(after.triples.len(), 1, "Existing triple should be untouched");
}

#[test]
fn test_remove_property_value_no_triples_returns_none() {
    let mut conn = setup_test_db();

    let result = Individual::remove_property_value(
        &mut conn,
        "foundation:Alice",
        "foundation:knows",
        "foundation:Bob",
        "test",
    ).unwrap();

    assert!(result.is_none(), "Should return None when property has no triples");
}

#[test]
fn test_remove_property_value_boolean() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:active", Object::Boolean(true)),
    ], "test").unwrap();

    let result = Individual::remove_property_value(
        &mut conn,
        "foundation:Alice",
        "foundation:active",
        "true",
        "test",
    ).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), Object::Boolean(true));
}

#[test]
fn test_remove_property_value_number() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:score", Object::Number(9.5)),
    ], "test").unwrap();

    let result = Individual::remove_property_value(
        &mut conn,
        "foundation:Alice",
        "foundation:score",
        "9.5",
        "test",
    ).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), Object::Number(9.5));
}

#[test]
fn test_remove_property_value_only_removes_matching_multivalue() {
    let mut conn = setup_test_db();

    store::append_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
    ], "test").unwrap();

    let result = Individual::remove_property_value(
        &mut conn,
        "foundation:Alice",
        "foundation:knows",
        "foundation:Bob",
        "test",
    ).unwrap();

    assert!(result.is_some());

    let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert_eq!(after.triples.len(), 1, "Only the matching value should be removed");
    assert_eq!(
        after.triples[0].object,
        Object::Iri("foundation:Carol".to_string()),
    );
}

#[test]
fn test_get_property_count_returns_zero_when_no_values() {
    let conn = setup_test_db();
    let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert_eq!(count, 0);
}

#[test]
fn test_get_property_count_returns_one_for_single_value() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
    ], "test").unwrap();

    let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_get_property_count_returns_correct_count_for_multiple_values() {
    let mut conn = setup_test_db();
    store::append_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Dave".to_string())),
    ], "test").unwrap();

    let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert_eq!(count, 3);
}

#[test]
fn test_get_property_count_excludes_retracted_values() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
    ], "test").unwrap();
    Individual::remove_property_value(&mut conn, "foundation:Alice", "foundation:knows", "foundation:Bob", "test").unwrap();

    let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert_eq!(count, 0);
}

#[test]
fn test_clear_property_removes_all_values() {
    let mut conn = setup_test_db();
    store::append_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
    ], "test").unwrap();

    Individual::clear_property(&mut conn, "foundation:Alice", "foundation:knows", "test").unwrap();

    let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert!(after.triples.is_empty(), "All values should have been retracted");
}

#[test]
fn test_clear_property_is_noop_when_no_values() {
    let mut conn = setup_test_db();
    let result = Individual::clear_property(&mut conn, "foundation:Alice", "foundation:knows", "test");
    assert!(result.is_ok(), "clear_property on empty property should not error");
}

#[test]
fn test_clear_property_preserves_other_properties() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        Triple::new("foundation:Alice", "foundation:name", Object::Literal {
            value: "Alice".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    Individual::clear_property(&mut conn, "foundation:Alice", "foundation:knows", "test").unwrap();

    let knows = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
    assert!(knows.triples.is_empty(), "foundation:knows should be cleared");

    let name = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:name").unwrap();
    assert_eq!(name.triples.len(), 1, "foundation:name must not be affected");
}

#[test]
fn test_get_retracted_properties_empty_when_nothing_retracted() {
    let conn = setup_test_db();
    let result = Individual::get_retracted_properties(&conn, "foundation:Alice").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_get_retracted_properties_returns_retracted_triples() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "foundation:score", Object::Integer(42)),
    ], "test").unwrap();
    Individual::clear_property(&mut conn, "foundation:Alice", "foundation:score", "test").unwrap();

    let result = Individual::get_retracted_properties(&conn, "foundation:Alice").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].predicate, "foundation:score");
}

#[test]
fn test_get_retracted_properties_filters_metadata_predicates() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Alice", "rdfs:label", Object::Literal {
            value: "Alice".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("foundation:Alice", "rdfs:comment", Object::Literal {
            value: "A person".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("foundation:Alice", "foundation:score", Object::Integer(10)),
    ], "test").unwrap();

    Individual::clear_property(&mut conn, "foundation:Alice", "rdfs:label", "test").unwrap();
    Individual::clear_property(&mut conn, "foundation:Alice", "rdfs:comment", "test").unwrap();
    Individual::clear_property(&mut conn, "foundation:Alice", "foundation:score", "test").unwrap();

    let result = Individual::get_retracted_properties(&conn, "foundation:Alice").unwrap();
    let predicates: Vec<&str> = result.iter().map(|t| t.predicate.as_str()).collect();
    assert!(!predicates.contains(&"rdfs:label"), "rdfs:label must be filtered");
    assert!(!predicates.contains(&"rdfs:comment"), "rdfs:comment must be filtered");
    assert!(predicates.contains(&"foundation:score"), "foundation:score must be included");
}

#[test]
fn test_append_property_adds_without_removing_existing() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:Person");
    c.assert(&mut conn, ClassType::OwlClass, "Person", "https://example.com/person.svg", None, "test").unwrap();

    let prop = Property::new("foundation:tag");
    prop.assert(&mut conn, PropertyType::DatatypeProperty, "tag", None,
        &["foundation:Person"], Some("xsd:string"), None, "test").unwrap();

    let ind = Individual::new("foundation:alice");
    ind.assert(&mut conn, "foundation:Person", "Alice", "https://example.com/person.svg", "test").unwrap();

    ind.add_property(&mut conn, "foundation:tag", vec![
        Object::Literal { value: "first".to_string(), datatype: Some("xsd:string".to_string()), language: None },
    ], "test").unwrap();

    ind.append_property(&mut conn, "foundation:tag", vec![
        Object::Literal { value: "second".to_string(), datatype: Some("xsd:string".to_string()), language: None },
    ], "test").unwrap();

    let result = crate::eavto::query::get_by_entity_predicate(&conn, "foundation:alice", "foundation:tag").unwrap();
    assert_eq!(result.triples.len(), 2, "append_property must not remove existing values");
    let values: Vec<_> = result.triples.iter()
        .filter_map(|t| t.object.as_literal())
        .collect();
    assert!(values.iter().any(|v| v == "first"), "original value must remain");
    assert!(values.iter().any(|v| v == "second"), "new value must be added");
}

#[test]
fn test_add_property_replaces_existing_values() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:Person");
    c.assert(&mut conn, ClassType::OwlClass, "Person", "https://example.com/person.svg", None, "test").unwrap();

    let prop = Property::new("foundation:tag");
    prop.assert(&mut conn, PropertyType::DatatypeProperty, "tag", None,
        &["foundation:Person"], Some("xsd:string"), None, "test").unwrap();

    let ind = Individual::new("foundation:alice");
    ind.assert(&mut conn, "foundation:Person", "Alice", "https://example.com/person.svg", "test").unwrap();

    ind.add_property(&mut conn, "foundation:tag", vec![
        Object::Literal { value: "first".to_string(), datatype: Some("xsd:string".to_string()), language: None },
    ], "test").unwrap();

    ind.add_property(&mut conn, "foundation:tag", vec![
        Object::Literal { value: "replaced".to_string(), datatype: Some("xsd:string".to_string()), language: None },
    ], "test").unwrap();

    let result = crate::eavto::query::get_by_entity_predicate(&conn, "foundation:alice", "foundation:tag").unwrap();
    assert_eq!(result.triples.len(), 1, "add_property must replace all existing values");
    assert_eq!(result.triples[0].object.as_literal().unwrap(), "replaced");
}

// ── propertyType field in serializable_properties ────────────────────────────

#[test]
fn test_serializable_properties_property_type_value() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "test:Subject".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("foundation:name".to_string(), Object::Literal {
            value: "Alice".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["propertyType"], "value",
        "literal without ontology definition must be 'value'");
}

#[test]
fn test_serializable_properties_property_type_reference_builtin() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "test:Subject".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("rdf:type".to_string(), Object::Iri("foundation:MyClass".to_string()))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["propertyType"], "reference",
        "built-in IRI-valued property (rdf:type) must fall back to 'reference'");
}

#[test]
fn test_serializable_properties_property_type_reference_defined() {
    let mut conn = setup_test_db();

    Property::new("test:relProp").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "rel prop",
        None,
        &[],
        Some("test:Target"),
        None,
        "test",
    ).unwrap();

    let ind = Individual {
        iri: "test:Subject".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("test:relProp".to_string(), Object::Iri("test:Target_1".to_string()))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["propertyType"], "reference",
        "defined ObjectProperty without queryConfig must be 'reference'");
}

#[test]
fn test_serializable_properties_property_type_calculation() {
    let mut conn = setup_test_db();

    Property::new("test:calcProp").assert(
        &mut conn,
        PropertyType::DatatypeProperty,
        "calc prop",
        None,
        &[],
        Some("xsd:decimal"),
        Some("unit:Meter"),
        "test",
    ).unwrap();

    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
        [],
    ).unwrap();
    let tx_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES ('test:calcProp', 'foundation:formula', '{{test:other}} * 2', 'literal', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params![tx_id],
    ).unwrap();

    let ind = Individual {
        iri: "test:Subject".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("test:calcProp".to_string(), Object::Number(3.14))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["propertyType"], "calculation",
        "DatatypeProperty with formula must be 'calculation'");
}

#[test]
fn test_serializable_properties_property_type_query() {
    let mut conn = setup_test_db();

    Property::new("test:queryProp").assert(
        &mut conn,
        PropertyType::ObjectProperty,
        "query prop",
        None,
        &[],
        Some("test:Target"),
        None,
        "test",
    ).unwrap();

    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
        [],
    ).unwrap();
    let tx_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES ('test:queryProp', 'foundation:queryConfig', '{\"targetClass\":\"test:Target\",\"filters\":[]}', 'literal', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params![tx_id],
    ).unwrap();

    let ind = Individual {
        iri: "test:Subject".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![("test:queryProp".to_string(), Object::Iri("test:Target_1".to_string()))],
        property_tx: vec![0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    assert_eq!(props[0]["propertyType"], "query",
        "ObjectProperty with queryConfig must be 'query'");
}

#[test]
fn test_serializable_properties_property_type_always_present() {
    let conn = setup_test_db();

    let ind = Individual {
        iri: "test:Subject".to_string(),
        label: None, icon: None, comment: None, types: vec![],
        properties: vec![
            ("rdf:type".to_string(), Object::Iri("foundation:MyClass".to_string())),
            ("rdfs:label".to_string(), Object::Literal {
                value: "My Label".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            ("foundation:unknown".to_string(), Object::Integer(42)),
        ],
        property_tx: vec![0, 0, 0],
        backlinks: vec![],
    };

    let props = ind.serializable_properties(&conn);
    for entry in &props {
        let pt = entry.get("propertyType").and_then(|v| v.as_str()).unwrap_or("");
        assert!(!pt.is_empty(),
            "propertyType must be non-empty for every entry, got empty on property '{}'",
            entry["property"]);
        assert!(
            matches!(pt, "value" | "calculation" | "reference" | "query"),
            "propertyType must be one of the four canonical values, got '{}' on property '{}'",
            pt, entry["property"]);
    }
}

// Regression: Bug_1780923710393 — foundation:comment (non-existent property) produced a
// misleading "no domain defined" error instead of "Property not found".
#[test]
fn test_add_property_nonexistent_property_returns_not_found() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:AcceptanceCriterion");
    c.assert(&mut conn, ClassType::OwlClass, "AcceptanceCriterion", "https://example.com/ac.svg", None, "test").unwrap();

    let ind = Individual::new("foundation:AC_1");
    ind.assert(&mut conn, "foundation:AcceptanceCriterion", "AC 1", "https://example.com/ac.svg", "test").unwrap();

    let result = ind.add_property(
        &mut conn,
        "foundation:comment",
        vec![Object::Literal { value: "some text".to_string(), datatype: Some("xsd:string".to_string()), language: None }],
        "test",
    );

    assert!(result.is_err(), "Non-existent property must be rejected");
    if let Err(OwlError::NotFound(msg)) = &result {
        assert!(msg.contains("Property not found"), "Error must say 'Property not found', got: {}", msg);
        assert!(msg.contains("foundation:comment"), "Error must contain the property IRI");
        assert!(msg.contains("rdfs:comment"), "Error must suggest rdfs:comment for 'comment' local name");
        assert!(!msg.contains("no domain defined"), "Error must NOT contain the misleading 'no domain defined' text");
    } else {
        panic!("Expected NotFound error, got: {:?}", result);
    }
}

#[test]
fn test_append_property_nonexistent_property_returns_not_found() {
    let mut conn = setup_test_db();

    let c = Class::new("foundation:AcceptanceCriterion");
    c.assert(&mut conn, ClassType::OwlClass, "AcceptanceCriterion", "https://example.com/ac.svg", None, "test").unwrap();

    let ind = Individual::new("foundation:AC_1");
    ind.assert(&mut conn, "foundation:AcceptanceCriterion", "AC 1", "https://example.com/ac.svg", "test").unwrap();

    let result = ind.append_property(
        &mut conn,
        "foundation:comment",
        vec![Object::Literal { value: "some text".to_string(), datatype: Some("xsd:string".to_string()), language: None }],
        "test",
    );

    assert!(result.is_err(), "Non-existent property must be rejected in append_property");
    if let Err(OwlError::NotFound(msg)) = &result {
        assert!(msg.contains("Property not found"), "Error must say 'Property not found', got: {}", msg);
        assert!(msg.contains("rdfs:comment"), "Error must suggest rdfs:comment");
    } else {
        panic!("Expected NotFound error, got: {:?}", result);
    }
}

#[test]
fn test_add_property_existing_property_wrong_domain_keeps_original_message() {
    let mut conn = setup_test_db();

    let class_a = Class::new("foundation:ClassA");
    class_a.assert(&mut conn, ClassType::OwlClass, "ClassA", "https://example.com/a.svg", None, "test").unwrap();

    let class_b = Class::new("foundation:ClassB");
    class_b.assert(&mut conn, ClassType::OwlClass, "ClassB", "https://example.com/b.svg", None, "test").unwrap();

    let prop = Property::new("foundation:propForA");
    prop.assert(&mut conn, PropertyType::DatatypeProperty, "prop for A", None,
        &["foundation:ClassA"], Some("xsd:string"), None, "test").unwrap();

    let ind = Individual::new("foundation:B_1");
    ind.assert(&mut conn, "foundation:ClassB", "B 1", "https://example.com/b.svg", "test").unwrap();

    let result = ind.add_property(
        &mut conn,
        "foundation:propForA",
        vec![Object::Literal { value: "val".to_string(), datatype: Some("xsd:string".to_string()), language: None }],
        "test",
    );

    assert!(result.is_err(), "Property with wrong domain must be rejected");
    if let Err(OwlError::InvalidOperation(msg)) = &result {
        assert!(msg.contains("is not defined for"), "Error must contain 'is not defined for', got: {}", msg);
        assert!(!msg.contains("Property not found"), "Must NOT use the NotFound message for an existing property");
    } else {
        panic!("Expected InvalidOperation error, got: {:?}", result);
    }
}

#[test]
fn test_append_property_respects_max_cardinality() {
    use crate::owl::cardinality::{set_class_cardinality_restrictions, PropertyRestriction};

    let mut conn = setup_test_db();

    let c = Class::new("foundation:Task");
    c.assert(&mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test").unwrap();

    let prop = Property::new("foundation:tag");
    prop.assert(&mut conn, PropertyType::DatatypeProperty, "tag", None,
        &["foundation:Task"], Some("xsd:string"), None, "test").unwrap();

    set_class_cardinality_restrictions(&mut conn, "foundation:Task", &[
        PropertyRestriction { property_iri: "foundation:tag", min: None, max: Some(2) },
    ], "test").unwrap();

    let ind = Individual::new("foundation:task1");
    ind.assert(&mut conn, "foundation:Task", "Task 1", "https://example.com/task.svg", "test").unwrap();

    ind.append_property(&mut conn, "foundation:tag", vec![
        Object::Literal { value: "a".to_string(), datatype: Some("xsd:string".to_string()), language: None },
    ], "test").unwrap();

    ind.append_property(&mut conn, "foundation:tag", vec![
        Object::Literal { value: "b".to_string(), datatype: Some("xsd:string".to_string()), language: None },
    ], "test").unwrap();

    let result = ind.append_property(&mut conn, "foundation:tag", vec![
        Object::Literal { value: "c".to_string(), datatype: Some("xsd:string".to_string()), language: None },
    ], "test");

    assert!(result.is_err(), "Third append should fail due to maxCardinality=2");
}

// ── add_iri_value ─────────────────────────────────────────────────────────────

#[test]
fn test_add_iri_value_happy_path() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Alice", rdf::TYPE, Object::Iri("test:Person".to_string())),
    ], "test").unwrap();

    Individual::add_iri_value(&mut conn, "test:Alice", "test:knows", "test:Bob", "test").unwrap();

    let result = crate::owl::get_all_iri_properties(&conn, "test:Alice", "test:knows").unwrap();
    assert!(result.contains(&"test:Bob".to_string()));
}

#[test]
fn test_add_iri_value_is_idempotent() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Alice", rdf::TYPE, Object::Iri("test:Person".to_string())),
    ], "test").unwrap();

    Individual::add_iri_value(&mut conn, "test:Alice", "test:knows", "test:Bob", "test").unwrap();
    Individual::add_iri_value(&mut conn, "test:Alice", "test:knows", "test:Bob", "test").unwrap();

    let result = crate::owl::get_all_iri_properties(&conn, "test:Alice", "test:knows").unwrap();
    let bob_count = result.iter().filter(|v| v.as_str() == "test:Bob").count();
    assert_eq!(bob_count, 1, "duplicate add must not create a second IRI triple");
}

#[test]
fn test_add_iri_value_multi_value_accumulates() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Alice", rdf::TYPE, Object::Iri("test:Person".to_string())),
    ], "test").unwrap();

    Individual::add_iri_value(&mut conn, "test:Alice", "test:knows", "test:Bob", "test").unwrap();
    Individual::add_iri_value(&mut conn, "test:Alice", "test:knows", "test:Carol", "test").unwrap();

    let result = crate::owl::get_all_iri_properties(&conn, "test:Alice", "test:knows").unwrap();
    assert!(result.contains(&"test:Bob".to_string()));
    assert!(result.contains(&"test:Carol".to_string()));
}

// ── remove_iri_value ──────────────────────────────────────────────────────────

#[test]
fn test_remove_iri_value_removes_target() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Alice", rdf::TYPE, Object::Iri("test:Person".to_string())),
        Triple::new("test:Alice", "test:knows", Object::Iri("test:Bob".to_string())),
        Triple::new("test:Alice", "test:knows", Object::Iri("test:Carol".to_string())),
    ], "test").unwrap();

    Individual::remove_iri_value(&mut conn, "test:Alice", "test:knows", "test:Bob", "test").unwrap();

    let result = crate::owl::get_all_iri_properties(&conn, "test:Alice", "test:knows").unwrap();
    assert!(!result.contains(&"test:Bob".to_string()), "removed IRI must not appear in current view");
    assert!(result.contains(&"test:Carol".to_string()), "other IRI must remain");
}

#[test]
fn test_remove_iri_value_is_immutable_history_preserved() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Alice", rdf::TYPE, Object::Iri("test:Person".to_string())),
        Triple::new("test:Alice", "test:knows", Object::Iri("test:Bob".to_string())),
    ], "test").unwrap();

    Individual::remove_iri_value(&mut conn, "test:Alice", "test:knows", "test:Bob", "test").unwrap();

    let conn_ref: &rusqlite::Connection = &conn;
    let hist_count: i64 = conn_ref.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='test:Alice' AND predicate='test:knows' AND object='test:Bob'",
        [], |r| r.get(0),
    ).unwrap();
    assert!(hist_count >= 1, "historical row must still exist after retraction");
}

#[test]
fn test_remove_iri_value_noop_when_not_present() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Alice", rdf::TYPE, Object::Iri("test:Person".to_string())),
    ], "test").unwrap();

    let result = Individual::remove_iri_value(&mut conn, "test:Alice", "test:knows", "test:Ghost", "test");
    assert!(result.is_ok(), "removing non-existent IRI must not error");
}

// ── status transition via replace_all_property_iris (single-valued) ──────────

#[test]
fn test_status_transition_leaves_exactly_one_status() {
    let mut conn = setup_test_db();
    store::assert_triples(&mut conn, &[
        Triple::new("test:Job", rdf::TYPE, Object::Iri("test:WorkItem".to_string())),
    ], "test").unwrap();

    Individual::add_iri_value(&mut conn, "test:Job", "foundation:hasStatus", "foundation:InProgress", "test").unwrap();

    crate::owl::replace_all_property_iris(&mut conn, "test:Job", "foundation:hasStatus", &["foundation:Completed"], "test").unwrap();

    let statuses = crate::owl::get_all_iri_properties(&conn, "test:Job", "foundation:hasStatus").unwrap();
    assert_eq!(statuses.len(), 1, "exactly one hasStatus must remain after transition");
    assert_eq!(statuses[0], "foundation:Completed", "status must be Completed after transition");
}
