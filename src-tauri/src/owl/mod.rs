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
mod thing;
pub mod vocabulary;

pub use class::{Class, ClassType};
pub use property::{Property, ObjectProperty, DatatypeProperty, PropertyType};
pub use individual::Individual;
pub use thing::Thing;
pub use crate::eavto::Object;

use rusqlite::Connection;

/// OWL operation errors
#[derive(Debug)]
pub enum OwlError {
    DatabaseError(String),
    ValidationError(String),
    NotFound(String),
    InvalidOperation(String),
}

impl std::fmt::Display for OwlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OwlError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            OwlError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            OwlError::NotFound(msg) => write!(f, "Not found: {}", msg),
            OwlError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
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

/// Search result for classes and individuals
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub is_class: bool,
}

/// Search for classes by label (case-insensitive, ranked by relevance)
pub fn search_classes(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    use vocabulary::{rdf, rdfs, owl};
    use crate::eavto::query;

    // Get all classes
    let all_classes_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::CLASS)?;

    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    for triple in all_classes_result.triples {
        let class_iri = &triple.subject;

        // Get thing (basic entity info)
        let thing = Thing::get(conn, class_iri);
        let label_lower = thing.label.to_lowercase();

        // Check if matches query (case-insensitive)
        if label_lower.contains(&query_lower) {
            // Calculate relevance score (lower is better)
            let score = if label_lower == query_lower {
                0 // Exact match
            } else if label_lower.starts_with(&query_lower) {
                1 // Starts with query
            } else {
                2 // Contains query
            };

            results.push((score, SearchResult {
                id: class_iri.clone(),
                label: thing.label,
                icon: thing.icon,
                is_class: true,
            }));
        }
    }

    // Sort by relevance score, then label length, then alphabetically
    results.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.label.len().cmp(&b.1.label.len()))
            .then_with(|| a.1.label.cmp(&b.1.label))
    });

    // Take top results and remove scores
    Ok(results.into_iter().take(limit).map(|(_, r)| r).collect())
}

/// Search for individuals by label (case-insensitive, ranked by relevance)
pub fn search_individuals(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    use vocabulary::{rdf, rdfs, owl};
    use crate::eavto::query;

    // Get all entities with rdf:type that are NOT owl:Class
    let all_types_result = query::get_by_predicate(conn, rdf::TYPE)?;

    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    for triple in all_types_result.triples {
        // Skip if it's a class
        if let Object::Iri(type_iri) = &triple.object {
            if type_iri == owl::CLASS {
                continue;
            }
        }

        let individual_iri = &triple.subject;

        // Skip if already processed
        if !seen.insert(individual_iri.clone()) {
            continue;
        }

        let individual = Individual::new(individual_iri);

        // Get label
        let label_result = query::get_by_entity_predicate(conn, individual_iri, rdfs::LABEL)?;
        if let Some(label_triple) = label_result.triples.first() {
            if let Object::Literal { value: label, .. } = &label_triple.object {
                let label_lower = label.to_lowercase();

                // Check if matches query (case-insensitive)
                if label_lower.contains(&query_lower) {
                    // Get icon
                    let icon_result = query::get_by_entity_predicate(conn, individual_iri, "foundation:icon")?;
                    let icon = icon_result.triples.first().and_then(|t| {
                        if let Object::Literal { value, .. } = &t.object {
                            Some(value.clone())
                        } else {
                            None
                        }
                    });

                    // Calculate relevance score (lower is better)
                    let score = if label_lower == query_lower {
                        0 // Exact match
                    } else if label_lower.starts_with(&query_lower) {
                        1 // Starts with query
                    } else {
                        2 // Contains query
                    };

                    results.push((score, SearchResult {
                        id: individual_iri.clone(),
                        label: label.clone(),
                        icon,
                        is_class: false,
                    }));
                }
            }
        }
    }

    // Sort by relevance score, then label length, then alphabetically
    results.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.label.len().cmp(&b.1.label.len()))
            .then_with(|| a.1.label.cmp(&b.1.label))
    });

    // Take top results and remove scores
    Ok(results.into_iter().take(limit).map(|(_, r)| r).collect())
}
