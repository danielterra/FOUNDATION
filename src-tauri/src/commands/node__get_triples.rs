use crate::{AppState, DisplayTriple};
use crate::namespaces;

#[tauri::command]
pub fn get_node_triples(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Compress IRI from frontend to database format
        let compressed_node_id = namespaces::compress_iri(&node_id);

        let mut stmt = conn
            .prepare("SELECT predicate, object, object_value, object_type FROM triples WHERE subject = ? AND retracted = 0 ORDER BY tx")
            .map_err(|e| format!("Prepare error: {}", e))?;

        let display_triples: Result<Vec<DisplayTriple>, _> = stmt
            .query_map([&compressed_node_id], |row| {
                let predicate: String = row.get(0)?;
                let object: Option<String> = row.get(1)?;
                let object_value: Option<String> = row.get(2)?;
                let object_type: String = row.get(3)?;

                // Determine value and value type for display
                let (v, v_type) = if object_type == "iri" {
                    (object.unwrap_or_default(), "ref".to_string())
                } else {
                    (object_value.unwrap_or_default(), "literal".to_string())
                };

                Ok(DisplayTriple {
                    a: predicate,
                    v,
                    v_type,
                    v_label: None,
                    v_icon: None,
                    a_comment: None,
                    domain: None,
                    domain_label: None,
                    domain_icon: None,
                })
            })
            .map_err(|e| format!("Query error: {}", e))?
            .collect();

        let mut display_triples = display_triples.map_err(|e| format!("Row error: {}", e))?;

        // Prepare label lookup statement once
        let mut label_stmt = conn
            .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1")
            .map_err(|e| format!("Label query prepare error: {}", e))?;

        // Expand IRIs and fetch labels for references
        for triple in &mut display_triples {
            triple.a = namespaces::expand_iri(&triple.a);
            if triple.v_type == "ref" {
                let compressed_ref = triple.v.clone();
                triple.v = namespaces::expand_iri(&triple.v);

                // Fetch label for the referenced node
                if let Ok(label) = label_stmt.query_row([&compressed_ref], |row| row.get::<_, String>(0)) {
                    triple.v_label = Some(label);
                } else {
                    triple.v_label = None;
                }
            } else {
                triple.v_label = None;
            }
        }

        Ok(serde_json::to_string(&display_triples).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}
