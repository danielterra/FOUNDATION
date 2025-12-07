use crate::AppState;
use crate::namespaces;

#[tauri::command]
pub fn node__get_icon(state: tauri::State<AppState>, node_id: String) -> Result<Option<String>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let compressed_node_id = namespaces::compress_iri(&node_id);

        // Default icons for core OWL/RDF/RDFS classes
        let default_icon = match compressed_node_id.as_str() {
            "owl:Thing" => Some("workspaces".to_string()),
            "rdfs:Class" => Some("grid_view".to_string()),
            "owl:Class" => Some("grid_view".to_string()),
            "rdf:Property" => Some("settings_ethernet".to_string()),
            "owl:ObjectProperty" => Some("link".to_string()),
            "owl:DatatypeProperty" => Some("text_fields".to_string()),
            _ => None
        };

        // Query for foundation:icon annotation
        let icon: Option<String> = conn
            .query_row(
                "SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:icon' AND retracted = 0 LIMIT 1",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .ok();

        // If no icon found, check if this is an instance and get icon from its class
        let icon_with_fallback = if icon.is_none() {
            // Check if this node is an instance (has rdf:type pointing to non-Class entities)
            let class_id: Option<String> = conn
                .query_row(
                    "SELECT object FROM triples
                     WHERE subject = ?
                     AND predicate = 'rdf:type'
                     AND object_type = 'iri'
                     AND retracted = 0
                     AND object NOT IN ('owl:Class', 'rdfs:Class')
                     LIMIT 1",
                    [&compressed_node_id],
                    |row| row.get(0),
                )
                .ok();

            // If it's an instance, get the class icon
            if let Some(class_id) = class_id {
                conn.query_row(
                    "SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:icon' AND retracted = 0 LIMIT 1",
                    [&class_id],
                    |row| row.get(0),
                )
                .ok()
            } else {
                None
            }
        } else {
            icon
        };

        // Return custom icon if found, otherwise default
        Ok(icon_with_fallback.or(default_icon))
    } else {
        Err("Database not initialized".to_string())
    }
}
