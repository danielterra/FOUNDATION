use serde_json::Value;
use rusqlite::Connection;
use crate::owl::Class;
use super::ToolResult;

#[cfg(test)]
mod tests {
    use super::learn_concept_one;
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
            "required_fields": ["foundation:nonExistent"]
        });

        let result = learn_concept_one(&mut conn, &args);

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
            "required_fields": ["foundation:myProp"]
        });

        let result = learn_concept_one(&mut conn, &args);
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
            "required_fields": ["foundation:myRef"]
        });

        let result = learn_concept_one(&mut conn, &args);
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
            "upsert_details": ["foundation:newProp"],
            "required_fields": ["foundation:newProp"]
        });

        let result = learn_concept_one(&mut conn, &args);
        assert!(result.success, "Expected success when required_field is in upsert_details, got error: {:?}", result.error);
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
            "upsert_details": ["foundation:newRef"],
            "required_fields": ["foundation:newRef"]
        });

        let result = learn_concept_one(&mut conn, &args);
        assert!(result.success, "Expected success when required_field is a connection in upsert_details, got error: {:?}", result.error);
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

        let result = learn_concept_one(&mut conn, &args);

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

        let result = learn_concept_one(&mut conn, &args);

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
            Triple::new("foundation:StatusWithIcon", "foundation:icon", Object::Literal {
                value: "check_circle".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let args = serde_json::json!({
            "iri": "foundation:TestClass",
            "allowed_statuses": ["foundation:StatusWithIcon"]
        });

        let result = learn_concept_one(&mut conn, &args);
        assert!(result.success, "Expected success, got error: {:?}", result.error);
    }

    // ── remove_details ───────────────────────────────────────────────────────

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

        learn_concept_one(conn, &serde_json::json!({"iri": "foundation:ConceptA", "upsert_details": ["foundation:sharedProp"]}));
        learn_concept_one(conn, &serde_json::json!({"iri": "foundation:ConceptB", "upsert_details": ["foundation:sharedProp"]}));
    }

    #[test]
    fn test_remove_details_with_other_domains_preserves_property() {
        let mut conn = setup_test_db();
        setup_two_concepts_with_shared_property(&mut conn);

        let result = learn_concept_one(&mut conn, &serde_json::json!({
            "iri": "foundation:ConceptA",
            "remove_details": ["foundation:sharedProp"]
        }));
        assert!(result.success, "remove_details should succeed: {:?}", result.error);

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

        learn_concept_one(&mut conn, &serde_json::json!({"iri": "foundation:OnlyOwner", "upsert_details": ["foundation:singleDomainProp"]}));

        let result = learn_concept_one(&mut conn, &serde_json::json!({
            "iri": "foundation:OnlyOwner",
            "remove_details": ["foundation:singleDomainProp"]
        }));
        assert!(result.success, "remove_details should succeed: {:?}", result.error);

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

        let result = learn_concept_one(&mut conn, &serde_json::json!({
            "iri": "foundation:SomeConcept",
            "remove_details": ["foundation:doesNotExist"]
        }));
        assert!(result.success, "remove_details with nonexistent property must succeed silently");
    }

    #[test]
    fn test_forget_concept_rejected_when_subclasses_exist() {
        use crate::ai::functions::{ToolCall, execute_tool};

        let mut conn = setup_test_db();

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
        assert!(execute_tool(&mut conn, &create_parent, None).success);

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
        assert!(execute_tool(&mut conn, &create_child, None).success);

        let delete_call = ToolCall {
            name: "forget_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{"iri": "foundation:Animal"}]
            }),
        };
        let result = execute_tool(&mut conn, &delete_call, None);
        assert!(!result.success, "deleting a concept with subclasses must be rejected");
        let err = result.error.unwrap();
        assert!(err.contains("foundation:Dog"), "error must mention the dependent subclass; got: {err}");
    }

    #[test]
    fn test_forget_concept_allowed_when_no_subclasses() {
        use crate::ai::functions::{ToolCall, execute_tool};

        let mut conn = setup_test_db();

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
        assert!(execute_tool(&mut conn, &create, None).success);

        let delete_call = ToolCall {
            name: "forget_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{"iri": "foundation:Leaf"}]
            }),
        };
        let result = execute_tool(&mut conn, &delete_call, None);
        assert!(result.success, "deleting a leaf concept must succeed; got: {:?}", result.error);
    }

    #[test]
    fn test_update_concept_super_concepts_empty_array_is_rejected() {
        use crate::ai::functions::{ToolCall, execute_tool};

        let mut conn = setup_test_db();

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
        assert!(execute_tool(&mut conn, &create, None).success);

        let update = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:Widget",
                    "super_concepts": []
                }]
            }),
        };
        let result = execute_tool(&mut conn, &update, None);
        assert!(!result.success, "setting super_concepts to empty must be rejected");
    }

    #[test]
    fn test_rename_concept_migrates_all_references() {
        use crate::ai::functions::{ToolCall, execute_tool};
        use crate::eavto::query;

        let mut conn = setup_test_db();

        // Create concept with a property, a subclass, and an instance
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
        assert!(execute_tool(&mut conn, &create_concept, None).success);

        // Create a property with OldName as domain
        crate::owl::Property::new("foundation:testProp")
            .assert(&mut conn, crate::owl::PropertyType::DatatypeProperty, "test prop", None, &["foundation:OldName"], Some("xsd:string"), None, "test")
            .unwrap();

        // Create a property with OldName as range (incoming reference)
        crate::owl::Property::new("foundation:refToOld")
            .assert(&mut conn, crate::owl::PropertyType::ObjectProperty, "ref to old", None, &[], Some("foundation:OldName"), None, "test")
            .unwrap();

        // Create an instance of OldName
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Instance_1", "rdf:type", Object::Iri("foundation:OldName".to_string())),
        ], "test").unwrap();

        // Create a subclass of OldName
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
        assert!(execute_tool(&mut conn, &create_sub, None).success);

        // Rename OldName to NewName
        let rename = ToolCall {
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:OldName",
                    "new_iri": "foundation:NewName"
                }]
            }),
        };
        let result = execute_tool(&mut conn, &rename, None);
        assert!(result.success, "rename must succeed; got: {:?}", result.error);

        // Old IRI should no longer exist as a class
        let old_class = crate::owl::Class::get(&conn, "foundation:OldName").unwrap();
        assert!(old_class.is_none(), "old IRI must be gone after rename");

        // New IRI should exist as a class
        let new_class = crate::owl::Class::get(&conn, "foundation:NewName").unwrap();
        assert!(new_class.is_some(), "new IRI must exist after rename");

        // Property domain must point to new IRI
        let prop = crate::owl::Property::get(&conn, "foundation:testProp").unwrap().unwrap();
        assert!(prop.domains.contains(&"foundation:NewName".to_string()), "domain must be updated to new IRI");
        assert!(!prop.domains.contains(&"foundation:OldName".to_string()), "old IRI must not remain in domain");

        // Property range must point to new IRI
        let ref_prop = crate::owl::Property::get(&conn, "foundation:refToOld").unwrap().unwrap();
        assert!(ref_prop.ranges.contains(&"foundation:NewName".to_string()), "range must be updated to new IRI");

        // Instance rdf:type must reference new IRI
        let type_triples = query::get_by_entity_predicate(&conn, "foundation:Instance_1", "rdf:type").unwrap();
        let has_new_type = type_triples.triples.iter().any(|t| t.object.as_iri() == Some("foundation:NewName"));
        assert!(has_new_type, "instance type must be updated to new IRI");

        // Subclass superclass must reference new IRI
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
            name: "learn_concepts".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:DoesNotExist",
                    "new_iri": "foundation:SomeTarget"
                }]
            }),
        };
        let result = execute_tool(&mut conn, &rename, None);
        assert!(!result.success, "rename of non-existent concept must fail");
    }

    #[test]
    fn test_rename_concept_rejects_existing_target() {
        use crate::ai::functions::{ToolCall, execute_tool};

        let mut conn = setup_test_db();

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
            assert!(execute_tool(&mut conn, &create, None).success);
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
        let result = execute_tool(&mut conn, &rename, None);
        assert!(!result.success, "rename to existing IRI must fail");
    }
}

