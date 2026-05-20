mod class;
mod property;
mod individual;
mod thing;
mod icons;
mod graph_config;
mod search;
pub mod vocabulary;
pub mod cardinality;
pub mod aggregation;
pub mod formula;
pub mod formula_worker;
pub mod query_property;
pub mod query_worker;

pub use graph_config::{load_graph_node_groups, get_graph_node_type_config, GraphNodeTypeConfig};

pub use icons::{validate_icon, icon_name_to_iri, icon_iri_to_display, icon_literal_to_display, icon_store_value, seed_icon_library};

pub use class::{Class, ClassType};
pub use property::{Property, PropertyType, DomainLabel};
pub use individual::Individual;
pub use thing::Thing;
pub use crate::eavto::Object;
pub use crate::eavto::Connection;
pub use crate::eavto::DbExecutor;
pub use crate::eavto::initialize_with_progress;
pub use crate::eavto::get_stats;

pub use individual::{
    is_system_locked, set_system_locked, check_system_locked,
    get_all_iri_properties, replace_all_property_iris,
    get_literal_property, get_all_literal_properties, get_iri_property,
    has_property_iri, has_property_literal,
    is_instance_of, is_subclass_of, find_entities_with_property, find_entities_with_predicate,
    validate_allowed_status, resolve_status_appearance, get_entity_status_info,
    touch, LAST_UPDATED_AT,
};

pub use search::{
    SearchResult, ClassSearchResult,
    search, search_instances, search_classes, search_individuals,
    try_iri_direct_lookup,
};

#[derive(Debug)]
pub enum OwlError {
    DatabaseError(String),
    ValidationError(String),
    NotFound(String),
    InvalidOperation(String),
    CardinalityViolation(String),
}

impl std::fmt::Display for OwlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OwlError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            OwlError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            OwlError::NotFound(msg) => write!(f, "Not found: {}", msg),
            OwlError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
            OwlError::CardinalityViolation(msg) => write!(f, "Cardinality violation: {}", msg),
        }
    }
}

impl std::error::Error for OwlError {}

impl From<crate::eavto::connection::DbError> for OwlError {
    fn from(err: crate::eavto::connection::DbError) -> Self {
        OwlError::DatabaseError(err.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for OwlError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        OwlError::DatabaseError(err.to_string())
    }
}

type Result<T> = std::result::Result<T, OwlError>;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
