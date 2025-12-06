use crate::AppState;
use crate::db;

#[tauri::command]
pub fn get_db_stats(state: tauri::State<AppState>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let stats = db::get_stats(conn).map_err(|e| format!("Database error: {:?}", e))?;
        Ok(serde_json::to_string(&stats).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}
