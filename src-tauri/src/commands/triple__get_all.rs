use crate::{AppState, TripleData};

#[tauri::command]
pub fn get_all_triples(state: tauri::State<AppState>, limit: Option<i64>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let limit_clause = limit.unwrap_or(100);

        let mut stmt = conn
            .prepare(&format!(
                "SELECT subject, predicate, object, object_value, object_type, object_datatype, tx, origin_id, retracted FROM triples ORDER BY tx DESC LIMIT {}",
                limit_clause
            ))
            .map_err(|e| format!("Prepare error: {}", e))?;

        let triples: Result<Vec<TripleData>, _> = stmt
            .query_map([], |row| {
                Ok(TripleData {
                    subject: row.get(0)?,
                    predicate: row.get(1)?,
                    object: row.get(2)?,
                    object_value: row.get(3)?,
                    object_type: row.get(4)?,
                    object_datatype: row.get(5)?,
                    tx: row.get(6)?,
                    origin_id: row.get(7)?,
                    retracted: row.get::<_, i32>(8)? != 0,
                })
            })
            .map_err(|e| format!("Query error: {}", e))?
            .collect();

        let triples = triples.map_err(|e| format!("Row error: {}", e))?;
        Ok(serde_json::to_string(&triples).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}
