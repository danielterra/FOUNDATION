// ============================================================================
// OWL Thing - Basic Entity Operations
// ============================================================================
// Represents owl:Thing - the most basic entity with just metadata
// All entities (classes, individuals) are ultimately Things
// ============================================================================

use crate::eavto::Connection;
use crate::eavto::query;
use crate::owl::vocabulary::rdfs;
use serde::Serialize;
use std::collections::HashMap;

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

        let icon = query::get_by_entity_predicate(conn, &iri, "foundation:hasIcon")
            .ok()
            .and_then(|r| {
                r.triples.first()
                    .and_then(|t| t.object.as_iri())
                    .map(|s| s.to_string())
            })
            .and_then(|icon_iri| crate::owl::icon_iri_to_display(conn, &icon_iri))
            .or_else(|| {
                query::get_by_entity_predicate(conn, &iri, "foundation:icon")
                    .ok()
                    .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
            });

        Thing {
            iri,
            label,
            icon,
        }
    }

    /// Batch-load metadata for multiple entities in a single SQL query.
    /// Entities with no label in the store use their IRI as the label.
    pub fn get_batch(conn: &Connection, iris: &[String]) -> HashMap<String, Thing> {
        struct RawMetadata {
            label: Option<String>,
            icon_literal: Option<String>,
            has_icon_iri: Option<String>,
        }

        if iris.is_empty() {
            return HashMap::new();
        }
        let predicates = &[rdfs::LABEL, "foundation:icon", "foundation:hasIcon"];
        let rows = match query::get_predicates_for_subjects(conn, iris, predicates) {
            Ok(r) => r,
            Err(_) => return iris.iter()
                .map(|iri| (iri.clone(), Thing { iri: iri.clone(), label: iri.clone(), icon: None }))
                .collect(),
        };

        let mut raw: HashMap<String, RawMetadata> = HashMap::new();
        for (subject, predicate, object) in rows {
            let entry = raw.entry(subject).or_insert(RawMetadata {
                label: None,
                icon_literal: None,
                has_icon_iri: None,
            });
            match predicate.as_str() {
                p if p == rdfs::LABEL => {
                    if entry.label.is_none() { entry.label = object.as_literal(); }
                }
                "foundation:icon" => {
                    if entry.icon_literal.is_none() { entry.icon_literal = object.as_literal(); }
                }
                "foundation:hasIcon" => {
                    if entry.has_icon_iri.is_none() {
                        entry.has_icon_iri = object.as_iri().map(|s| s.to_string());
                    }
                }
                _ => {}
            }
        }

        iris.iter().map(|iri| {
            let metadata = raw.get(iri);
            let label = metadata
                .and_then(|m| m.label.clone())
                .unwrap_or_else(|| iri.clone());
            let icon = metadata
                .and_then(|m| m.has_icon_iri.as_deref())
                .and_then(|icon_iri| crate::owl::icon_iri_to_display(conn, icon_iri))
                .or_else(|| metadata.and_then(|m| m.icon_literal.clone()));
            (iri.clone(), Thing { iri: iri.clone(), label, icon })
        }).collect()
    }
}
