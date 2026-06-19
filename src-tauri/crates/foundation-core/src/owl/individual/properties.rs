use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, vocabulary};

#[cfg(test)]
#[path = "properties_tests.rs"]
mod tests;

/// Returns all current values for a predicate on an entity as strings,
/// preferring the IRI form and falling back to the literal form (COALESCE semantics).
/// Preserves insertion order. Returns an empty Vec when the predicate is absent.
pub fn get_all_property_values(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<Vec<String>> {
    let result = query::get_by_entity_predicate(conn, entity, predicate)?;
    Ok(result.triples.iter()
        .filter_map(|t| t.object.as_iri().map(|s| s.to_string()).or_else(|| t.object.as_literal()))
        .collect())
}

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

/// Replace all literal (xsd:string) values for a predicate on an entity with a new set
pub fn replace_all_property_literals(
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
        .map(|value| Triple::new(entity, predicate, Object::Literal {
            value: value.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }))
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

/// Returns all literal values for a predicate on an entity
pub fn get_all_literal_properties(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<Vec<String>> {
    let result = query::get_by_entity_predicate(conn, entity, predicate)?;
    Ok(result.triples.iter()
        .filter_map(|t| t.object.as_literal())
        .map(|s| s.to_string())
        .collect())
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

/// Returns true if `child_iri` is equal to or a (transitive) subclass of `ancestor_iri`,
/// walking rdfs:subClassOf upward via BFS. Stops at owl:Thing to avoid infinite loops.
pub fn is_subclass_of(conn: &Connection, child_iri: &str, ancestor_iri: &str) -> bool {
    if child_iri == ancestor_iri {
        return true;
    }
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![child_iri.to_string()];
    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if let Ok(supers) = get_all_iri_properties(conn, &current, vocabulary::rdfs::SUB_CLASS_OF) {
            for s in supers {
                if s == ancestor_iri {
                    return true;
                }
                if s != "owl:Thing" {
                    queue.push(s);
                }
            }
        }
    }
    false
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

/// Bound-only paginated variant: returns up to `limit` IRIs starting at `offset`.
///
/// Use when the set is bounded-by-nature (e.g. AI services, IMAP accounts) and
/// keyset-tx does not apply (entities are not accumulated in append-on-top order).
///
/// `order_predicate`:
/// - `None` — preserves the original `ORDER BY t.subject` behaviour (no change).
/// - `Some(pred)` — orders DESC by the current value of `pred`
///   (COALESCE(object, object_value)), with NULLs last, tie-broken by subject ASC.
pub fn find_entities_with_property_bounded(
    conn: &Connection,
    predicate: &str,
    object: &str,
    limit: i64,
    offset: i64,
    order_predicate: Option<&str>,
) -> Result<Vec<String>> {
    use rusqlite::types::Value as SqlValue;
    let (sql, params): (String, Vec<SqlValue>) = match order_predicate {
        None => {
            let sql = String::from(
                "SELECT t.subject FROM triples_current t \
                 WHERE t.predicate = ? AND t.object = ? \
                 ORDER BY t.subject \
                 LIMIT ? OFFSET ?",
            );
            let params = vec![
                SqlValue::Text(predicate.to_string()),
                SqlValue::Text(object.to_string()),
                SqlValue::Integer(limit),
                SqlValue::Integer(offset),
            ];
            (sql, params)
        }
        Some(ord_pred) => {
            let sql = String::from(
                "SELECT t.subject \
                 FROM triples_current t \
                 LEFT JOIN triples_current tord \
                   ON tord.subject = t.subject \
                  AND tord.predicate = ?3 \
                 WHERE t.predicate = ?1 AND t.object = ?2 \
                 ORDER BY COALESCE(tord.object, tord.object_value) DESC NULLS LAST, \
                          t.subject ASC \
                 LIMIT ?4 OFFSET ?5",
            );
            let params = vec![
                SqlValue::Text(predicate.to_string()),
                SqlValue::Text(object.to_string()),
                SqlValue::Text(ord_pred.to_string()),
                SqlValue::Integer(limit),
                SqlValue::Integer(offset),
            ];
            (sql, params)
        }
    };
    let mut stmt = conn.prepare(&sql).map_err(|e| OwlError::DatabaseError(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| row.get(0))
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
    Ok(rows)
}

/// Keyset-paginated variant: returns (iri, creation_tx) pairs for entities where
/// `predicate` = `object`, ordered by creation_tx DESC, limited to `limit`.
/// `after_tx` is exclusive — only entities whose creation tx is strictly less than
/// this value are returned. Pass `None` to start from the most recent.
///
/// "creation_tx" is the tx of the predicate=object triple itself, which is written
/// exactly once at creation time (e.g. rdf:type). This makes it a stable, monotonic
/// cursor suitable for keyset pagination.
pub fn find_entities_with_property_keyset(
    conn: &Connection,
    predicate: &str,
    object: &str,
    after_tx: Option<i64>,
    limit: i64,
) -> Result<Vec<(String, i64)>> {
    use rusqlite::types::Value as SqlValue;
    let mut sql = String::from(
        "SELECT t.subject, t.tx \
         FROM triples_current t \
         WHERE t.predicate = ? \
           AND t.object = ?",
    );
    let mut params: Vec<SqlValue> = vec![
        SqlValue::Text(predicate.to_string()),
        SqlValue::Text(object.to_string()),
    ];
    if let Some(cursor) = after_tx {
        sql.push_str(" AND t.tx < ?");
        params.push(SqlValue::Integer(cursor));
    }
    sql.push_str(" ORDER BY t.tx DESC LIMIT ?");
    params.push(SqlValue::Integer(limit));

    let mut stmt = conn.prepare(&sql).map_err(|e| OwlError::DatabaseError(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
    Ok(rows)
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

/// Returns all current triples for an entity (state from `triples_current`).
/// Each (subject, predicate) group reflects the latest TX only — no historical rows.
pub fn get_all_current_triples(
    conn: &Connection,
    entity: &str,
) -> Result<Vec<crate::eavto::Triple>> {
    let result = query::get_by_entity(conn, entity)?;
    Ok(result.triples)
}
