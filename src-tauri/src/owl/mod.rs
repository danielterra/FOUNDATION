// ============================================================================
// OWL Module - RDF/RDFS/OWL Abstraction Layer
// ============================================================================
// Provides high-level interfaces for managing RDF, RDFS, and OWL concepts
// over the EAVTO triple store.
//
// Architecture:
// - Abstracts EAVTO triple operations into semantic operations
// - Provides type-safe interfaces for Classes, Properties, and Individuals
// - Implements RDF/RDFS/OWL vocabulary operations
// ============================================================================

mod class;
mod property;
mod individual;
pub mod vocabulary;

pub use class::{Class, ClassType};
pub use property::{Property, ObjectProperty, DatatypeProperty, PropertyType};
pub use individual::Individual;
pub use vocabulary::Vocabulary;

use rusqlite::Connection;
use crate::eavto::{store, Triple, Object};

/// OWL operation errors
#[derive(Debug)]
pub enum OwlError {
    DatabaseError(String),
    ValidationError(String),
    NotFound(String),
}

impl std::fmt::Display for OwlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OwlError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            OwlError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            OwlError::NotFound(msg) => write!(f, "Not found: {}", msg),
        }
    }
}

impl std::error::Error for OwlError {}

impl From<rusqlite::Error> for OwlError {
    fn from(err: rusqlite::Error) -> Self {
        OwlError::DatabaseError(err.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for OwlError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        OwlError::DatabaseError(err.to_string())
    }
}

type Result<T> = std::result::Result<T, OwlError>;

/// Import triples with semantic awareness
/// This function intelligently routes triples to appropriate OWL operations
pub fn import_triples(conn: &mut Connection, triples: &[Triple], origin: &str) -> Result<u64> {
    use vocabulary::{rdf, rdfs, owl};

    let mut facts_inserted = 0u64;

    for triple in triples {
        // Detect semantic patterns and use appropriate OWL operations
        match (triple.predicate.as_str(), &triple.object) {
            // Class declarations
            (rdf::TYPE, Object::Iri(class_type)) if class_type == rdfs::CLASS || class_type == owl::CLASS => {
                let class = Class::new(&triple.subject);
                let class_type_enum = if class_type == owl::CLASS {
                    class::ClassType::OwlClass
                } else {
                    class::ClassType::RdfsClass
                };
                class.assert_class(conn, class_type_enum, origin)?;
                facts_inserted += 1;
            }

            // Property declarations
            (rdf::TYPE, Object::Iri(prop_type)) if prop_type == owl::OBJECT_PROPERTY => {
                let prop = ObjectProperty::new(&triple.subject);
                prop.assert(conn, origin)?;
                facts_inserted += 1;
            }

            (rdf::TYPE, Object::Iri(prop_type)) if prop_type == owl::DATATYPE_PROPERTY => {
                let prop = DatatypeProperty::new(&triple.subject);
                prop.assert(conn, origin)?;
                facts_inserted += 1;
            }

            // Class hierarchy
            (rdfs::SUB_CLASS_OF, Object::Iri(super_class)) => {
                let class = Class::new(&triple.subject);
                class.add_super_class(conn, super_class, origin)?;
                facts_inserted += 1;
            }

            // Labels
            (rdfs::LABEL, Object::Literal { value, language, .. }) => {
                // Try as class first, then property, then individual
                let class = Class::new(&triple.subject);
                class.add_label(conn, value, language.as_deref(), origin)?;
                facts_inserted += 1;
            }

            // Domain and Range
            (rdfs::DOMAIN, Object::Iri(domain_class)) => {
                let prop = Property::new(&triple.subject);
                prop.add_domain(conn, domain_class, origin)?;
                facts_inserted += 1;
            }

            (rdfs::RANGE, Object::Iri(range_class)) => {
                let prop = Property::new(&triple.subject);
                prop.add_range(conn, range_class, origin)?;
                facts_inserted += 1;
            }

            // Instance type declarations
            (rdf::TYPE, Object::Iri(class_iri)) => {
                let individual = Individual::new(&triple.subject);
                individual.assert_type(conn, class_iri, origin)?;
                facts_inserted += 1;
            }

            // Generic triples - fallback to direct EAVTO storage
            _ => {
                store::assert_triples(conn, &[triple.clone()], origin)?;
                facts_inserted += 1;
            }
        }
    }

    Ok(facts_inserted)
}