pub(super) fn load_concept_context(conn: &Connection, iri: &str) -> Option<Value> {
    get_concept_one(conn, &serde_json::json!({ "iri": iri })).result
}

pub fn get_concepts(conn: &Connection, args: &Value) -> ToolResult {
    let iris = match args.get("iris").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iris".to_string()),
            concept: None,
        },
    };

    let mut results = Vec::new();
    let mut errors = Vec::new();
    for v in iris {
        if let Some(iri) = v.as_str() {
            let r = get_concept_one(conn, &serde_json::json!({ "iri": iri }));
            if r.success {
                if let Some(val) = r.result {
                    results.push(val);
                }
            } else {
                errors.push(format!("{}: {}", iri, r.error.unwrap_or_default()));
            }
        }
    }

    ToolResult {
        success: errors.is_empty(),
        result: Some(serde_json::json!({ "concepts": results })),
        error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
        concept: None,
    }
}

fn get_concept_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").or_else(|| args.get("IRI")).and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    match (|| {
        let concept = Class::get(conn, iri)?
            .ok_or_else(|| crate::owl::OwlError::NotFound(iri.to_string()))?;

        let allowed_values: Vec<serde_json::Value> = if !concept.one_of_values.is_empty() {
            concept.one_of_values.iter().map(|value_iri| {
                let label = crate::owl::get_literal_property(conn, value_iri, "rdfs:label")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| value_iri.clone());
                serde_json::json!({
                    "iri": value_iri,
                    "label": label,
                })
            }).collect()
        } else {
            Vec::new()
        };

        let allowed_statuses: Vec<serde_json::Value> = {
            let status_iris = crate::owl::get_all_iri_properties(conn, iri, "foundation:allowedStatus")?;
            status_iris.iter()
                .map(|status_iri| {
                    let thing = crate::owl::Thing::get(conn, status_iri);
                    let (icon, color) = crate::owl::resolve_status_appearance(conn, status_iri);
                    serde_json::json!({
                        "iri": status_iri,
                        "label": thing.label,
                        "icon": icon,
                        "color": color,
                    })
                })
                .collect()
        };

        let required_fields: Vec<serde_json::Value> = {
            let restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, iri)?;
            restrictions.into_iter()
                .filter(|r| r.is_required())
                .map(|r| {
                    let label = crate::owl::get_literal_property(conn, &r.property_iri, "rdfs:label")
                        .ok()
                        .flatten();
                    serde_json::json!({
                        "property": r.property_iri,
                        "label": label,
                    })
                })
                .collect()
        };

        let incoming_properties: Vec<serde_json::Value> = {
            use crate::eavto::query;
            let result = query::get_by_predicate_object(conn, "rdfs:range", iri)
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
            result.triples.iter()
                .map(|t| {
                    let prop_iri = &t.subject;
                    let label = crate::owl::get_literal_property(conn, prop_iri, "rdfs:label")
                        .ok()
                        .flatten();
                    let domain_iris = crate::owl::get_all_iri_properties(conn, prop_iri, "rdfs:domain")
                        .unwrap_or_default();
                    let domains: Vec<serde_json::Value> = domain_iris.iter()
                        .map(|d| {
                            let d_label = crate::owl::Thing::get(conn, d).label;
                            serde_json::json!({"iri": d, "label": d_label})
                        })
                        .collect();
                    serde_json::json!({
                        "property": prop_iri,
                        "label": label,
                        "domains": domains,
                    })
                })
                .collect()
        };

        let mut response = serde_json::json!({
            "iri": concept.iri,
            "label": concept.label,
            "icon": concept.icon,
            "comment": concept.comment,
            "types": concept.types.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "superClasses": concept.super_classes.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "subClasses": concept.sub_classes.iter().map(|t| serde_json::json!({
                "iri": t.iri,
                "label": t.label,
            })).collect::<Vec<_>>(),
            "properties": concept.properties.iter().map(|(prop, source)| serde_json::json!({
                "property": prop,
                "source": source,
            })).collect::<Vec<_>>(),
            "instanceCount": concept.backlinks.len(),
            "allowedStatuses": allowed_statuses,
            "requiredFields": required_fields,
            "incomingProperties": incoming_properties,
        });

        if !allowed_values.is_empty() {
            response["allowedValues"] = serde_json::json!(allowed_values);
        }

        Ok::<_, crate::owl::OwlError>(response)
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
            concept: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }
}

