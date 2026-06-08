use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubconsciousEntity {
    pub iri: String,
    pub label: String,
    pub type_iri: String,
    pub type_label: String,
    pub icon: Option<String>,
    pub score: f32,
    pub properties: Vec<(String, String)>,
}

const INSTANCE_SEARCH_LIMIT: usize = 10;
const CONCEPT_SEARCH_LIMIT: usize = 5;

const POSSESSIVE_PRONOUNS: &[&str] = &[
    "meu", "minha", "meus", "minhas", // PT
    "my",                              // EN
    "mi", "mis",                       // ES
];

fn resolve_possessive_entities(content: &str, conn: &rusqlite::Connection) -> Vec<(String, f32, String)> {
    let lower_words: Vec<String> = content
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();

    if !lower_words.iter().any(|w| POSSESSIVE_PRONOUNS.contains(&w.as_str())) {
        return vec![];
    }

    let this_user_props: Vec<(String, String)> = conn
        .prepare(
            "SELECT predicate, object FROM triples \
             WHERE subject = 'foundation:ThisUser' AND retracted = 0 \
               AND object_type = 'iri' \
               AND predicate NOT IN ('rdf:type', 'foundation:hasStatus', 'foundation:hasIcon')",
        )
        .map(|mut stmt| {
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
            .unwrap_or_default()
        })
        .unwrap_or_default();

    let mut results = vec![];
    for (predicate, obj_iri) in &this_user_props {
        let pred_label: Option<String> = conn.query_row(
            "SELECT object_value FROM triples \
             WHERE subject = ?1 AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1",
            [predicate.as_str()],
            |row| row.get(0),
        ).ok();

        if let Some(label) = pred_label {
            if lower_words.iter().any(|w| *w == label.to_lowercase()) {
                // Use inverseLabel (from DomainLabel) when available — it describes
                // the relationship from the matched entity's perspective (e.g. "son" for "mother").
                let inverse_label: Option<String> = conn.query_row(
                    "SELECT object_value FROM triples
                     WHERE predicate = 'foundation:inverseLabel' AND retracted = 0
                       AND subject IN (
                           SELECT subject FROM triples
                           WHERE predicate = 'foundation:onProperty'
                             AND object = ?1 AND retracted = 0
                       )
                     ORDER BY tx DESC LIMIT 1",
                    [predicate.as_str()],
                    |row| row.get(0),
                ).ok();
                let rel_label = inverse_label.unwrap_or(label);
                results.push((obj_iri.clone(), 10.0_f32, rel_label));
            }
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

pub fn run_subconscious(content: &str, exclude_iri: Option<&str>, conn: &Connection) -> Vec<SubconsciousEntity> {
    let mut entities: Vec<SubconsciousEntity> = Vec::new();
    let mut seen_iris: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Resolve possessive pronouns → ThisUser properties (e.g. "my mother" → Andrea Terra)
    for (iri, score, rel_label) in resolve_possessive_entities(content, conn) {
        if exclude_iri.map_or(false, |ex| ex == iri) { continue; }
        if !seen_iris.insert(iri.clone()) { continue; }
        if let Some(mut entity) = enrich(conn, &iri, score) {
            let user_label: String = conn.query_row(
                "SELECT object_value FROM triples \
                 WHERE subject = 'foundation:ThisUser' AND predicate = 'rdfs:label' \
                   AND retracted = 0 LIMIT 1",
                [],
                |row| row.get(0),
            ).unwrap_or_else(|_| "foundation:ThisUser".to_string());
            entity.properties.insert(0, (rel_label, format!("{user_label} (foundation:ThisUser)")));
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
        match enrich(conn, iri, *score) {
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
        match enrich(conn, iri, *score) {
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
    conn.query_row(
        "SELECT object_value FROM triples
         WHERE subject = ?1 AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1",
        [predicate],
        |row| row.get::<_, String>(0),
    ).ok().unwrap_or_else(|| {
        predicate.rsplit_once(':')
            .map(|(_, local)| local.to_string())
            .unwrap_or_else(|| predicate.to_string())
    })
}

fn fetch_properties(conn: &Connection, iri: &str) -> Vec<(String, String)> {
    let excluded_placeholders = EXCLUDED_PREDICATES.iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT predicate, object, object_value, object_type FROM triples
         WHERE subject = ?1 AND retracted = 0
           AND predicate NOT IN ({})
         ORDER BY predicate",
        excluded_placeholders
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&iri as &dyn rusqlite::ToSql];
    let excluded: Vec<&str> = EXCLUDED_PREDICATES.to_vec();
    for p in &excluded {
        params.push(p as &dyn rusqlite::ToSql);
    }

    let rows: Vec<(String, Option<String>, Option<String>, String)> = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    rows.into_iter().filter_map(|(pred, obj, obj_val, obj_type)| {
        let value = if obj_type == "iri" || obj_type == "blank" {
            let obj_iri = obj?;
            conn.query_row(
                "SELECT object_value FROM triples
                 WHERE subject = ?1 AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1",
                [&obj_iri],
                |row| row.get::<_, String>(0),
            ).ok().unwrap_or(obj_iri)
        } else {
            obj_val?
        };

        Some((predicate_label(conn, &pred), truncate(&value)))
    }).collect()
}

fn enrich(conn: &Connection, iri: &str, score: f32) -> Option<SubconsciousEntity> {
    let instance_type = conn.query_row(
        "SELECT object FROM triples
         WHERE subject = ?1 AND retracted = 0 AND predicate = 'rdf:type'
           AND object NOT LIKE 'owl:%'
           AND object NOT LIKE 'rdf:%'
           AND object NOT LIKE 'rdfs:%'
         LIMIT 1",
        [iri],
        |row| row.get::<_, String>(0),
    ).ok();

    let meta_type = if instance_type.is_none() {
        conn.query_row(
            "SELECT object FROM triples
             WHERE subject = ?1 AND retracted = 0 AND predicate = 'rdf:type'
             LIMIT 1",
            [iri],
            |row| row.get::<_, String>(0),
        ).ok()
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
        let role = conn.query_row(
            "SELECT object_value FROM triples
             WHERE subject = ?1 AND predicate = 'foundation:role' AND retracted = 0 LIMIT 1",
            [iri],
            |row| row.get::<_, String>(0),
        ).ok();
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

    let properties = fetch_properties(conn, iri);

    Some(SubconsciousEntity {
        iri: iri.to_string(),
        label,
        type_iri,
        type_label,
        icon,
        score,
        properties,
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
    for (i, e) in entities.iter().enumerate() {
        lines.push(format!("{}. \"{}\" [{}] — {}", i + 1, e.label, e.type_label, e.iri));
        for (key, val) in &e.properties {
            lines.push(format!("   - {}: {}", key, val));
        }
    }

    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{split_into_chunks, extract_chunk_query};

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
}
