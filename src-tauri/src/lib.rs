mod db;
mod ontology;
mod rdf;
mod namespaces;

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

// Triple structure for serialization (maps to triples table)
#[derive(serde::Serialize)]
struct TripleData {
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
#[derive(serde::Serialize)]
struct DisplayTriple {
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
struct GraphNode {
    id: String,
    label: String,
    group: i32, // 1=RDF/RDFS/OWL, 2=BFO, 3=Schema.org, 4=FOAF, 5=Bridge
    icon: Option<String>, // Material Symbols icon name
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
fn get_all_triples(state: tauri::State<AppState>, limit: Option<i64>) -> Result<String, String> {
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

#[tauri::command]
fn get_node_triples(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Compress IRI from frontend to database format
        let compressed_node_id = namespaces::compress_iri(&node_id);

        let mut stmt = conn
            .prepare("SELECT predicate, object, object_value, object_type FROM triples WHERE subject = ? AND retracted = 0 ORDER BY tx")
            .map_err(|e| format!("Prepare error: {}", e))?;

        let display_triples: Result<Vec<DisplayTriple>, _> = stmt
            .query_map([&compressed_node_id], |row| {
                let predicate: String = row.get(0)?;
                let object: Option<String> = row.get(1)?;
                let object_value: Option<String> = row.get(2)?;
                let object_type: String = row.get(3)?;

                // Determine value and value type for display
                let (v, v_type) = if object_type == "iri" {
                    (object.unwrap_or_default(), "ref".to_string())
                } else {
                    (object_value.unwrap_or_default(), "literal".to_string())
                };

                Ok(DisplayTriple {
                    a: predicate,
                    v,
                    v_type,
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

        let mut display_triples = display_triples.map_err(|e| format!("Row error: {}", e))?;

        // Prepare label lookup statement once
        let mut label_stmt = conn
            .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1")
            .map_err(|e| format!("Label query prepare error: {}", e))?;

        // Expand IRIs and fetch labels for references
        for triple in &mut display_triples {
            triple.a = namespaces::expand_iri(&triple.a);
            if triple.v_type == "ref" {
                let compressed_ref = triple.v.clone();
                triple.v = namespaces::expand_iri(&triple.v);

                // Fetch label for the referenced node
                if let Ok(label) = label_stmt.query_row([&compressed_ref], |row| row.get::<_, String>(0)) {
                    triple.v_label = Some(label);
                } else {
                    triple.v_label = None;
                }
            } else {
                triple.v_label = None;
            }
        }

        Ok(serde_json::to_string(&display_triples).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}

#[tauri::command]
fn get_node_backlinks(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
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

#[derive(serde::Serialize)]
struct NodeStatistics {
    children_count: i64,
    backlinks_count: i64,
    synonyms_count: i64,
    related_count: i64,
    examples_count: i64,
}

#[tauri::command]
fn get_node_statistics(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let compressed_node_id = namespaces::compress_iri(&node_id);

        // Count children (things that have this as rdfs:subClassOf or rdfs:subPropertyOf or skos:broader)
        let children_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT subject) FROM triples WHERE object = ? AND predicate IN ('rdfs:subClassOf', 'rdfs:subPropertyOf', 'skos:broader') AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count all backlinks
        let backlinks_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE object = ? AND object_type = 'iri' AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count synonyms (skos:altLabel)
        let synonyms_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = 'skos:altLabel' AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count related concepts (skos:related, FOUNDATION:antonym, etc.)
        let related_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate IN ('skos:related', 'FOUNDATION:antonym', 'rdfs:seeAlso', 'FOUNDATION:causes', 'FOUNDATION:entails') AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count examples (skos:example)
        let examples_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = 'skos:example' AND retracted = 0",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let stats = NodeStatistics {
            children_count,
            backlinks_count,
            synonyms_count,
            related_count,
            examples_count,
        };

        Ok(serde_json::to_string(&stats).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}

#[tauri::command]
fn get_node_icon(state: tauri::State<AppState>, node_id: String) -> Result<Option<String>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let compressed_node_id = namespaces::compress_iri(&node_id);

        // Default icons for core OWL/RDF/RDFS classes
        let default_icon = match compressed_node_id.as_str() {
            "owl:Thing" => Some("workspaces".to_string()),
            "rdfs:Class" => Some("grid_view".to_string()),
            "owl:Class" => Some("grid_view".to_string()),
            "rdf:Property" => Some("settings_ethernet".to_string()),
            "owl:ObjectProperty" => Some("link".to_string()),
            "owl:DatatypeProperty" => Some("text_fields".to_string()),
            _ => None
        };

        // Query for foundation:icon annotation
        let icon: Option<String> = conn
            .query_row(
                "SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:icon' AND retracted = 0 LIMIT 1",
                [&compressed_node_id],
                |row| row.get(0),
            )
            .ok();

        // Return custom icon if found, otherwise default
        Ok(icon.or(default_icon))
    } else {
        Err("Database not initialized".to_string())
    }
}

#[derive(serde::Serialize)]
struct ApplicableProperty {
    property_id: String,
    property_label: String,
    description: Option<String>,
    property_type: String,  // "object" or "datatype"
    range: Option<String>,
    range_label: Option<String>,
    range_icon: Option<String>,  // Icon of the range class
    cardinality: Option<String>,  // "single", "multiple", etc.
    source_class: String,  // Which class defines this property (for grouping)
    source_class_label: String,  // Label of the source class
    source_class_icon: Option<String>,  // Icon of the source class
    is_inherited: bool,  // true if from parent class, false if from own class
}

#[tauri::command]
fn get_applicable_properties(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        let compressed_node_id = namespaces::compress_iri(&node_id);

        // Get all parent classes (rdfs:subClassOf chain) for inheritance
        let mut class_hierarchy = vec![compressed_node_id.clone()];
        let mut visited = std::collections::HashSet::new();
        let mut queue = vec![compressed_node_id.clone()];

        // Build complete class hierarchy (BFS traversal)
        while let Some(current) = queue.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            // Query parent classes
            let parents: Vec<String> = conn
                .prepare(
                    "SELECT object FROM triples
                     WHERE subject = ?
                     AND predicate = 'rdfs:subClassOf'
                     AND object_type = 'iri'
                     AND retracted = 0"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([&current], |row| row.get(0))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();

            for parent in parents {
                if !class_hierarchy.contains(&parent) {
                    class_hierarchy.push(parent.clone());
                }
                if !visited.contains(&parent) {
                    queue.push(parent);
                }
            }
        }

        // Find all properties where rdfs:domain is one of the classes in the hierarchy
        let mut properties = Vec::new();
        let mut seen_properties = std::collections::HashSet::new();

        for class_id in &class_hierarchy {
            let props: Vec<(String, Option<String>)> = conn
                .prepare(
                    "SELECT DISTINCT t1.subject, t2.object
                     FROM triples t1
                     LEFT JOIN triples t2 ON t1.subject = t2.subject
                         AND t2.predicate = 'rdfs:range'
                         AND t2.object_type = 'iri'
                         AND t2.retracted = 0
                     WHERE t1.predicate = 'rdfs:domain'
                     AND t1.object = ?
                     AND t1.object_type = 'iri'
                     AND t1.retracted = 0"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([class_id], |row| Ok((row.get(0)?, row.get(1).ok())))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();

            for (prop_id, range) in props {
                // Avoid duplicates
                if seen_properties.contains(&prop_id) {
                    continue;
                }
                seen_properties.insert(prop_id.clone());

                // Get property label - use object_value for literals
                let prop_label: String = conn
                    .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0")
                    .ok()
                    .and_then(|mut stmt| stmt.query_row([&prop_id], |row| row.get(0)).ok())
                    .unwrap_or_else(|| prop_id.split([':', '#']).last().unwrap_or(&prop_id).to_string());

                // Get property description (rdfs:comment) - use object_value for literals
                let description: Option<String> = conn
                    .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:comment' AND retracted = 0")
                    .ok()
                    .and_then(|mut stmt| stmt.query_row([&prop_id], |row| row.get(0)).ok());

                // Get range label if range exists - use object_value for literals
                let range_label = if let Some(ref r) = range {
                    // Try to get label from database
                    let label_from_db = conn
                        .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0")
                        .ok()
                        .and_then(|mut stmt| stmt.query_row([r], |row| row.get(0)).ok());

                    // If no label found, extract from IRI (e.g., "owl:Thing" -> "Thing")
                    Some(label_from_db.unwrap_or_else(||
                        r.split([':', '#']).last().unwrap_or(r).to_string()
                    ))
                } else {
                    None
                };

                // Determine property type based on range
                let property_type = if let Some(ref r) = range {
                    // Check if it's an XSD datatype or rdfs:Literal
                    if r.starts_with("xsd:") ||
                       r.contains("/XMLSchema#") ||
                       r.contains("Literal") ||
                       r.contains("string") ||
                       r.contains("integer") ||
                       r.contains("boolean") ||
                       r.contains("dateTime") ||
                       r.contains("date") ||
                       r.contains("time") ||
                       r.contains("decimal") ||
                       r.contains("float") ||
                       r.contains("double") ||
                       r.contains("anyURI") ||
                       r.contains("base64") ||
                       r.contains("hexBinary") {
                        "datatype".to_string()
                    } else {
                        "object".to_string()
                    }
                } else {
                    "object".to_string()
                };

                // Check if property is owl:FunctionalProperty (max cardinality = 1)
                let is_functional: bool = conn
                    .prepare("SELECT 1 FROM triples WHERE subject = ? AND predicate = 'rdf:type' AND object = 'owl:FunctionalProperty' AND retracted = 0")
                    .ok()
                    .and_then(|mut stmt| stmt.query_row([&prop_id], |_| Ok(true)).ok())
                    .unwrap_or(false);

                let cardinality: Option<String> = if is_functional {
                    Some("exactly one".to_string())
                } else {
                    Some("one or more".to_string())
                };

                // Get source class label
                let source_class_label: String = conn
                    .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0")
                    .ok()
                    .and_then(|mut stmt| stmt.query_row([class_id], |row| row.get(0)).ok())
                    .unwrap_or_else(|| class_id.split([':', '#']).last().unwrap_or(class_id).to_string());

                // Get source class icon
                let source_class_icon: Option<String> = conn
                    .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:icon' AND retracted = 0")
                    .ok()
                    .and_then(|mut stmt| stmt.query_row([class_id], |row| row.get(0)).ok());

                // Get range icon (if range exists and is an object property)
                let range_icon: Option<String> = if property_type == "object" {
                    range.as_ref().and_then(|r| {
                        conn.prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:icon' AND retracted = 0")
                            .ok()
                            .and_then(|mut stmt| stmt.query_row([r], |row| row.get(0)).ok())
                    })
                } else {
                    None
                };

                properties.push(ApplicableProperty {
                    property_id: namespaces::expand_iri(&prop_id),
                    property_label: prop_label,
                    description,
                    property_type,
                    range: range.map(|r| namespaces::expand_iri(&r)),
                    range_label,
                    range_icon,
                    cardinality,
                    source_class: class_id.clone(),
                    source_class_label,
                    source_class_icon,
                    is_inherited: class_id != &compressed_node_id,
                });
            }
        }

        // Sort properties by label for better UX
        properties.sort_by(|a, b| a.property_label.cmp(&b.property_label));

        Ok(serde_json::to_string(&properties).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}

#[tauri::command]
fn get_ontology_graph(state: tauri::State<AppState>, central_node_id: Option<String>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Use owl:Thing as default central node and compress to prefix
        let central_id = central_node_id
            .map(|id| namespaces::compress_iri(&id))
            .unwrap_or_else(|| "owl:Thing".to_string());

        // First, find all equivalents of the central node
        let mut central_ids = vec![central_id.clone()];

        // Query all equivalentClass relationships involving the central node
        let equiv_query = "SELECT subject, object FROM triples
                           WHERE predicate = 'owl:equivalentClass'
                           AND object_type = 'iri'
                           AND (subject = ? OR object = ?)
                           AND retracted = 0
                           AND subject NOT LIKE '_:%'
                           AND object NOT LIKE '_:%'";

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
                    "SELECT subject, object FROM triples
                     WHERE predicate = 'rdfs:subClassOf'
                     AND object_type = 'iri'
                     AND object = ?
                     AND retracted = 0
                     AND subject NOT LIKE '_:%'"
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
                    "SELECT subject, object FROM triples
                     WHERE subject = ?
                     AND predicate = 'rdfs:subClassOf'
                     AND object_type = 'iri'
                     AND retracted = 0
                     AND object NOT LIKE '_:%'"
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
                "SELECT subject, object FROM triples
                 WHERE predicate = 'owl:equivalentClass'
                 AND object_type = 'iri'
                 AND retracted = 0
                 AND subject NOT LIKE '_:%'
                 AND object NOT LIKE '_:%'"
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
                let canonical = if root_e == "owl:Thing" {
                    root_v.clone()
                } else if root_v == "owl:Thing" {
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
                    "SELECT tx FROM triples
                     WHERE subject = ?
                     AND predicate = 'rdf:type'
                     AND object_type = 'iri'
                     AND (object = 'owl:Class' OR object = 'rdfs:Class')
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
                // Try to get literal label from object_value first, then fall back to object
                if let Ok(label) = conn.query_row(
                    "SELECT COALESCE(object_value, object) FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1",
                    [equiv_id],
                    |row| row.get::<_, String>(0),
                ) {
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

            // Get icon for this node (checking all equivalent entities)
            let mut icon: Option<String> = None;
            for equiv_id in equivalent_entities {
                // Check for foundation:icon
                if let Ok(found_icon) = conn.query_row(
                    "SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:icon' AND retracted = 0 LIMIT 1",
                    [equiv_id],
                    |row| row.get::<_, String>(0),
                ) {
                    icon = Some(found_icon);
                    break;
                }
            }

            // If no icon found, check for default icons for core OWL/RDF/RDFS classes
            if icon.is_none() {
                icon = match entity_id.as_str() {
                    "owl:Thing" => Some("workspaces".to_string()),
                    "rdfs:Class" => Some("grid_view".to_string()),
                    "owl:Class" => Some("grid_view".to_string()),
                    "rdf:Property" => Some("settings_ethernet".to_string()),
                    "owl:ObjectProperty" => Some("link".to_string()),
                    "owl:DatatypeProperty" => Some("text_fields".to_string()),
                    _ => None
                };
            }

            nodes.push(GraphNode {
                id: entity_id.clone(),
                label,
                group,
                icon,
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

        // Add instances of the central class (if it's a class)
        let mut stmt = conn.prepare(
            "SELECT subject FROM triples
             WHERE predicate = 'rdf:type'
             AND object = ?
             AND object_type = 'iri'
             AND retracted = 0
             AND subject NOT LIKE '_:%'
             LIMIT 50"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let instances: Vec<String> = stmt
            .query_map([&canonical_central_id], |row| row.get(0))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        // Get icon of the central class to use for instances
        let class_icon: Option<String> = nodes.iter()
            .find(|n| n.id == canonical_central_id)
            .and_then(|n| n.icon.clone());

        // Add instance nodes and links
        for instance_id in instances {
            // Get label for instance (prefer foundation:name, fallback to rdfs:label, then IRI fragment)
            let label: String = conn
                .query_row(
                    "SELECT COALESCE(object_value, object) FROM triples
                     WHERE subject = ?
                     AND (predicate = 'foundation:name' OR predicate = 'rdfs:label')
                     AND retracted = 0
                     LIMIT 1",
                    [&instance_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| {
                    instance_id.split(&['/', '#', ':'][..]).last().unwrap_or(&instance_id).to_string()
                });

            // Check if instance has a custom icon, otherwise inherit from class
            let instance_icon: Option<String> = conn
                .query_row(
                    "SELECT object_value FROM triples
                     WHERE subject = ?
                     AND predicate = 'foundation:hasIcon'
                     AND retracted = 0
                     LIMIT 1",
                    [&instance_id],
                    |row| row.get(0),
                )
                .ok()
                .or_else(|| class_icon.clone());

            // Add instance node (group 6 = instance)
            nodes.push(GraphNode {
                id: instance_id.clone(),
                label,
                group: 6,
                icon: instance_icon,
            });

            // Add rdf:type link from instance to class
            links.push(GraphLink {
                source: instance_id,
                target: canonical_central_id.clone(),
                label: "type".to_string(),
            });
        }

        // Expand all IRIs back to full form for frontend
        let mut expanded_nodes: Vec<GraphNode> = nodes.into_iter().map(|mut node| {
            node.id = namespaces::expand_iri(&node.id);
            node
        }).collect();

        let expanded_links: Vec<GraphLink> = links.into_iter().map(|mut link| {
            link.source = namespaces::expand_iri(&link.source);
            link.target = namespaces::expand_iri(&link.target);
            link
        }).collect();

        let expanded_central_id = namespaces::expand_iri(&canonical_central_id);

        let graph_data = GraphData {
            nodes: expanded_nodes,
            links: expanded_links,
            central_node_id: expanded_central_id
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
                "SELECT DISTINCT subject, tx FROM triples
                 WHERE predicate = 'rdf:type'
                 AND object_type = 'iri'
                 AND (object = 'owl:Class' OR object = 'rdfs:Class')
                 AND retracted = 0
                 AND subject NOT LIKE '_:%'"
            )
            .map_err(|e| format!("Prepare error: {}", e))?;

        let class_entities: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(Result::ok)
            .collect();

        let mut results = Vec::new();

        for (entity_id, tx) in class_entities {
            // Get label (prefer object_value for literals, fallback to object for IRIs)
            let clean_label: String = conn
                .query_row(
                    "SELECT COALESCE(object_value, object) FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1",
                    [&entity_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| {
                    entity_id.split(&['/', '#', ':'][..]).last().unwrap_or(&entity_id).to_string()
                });

            // Get definition (check multiple definition properties, prefer object_value for literals)
            let definition: Option<String> = conn
                .query_row(
                    "SELECT COALESCE(object_value, object) FROM triples
                     WHERE subject = ?
                     AND predicate IN ('skos:definition', 'rdfs:comment')
                     AND retracted = 0
                     LIMIT 1",
                    [&entity_id],
                    |row| row.get(0),
                )
                .ok();

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

        // Expand IRIs back to full form for frontend
        let expanded_results: Vec<SearchResult> = results.into_iter().map(|mut result| {
            result.id = namespaces::expand_iri(&result.id);
            result
        }).collect();

        Ok(serde_json::to_string(&expanded_results).map_err(|e| format!("Serialization error: {}", e))?)
    } else {
        Err("Database not initialized".to_string())
    }
}

// Setup wizard data structures
#[derive(serde::Serialize, serde::Deserialize)]
struct SetupData {
    person_name: String,
    person_email: Option<String>,
    computer_name: String,
    computer_processor: Option<String>,
    computer_memory: Option<i32>,
}

#[derive(serde::Serialize)]
struct SystemInfo {
    hostname: String,
    os_name: String,
    os_version: String,
    cpu_brand: String,
    cpu_cores: usize,
    total_memory_gb: f64,
}

// Check if initial setup has been completed
#[tauri::command]
fn check_initial_setup(state: tauri::State<AppState>) -> Result<bool, String> {
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

// Get system information automatically
#[tauri::command]
fn get_system_info() -> Result<SystemInfo, String> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());
    let os_name = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());

    let cpu_brand = sys.cpus().first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let cpu_cores = sys.cpus().len();
    let total_memory_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

    Ok(SystemInfo {
        hostname,
        os_name,
        os_version,
        cpu_brand,
        cpu_cores,
        total_memory_gb,
    })
}

// Complete initial setup by creating Person, Computer, and SoftwareAgent instances
#[tauri::command]
fn complete_initial_setup(state: tauri::State<AppState>, setup_data: String) -> Result<String, String> {
    let data: SetupData = serde_json::from_str(&setup_data)
        .map_err(|e| format!("Parse error: {}", e))?;

    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Origin IDs
        let user_origin_id: i64 = 2;  // foundation:CurrentUser
        let foundation_origin_id: i64 = 3;  // foundation:FOUNDATION

        // Get current timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Get next transaction ID
        let tx: i64 = conn.query_row(
            "SELECT COALESCE(MAX(tx), 0) + 1 FROM triples",
            [],
            |row| row.get(0)
        ).map_err(|e| format!("Query error: {}", e))?;

        // Create Person instance (CurrentUser)
        let person_id = "foundation:CurrentUser";

        // Person rdf:type
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted)
             VALUES (?, 'rdf:type', 'foundation:Person', 'iri', ?, ?, ?, 0)",
            (person_id, tx, user_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Person name
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
             VALUES (?, 'foundation:name', ?, 'literal', 'xsd:string', ?, ?, ?, 0)",
            (person_id, &data.person_name, tx + 1, user_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Person email (if provided)
        if let Some(ref email) = data.person_email {
            if !email.is_empty() {
                conn.execute(
                    "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
                     VALUES (?, 'foundation:email', ?, 'literal', 'xsd:string', ?, ?, ?, 0)",
                    (person_id, email, tx + 2, user_origin_id, now)
                ).map_err(|e| format!("Insert error: {}", e))?;
            }
        }

        // Create Computer instance
        let computer_id = "foundation:CurrentComputer";

        // Computer rdf:type
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted)
             VALUES (?, 'rdf:type', 'foundation:Computer', 'iri', ?, ?, ?, 0)",
            (computer_id, tx + 3, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Computer name
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
             VALUES (?, 'foundation:name', ?, 'literal', 'xsd:string', ?, ?, ?, 0)",
            (computer_id, &data.computer_name, tx + 4, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Computer processor (if provided)
        if let Some(ref processor) = data.computer_processor {
            if !processor.is_empty() {
                conn.execute(
                    "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
                     VALUES (?, 'foundation:processor', ?, 'literal', 'xsd:string', ?, ?, ?, 0)",
                    (computer_id, processor, tx + 5, foundation_origin_id, now)
                ).map_err(|e| format!("Insert error: {}", e))?;
            }
        }

        // Computer memory (if provided)
        if let Some(memory) = data.computer_memory {
            conn.execute(
                "INSERT INTO triples (subject, predicate, object_integer, object_type, object_datatype, tx, origin_id, created_at, retracted)
                 VALUES (?, 'foundation:memory', ?, 'literal', 'xsd:integer', ?, ?, ?, 0)",
                (computer_id, memory as i64, tx + 6, foundation_origin_id, now)
            ).map_err(|e| format!("Insert error: {}", e))?;
        }

        // Create SoftwareAgent instance (FOUNDATION app)
        let software_id = "foundation:FOUNDATIONApp";

        // Software rdf:type
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted)
             VALUES (?, 'rdf:type', 'foundation:SoftwareAgent', 'iri', ?, ?, ?, 0)",
            (software_id, tx + 7, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Software name
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
             VALUES (?, 'foundation:name', 'FOUNDATION', 'literal', 'xsd:string', ?, ?, ?, 0)",
            (software_id, tx + 8, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Software version
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
             VALUES (?, 'foundation:version', '0.1.0', 'literal', 'xsd:string', ?, ?, ?, 0)",
            (software_id, tx + 9, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        Ok("Setup completed successfully".to_string())
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

            // Print database stats
            if let Ok(stats) = db::get_stats(&conn) {
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
        .manage(AppState {
            db: Mutex::new(db_conn),
        })
        .invoke_handler(tauri::generate_handler![greet, get_db_stats, get_all_triples, get_node_triples, get_node_backlinks, get_node_statistics, get_node_icon, get_applicable_properties, get_ontology_graph, search_classes, check_initial_setup, get_system_info, complete_initial_setup])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
