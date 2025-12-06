use crate::{AppState, GraphNode, GraphLink, GraphData};
use crate::namespaces;

#[tauri::command]
pub fn get_ontology_graph(state: tauri::State<AppState>, central_node_id: Option<String>) -> Result<String, String> {
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
