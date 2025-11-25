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

// Search result structure
#[derive(serde::Serialize)]
struct SearchResult {
    id: String,
    label: String,
    definition: Option<String>,
    group: i32,
    score: f32,
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
fn get_node_facts(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let mut stmt = conn
            .prepare("SELECT e, a, v, tx, origin, retracted, v_type FROM facts WHERE e = ? AND retracted = 0 ORDER BY tx")
            .map_err(|e| format!("Prepare error: {}", e))?;

        let facts: Result<Vec<Fact>, _> = stmt
            .query_map([&node_id], |row| {
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
fn get_ontology_graph(state: tauri::State<AppState>, central_node_id: Option<String>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Use owl:Thing as default central node
        let central_id = central_node_id.unwrap_or_else(|| "http://www.w3.org/2002/07/owl#Thing".to_string());

        // Build set of nodes to include (central + parents + siblings + 2 levels of children)
        let mut included_nodes = std::collections::HashSet::new();
        included_nodes.insert(central_id.clone());

        // Add parent (superclass) of central node
        let parent: Option<String> = conn
            .query_row(
                "SELECT v FROM facts
                 WHERE e = ?
                 AND a = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
                 AND retracted = 0
                 AND v NOT LIKE '_:%'
                 LIMIT 1",
                [&central_id],
                |row| row.get(0),
            )
            .ok();

        if let Some(parent_id) = parent {
            included_nodes.insert(parent_id.clone());

            // Add siblings (other children of parent)
            let siblings: Vec<String> = conn
                .prepare(
                    "SELECT e FROM facts
                     WHERE a = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
                     AND v = ?
                     AND retracted = 0
                     AND e NOT LIKE '_:%'"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([&parent_id], |row| row.get(0))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();

            for sibling in siblings {
                included_nodes.insert(sibling);
            }
        }

        // Level 1: Direct children (subclasses of central node), excluding blank nodes
        let level1_children: Vec<String> = conn
            .prepare(
                "SELECT e FROM facts
                 WHERE a = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
                 AND v = ?
                 AND retracted = 0
                 AND e NOT LIKE '_:%'"
            )
            .map_err(|e| format!("Prepare error: {}", e))?
            .query_map([&central_id], |row| row.get(0))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        for child in &level1_children {
            included_nodes.insert(child.clone());
        }

        // Level 2: Children of level 1 children, excluding blank nodes
        for level1_child in &level1_children {
            let level2_children: Vec<String> = conn
                .prepare(
                    "SELECT e FROM facts
                     WHERE a = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
                     AND v = ?
                     AND retracted = 0
                     AND e NOT LIKE '_:%'"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([level1_child], |row| row.get(0))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();

            for child in level2_children {
                included_nodes.insert(child);
            }
        }

        // Get all owl:Class instances that are in our included set
        let mut nodes = Vec::new();
        let mut node_ids = std::collections::HashSet::new();

        let mut stmt = conn
            .prepare(
                "SELECT e, tx FROM facts
                 WHERE a = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
                 AND v = 'http://www.w3.org/2002/07/owl#Class'
                 AND retracted = 0
                 AND e NOT LIKE '_:%'
                 ORDER BY tx"
            )
            .map_err(|e| format!("Prepare error: {}", e))?;

        let class_entities: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .filter(|(entity_id, _)| included_nodes.contains(entity_id))
            .collect();

        // Create nodes for each class
        for (entity_id, tx) in class_entities {
            // Get label if available
            let mut label: String = conn
                .query_row(
                    "SELECT v FROM facts WHERE e = ? AND a = 'http://www.w3.org/2000/01/rdf-schema#label' AND retracted = 0 LIMIT 1",
                    [&entity_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| {
                    // Extract last part of URI as fallback label
                    entity_id.split(&['/', '#'][..]).last().unwrap_or(&entity_id).to_string()
                });

            // Remove quotes and language tags from labels (e.g., "entity"@en -> entity)
            if label.starts_with('"') {
                if let Some(end_quote_idx) = label[1..].find('"') {
                    // Extract string between quotes
                    label = label[1..1+end_quote_idx].to_string();
                }
            }

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

        // Get all relationships where value is a reference (link to another entity)
        let mut links = Vec::new();
        let mut stmt = conn
            .prepare(
                "SELECT e, a, v FROM facts
                 WHERE retracted = 0
                 AND v_type = 'ref'"
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

#[tauri::command]
fn search_classes(state: tauri::State<AppState>, query: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        if query.trim().is_empty() {
            return Ok(serde_json::to_string(&Vec::<SearchResult>::new()).unwrap());
        }

        // Build search pattern for LIKE queries
        let search_pattern = format!("%{}%", query.to_lowercase());

        // Get all owl:Class entities
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT e, tx FROM facts
                 WHERE a = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
                 AND v = 'http://www.w3.org/2002/07/owl#Class'
                 AND retracted = 0
                 AND e NOT LIKE '_:%'"
            )
            .map_err(|e| format!("Prepare error: {}", e))?;

        let class_entities: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        let mut results = Vec::new();

        for (entity_id, tx) in class_entities {
            // Get label
            let label: String = conn
                .query_row(
                    "SELECT v FROM facts WHERE e = ? AND a = 'http://www.w3.org/2000/01/rdf-schema#label' AND retracted = 0 LIMIT 1",
                    [&entity_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| {
                    entity_id.split(&['/', '#'][..]).last().unwrap_or(&entity_id).to_string()
                });

            // Clean label (remove quotes and language tags)
            let clean_label = if label.starts_with('"') {
                if let Some(end_quote_idx) = label[1..].find('"') {
                    label[1..1+end_quote_idx].to_string()
                } else {
                    label.clone()
                }
            } else {
                label.clone()
            };

            // Get definition (check multiple definition properties)
            let definition: Option<String> = conn
                .query_row(
                    "SELECT v FROM facts
                     WHERE e = ?
                     AND a IN (
                         'http://www.w3.org/2004/02/skos/core#definition',
                         'http://purl.obolibrary.org/obo/IAO_0000115',
                         'http://www.w3.org/2000/01/rdf-schema#comment'
                     )
                     AND retracted = 0
                     LIMIT 1",
                    [&entity_id],
                    |row| row.get(0),
                )
                .ok()
                .map(|def: String| {
                    // Clean definition
                    if def.starts_with('"') {
                        if let Some(end_quote_idx) = def[1..].find('"') {
                            def[1..1+end_quote_idx].to_string()
                        } else {
                            def
                        }
                    } else {
                        def
                    }
                });

            // Calculate relevance score with weighted matches
            let query_lower = query.to_lowercase();
            let label_lower = clean_label.to_lowercase();

            let mut score = 0.0f32;

            // Exact match in label (highest priority)
            if label_lower == query_lower {
                score += 100.0;
            }
            // Label starts with query (very high priority)
            else if label_lower.starts_with(&query_lower) {
                score += 50.0;
            }
            // Label contains query (high priority)
            else if label_lower.contains(&query_lower) {
                score += 30.0;
            }

            // Definition match (medium priority)
            if let Some(def) = &definition {
                if def.to_lowercase().contains(&query_lower) {
                    score += 10.0;
                }
            }

            // ID match (low priority)
            if entity_id.to_lowercase().contains(&query_lower) {
                score += 5.0;
            }

            // Only include if there's a match
            if score > 0.0 {
                let group = if tx <= 100 {
                    1
                } else if tx <= 10000 {
                    2
                } else {
                    3
                };

                results.push(SearchResult {
                    id: entity_id,
                    label: clean_label,
                    definition,
                    group,
                    score,
                });
            }
        }

        // Sort by score (descending) and limit to top 5
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(5);

        Ok(serde_json::to_string(&results).map_err(|e| format!("Serialization error: {}", e))?)
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
        .invoke_handler(tauri::generate_handler![greet, get_db_stats, get_all_facts, get_node_facts, get_ontology_graph, search_classes])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
