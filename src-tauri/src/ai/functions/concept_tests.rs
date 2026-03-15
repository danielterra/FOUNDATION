use super::learn_concept_one;
use crate::eavto::{store, Triple, Object};
use crate::eavto::test_helpers::setup_test_db;

#[tokio::test]
async fn test_update_concept_required_fields_rejects_nonexistent_property() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "required_fields": ["foundation:nonExistent"]
    });

    let result = learn_concept_one(&conn, &args).await;

    assert!(!result.success);
    let error = result.error.unwrap();
    assert!(
        error.contains("foundation:nonExistent") && error.contains("not defined in this ontology"),
        "Expected error about undefined property, got: {}",
        error
    );
}

#[tokio::test]
async fn test_update_concept_required_fields_accepts_valid_datatype_property() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:myProp", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "required_fields": ["foundation:myProp"]
    });

    let result = learn_concept_one(&conn, &args).await;
    assert!(result.success, "Expected success, got error: {:?}", result.error);
}

#[tokio::test]
async fn test_update_concept_required_fields_accepts_valid_object_property() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:myRef", "rdf:type", Object::Iri("owl:ObjectProperty".to_string())),
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "required_fields": ["foundation:myRef"]
    });

    let result = learn_concept_one(&conn, &args).await;
    assert!(result.success, "Expected success, got error: {:?}", result.error);
}

#[tokio::test]
async fn test_required_fields_can_reference_property_in_upsert_details() {
    let conn = setup_test_db().await;

    crate::owl::Property::new("foundation:newProp")
        .assert(&conn, crate::owl::PropertyType::DatatypeProperty, "new prop", None, &[], Some("xsd:string"), None, "test")
        .await.unwrap();

    store::assert_triples(&conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "upsert_details": ["foundation:newProp"],
        "required_fields": ["foundation:newProp"]
    });

    let result = learn_concept_one(&conn, &args).await;
    assert!(result.success, "Expected success when required_field is in upsert_details, got error: {:?}", result.error);
}

#[tokio::test]
async fn test_required_fields_can_reference_connection_in_upsert_details() {
    let conn = setup_test_db().await;

    crate::owl::Property::new("foundation:newRef")
        .assert(&conn, crate::owl::PropertyType::ObjectProperty, "new ref", None, &[], Some("foundation:TargetClass"), None, "test")
        .await.unwrap();

    store::assert_triples(&conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:TargetClass", "rdf:type", Object::Iri("owl:Class".to_string())),
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "upsert_details": ["foundation:newRef"],
        "required_fields": ["foundation:newRef"]
    });

    let result = learn_concept_one(&conn, &args).await;
    assert!(result.success, "Expected success when required_field is a connection in upsert_details, got error: {:?}", result.error);
}

#[tokio::test]
async fn test_allowed_statuses_rejects_nonexistent_status_iri() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
        Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:TestClass", "rdfs:label", Object::Literal {
            value: "Test Class".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "allowed_statuses": ["foundation:Status_inactive"]
    });

    let result = learn_concept_one(&conn, &args).await;

    assert!(!result.success);
    let error = result.error.unwrap();
    assert!(
        error.contains("foundation:Status_inactive") && error.contains("does not exist"),
        "Expected error about non-existent status, got: {}",
        error
    );
}

#[tokio::test]
async fn test_allowed_statuses_rejects_status_without_icon() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
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
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "allowed_statuses": ["foundation:StatusNoIcon"]
    });

    let result = learn_concept_one(&conn, &args).await;

    assert!(!result.success);
    let error = result.error.unwrap();
    assert!(
        error.contains("foundation:StatusNoIcon") && error.contains("no icon"),
        "Expected error about missing icon, got: {}",
        error
    );
}

