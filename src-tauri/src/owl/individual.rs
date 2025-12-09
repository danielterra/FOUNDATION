// ============================================================================
// OWL Individual - Individual/Instance Operations
// ============================================================================
// High-level operations for managing individuals (instances of classes)
//
// IMPORTANT: Individuals are instances, NOT classes
// - Individuals use rdf:type to declare their class
// - Individuals NEVER use rdfs:subClassOf (that's for classes)
// - Example: foundation:John rdf:type foundation:Person (NOT subClassOf)
// ============================================================================

use rusqlite::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, Thing, Class, vocabulary::{rdf, rdfs}};

/// Represents an OWL Individual (instance of a class)
///
/// An Individual is an instance of a Class, not a Class itself.
/// It uses rdf:type to declare its class membership.
///
/// Example:
/// ```text
/// foundation:John rdf:type foundation:Person .  // John is an instance
/// foundation:Person rdf:type owl:Class .         // Person is a class
/// ```
#[derive(Debug, Clone)]
pub struct Individual {
    pub iri: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub comment: Option<String>,
    pub types: Vec<Thing>,
    pub properties: Vec<(String, Object)>, // (property_iri, value)
}

impl Individual {
    /// Create a new empty Individual reference (only IRI)
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            iri: iri.into(),
            label: None,
            icon: None,
            comment: None,
            types: Vec::new(),
            properties: Vec::new(),
        }
    }

    /// Get complete individual data from database
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

        // Get types (classes)
        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE)?;
        let types: Vec<Thing> = types_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|type_iri| Thing::get(conn, type_iri))
            .collect();

        // Get all properties (excluding metadata)
        let all_triples = query::get_by_entity(conn, &iri)?;
        let properties: Vec<(String, Object)> = all_triples.triples.into_iter()
            .filter(|t| {
                t.predicate != rdf::TYPE
                    && t.predicate != rdfs::LABEL
                    && t.predicate != rdfs::COMMENT
                    && t.predicate != "foundation:icon"
            })
            .map(|t| (t.predicate, t.object))
            .collect();

        Ok(Self {
            iri,
            label,
            icon,
            comment,
            types,
            properties,
        })
    }

    /// Assert individual with required metadata (label and icon)
    /// This is the recommended way to create individuals
    pub fn assert(
        &self,
        conn: &mut Connection,
        class_iri: &str,
        label: &str,
        icon: &str,
        origin: &str
    ) -> Result<()> {
        // Create individual type
        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(class_iri.to_string()));
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

        Ok(())
    }

    /// Add a property to this individual
    /// Validates that the property is defined in the individual's class or inherited from parent classes
    pub fn add_property(&self, conn: &mut Connection, property: &str, value: Object, origin: &str) -> Result<()> {
        // Get individual's types (classes)
        let types_result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;

        if types_result.triples.is_empty() {
            return Err(OwlError::NotFound(format!("Individual {} has no rdf:type", self.iri)));
        }

        // Check if property is valid for any of the individual's classes
        let mut property_is_valid = false;

        for triple in &types_result.triples {
            if let Some(class_iri) = triple.object.as_iri() {
                // Get class with all its properties (including inherited)
                if let Ok(class) = Class::get(conn, class_iri) {
                    // Check if property exists in this class or its parents
                    if class.properties.iter().any(|(prop_iri, _)| prop_iri == property) {
                        property_is_valid = true;
                        break;
                    }
                }
            }
        }

        if !property_is_valid {
            return Err(OwlError::InvalidOperation(
                format!("Property {} is not defined in any class of individual {}", property, self.iri)
            ));
        }

        // Property is valid, assert the triple
        let triple = Triple::new(&self.iri, property, value);
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Check if this individual exists (has at least one triple)
    pub fn exists(&self, conn: &Connection) -> Result<bool> {
        let result = query::get_by_entity(conn, &self.iri)?;
        Ok(!result.triples.is_empty())
    }
}