use crate::{AppState, GraphNode, GraphLink, GraphData};
use crate::namespaces;

#[tauri::command]
pub fn get_ontology_graph(state: tauri::State<AppState>, central_node_id: Option<String>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Use CurrentUser as default central node and compress to prefix
        let central_id = central_node_id
            .map(|id| namespaces::compress_iri(&id))
            .unwrap_or_else(|| "foundation:CurrentUser".to_string());

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
        // Store edge predicates for labeling: (source, target) -> predicate
        let mut edge_predicates: std::collections::HashMap<(String, String), String> = std::collections::HashMap::new();

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
            edge_predicates.insert((child.clone(), parent.clone()), "rdfs:subClassOf".to_string());
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
            edge_predicates.insert((child.clone(), parent.clone()), "rdfs:subClassOf".to_string());
        }

        // REMOVED: grandparents loading for simpler 1-level visualization

        // Check if central node is an instance (do this BEFORE equivalence mapping)
        let central_is_instance = conn.query_row(
            "SELECT COUNT(*) FROM triples
             WHERE subject = ?
             AND predicate = 'rdf:type'
             AND object_type = 'iri'
             AND retracted = 0
             AND object NOT IN ('owl:Class', 'rdfs:Class')",
            [&central_id],
            |row| row.get::<_, i64>(0)
        ).unwrap_or(0) > 0;

        if central_is_instance {
            // Load the classes that this instance belongs to
            let instance_classes: Vec<String> = conn
                .prepare(
                    "SELECT object FROM triples
                     WHERE subject = ?
                     AND predicate = 'rdf:type'
                     AND object_type = 'iri'
                     AND retracted = 0
                     AND object NOT IN ('owl:Class', 'rdfs:Class')"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([&central_id], |row| row.get(0))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();

            // Add these classes to included_nodes (only direct classes, not their parents)
            for class_id in &instance_classes {
                included_nodes.insert(class_id.clone());
                // Add rdf:type edge from instance to class
                included_edges.insert((central_id.clone(), class_id.clone()));
                edge_predicates.insert((central_id.clone(), class_id.clone()), "rdf:type".to_string());
            }

            // Load entities that reference this instance (backlinks/incoming relationships)
            let backlink_edges: Vec<(String, String, String)> = conn
                .prepare(
                    "SELECT subject, predicate, object FROM triples
                     WHERE object = ?
                     AND object_type = 'iri'
                     AND retracted = 0
                     AND predicate != 'rdf:type'
                     AND subject NOT LIKE '_:%'"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([&central_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();

            // Add backlink nodes and edges
            for (subject, predicate, object) in &backlink_edges {
                included_nodes.insert(subject.clone());
                included_edges.insert((subject.clone(), object.clone()));
                edge_predicates.insert((subject.clone(), object.clone()), predicate.clone());
            }

            println!("[Graph] Loaded {} backlink edges for instance", backlink_edges.len());

            // Load outgoing relationships from this instance (properties it owns)
            let outgoing_edges: Vec<(String, String, String)> = conn
                .prepare(
                    "SELECT subject, predicate, object FROM triples
                     WHERE subject = ?
                     AND object_type = 'iri'
                     AND retracted = 0
                     AND predicate != 'rdf:type'
                     AND object NOT LIKE '_:%'"
                )
                .map_err(|e| format!("Prepare error: {}", e))?
                .query_map([&central_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .map_err(|e| format!("Query error: {}", e))?
                .filter_map(Result::ok)
                .collect();

            // Add outgoing nodes and edges
            for (subject, predicate, object) in &outgoing_edges {
                included_nodes.insert(object.clone());
                included_edges.insert((subject.clone(), object.clone()));
                edge_predicates.insert((subject.clone(), object.clone()), predicate.clone());
            }

            println!("[Graph] Loaded {} outgoing edges for instance", outgoing_edges.len());
        }

        println!("[Graph] Central is instance: {}", central_is_instance);

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

        // Update included_edges to use canonical representatives and preserve predicates
        let mut canonical_edges = std::collections::HashSet::new();
        let mut canonical_edge_predicates: std::collections::HashMap<(String, String), String> = std::collections::HashMap::new();
        for (source, target) in &included_edges {
            let canonical_source = equivalence_map.get(source).unwrap_or(source);
            let canonical_target = equivalence_map.get(target).unwrap_or(target);
            // Skip self-loops that might arise from equivalence
            if canonical_source != canonical_target {
                canonical_edges.insert((canonical_source.clone(), canonical_target.clone()));
                // Preserve the predicate
                if let Some(predicate) = edge_predicates.get(&(source.clone(), target.clone())) {
                    canonical_edge_predicates.insert((canonical_source.clone(), canonical_target.clone()), predicate.clone());
                }
            }
        }

        // Get only the classes that we included (by querying each one directly)
        let mut nodes = Vec::new();
        let mut node_ids = std::collections::HashSet::new();

        let canonical_central_id = equivalence_map.get(&central_id).unwrap_or(&central_id).clone();

        for entity_id in &canonical_nodes {
            // Get all equivalent entities for this canonical
            let default_vec = vec![entity_id.clone()];
            let equivalent_entities = equiv_groups.get(entity_id).map(|v| v.as_slice()).unwrap_or(&default_vec[..]);

            // Check if this is the central node - always include it even if not a class
            let is_central = entity_id == &canonical_central_id;

            // Check if this entity is a class (has rdf:type owl:Class or rdfs:Class)
            let mut is_class = false;
            for equiv_id in equivalent_entities {
                if let Ok(count) = conn.query_row(
                    "SELECT COUNT(*) FROM triples
                     WHERE subject = ?
                     AND predicate = 'rdf:type'
                     AND object_type = 'iri'
                     AND (object = 'owl:Class' OR object = 'rdfs:Class')
                     AND retracted = 0",
                    [equiv_id],
                    |row| row.get::<_, i64>(0),
                ) {
                    if count > 0 {
                        is_class = true;
                        break;
                    }
                }
            }

            // Check if this is an instance (has rdf:type but not to owl:Class or rdfs:Class)
            let is_instance = conn.query_row(
                "SELECT COUNT(*) FROM triples
                 WHERE subject = ?
                 AND predicate = 'rdf:type'
                 AND object_type = 'iri'
                 AND retracted = 0
                 AND object NOT IN ('owl:Class', 'rdfs:Class')",
                [entity_id],
                |row| row.get::<_, i64>(0)
            ).unwrap_or(0) > 0;

            // Include if it's a class, central node, or an instance
            if !is_class && !is_central && !is_instance {
                continue;
            }

            // Collect labels from all equivalent entities
            let mut labels = Vec::new();
            for equiv_id in equivalent_entities {
                // Try foundation:name first (for instances), then rdfs:label (for classes)
                let label_result = conn.query_row(
                    "SELECT COALESCE(object_value, object) FROM triples
                     WHERE subject = ? AND predicate = 'foundation:name'
                     AND retracted = 0 LIMIT 1",
                    [equiv_id],
                    |row| row.get::<_, String>(0),
                ).or_else(|_| {
                    conn.query_row(
                        "SELECT COALESCE(object_value, object) FROM triples
                         WHERE subject = ? AND predicate = 'rdfs:label'
                         AND retracted = 0 LIMIT 1",
                        [equiv_id],
                        |row| row.get::<_, String>(0),
                    )
                });

                if let Ok(label) = label_result {
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

            // Determine group: classes get group 1, instances get group 6
            let group = if is_instance {
                6 // Instance
            } else {
                1 // Class (or central node)
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

            // If no icon found, check if this is an instance and get icon from its class
            if icon.is_none() {
                for equiv_id in equivalent_entities {
                    if let Ok(class_id) = conn.query_row(
                        "SELECT object FROM triples
                         WHERE subject = ?
                         AND predicate = 'rdf:type'
                         AND object_type = 'iri'
                         AND retracted = 0
                         AND object NOT IN ('owl:Class', 'rdfs:Class')
                         LIMIT 1",
                        [equiv_id],
                        |row| row.get::<_, String>(0),
                    ) {
                        // Get icon from the class
                        if let Ok(class_icon) = conn.query_row(
                            "SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:icon' AND retracted = 0 LIMIT 1",
                            [&class_id],
                            |row| row.get::<_, String>(0),
                        ) {
                            icon = Some(class_icon);
                            break;
                        }
                    }
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

        // Build links - will include subClassOf, rdf:type, and property edges
        let mut links = Vec::new();
        for (source, target) in canonical_edges {
            // Double-check both nodes are in our set (should always be true)
            if node_ids.contains(&source) && node_ids.contains(&target) {
                // Get the predicate label, simplify it for display
                let predicate = canonical_edge_predicates.get(&(source.clone(), target.clone()))
                    .map(|p| p.as_str())
                    .unwrap_or("subClassOf");

                // Simplify predicate for display: extract the local name after '/', '#', or ':'
                let label = predicate
                    .rsplit(|c| c == '/' || c == '#' || c == ':')
                    .next()
                    .unwrap_or(predicate)
                    .to_string();

                links.push(GraphLink {
                    source,
                    target,
                    label,
                });
            }
        }

        // Add instances of the central class (only if central node is NOT an instance)
        if !central_is_instance {
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
                     AND predicate = 'foundation:name'
                     AND retracted = 0
                     LIMIT 1",
                    [&instance_id],
                    |row| row.get(0),
                )
                .or_else(|_| {
                    conn.query_row(
                        "SELECT COALESCE(object_value, object) FROM triples
                         WHERE subject = ?
                         AND predicate = 'rdfs:label'
                         AND retracted = 0
                         LIMIT 1",
                        [&instance_id],
                        |row| row.get(0),
                    )
                })
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
        } // End if !central_is_instance

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
