use crate::AppState;
use crate::namespaces;

#[derive(serde::Serialize)]
pub struct ApplicableProperty {
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
pub fn get_applicable_properties(state: tauri::State<AppState>, node_id: String) -> Result<String, String> {
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
