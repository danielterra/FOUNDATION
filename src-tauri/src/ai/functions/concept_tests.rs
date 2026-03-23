use super::{define_class_one, retract_class_one};
use crate::eavto::{store, Triple, Object};
use crate::eavto::test_helpers::setup_test_db;

#[test]
fn test_update_concept_required_fields_rejects_nonexistent_property() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "property_restrictions": [{"property_iri": "foundation:nonExistent", "cardinality_min": 1}]
    });

    let result = define_class_one(&mut conn, &args);

    assert!(!result.success);
    let error = result.error.unwrap();
    assert!(
        error.contains("foundation:nonExistent") && error.contains("not defined in this ontology"),
        "Expected error about undefined property, got: {}",
        error
    );
}

#[test]
fn test_update_concept_required_fields_accepts_valid_datatype_property() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:myProp", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
    ], "test").unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "property_restrictions": [{"property_iri": "foundation:myProp", "cardinality_min": 1}]
    });

    let result = define_class_one(&mut conn, &args);
    assert!(result.success, "Expected success, got error: {:?}", result.error);
}

#[test]
fn test_update_concept_required_fields_accepts_valid_object_property() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:myRef", "rdf:type", Object::Iri("owl:ObjectProperty".to_string())),
    ], "test").unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "property_restrictions": [{"property_iri": "foundation:myRef", "cardinality_min": 1}]
    });

    let result = define_class_one(&mut conn, &args);
    assert!(result.success, "Expected success, got error: {:?}", result.error);
}

#[test]
fn test_required_fields_can_reference_property_in_upsert_details() {
    let mut conn = setup_test_db();

    crate::owl::Property::new("foundation:newProp")
        .assert(&mut conn, crate::owl::PropertyType::DatatypeProperty, "new prop", None, &[], Some("xsd:string"), None, "test")
        .unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "add_properties": ["foundation:newProp"],
        "property_restrictions": [{"property_iri": "foundation:newProp", "cardinality_min": 1}]
    });

    let result = define_class_one(&mut conn, &args);
    assert!(result.success, "Expected success when property_restriction is in add_properties, got error: {:?}", result.error);
}

#[test]
fn test_required_fields_can_reference_connection_in_upsert_details() {
    let mut conn = setup_test_db();

    crate::owl::Property::new("foundation:newRef")
        .assert(&mut conn, crate::owl::PropertyType::ObjectProperty, "new ref", None, &[], Some("foundation:TargetClass"), None, "test")
        .unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:TargetClass", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "add_properties": ["foundation:newRef"],
        "property_restrictions": [{"property_iri": "foundation:newRef", "cardinality_min": 1}]
    });

    let result = define_class_one(&mut conn, &args);
    assert!(result.success, "Expected success when property_restriction is a connection in add_properties, got error: {:?}", result.error);
}

#[test]
fn test_allowed_statuses_rejects_nonexistent_status_iri() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "allowed_statuses": ["foundation:Status_inactive"]
    });

    let result = define_class_one(&mut conn, &args);

    assert!(!result.success);
    let error = result.error.unwrap();
    assert!(
        error.contains("foundation:Status_inactive") && error.contains("does not exist"),
        "Expected error about non-existent status, got: {}",
        error
    );
}

#[test]
fn test_allowed_statuses_rejects_status_without_icon() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:StatusNoIcon", "rdfs:label", Object::Literal {
            value: "No Icon Status".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "allowed_statuses": ["foundation:StatusNoIcon"]
    });

    let result = define_class_one(&mut conn, &args);

    assert!(!result.success);
    let error = result.error.unwrap();
    assert!(
        error.contains("foundation:StatusNoIcon") && error.contains("no icon"),
        "Expected error about missing icon, got: {}",
        error
    );
}

#[test]
fn test_allowed_statuses_accepts_valid_status_with_icon() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:StatusWithIcon", "rdfs:label", Object::Literal {
            value: "Active".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:StatusWithIcon", "foundation:hasIcon", Object::Iri(crate::owl::icon_name_to_iri("check_circle"))),
    ], "test").unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "allowed_statuses": ["foundation:StatusWithIcon"]
    });

    let result = define_class_one(&mut conn, &args);
    assert!(result.success, "Expected success, got error: {:?}", result.error);
}

// ── remove_properties ────────────────────────────────────────────────────

