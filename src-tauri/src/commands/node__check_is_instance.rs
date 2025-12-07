use crate::AppState;

// Check if a node is an instance (has rdf:type that is not owl:Class or rdfs:Class)
#[tauri::command]
pub fn node__check_is_instance(state: tauri::State<AppState>, node_id: String) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Compress the node ID
        let compressed_node_id = crate::namespaces::compress_iri(&node_id);

        // Check if node has rdf:type pointing to a user-defined class (not owl:Class/rdfs:Class)
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples
             WHERE subject = ?
             AND predicate = 'rdf:type'
             AND object_type = 'iri'
             AND retracted = 0
             AND object NOT IN ('owl:Class', 'rdfs:Class')",
            [&compressed_node_id],
            |row| row.get(0)
        ).unwrap_or(0);

        Ok(count > 0)
    } else {
        Err("Database not initialized".to_string())
    }
}
