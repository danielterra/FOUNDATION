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
        // Compress IRI from frontend (full URI -> prefix)
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
                })
            })
            .map_err(|e| format!("Query error: {}", e))?
            .collect();

        let mut backlinks = backlinks.map_err(|e| format!("Row error: {}", e))?;

        // Prepare label lookup statement once
        let mut label_stmt = conn
            .prepare("SELECT object_value FROM triples WHERE subject = ? AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1")
            .map_err(|e| format!("Label query prepare error: {}", e))?;

        // Expand IRIs and fetch labels for references
        for triple in &mut backlinks {
            triple.a = namespaces::expand_iri(&triple.a);
            let compressed_ref = triple.v.clone();
            triple.v = namespaces::expand_iri(&triple.v);

            // Fetch label for the referenced node
            if let Ok(label) = label_stmt.query_row([&compressed_ref], |row| row.get::<_, String>(0)) {
                triple.v_label = Some(label);
            } else {
                triple.v_label = None;
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

        // Count related concepts (skos:related, supernova:antonym, etc.)
        let related_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate IN ('skos:related', 'supernova:antonym', 'rdfs:seeAlso', 'supernova:causes', 'supernova:entails') AND retracted = 0",
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

#[derive(serde::Serialize)]
struct ApplicableProperty {
    property_id: String,
    property_label: String,
    property_type: String,
    range: Option<String>,
    range_label: Option<String>,
}

#[tauri::command]
fn get_applicable_properties(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref _conn) = *db {
        let _compressed_node_id = namespaces::compress_iri(&node_id);

        // TODO: Implement property discovery based on rdfs:domain and rdfs:range
        // For now, return empty array since WordNet vocabulary doesn't define property domains/ranges
        // This will be implemented when we import ontologies that define properties with domain/range constraints

        let properties: Vec<ApplicableProperty> = vec![];

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
                     AND predicate IN (
                         'skos:definition',
                         'rdfs:comment'
                     )
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
        .invoke_handler(tauri::generate_handler![greet, get_db_stats, get_all_triples, get_node_triples, get_node_backlinks, get_node_statistics, get_applicable_properties, get_ontology_graph, search_classes])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
