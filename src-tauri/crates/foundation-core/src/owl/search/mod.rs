mod engine;
mod scoring;

use crate::owl::{
    Connection, Object, Result, Thing,
    icon_iri_to_display,
    vocabulary,
};
use engine::{enrich_from_triples, search_structured, search_global};

/// Rich search result for instances, including matched properties, concept type and status.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub matched_properties: Vec<serde_json::Value>,
    pub class_type: Option<serde_json::Value>,
    pub status: Option<serde_json::Value>,
}

/// Search result for classes and individuals
#[derive(Debug, Clone)]
pub struct ClassSearchResult {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub is_class: bool,
}

/// Search classes and individuals by IRI, label, comment, and literal properties.
/// Results are ranked by relevance and enriched with concept type, icon, and status.
/// If `query` is a single `prefix:localname` token, load that entity directly from the DB.
/// Returns `None` if the pattern doesn't match or the entity doesn't exist.
pub fn try_iri_direct_lookup(conn: &Connection, query: &str) -> Option<SearchResult> {
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

pub fn search_instances(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let iri_hit = try_iri_direct_lookup(conn, query);

    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .collect();

    let (mut results, _total) = search(conn, &tokens, None, None, None, false, limit, 0)?;

    if let Some(hit) = iri_hit {
        results.retain(|r| r.id != hit.id);
        results.insert(0, hit);
        results.truncate(limit);
    }

    Ok(results)
}

/// Search for classes by label (case-insensitive, ranked by relevance)
pub fn search_classes(conn: &Connection, query: &str, limit: usize) -> Result<Vec<ClassSearchResult>> {
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

            results.push((score, ClassSearchResult {
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
) -> Result<Vec<ClassSearchResult>> {
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
                            Object::Literal { value, .. } =>
                                Some(crate::owl::icon_literal_to_display(value)),
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

                    results.push((score, ClassSearchResult {
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

/// Unified search across classes and individuals.
///
/// Path A (class_iri or filters provided): loads candidates for that class, optionally
/// applies multi-token AND scoring in Rust, then paginates and enriches.
///
/// Path B (global): uses Tantivy BM25 full-text search to find and rank candidates,
/// then enriches the result page with triple data.
pub fn search(
    conn: &Connection,
    tokens: &[String],
    entity_type_filter: Option<&str>,
    class_iri: Option<&str>,
    filters: Option<&[(String, String, String)]>,
    include_retracted: bool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<SearchResult>, usize)> {
    if filters.is_some() || include_retracted || class_iri.is_some() {
        search_structured(conn, tokens, entity_type_filter, class_iri, filters, include_retracted, limit, offset)
    } else {
        search_global(conn, tokens, entity_type_filter, class_iri, limit, offset)
    }
}
