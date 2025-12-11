// ============================================================================
// OWL Class - Class Operations
// ============================================================================
// High-level operations for managing OWL/RDFS classes
// ============================================================================

use rusqlite::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, Thing, vocabulary::{rdf, rdfs, owl}};

/// Represents an OWL/RDFS Class with all its data
#[derive(Debug, Clone)]
pub struct Class {
    pub iri: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub comment: Option<String>,
    pub types: Vec<Thing>, // rdf:type (e.g., owl:Class, rdfs:Class)
    pub super_classes: Vec<Thing>,
    pub sub_classes: Vec<Thing>,
    pub properties: Vec<(String, String)>, // (property_iri, source_class_iri)
    pub backlinks: Vec<(String, String, Object)>, // (source_entity, property_iri, value) - entities that reference this class
}

impl Class {
    /// Create a new empty Class reference (only IRI)
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            iri: iri.into(),
            label: None,
            icon: None,
            comment: None,
            types: Vec::new(),
            super_classes: Vec::new(),
            sub_classes: Vec::new(),
            properties: Vec::new(),
            backlinks: Vec::new(),
        }
    }

    /// Get complete class data from database
    pub fn get(conn: &Connection, iri: impl Into<String>) -> Result<Self> {
        let iri = iri.into();

        // Get label
        let label_result = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL)?;
        let label = label_result.triples.first()
            .and_then(|t| t.object.as_literal());

        // Get icon
        let icon_result = query::get_by_entity_predicate(conn, &iri, "foundation:icon")?;
        let icon = icon_result.triples.first()
            .and_then(|t| t.object.as_literal());

        // Get comment
        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT)?;
        let comment = comment_result.triples.first()
            .and_then(|t| t.object.as_literal());

        // Get types (rdf:type)
        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE)?;
        let types: Vec<Thing> = types_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|type_iri| Thing::get(conn, type_iri))
            .collect();

        // Get super classes with their info (shallow - no recursion)
        let super_result = query::get_by_entity_predicate(conn, &iri, rdfs::SUB_CLASS_OF)?;
        let super_classes: Vec<Thing> = super_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|super_iri| Thing::get(conn, super_iri))
            .collect();

        // Get sub classes with their info (shallow - no recursion)
        let sub_result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, &iri)?;
        let sub_classes: Vec<Thing> = sub_result.triples.iter()
            .map(|t| Thing::get(conn, &t.subject))
            .collect();

        // Get properties with source
        let properties = Self::get_properties(conn, &iri)?;

        // Get backlinks - instances of this class (rdf:type references)
        let backlinks_result = query::get_by_predicate_object(conn, rdf::TYPE, &iri)?;
        let backlinks: Vec<(String, String, Object)> = backlinks_result.triples.iter()
            .map(|t| (t.subject.clone(), rdf::TYPE.to_string(), Object::Iri(iri.clone())))
            .collect();

        Ok(Self {
            iri,
            label,
            icon,
            comment,
            types,
            super_classes,
            sub_classes,
            properties,
            backlinks,
        })
    }

    /// Get all properties for this class (declared, used, and inherited)
    /// Returns Vec<(property_iri, source_class_iri)>
    fn get_properties(
        conn: &Connection,
        class_iri: &str
    ) -> Result<Vec<(String, String)>> {
        let mut all_properties: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Add declared properties (rdfs:domain points to this class)
        let declared_result = query::get_by_predicate_object(conn, rdfs::DOMAIN, class_iri)?;
        for triple in declared_result.triples {
            if seen.insert(triple.subject.clone()) {
                all_properties.push((triple.subject.clone(), class_iri.to_string()));
            }
        }

        // Add inherited properties from superclasses recursively
        let super_result = query::get_by_entity_predicate(conn, class_iri, rdfs::SUB_CLASS_OF)?;
        let super_classes: Vec<String> = super_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        for super_class_iri in super_classes {
            if super_class_iri != "owl:Thing" {
                // Recursively get properties from superclass
                let inherited_props = Self::get_properties(conn, &super_class_iri)?;
                for (prop, source) in inherited_props {
                    if seen.insert(prop.clone()) {
                        all_properties.push((prop, source));
                    }
                }
            }
        }

        Ok(all_properties)
    }

    /// Assert class with required metadata (label and icon)
    /// If super_class is None, automatically assigns owl:Thing as parent
    pub fn assert(
        &self,
        conn: &mut Connection,
        class_type: ClassType,
        label: &str,
        icon: &str,
        super_class: Option<&str>,
        origin: &str
    ) -> Result<()> {
        let type_iri = match class_type {
            ClassType::RdfsClass => rdfs::CLASS,
            ClassType::OwlClass => owl::CLASS,
        };

        // Create class type (instance of owl:Class or rdfs:Class)
        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(type_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;

        // Add required label
        let label_obj = Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let label_triple = Triple::new(&self.iri, rdfs::LABEL, label_obj);
        store::assert_triples(conn, &[label_triple], origin)?;

        // Add required icon
        let icon_obj = Object::Literal {
            value: icon.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let icon_triple = Triple::new(&self.iri, "foundation:icon", icon_obj);
        store::assert_triples(conn, &[icon_triple], origin)?;

        // Add subClassOf relationship (defaults to owl:Thing if not specified)
        let parent = super_class.unwrap_or(owl::THING);
        let subclass_triple = Triple::new(&self.iri, rdfs::SUB_CLASS_OF, Object::Iri(parent.to_string()));
        store::assert_triples(conn, &[subclass_triple], origin)?;

        Ok(())
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

    /// Get all instances of this class (returned as IRIs only)
    /// Call separately when needed - can be thousands of instances
    pub fn get_instances(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let result = query::get_by_predicate_object(conn, rdf::TYPE, class_iri)?;
        Ok(result.triples.iter()
            .map(|t| t.subject.clone())
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
    fn test_assert_and_get_class() {
        let mut conn = setup_test_db();
        let class = Class::new("foundation:TestClass");

        // Assert class with label and icon (will default to owl:Thing as parent)
        let result = class.assert(&mut conn, ClassType::OwlClass, "Test Class", "test-icon", None, "test");
        assert!(result.is_ok());

        // Verify it exists
        assert!(class.exists(&conn).unwrap());

        // Get complete class data
        let class_data = Class::get(&conn, "foundation:TestClass").unwrap();
        assert_eq!(class_data.iri, "foundation:TestClass");
        assert_eq!(class_data.label, Some("Test Class".to_string()));
        assert_eq!(class_data.icon, Some("test-icon".to_string()));
        // Should have owl:Thing as super class
        assert_eq!(class_data.super_classes.len(), 1);
        assert_eq!(class_data.super_classes[0].iri, "owl:Thing");
    }

    #[test]
    fn test_get_instances() {
        let mut conn = setup_test_db();
        let class = Class::new("foundation:Person");

        class.assert(&mut conn, ClassType::OwlClass, "Person", "person-icon", None, "test").unwrap();

        // Create instances
        let triple1 = Triple::new("foundation:John", rdf::TYPE, Object::Iri("foundation:Person".to_string()));
        let triple2 = Triple::new("foundation:Jane", rdf::TYPE, Object::Iri("foundation:Person".to_string()));
        store::assert_triples(&mut conn, &[triple1, triple2], "test").unwrap();

        // Get instances separately
        let instances = Class::get_instances(&conn, "foundation:Person").unwrap();
        assert_eq!(instances.len(), 2);
        assert!(instances.contains(&"foundation:John".to_string()));
        assert!(instances.contains(&"foundation:Jane".to_string()));
    }

    #[test]
    fn test_class_hierarchy() {
        let mut conn = setup_test_db();

        // Create super class (with owl:Thing as parent)
        let super_class = Class::new("foundation:Animal");
        super_class.assert(&mut conn, ClassType::OwlClass, "Animal", "animal-icon", None, "test").unwrap();

        // Create sub class (with Animal as parent)
        let sub_class = Class::new("foundation:Dog");
        sub_class.assert(&mut conn, ClassType::OwlClass, "Dog", "dog-icon", Some("foundation:Animal"), "test").unwrap();

        // Get super class data and check sub classes
        let animal_data = Class::get(&conn, "foundation:Animal").unwrap();
        assert_eq!(animal_data.sub_classes.len(), 1);
        assert_eq!(animal_data.sub_classes[0].iri, "foundation:Dog");

        // Get sub class data and check super classes
        let dog_data = Class::get(&conn, "foundation:Dog").unwrap();
        assert_eq!(dog_data.super_classes.len(), 1);
        assert_eq!(dog_data.super_classes[0].iri, "foundation:Animal");
    }

    #[test]
    fn test_single_subclass_of_relationship() {
        let mut conn = setup_test_db();

        // Create class with explicit parent
        let test_class = Class::new("foundation:TestClass");
        test_class.assert(&mut conn, ClassType::OwlClass, "Test Class", "test-icon", Some("owl:Thing"), "test").unwrap();

        // Get class data
        let class_data = Class::get(&conn, "foundation:TestClass").unwrap();

        // Should have exactly 1 super class
        assert_eq!(class_data.super_classes.len(), 1, "Expected exactly 1 super class, found {}", class_data.super_classes.len());
        assert_eq!(class_data.super_classes[0].iri, "owl:Thing");
    }
}
