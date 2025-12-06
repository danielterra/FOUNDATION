use crate::{AppState, NodeStatistics};
use crate::namespaces;

#[tauri::command]
pub fn get_node_statistics(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let compressed_node_id = namespaces::compress_iri(&node_id);

        // Count children (things that have this as rdfs:subClassOf or rdfs:subPropertyOf or skos:broader)
        let children_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT subject) FROM triples WHERE object = ? AND predicate IN ('rdfs:subClassOf', 'rdfs:subPropertyOf', 'skos:broader') AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count all backlinks
        let backlinks_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE object = ? AND object_type = 'iri' AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count synonyms (skos:altLabel)
        let synonyms_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = 'skos:altLabel' AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count related concepts (skos:related, FOUNDATION:antonym, etc.)
        let related_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate IN ('skos:related', 'FOUNDATION:antonym', 'rdfs:seeAlso', 'FOUNDATION:causes', 'FOUNDATION:entails') AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count examples (skos:example)
        let examples_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = 'skos:example' AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let stats = NodeStatistics {
            children_count,
            backlinks_count,
            synonyms_count,
            related_count,
            examples_count,
        };

        Ok(serde_json::to_string(&stats).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}
