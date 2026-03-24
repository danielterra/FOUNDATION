mod class;
mod property;
mod individual;
mod thing;
mod icons;
mod graph_config;
pub mod vocabulary;
pub mod cardinality;
pub mod formula;
pub mod formula_worker;

pub use graph_config::{load_graph_node_groups, get_graph_node_type_config, GraphNodeTypeConfig};

pub use icons::{validate_icon, icon_name_to_iri, icon_iri_to_display, icon_store_value, seed_icon_library};

pub use class::{Class, ClassType};
pub use property::{Property, PropertyType, DomainLabel};
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

/// Returns `true` if the entity has `foundation:isSystemLocked = true`.
pub fn is_system_locked(conn: &Connection, iri: &str) -> bool {
    use crate::eavto::query;
    query::get_by_entity_predicate(conn, iri, "foundation:isSystemLocked")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| if let Object::Boolean(b) = t.object { Some(b) } else { None })
        .unwrap_or(false)
}

/// Sets `foundation:isSystemLocked` on any entity, bypassing the lock guard.
/// This is the only write operation intentionally exempt from lock enforcement.
pub fn set_system_locked(conn: &mut Connection, iri: &str, locked: bool) -> Result<()> {
    let triple = crate::eavto::Triple::new(iri, "foundation:isSystemLocked", Object::Boolean(locked));
    crate::eavto::store::assert_triples(conn, &[triple], "user")
        .map_err(|e| OwlError::InvalidOperation(e.to_string()))?;
    Ok(())
}

/// Returns `Err` if the entity at `iri` has `foundation:isSystemLocked = true`.
/// Pass `Some("foundation:isSystemLocked")` as `exempt_property` to allow writing
/// the lock flag itself (prevents deadlock when bootstrapping locked entities).
pub fn check_system_locked(
    conn: &Connection,
    iri: &str,
    exempt_property: Option<&str>,
) -> Result<()> {
    if exempt_property == Some("foundation:isSystemLocked") {
        return Ok(());
    }
    use crate::eavto::query;
    let result = query::get_by_entity_predicate(conn, iri, "foundation:isSystemLocked")?;
    let is_locked = result.triples.first()
        .and_then(|t| if let Object::Boolean(b) = &t.object { Some(*b) } else { None })
        .unwrap_or(false);
    if is_locked {
        return Err(OwlError::InvalidOperation(format!(
            "Entity '{}' is system-locked and cannot be modified",
            iri
        )));
    }
    Ok(())
}

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
                        has_icon_result.triples.first().and_then(|t| match &t.object {
                            Object::Iri(iri) => icon_iri_to_display(conn, iri),
                            Object::Literal { value, .. } => Some(value.clone()),
                            _ => None,
                        })
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
/// If `query` is a single `prefix:localname` token, load that entity directly from the DB.
/// Returns `None` if the pattern doesn't match or the entity doesn't exist.
pub fn try_iri_direct_lookup(conn: &Connection, query: &str) -> Option<RichSearchResult> {
    let trimmed = query.trim();
    if trimmed.contains(' ') {
        return None;
    }
    let colon = trimmed.find(':')?;
    if colon == 0 || colon == trimmed.len() - 1 {
        return None;
    }
    let iris = vec![trimmed.to_string()];
    let batch = crate::eavto::query::batch_load_triples_for_subjects(conn, &iris).ok()?;
    let triples = batch.get(trimmed)?;
    if triples.is_empty() {
        return None;
    }
    Some(enrich_from_triples(conn, trimmed, triples, vec![]))
}