#[tokio::test]
async fn test_allowed_statuses_accepts_valid_status_with_icon() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
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
        Triple::new("foundation:StatusWithIcon", "foundation:icon", Object::Literal {
            value: "check_circle".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").await.unwrap();

    let args = serde_json::json!({
        "iri": "foundation:TestClass",
        "allowed_statuses": ["foundation:StatusWithIcon"]
    });

    let result = learn_concept_one(&conn, &args).await;
    assert!(result.success, "Expected success, got error: {:?}", result.error);
}

// ── remove_details ───────────────────────────────────────────────────────

async fn setup_two_concepts_with_shared_property(conn: &crate::eavto::Connection) {
    crate::owl::Property::new("foundation:sharedProp")
        .assert(conn, crate::owl::PropertyType::DatatypeProperty, "Shared Prop", None, &[], Some("xsd:string"), None, "test")
        .await.unwrap();

    store::assert_triples(conn, &[
        Triple::new("foundation:ConceptA", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:ConceptA", "rdfs:label", Object::Literal {
            value: "Concept A".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("foundation:ConceptB", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:ConceptB", "rdfs:label", Object::Literal {
            value: "Concept B".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").await.unwrap();

    learn_concept_one(conn, &serde_json::json!({"iri": "foundation:ConceptA", "upsert_details": ["foundation:sharedProp"]})).await;
    learn_concept_one(conn, &serde_json::json!({"iri": "foundation:ConceptB", "upsert_details": ["foundation:sharedProp"]})).await;
}

#[tokio::test]
async fn test_remove_details_with_other_domains_preserves_property() {
    let conn = setup_test_db().await;
    setup_two_concepts_with_shared_property(&conn).await;

    let result = learn_concept_one(&conn, &serde_json::json!({
        "iri": "foundation:ConceptA",
        "remove_details": ["foundation:sharedProp"]
    })).await;
    assert!(result.success, "remove_details should succeed: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:sharedProp").await.unwrap();
    assert!(prop.is_some(), "property must still exist (has other domain)");
    let domains = prop.unwrap().domains;
    assert!(!domains.contains(&"foundation:ConceptA".to_string()), "ConceptA must be removed from domains");
    assert!(domains.contains(&"foundation:ConceptB".to_string()), "ConceptB domain must be preserved");
}

#[tokio::test]
async fn test_remove_details_last_domain_deletes_property() {
    let conn = setup_test_db().await;

    crate::owl::Property::new("foundation:singleDomainProp")
        .assert(&conn, crate::owl::PropertyType::DatatypeProperty, "Single Domain Prop", None, &[], Some("xsd:string"), None, "test")
        .await.unwrap();

    store::assert_triples(&conn, &[
        Triple::new("foundation:OnlyOwner", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:OnlyOwner", "rdfs:label", Object::Literal {
            value: "Only Owner".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").await.unwrap();

    learn_concept_one(&conn, &serde_json::json!({"iri": "foundation:OnlyOwner", "upsert_details": ["foundation:singleDomainProp"]})).await;

    let result = learn_concept_one(&conn, &serde_json::json!({
        "iri": "foundation:OnlyOwner",
        "remove_details": ["foundation:singleDomainProp"]
    })).await;
    assert!(result.success, "remove_details should succeed: {:?}", result.error);

    let prop = crate::owl::Property::get(&conn, "foundation:singleDomainProp").await.unwrap();
    assert!(prop.is_none(), "property must be deleted when it has no remaining domains");
}

#[tokio::test]
async fn test_remove_details_nonexistent_property_is_ignored() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
        Triple::new("foundation:SomeConcept", "rdf:type", Object::Iri("owl:Class".to_string())),
        Triple::new("foundation:SomeConcept", "rdfs:label", Object::Literal {
            value: "Some Concept".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").await.unwrap();

    let result = learn_concept_one(&conn, &serde_json::json!({
        "iri": "foundation:SomeConcept",
        "remove_details": ["foundation:doesNotExist"]
    })).await;
    assert!(result.success, "remove_details with nonexistent property must succeed silently");
}

#[tokio::test]
async fn test_forget_concept_rejected_when_subclasses_exist() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let conn = setup_test_db().await;

    let create_parent = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Animal",
                "label": "Animal",
                "icon": "pets",
                "super_concepts": ["owl:Thing"]
            }]
        }),
    };
    assert!(execute_tool(&conn, &create_parent, None).await.success);

    let create_child = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Dog",
                "label": "Dog",
                "icon": "pets",
                "super_concepts": ["foundation:Animal"]
            }]
        }),
    };
    assert!(execute_tool(&conn, &create_child, None).await.success);

    let delete_call = ToolCall {
        name: "forget_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:Animal"}]
        }),
    };
    let result = execute_tool(&conn, &delete_call, None).await;
    assert!(!result.success, "deleting a concept with subclasses must be rejected");
    let err = result.error.unwrap();
    assert!(err.contains("foundation:Dog"), "error must mention the dependent subclass; got: {err}");
}

#[tokio::test]
async fn test_forget_concept_allowed_when_no_subclasses() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let conn = setup_test_db().await;

    let create = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Leaf",
                "label": "Leaf",
                "icon": "eco",
                "super_concepts": ["owl:Thing"]
            }]
        }),
    };
    assert!(execute_tool(&conn, &create, None).await.success);

    let delete_call = ToolCall {
        name: "forget_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{"iri": "foundation:Leaf"}]
        }),
    };
    let result = execute_tool(&conn, &delete_call, None).await;
    assert!(result.success, "deleting a leaf concept must succeed; got: {:?}", result.error);
}

