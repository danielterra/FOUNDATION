use serde::{Deserialize, Serialize};
use crate::eavto::Connection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyHit {
    pub prop_label: String,
    pub prop_iri: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubconsciousEntity {
    pub iri: String,
    pub label: String,
    pub type_iri: String,
    pub type_label: String,
    pub icon: Option<String>,
    pub score: f32,
    #[serde(default)]
    pub property_hits: Vec<PropertyHit>,
    pub is_open_loop: Option<bool>,
}

const INSTANCE_SEARCH_LIMIT: usize = 10;
const CONCEPT_SEARCH_LIMIT: usize = 5;
const MAX_PROPERTY_HITS_PER_ENTITY: usize = 8;

const POSSESSIVE_PRONOUNS: &[&str] = &[
    "meu", "minha", "meus", "minhas", // PT
    "my",                              // EN
    "mi", "mis",                       // ES
];

fn resolve_possessive_entities(content: &str, conn: &Connection) -> Vec<(String, f32, String)> {
    let lower_words: Vec<String> = content
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();

    if !lower_words.iter().any(|w| POSSESSIVE_PRONOUNS.contains(&w.as_str())) {
        return vec![];
    }

    let this_user_triples = crate::owl::get_all_current_triples(conn, "foundation:ThisUser")
        .unwrap_or_default();

    let excluded = &["rdf:type", "foundation:hasStatus", "foundation:hasIcon"];
    let iri_props: Vec<(String, String)> = this_user_triples
        .into_iter()
        .filter(|t| {
            !excluded.contains(&t.predicate.as_str()) && t.object.as_iri().is_some()
        })
        .filter_map(|t| t.object.as_iri().map(|obj| (t.predicate.clone(), obj.to_string())))
        .collect();

    let mut results = vec![];
    for (predicate, obj_iri) in &iri_props {
        let pred_label = predicate_label(conn, predicate);
        if lower_words.iter().any(|w| *w == pred_label.to_lowercase()) {
            let inverse_label = crate::owl::Property::get(conn, predicate)
                .ok()
                .flatten()
                .and_then(|p| p.domain_labels.into_iter().next())
                .and_then(|dl| dl.inverse_label);
            let rel_label = inverse_label.unwrap_or(pred_label);
            results.push((obj_iri.clone(), 10.0_f32, rel_label));
        }
    }

    results
}

fn split_into_chunks(content: &str) -> Vec<String> {
    content
        .split(|c| matches!(c, '.' | '!' | '?' | '\n' | ';' | ','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn extract_chunk_query(chunk: &str) -> String {
    chunk
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| {
            !w.is_empty()
                && (w.chars().all(|c| c.is_uppercase())
                    || !super::stopwords::STOPWORDS.contains(&w.to_lowercase().as_str()))
                && (w.chars().count() >= 3 || w.chars().any(|c| c.is_ascii_digit()))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalizes a string for re-match: keeps only alphanumeric chars, lowercased.
/// Must be identical to the normalization applied in extract_chunk_query so that
/// term-comparison is symmetric.
fn normalize_for_match(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Derives the re-match term set from the full user message content.
/// Reuses extract_chunk_query over all chunks so the term set is exactly
/// the same tokens that drive Tantivy recall.
fn derive_query_terms(content: &str) -> Vec<String> {
    let chunks = split_into_chunks(content);
    let mut terms: std::collections::HashSet<String> = std::collections::HashSet::new();
    for chunk in &chunks {
        let q = extract_chunk_query(chunk);
        for token in q.split_whitespace() {
            let norm = normalize_for_match(token);
            if !norm.is_empty() {
                terms.insert(norm);
            }
        }
    }
    // Also include possessive pronoun tokens from the full message so the
    // term set covers possessive-resolved entities.
    let full_q = extract_chunk_query(content);
    for token in full_q.split_whitespace() {
        let norm = normalize_for_match(token);
        if !norm.is_empty() {
            terms.insert(norm);
        }
    }
    terms.into_iter().collect()
}

pub fn run_subconscious(content: &str, exclude_iri: Option<&str>, conn: &Connection) -> Vec<SubconsciousEntity> {
    let mut entities: Vec<SubconsciousEntity> = Vec::new();
    let mut seen_iris: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Derive query term set once — reused by all enrich calls for property re-match.
    let query_terms = derive_query_terms(content);

    // Resolve possessive pronouns → ThisUser properties (e.g. "my mother" → Andrea Terra)
    for (iri, score, rel_label) in resolve_possessive_entities(content, conn) {
        if exclude_iri.map_or(false, |ex| ex == iri) { continue; }
        if !seen_iris.insert(iri.clone()) { continue; }
        if let Some(mut entity) = enrich(conn, &iri, score, &query_terms) {
            let user_label = crate::owl::get_literal_property(conn, "foundation:ThisUser", "rdfs:label")
                .ok()
                .flatten()
                .unwrap_or_else(|| "foundation:ThisUser".to_string());
            // Inject the possessive relationship as the first property hit so the AI
            // sees "minha mãe → Andrea Terra" even if "mãe" didn't match a prop on Andrea.
            entity.property_hits.insert(0, PropertyHit {
                prop_label: rel_label,
                prop_iri: "foundation:ThisUser".to_string(),
                value: format!("{user_label} (foundation:ThisUser)"),
            });
            entities.push(entity);
        }
    }

    let full_query = extract_chunk_query(content);
    let concept_query = if full_query.is_empty() { content } else { &full_query };

    let concept_hits = crate::search::search_concepts_with_scores(concept_query, CONCEPT_SEARCH_LIMIT);
    crate::commands::log_backend("debug", &format!(
        "[subconscious] concept search '{}' → {} hits",
        &concept_query[..concept_query.len().min(60)],
        concept_hits.len()
    ));
    for (iri, score) in &concept_hits {
        if exclude_iri.map_or(false, |ex| ex == iri) { continue; }
        if !seen_iris.insert(iri.clone()) { continue; }
        match enrich(conn, iri, *score, &query_terms) {
            Some(entity) => {
                if matches!(entity.type_iri.as_str(),
                    "owl:ObjectProperty" | "owl:DatatypeProperty" | "owl:AnnotationProperty"
                ) { continue; }
                entities.push(entity);
            }
            None => crate::commands::log_backend("debug", &format!(
                "[subconscious] enrich failed for {} (score={:.3})", iri, score
            )),
        }
    }

    let chunks = split_into_chunks(content);
    let mut score_map: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for chunk in &chunks {
        let query = extract_chunk_query(chunk);
        if query.is_empty() { continue; }
        for (iri, score) in crate::search::search_with_scores(&query, None, INSTANCE_SEARCH_LIMIT * 3) {
            if iri.starts_with("foundation:AIConversationMessage_") { continue; }
            if iri.contains(":TextBlock_") { continue; }
            let entry = score_map.entry(iri).or_insert(0.0);
            if score > *entry { *entry = score; }
        }
    }
    let mut scored: Vec<(String, f32)> = score_map.into_iter().collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(INSTANCE_SEARCH_LIMIT);

    crate::commands::log_backend("debug", &format!(
        "[subconscious] instance search: {} chunks → {} unique hits",
        chunks.len(), scored.len()
    ));
    for (iri, score) in &scored {
        if exclude_iri.map_or(false, |ex| ex == iri) { continue; }
        if !seen_iris.insert(iri.clone()) { continue; }
        match enrich(conn, iri, *score, &query_terms) {
            Some(entity) => entities.push(entity),
            None => crate::commands::log_backend("debug", &format!(
                "[subconscious] enrich failed for {} (score={:.3})", iri, score
            )),
        }
    }

    entities
}

const MESSAGE_LABEL_LEN: usize = 120;
const PROPERTY_TRUNCATE_LEN: usize = 255;
const TRUNCATE_SUFFIX: &str = "...[truncated content]";

const EXCLUDED_PREDICATES: &[&str] = &[
    "rdf:type",
    "rdfs:label",
    "foundation:content",
    "foundation:subconsciousContext",
    "foundation:role",
    "foundation:sentAt",
    "foundation:partOfConversation",
    "foundation:sender",
    "foundation:receiver",
];

fn truncate(value: &str) -> String {
    let flat: String = value.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= PROPERTY_TRUNCATE_LEN {
        flat
    } else {
        let head: String = flat.chars().take(PROPERTY_TRUNCATE_LEN).collect();
        format!("{}{}", head, TRUNCATE_SUFFIX)
    }
}

fn predicate_label(conn: &Connection, predicate: &str) -> String {
    crate::owl::get_literal_property(conn, predicate, "rdfs:label")
        .ok()
        .flatten()
        .unwrap_or_else(|| {
            predicate.rsplit_once(':')
                .map(|(_, local)| local.to_string())
                .unwrap_or_else(|| predicate.to_string())
        })
}

/// Checks whether any term in `query_terms` (already normalized) is a substring
/// of the normalized form of `text`. Match is case-insensitive via pre-normalization.
fn any_term_matches(query_terms: &[String], text: &str) -> bool {
    let norm = normalize_for_match(text);
    query_terms.iter().any(|term| norm.contains(term.as_str()))
}

fn fetch_property_hits(conn: &Connection, iri: &str, query_terms: &[String]) -> Vec<PropertyHit> {
    let current_triples = match crate::owl::get_all_current_triples(conn, iri) {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    let matched: Vec<PropertyHit> = current_triples
        .into_iter()
        .filter(|t| !EXCLUDED_PREDICATES.contains(&t.predicate.as_str()))
        .filter_map(|t| {
            let prop_iri = t.predicate.clone();
            let prop_label = predicate_label(conn, &prop_iri);

            // Resolve IRI objects to their rdfs:label; keep literals as-is.
            let raw_value = match t.object.as_iri() {
                Some(obj_iri) => {
                    crate::owl::get_literal_property(conn, obj_iri, "rdfs:label")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| obj_iri.to_string())
                }
                None => t.object.as_literal()?,
            };

            // Re-match: include hit only if any query term matches prop_label OR raw value.
            // Truncation is applied AFTER the match check, only at the display boundary.
            if !any_term_matches(query_terms, &prop_label)
                && !any_term_matches(query_terms, &raw_value)
            {
                return None;
            }

            Some(PropertyHit {
                prop_label,
                prop_iri,
                value: truncate(&raw_value),
            })
        })
        .collect();

    let total = matched.len();
    if total > MAX_PROPERTY_HITS_PER_ENTITY {
        crate::commands::log_backend("debug", &format!(
            "[subconscious] property hits truncated for {}: {} matched, {} discarded (limit={})",
            iri, total, total - MAX_PROPERTY_HITS_PER_ENTITY, MAX_PROPERTY_HITS_PER_ENTITY
        ));
        matched.into_iter().take(MAX_PROPERTY_HITS_PER_ENTITY).collect()
    } else {
        matched
    }
}

fn enrich(conn: &Connection, iri: &str, score: f32, query_terms: &[String]) -> Option<SubconsciousEntity> {
    let all_type_iris = crate::owl::get_all_iri_properties(conn, iri, "rdf:type")
        .unwrap_or_default();

    let instance_type = all_type_iris.iter()
        .find(|t| !t.starts_with("owl:") && !t.starts_with("rdf:") && !t.starts_with("rdfs:"))
        .cloned();

    let meta_type = if instance_type.is_none() {
        all_type_iris.into_iter().next()
    } else {
        None
    };

    let type_iri = instance_type.or(meta_type)?;

    let label = if type_iri == "foundation:AIConversationMessage" {
        let raw = crate::owl::get_literal_property(conn, iri, "foundation:content")
            .ok()
            .flatten()?;
        let text = extract_content_text(&raw);
        if text.is_empty() { return None; }
        let preview: String = text.chars().take(MESSAGE_LABEL_LEN).collect();
        let role = crate::owl::get_literal_property(conn, iri, "foundation:role")
            .ok()
            .flatten();
        match role.as_deref() {
            Some("user")      => format!("You: {}", preview),
            Some("assistant") => format!("Assistant: {}", preview),
            _                 => preview,
        }
    } else {
        crate::owl::get_literal_property(conn, iri, "rdfs:label")
            .ok()
            .flatten()?
    };

    let type_label = crate::owl::get_literal_property(conn, &type_iri, "rdfs:label")
        .ok()
        .flatten()
        .unwrap_or_else(|| match type_iri.as_str() {
            "owl:Class"              => "Class".to_string(),
            "owl:ObjectProperty"     => "Object Property".to_string(),
            "owl:DatatypeProperty"   => "Datatype Property".to_string(),
            "owl:AnnotationProperty" => "Annotation Property".to_string(),
            other => other.rsplit_once(':').map(|(_, l)| l.to_string()).unwrap_or_else(|| other.to_string()),
        });

    let icon = crate::owl::Thing::get(conn, iri).icon
        .or_else(|| crate::owl::Thing::get(conn, &type_iri).icon)
        .or_else(|| match type_iri.as_str() {
            "owl:Class"                                      => Some("category".to_string()),
            "owl:ObjectProperty" | "owl:DatatypeProperty" |
            "owl:AnnotationProperty"                         => Some("link".to_string()),
            _                                                => Some("chat".to_string()),
        });

    let property_hits = fetch_property_hits(conn, iri, query_terms);

    Some(SubconsciousEntity {
        iri: iri.to_string(),
        label,
        type_iri,
        type_label,
        icon,
        score,
        property_hits,
        is_open_loop: None,
    })
}

fn extract_content_text(raw: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(arr) = value.as_array() {
            let text: String = arr.iter()
                .filter_map(|b| b.get("text")?.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            if !text.is_empty() {
                return text;
            }
        }
        if let Some(s) = value.as_str() {
            return s.to_string();
        }
    }
    raw.to_string()
}

pub fn format_context(entities: &[SubconsciousEntity]) -> Option<String> {
    if entities.is_empty() {
        return None;
    }

    let header = concat!(
        "## Memory Context\n",
        "Relevant entities from your knowledge graph (ranked by relevance):");
    let mut lines = vec![header.to_string()];

    for e in entities {
        if e.property_hits.is_empty() {
            // Anchor line: entity has no matching property hits.
            lines.push(format!(
                "- entity_label=\"{}\" entity_iri={} type={}",
                e.label, e.iri, e.type_iri
            ));
        } else {
            // One flat line per property hit, each repeating entity identity.
            for hit in &e.property_hits {
                lines.push(format!(
                    "- entity_label=\"{}\" entity_iri={} prop_label=\"{}\" prop_iri={} value=\"{}\"",
                    e.label, e.iri, hit.prop_label, hit.prop_iri, hit.value
                ));
            }
        }
    }

    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{
        split_into_chunks, extract_chunk_query,
        normalize_for_match, any_term_matches, derive_query_terms,
        PropertyHit, SubconsciousEntity, format_context,
    };

    #[test]
    fn test_split_into_chunks_sentence_boundaries() {
        let chunks = split_into_chunks("Hello world. How are you? Fine!");
        assert_eq!(chunks, vec!["Hello world", "How are you", "Fine"]);
    }

    #[test]
    fn test_split_into_chunks_comma_separated() {
        let chunks = split_into_chunks("falei com a Mariana, ela vai me passar algumas coisas");
        assert_eq!(chunks, vec!["falei com a Mariana", "ela vai me passar algumas coisas"]);
    }

    #[test]
    fn test_split_into_chunks_newlines() {
        let chunks = split_into_chunks("first line\nsecond line\nthird");
        assert_eq!(chunks, vec!["first line", "second line", "third"]);
    }

    #[test]
    fn test_extract_chunk_query_filters_short_words() {
        let q = extract_chunk_query("falei com a Mariana ela vai passar");
        assert_eq!(q, "falei Mariana passar");
    }

    #[test]
    fn test_extract_chunk_query_keeps_short_meaningful_words() {
        // PT input: "mãe" (3 chars, not a stopword) must be kept; "me" (2 chars) must be filtered.
        let q = extract_chunk_query("quando minha mãe precisa me pagar");
        assert!(q.contains("mãe"), "should keep 'mãe' (3 chars, not a stopword)");
        assert!(!q.contains(" me ") && !q.ends_with(" me"), "should filter 'me' (2 chars)");
    }

    #[test]
    fn test_extract_chunk_query_keeps_digits() {
        let q = extract_chunk_query("transferir os 26k de uma vez");
        assert!(q.contains("26k"), "should keep '26k' even though it's 3 chars");
        assert!(!q.contains(" os "), "should drop 'os'");
        assert!(!q.contains(" de "), "should drop 'de'");
    }

    #[test]
    fn test_extract_chunk_query_strips_punctuation() {
        let q = extract_chunk_query("Mariana, Terra.");
        assert_eq!(q, "Mariana Terra");
    }

    #[test]
    fn test_chunked_approach_finds_entity_late_in_message() {
        let long_msg = "I did shopping today, talked to family, had lunch, went for a walk, and by the way the Mariana debt needs attention";
        let chunks = split_into_chunks(long_msg);
        let last_chunk_query = extract_chunk_query(chunks.last().unwrap());
        assert!(last_chunk_query.contains("Mariana"), "should find Mariana even at the end of a long message");
    }

    #[test]
    fn test_normalize_for_match_strips_punctuation_and_lowercases() {
        assert_eq!(normalize_for_match("Mariana!"), "mariana");
        assert_eq!(normalize_for_match("São Paulo"), "saopaulo");
        assert_eq!(normalize_for_match("R$ 1.500"), "r1500");
    }

    #[test]
    fn test_any_term_matches_substring() {
        let terms: Vec<String> = vec!["mariana".to_string(), "debt".to_string()];
        assert!(any_term_matches(&terms, "Mariana Terra"), "should match 'Mariana' via substring");
        assert!(any_term_matches(&terms, "Has debt"));
        assert!(!any_term_matches(&terms, "Andrea Silva"));
    }

    #[test]
    fn test_derive_query_terms_deduplicates() {
        let terms = derive_query_terms("Mariana, Mariana precisa pagar");
        assert_eq!(terms.iter().filter(|t| t.as_str() == "mariana").count(), 1,
            "Mariana should appear only once in the term set");
    }

    #[test]
    fn test_format_context_anchor_line_for_no_hits() {
        let entity = SubconsciousEntity {
            iri: "foundation:Person_1".to_string(),
            label: "Andrea Terra".to_string(),
            type_iri: "foundation:Person".to_string(),
            type_label: "Person".to_string(),
            icon: None,
            score: 0.9,
            property_hits: vec![],
            is_open_loop: None,
        };
        let ctx = format_context(&[entity]).unwrap();
        assert!(ctx.contains("entity_label=\"Andrea Terra\""), "should include entity_label");
        assert!(ctx.contains("entity_iri=foundation:Person_1"), "should include entity_iri");
        assert!(ctx.contains("type=foundation:Person"), "should include type");
        assert!(!ctx.contains("prop_label"), "anchor line must not contain prop_label");
    }

    #[test]
    fn test_format_context_flat_lines_per_hit() {
        let entity = SubconsciousEntity {
            iri: "foundation:Person_1".to_string(),
            label: "Andrea Terra".to_string(),
            type_iri: "foundation:Person".to_string(),
            type_label: "Person".to_string(),
            icon: None,
            score: 0.9,
            property_hits: vec![
                PropertyHit {
                    prop_label: "Debt".to_string(),
                    prop_iri: "foundation:hasDebt".to_string(),
                    value: "R$ 500".to_string(),
                },
                PropertyHit {
                    prop_label: "Phone".to_string(),
                    prop_iri: "foundation:phone".to_string(),
                    value: "11 9999-0000".to_string(),
                },
            ],
            is_open_loop: None,
        };
        let ctx = format_context(&[entity]).unwrap();
        let lines: Vec<&str> = ctx.lines().collect();
        // Header + 2 hit lines
        assert_eq!(lines.len(), 3, "should have header + one line per hit");
        for line in &lines[1..] {
            assert!(line.contains("entity_label=\"Andrea Terra\""), "each hit line must repeat entity_label");
            assert!(line.contains("entity_iri=foundation:Person_1"), "each hit line must repeat entity_iri");
            assert!(line.contains("prop_iri="), "each hit line must have prop_iri");
            assert!(line.contains("prop_label="), "each hit line must have prop_label");
            assert!(line.contains("value="), "each hit line must have value");
        }
    }

    #[test]
    fn test_serde_roundtrip_property_hits() {
        let entity = SubconsciousEntity {
            iri: "foundation:X".to_string(),
            label: "X".to_string(),
            type_iri: "foundation:T".to_string(),
            type_label: "T".to_string(),
            icon: None,
            score: 1.0,
            property_hits: vec![PropertyHit {
                prop_label: "Name".to_string(),
                prop_iri: "rdfs:label".to_string(),
                value: "Foo".to_string(),
            }],
            is_open_loop: Some(true),
        };
        let json = serde_json::to_string(&entity).unwrap();
        let de: SubconsciousEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(de.property_hits.len(), 1);
        assert_eq!(de.property_hits[0].prop_iri, "rdfs:label");
        assert_eq!(de.is_open_loop, Some(true));
    }

    #[test]
    fn test_serde_default_property_hits_absent() {
        // Simulates deserialization of a historical payload without property_hits field.
        let json = r#"{"iri":"foundation:X","label":"X","type_iri":"foundation:T","type_label":"T","icon":null,"score":0.5}"#;
        let de: SubconsciousEntity = serde_json::from_str(json).unwrap();
        assert!(de.property_hits.is_empty(), "missing property_hits should deserialize as empty vec");
    }

    #[test]
    fn test_max_property_hits_per_entity_constant() {
        // Verifies the cap constant is in place and that format_context renders all hits when
        // within budget — the truncation itself happens inside fetch_property_hits (requires a
        // live DB), but we confirm that entities with exactly MAX hits render without loss.
        let hits: Vec<PropertyHit> = (0..super::MAX_PROPERTY_HITS_PER_ENTITY)
            .map(|i| PropertyHit {
                prop_label: format!("Prop {i}"),
                prop_iri: format!("foundation:prop{i}"),
                value: format!("val{i}"),
            })
            .collect();
        let entity = SubconsciousEntity {
            iri: "foundation:E_1".to_string(),
            label: "Entity".to_string(),
            type_iri: "foundation:T".to_string(),
            type_label: "T".to_string(),
            icon: None,
            score: 1.0,
            property_hits: hits,
            is_open_loop: None,
        };
        let ctx = format_context(&[entity]).unwrap();
        // Header + MAX_PROPERTY_HITS_PER_ENTITY lines
        assert_eq!(
            ctx.lines().count(),
            1 + super::MAX_PROPERTY_HITS_PER_ENTITY,
            "format_context must emit exactly one line per hit when at the cap"
        );
    }
}
