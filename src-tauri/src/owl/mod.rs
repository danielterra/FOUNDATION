mod class;
mod property;
mod individual;
mod thing;
pub mod vocabulary;
pub mod cardinality;

pub use class::{Class, ClassType};
pub use property::{Property, PropertyType};
pub use individual::Individual;
pub use thing::Thing;
pub use crate::eavto::Object;
pub use crate::eavto::Connection;
pub use crate::eavto::DbExecutor;
pub use crate::eavto::initialize_with_progress;
pub use crate::eavto::get_stats;

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

/// Search result for classes and individuals
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    #[allow(dead_code)]
    pub is_class: bool,
}

/// Search for classes by label (case-insensitive, ranked by relevance)
pub fn search_classes(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    use vocabulary::{rdf, owl};
    use crate::eavto::query;

    let all_classes_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::CLASS)?;

    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    for triple in all_classes_result.triples {
        let class_iri = &triple.subject;

        let thing = Thing::get(conn, class_iri);
        let label_lower = thing.label.to_lowercase();

        if label_lower.contains(&query_lower) {
            let score = if label_lower == query_lower {
                0
            } else if label_lower.starts_with(&query_lower) {
                1
            } else {
                2
            };

            results.push((score, SearchResult {
                id: class_iri.clone(),
                label: thing.label,
                icon: thing.icon,
                is_class: true,
            }));
        }
    }

    results.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.label.len().cmp(&b.1.label.len()))
            .then_with(|| a.1.label.cmp(&b.1.label))
    });

    Ok(results.into_iter().take(limit).map(|(_, r)| r).collect())
}

/// Search for individuals by label (case-insensitive, ranked by relevance)
pub fn search_individuals(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    use vocabulary::{rdf, rdfs, owl};
    use crate::eavto::query;

    let all_types_result = query::get_by_predicate(conn, rdf::TYPE)?;

    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    for triple in all_types_result.triples {
        if let Object::Iri(type_iri) = &triple.object {
            if type_iri == owl::CLASS {
                continue;
            }
        }

        let individual_iri = &triple.subject;

        if !seen.insert(individual_iri.clone()) {
            continue;
        }

        let _individual = Individual::new(individual_iri);

        let label_result = query::get_by_entity_predicate(conn, individual_iri, rdfs::LABEL)?;
        if let Some(label_triple) = label_result.triples.first() {
            if let Object::Literal { value: label, .. } = &label_triple.object {
                let label_lower = label.to_lowercase();

                if label_lower.contains(&query_lower) {
                    let icon_result = query::get_by_entity_predicate(
                        conn,
                        individual_iri,
                        "foundation:icon",
                    )?;
                    let icon = icon_result.triples.first().and_then(|t| {
                        if let Object::Literal { value, .. } = &t.object {
                            Some(value.clone())
                        } else {
                            None
                        }
                    });

                    let score = if label_lower == query_lower {
                        0
                    } else if label_lower.starts_with(&query_lower) {
                        1
                    } else {
                        2
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

    results.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.label.len().cmp(&b.1.label.len()))
            .then_with(|| a.1.label.cmp(&b.1.label))
    });

    Ok(results.into_iter().take(limit).map(|(_, r)| r).collect())
}

/// Returns the first literal value of a property for an entity
pub fn get_literal_property(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<Option<String>> {
    use crate::eavto::query;
    let result = query::get_by_entity_predicate(conn, entity, predicate)?;
    Ok(result.triples.first().and_then(|t| t.object.as_literal()).map(|s| s.to_string()))
}

/// Returns the first IRI value of a property for an entity
pub fn get_iri_property(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<Option<String>> {
    use crate::eavto::query;
    let result = query::get_by_entity_predicate(conn, entity, predicate)?;
    Ok(result.triples.first().and_then(|t| t.object.as_iri()).map(|s| s.to_string()))
}

/// Returns true if the entity has the given predicate pointing to the given IRI value
pub fn has_property_iri(conn: &Connection, entity: &str, predicate: &str, value: &str) -> bool {
    use crate::eavto::query;
    query::get_by_entity_predicate(conn, entity, predicate)
        .map(|r| {
            r.triples
                .iter()
                .any(|t| t.object.as_iri().map(|iri| iri == value).unwrap_or(false))
        })
        .unwrap_or(false)
}

/// Returns true if the entity has a literal property equal to the given value
pub fn has_property_literal(conn: &Connection, entity: &str, predicate: &str, value: &str) -> bool {
    use crate::eavto::query;
    query::get_by_entity_predicate(conn, entity, predicate)
        .map(|r| {
            r.triples
                .iter()
                .any(|t| t.object.as_literal().map(|v| v == value).unwrap_or(false))
        })
        .unwrap_or(false)
}

/// Returns true if the entity has `rdf:type` pointing to the given class IRI
pub fn is_instance_of(conn: &Connection, entity: &str, class_iri: &str) -> bool {
    has_property_iri(conn, entity, vocabulary::rdf::TYPE, class_iri)
}

/// Returns the IRIs of all entities that have the given predicate pointing to the given object IRI
pub fn find_entities_with_property(
    conn: &Connection,
    predicate: &str,
    object: &str,
) -> Result<Vec<String>> {
    use crate::eavto::query;
    let result = query::get_by_predicate_object(conn, predicate, object)?;
    Ok(result.triples.into_iter().map(|t| t.subject).collect())
}
