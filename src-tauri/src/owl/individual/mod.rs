use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, Thing, Class, vocabulary::{rdf, rdfs}};

mod core;
mod validation;
mod write;
mod find;
mod properties;
mod lock;
mod status;
mod timestamps;

pub use timestamps::{touch, LAST_UPDATED_AT};

pub use properties::{
    get_all_iri_properties, replace_all_property_iris,
    get_literal_property, get_iri_property,
    has_property_iri, has_property_literal,
    is_instance_of, find_entities_with_property, find_entities_with_predicate,
};
pub use lock::{is_system_locked, set_system_locked, check_system_locked};
pub use status::{validate_allowed_status, resolve_status_appearance, get_entity_status_info};

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
    pub property_tx: Vec<i64>, // transaction IDs parallel to properties
    pub backlinks: Vec<crate::eavto::query::BacklinkRow>,
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
            property_tx: Vec::new(),
            backlinks: Vec::new(),
        }
    }
}
