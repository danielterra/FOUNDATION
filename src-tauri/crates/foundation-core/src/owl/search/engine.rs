use crate::owl::{
    Connection, Object, OwlError, Result, Thing,
    icon_iri_to_display, get_entity_status_info,
    Class, Individual,
};
use super::SearchResult;
use super::scoring::{score_entity_against_tokens, matched_properties_for_tokens, entity_type_matches};

const BROWSE_PREFETCH_EXTRA: usize = 1000;

pub(super) fn enrich_from_triples(
    conn: &Connection,
    iri: &str,
    triples: &[crate::eavto::Triple],
    matched_properties: Vec<serde_json::Value>,
) -> SearchResult {
    let label = triples.iter()
        .find(|t| t.predicate == "rdfs:label")
        .and_then(|t| t.object.as_literal())
        .map(|s| s.to_string())
        .unwrap_or_else(|| iri.to_string());

    let icon = triples.iter()
        .find(|t| t.predicate == "foundation:hasIcon")
        .and_then(|t| match &t.object {
            Object::Iri(iri) => icon_iri_to_display(conn, iri),
            Object::Literal { value, .. } =>
                Some(crate::owl::icon_literal_to_display(value)),
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

    let class_type = if is_class {
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

    SearchResult {
        id: iri.to_string(),
        label,
        icon,
        entity_type,
        matched_properties,
        class_type,
        status,
    }
}

pub(super) fn search_structured(
    conn: &Connection,
    tokens: &[String],
    _entity_type_filter: Option<&str>,
    class_iri: Option<&str>,
    filters: Option<&[(String, String, String)]>,
    include_retracted: bool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<SearchResult>, usize)> {
    use crate::eavto::query;

    let candidate_iris: Vec<String> = if let Some(f) = filters {
        let constraint_refs: Vec<crate::eavto::query::PropertyFilter> = f.iter()
            .map(|(d, v, o)| crate::eavto::query::PropertyFilter::Compare(d.as_str(), v.as_str(), o.as_str()))
            .collect();
        if let Some(concept) = class_iri {
            let (iris, _) = Individual::find_by_class_and_properties_with_options(
                conn, concept, &constraint_refs, include_retracted, usize::MAX, 0, None,
            )?;
            iris
        } else {
            let (iris, _) = query::find_by_properties_with_options(
                conn, &constraint_refs, include_retracted, usize::MAX, 0,
            ).map_err(|e| OwlError::DatabaseError(e.to_string()))?;
            iris
        }
    } else if let Some(concept) = class_iri {
        if include_retracted {
            Individual::find_by_class_with_date_range(conn, concept, None, None, true)?
        } else {
            Class::get_instances(conn, concept)?
        }
    } else {
        return Err(OwlError::InvalidOperation("structured search requires class_iri or filters".to_string()));
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
        let results: Vec<SearchResult> = page.iter().map(|iri| {
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

    let results: Vec<SearchResult> = page.iter().map(|iri| {
        let empty = vec![];
        let triples = page_batch.get(iri.as_str()).unwrap_or(&empty);
        let mut matched_props = vec![];
        score_entity_against_tokens(iri, triples, tokens, &mut matched_props);
        enrich_from_triples(conn, iri, triples, matched_props)
    }).collect();

    Ok((results, total))
}

fn search_global_sql_fallback(
    conn: &Connection,
    tokens: &[String],
    entity_type_filter: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<SearchResult>, usize)> {
    use crate::eavto::query;

    let all_iris: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT subject FROM triples t
             WHERE predicate = 'rdfs:label' AND retracted = 0 AND t.is_current = 1",
        )
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?
        .query_map([], |row| row.get(0))
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let batch = query::batch_load_triples_for_subjects(conn, &all_iris)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let filter_entity_type = |iri: &String| -> bool {
        if entity_type_filter.is_none() {
            return true;
        }
        let empty = vec![];
        let triples = batch.get(iri.as_str()).unwrap_or(&empty);
        let type_iri = triples.iter()
            .find(|t| t.predicate == "rdf:type")
            .and_then(|t| t.object.as_iri());
        super::scoring::entity_type_matches(type_iri, entity_type_filter)
    };

    if tokens.is_empty() {
        let filtered: Vec<String> = all_iris.into_iter().filter(filter_entity_type).collect();
        let total = filtered.len();
        let page: Vec<String> = filtered.into_iter().skip(offset).take(limit).collect();
        let page_batch = query::batch_load_triples_for_subjects(conn, &page)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
        let results = page.iter().map(|iri| {
            let empty = vec![];
            let triples = page_batch.get(iri.as_str()).unwrap_or(&empty);
            enrich_from_triples(conn, iri, triples, vec![])
        }).collect();
        return Ok((results, total));
    }

    let mut scored: Vec<(String, i32)> = all_iris.iter()
        .filter(|iri| filter_entity_type(iri))
        .filter_map(|iri| {
            let empty = vec![];
            let triples = batch.get(iri.as_str()).unwrap_or(&empty);
            let mut matched = vec![];
            super::scoring::score_entity_against_tokens(iri, triples, tokens, &mut matched)
                .map(|score| (iri.clone(), score))
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));

    let total = scored.len();
    let page: Vec<String> = scored.into_iter().skip(offset).take(limit).map(|(iri, _)| iri).collect();
    let page_batch = query::batch_load_triples_for_subjects(conn, &page)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
    let results = page.iter().map(|iri| {
        let empty = vec![];
        let triples = page_batch.get(iri.as_str()).unwrap_or(&empty);
        let mut matched = vec![];
        super::scoring::score_entity_against_tokens(iri, triples, tokens, &mut matched);
        enrich_from_triples(conn, iri, triples, matched)
    }).collect();

    Ok((results, total))
}

pub(super) fn search_global(
    conn: &Connection,
    tokens: &[String],
    entity_type_filter: Option<&str>,
    class_iri: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<SearchResult>, usize)> {
    use crate::eavto::query;

    if !crate::search::is_initialized() {
        return search_global_sql_fallback(conn, tokens, entity_type_filter, limit, offset);
    }

    if tokens.is_empty() {
        if let Some(concept) = class_iri {
            let all_iris = Class::get_instances(conn, concept)?;
            let total = all_iris.len();
            let page: Vec<String> = all_iris.into_iter().skip(offset).take(limit).collect();
            let batch = query::batch_load_triples_for_subjects(conn, &page)
                .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
            let results: Vec<SearchResult> = page.iter().map(|iri| {
                let empty = vec![];
                let triples = batch.get(iri.as_str()).unwrap_or(&empty);
                enrich_from_triples(conn, iri, triples, vec![])
            }).collect();
            return Ok((results, total));
        }

        let big_limit = offset + limit + BROWSE_PREFETCH_EXTRA;
        let iris = crate::search::search_all(None, big_limit);
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
        let results: Vec<SearchResult> = page.iter().map(|iri| {
            let empty = vec![];
            let triples = batch.get(iri.as_str()).unwrap_or(&empty);
            enrich_from_triples(conn, iri, triples, vec![])
        }).collect();
        return Ok((results, total));
    }

    let query_str = tokens.join(" ");
    const TANTIVY_FETCH_MULTIPLIER: usize = 20;
    let fetch_limit = (offset + limit + 1) * TANTIVY_FETCH_MULTIPLIER;
    let iris = crate::search::search(&query_str, class_iri, fetch_limit);

    if iris.is_empty() {
        return Ok((vec![], 0));
    }

    let batch = query::batch_load_triples_for_subjects(conn, &iris)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let filtered: Vec<&String> = iris.iter()
        .filter(|iri| {
            let empty = vec![];
            let triples = batch.get(iri.as_str()).unwrap_or(&empty);
            if triples.is_empty() {
                return false;
            }
            if entity_type_filter.is_none() {
                return true;
            }
            let type_iri = triples.iter()
                .find(|t| t.predicate == "rdf:type")
                .and_then(|t| t.object.as_iri());
            entity_type_matches(type_iri, entity_type_filter)
        })
        .collect();

    let total = filtered.len();
    let page: Vec<&String> = filtered.into_iter().skip(offset).take(limit).collect();

    let results: Vec<SearchResult> = page.iter().map(|iri| {
        let empty = vec![];
        let triples = batch.get(iri.as_str()).unwrap_or(&empty);
        let matched_props = matched_properties_for_tokens(iri, triples, tokens);
        enrich_from_triples(conn, iri, triples, matched_props)
    }).collect();

    Ok((results, total))
}
