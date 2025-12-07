mod turtle;
mod namespaces;
mod commands;
mod eavto;
mod owl;

use std::sync::Mutex;
use rusqlite::Connection;

// Triple structure for serialization (maps to triples table)
#[derive(serde::Serialize)]
pub struct TripleData {
    subject: String,
    predicate: String,
    object: Option<String>,           // IRI or blank node (if object_type = 'iri' or 'blank')
    object_value: Option<String>,      // Literal value (if object_type = 'literal')
    object_type: String,               // 'iri', 'literal', or 'blank'
    object_datatype: Option<String>,   // Datatype IRI for literals
    tx: i64,
    origin_id: i64,
    retracted: bool,
}

// Simplified triple structure for UI display
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DisplayTriple {
    a: String,            // attribute (predicate)
    v: String,            // value (object or object_value)
    v_type: String,       // value type: 'ref' for IRIs, 'literal' for literals
    v_label: Option<String>, // optional label for the value (when v_type is 'ref')
    v_icon: Option<String>, // optional icon for the value (when v_type is 'ref')
    a_comment: Option<String>, // optional comment for the predicate
    domain: Option<String>, // optional domain class IRI for the predicate
    domain_label: Option<String>, // optional label for the domain class
    domain_icon: Option<String>, // optional icon for the domain class
}

// Graph structures for ontology visualization
#[derive(serde::Serialize)]
pub struct GraphNode {
    id: String,
    label: String,
    group: i32, // 1=Class, 6=Instance
    icon: Option<String>, // Material Symbols icon name
}

#[derive(serde::Serialize)]
pub struct GraphLink {
    source: String,
    target: String,
    label: String,
}

#[derive(serde::Serialize)]
pub struct GraphData {
    nodes: Vec<GraphNode>,
    links: Vec<GraphLink>,
    central_node_id: String,
}

// Search result structure
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    id: String,
    label: String,
    definition: Option<String>,
    group: i32,
    score: f32,
}

#[derive(serde::Serialize)]
pub struct NodeStatistics {
    children_count: i64,
    backlinks_count: i64,
    synonyms_count: i64,
    related_count: i64,
    examples_count: i64,
}

// Setup wizard data structures
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SetupData {
    person_name: String,
    person_email: Option<String>,
    computer_name: String,
    computer_processor: Option<String>,
    computer_memory: Option<i32>,
}

#[derive(serde::Serialize)]
pub struct SystemInfo {
    hostname: String,
    os_name: String,
    os_version: String,
    cpu_brand: String,
    cpu_cores: usize,
    total_memory_gb: f64,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize database on startup
    let db_conn = match eavto::get_connection() {
        Ok(conn) => {
            println!("Database initialized successfully");

            // Print database stats
            if let Ok(stats) = eavto::get_stats(&conn) {
                println!("Database stats:");
                println!("  Total triples: {}", stats.total_facts);
                println!("  Active triples: {}", stats.active_facts);
                println!("  Transactions: {}", stats.total_transactions);
                println!("  Entities: {}", stats.entities_count);
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
        .manage(Mutex::new(db_conn.expect("Database connection required")))
        .invoke_handler(tauri::generate_handler![
            commands::setup__check,
            commands::setup__init,
            commands::entity__get
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