pub fn search_instances_rich(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<RichSearchResult>> {
    let iri_hit = try_iri_direct_lookup(conn, query);

    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .collect();

    let (mut results, _total) = search_rich(conn, &tokens, None, None, None, false, limit, 0)?;

    if let Some(hit) = iri_hit {
        results.retain(|r| r.id != hit.id);
        results.insert(0, hit);
        results.truncate(limit);
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
/// Returns an error if the concept has no configured statuses, or if the status is not allowed.
pub fn validate_allowed_status(
    conn: &Connection,
    concept_iri: &str,
    status_iri: &str,
) -> Result<()> {
    use crate::eavto::query;
    let result = query::get_by_entity_predicate(conn, concept_iri, "foundation:allowedStatus")?;
    if result.triples.is_empty() {
        let concept_label = get_literal_property(conn, concept_iri, "rdfs:label")?
            .unwrap_or_else(|| concept_iri.to_string());
        return Err(OwlError::ValidationError(format!(
            "Concept '{}' has no statuses configured. Every concept must have at least one allowed status. Use learn_concepts to add allowedStatuses to '{}'.",
            concept_label, concept_iri
        )));
    }
    let allowed_iris: Vec<String> = result.triples.iter()
        .filter_map(|t| t.object.as_iri())
        .map(|s| s.to_string())
        .collect();
    if !allowed_iris.iter().any(|s| s == status_iri) {
        let allowed_labels: Vec<String> = allowed_iris.iter()
            .map(|iri| {
                get_literal_property(conn, iri, "rdfs:label")
                    .ok()
                    .flatten()
                    .map(|label| format!("{} ({})", label, iri))
                    .unwrap_or_else(|| iri.clone())
            })
            .collect();
        let concept_label = get_literal_property(conn, concept_iri, "rdfs:label")?
            .unwrap_or_else(|| concept_iri.to_string());
        return Err(OwlError::ValidationError(format!(
            "Status '{}' is not allowed for concept '{}'. Accepted statuses: {}",
            status_iri, concept_label, allowed_labels.join(", ")
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
            icon = {
                use crate::eavto::query;
                query::get_by_entity_predicate(conn, &current, "foundation:hasIcon")
                    .ok()
                    .and_then(|r| {
                        r.triples.first().and_then(|t| match &t.object {
                            crate::eavto::Object::Iri(icon_iri) => icon_iri_to_display(conn, icon_iri),
                            crate::eavto::Object::Literal { value, .. } => Some(value.clone()),
                            _ => None,
                        })
                    })
            };
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
    if filters.is_some() || include_retracted {
        search_rich_structured(conn, tokens, entity_type_filter, concept_iri, filters, include_retracted, limit, offset)
    } else {
        search_rich_global(conn, tokens, entity_type_filter, concept_iri, limit, offset)
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
            if comment_match.is_some() {
                token_score = 20;
                matched_prop = Some(serde_json::json!({ "detail_iri": "rdfs:comment" }));
            } else {
                let prop_match = triples.iter().find(|t| {
                    t.predicate != "rdfs:label"
                        && t.predicate != "rdfs:comment"
                        && t.predicate != "foundation:hasIcon"
                        && t.object.as_literal().map(|v| v.to_lowercase().contains(&token_lower)).unwrap_or(false)
                });
                if let Some(pm) = prop_match {
                    token_score = 10;
                    matched_prop = Some(serde_json::json!({ "detail_iri": pm.predicate }));
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
        .and_then(|t| match &t.object {
            Object::Iri(iri) => icon_iri_to_display(conn, iri),
            Object::Literal { value, .. } => Some(value.clone()),
            _ => None,
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
                    let pred = entry.splitn(2, '\x1F').next()?;
                    Some(serde_json::json!({ "detail_iri": pred }))
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
        let constraint_refs: Vec<(&str, &str, &str)> = f.iter()
            .map(|(d, v, o)| (d.as_str(), v.as_str(), o.as_str()))
            .collect();
        if let Some(concept) = concept_iri {
            let (iris, _) = Individual::find_by_class_and_properties_with_options(
                conn, concept, &constraint_refs, include_retracted, usize::MAX, 0,
            )?;
            iris
        } else {
            let (iris, _) = query::find_by_properties_with_options(
                conn, &constraint_refs, include_retracted, usize::MAX, 0,
            ).map_err(|e| OwlError::DatabaseError(e.to_string()))?;
            iris
        }
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
    concept_iri: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<RichSearchResult>, usize)> {
    use crate::eavto::query;

    if tokens.is_empty() {
        if let Some(concept) = concept_iri {
            let all_iris = Class::get_instances(conn, concept)?;
            let total = all_iris.len();
            let page: Vec<String> = all_iris.into_iter().skip(offset).take(limit).collect();
            let batch = query::batch_load_triples_for_subjects(conn, &page)
                .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
            let results: Vec<RichSearchResult> = page.iter().map(|iri| {
                let empty = vec![];
                let triples = batch.get(iri.as_str()).unwrap_or(&empty);
                enrich_from_triples(conn, iri, triples, vec![])
            }).collect();
            return Ok((results, total));
        }

        let big_limit = offset + limit + 1000;
        let rows = query::search_entities(conn, "", big_limit)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
        let filtered: Vec<_> = rows.into_iter()
            .filter(|r| entity_type_matches(r.type_iri.as_deref(), entity_type_filter))
            .collect();
        let total = filtered.len();
        let page: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();
        let results = page.iter().map(|r| enrich_from_sql_row(conn, r)).collect();
        return Ok((results, total));
    }

    // In tests, skip Tantivy (shared global index not populated with test data).
    #[cfg(test)]
    return search_rich_sql_fallback(conn, tokens, entity_type_filter, concept_iri, limit, offset);

    // Non-empty query: Tantivy BM25 + usage boost + optional concept filter.
    #[cfg(not(test))]
    {
        let query_str = tokens.join(" ");
        const TANTIVY_FETCH_MULTIPLIER: usize = 20;
        let fetch_limit = (offset + limit + 1) * TANTIVY_FETCH_MULTIPLIER;
        let iris = crate::search::search(&query_str, concept_iri, fetch_limit);

        if iris.is_empty() {
            return Ok((vec![], 0));
        }

        let batch = query::batch_load_triples_for_subjects(conn, &iris)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

        let filtered: Vec<&String> = iris.iter()
            .filter(|iri| {
                if entity_type_filter.is_none() {
                    return true;
                }
                let empty = vec![];
                let triples = batch.get(iri.as_str()).unwrap_or(&empty);
                let type_iri = triples.iter()
                    .find(|t| t.predicate == "rdf:type")
                    .and_then(|t| t.object.as_iri());
                entity_type_matches(type_iri, entity_type_filter)
            })
            .collect();

        let total = filtered.len();
        let page: Vec<&String> = filtered.into_iter().skip(offset).take(limit).collect();

        let results: Vec<RichSearchResult> = page.iter().map(|iri| {
            let empty = vec![];
            let triples = batch.get(iri.as_str()).unwrap_or(&empty);
            let matched_props = matched_properties_for_tokens(iri, triples, tokens);
            enrich_from_triples(conn, iri, triples, matched_props)
        }).collect();

        Ok((results, total))
    }
}

/// Finds which properties in the triples contain at least one of the query tokens.
/// Used to populate matchedProperties in Tantivy search results.
fn matched_properties_for_tokens(
    iri: &str,
    triples: &[crate::eavto::Triple],
    tokens: &[String],
) -> Vec<serde_json::Value> {
    let local_part = iri.split(':').last().unwrap_or("").to_lowercase();
    let label = triples.iter()
        .find(|t| t.predicate == "rdfs:label")
        .and_then(|t| t.object.as_literal())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    let mut matched: Vec<serde_json::Value> = Vec::new();

    for token in tokens {
        let tok = token.to_lowercase();

        // IRI or label match: no specific property to report
        if iri.to_lowercase().contains(&tok) || local_part.contains(&tok) || label.contains(&tok) {
            continue;
        }

        // Find which non-label property matches this token
        let prop_match = triples.iter().find(|t| {
            t.predicate != "rdfs:label"
                && t.predicate != "foundation:hasIcon"
                && t.object.as_literal()
                    .map(|v| v.to_lowercase().contains(&tok))
                    .unwrap_or(false)
        });

        if let Some(pm) = prop_match {
            let entry = serde_json::json!({ "detail_iri": pm.predicate });
            if !matched.iter().any(|e| e == &entry) {
                matched.push(entry);
            }
        }
    }

    matched
}

#[cfg(test)]
const SQL_FALLBACK_SCAN_LIMIT: usize = 5000;

#[cfg(test)]
fn search_rich_sql_fallback(
    conn: &Connection,
    tokens: &[String],
    entity_type_filter: Option<&str>,
    concept_iri: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<RichSearchResult>, usize)> {
    use crate::eavto::query;
    let first_token = match tokens.first() {
        Some(t) => t.as_str(),
        None => return Ok((vec![], 0)),
    };
    let rows = query::search_entities(conn, first_token, offset + limit + SQL_FALLBACK_SCAN_LIMIT)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let candidate_iris: Vec<String> = rows.iter()
        .filter(|r| entity_type_matches(r.type_iri.as_deref(), entity_type_filter))
        .filter(|r| concept_iri.map_or(true, |c| r.type_iri.as_deref() == Some(c)))
        .map(|r| r.subject.clone())
        .collect();

    let batch = query::batch_load_triples_for_subjects(conn, &candidate_iris)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

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

    let page_batch = query::batch_load_triples_for_subjects(conn, &page)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let results: Vec<RichSearchResult> = page.iter().map(|iri| {
        let empty = vec![];
        let triples = page_batch.get(iri.as_str()).unwrap_or(&empty);
        let mut matched_props = vec![];
        score_entity_against_tokens(iri, triples, tokens, &mut matched_props);
        enrich_from_triples(conn, iri, triples, matched_props)
    }).collect();

    Ok((results, total))
}

fn entity_type_matches(type_iri: Option<&str>, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(f) => {
            let is_class = type_iri == Some("owl:Class");
            let et = if is_class { "class" } else { "individual" };
            et == f
        }
    }
}


#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
