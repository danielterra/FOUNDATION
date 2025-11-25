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

// Graph structures for ontology visualization
#[derive(serde::Serialize)]
struct GraphNode {
    id: String,
    label: String,
    group: i32, // 1=RDF/RDFS/OWL, 2=BFO, 3=CCO
}

#[derive(serde::Serialize)]
struct GraphLink {
    source: String,
    target: String,
    label: String,
}

#[derive(serde::Serialize)]
struct GraphData {
    nodes: Vec<GraphNode>,
    links: Vec<GraphLink>,
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

#[tauri::command]
fn get_ontology_graph(state: tauri::State<AppState>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Get all classes (entities that are of type Class)
        let mut nodes = Vec::new();
        let mut node_ids = std::collections::HashSet::new();

        // Query for all owl:Class and rdfs:Class instances
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT e, tx FROM facts
                 WHERE a IN (
                     'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
                 ) AND v IN (
                     'http://www.w3.org/2002/07/owl#Class',
                     'http://www.w3.org/2000/01/rdf-schema#Class'
                 ) AND retracted = 0
                 ORDER BY tx"
            )
            .map_err(|e| format!("Prepare error: {}", e))?;

        let class_entities: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        // Create nodes for each class
        for (entity_id, tx) in class_entities {
            // Get label if available
            let label: String = conn
                .query_row(
                    "SELECT v FROM facts WHERE e = ? AND a = 'http://www.w3.org/2000/01/rdf-schema#label' AND retracted = 0 LIMIT 1",
                    [&entity_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| {
                    // Extract last part of URI as fallback label
                    entity_id.split(&['/', '#'][..]).last().unwrap_or(&entity_id).to_string()
                });

            // Determine group based on transaction ID
            let group = if tx <= 100 {
                1 // RDF/RDFS/OWL
            } else if tx <= 10000 {
                2 // BFO
            } else {
                3 // CCO
            };

            nodes.push(GraphNode {
                id: entity_id.clone(),
                label,
                group,
            });
            node_ids.insert(entity_id);
        }

        // Get all relationships (subClassOf, domain, range)
        let mut links = Vec::new();
        let mut stmt = conn
            .prepare(
                "SELECT e, a, v FROM facts
                 WHERE a IN (
                     'http://www.w3.org/2000/01/rdf-schema#subClassOf',
                     'http://www.w3.org/2000/01/rdf-schema#domain',
                     'http://www.w3.org/2000/01/rdf-schema#range'
                 ) AND retracted = 0"
            )
            .map_err(|e| format!("Prepare error: {}", e))?;

        let relationships: Vec<(String, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        for (source, predicate, target) in relationships {
            // Only include links between nodes we have
            if node_ids.contains(&source) && node_ids.contains(&target) {
                let label = predicate.split(&['/', '#'][..]).last().unwrap_or("relates").to_string();
                links.push(GraphLink {
                    source,
                    target,
                    label,
                });
            }
        }

        let graph_data = GraphData { nodes, links };
        Ok(serde_json::to_string(&graph_data).map_err(|e| format!("Serialization error: {}", e))?)
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
        .invoke_handler(tauri::generate_handler![greet, get_db_stats, get_all_facts, get_ontology_graph])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
