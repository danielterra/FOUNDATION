use crate::AppState;

#[tauri::command]
pub fn check_initial_setup(state: tauri::State<AppState>) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Check if there are any Person instances
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples
             WHERE predicate = 'rdf:type'
             AND object = 'foundation:Person'
             AND retracted = 0",
            [],
            |row| row.get(0)
        ).unwrap_or(0);

        Ok(count > 0)
    } else {
        Err("Database not initialized".to_string())
    }
}
