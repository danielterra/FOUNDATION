use crate::{AppState, DisplayTriple};
use crate::namespaces;

#[tauri::command]
pub fn get_node_backlinks(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Compress IRI from frontend (full URI -> prefix)
        let compressed_node_id = namespaces::compress_iri(&node_id);

        // Query for triples where this node is the OBJECT (incoming relations)
        let mut stmt = conn
            .prepare("SELECT subject, predicate FROM triples WHERE object = ? AND object_type = 'iri' AND retracted = 0 ORDER BY predicate")
            .map_err(|e| format!("Prepare error: {}", e))?;

        let backlinks: Result<Vec<DisplayTriple>, _> = stmt
            .query_map([&compressed_node_id], |row| {
                let subject: String = row.get(0)?;
                let predicate: String = row.get(1)?;

                Ok(DisplayTriple {
                    a: predicate,
                    v: subject,
                    v_type: "ref".to_string(),
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

        let mut backlinks = backlinks.map_err(|e| format!("Row error: {}", e))?;

        // Prepare label, icon and comment lookup statements once
        let mut label_stmt = conn
            .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1")
            .map_err(|e| format!("Label query prepare error: {}", e))?;

        let mut icon_stmt = conn
            .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:icon' AND retracted = 0 LIMIT 1")
            .map_err(|e| format!("Icon query prepare error: {}", e))?;

        let mut comment_stmt = conn
            .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:comment' AND retracted = 0 LIMIT 1")
            .map_err(|e| format!("Comment query prepare error: {}", e))?;

        let mut domain_stmt = conn
            .prepare("SELECT object FROM triples WHERE subject = ? AND predicate = 'rdfs:domain' AND retracted = 0 LIMIT 1")
            .map_err(|e| format!("Domain query prepare error: {}", e))?;

        // Expand IRIs and fetch labels + icons + comments for references
        for triple in &mut backlinks {
            let compressed_predicate = triple.a.clone();
            triple.a = namespaces::expand_iri(&triple.a);
            let compressed_ref = triple.v.clone();
            triple.v = namespaces::expand_iri(&triple.v);

            // Fetch label for the referenced node (property)
            if let Ok(label) = label_stmt.query_row([&compressed_ref], |row| row.get::<_, String>(0)) {
                triple.v_label = Some(label);
            } else {
                triple.v_label = None;
            }

            // Fetch icon for the referenced node (property)
            if let Ok(icon) = icon_stmt.query_row([&compressed_ref], |row| row.get::<_, String>(0)) {
                triple.v_icon = Some(icon);
            } else {
                triple.v_icon = None;
            }

            // Fetch comment for the property (v, not the predicate a)
            if let Ok(comment) = comment_stmt.query_row([&compressed_ref], |row| row.get::<_, String>(0)) {
                triple.a_comment = Some(comment);
            } else {
                triple.a_comment = None;
            }

            // Fetch domain for the property (the class that owns this property)
            if let Ok(domain_compressed) = domain_stmt.query_row([&compressed_ref], |row| row.get::<_, String>(0)) {
                let domain_expanded = namespaces::expand_iri(&domain_compressed);
                triple.domain = Some(domain_expanded);

                // Fetch label for the domain class
                if let Ok(domain_label) = label_stmt.query_row([&domain_compressed], |row| row.get::<_, String>(0)) {
                    triple.domain_label = Some(domain_label);
                }

                // Fetch icon for the domain class
                if let Ok(domain_icon) = icon_stmt.query_row([&domain_compressed], |row| row.get::<_, String>(0)) {
                    triple.domain_icon = Some(domain_icon);
                }
            }
        }

        Ok(serde_json::to_string(&backlinks).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}
