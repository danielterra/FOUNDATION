use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, vocabulary::{rdf, rdfs, owl}};

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
        }
    }

    /// Get complete property data
    pub async fn get(conn: &Connection, iri: impl Into<String>) -> Result<Option<Self>> {
        let iri = iri.into();

        let label_result = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL).await?;
        let label = label_result.triples.first().and_then(|t| t.object.as_literal());

        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT).await?;
        let comment = comment_result.triples.first().and_then(|t| t.object.as_literal());

        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE).await?;
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

        let domains_result = query::get_by_entity_predicate(conn, &iri, rdfs::DOMAIN).await?;
        let domains: Vec<String> = domains_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        let ranges_result = query::get_by_entity_predicate(conn, &iri, rdfs::RANGE).await?;
        let ranges: Vec<String> = ranges_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        let super_result = query::get_by_entity_predicate(conn, &iri, rdfs::SUB_PROPERTY_OF).await?;
        let super_properties: Vec<String> = super_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        let inverse_result = query::get_by_entity_predicate(conn, &iri, owl::INVERSE_OF).await?;
        let inverse_of = inverse_result.triples.first()
            .and_then(|t| t.object.as_iri())
            .map(|s| s.to_string());

        let unit_result = query::get_by_entity_predicate(conn, &iri, "qudt:hasUnit").await?;
        let unit = unit_result.triples.first()
            .and_then(|t| t.object.as_iri())
            .map(|s| s.to_string());

        let formula_result = query::get_by_entity_predicate(conn, &iri, "foundation:formula").await?;
        let formula = formula_result.triples.first().and_then(|t| t.object.as_literal());

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
        }))
    }

    /// Assert a new property with metadata
    ///
    /// IMPORTANT: If range is a numeric type (xsd:decimal, xsd:integer, xsd:float, xsd:double),
    /// you MUST provide a unit parameter with a valid QUDT unit (e.g., "unit:GigaBYTE")
    pub async fn assert(
        &self,
        conn: &Connection,
        property_type: PropertyType,
        label: &str,
        comment: Option<&str>,
        domains: &[&str],
        range: Option<&str>,
        unit: Option<&str>,
        origin: &str
    ) -> Result<()> {
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

        if let Some(range_class) = range {
            triples.push(Triple::new(&self.iri, rdfs::RANGE, Object::Iri(range_class.to_string())));
        }

        if let Some(unit_iri) = unit {
            triples.push(Triple::new(&self.iri, "qudt:hasUnit", Object::Iri(unit_iri.to_string())));
        }

        store::assert_triples(conn, &triples, origin).await
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;

        if let Some(formula_str) = &self.formula {
            let formula_triple = Triple::new(&self.iri, "foundation:formula", Object::Literal {
                value: formula_str.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[formula_triple], origin).await
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn retract(conn: &Connection, iri: &str, origin: &str) -> Result<Vec<String>> {
        let facts = query::get_by_predicate(conn, iri).await?;
        let mut affected: std::collections::HashSet<String> = std::collections::HashSet::new();
        let facts_to_retract: Vec<Triple> = facts.triples.into_iter()
            .map(|t| {
                affected.insert(t.subject.clone());
                Triple::new(t.subject, t.predicate, t.object)
            })
            .collect();
        if !facts_to_retract.is_empty() {
            store::retract_triples(conn, &facts_to_retract, origin).await
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        }
        let definition = query::get_by_entity(conn, iri).await?;
        let def_triples: Vec<Triple> = definition.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        if !def_triples.is_empty() {
            store::retract_triples(conn, &def_triples, origin).await
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        }
        Ok(affected.into_iter().collect())
    }

    pub async fn find_all_iris(conn: &Connection) -> Result<Vec<String>> {
        let obj_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::OBJECT_PROPERTY).await?;
        let dat_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::DATATYPE_PROPERTY).await?;
        let mut iris: Vec<String> = obj_result.triples.into_iter()
            .chain(dat_result.triples)
            .map(|t| t.subject)
            .collect();
        iris.sort();
        iris.dedup();
        Ok(iris)
    }

    pub async fn is_functional(conn: &Connection, property_iri: &str) -> Result<bool> {
        let types_result = crate::eavto::query::get_by_entity_predicate_internal(
            conn,
            property_iri,
            rdf::TYPE,
            false
        ).await?;

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

    #[tokio::test]
    async fn test_assert_numeric_property_requires_unit() {
        let conn = setup_test_db().await;
        let prop = Property::new("foundation:hasAge");

        let result = prop.assert(
            &conn,
            PropertyType::DatatypeProperty,
            "has age",
            Some("The age of a person"),
            &["foundation:Person"],
            Some("xsd:integer"),
            None,
            "test"
        ).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("qudt:unit"));
    }

    #[tokio::test]
    async fn test_assert_numeric_property_with_unit() {
        let conn = setup_test_db().await;
        let prop = Property::new("foundation:hasAge");

        let result = prop.assert(
            &conn,
            PropertyType::DatatypeProperty,
            "has age",
            Some("The age of a person"),
            &["foundation:Person"],
            Some("xsd:integer"),
            Some("unit:YR"),
            "test"
        ).await;
        assert!(result.is_ok());

        let property = Property::get(&conn, "foundation:hasAge").await.unwrap().unwrap();
        assert_eq!(property.iri, "foundation:hasAge");
        assert_eq!(property.label, Some("has age".to_string()));
        assert_eq!(property.comment, Some("The age of a person".to_string()));
        assert_eq!(property.property_type, PropertyType::DatatypeProperty);
        assert_eq!(property.domains.len(), 1);
        assert_eq!(property.domains[0], "foundation:Person");
        assert_eq!(property.ranges.len(), 1);
        assert_eq!(property.ranges[0], "xsd:integer");
    }

    #[tokio::test]
    async fn test_object_property() {
        let conn = setup_test_db().await;
        let prop = Property::new("foundation:hasParent");

        prop.assert(
            &conn,
            PropertyType::ObjectProperty,
            "has parent",
            None,
            &["foundation:Person"],
            Some("foundation:Person"),
            None,
            "test"
        ).await.unwrap();

        let property = Property::get(&conn, "foundation:hasParent").await.unwrap().unwrap();
        assert_eq!(property.property_type, PropertyType::ObjectProperty);
        assert!(Property::get(&conn, "foundation:hasParent").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_non_numeric_property_cannot_have_unit() {
        let conn = setup_test_db().await;
        let prop = Property::new("foundation:hasName");

        let result = prop.assert(
            &conn,
            PropertyType::DatatypeProperty,
            "has name",
            None,
            &["foundation:Person"],
            Some("xsd:string"),
            Some("unit:GigaBYTE"),
            "test"
        ).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-numeric"));
    }

    #[tokio::test]
    async fn test_all_numeric_types_require_unit() {
        let conn = setup_test_db().await;

        let numeric_types = vec![
            ("xsd:decimal", "unit:Meter"),
            ("xsd:integer", "unit:YR"),
            ("xsd:float", "unit:KiloGM"),
            ("xsd:double", "unit:Second"),
        ];

        for (i, (xsd_type, unit)) in numeric_types.iter().enumerate() {
            let prop = Property::new(&format!("test:prop{}", i));

            let result = prop.assert(
                &conn,
                PropertyType::DatatypeProperty,
                "test prop",
                None,
                &[],
                Some(xsd_type),
                None,
                "test"
            ).await;
            assert!(result.is_err(), "Should fail for {} without unit", xsd_type);

            let result = prop.assert(
                &conn,
                PropertyType::DatatypeProperty,
                "test prop",
                None,
                &[],
                Some(xsd_type),
                Some(unit),
                "test"
            ).await;
            assert!(result.is_ok(), "Should succeed for {} with unit", xsd_type);
        }
    }

    // ── retract ─────────────────────────────────────────────────────────────

    async fn assert_object_property(conn: &Connection, iri: &str) {
        Property::new(iri).assert(
            conn,
            PropertyType::ObjectProperty,
            "Test Property",
            Some("A test property"),
            &["foundation:Person"],
            Some("foundation:Person"),
            None,
            "test",
        ).await.unwrap();
    }

    #[tokio::test]
    async fn test_retract_removes_property_definition() {
        let conn = setup_test_db().await;
        assert_object_property(&conn, "foundation:hasParent").await;

        assert!(Property::get(&conn, "foundation:hasParent").await.unwrap().is_some());

        Property::retract(&conn, "foundation:hasParent", "test").await.unwrap();

        assert!(Property::get(&conn, "foundation:hasParent").await.unwrap().is_none(),
            "property should no longer exist after retraction");
    }

    #[tokio::test]
    async fn test_retract_removes_fact_triples_using_property() {
        let conn = setup_test_db().await;
        assert_object_property(&conn, "foundation:hasParent").await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:alice", "foundation:hasParent", Object::Iri("foundation:bob".to_string())),
            Triple::new("foundation:carol", "foundation:hasParent", Object::Iri("foundation:dave".to_string())),
        ], "test").await.unwrap();

        Property::retract(&conn, "foundation:hasParent", "test").await.unwrap();

        let alice_facts = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:alice", "foundation:hasParent"
        ).await.unwrap();
        assert!(alice_facts.triples.is_empty(), "fact triple for alice must be retracted");

        let carol_facts = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:carol", "foundation:hasParent"
        ).await.unwrap();
        assert!(carol_facts.triples.is_empty(), "fact triple for carol must be retracted");
    }

    #[tokio::test]
    async fn test_retract_returns_affected_subjects() {
        let conn = setup_test_db().await;
        assert_object_property(&conn, "foundation:hasParent").await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:alice", "foundation:hasParent", Object::Iri("foundation:bob".to_string())),
            Triple::new("foundation:carol", "foundation:hasParent", Object::Iri("foundation:dave".to_string())),
        ], "test").await.unwrap();

        let mut affected = Property::retract(&conn, "foundation:hasParent", "test").await.unwrap();
        affected.sort();

        assert_eq!(affected, vec!["foundation:alice", "foundation:carol"]);
    }

    #[tokio::test]
    async fn test_retract_nonexistent_property_returns_empty() {
        let conn = setup_test_db().await;

        let affected = Property::retract(&conn, "foundation:ghost", "test").await.unwrap();
        assert!(affected.is_empty(), "retracting a non-existent property must not error");
    }

    #[tokio::test]
    async fn test_retract_with_no_usages_returns_empty_affected() {
        let conn = setup_test_db().await;
        assert_object_property(&conn, "foundation:hasParent").await;

        let affected = Property::retract(&conn, "foundation:hasParent", "test").await.unwrap();
        assert!(affected.is_empty(), "no instances used the property, so affected must be empty");
    }

    #[tokio::test]
    async fn test_retract_datatype_property() {
        let conn = setup_test_db().await;

        Property::new("foundation:birthDate").assert(
            &conn,
            PropertyType::DatatypeProperty,
            "birth date",
            None,
            &["foundation:Person"],
            Some("xsd:string"),
            None,
            "test",
        ).await.unwrap();

        store::assert_triples(&conn, &[
            Triple::new("foundation:alice", "foundation:birthDate", Object::Literal {
                value: "1990-01-01".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").await.unwrap();

        let affected = Property::retract(&conn, "foundation:birthDate", "test").await.unwrap();
        assert!(affected.contains(&"foundation:alice".to_string()));
        assert!(Property::get(&conn, "foundation:birthDate").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_property_characteristics() {
        let conn = setup_test_db().await;
        let prop = Property::new("foundation:hasParent");

        prop.assert(
            &conn,
            PropertyType::ObjectProperty,
            "has parent",
            None,
            &[],
            None,
            None,
            "test"
        ).await.unwrap();

        let functional_triple = Triple::new(
            "foundation:hasParent",
            rdf::TYPE,
            Object::Iri(owl::FUNCTIONAL_PROPERTY.to_string())
        );
        store::assert_triples(&conn, &[functional_triple], "test").await.unwrap();

        let property = Property::get(&conn, "foundation:hasParent").await.unwrap().unwrap();
        assert!(property.is_functional);
    }

    #[tokio::test]
    async fn test_transitive_property_detection() {
        let conn = setup_test_db().await;

        Property::new("foundation:isAncestorOf").assert(
            &conn,
            PropertyType::ObjectProperty,
            "is ancestor of",
            None,
            &[],
            None,
            None,
            "test",
        ).await.unwrap();

        store::append_triples(&conn, &[
            Triple::new("foundation:isAncestorOf", rdf::TYPE, Object::Iri(owl::TRANSITIVE_PROPERTY.to_string())),
        ], "test").await.unwrap();

        let property = Property::get(&conn, "foundation:isAncestorOf").await.unwrap().unwrap();
        assert!(property.is_transitive, "property should be detected as transitive");
        assert!(!property.is_symmetric);
        assert!(!property.is_functional);
        assert_eq!(property.property_type, PropertyType::ObjectProperty);
    }

    #[tokio::test]
    async fn test_symmetric_property_detection() {
        let conn = setup_test_db().await;

        Property::new("foundation:isSiblingOf").assert(
            &conn,
            PropertyType::ObjectProperty,
            "is sibling of",
            None,
            &[],
            None,
            None,
            "test",
        ).await.unwrap();

        store::append_triples(&conn, &[
            Triple::new("foundation:isSiblingOf", rdf::TYPE, Object::Iri(owl::SYMMETRIC_PROPERTY.to_string())),
        ], "test").await.unwrap();

        let property = Property::get(&conn, "foundation:isSiblingOf").await.unwrap().unwrap();
        assert!(property.is_symmetric, "property should be detected as symmetric");
        assert!(!property.is_transitive);
        assert!(!property.is_functional);
        assert_eq!(property.property_type, PropertyType::ObjectProperty);
    }

    #[tokio::test]
    async fn test_annotation_property_detection() {
        let conn = setup_test_db().await;

        Property::new("foundation:seeAlso").assert(
            &conn,
            PropertyType::AnnotationProperty,
            "see also",
            None,
            &[],
            None,
            None,
            "test",
        ).await.unwrap();

        let property = Property::get(&conn, "foundation:seeAlso").await.unwrap().unwrap();
        assert_eq!(property.property_type, PropertyType::AnnotationProperty);
        assert!(!property.is_functional);
        assert!(!property.is_transitive);
        assert!(!property.is_symmetric);
    }

    #[tokio::test]
    async fn test_transitive_and_symmetric_combined() {
        let conn = setup_test_db().await;

        Property::new("foundation:equals").assert(
            &conn,
            PropertyType::ObjectProperty,
            "equals",
            None,
            &[],
            None,
            None,
            "test",
        ).await.unwrap();

        store::append_triples(&conn, &[
            Triple::new("foundation:equals", rdf::TYPE, Object::Iri(owl::TRANSITIVE_PROPERTY.to_string())),
            Triple::new("foundation:equals", rdf::TYPE, Object::Iri(owl::SYMMETRIC_PROPERTY.to_string())),
        ], "test").await.unwrap();

        let property = Property::get(&conn, "foundation:equals").await.unwrap().unwrap();
        assert!(property.is_transitive);
        assert!(property.is_symmetric);
        assert_eq!(property.property_type, PropertyType::ObjectProperty);
    }

    #[tokio::test]
    async fn test_property_without_characteristics_has_all_false() {
        let conn = setup_test_db().await;

        Property::new("foundation:hasName").assert(
            &conn,
            PropertyType::DatatypeProperty,
            "has name",
            None,
            &[],
            Some("xsd:string"),
            None,
            "test",
        ).await.unwrap();

        let property = Property::get(&conn, "foundation:hasName").await.unwrap().unwrap();
        assert!(!property.is_functional);
        assert!(!property.is_transitive);
        assert!(!property.is_symmetric);
        assert_eq!(property.property_type, PropertyType::DatatypeProperty);
    }
}
