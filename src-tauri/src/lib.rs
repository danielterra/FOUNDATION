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
    group: i32, // 1=RDF/RDFS/OWL, 2=BFO, 3=Schema.org, 4=FOAF, 5=Bridge
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
    central_node_id: String,
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

        // First, find all equivalents of the central node
        let mut central_ids = vec![central_id.clone()];

        // Query all equivalentClass relationships involving the central node
        let equiv_query = "SELECT e, v FROM facts
                           WHERE a = 'http://www.w3.org/2002/07/owl#equivalentClass'
                           AND (e = ? OR v = ?)
                           AND retracted = 0
                           AND e NOT LIKE '_:%'
                           AND v NOT LIKE '_:%'";

        let equiv_results: Vec<(String, String)> = conn
            .prepare(equiv_query)
            .map_err(|e| format!("Prepare error: {}", e))?
            .query_map([&central_id, &central_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        // Add all equivalent IDs
        for (e, v) in equiv_results {
            if e != central_id && !central_ids.contains(&e) {
                central_ids.push(e);
            }
            if v != central_id && !central_ids.contains(&v) {
                central_ids.push(v);
            }
        }

        println!("[Graph] Central node '{}' has {} equivalents: {:?}",
                 central_id, central_ids.len(), central_ids);

        // Follow the graph correctly: load edges AND nodes, ensuring connectivity
        let mut included_nodes = std::collections::HashSet::new();
        let mut included_edges = std::collections::HashSet::new();

        // Include all central equivalents
        for id in &central_ids {
            included_nodes.insert(id.clone());
        }

        // DOWNWARD (children and grandchildren)
        // 1. Load edges where subClassOf points TO central (child edges from ALL equivalents)
        let mut child_edges: Vec<(String, String)> = Vec::new();
        for central_equiv in &central_ids {
            let edges: Vec<(String, String)> = conn
                .prepare(
                    "SELECT e, v FROM facts
                     WHERE a = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
                     AND v = ?
                     AND retracted = 0
                     AND e NOT LIKE '_:%'"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([central_equiv], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();
            child_edges.extend(edges);
        }

        println!("[Graph] Collected {} child edges for central node", child_edges.len());

        // 2. Load child nodes from these edges
        for (child, parent) in &child_edges {
            included_nodes.insert(child.clone());
            included_edges.insert((child.clone(), parent.clone()));
        }

        // REMOVED: grandchildren loading for simpler 1-level visualization

        // UPWARD (parents only, no grandparents)
        // Load parent edges for all nodes (Thing has no parents in the ontology, so query returns empty)
        let mut parent_edges: Vec<(String, String)> = Vec::new();
        for central_equiv in &central_ids {
            let edges: Vec<(String, String)> = conn
                .prepare(
                    "SELECT e, v FROM facts
                     WHERE e = ?
                     AND a = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
                     AND retracted = 0
                     AND v NOT LIKE '_:%'"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([central_equiv], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();
            parent_edges.extend(edges);
        }

        println!("[Graph] Collected {} parent edges for central node", parent_edges.len());

        // 2. Load parent nodes from these edges
        for (child, parent) in &parent_edges {
            included_nodes.insert(parent.clone());
            included_edges.insert((child.clone(), parent.clone()));
        }

        // REMOVED: grandparents loading for simpler 1-level visualization

        // Build equivalence map from owl:equivalentClass relationships
        let mut equivalence_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        // Query all equivalentClass relationships
        let equiv_relations: Vec<(String, String)> = conn
            .prepare(
                "SELECT e, v FROM facts
                 WHERE a = 'http://www.w3.org/2002/07/owl#equivalentClass'
                 AND retracted = 0
                 AND e NOT LIKE '_:%'
                 AND v NOT LIKE '_:%'"
            )
            .map_err(|e| format!("Prepare error: {}", e))?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        // Build equivalence groups using Union-Find approach
        // First pass: build initial mappings
        for (e, v) in &equiv_relations {
            if !equivalence_map.contains_key(e) {
                equivalence_map.insert(e.clone(), e.clone());
            }
            if !equivalence_map.contains_key(v) {
                equivalence_map.insert(v.clone(), v.clone());
            }
        }

        // Second pass: merge equivalence classes
        for (e, v) in equiv_relations {
            let mut root_e = e.clone();
            while equivalence_map.get(&root_e).unwrap() != &root_e {
                root_e = equivalence_map.get(&root_e).unwrap().clone();
            }

            let mut root_v = v.clone();
            while equivalence_map.get(&root_v).unwrap() != &root_v {
                root_v = equivalence_map.get(&root_v).unwrap().clone();
            }

            if root_e != root_v {
                // Choose canonical: prefer schema:Thing over owl:Thing
                let canonical = if root_e == "http://www.w3.org/2002/07/owl#Thing" {
                    root_v.clone()
                } else if root_v == "http://www.w3.org/2002/07/owl#Thing" {
                    root_e.clone()
                } else if root_e.len() < root_v.len() {
                    root_e.clone()
                } else {
                    root_v.clone()
                };

                // Point both roots to canonical
                equivalence_map.insert(root_e, canonical.clone());
                equivalence_map.insert(root_v, canonical.clone());
            }
        }

        // Third pass: path compression - make all point directly to root
        let keys: Vec<String> = equivalence_map.keys().cloned().collect();
        for key in keys {
            let mut root = key.clone();
            while equivalence_map.get(&root).unwrap() != &root {
                root = equivalence_map.get(&root).unwrap().clone();
            }
            equivalence_map.insert(key, root);
        }

        // Build reverse map: canonical -> list of all equivalent URIs
        let mut equiv_groups: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for (entity, canonical) in &equivalence_map {
            equiv_groups.entry(canonical.clone()).or_insert_with(Vec::new).push(entity.clone());
        }

        // Update included_nodes to use canonical representatives
        let mut canonical_nodes = std::collections::HashSet::new();
        for node in &included_nodes {
            let canonical = equivalence_map.get(node).unwrap_or(node);
            canonical_nodes.insert(canonical.clone());
        }

        // Update included_edges to use canonical representatives
        let mut canonical_edges = std::collections::HashSet::new();
        for (source, target) in &included_edges {
            let canonical_source = equivalence_map.get(source).unwrap_or(source);
            let canonical_target = equivalence_map.get(target).unwrap_or(target);
            // Skip self-loops that might arise from equivalence
            if canonical_source != canonical_target {
                canonical_edges.insert((canonical_source.clone(), canonical_target.clone()));
            }
        }

        // Get only the classes that we included (by querying each one directly)
        let mut nodes = Vec::new();
        let mut node_ids = std::collections::HashSet::new();

        for entity_id in &canonical_nodes {
            // Get all equivalent entities for this canonical
            let default_vec = vec![entity_id.clone()];
            let equivalent_entities = equiv_groups.get(entity_id).map(|v| v.as_slice()).unwrap_or(&default_vec[..]);

            // Query to check if this entity is a class and get its tx (try all equivalents)
            let mut class_tx: Option<i64> = None;
            for equiv_id in equivalent_entities {
                if let Ok(tx) = conn.query_row(
                    "SELECT tx FROM facts
                     WHERE e = ?
                     AND a = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
                     AND (v = 'http://www.w3.org/2002/07/owl#Class' OR v = 'http://www.w3.org/2000/01/rdf-schema#Class')
                     AND retracted = 0
                     LIMIT 1",
                    [equiv_id],
                    |row| row.get(0),
                ) {
                    class_tx = Some(tx);
                    break;
                }
            }

            // Skip if not a class
            let tx = match class_tx {
                Some(t) => t,
                None => continue,
            };

            // Collect labels from all equivalent entities
            let mut labels = Vec::new();
            for equiv_id in equivalent_entities {
                if let Ok(mut label) = conn.query_row(
                    "SELECT v FROM facts WHERE e = ? AND a = 'http://www.w3.org/2000/01/rdf-schema#label' AND retracted = 0 LIMIT 1",
                    [equiv_id],
                    |row| row.get::<_, String>(0),
                ) {
                    // Remove quotes and language tags from labels (e.g., "entity"@en -> entity)
                    if label.starts_with('"') {
                        if let Some(end_quote_idx) = label[1..].find('"') {
                            label = label[1..1+end_quote_idx].to_string();
                        }
                    }
                    labels.push(label);
                } else {
                    // Fallback: extract last part of URI
                    let fallback = equiv_id.split(&['/', '#'][..]).last().unwrap_or(equiv_id).to_string();
                    labels.push(fallback);
                }
            }

            // Deduplicate labels (case-insensitive)
            labels.sort();
            labels.dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());

            // Create combined label
            let label = if labels.len() > 1 {
                labels.join(" ≡ ") // Show equivalence
            } else {
                labels.into_iter().next().unwrap_or_else(|| "Unknown".to_string())
            };

            // Determine group based on transaction ID
            let group = if tx <= 100 {
                1 // RDF/RDFS/OWL
            } else if tx <= 10000 {
                2 // BFO
            } else if tx <= 20000 {
                3 // Schema.org
            } else if tx <= 30000 {
                4 // FOAF
            } else {
                5 // Bridge
            };

            nodes.push(GraphNode {
                id: entity_id.clone(),
                label,
                group,
            });
            node_ids.insert(entity_id);
        }

        // Use ONLY the edges we explicitly loaded (from canonical_edges)
        let mut links = Vec::new();
        for (source, target) in canonical_edges {
            // Double-check both nodes are in our set (should always be true)
            if node_ids.contains(&source) && node_ids.contains(&target) {
                // The predicate is always subClassOf since we only loaded those edges
                links.push(GraphLink {
                    source,
                    target,
                    label: "subClassOf".to_string(),
                });
            }
        }

        // Get the canonical ID that was actually used
        let canonical_central_id = equivalence_map.get(&central_id).unwrap_or(&central_id).clone();

        let graph_data = GraphData {
            nodes,
            links,
            central_node_id: canonical_central_id
        };
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

        // Get all owl:Class and rdfs:Class entities
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT e, tx FROM facts
                 WHERE a = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
                 AND (v = 'http://www.w3.org/2002/07/owl#Class' OR v = 'http://www.w3.org/2000/01/rdf-schema#Class')
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
                } else if tx <= 20000 {
                    3
                } else if tx <= 30000 {
                    4
                } else {
                    5
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
