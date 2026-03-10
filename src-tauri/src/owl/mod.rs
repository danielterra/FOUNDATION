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

/// Search classes and individuals by IRI, label, comment, and literal properties.
/// Results are ranked by relevance and enriched with concept type, icon, and status.
pub fn search_instances_rich(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<RichSearchResult>> {
    use crate::eavto::query;

    let rows = query::search_entities(conn, query, limit)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let mut results = Vec::with_capacity(rows.len());

    for row in rows {
        let icon = row.has_icon_iri
            .as_deref()
            .and_then(|iri| icon_iri_to_display(conn, iri))
            .or(row.icon_literal);

        let is_class = row.type_iri.as_deref() == Some("owl:Class");
        let entity_type = if is_class { "class" } else { "individual" }.to_string();

        let concept_type = if is_class {
            None
        } else {
            row.type_iri.as_deref().and_then(|type_iri| {
                if type_iri.starts_with("owl:") || type_iri.starts_with("rdf:") || type_iri.starts_with("rdfs:") {
                    None
                } else {
                    let type_thing = Thing::get(conn, type_iri);
                    Some(serde_json::json!({
                        "iri": type_iri,
                        "label": type_thing.label,
                        "icon": type_thing.icon,
                    }))
                }
            })
        };

        let matched_properties: Vec<serde_json::Value> = row.props_raw
            .as_deref()
            .map(|raw| {
                raw.split('\x1E')
                    .filter_map(|entry| {
                        let mut parts = entry.splitn(2, '\x1F');
                        let pred = parts.next()?;
                        let val = parts.next()?;
                        Some(serde_json::json!({ "detail_iri": pred, "value": val }))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let status = get_entity_status_info(conn, &row.subject)
            .map(|(s_iri, s_label, s_color, s_icon)| serde_json::json!({
                "iri": s_iri,
                "label": s_label,
                "icon": s_icon,
                "color": s_color,
            }));

        results.push(RichSearchResult {
            id: row.subject,
            label: row.label,
            icon,
            entity_type,
            matched_properties,
            concept_type,
            status,
        });
    }

    Ok(results)
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

/// Unified search across classes and individuals.
///
/// Path A (concept_iri or filters provided): loads candidates for that class, optionally
/// applies multi-token AND scoring in Rust, then paginates and enriches.
///
/// Path B (global): uses SQL-based `search_entities` / `search_entities_scores_only` to
/// find candidates, intersects per-token score maps for multi-token AND, then enriches the
/// result page.
pub fn search_rich(
    conn: &Connection,
    tokens: &[String],
    entity_type_filter: Option<&str>,
    concept_iri: Option<&str>,
    filters: Option<&[(String, String, String)]>,
    include_retracted: bool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<RichSearchResult>, usize)> {
    if concept_iri.is_some() || filters.is_some() {
        search_rich_structured(conn, tokens, entity_type_filter, concept_iri, filters, include_retracted, limit, offset)
    } else {
        search_rich_global(conn, tokens, entity_type_filter, limit, offset)
    }
}

fn score_entity_against_tokens(
    iri: &str,
    triples: &[crate::eavto::Triple],
    tokens: &[String],
    matched_properties: &mut Vec<serde_json::Value>,
) -> Option<i32> {
    let label = triples.iter()
        .find(|t| t.predicate == "rdfs:label")
        .and_then(|t| t.object.as_literal())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    let local_part = iri.split(':').last().unwrap_or("").to_lowercase();

    let mut total_score: i32 = 0;

    for token in tokens {
        let token_lower = token.to_lowercase();
        let mut token_score: i32 = 0;
        let mut matched_prop: Option<serde_json::Value> = None;

        if iri.to_lowercase() == token_lower || local_part == token_lower {
            token_score = 100;
        } else if label == token_lower {
            token_score = 50;
        } else if label.starts_with(&token_lower) {
            token_score = 40;
        } else if label.contains(&token_lower) {
            token_score = 30;
        } else {
            let comment_match = triples.iter()
                .find(|t| t.predicate == "rdfs:comment" && t.object.as_literal().map(|v| v.to_lowercase().contains(&token_lower)).unwrap_or(false));
            if let Some(ct) = comment_match {
                token_score = 20;
                matched_prop = Some(serde_json::json!({
                    "detail_iri": "rdfs:comment",
                    "value": ct.object.as_literal().unwrap_or_default(),
                }));
            } else {
                let prop_match = triples.iter().find(|t| {
                    t.predicate != "rdfs:label"
                        && t.predicate != "rdfs:comment"
                        && t.predicate != "foundation:icon"
                        && t.predicate != "foundation:hasIcon"
                        && t.object.as_literal().map(|v| v.to_lowercase().contains(&token_lower)).unwrap_or(false)
                });
                if let Some(pm) = prop_match {
                    token_score = 10;
                    matched_prop = Some(serde_json::json!({
                        "detail_iri": pm.predicate,
                        "value": pm.object.as_literal().unwrap_or_default(),
                    }));
                }
            }
        }

        if token_score == 0 {
            return None;
        }
        total_score += token_score;
        if let Some(mp) = matched_prop {
            matched_properties.push(mp);
        }
    }

    Some(total_score)
}

fn enrich_from_triples(
    conn: &Connection,
    iri: &str,
    triples: &[crate::eavto::Triple],
    matched_properties: Vec<serde_json::Value>,
) -> RichSearchResult {
    let label = triples.iter()
        .find(|t| t.predicate == "rdfs:label")
        .and_then(|t| t.object.as_literal())
        .map(|s| s.to_string())
        .unwrap_or_else(|| iri.to_string());

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

    let type_iri = triples.iter()
        .filter(|t| t.predicate == "rdf:type")
        .filter_map(|t| t.object.as_iri())
        .find(|iri| !iri.starts_with("owl:") && !iri.starts_with("rdf:") && !iri.starts_with("rdfs:"))
        .or_else(|| {
            triples.iter()
                .find(|t| t.predicate == "rdf:type")
                .and_then(|t| t.object.as_iri())
        })
        .map(|s| s.to_string());

    let is_class = type_iri.as_deref() == Some("owl:Class");
    let entity_type = if is_class { "class" } else { "individual" }.to_string();

    let concept_type = if is_class {
        None
    } else {
        type_iri.as_deref().and_then(|t| {
            if t.starts_with("owl:") || t.starts_with("rdf:") || t.starts_with("rdfs:") {
                None
            } else {
                let type_thing = Thing::get(conn, t);
                Some(serde_json::json!({
                    "iri": t,
                    "label": type_thing.label,
                    "icon": type_thing.icon,
                }))
            }
        })
    };

    let status = get_entity_status_info(conn, iri)
        .map(|(s_iri, s_label, s_color, s_icon)| serde_json::json!({
            "iri": s_iri,
            "label": s_label,
            "icon": s_icon,
            "color": s_color,
        }));

    RichSearchResult {
        id: iri.to_string(),
        label,
        icon,
        entity_type,
        matched_properties,
        concept_type,
        status,
    }
}

fn enrich_from_sql_row(
    conn: &Connection,
    row: &crate::eavto::query::EntitySearchRow,
) -> RichSearchResult {
    let icon = row.has_icon_iri
        .as_deref()
        .and_then(|iri| icon_iri_to_display(conn, iri))
        .or_else(|| row.icon_literal.clone());

    let is_class = row.type_iri.as_deref() == Some("owl:Class");
    let entity_type = if is_class { "class" } else { "individual" }.to_string();

    let concept_type = if is_class {
        None
    } else {
        row.type_iri.as_deref().and_then(|t| {
            if t.starts_with("owl:") || t.starts_with("rdf:") || t.starts_with("rdfs:") {
                None
            } else {
                let type_thing = Thing::get(conn, t);
                Some(serde_json::json!({
                    "iri": t,
                    "label": type_thing.label,
                    "icon": type_thing.icon,
                }))
            }
        })
    };

    let matched_properties: Vec<serde_json::Value> = row.props_raw
        .as_deref()
        .map(|raw| {
            raw.split('\x1E')
                .filter_map(|entry| {
                    let mut parts = entry.splitn(2, '\x1F');
                    let pred = parts.next()?;
                    let val = parts.next()?;
                    Some(serde_json::json!({ "detail_iri": pred, "value": val }))
                })
                .collect()
        })
        .unwrap_or_default();

    let status = get_entity_status_info(conn, &row.subject)
        .map(|(s_iri, s_label, s_color, s_icon)| serde_json::json!({
            "iri": s_iri,
            "label": s_label,
            "icon": s_icon,
            "color": s_color,
        }));

    RichSearchResult {
        id: row.subject.clone(),
        label: row.label.clone(),
        icon,
        entity_type,
        matched_properties,
        concept_type,
        status,
    }
}

fn search_rich_structured(
    conn: &Connection,
    tokens: &[String],
    _entity_type_filter: Option<&str>,
    concept_iri: Option<&str>,
    filters: Option<&[(String, String, String)]>,
    include_retracted: bool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<RichSearchResult>, usize)> {
    use crate::eavto::query;

    let candidate_iris: Vec<String> = if let Some(f) = filters {
        let concept = concept_iri.ok_or_else(|| OwlError::ValidationError(
            "concept_iri is required when filters are provided".to_string()
        ))?;
        let constraint_refs: Vec<(&str, &str, &str)> = f.iter()
            .map(|(d, v, o)| (d.as_str(), v.as_str(), o.as_str()))
            .collect();
        let (iris, _) = Individual::find_by_class_and_properties_with_options(
            conn, concept, &constraint_refs, include_retracted, usize::MAX, 0,
        )?;
        iris
    } else if let Some(concept) = concept_iri {
        if include_retracted {
            Individual::find_by_class_with_date_range(conn, concept, None, None, true)?
        } else {
            Class::get_instances(conn, concept)?
        }
    } else {
        return Err(OwlError::InvalidOperation("structured search requires concept_iri or filters".to_string()));
    };

    let load_batch = |subjects: &[String]| -> Result<std::collections::HashMap<String, Vec<crate::eavto::Triple>>> {
        let active = query::batch_load_triples_for_subjects(conn, subjects)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
        if !include_retracted {
            return Ok(active);
        }
        let missing: Vec<String> = subjects.iter()
            .filter(|s| !active.contains_key(s.as_str()))
            .cloned()
            .collect();
        if missing.is_empty() {
            return Ok(active);
        }
        let retracted = query::batch_load_retracted_triples_for_subjects(conn, &missing)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
        let mut combined = active;
        combined.extend(retracted);
        Ok(combined)
    };

    if tokens.is_empty() {
        let total = candidate_iris.len();
        let page: Vec<String> = candidate_iris.into_iter().skip(offset).take(limit).collect();
        let batch = load_batch(&page)?;
        let results: Vec<RichSearchResult> = page.iter().map(|iri| {
            let empty = vec![];
            let triples = batch.get(iri.as_str()).unwrap_or(&empty);
            enrich_from_triples(conn, iri, triples, vec![])
        }).collect();
        return Ok((results, total));
    }

    let batch = load_batch(&candidate_iris)?;

    let mut scored: Vec<(String, i32)> = Vec::new();
    for iri in &candidate_iris {
        let empty = vec![];
        let triples = batch.get(iri.as_str()).unwrap_or(&empty);
        let mut matched_props = vec![];
        if let Some(score) = score_entity_against_tokens(iri, triples, tokens, &mut matched_props) {
            scored.push((iri.clone(), score));
        }
    }
    scored.sort_by(|a, b| b.1.cmp(&a.1));

    let total = scored.len();
    let page: Vec<String> = scored.into_iter().skip(offset).take(limit).map(|(iri, _)| iri).collect();

    let page_batch = load_batch(&page)?;

    let results: Vec<RichSearchResult> = page.iter().map(|iri| {
        let empty = vec![];
        let triples = page_batch.get(iri.as_str()).unwrap_or(&empty);
        let mut matched_props = vec![];
        score_entity_against_tokens(iri, triples, tokens, &mut matched_props);
        enrich_from_triples(conn, iri, triples, matched_props)
    }).collect();

    Ok((results, total))
}

fn search_rich_global(
    conn: &Connection,
    tokens: &[String],
    entity_type_filter: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<RichSearchResult>, usize)> {
    use crate::eavto::query;

    if tokens.is_empty() {
        let big_limit = offset + limit + 1000;
        let rows = query::search_entities(conn, "", big_limit)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

        let filtered: Vec<_> = rows.into_iter()
            .filter(|r| {
                if let Some(f) = entity_type_filter {
                    let is_class = r.type_iri.as_deref() == Some("owl:Class");
                    let et = if is_class { "class" } else { "individual" };
                    et == f
                } else {
                    true
                }
            })
            .collect();

        let total = filtered.len();
        let page: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();
        let results = page.iter().map(|r| enrich_from_sql_row(conn, r)).collect();
        return Ok((results, total));
    }

    if tokens.len() == 1 {
        let big_limit = offset + limit + 10000;
        let rows = query::search_entities(conn, &tokens[0], big_limit)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

        let filtered: Vec<_> = rows.into_iter()
            .filter(|r| {
                if let Some(f) = entity_type_filter {
                    let is_class = r.type_iri.as_deref() == Some("owl:Class");
                    let et = if is_class { "class" } else { "individual" };
                    et == f
                } else {
                    true
                }
            })
            .collect();

        let total = filtered.len();
        let page: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();
        let results = page.iter().map(|r| enrich_from_sql_row(conn, r)).collect();
        return Ok((results, total));
    }

    let score_maps: Vec<std::collections::HashMap<String, i32>> = tokens.iter()
        .map(|token| {
            query::search_entities_scores_only(conn, token)
                .map_err(|e| OwlError::DatabaseError(e.to_string()))
        })
        .collect::<Result<Vec<_>>>()?;

    let first_map = &score_maps[0];
    let mut combined: Vec<(String, i32)> = first_map.iter()
        .filter_map(|(subject, &score0)| {
            let mut total_score = score0;
            for map in &score_maps[1..] {
                match map.get(subject) {
                    Some(&s) => total_score += s,
                    None => return None,
                }
            }
            Some((subject.clone(), total_score))
        })
        .collect();

    if let Some(f) = entity_type_filter {
        let f_owned = f.to_string();
        let page_subjects: Vec<String> = combined.iter().map(|(s, _)| s.clone()).collect();
        if !page_subjects.is_empty() {
            let batch = query::batch_load_triples_for_subjects(conn, &page_subjects)
                .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
            combined.retain(|(subject, _)| {
                let empty = vec![];
                let triples = batch.get(subject.as_str()).unwrap_or(&empty);
                let is_class = triples.iter()
                    .any(|t| t.predicate == "rdf:type" && t.object.as_iri() == Some("owl:Class"));
                let et = if is_class { "class" } else { "individual" };
                et == f_owned.as_str()
            });
        }
    }

    combined.sort_by(|a, b| b.1.cmp(&a.1));
    let total = combined.len();
    let page_subjects: Vec<String> = combined.into_iter().skip(offset).take(limit).map(|(s, _)| s).collect();

    let batch = query::batch_load_triples_for_subjects(conn, &page_subjects)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let results: Vec<RichSearchResult> = page_subjects.iter().map(|iri| {
        let empty = vec![];
        let triples = batch.get(iri.as_str()).unwrap_or(&empty);
        let mut matched_props = vec![];
        score_entity_against_tokens(iri, triples, tokens, &mut matched_props);
        enrich_from_triples(conn, iri, triples, matched_props)
    }).collect();

    Ok((results, total))
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
#[path = "mod_tests.rs"]
mod tests;
