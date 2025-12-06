use crate::AppState;
use crate::namespaces;

#[tauri::command]
pub fn get_node_icon(state: tauri::State<AppState>, node_id: String) -> Result<Option<String>, String> {
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

        // Return custom icon if found, otherwise default
        Ok(icon.or(default_icon))
    } else {
        Err("Database not initialized".to_string())
    }
}
