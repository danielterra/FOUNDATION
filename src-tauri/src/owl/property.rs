use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, vocabulary::{rdf, rdfs, owl}};

#[derive(Debug, Clone)]
pub struct DomainLabel {
    pub domain: String,
    pub forward_label: String,
    pub inverse_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Property {
    pub iri: String,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub property_type: PropertyType,
    pub domains: Vec<String>,
    pub ranges: Vec<String>,
    pub super_properties: Vec<String>,
    pub is_functional: bool,
    pub is_transitive: bool,
    pub is_symmetric: bool,
    pub inverse_of: Option<String>,
    pub unit: Option<String>,
    pub formula: Option<String>,
    pub domain_labels: Vec<DomainLabel>,
    pub ai_behavior_rules: Option<String>,
}

impl Property {
    /// Create a new Property reference (for asserting)
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            iri: iri.into(),
            label: None,
            comment: None,
            property_type: PropertyType::RdfProperty,
            domains: vec![],
            ranges: vec![],
            super_properties: vec![],
            is_functional: false,
            is_transitive: false,
            is_symmetric: false,
            inverse_of: None,
            unit: None,
            formula: None,
            domain_labels: vec![],
            ai_behavior_rules: None,
        }
    }

    fn get_domain_labels(conn: &Connection, property_iri: &str) -> Result<Vec<DomainLabel>> {
        let dl_result = query::get_by_predicate_object(
            conn, "foundation:onProperty", property_iri,
        )?;
        let mut domain_labels = Vec::new();
        for triple in dl_result.triples {
            let dl_iri = &triple.subject;
            let domain = query::get_by_entity_predicate(conn, dl_iri, "foundation:forDomain")?
                .triples.first().and_then(|t| t.object.as_iri()).map(|s| s.to_string());
            let forward_label = query::get_by_entity_predicate(
                conn, dl_iri, "foundation:forwardLabel",
            )?.triples.first().and_then(|t| t.object.as_literal());
            let inverse_label = query::get_by_entity_predicate(
                conn, dl_iri, "foundation:inverseLabel",
            )?.triples.first().and_then(|t| t.object.as_literal());
            if let (Some(domain), Some(forward_label)) = (domain, forward_label) {
                domain_labels.push(DomainLabel { domain, forward_label, inverse_label });
            }
        }
        Ok(domain_labels)
    }

    /// Get complete property data
    pub fn get(conn: &Connection, iri: impl Into<String>) -> Result<Option<Self>> {
        let iri = iri.into();

        // Get label
        let label_result = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL)?;
        let label = label_result.triples.first().and_then(|t| t.object.as_literal());

        // Get comment
        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT)?;
        let comment = comment_result.triples.first().and_then(|t| t.object.as_literal());

        // Get property type
        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE)?;
        if types_result.triples.is_empty() {
            return Ok(None);
        }
        let mut property_type = PropertyType::RdfProperty;
        let mut is_functional = false;
        let mut is_transitive = false;
        let mut is_symmetric = false;

        for triple in &types_result.triples {
            if let Some(type_iri) = triple.object.as_iri() {
                match type_iri {
                    t if t == owl::OBJECT_PROPERTY => property_type = PropertyType::ObjectProperty,
                    t if t == owl::DATATYPE_PROPERTY => {
                        property_type = PropertyType::DatatypeProperty
                    }
                    t if t == owl::ANNOTATION_PROPERTY => {
                        property_type = PropertyType::AnnotationProperty
                    }
                    t if t == owl::FUNCTIONAL_PROPERTY => is_functional = true,
                    t if t == owl::TRANSITIVE_PROPERTY => is_transitive = true,
                    t if t == owl::SYMMETRIC_PROPERTY => is_symmetric = true,
                    _ => {}
                }
            }
        }

        // Get domains
        let domains_result = query::get_by_entity_predicate(conn, &iri, rdfs::DOMAIN)?;
        let domains: Vec<String> = domains_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        // Get ranges
        let ranges_result = query::get_by_entity_predicate(conn, &iri, rdfs::RANGE)?;
        let ranges: Vec<String> = ranges_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        // Get super properties
        let super_result = query::get_by_entity_predicate(conn, &iri, rdfs::SUB_PROPERTY_OF)?;
        let super_properties: Vec<String> = super_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        // Get inverse
        let inverse_result = query::get_by_entity_predicate(conn, &iri, owl::INVERSE_OF)?;
        let inverse_of = inverse_result.triples.first()
            .and_then(|t| t.object.as_iri())
            .map(|s| s.to_string());

        // Get QUDT unit
        let unit_result = query::get_by_entity_predicate(conn, &iri, "qudt:hasUnit")?;
        let unit = unit_result.triples.first()
            .and_then(|t| t.object.as_iri())
            .map(|s| s.to_string());

        let formula_result = query::get_by_entity_predicate(conn, &iri, "foundation:formula")?;
        let formula = formula_result.triples.first().and_then(|t| t.object.as_literal());

        let ai_behavior_rules_result = query::get_by_entity_predicate(
            conn, &iri, "foundation:aiBehaviorRules",
        )?;
        let ai_behavior_rules = ai_behavior_rules_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let domain_labels = Self::get_domain_labels(conn, &iri)?;

        Ok(Some(Self {
            iri,
            label,
            comment,
            property_type,
            domains,
            ranges,
            super_properties,
            is_functional,
            is_transitive,
            is_symmetric,
            inverse_of,
            unit,
            formula,
            domain_labels,
            ai_behavior_rules,
        }))
    }

    /// Assert a new property with metadata
    ///
    /// IMPORTANT: If range is a numeric type (xsd:decimal, xsd:integer, xsd:float, xsd:double),
    /// you MUST provide a unit parameter with a valid QUDT unit (e.g., "unit:GigaBYTE")
    pub fn assert(
        &self,
        conn: &mut Connection,
        property_type: PropertyType,
        label: &str,
        comment: Option<&str>,
        domains: &[&str],
        range: Option<&str>,
        unit: Option<&str>,
        origin: &str
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, &self.iri, None)?;
        // Validate that numeric ranges have a unit
        if let Some(range_value) = range {
            let is_numeric = matches!(
                range_value,
                "xsd:decimal" | "xsd:integer" | "xsd:float" | "xsd:double"
            );

            if is_numeric && unit.is_none() {
                return Err(crate::owl::OwlError::ValidationError(
                    format!(
                        "Property '{}' has numeric range '{}' but no qudt:unit specified. \
                         Numeric properties MUST have a unit \
                         (e.g., unit:GigaBYTE, unit:Second, unit:Meter)",
                        self.iri, range_value
                    )
                ));
            }

            if !is_numeric && unit.is_some() {
                return Err(crate::owl::OwlError::ValidationError(
                    format!(
                        "Property '{}' has non-numeric range '{}' but qudt:unit was specified. \
                         Only numeric properties can have units.",
                        self.iri, range_value
                    )
                ));
            }
        }

        // Assert property type
        let type_iri = match property_type {
            PropertyType::RdfProperty => rdf::PROPERTY,
            PropertyType::ObjectProperty => owl::OBJECT_PROPERTY,
            PropertyType::DatatypeProperty => owl::DATATYPE_PROPERTY,
            PropertyType::AnnotationProperty => owl::ANNOTATION_PROPERTY,
        };

        let mut triples = vec![
            Triple::new(&self.iri, rdf::TYPE, Object::Iri(type_iri.to_string())),
            Triple::new(&self.iri, rdfs::LABEL, Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ];

        // Add comment if provided
        if let Some(comment_text) = comment {
            triples.push(Triple::new(&self.iri, rdfs::COMMENT, Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }));
        }

        for domain_class in domains {
            triples.push(Triple::new(
                &self.iri,
                rdfs::DOMAIN,
                Object::Iri(domain_class.to_string()),
            ));
        }

        // Add range if provided
        if let Some(range_class) = range {
            triples.push(Triple::new(&self.iri, rdfs::RANGE, Object::Iri(range_class.to_string())));
        }

        // Add QUDT unit if provided (required for numeric ranges)
        if let Some(unit_iri) = unit {
            triples.push(Triple::new(&self.iri, "qudt:hasUnit", Object::Iri(unit_iri.to_string())));
        }

        store::assert_triples(conn, &triples, origin)?;

        if let Some(formula_str) = &self.formula {
            let formula_triple = Triple::new(&self.iri, "foundation:formula", Object::Literal {
                value: formula_str.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[formula_triple], origin)?;
        }

        Ok(())
    }

    pub fn retract(conn: &mut Connection, iri: &str, origin: &str) -> Result<Vec<String>> {
        crate::owl::check_system_locked(conn, iri, None)?;
        let facts = query::get_by_predicate(conn, iri)?;
        let mut affected: std::collections::HashSet<String> = std::collections::HashSet::new();
        let facts_to_retract: Vec<Triple> = facts.triples.into_iter()
            .map(|t| {
                affected.insert(t.subject.clone());
                Triple::new(t.subject, t.predicate, t.object)
            })
            .collect();
        if !facts_to_retract.is_empty() {
            store::retract_triples(conn, &facts_to_retract, origin)?;
        }
        let definition = query::get_by_entity(conn, iri)?;
        let def_triples: Vec<Triple> = definition.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        if !def_triples.is_empty() {
            store::retract_triples(conn, &def_triples, origin)?;
        }
        Ok(affected.into_iter().collect())
    }

    /// Check if a property is functional (has at most one value per subject)
    ///
    /// Returns true if the property is marked as owl:FunctionalProperty in the ontology.
    /// This is used by the query layer to determine whether to return one value or multiple values.
    ///
    /// IMPORTANT: This method uses get_by_entity_predicate_internal with check_functional=false
    /// to avoid infinite recursion.
    pub fn find_all_iris(conn: &Connection) -> Result<Vec<String>> {
        let obj_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::OBJECT_PROPERTY)?;
        let dat_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::DATATYPE_PROPERTY)?;
        let mut iris: Vec<String> = obj_result.triples.into_iter()
            .chain(dat_result.triples)
            .map(|t| t.subject)
            .collect();
        iris.sort();
        iris.dedup();
        Ok(iris)
    }

    pub fn is_functional(conn: &Connection, property_iri: &str) -> Result<bool> {
        let types_result = crate::eavto::query::get_by_entity_predicate_internal(
            conn,
            property_iri,
            rdf::TYPE,
            false
        )?;

        for triple in &types_result.triples {
            if let Some(type_iri) = triple.object.as_iri() {
                if type_iri == owl::FUNCTIONAL_PROPERTY {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

/// ObjectProperty is just an alias - use Property with PropertyType::ObjectProperty
#[allow(dead_code)]
pub type ObjectProperty = Property;

/// DatatypeProperty is just an alias - use Property with PropertyType::DatatypeProperty
#[allow(dead_code)]
pub type DatatypeProperty = Property;

/// Property type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    RdfProperty,
    ObjectProperty,
    DatatypeProperty,
    AnnotationProperty,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    #[test]
    fn test_assert_numeric_property_requires_unit() {
        let mut conn = setup_test_db();
        let prop = Property::new("foundation:hasAge");

        // Try to assert numeric property WITHOUT unit - should fail
        let result = prop.assert(
            &mut conn,
            PropertyType::DatatypeProperty,
            "has age",
            Some("The age of a person"),
            &["foundation:Person"],
            Some("xsd:integer"),
            None, // NO UNIT - should fail
            "test"
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("qudt:unit"));
    }

    #[test]
    fn test_assert_numeric_property_with_unit() {
        let mut conn = setup_test_db();
        let prop = Property::new("foundation:hasAge");

        // Assert numeric property WITH unit - should succeed
        let result = prop.assert(
            &mut conn,
            PropertyType::DatatypeProperty,
            "has age",
            Some("The age of a person"),
            &["foundation:Person"],
            Some("xsd:integer"),
            Some("unit:YR"), // WITH UNIT - should succeed
            "test"
        );
        assert!(result.is_ok());

        // Get complete property data
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

        // Assert object property (no unit needed for object properties)
        prop.assert(
            &mut conn,
            PropertyType::ObjectProperty,
            "has parent",
            None,
            &["foundation:Person"],
            Some("foundation:Person"),
            None, // Object properties don't need units
            "test"
        ).unwrap();

        // Get and verify
        let property = Property::get(&conn, "foundation:hasParent").unwrap().unwrap();
        assert_eq!(property.property_type, PropertyType::ObjectProperty);
        assert!(Property::get(&conn, "foundation:hasParent").unwrap().is_some());
    }

    #[test]
    fn test_non_numeric_property_cannot_have_unit() {
        let mut conn = setup_test_db();
        let prop = Property::new("foundation:hasName");

        // Try to assert string property WITH unit - should fail
        let result = prop.assert(
            &mut conn,
            PropertyType::DatatypeProperty,
            "has name",
            None,
            &["foundation:Person"],
            Some("xsd:string"),
            Some("unit:GigaBYTE"), // String property with unit - should fail
            "test"
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-numeric"));
    }

    #[test]
    fn test_all_numeric_types_require_unit() {
        let mut conn = setup_test_db();

        // Test all numeric types
        let numeric_types = vec![
            ("xsd:decimal", "unit:Meter"),
            ("xsd:integer", "unit:YR"),
            ("xsd:float", "unit:KiloGM"),
            ("xsd:double", "unit:Second"),
        ];

        for (i, (xsd_type, unit)) in numeric_types.iter().enumerate() {
            let prop = Property::new(&format!("test:prop{}", i));

            // Without unit - should fail
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

            // With unit - should succeed
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

    // ── retract ─────────────────────────────────────────────────────────────

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

        // Create two instances using this property
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:alice", "foundation:hasParent", Object::Iri("foundation:bob".to_string())),
            Triple::new("foundation:carol", "foundation:hasParent", Object::Iri("foundation:dave".to_string())),
        ], "test").unwrap();

        Property::retract(&mut conn, "foundation:hasParent", "test").unwrap();

        // Fact triples must be gone
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

        // Assert property
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

        // Add functional characteristic
        let functional_triple = Triple::new(
            "foundation:hasParent",
            rdf::TYPE,
            Object::Iri(owl::FUNCTIONAL_PROPERTY.to_string())
        );
        store::assert_triples(&mut conn, &[functional_triple], "test").unwrap();

        // Get and verify
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
}
