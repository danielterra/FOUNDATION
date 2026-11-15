use crate::owl::{Class, Connection, Individual, Result};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Class,
    Individual,
}

pub fn resolve_entity_kind(conn: &Connection, iri: &str) -> Result<Option<EntityKind>> {
    if Class::exists(conn, iri) {
        return Ok(Some(EntityKind::Class));
    }

    if Individual::get(conn, iri)?.is_some() {
        return Ok(Some(EntityKind::Individual));
    }

    Ok(None)
}

pub fn resolve_entity_kinds(
    conn: &Connection,
    iris: &[String],
) -> Result<HashMap<String, EntityKind>> {
    let mut kinds = HashMap::new();

    for iri in iris {
        if kinds.contains_key(iri) {
            continue;
        }

        if let Some(kind) = resolve_entity_kind(conn, iri)? {
            kinds.insert(iri.clone(), kind);
        }
    }

    Ok(kinds)
}

#[cfg(test)]
mod tests {
    use super::EntityKind;

    #[test]
    fn entity_kind_serializes_for_frontend_discriminator() {
        assert_eq!(
            serde_json::to_string(&EntityKind::Class).unwrap(),
            "\"class\""
        );
        assert_eq!(
            serde_json::to_string(&EntityKind::Individual).unwrap(),
            "\"individual\""
        );
    }
}
