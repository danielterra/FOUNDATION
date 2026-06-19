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
                r.triples.first().and_then(|t| match &t.object {
                    crate::eavto::Object::Iri(icon_iri) => crate::owl::icon_iri_to_display(conn, icon_iri),
                    crate::eavto::Object::Literal { value, .. } =>
                        Some(crate::owl::icon_literal_to_display(value)),
                    _ => None,
                })
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
            has_icon: Option<crate::eavto::Object>,
        }

        if iris.is_empty() {
            return HashMap::new();
        }
        let predicates = &[rdfs::LABEL, "foundation:hasIcon"];
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
                has_icon: None,
            });
            match predicate.as_str() {
                p if p == rdfs::LABEL => {
                    if entry.label.is_none() { entry.label = object.as_literal(); }
                }
                "foundation:hasIcon" => {
                    if entry.has_icon.is_none() { entry.has_icon = Some(object); }
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
                .and_then(|m| m.has_icon.as_ref())
                .and_then(|obj| match obj {
                    crate::eavto::Object::Iri(icon_iri) => crate::owl::icon_iri_to_display(conn, icon_iri),
                    crate::eavto::Object::Literal { value, .. } =>
                        Some(crate::owl::icon_literal_to_display(value)),
                    _ => None,
                });
            (iri.clone(), Thing { iri: iri.clone(), label, icon })
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Triple, Object};
    use crate::owl::vocabulary::rdfs;

    #[test]
    fn test_get_batch_empty_slice_returns_empty_map() {
        let conn = setup_test_db();
        let result = Thing::get_batch(&conn, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_batch_unknown_iri_uses_iri_as_label() {
        let conn = setup_test_db();
        let iris = vec!["foundation:Unknown".to_string()];
        let result = Thing::get_batch(&conn, &iris);
        let thing = result.get("foundation:Unknown").unwrap();
        assert_eq!(thing.iri, "foundation:Unknown");
        assert_eq!(thing.label, "foundation:Unknown");
        assert!(thing.icon.is_none());
    }

    #[test]
    fn test_get_batch_returns_label_for_known_entity() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:MyEntity", rdfs::LABEL, Object::Literal {
                value: "My Entity".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let iris = vec!["foundation:MyEntity".to_string()];
        let result = Thing::get_batch(&conn, &iris);
        let thing = result.get("foundation:MyEntity").unwrap();
        assert_eq!(thing.label, "My Entity");
        assert!(thing.icon.is_none());
    }

    #[test]
    fn test_get_batch_returns_icon_literal() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:MyEntity", rdfs::LABEL, Object::Literal {
                value: "My Entity".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:MyEntity", "foundation:hasIcon", Object::Literal {
                value: "https://example.com/icon.png".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let iris = vec!["foundation:MyEntity".to_string()];
        let result = Thing::get_batch(&conn, &iris);
        let thing = result.get("foundation:MyEntity").unwrap();
        assert_eq!(thing.icon, Some("https://example.com/icon.png".to_string()));
    }

    #[test]
    fn test_get_batch_loads_multiple_entities() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:EntityA", rdfs::LABEL, Object::Literal {
                value: "Entity A".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:EntityB", rdfs::LABEL, Object::Literal {
                value: "Entity B".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let iris = vec![
            "foundation:EntityA".to_string(),
            "foundation:EntityB".to_string(),
        ];
        let result = Thing::get_batch(&conn, &iris);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("foundation:EntityA").unwrap().label, "Entity A");
        assert_eq!(result.get("foundation:EntityB").unwrap().label, "Entity B");
    }

    #[test]
    fn test_get_batch_mixed_known_and_unknown() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Known", rdfs::LABEL, Object::Literal {
                value: "Known Entity".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let iris = vec![
            "foundation:Known".to_string(),
            "foundation:Unknown".to_string(),
        ];
        let result = Thing::get_batch(&conn, &iris);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("foundation:Known").unwrap().label, "Known Entity");
        assert_eq!(result.get("foundation:Unknown").unwrap().label, "foundation:Unknown");
    }
}