fn setup_two_concepts_with_shared_property(conn: &mut crate::eavto::Connection) {
    crate::owl::Property::new("foundation:sharedProp")
        .assert(conn, crate::owl::PropertyType::DatatypeProperty, "Shared Prop", None, &[], Some("xsd:string"), None, "test")
        .unwrap();

    store::assert_triples(conn, &[
        Triple::new("foundation:ConceptA", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:ConceptA", "rdfs:label", Object::Literal {
            value: "Concept A".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("foundation:ConceptB", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:ConceptB", "rdfs:label", Object::Literal {
            value: "Concept B".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    define_class_one(conn, &serde_json::json!({"iri": "foundation:ConceptA", "add_properties": ["foundation:sharedProp"]}));
    define_class_one(conn, &serde_json::json!({"iri": "foundation:ConceptB", "add_properties": ["foundation:sharedProp"]}));
}

#[test]
fn test_remove_details_with_other_domains_preserves_property() {
    let mut conn = setup_test_db();
    setup_two_concepts_with_shared_property(&mut conn);

    let result = define_class_one(&mut conn, &serde_json::json!({
        "iri": "foundation:ConceptA",
        "remove_properties": ["foundation:sharedProp"]
    }));
    assert!(result.success, "remove_properties should succeed: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:sharedProp").unwrap();
    assert!(prop.is_some(), "property must still exist (has other domain)");
    let domains = prop.unwrap().domains;
    assert!(!domains.contains(&"foundation:ConceptA".to_string()), "ConceptA must be removed from domains");
    assert!(domains.contains(&"foundation:ConceptB".to_string()), "ConceptB domain must be preserved");
}

#[test]
fn test_remove_details_last_domain_deletes_property() {
    let mut conn = setup_test_db();

    crate::owl::Property::new("foundation:singleDomainProp")
        .assert(&mut conn, crate::owl::PropertyType::DatatypeProperty, "Single Domain Prop", None, &[], Some("xsd:string"), None, "test")
        .unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:OnlyOwner", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:OnlyOwner", "rdfs:label", Object::Literal {
            value: "Only Owner".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    define_class_one(&mut conn, &serde_json::json!({"iri": "foundation:OnlyOwner", "add_properties": ["foundation:singleDomainProp"]}));

    let result = define_class_one(&mut conn, &serde_json::json!({
        "iri": "foundation:OnlyOwner",
        "remove_properties": ["foundation:singleDomainProp"]
    }));
    assert!(result.success, "remove_properties should succeed: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:singleDomainProp").unwrap();
    assert!(prop.is_none(), "property must be deleted when it has no remaining domains");
}

#[test]
fn test_remove_details_nonexistent_property_is_ignored() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:SomeConcept", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:SomeConcept", "rdfs:label", Object::Literal {
            value: "Some Concept".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let result = define_class_one(&mut conn, &serde_json::json!({
        "iri": "foundation:SomeConcept",
        "remove_properties": ["foundation:doesNotExist"]
    }));
    assert!(result.success, "remove_properties with nonexistent property must succeed silently");
}

#[test]
fn test_forget_concept_rejected_when_subclasses_exist() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let mut conn = setup_test_db();

    let create_parent = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Animal",
                "label": "Animal",
                "icon": "pets",
                "super_classes": ["owl:Thing"]
            }]
        }),
    };
    assert!(execute_tool(&mut conn, &create_parent, None, None).success);

    let create_child = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Dog",
                "label": "Dog",
                "icon": "pets",
                "super_classes": ["foundation:Animal"]
            }]
        }),
    };
    assert!(execute_tool(&mut conn, &create_child, None, None).success);

    let delete_call = ToolCall {
        name: "retract_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:Animal"}]
        }),
    };
    let result = execute_tool(&mut conn, &delete_call, None, None);
    assert!(!result.success, "retracting a class with subclasses must be rejected");
    let err = result.error.unwrap();
    assert!(err.contains("foundation:Dog"), "error must mention the dependent subclass; got: {err}");
}

#[test]
fn test_forget_concept_allowed_when_no_subclasses() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let mut conn = setup_test_db();

    let create = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Leaf",
                "label": "Leaf",
                "icon": "eco",
                "super_classes": ["owl:Thing"]
            }]
        }),
    };
    assert!(execute_tool(&mut conn, &create, None, None).success);

    let delete_call = ToolCall {
        name: "retract_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:Leaf"}]
        }),
    };
    let result = execute_tool(&mut conn, &delete_call, None, None);
    assert!(result.success, "retracting a leaf class must succeed; got: {:?}", result.error);
}

#[test]
fn test_forget_concept_cascades_deletes_instances() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Widget", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:Widget", "rdfs:label", Object::Literal {
            value: "Widget".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("foundation:Widget_1", "rdf:type", Object::Iri("foundation:Widget".to_string())),
        Triple::new("foundation:Widget_1", "rdfs:label", Object::Literal {
            value: "First Widget".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("foundation:Widget_2", "rdf:type", Object::Iri("foundation:Widget".to_string())),
        Triple::new("foundation:Widget_2", "rdfs:label", Object::Literal {
            value: "Second Widget".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let result = retract_class_one(&mut conn, &serde_json::json!({"iri": "foundation:Widget"}));
    assert!(result.success, "cascade retract must succeed; got: {:?}", result.error);

    let deleted = result.result.unwrap();
    assert_eq!(deleted["deleted_instances"], 2, "must report 2 deleted instances");

    let class = crate::owl::Class::get(&conn, "foundation:Widget").unwrap();
    assert!(class.is_none(), "class must be gone after retraction");

    let inst1 = crate::owl::Individual::get(&conn, "foundation:Widget_1").unwrap();
    assert!(inst1.is_none(), "instance 1 must be deleted");
    let inst2 = crate::owl::Individual::get(&conn, "foundation:Widget_2").unwrap();
    assert!(inst2.is_none(), "instance 2 must be deleted");
}

#[test]
fn test_forget_concept_no_instances_returns_zero_deleted() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:EmptyConcept", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:EmptyConcept", "rdfs:label", Object::Literal {
            value: "Empty Concept".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let result = retract_class_one(&mut conn, &serde_json::json!({"iri": "foundation:EmptyConcept"}));
    assert!(result.success, "retracting class with no instances must succeed; got: {:?}", result.error);

    let deleted = result.result.unwrap();
    assert_eq!(deleted["deleted_instances"], 0, "must report 0 deleted instances");
}

#[test]
fn test_update_concept_super_concepts_empty_array_is_rejected() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let mut conn = setup_test_db();

    let create = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Widget",
                "label": "Widget",
                "icon": "widgets",
                "super_classes": ["owl:Thing"]
            }]
        }),
    };
    assert!(execute_tool(&mut conn, &create, None, None).success);

    let update = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Widget",
                "super_classes": []
            }]
        }),
    };
    let result = execute_tool(&mut conn, &update, None, None);
    assert!(!result.success, "setting super_classes to empty must be rejected");
}

