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
}

pub fn run_subconscious(content: &str, conn: &Connection) -> Vec<SubconsciousEntity> {
    let mut entities: Vec<SubconsciousEntity> = Vec::new();

    let scored = crate::search::search_with_scores(content, None, 10);
    crate::commands::log_backend("debug", &format!(
        "[subconscious] search '{}' → {} raw hits",
        &content[..content.len().min(60)],
        scored.len()
    ));
    for (iri, score) in &scored {
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

fn enrich(conn: &Connection, iri: &str, score: f32, is_open_loop: bool) -> Option<SubconsciousEntity> {
    let label = crate::owl::get_literal_property(conn, iri, "rdfs:label")
        .ok()
        .flatten()?;

    let type_iri = conn.query_row(
        "SELECT object FROM triples
         WHERE subject = ?1 AND retracted = 0 AND predicate = 'rdf:type'
           AND object NOT LIKE 'owl:%'
           AND object NOT LIKE 'rdf:%'
           AND object NOT LIKE 'rdfs:%'
         LIMIT 1",
        [iri],
        |row| row.get::<_, String>(0),
    ).ok()?;

    let type_label = crate::owl::get_literal_property(conn, &type_iri, "rdfs:label")
        .ok()
        .flatten()
        .unwrap_or_else(|| type_iri.clone());

    let icon = crate::owl::get_literal_property(conn, iri, "foundation:icon")
        .ok()
        .flatten()
        .or_else(|| crate::owl::get_literal_property(conn, &type_iri, "foundation:icon")
            .ok()
            .flatten());

    Some(SubconsciousEntity { iri: iri.to_string(), label, type_iri, type_label, icon, score, is_open_loop })
}

fn query_open_loops(conn: &Connection) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT DISTINCT t_type.subject
         FROM triples t_type
         JOIN triples t_status
           ON t_status.subject = t_type.subject
          AND t_status.predicate = 'foundation:hasStatus'
          AND t_status.retracted = 0
         JOIN triples t_status_label
           ON t_status_label.subject = t_status.object
          AND t_status_label.predicate = 'rdfs:label'
          AND t_status_label.retracted = 0
         WHERE t_type.retracted = 0
           AND t_type.predicate = 'rdf:type'
           AND (
               (t_type.object = 'foundation:UserProblem'
                AND t_status_label.object_value IN ('Pending', 'In Progress'))
               OR
               (t_type.object = 'foundation:Task'
                AND t_status_label.object_value IN ('Pending', 'Planned'))
           )
           AND NOT EXISTS (
               SELECT 1 FROM triples t_date
               WHERE t_date.subject = t_type.subject
                 AND t_date.predicate = 'foundation:dueDate'
                 AND t_date.retracted = 0
           )",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map([], |row| row.get::<_, String>(0))
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

pub fn format_context(entities: &[SubconsciousEntity]) -> Option<String> {
    if entities.is_empty() {
        return None;
    }

    let relevant: Vec<_> = entities.iter().filter(|e| !e.is_open_loop).collect();
    let open_loops: Vec<_> = entities.iter().filter(|e| e.is_open_loop).collect();

    let mut parts: Vec<String> = Vec::new();

    if !relevant.is_empty() {
        let mut lines = vec!["## Memory Context\nRelevant entities from your knowledge graph (ranked by relevance):".to_string()];
        for (i, e) in relevant.iter().enumerate() {
            lines.push(format!("{}. \"{}\" [{}] — {}", i + 1, e.label, e.type_label, e.iri));
        }
        parts.push(lines.join("\n"));
    }

    if !open_loops.is_empty() {
        let mut lines = vec!["## Open Loops\nPending problems and tasks requiring your attention:".to_string()];
        for e in &open_loops {
            lines.push(format!("- [{}] \"{}\" — {}", e.type_label, e.label, e.iri));
        }
        parts.push(lines.join("\n"));
    }

    Some(parts.join("\n\n"))
}