#[tokio::test]
async fn test_update_concept_super_concepts_empty_array_is_rejected() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let conn = setup_test_db().await;

    let create = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Widget",
                "label": "Widget",
                "icon": "widgets",
                "super_concepts": ["owl:Thing"]
            }]
        }),
    };
    assert!(execute_tool(&conn, &create, None).await.success);

    let update = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:Widget",
                "super_concepts": []
            }]
        }),
    };
    let result = execute_tool(&conn, &update, None).await;
    assert!(!result.success, "setting super_concepts to empty must be rejected");
}

#[tokio::test]
async fn test_rename_concept_migrates_all_references() {
    use crate::ai::functions::{ToolCall, execute_tool};
    use crate::eavto::query;

    let conn = setup_test_db().await;

    let create_concept = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:OldName",
                "label": "Old Name",
                "icon": "label",
                "super_concepts": ["owl:Thing"]
            }]
        }),
    };
    assert!(execute_tool(&conn, &create_concept, None).await.success);

    crate::owl::Property::new("foundation:testProp")
        .assert(&conn, crate::owl::PropertyType::DatatypeProperty, "test prop", None, &["foundation:OldName"], Some("xsd:string"), None, "test")
        .await.unwrap();

    crate::owl::Property::new("foundation:refToOld")
        .assert(&conn, crate::owl::PropertyType::ObjectProperty, "ref to old", None, &[], Some("foundation:OldName"), None, "test")
        .await.unwrap();

    store::assert_triples(&conn, &[
        Triple::new("foundation:Instance_1", "rdf:type", Object::Iri("foundation:OldName".to_string())),
    ], "test").await.unwrap();

    let create_sub = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:SubOfOld",
                "label": "Sub Of Old",
                "icon": "label",
                "super_concepts": ["foundation:OldName"]
            }]
        }),
    };
    assert!(execute_tool(&conn, &create_sub, None).await.success);

    let rename = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:OldName",
                "new_iri": "foundation:NewName"
            }]
        }),
    };
    let result = execute_tool(&conn, &rename, None).await;
    assert!(result.success, "rename must succeed; got: {:?}", result.error);

    let old_class = crate::owl::Class::get(&conn, "foundation:OldName").await.unwrap();
    assert!(old_class.is_none(), "old IRI must be gone after rename");

    let new_class = crate::owl::Class::get(&conn, "foundation:NewName").await.unwrap();
    assert!(new_class.is_some(), "new IRI must exist after rename");

    let prop = crate::owl::Property::get(&conn, "foundation:testProp").await.unwrap().unwrap();
    assert!(prop.domains.contains(&"foundation:NewName".to_string()), "domain must be updated to new IRI");
    assert!(!prop.domains.contains(&"foundation:OldName".to_string()), "old IRI must not remain in domain");

    let ref_prop = crate::owl::Property::get(&conn, "foundation:refToOld").await.unwrap().unwrap();
    assert!(ref_prop.ranges.contains(&"foundation:NewName".to_string()), "range must be updated to new IRI");

    let type_triples = query::get_by_entity_predicate(&conn, "foundation:Instance_1", "rdf:type").await.unwrap();
    let has_new_type = type_triples.triples.iter().any(|t| t.object.as_iri() == Some("foundation:NewName"));
    assert!(has_new_type, "instance type must be updated to new IRI");

    let sub_class = crate::owl::Class::get(&conn, "foundation:SubOfOld").await.unwrap().unwrap();
    assert!(
        sub_class.super_classes.iter().any(|t| t.iri == "foundation:NewName"),
        "subclass superclass must be updated to new IRI"
    );
}

#[tokio::test]
async fn test_rename_concept_rejects_nonexistent_source() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let conn = setup_test_db().await;

    let rename = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:DoesNotExist",
                "new_iri": "foundation:SomeTarget"
            }]
        }),
    };
    let result = execute_tool(&conn, &rename, None).await;
    assert!(!result.success, "rename of non-existent concept must fail");
}

#[tokio::test]
async fn test_rename_concept_rejects_existing_target() {
    use crate::ai::functions::{ToolCall, execute_tool};

    let conn = setup_test_db().await;

    for iri in &["foundation:SourceConcept", "foundation:TargetConcept"] {
        let create = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": iri,
                    "label": iri,
                    "icon": "label",
                    "super_concepts": ["owl:Thing"]
                }]
            }),
        };
        assert!(execute_tool(&conn, &create, None).await.success);
    }

    let rename = ToolCall {
        name: "learn_concepts".to_string(),
        arguments: serde_json::json!({
            "operations": [{
                "iri": "foundation:SourceConcept",
                "new_iri": "foundation:TargetConcept"
            }]
        }),
    };
    let result = execute_tool(&conn, &rename, None).await;
    assert!(!result.success, "rename to existing IRI must fail");
}
