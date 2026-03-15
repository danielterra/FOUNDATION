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
    pub async fn get(conn: &Connection, iri: impl Into<String>) -> Thing {
        let iri = iri.into();

        let label = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL).await
            .ok()
            .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
            .unwrap_or_else(|| iri.clone());

        let icon = if let Some(icon_result) = query::get_by_entity_predicate(conn, &iri, "foundation:hasIcon").await.ok() {
            if let Some(triple) = icon_result.triples.first() {
                match &triple.object {
                    crate::eavto::Object::Iri(icon_iri) => crate::owl::icon_iri_to_display(conn, icon_iri).await,
                    crate::eavto::Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        };
        let icon = if icon.is_some() {
            icon
        } else {
            query::get_by_entity_predicate(conn, &iri, "foundation:icon").await
                .ok()
                .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
        };

        Thing {
            iri,
            label,
            icon,
        }
    }

    /// Synchronous alias for `get` — for use in sync contexts that cannot await.
    /// Falls back to IRI as label when DB is not accessible.
    pub fn get_sync(_conn: &Connection, iri: impl Into<String>) -> Thing {
        let iri: String = iri.into();
        Thing { iri: iri.clone(), label: iri, icon: None }
    }

    /// Batch-load metadata for multiple entities in a single SQL query.
    /// Entities with no label in the store use their IRI as the label.
    pub async fn get_batch(conn: &Connection, iris: &[String]) -> HashMap<String, Thing> {
        struct RawMetadata {
            label: Option<String>,
            icon_literal: Option<String>,
            has_icon: Option<crate::eavto::Object>,
        }

        if iris.is_empty() {
            return HashMap::new();
        }
        let predicates = &[rdfs::LABEL, "foundation:icon", "foundation:hasIcon"];
        let rows = match query::get_predicates_for_subjects(conn, iris, predicates).await {
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
                has_icon: None,
            });
            match predicate.as_str() {
                p if p == rdfs::LABEL => {
                    if entry.label.is_none() { entry.label = object.as_literal(); }
                }
                "foundation:icon" => {
                    if entry.icon_literal.is_none() { entry.icon_literal = object.as_literal(); }
                }
                "foundation:hasIcon" => {
                    if entry.has_icon.is_none() { entry.has_icon = Some(object); }
                }
                _ => {}
            }
        }

        let mut result = HashMap::new();
        for iri in iris {
            let metadata = raw.get(iri);
            let label = metadata
                .and_then(|m| m.label.clone())
                .unwrap_or_else(|| iri.clone());
            let icon = if let Some(obj) = metadata.and_then(|m| m.has_icon.as_ref()) {
                match obj {
                    crate::eavto::Object::Iri(icon_iri) => {
                        crate::owl::icon_iri_to_display(conn, icon_iri).await
                            .or_else(|| metadata.and_then(|m| m.icon_literal.clone()))
                    }
                    crate::eavto::Object::Literal { value, .. } => Some(value.clone()),
                    _ => metadata.and_then(|m| m.icon_literal.clone()),
                }
            } else {
                metadata.and_then(|m| m.icon_literal.clone())
            };
            result.insert(iri.clone(), Thing { iri: iri.clone(), label, icon });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Triple, Object};
    use crate::owl::vocabulary::rdfs;

    #[tokio::test]
    async fn test_get_batch_empty_slice_returns_empty_map() {
        let conn = setup_test_db().await;
        let result = Thing::get_batch(&conn, &[]).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_get_batch_unknown_iri_uses_iri_as_label() {
        let conn = setup_test_db().await;
        let iris = vec!["foundation:Unknown".to_string()];
        let result = Thing::get_batch(&conn, &iris).await;
        let thing = result.get("foundation:Unknown").unwrap();
        assert_eq!(thing.iri, "foundation:Unknown");
        assert_eq!(thing.label, "foundation:Unknown");
        assert!(thing.icon.is_none());
    }

    #[tokio::test]
    async fn test_get_batch_returns_label_for_known_entity() {
        let mut conn = setup_test_db().await;
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:MyEntity", rdfs::LABEL, Object::Literal {
                value: "My Entity".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").await.unwrap();

        let iris = vec!["foundation:MyEntity".to_string()];
        let result = Thing::get_batch(&conn, &iris).await;
        let thing = result.get("foundation:MyEntity").unwrap();
        assert_eq!(thing.label, "My Entity");
        assert!(thing.icon.is_none());
    }

    #[tokio::test]
    async fn test_get_batch_returns_icon_literal() {
        let mut conn = setup_test_db().await;
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:MyEntity", rdfs::LABEL, Object::Literal {
                value: "My Entity".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:MyEntity", "foundation:icon", Object::Literal {
                value: "star".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").await.unwrap();

        let iris = vec!["foundation:MyEntity".to_string()];
        let result = Thing::get_batch(&conn, &iris).await;
        let thing = result.get("foundation:MyEntity").unwrap();
        assert_eq!(thing.icon, Some("star".to_string()));
    }

    #[tokio::test]
    async fn test_get_batch_loads_multiple_entities() {
        let mut conn = setup_test_db().await;
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
        ], "test").await.unwrap();

        let iris = vec![
            "foundation:EntityA".to_string(),
            "foundation:EntityB".to_string(),
        ];
        let result = Thing::get_batch(&conn, &iris).await;
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("foundation:EntityA").unwrap().label, "Entity A");
        assert_eq!(result.get("foundation:EntityB").unwrap().label, "Entity B");
    }

    #[tokio::test]
    async fn test_get_batch_mixed_known_and_unknown() {
        let mut conn = setup_test_db().await;
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Known", rdfs::LABEL, Object::Literal {
                value: "Known Entity".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").await.unwrap();

        let iris = vec![
            "foundation:Known".to_string(),
            "foundation:Unknown".to_string(),
        ];
        let result = Thing::get_batch(&conn, &iris).await;
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("foundation:Known").unwrap().label, "Known Entity");
        assert_eq!(result.get("foundation:Unknown").unwrap().label, "foundation:Unknown");
    }
}
