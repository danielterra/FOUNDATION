// ============================================================================
// OWL Thing - Basic Entity Operations
// ============================================================================
// Represents owl:Thing - the most basic entity with just metadata
// All entities (classes, individuals) are ultimately Things
// ============================================================================

use rusqlite::Connection;
use crate::eavto::query;
use crate::owl::vocabulary::rdfs;
use serde::Serialize;

/// Represents owl:Thing - basic entity with metadata only
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Thing {
    pub iri: String,
    pub label: String,
    pub icon: Option<String>,
}

impl Thing {
    /// Get basic entity info (id, label, icon only - no relationships)
    /// If no rdfs:label exists, returns the IRI as label
    pub fn get(conn: &Connection, iri: impl Into<String>) -> Thing {
        let iri = iri.into();

        let label = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL)
            .ok()
            .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
            .unwrap_or_else(|| iri.clone());

        let icon = query::get_by_entity_predicate(conn, &iri, "foundation:icon")
            .ok()
            .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()));

        Thing {
            iri,
            label,
            icon,
        }
    }
}