#[test]
fn test_rename_concept_migrates_all_references() {
    use crate::ai::functions::{ToolCall, execute_tool};
    use crate::eavto::query;

    let mut conn = setup_test_db();

    let create_concept = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:OldName",
                "label": "Old Name",
                "icon": "label",
                "super_classes": ["owl:Thing"]
            }]
        }),
    };
    assert!(execute_tool(&mut conn, &create_concept, None, None).success);

    // Property with OldName as domain
    crate::owl::Property::new("foundation:testProp")
        .assert(&mut conn, crate::owl::PropertyType::DatatypeProperty, "test prop", None, &["foundation:OldName"], Some("xsd:string"), None, "test")
        .unwrap();

    // Property pointing to OldName as range
    crate::owl::Property::new("foundation:refToOld")
        .assert(&mut conn, crate::owl::PropertyType::ObjectProperty, "ref to old", None, &[], Some("foundation:OldName"), None, "test")
        .unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Instance_1", "rdf:type", Object::Iri("foundation:OldName".to_string())),
    ], "test").unwrap();

    let create_sub = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:SubOfOld",
                "label": "Sub Of Old",
                "icon": "label",
                "super_classes": ["foundation:OldName"]
            }]
        }),
    };
    assert!(execute_tool(&mut conn, &create_sub, None, None).success);

    let rename = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:OldName",
                "new_iri": "foundation:NewName"
            }]
        }),
    };
    let result = execute_tool(&mut conn, &rename, None, None);
    assert!(result.success, "rename must succeed; got: {:?}", result.error);

    let old_class = crate::owl::Class::get(&conn, "foundation:OldName").unwrap();
    assert!(old_class.is_none(), "old IRI must be gone after rename");

    let new_class = crate::owl::Class::get(&conn, "foundation:NewName").unwrap();
    assert!(new_class.is_some(), "new IRI must exist after rename");

    let prop = crate::owl::Property::get(&conn, "foundation:testProp").unwrap().unwrap();
    assert!(prop.domains.contains(&"foundation:NewName".to_string()), "domain must be updated to new IRI");
    assert!(!prop.domains.contains(&"foundation:OldName".to_string()), "old IRI must not remain in domain");

    let ref_prop = crate::owl::Property::get(&conn, "foundation:refToOld").unwrap().unwrap();
    assert!(ref_prop.ranges.contains(&"foundation:NewName".to_string()), "range must be updated to new IRI");

    let type_triples = query::get_by_entity_predicate(&conn, "foundation:Instance_1", "rdf:type").unwrap();
    let has_new_type = type_triples.triples.iter().any(|t| t.object.as_iri() == Some("foundation:NewName"));
    assert!(has_new_type, "instance type must be updated to new IRI");

    let sub_class = crate::owl::Class::get(&conn, "foundation:SubOfOld").unwrap().unwrap();
    assert!(
        sub_class.super_classes.iter().any(|t| t.iri == "foundation:NewName"),
        "subclass superclass must be updated to new IRI"
    );
}

#[test]
fn test_rename_concept_rejects_nonexistent_source() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let mut conn = setup_test_db();

    let rename = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:DoesNotExist",
                "new_iri": "foundation:SomeTarget"
            }]
        }),
    };
    let result = execute_tool(&mut conn, &rename, None, None);
    assert!(!result.success, "rename of non-existent class must fail");
}

#[test]
fn test_rename_concept_rejects_existing_target() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let mut conn = setup_test_db();

    for iri in &["foundation:SourceConcept", "foundation:TargetConcept"] {
        let create = ToolCall {
            name: "define_class".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": iri,
                    "label": iri,
                    "icon": "label",
                    "super_classes": ["owl:Thing"]
                }]
            }),
        };
        assert!(execute_tool(&mut conn, &create, None, None).success);
    }

    let rename = ToolCall {
        name: "define_class".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:SourceConcept",
                "new_iri": "foundation:TargetConcept"
            }]
        }),
    };
    let result = execute_tool(&mut conn, &rename, None, None);
    assert!(!result.success, "rename to existing IRI must fail");
}
