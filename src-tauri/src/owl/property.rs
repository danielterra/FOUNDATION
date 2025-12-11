// ============================================================================
// OWL Property - Property Operations
// ============================================================================
// High-level operations for managing OWL/RDFS properties
// ============================================================================

use rusqlite::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, vocabulary::{rdf, rdfs, owl}};

/// Base Property type with complete data
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
        }
    }

    /// Get complete property data
    pub fn get(conn: &Connection, iri: impl Into<String>) -> Result<Self> {
        let iri = iri.into();

        // Get label
        let label_result = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL)?;
        let label = label_result.triples.first().and_then(|t| t.object.as_literal());

        // Get comment
        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT)?;
        let comment = comment_result.triples.first().and_then(|t| t.object.as_literal());

        // Get property type
        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE)?;
        let mut property_type = PropertyType::RdfProperty;
        let mut is_functional = false;
        let mut is_transitive = false;
        let mut is_symmetric = false;

        for triple in &types_result.triples {
            if let Some(type_iri) = triple.object.as_iri() {
                match type_iri {
                    t if t == owl::OBJECT_PROPERTY => property_type = PropertyType::ObjectProperty,
                    t if t == owl::DATATYPE_PROPERTY => property_type = PropertyType::DatatypeProperty,
                    t if t == owl::ANNOTATION_PROPERTY => property_type = PropertyType::AnnotationProperty,
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

        Ok(Self {
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
        })
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
        domain: Option<&str>,
        range: Option<&str>,
        unit: Option<&str>,
        origin: &str
    ) -> Result<()> {
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
                         Numeric properties MUST have a unit (e.g., unit:GigaBYTE, unit:Second, unit:Meter)",
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

        // Add domain if provided
        if let Some(domain_class) = domain {
            triples.push(Triple::new(&self.iri, rdfs::DOMAIN, Object::Iri(domain_class.to_string())));
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
        Ok(())
    }

    /// Check if this property exists
    pub fn exists(&self, conn: &Connection) -> Result<bool> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;
        Ok(!result.triples.is_empty())
    }
}

/// ObjectProperty is just an alias - use Property with PropertyType::ObjectProperty
pub type ObjectProperty = Property;

/// DatatypeProperty is just an alias - use Property with PropertyType::DatatypeProperty
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
            Some("foundation:Person"),
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
            Some("foundation:Person"),
            Some("xsd:integer"),
            Some("unit:YR"), // WITH UNIT - should succeed
            "test"
        );
        assert!(result.is_ok());

        // Get complete property data
        let property = Property::get(&conn, "foundation:hasAge").unwrap();
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
            Some("foundation:Person"),
            Some("foundation:Person"),
            None, // Object properties don't need units
            "test"
        ).unwrap();

        // Get and verify
        let property = Property::get(&conn, "foundation:hasParent").unwrap();
        assert_eq!(property.property_type, PropertyType::ObjectProperty);
        assert!(property.exists(&conn).unwrap());
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
            Some("foundation:Person"),
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
                None,
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
                None,
                Some(xsd_type),
                Some(unit),
                "test"
            );
            assert!(result.is_ok(), "Should succeed for {} with unit", xsd_type);
        }
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
            None,
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
        let property = Property::get(&conn, "foundation:hasParent").unwrap();
        assert!(property.is_functional);
    }
}
