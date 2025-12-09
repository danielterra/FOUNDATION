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
        })
    }

    /// Assert a new property with metadata
    pub fn assert(
        &self,
        conn: &mut Connection,
        property_type: PropertyType,
        label: &str,
        comment: Option<&str>,
        domain: Option<&str>,
        range: Option<&str>,
        origin: &str
    ) -> Result<()> {
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
    fn test_assert_and_get() {
        let mut conn = setup_test_db();
        let prop = Property::new("foundation:hasAge");

        // Assert property with metadata
        let result = prop.assert(
            &mut conn,
            PropertyType::DatatypeProperty,
            "has age",
            Some("The age of a person"),
            Some("foundation:Person"),
            Some("xsd:integer"),
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

        // Assert object property
        prop.assert(
            &mut conn,
            PropertyType::ObjectProperty,
            "has parent",
            None,
            Some("foundation:Person"),
            Some("foundation:Person"),
            "test"
        ).unwrap();

        // Get and verify
        let property = Property::get(&conn, "foundation:hasParent").unwrap();
        assert_eq!(property.property_type, PropertyType::ObjectProperty);
        assert!(property.exists(&conn).unwrap());
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
