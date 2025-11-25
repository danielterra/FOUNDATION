mod db;
mod ontology;

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

// Fact structure for serialization
#[derive(serde::Serialize)]
struct Fact {
    e: String,
    a: String,
    v: String,
    tx: i64,
    origin: String,
    retracted: bool,
    v_type: String,
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

#[tauri::command]
fn get_all_facts(state: tauri::State<AppState>, limit: Option<i64>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let limit_clause = limit.unwrap_or(100);

        let mut stmt = conn
            .prepare(&format!(
                "SELECT e, a, v, tx, origin, retracted, v_type FROM facts ORDER BY tx DESC LIMIT {}",
                limit_clause
            ))
            .map_err(|e| format!("Prepare error: {}", e))?;

        let facts: Result<Vec<Fact>, _> = stmt
            .query_map([], |row| {
                Ok(Fact {
                    e: row.get(0)?,
                    a: row.get(1)?,
                    v: row.get(2)?,
                    tx: row.get(3)?,
                    origin: row.get(4)?,
                    retracted: row.get::<_, i32>(5)? != 0,
                    v_type: row.get(6)?,
                })
            })
            .map_err(|e| format!("Query error: {}", e))?
            .collect();

        let facts = facts.map_err(|e| format!("Row error: {}", e))?;
        Ok(serde_json::to_string(&facts).map_err(|e| format!("Serialization error: {}", e))?)
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

            // Print stats before import
            if let Ok(stats) = db::get_stats(&conn) {
                println!("Database stats:");
                println!("  Total facts: {}", stats.total_facts);
                println!("  Active facts: {}", stats.active_facts);
                println!("  Transactions: {}", stats.total_transactions);
                println!("  Entities: {}", stats.entities_count);
                println!("  Ontology imported: {}", stats.ontology_imported);

                // Import core ontologies if not already imported
                if !stats.ontology_imported {
                    println!("\n🚀 Importing core ontologies...");
                    match ontology::import_all_core_ontologies(&conn) {
                        Ok(import_stats) => {
                            println!("\n✅ Ontology import successful!");
                            for stat in import_stats {
                                println!("  - {}: {} facts", stat.file, stat.facts_inserted);
                            }

                            // Print updated stats
                            if let Ok(new_stats) = db::get_stats(&conn) {
                                println!("\n📊 Updated database stats:");
                                println!("  Total facts: {}", new_stats.total_facts);
                                println!("  Active facts: {}", new_stats.active_facts);
                                println!("  Entities: {}", new_stats.entities_count);
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to import ontologies: {:?}", e);
                        }
                    }
                }
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
        .invoke_handler(tauri::generate_handler![greet, get_db_stats, get_all_facts])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
