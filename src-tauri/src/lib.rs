mod db;

use std::sync::Mutex;
use rusqlite::Connection;

// Global database connection (managed by Tauri state)
pub struct AppState {
    pub db: Mutex<Option<Connection>>,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// Database commands
#[tauri::command]
fn get_db_stats(state: tauri::State<AppState>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let stats = db::get_stats(conn).map_err(|e| format!("Database error: {:?}", e))?;
        Ok(serde_json::to_string(&stats).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize database on startup
    let db_conn = match db::get_connection() {
        Ok(conn) => {
            println!("Database initialized successfully");

            // Print stats
            if let Ok(stats) = db::get_stats(&conn) {
                println!("Database stats:");
                println!("  Total facts: {}", stats.total_facts);
                println!("  Active facts: {}", stats.active_facts);
                println!("  Transactions: {}", stats.total_transactions);
                println!("  Entities: {}", stats.entities_count);
                println!("  Ontology imported: {}", stats.ontology_imported);
            }

            Some(conn)
        }
        Err(e) => {
            eprintln!("Failed to initialize database: {:?}", e);
            None
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            db: Mutex::new(db_conn),
        })
        .invoke_handler(tauri::generate_handler![greet, get_db_stats])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
