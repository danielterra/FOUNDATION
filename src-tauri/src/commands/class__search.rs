use crate::{AppState, SearchResult};
use crate::namespaces;

#[tauri::command]
pub fn search_classes(state: tauri::State<AppState>, query: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        if query.trim().is_empty() {
            return Ok(serde_json::to_string(&Vec::<SearchResult>::new()).unwrap());
        }

        // Build search pattern for LIKE queries
        let search_pattern = format!("%{}%", query.to_lowercase());

        // Get all owl:Class and rdfs:Class entities
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT subject, tx FROM triples
                 WHERE predicate = 'rdf:type'
                 AND object_type = 'iri'
                 AND (object = 'owl:Class' OR object = 'rdfs:Class')
                 AND retracted = 0
                 AND subject NOT LIKE '_:%'"
            )
            .map_err(|e| format!("Prepare error: {}", e))?;

        let class_entities: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        let mut results = Vec::new();

        for (entity_id, tx) in class_entities {
            // Get label (prefer object_value for literals, fallback to object for IRIs)
            let clean_label: String = conn
                .query_row(
                    "SELECT COALESCE(object_value, object) FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1",
                    [&entity_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| {
                    entity_id.split(&['/', '#', ':'][..]).last().unwrap_or(&entity_id).to_string()
                });

            // Get definition (check multiple definition properties, prefer object_value for literals)
            let definition: Option<String> = conn
                .query_row(
                    "SELECT COALESCE(object_value, object) FROM triples
                     WHERE subject = ?
                     AND predicate IN ('skos:definition', 'rdfs:comment')
                     AND retracted = 0
                     LIMIT 1",
                    [&entity_id],
                    |row| row.get(0),
                )
                .ok();

            // Calculate relevance score with weighted matches
            let query_lower = query.to_lowercase();
            let label_lower = clean_label.to_lowercase();

            let mut score = 0.0f32;

            // Exact match in label (highest priority)
            if label_lower == query_lower {
                score += 100.0;
            }
            // Label starts with query (very high priority)
            else if label_lower.starts_with(&query_lower) {
                score += 50.0;
            }
            // Label contains query (high priority)
            else if label_lower.contains(&query_lower) {
                score += 30.0;
            }

            // Definition match (medium priority)
            if let Some(def) = &definition {
                if def.to_lowercase().contains(&query_lower) {
                    score += 10.0;
                }
            }

            // ID match (low priority)
            if entity_id.to_lowercase().contains(&query_lower) {
                score += 5.0;
            }

            // Only include if there's a match
            if score > 0.0 {
                let group = if tx <= 100 {
                    1
                } else if tx <= 10000 {
                    2
                } else if tx <= 20000 {
                    3
                } else if tx <= 30000 {
                    4
                } else {
                    5
                };

                results.push(SearchResult {
                    id: entity_id,
                    label: clean_label,
                    definition,
                    group,
                    score,
                });
            }
        }

        // Sort by score (descending) and limit to top 5
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(5);

        // Expand IRIs back to full form for frontend
        let expanded_results: Vec<SearchResult> = results.into_iter().map(|mut result| {
            result.id = namespaces::expand_iri(&result.id);
            result
        }).collect();

        Ok(serde_json::to_string(&expanded_results).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}
