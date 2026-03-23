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
    pub is_open_loop: bool,
    pub properties: Vec<(String, String)>,
}

const INSTANCE_SEARCH_LIMIT: usize = 10;
const CONCEPT_SEARCH_LIMIT: usize = 5;

pub fn run_subconscious(content: &str, exclude_iri: Option<&str>, conn: &Connection) -> Vec<SubconsciousEntity> {
    let mut entities: Vec<SubconsciousEntity> = Vec::new();
    let mut seen_iris: std::collections::HashSet<String> = std::collections::HashSet::new();

    let concept_hits = crate::search::search_concepts_with_scores(content, CONCEPT_SEARCH_LIMIT);
    crate::commands::log_backend("debug", &format!(
        "[subconscious] concept search '{}' → {} hits",
        &content[..content.len().min(60)],
        concept_hits.len()
    ));
    for (iri, score) in &concept_hits {
        if exclude_iri.map_or(false, |ex| ex == iri) { continue; }
        if !seen_iris.insert(iri.clone()) { continue; }
        match enrich(conn, iri, *score, false) {
            Some(entity) => entities.push(entity),
            None => crate::commands::log_backend("debug", &format!(
                "[subconscious] enrich failed for {} (score={:.3})", iri, score
            )),
        }
    }

    let scored = crate::search::search_with_scores(content, None, INSTANCE_SEARCH_LIMIT);
    crate::commands::log_backend("debug", &format!(
        "[subconscious] instance search '{}' → {} raw hits",
        &content[..content.len().min(60)],
        scored.len()
    ));
    for (iri, score) in &scored {
        if exclude_iri.map_or(false, |ex| ex == iri) { continue; }
        if !seen_iris.insert(iri.clone()) { continue; }
        match enrich(conn, iri, *score, false) {
            Some(entity) => entities.push(entity),
            None => crate::commands::log_backend("debug", &format!(
                "[subconscious] enrich failed for {} (score={:.3})", iri, score
            )),
        }
    }

    let open_loops = query_open_loops(conn);
    crate::commands::log_backend("debug", &format!(
        "[subconscious] open loops: {}", open_loops.len()
    ));
    for iri in open_loops {
        if entities.iter().any(|e| e.iri == iri) {
            continue;
        }
        if let Some(entity) = enrich(conn, &iri, 1.0, true) {
            entities.push(entity);
        }
    }

    entities
}

const MESSAGE_LABEL_LEN: usize = 120;
const OPEN_LOOPS_LIMIT: usize = 10;
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

fn enrich(
    conn: &Connection,
    iri: &str,
    score: f32,
    is_open_loop: bool,
) -> Option<SubconsciousEntity> {
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
        is_open_loop,
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

fn query_open_loops(conn: &Connection) -> Vec<String> {
    use chrono::Datelike;

    let tomorrow = chrono::Local::now() + chrono::Duration::days(1);
    let tomorrow_str = tomorrow.format("%Y-%m-%d").to_string();

    let props: &[(&str, &str, &str)] = &[
        ("foundation:hasStatus", "foundation:Completed", "!="),
        ("foundation:dueDate", &tomorrow_str, "?<="),
    ];

    let mut results = Vec::new();
    for class_iri in &["foundation:Task", "foundation:UserProblem"] {
        if let Ok((iris, _)) = crate::owl::Individual::find_by_class_and_properties_with_options(
            conn, class_iri, props, false, usize::MAX, 0,
        ) {
            results.extend(iris);
        }
    }

    let today_day = chrono::Local::now().day().to_string();
    let tomorrow_day = tomorrow.day().to_string();
    for day_str in [today_day.as_str(), tomorrow_day.as_str()] {
        let props = &[("foundation:dueDayOfMonth", day_str, "=")];
        if let Ok((iris, _)) = crate::owl::Individual::find_by_class_and_properties_with_options(
            conn, "foundation:RecurringPurchase", props, false, usize::MAX, 0,
        ) {
            results.extend(iris);
        }
    }

    results.truncate(OPEN_LOOPS_LIMIT);
    results
}

pub fn format_context(entities: &[SubconsciousEntity]) -> Option<String> {
    if entities.is_empty() {
        return None;
    }

    let relevant: Vec<_> = entities.iter().filter(|e| !e.is_open_loop).collect();
    let open_loops: Vec<_> = entities.iter().filter(|e| e.is_open_loop).collect();

    let mut parts: Vec<String> = Vec::new();

    if !relevant.is_empty() {
        let header = concat!(
            "## Memory Context\n",
            "Relevant entities from your knowledge graph (ranked by relevance):");
        let mut lines = vec![header.to_string()];
        for (i, e) in relevant.iter().enumerate() {
            lines.push(format!("{}. \"{}\" [{}] — {}", i + 1, e.label, e.type_label, e.iri));
            for (key, val) in &e.properties {
                lines.push(format!("   - {}: {}", key, val));
            }
        }
        parts.push(lines.join("\n"));
    }

    if !open_loops.is_empty() {
        let header = concat!(
            "## Open Loops\n",
            "Pending problems and tasks requiring your attention:");
        let mut lines = vec![header.to_string()];
        for e in &open_loops {
            lines.push(format!("- [{}] \"{}\" — {}", e.type_label, e.label, e.iri));
            for (key, val) in &e.properties {
                lines.push(format!("   - {}: {}", key, val));
            }
        }
        parts.push(lines.join("\n"));
    }

    Some(parts.join("\n\n"))
}
