// ============================================================================
// OWL Class - Class Operations
// ============================================================================
// High-level operations for managing OWL/RDFS classes
// ============================================================================

use rusqlite::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, vocabulary::{rdf, rdfs, owl}};

/// Represents an OWL/RDFS Class
#[derive(Debug, Clone)]
pub struct Class {
    pub iri: String,
}

impl Class {
    /// Create a new Class reference
    pub fn new(iri: impl Into<String>) -> Self {
        Self { iri: iri.into() }
    }

    /// Assert that this IRI is a class (rdf:type rdfs:Class or owl:Class)
    pub fn assert_class(&self, conn: &mut Connection, class_type: ClassType, origin: &str) -> Result<()> {
        let type_iri = match class_type {
            ClassType::RdfsClass => rdfs::CLASS,
            ClassType::OwlClass => owl::CLASS,
        };

        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(type_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add rdfs:label to this class
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

    /// Add rdfs:comment to this class
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

    /// Add rdfs:subClassOf relationship
    pub fn add_super_class(&self, conn: &mut Connection, super_class: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, rdfs::SUB_CLASS_OF, Object::Iri(super_class.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add owl:equivalentClass relationship
    pub fn add_equivalent_class(&self, conn: &mut Connection, equivalent: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, owl::EQUIVALENT_CLASS, Object::Iri(equivalent.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add owl:disjointWith relationship
    pub fn add_disjoint_class(&self, conn: &mut Connection, disjoint: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, owl::DISJOINT_WITH, Object::Iri(disjoint.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Get all super classes (rdfs:subClassOf)
    pub fn get_super_classes(&self, conn: &Connection) -> Result<Vec<String>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdfs::SUB_CLASS_OF)?;
        Ok(result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect())
    }

    /// Get all labels
    pub fn get_labels(&self, conn: &Connection) -> Result<Vec<(String, Option<String>)>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdfs::LABEL)?;
        Ok(result.triples.iter()
            .filter_map(|t| {
                if let Object::Literal { value, language, .. } = &t.object {
                    Some((value.clone(), language.clone()))
                } else {
                    None
                }
            })
            .collect())
    }

    /// Get first label (most common use case)
    pub fn get_label(&self, conn: &Connection) -> Result<Option<String>> {
        let labels = self.get_labels(conn)?;
        Ok(labels.first().map(|(label, _)| label.clone()))
    }

    /// Get comments
    pub fn get_comments(&self, conn: &Connection) -> Result<Vec<String>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdfs::COMMENT)?;
        Ok(result.triples.iter()
            .filter_map(|t| t.object.as_literal())
            .collect())
    }

    /// Get first comment
    pub fn get_comment(&self, conn: &Connection) -> Result<Option<String>> {
        let comments = self.get_comments(conn)?;
        Ok(comments.first().cloned())
    }

    /// Get icon (foundation:icon)
    pub fn get_icon(&self, conn: &Connection) -> Result<Option<String>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, "foundation:icon")?;
        Ok(result.triples.first()
            .and_then(|t| t.object.as_literal()))
    }

    /// Check if this class exists (has rdf:type rdfs:Class or owl:Class)
    pub fn exists(&self, conn: &Connection) -> Result<bool> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;
        Ok(result.triples.iter().any(|t| {
            if let Object::Iri(type_iri) = &t.object {
                type_iri == rdfs::CLASS || type_iri == owl::CLASS
            } else {
                false
            }
        }))
    }

    /// Get all instances of this class
    pub fn get_instances(&self, conn: &Connection) -> Result<Vec<String>> {
        // Query all triples where predicate is rdf:type and object is this class
        let result = query::get_by_predicate(conn, rdf::TYPE)?;
        Ok(result.triples.iter()
            .filter_map(|t| {
                if let Object::Iri(class_iri) = &t.object {
                    if class_iri == &self.iri {
                        Some(t.subject.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect())
    }
}

/// Type of class (RDFS or OWL)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassType {
    RdfsClass,
    OwlClass,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    #[test]
    fn test_assert_owl_class() {
        let mut conn = setup_test_db();
        let class = Class::new("foundation:TestClass");

        let result = class.assert_class(&mut conn, ClassType::OwlClass, "test");
        assert!(result.is_ok());

        // Verify it exists
        assert!(class.exists(&conn).unwrap());
    }

    #[test]
    fn test_add_label() {
        let mut conn = setup_test_db();
        let class = Class::new("foundation:LabelTest");

        class.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        class.add_label(&mut conn, "Test Label", Some("en"), "test").unwrap();

        let labels = class.get_labels(&conn).unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "Test Label");
        assert_eq!(labels[0].1, Some("en".to_string()));
    }

    #[test]
    fn test_add_super_class() {
        let mut conn = setup_test_db();
        let class = Class::new("foundation:SubClass");
        let super_class = Class::new("foundation:SuperClass");

        class.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        super_class.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();
        class.add_super_class(&mut conn, "foundation:SuperClass", "test").unwrap();

        let supers = class.get_super_classes(&conn).unwrap();
        assert_eq!(supers.len(), 1);
        assert_eq!(supers[0], "foundation:SuperClass");
    }

    #[test]
    fn test_get_instances() {
        let mut conn = setup_test_db();
        let class = Class::new("foundation:Person");

        class.assert_class(&mut conn, ClassType::OwlClass, "test").unwrap();

        // Create instances
        let triple1 = Triple::new("foundation:John", rdf::TYPE, Object::Iri("foundation:Person".to_string()));
        let triple2 = Triple::new("foundation:Jane", rdf::TYPE, Object::Iri("foundation:Person".to_string()));
        store::assert_triples(&mut conn, &[triple1, triple2], "test").unwrap();

        let instances = class.get_instances(&conn).unwrap();
        assert_eq!(instances.len(), 2);
        assert!(instances.contains(&"foundation:John".to_string()));
        assert!(instances.contains(&"foundation:Jane".to_string()));
    }
}