pub fn learn_concept(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, learn_concept_one)
}

fn learn_concept_one(
    conn: &mut Connection,
    args: &Value,
) -> ToolResult {
    let orig_iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    match (|| {
        let new_iri_arg = args.get("new_iri").and_then(|v| v.as_str());
        let iri: &str = if let Some(new_iri) = new_iri_arg {
            if new_iri != orig_iri {
                if Class::get(conn, orig_iri)?.is_none() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Concept '{}' not found. Cannot rename a non-existent concept.", orig_iri
                    )));
                }
                if Class::get(conn, new_iri)?.is_some() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Concept '{}' already exists. Cannot rename to an existing IRI.", new_iri
                    )));
                }
                crate::eavto::store::rename_iri(conn, orig_iri, new_iri, "ai")
                    .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
                super::batch::queue_event(
                    "entity-updated",
                    serde_json::json!({"entityId": orig_iri}),
                );
                new_iri
            } else {
                orig_iri
            }
        } else {
            orig_iri
        };

        let existing = Class::get(conn, iri)?;
        let is_new = existing.is_none();

        let label_arg = args.get("label").and_then(|v| v.as_str());
        let icon_arg = args.get("icon").and_then(|v| v.as_str());

        let needs_assert = is_new || label_arg.is_some() || icon_arg.is_some();

        if needs_assert {
            let label = label_arg
                .or_else(|| existing.as_ref().and_then(|c| c.label.as_deref()))
                .ok_or_else(|| crate::owl::OwlError::ValidationError(
                    "Missing required parameter: label (required when creating a new concept)".to_string()
                ))?;
            let icon = icon_arg
                .or_else(|| existing.as_ref().and_then(|c| c.icon.as_deref()))
                .ok_or_else(|| crate::owl::OwlError::ValidationError(
                    "Missing required parameter: icon (required when creating a new concept)".to_string()
                ))?;
            let concept = Class::new(iri);
            concept.assert(conn, crate::owl::ClassType::OwlClass, label, icon, None, "ai")?;
        }

        if let Some(comment) = args.get("comment").and_then(|v| v.as_str()) {
            Class::set_comment(conn, iri, comment, "ai")?;
        }

        let super_concepts_val = args.get("super_concepts");

        if is_new {
            let iris: Vec<&str> = super_concepts_val
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            if iris.is_empty() {
                return Err(crate::owl::OwlError::ValidationError(
                    "Missing required parameter: super_concepts (at least one superclass is required when creating a concept)".to_string()
                ));
            }
            Class::set_super_classes(conn, iri, &iris, "ai")?;
        } else if let Some(val) = super_concepts_val {
            let iris: Vec<&str> = val
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            if iris.is_empty() {
                return Err(crate::owl::OwlError::ValidationError(
                    "super_concepts must contain at least one superclass".to_string()
                ));
            }
            Class::set_super_classes(conn, iri, &iris, "ai")?;
        }

        if let Some(allowed_statuses) = args.get("allowed_statuses").and_then(|v| v.as_array()) {
            let status_iris: Vec<&str> = allowed_statuses.iter()
                .filter_map(|v| v.as_str())
                .collect();

            for status_iri in &status_iris {
                let individual = crate::owl::Individual::get(conn, *status_iri)?;
                if individual.is_none() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Status '{}' does not exist. Use remember_concepts to query existing Status instances before setting allowedStatuses.",
                        status_iri
                    )));
                }
                let (icon, _) = crate::owl::resolve_status_appearance(conn, status_iri);
                if icon.is_none() {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Status '{}' exists but has no icon. All statuses must have a valid icon.",
                        status_iri
                    )));
                }
            }

            crate::owl::replace_all_property_iris(
                conn, iri, "foundation:allowedStatus", &status_iris, "ai",
            )?;
        }

        if let Some(remove_details) = args.get("remove_details").and_then(|v| v.as_array()) {
            for item in remove_details {
                if let Some(prop_iri) = item.as_str() {
                    let mut prop = match crate::owl::Property::get(conn, prop_iri)? {
                        Some(p) => p,
                        None => continue,
                    };
                    prop.domains.retain(|d| d != iri);
                    if prop.domains.is_empty() {
                        crate::owl::Property::retract(conn, prop_iri, "ai")?;
                    } else {
                        let domains: Vec<&str> = prop.domains.iter().map(|s| s.as_str()).collect();
                        prop.assert(conn, prop.property_type, prop.label.as_deref().unwrap_or(""), None, &domains, prop.ranges.first().map(|s| s.as_str()), prop.unit.as_deref(), "ai")?;
                    }
                    super::batch::queue_event("entity-updated", serde_json::json!({"entityId": prop_iri}));
                }
            }
        }

        if let Some(details) = args.get("upsert_details").and_then(|v| v.as_array()) {
            for detail in details {
                let prop_iri = detail.as_str()
                    .or_else(|| detail.get("iri").and_then(|v| v.as_str()))
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        "Each upsert_details item must be a property IRI string".to_string()
                    ))?;

                let mut prop = crate::owl::Property::get(conn, prop_iri)?
                    .ok_or_else(|| crate::owl::OwlError::ValidationError(
                        format!("Property '{}' not found. Define it first with learn_properties.", prop_iri)
                    ))?;

                if !prop.domains.contains(&iri.to_string()) {
                    prop.domains.push(iri.to_string());
                    let domains: Vec<&str> = prop.domains.iter().map(|s| s.as_str()).collect();
                    prop.assert(conn, prop.property_type, prop.label.as_deref().unwrap_or(""), None, &domains, prop.ranges.first().map(|s| s.as_str()), prop.unit.as_deref(), "ai")?;
                }

                super::batch::queue_event("entity-updated", serde_json::json!({"entityId": prop_iri}));
                super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
            }
        }

        if let Some(required_fields) = args.get("required_fields").and_then(|v| v.as_array()) {
            let prop_iris: Vec<&str> = required_fields.iter()
                .filter_map(|v| v.as_str())
                .collect();

            for prop_iri in &prop_iris {
                let prop = crate::owl::Property::get(conn, *prop_iri)?;
                let is_valid = prop.map(|p| matches!(
                    p.property_type,
                    crate::owl::PropertyType::ObjectProperty | crate::owl::PropertyType::DatatypeProperty
                )).unwrap_or(false);
                if !is_valid {
                    return Err(crate::owl::OwlError::ValidationError(format!(
                        "Property '{}' is not defined in this ontology",
                        prop_iri
                    )));
                }
            }

            crate::owl::cardinality::set_class_required_fields(conn, iri, &prop_iris, "ai")?;
        }

        if is_new {
            super::batch::queue_event("entity-created", serde_json::json!({"entityId": iri}));
        } else {
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": iri,
            "message": if is_new {
                format!("Concept {} created successfully", iri)
            } else {
                format!("Concept {} updated successfully", iri)
            },
        }))
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
            concept: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: load_concept_context(conn, orig_iri),
        },
    }
}

pub fn delete_concept(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, delete_concept_one)
}

fn delete_concept_one(
    conn: &mut Connection,
    args: &Value,
) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    match (|| {
        if let Some(class) = Class::get(conn, iri)? {
            let child_iris: Vec<String> = class.sub_classes.iter()
                .map(|t| t.iri.clone())
                .filter(|s| !s.starts_with("_:"))
                .collect();
            if !child_iris.is_empty() {
                return Err(crate::owl::OwlError::ValidationError(format!(
                    "Cannot delete concept '{}': it has {} subclass(es) that depend on it: {}. \
                     Remove the superclass reference from each subclass first.",
                    iri, child_iris.len(), child_iris.join(", ")
                )));
            }
        }

        Class::retract_all(conn, iri, "ai")?;

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "message": format!("Concept {} deleted successfully", iri),
        }))
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
            concept: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }
}
