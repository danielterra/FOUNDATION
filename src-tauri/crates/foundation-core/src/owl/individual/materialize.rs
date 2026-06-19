use std::collections::HashMap;
use crate::eavto::{Connection, query};

#[cfg(test)]
#[path = "materialize_tests.rs"]
mod tests;

/// Typed scalar extracted from a triple object.
#[derive(Debug, Clone)]
pub enum ShallowValue {
    Literal(String),
    Iri(String),
}

/// Materializes the current (latest-TX) triples of a single entity as a flat
/// predicate → values map.  No Foundation-specific IRIs are referenced here;
/// the caller owns all domain knowledge.
///
/// - Literal objects → ShallowValue::Literal (string representation)
/// - IRI objects → ShallowValue::Iri
/// - Multi-valued predicates accumulate all values in the Vec
pub fn materialize_individual_shallow(
    conn: &Connection,
    iri: &str,
) -> HashMap<String, Vec<ShallowValue>> {
    let result = match query::get_by_entity(conn, iri) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };

    let mut map: HashMap<String, Vec<ShallowValue>> = HashMap::new();

    for triple in result.triples {
        let sv = if let Some(iri_val) = triple.object.as_iri() {
            ShallowValue::Iri(iri_val.to_string())
        } else if let Some(lit) = triple.object.as_literal() {
            ShallowValue::Literal(lit)
        } else {
            continue;
        };
        map.entry(triple.predicate).or_default().push(sv);
    }

    map
}
