// ============================================================================
// OWL Property - Property Operations
// ============================================================================
// High-level operations for managing OWL/RDFS properties
// ============================================================================

use rusqlite::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, vocabulary::{rdf, rdfs, owl}};

/// Base Property type
#[derive(Debug, Clone)]
pub struct Property {
    pub iri: String,
}

impl Property {
    /// Create a new Property reference
    pub fn new(iri: impl Into<String>) -> Self {
        Self { iri: iri.into() }
    }

    /// Assert that this IRI is a property
    pub fn assert_property(&self, conn: &mut Connection, property_type: PropertyType, origin: &str) -> Result<()> {
        let type_iri = match property_type {
            PropertyType::RdfProperty => rdf::PROPERTY,
            PropertyType::ObjectProperty => owl::OBJECT_PROPERTY,
            PropertyType::DatatypeProperty => owl::DATATYPE_PROPERTY,
            PropertyType::AnnotationProperty => owl::ANNOTATION_PROPERTY,
        };

        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(type_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add rdfs:label
    pub fn add_label(&self, conn: &mut Connection, label: &str, language: Option<&str>, origin: &str) -> Result<()> {
        let object = Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: language.map(|l| l.to_string()),
        };

        let triple = Triple::new(&self.iri, rdfs::LABEL, object);
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add rdfs:comment
    pub fn add_comment(&self, conn: &mut Connection, comment: &str, language: Option<&str>, origin: &str) -> Result<()> {
        let object = Object::Literal {
            value: comment.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: language.map(|l| l.to_string()),
        };

        let triple = Triple::new(&self.iri, rdfs::COMMENT, object);
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add rdfs:domain (class that this property applies to)
    pub fn add_domain(&self, conn: &mut Connection, domain_class: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, rdfs::DOMAIN, Object::Iri(domain_class.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add rdfs:range (value type of this property)
    pub fn add_range(&self, conn: &mut Connection, range_class: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, rdfs::RANGE, Object::Iri(range_class.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add rdfs:subPropertyOf relationship
    pub fn add_super_property(&self, conn: &mut Connection, super_property: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, rdfs::SUB_PROPERTY_OF, Object::Iri(super_property.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Get domain classes
    pub fn get_domains(&self, conn: &Connection) -> Result<Vec<String>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdfs::DOMAIN)?;
        Ok(result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect())
    }

    /// Get range classes
    pub fn get_ranges(&self, conn: &Connection) -> Result<Vec<String>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdfs::RANGE)?;
        Ok(result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect())
    }

    /// Check if this property exists
    pub fn exists(&self, conn: &Connection) -> Result<bool> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;
        Ok(!result.triples.is_empty())
    }

    /// Get label
    pub fn get_label(&self, conn: &Connection) -> Result<Option<String>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdfs::LABEL)?;
        Ok(result.triples.first()
            .and_then(|t| t.object.as_literal()))
    }

    /// Get comment
    pub fn get_comment(&self, conn: &Connection) -> Result<Option<String>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdfs::COMMENT)?;
        Ok(result.triples.first()
            .and_then(|t| t.object.as_literal()))
    }

    /// Check if property is functional
    pub fn is_functional(&self, conn: &Connection) -> Result<bool> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;
        Ok(result.triples.iter().any(|t| {
            if let Object::Iri(type_iri) = &t.object {
                type_iri == owl::FUNCTIONAL_PROPERTY
            } else {
                false
            }
        }))
    }
}

/// ObjectProperty (relates individuals to individuals)
#[derive(Debug, Clone)]
pub struct ObjectProperty {
    pub property: Property,
}

impl ObjectProperty {
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            property: Property::new(iri),
        }
    }

    /// Assert as owl:ObjectProperty
    pub fn assert(&self, conn: &mut Connection, origin: &str) -> Result<()> {
        self.property.assert_property(conn, PropertyType::ObjectProperty, origin)
    }

    /// Add owl:inverseOf
    pub fn add_inverse(&self, conn: &mut Connection, inverse: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.property.iri, owl::INVERSE_OF, Object::Iri(inverse.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Mark as owl:FunctionalProperty
    pub fn mark_functional(&self, conn: &mut Connection, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.property.iri, rdf::TYPE, Object::Iri(owl::FUNCTIONAL_PROPERTY.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Mark as owl:TransitiveProperty
    pub fn mark_transitive(&self, conn: &mut Connection, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.property.iri, rdf::TYPE, Object::Iri(owl::TRANSITIVE_PROPERTY.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Mark as owl:SymmetricProperty
    pub fn mark_symmetric(&self, conn: &mut Connection, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.property.iri, rdf::TYPE, Object::Iri(owl::SYMMETRIC_PROPERTY.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }
}

/// DatatypeProperty (relates individuals to literals)
#[derive(Debug, Clone)]
pub struct DatatypeProperty {
    pub property: Property,
}

impl DatatypeProperty {
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            property: Property::new(iri),
        }
    }

    /// Assert as owl:DatatypeProperty
    pub fn assert(&self, conn: &mut Connection, origin: &str) -> Result<()> {
        self.property.assert_property(conn, PropertyType::DatatypeProperty, origin)
    }

    /// Mark as owl:FunctionalProperty
    pub fn mark_functional(&self, conn: &mut Connection, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.property.iri, rdf::TYPE, Object::Iri(owl::FUNCTIONAL_PROPERTY.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }
}

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
    fn test_assert_object_property() {
        let mut conn = setup_test_db();
        let prop = ObjectProperty::new("foundation:hasParent");

        let result = prop.assert(&mut conn, "test");
        assert!(result.is_ok());
        assert!(prop.property.exists(&conn).unwrap());
    }

    #[test]
    fn test_add_domain_range() {
        let mut conn = setup_test_db();
        let prop = Property::new("foundation:hasAge");

        prop.assert_property(&mut conn, PropertyType::DatatypeProperty, "test").unwrap();
        prop.add_domain(&mut conn, "foundation:Person", "test").unwrap();
        prop.add_range(&mut conn, "xsd:integer", "test").unwrap();

        let domains = prop.get_domains(&conn).unwrap();
        let ranges = prop.get_ranges(&conn).unwrap();

        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0], "foundation:Person");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], "xsd:integer");
    }

    #[test]
    fn test_object_property_inverse() {
        let mut conn = setup_test_db();
        let prop = ObjectProperty::new("foundation:hasChild");

        prop.assert(&mut conn, "test").unwrap();
        prop.add_inverse(&mut conn, "foundation:hasParent", "test").unwrap();

        let result = query::get_by_entity_predicate(&conn, "foundation:hasChild", owl::INVERSE_OF).unwrap();
        assert_eq!(result.triples.len(), 1);
    }

    #[test]
    fn test_mark_functional() {
        let mut conn = setup_test_db();
        let prop = ObjectProperty::new("foundation:hasMother");

        prop.assert(&mut conn, "test").unwrap();
        prop.mark_functional(&mut conn, "test").unwrap();

        let result = query::get_by_entity_predicate(&conn, "foundation:hasMother", rdf::TYPE).unwrap();
        assert!(result.triples.iter().any(|t| {
            if let Object::Iri(iri) = &t.object {
                iri == owl::FUNCTIONAL_PROPERTY
            } else {
                false
            }
        }));
    }
}
