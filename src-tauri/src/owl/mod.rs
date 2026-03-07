mod class;
mod property;
mod individual;
mod thing;
mod icons;
pub mod vocabulary;
pub mod cardinality;
pub mod formula;
pub mod formula_worker;

pub use icons::{validate_icon, icon_name_to_iri, icon_iri_to_display, icon_store_value, seed_icon_library, migrate_icon_to_has_icon};

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

        let label_result = query::get_by_entity_predicate(conn, individual_iri, rdfs::LABEL)?;
        if let Some(label_triple) = label_result.triples.first() {
            if let Object::Literal { value: label, .. } = &label_triple.object {
                let label_lower = label.to_lowercase();

                if label_lower.contains(&query_lower) {
                    let icon = {
                        let has_icon_result = query::get_by_entity_predicate(
                            conn, individual_iri, "foundation:hasIcon",
                        )?;
                        let from_has_icon = has_icon_result.triples.first()
                            .and_then(|t| t.object.as_iri())
                            .and_then(|iri| icon_iri_to_display(conn, iri));
                        if from_has_icon.is_some() {
                            from_has_icon
                        } else {
                            let icon_result = query::get_by_entity_predicate(
                                conn, individual_iri, "foundation:icon",
                            )?;
                            icon_result.triples.first().and_then(|t| {
                                if let Object::Literal { value, .. } = &t.object {
                                    Some(value.clone())
                                } else {
                                    None
                                }
                            })
                        }
                    };

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

/// Rich search result for instances, including matched properties, concept type and status.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RichSearchResult {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub matched_properties: Vec<serde_json::Value>,
    pub concept_type: Option<serde_json::Value>,
    pub status: Option<serde_json::Value>,
}

/// Search instances by label and all literal property values (case-insensitive, ranked by relevance).
/// Returns only instances (not classes), enriched with matched properties, concept type and status.
pub fn search_instances_rich(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<RichSearchResult>> {
    use crate::eavto::query;

    let thing_iris = Individual::search(conn)?;

    let query_lower = query.to_lowercase();

    let batch = query::batch_load_triples_for_subjects(conn, &thing_iris)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let mut scored: Vec<(i32, RichSearchResult)> = Vec::new();

    for iri in &thing_iris {
        let triples = match batch.get(iri.as_str()) {
            Some(t) => t,
            None => continue,
        };

        let label = triples.iter()
            .find(|t| t.predicate == vocabulary::rdfs::LABEL)
            .and_then(|t| t.object.as_literal())
            .map(|s| s.to_string())
            .unwrap_or_else(|| iri.clone());

        let icon = triples.iter()
            .find(|t| t.predicate == "foundation:hasIcon")
            .and_then(|t| t.object.as_iri())
            .and_then(|icon_iri| icon_iri_to_display(conn, icon_iri))
            .or_else(|| {
                triples.iter()
                    .find(|t| t.predicate == "foundation:icon")
                    .and_then(|t| t.object.as_literal())
                    .map(|s| s.to_string())
            });

        let label_lower = label.to_lowercase();

        let score: i32;
        let mut matched_properties: Vec<serde_json::Value> = Vec::new();

        if query_lower.is_empty() {
            score = 0;
        } else if label_lower == query_lower {
            score = 3;
        } else if label_lower.starts_with(&query_lower) {
            score = 2;
        } else if label_lower.contains(&query_lower) {
            score = 1;
        } else {
            let mut prop_score = 0i32;
            for triple in triples.iter().filter(|t| {
                t.predicate != vocabulary::rdfs::LABEL
                    && t.predicate != "foundation:icon"
                    && t.predicate != "foundation:hasIcon"
            }) {
                if let Some(val_str) = triple.object.as_literal() {
                    if val_str.to_lowercase().contains(&query_lower) {
                        prop_score += 1;
                        let mut entry = serde_json::json!({
                            "detail_iri": triple.predicate,
                            "value": val_str,
                        });
                        if let Some(dt) = triple.object.datatype() {
                            entry["datatype"] = serde_json::json!(dt);
                        }
                        matched_properties.push(entry);
                    }
                }
            }
            if prop_score == 0 {
                continue;
            }
            matched_properties.dedup_by(|a, b| a["detail_iri"] == b["detail_iri"]);
            score = prop_score;
        }

        let concept_type = triples.iter()
            .find(|t| t.predicate == vocabulary::rdf::TYPE)
            .and_then(|t| t.object.as_iri())
            .filter(|type_iri| {
                !type_iri.starts_with("owl:")
                    && !type_iri.starts_with("rdfs:")
                    && !type_iri.starts_with("rdf:")
            })
            .map(|type_iri| {
                let type_thing = Thing::get(conn, type_iri);
                serde_json::json!({
                    "iri": type_iri,
                    "label": type_thing.label,
                    "icon": type_thing.icon,
                })
            });

        let status = get_entity_status_info(conn, iri)
            .map(|(s_iri, s_label, s_color, s_icon)| serde_json::json!({
                "iri": s_iri,
                "label": s_label,
                "icon": s_icon,
                "color": s_color,
            }));

        scored.push((score, RichSearchResult {
            id: iri.clone(),
            label,
            icon,
            entity_type: "individual".to_string(),
            matched_properties,
            concept_type,
            status,
        }));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.len().cmp(&b.1.label.len())));

    Ok(scored.into_iter().take(limit).map(|(_, r)| r).collect())
}

/// Returns all IRI values for a predicate on an entity
pub fn get_all_iri_properties(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<Vec<String>> {
    use crate::eavto::query;
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
    use crate::eavto::{store, query, Triple, Object};
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

/// Validates that `status_iri` is in the `foundation:allowedStatus` list of `concept_iri`.
/// If the concept has no `allowedStatus` triples, any status is accepted.
pub fn validate_allowed_status(
    conn: &Connection,
    concept_iri: &str,
    status_iri: &str,
) -> Result<()> {
    use crate::eavto::query;
    let result = query::get_by_entity_predicate(conn, concept_iri, "foundation:allowedStatus")?;
    if result.triples.is_empty() {
        return Ok(());
    }
    let allowed: Vec<&str> = result.triples.iter()
        .filter_map(|t| t.object.as_iri())
        .collect();
    if !allowed.contains(&status_iri) {
        let allowed_list = allowed.join(", ");
        return Err(OwlError::ValidationError(format!(
            "Status '{}' is not allowed for concept '{}'. Allowed statuses: {}",
            status_iri, concept_iri, allowed_list
        )));
    }
    Ok(())
}

/// Resolves icon and color for a status IRI, following `foundation:parentStatus` recursively
/// when either is absent on the status itself.
pub fn resolve_status_appearance(
    conn: &Connection,
    status_iri: &str,
) -> (Option<String>, Option<String>) {
    let mut current = status_iri.to_string();
    let mut icon: Option<String> = None;
    let mut color: Option<String> = None;

    loop {
        if icon.is_none() {
            icon = get_iri_property(conn, &current, "foundation:hasIcon")
                .ok()
                .flatten()
                .and_then(|iri| icon_iri_to_display(conn, &iri))
                .or_else(|| get_literal_property(conn, &current, "foundation:icon").ok().flatten());
        }
        if color.is_none() {
            color = get_literal_property(conn, &current, "foundation:color").ok().flatten();
        }

        if icon.is_some() && color.is_some() {
            break;
        }

        match get_iri_property(conn, &current, "foundation:parentStatus").ok().flatten() {
            Some(parent) if parent != current => current = parent,
            _ => break,
        }
    }

    (icon, color)
}

/// Finds the first property value of the entity that is an instance of `foundation:Status`.
/// Returns `(iri, label, color, icon)` if a status is found.
/// Color and icon are resolved recursively via `foundation:parentStatus` if absent.
pub fn get_entity_status_info(
    conn: &Connection,
    entity_iri: &str,
) -> Option<(String, String, Option<String>, Option<String>)> {
    use crate::eavto::query;
    let result = query::get_by_entity(conn, entity_iri).ok()?;
    for triple in &result.triples {
        if let Some(iri) = triple.object.as_iri() {
            if is_instance_of(conn, iri, "foundation:Status") {
                let thing = Thing::get(conn, iri);
                let (icon, color) = resolve_status_appearance(conn, iri);
                return Some((iri.to_string(), thing.label, color, icon));
            }
        }
    }
    None
}

/// Returns `(class_group, individual_group, literal_group)` from the ontology.
/// Falls back to the compile-time defaults `(1, 6, 7)` if the ontology data is missing.
pub fn load_graph_node_groups(conn: &Connection) -> (u8, u8, u8) {
    let configs = get_graph_node_type_config(conn);
    let group_for = |label: &str| -> Option<u8> {
        configs.iter().find(|c| c.label == label).map(|c| c.group)
    };
    (
        group_for("Class Node").unwrap_or(1),
        group_for("Individual Node").unwrap_or(6),
        group_for("Literal Node").unwrap_or(7),
    )
}

/// Returns all `foundation:GraphNodeType` individuals with their configuration as a serializable structure.
pub fn get_graph_node_type_config(conn: &Connection) -> Vec<GraphNodeTypeConfig> {
    use crate::eavto::query;

    let Ok(types_result) = query::get_by_predicate_object(conn, vocabulary::rdf::TYPE, "foundation:GraphNodeType") else {
        return vec![];
    };

    let mut configs = Vec::new();
    for triple in &types_result.triples {
        let iri = &triple.subject;

        let label = get_literal_property(conn, iri, vocabulary::rdfs::LABEL)
            .ok()
            .flatten()
            .unwrap_or_default();

        let group_str = get_literal_property(conn, iri, "foundation:graphGroup")
            .ok()
            .flatten()
            .unwrap_or_default();

        let Ok(group) = group_str.parse::<u8>() else {
            continue;
        };

        configs.push(GraphNodeTypeConfig {
            iri: iri.clone(),
            label,
            group,
        });
    }

    configs.sort_by_key(|c| c.group);
    configs
}

#[derive(Debug, serde::Serialize)]
pub struct GraphNodeTypeConfig {
    pub iri: String,
    pub label: String,
    pub group: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::eavto::{store, Triple, Object};

    #[test]
    fn test_replace_all_property_iris_saves_all_values() {
        let mut conn = setup_test_db();

        // Create a subject entity
        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "foundation:TestConcept",
                "rdf:type",
                Object::Iri("owl:Class".to_string()),
            )],
            "test",
        ).unwrap();

        // Create target IRI entities so they exist
        for iri in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
            store::assert_triples(
                &mut conn,
                &[Triple::new(*iri, "rdf:type", Object::Iri("foundation:Status".to_string()))],
                "test",
            ).unwrap();
        }

        // Replace with three values at once
        replace_all_property_iris(
            &mut conn,
            "foundation:TestConcept",
            "foundation:allowedStatus",
            &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"],
            "test",
        ).unwrap();

        // All three must be active
        let active: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples \
             WHERE subject = 'foundation:TestConcept' \
               AND predicate = 'foundation:allowedStatus' \
               AND retracted = 0",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(active, 3, "All three allowedStatus values must be stored");

        // Verify the specific IRIs are present
        for status in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM triples \
                 WHERE subject = 'foundation:TestConcept' \
                   AND predicate = 'foundation:allowedStatus' \
                   AND object = ? AND retracted = 0",
                [status],
                |row| row.get(0),
            ).unwrap();
            assert!(exists, "{status} must be stored as allowedStatus");
        }
    }

    #[test]
    fn test_replace_all_property_iris_replaces_existing_values() {
        let mut conn = setup_test_db();

        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "foundation:TestConcept",
                "rdf:type",
                Object::Iri("owl:Class".to_string()),
            )],
            "test",
        ).unwrap();

        for iri in &["foundation:StatusA", "foundation:StatusB", "foundation:StatusC"] {
            store::assert_triples(
                &mut conn,
                &[Triple::new(*iri, "rdf:type", Object::Iri("foundation:Status".to_string()))],
                "test",
            ).unwrap();
        }

        // Set initial values
        replace_all_property_iris(
            &mut conn,
            "foundation:TestConcept",
            "foundation:allowedStatus",
            &["foundation:StatusA", "foundation:StatusB"],
            "test",
        ).unwrap();

        // Replace with a different set
        replace_all_property_iris(
            &mut conn,
            "foundation:TestConcept",
            "foundation:allowedStatus",
            &["foundation:StatusB", "foundation:StatusC"],
            "test",
        ).unwrap();

        let active: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples \
             WHERE subject = 'foundation:TestConcept' \
               AND predicate = 'foundation:allowedStatus' \
               AND retracted = 0",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(active, 2, "Only the new set of values must remain");

        let status_a_active: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM triples \
             WHERE subject = 'foundation:TestConcept' \
               AND predicate = 'foundation:allowedStatus' \
               AND object = 'foundation:StatusA' AND retracted = 0",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(!status_a_active, "StatusA must be retracted after replacement");
    }
}
