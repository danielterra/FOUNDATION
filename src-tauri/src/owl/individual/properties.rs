use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, vocabulary};

/// Returns all IRI values for a predicate on an entity
pub fn get_all_iri_properties(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<Vec<String>> {
    let result = query::get_by_entity_predicate(conn, entity, predicate)?;
    Ok(result.triples.iter()
        .filter_map(|t| t.object.as_iri())
        .map(|s| s.to_string())
        .collect())
}

/// Replace all IRI values for a predicate on an entity with a new set
pub fn replace_all_property_iris(
    conn: &mut Connection,
    entity: &str,
    predicate: &str,
    values: &[&str],
    origin: &str,
) -> Result<()> {
    let old = query::get_by_entity_predicate(conn, entity, predicate)?;
    for triple in old.triples {
        store::retract_triples(conn, &[Triple::new(entity, predicate, triple.object)], origin)?;
    }
    let new_triples: Vec<Triple> = values.iter()
        .map(|value| Triple::new(entity, predicate, Object::Iri(value.to_string())))
        .collect();
    if !new_triples.is_empty() {
        store::assert_triples(conn, &new_triples, origin)?;
    }
    Ok(())
}

/// Returns the first literal value of a property for an entity
pub fn get_literal_property(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<Option<String>> {
    let result = query::get_by_entity_predicate(conn, entity, predicate)?;
    Ok(result.triples.first().and_then(|t| t.object.as_literal()).map(|s| s.to_string()))
}

/// Returns the first IRI value of a property for an entity
pub fn get_iri_property(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<Option<String>> {
    let result = query::get_by_entity_predicate(conn, entity, predicate)?;
    Ok(result.triples.first().and_then(|t| t.object.as_iri()).map(|s| s.to_string()))
}

/// Returns true if the entity has the given predicate pointing to the given IRI value
pub fn has_property_iri(conn: &Connection, entity: &str, predicate: &str, value: &str) -> bool {
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
    let result = query::get_by_predicate_object(conn, predicate, object)?;
    Ok(result.triples.into_iter().map(|t| t.subject).collect())
}

pub fn find_entities_with_predicate(
    conn: &Connection,
    predicate: &str,
) -> Result<Vec<String>> {
    let result = query::get_by_predicate(conn, predicate)?;
    let mut seen = std::collections::HashSet::new();
    Ok(result.triples.into_iter()
        .map(|t| t.subject)
        .filter(|s| seen.insert(s.clone()))
        .collect())
}
