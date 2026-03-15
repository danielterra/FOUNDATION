use serde_json::Value;
use rusqlite::Connection;
use crate::owl::vocabulary::rdfs;
use super::ToolResult;

const MAX_DEPTH_CAP: usize = 5;

pub fn get_concept_graph(conn: &Connection, args: &Value) -> ToolResult {
    let concept_iri = match args.get("concept_iri").and_then(|v| v.as_str()) {
        Some(iri) if !iri.is_empty() => iri,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: concept_iri".to_string()),
            concept: None,
        },
    };

    let max_depth = args.get("max_depth")
        .and_then(|v| v.as_u64())
        .map(|d| d as usize)
        .unwrap_or(2)
        .min(MAX_DEPTH_CAP);

    match crate::owl::get_literal_property(conn, concept_iri, rdfs::LABEL) {
        Ok(Some(_)) => {}
        Ok(None) => return ToolResult {
            success: false,
            result: None,
            error: Some(format!("entity_not_found: {concept_iri}")),
            concept: None,
        },
        Err(e) => return ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    }

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<(String, usize)> = std::collections::VecDeque::new();
    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut edge_set: std::collections::HashSet<(String, String, String)> = std::collections::HashSet::new();
    let mut edge_counter = 0usize;
    let mut depth_reached = 0usize;

    queue.push_back((concept_iri.to_string(), 0));

    while let Some((current, depth)) = queue.pop_front() {
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());
        depth_reached = depth_reached.max(depth);

        let thing = crate::owl::Thing::get(conn, &current);
        let mut node = serde_json::json!({
            "iri": current,
            "label": thing.label,
        });
        if let Some(icon) = thing.icon {
            node["icon"] = Value::String(icon);
        }
        nodes.push(node);

        if depth >= max_depth {
            continue;
        }

        // Outbound: follow properties whose domain is this class or any ancestor.
        // Properties are often declared on parent classes (e.g. MetaAbstractTask), so we must
        // check the full superclass chain to avoid missing inherited properties.
        let hierarchy = superclass_chain(conn, &current);
        let mut prop_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for ancestor in &hierarchy {
            let props = crate::owl::find_entities_with_property(conn, rdfs::DOMAIN, ancestor)
                .unwrap_or_default();
            for prop_iri in props {
                if !prop_seen.insert(prop_iri.clone()) {
                    continue;
                }
                let ranges = crate::owl::get_all_iri_properties(conn, &prop_iri, rdfs::RANGE)
                    .unwrap_or_default();
                for range_iri in ranges {
                    if is_skip_target(&range_iri) {
                        continue;
                    }
                    let edge_key = (prop_iri.clone(), current.clone(), range_iri.clone());
                    if edge_set.insert(edge_key) {
                        edge_counter += 1;
                        edges.push(build_edge(conn, edge_counter, &prop_iri, &current, &range_iri));
                    }
                    if !visited.contains(&range_iri) {
                        queue.push_back((range_iri, depth + 1));
                    }
                }
            }
        }

        // Inbound: find classes that point to this one so the graph is navigable in both directions.
        let inbound_props = crate::owl::find_entities_with_property(conn, rdfs::RANGE, &current)
            .unwrap_or_default();
        for prop_iri in &inbound_props {
            let domains = crate::owl::get_all_iri_properties(conn, prop_iri, rdfs::DOMAIN)
                .unwrap_or_default();
            for domain_iri in domains {
                if is_skip_target(&domain_iri) {
                    continue;
                }
                let edge_key = (prop_iri.clone(), domain_iri.clone(), current.clone());
                if edge_set.insert(edge_key) {
                    edge_counter += 1;
                    edges.push(build_edge(conn, edge_counter, prop_iri, &domain_iri, &current));
                }
                if !visited.contains(&domain_iri) {
                    queue.push_back((domain_iri, depth + 1));
                }
            }
        }
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "rootConcept": concept_iri,
            "depthReached": depth_reached,
            "graphNodes": nodes,
            "graphEdges": edges,
        })),
        error: None,
        concept: None,
    }
}

/// Returns the class itself plus all transitive superclasses, excluding owl:Thing and rdfs:Resource.
fn superclass_chain(conn: &Connection, class_iri: &str) -> Vec<String> {
    let mut chain = vec![class_iri.to_string()];
    let mut queue = std::collections::VecDeque::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(class_iri.to_string());
    queue.push_back(class_iri.to_string());

    while let Some(current) = queue.pop_front() {
        let superclasses = crate::owl::get_all_iri_properties(conn, &current, rdfs::SUB_CLASS_OF)
            .unwrap_or_default();
        for sc in superclasses {
            if !seen.contains(&sc) && !is_skip_target(&sc) && sc != "rdfs:Resource" {
                seen.insert(sc.clone());
                chain.push(sc.clone());
                queue.push_back(sc);
            }
        }
    }
    chain
}

fn is_skip_target(iri: &str) -> bool {
    iri == "owl:Thing"
        || iri.starts_with("xsd:")
        || iri.starts_with("owl:")
        || iri.starts_with("rdf:")
}

fn build_edge(conn: &Connection, id: usize, prop_iri: &str, source: &str, target: &str) -> Value {
    let prop_label = crate::owl::get_literal_property(conn, prop_iri, rdfs::LABEL)
        .ok()
        .flatten();
    let mut edge = serde_json::json!({
        "id": format!("e{id}"),
        "property": prop_iri,
        "source": source,
        "target": target,
    });
    if let Some(label) = prop_label {
        edge["propertyLabel"] = Value::String(label);
    }
    edge
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Class, ClassType, Property, PropertyType};

    fn setup_two_classes_one_prop() -> crate::eavto::Connection {
        let mut conn = setup_test_db();
        Class::new("test:A").assert(&mut conn, ClassType::OwlClass, "Class A", "circle", None, "test").unwrap();
        Class::new("test:B").assert(&mut conn, ClassType::OwlClass, "Class B", "square", None, "test").unwrap();
        Property::new("test:linksTo")
            .assert(&mut conn, PropertyType::ObjectProperty, "Links To", None, &["test:A"], Some("test:B"), None, "test")
            .unwrap();
        conn
    }

    // ── missing parameter ────────────────────────────────────────────────────

    #[test]
    fn test_missing_concept_iri_returns_error() {
        let conn = setup_test_db();
        let result = get_concept_graph(&conn, &serde_json::json!({}));
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap_or("").contains("concept_iri"));
    }

    // ── entity not found ─────────────────────────────────────────────────────

    #[test]
    fn test_unknown_iri_returns_entity_not_found() {
        let conn = setup_test_db();
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:DoesNotExist"}));
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap_or("").contains("entity_not_found"));
    }

    // ── root node present ────────────────────────────────────────────────────

    #[test]
    fn test_root_node_always_in_result() {
        let conn = setup_two_classes_one_prop();
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:A", "max_depth": 0}));
        assert!(result.success);
        let nodes = result.result.as_ref().unwrap()["graphNodes"].as_array().unwrap();
        assert!(nodes.iter().any(|n| n["iri"] == "test:A"));
    }

    // ── outbound traversal (direct domain) ───────────────────────────────────

    #[test]
    fn test_outbound_edge_via_direct_domain() {
        let conn = setup_two_classes_one_prop();
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:A", "max_depth": 1}));
        assert!(result.success);
        let r = result.result.as_ref().unwrap();
        let edges = r["graphEdges"].as_array().unwrap();
        assert!(
            edges.iter().any(|e| e["source"] == "test:A" && e["target"] == "test:B" && e["property"] == "test:linksTo"),
            "expected outbound edge test:A → test:linksTo → test:B"
        );
        let nodes = r["graphNodes"].as_array().unwrap();
        assert!(nodes.iter().any(|n| n["iri"] == "test:B"), "test:B must be in nodes");
    }

    // ── outbound traversal via inherited (superclass) domain ─────────────────

    #[test]
    fn test_outbound_edge_via_inherited_domain() {
        let mut conn = setup_test_db();
        // Parent class carries the property
        Class::new("test:Parent").assert(&mut conn, ClassType::OwlClass, "Parent", "folder", None, "test").unwrap();
        Class::new("test:Child").assert(&mut conn, ClassType::OwlClass, "Child", "file", Some("test:Parent"), "test").unwrap();
        Class::new("test:Target").assert(&mut conn, ClassType::OwlClass, "Target", "target", None, "test").unwrap();
        Property::new("test:parentProp")
            .assert(&mut conn, PropertyType::ObjectProperty, "Parent Prop", None, &["test:Parent"], Some("test:Target"), None, "test")
            .unwrap();

        // Querying Child should still surface the edge because Parent is in Child's superclass chain
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:Child", "max_depth": 1}));
        assert!(result.success);
        let edges = result.result.as_ref().unwrap()["graphEdges"].as_array().unwrap();
        assert!(
            edges.iter().any(|e| e["source"] == "test:Child" && e["target"] == "test:Target"),
            "expected inherited-domain edge test:Child → test:Target via test:parentProp"
        );
    }

    // ── inbound traversal ────────────────────────────────────────────────────

    #[test]
    fn test_inbound_edge_when_class_is_range() {
        let mut conn = setup_test_db();
        Class::new("test:Source").assert(&mut conn, ClassType::OwlClass, "Source", "source", None, "test").unwrap();
        Class::new("test:Sink").assert(&mut conn, ClassType::OwlClass, "Sink", "sink", None, "test").unwrap();
        Property::new("test:flowsTo")
            .assert(&mut conn, PropertyType::ObjectProperty, "Flows To", None, &["test:Source"], Some("test:Sink"), None, "test")
            .unwrap();

        // Query from the range side: test:Sink — should see inbound edge from test:Source
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:Sink", "max_depth": 1}));
        assert!(result.success);
        let edges = result.result.as_ref().unwrap()["graphEdges"].as_array().unwrap();
        assert!(
            edges.iter().any(|e| e["source"] == "test:Source" && e["target"] == "test:Sink"),
            "expected inbound edge test:Source → test:Sink"
        );
    }

    // ── depth limit respected ────────────────────────────────────────────────

    #[test]
    fn test_depth_zero_returns_only_root() {
        let conn = setup_two_classes_one_prop();
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:A", "max_depth": 0}));
        assert!(result.success);
        let r = result.result.as_ref().unwrap();
        assert_eq!(r["graphEdges"].as_array().unwrap().len(), 0);
        assert_eq!(r["graphNodes"].as_array().unwrap().len(), 1);
        assert_eq!(r["depthReached"], 0);
    }

    #[test]
    fn test_depth_cap_at_five() {
        let conn = setup_two_classes_one_prop();
        // max_depth: 99 must be silently capped — no panic or infinite loop
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:A", "max_depth": 99}));
        assert!(result.success);
        let depth = result.result.as_ref().unwrap()["depthReached"].as_u64().unwrap();
        assert!(depth <= 5);
    }

    // ── edge deduplication ───────────────────────────────────────────────────

    #[test]
    fn test_no_duplicate_edges() {
        // A → prop → B: when we visit B we would see A as an inbound domain — must not add the edge again
        let conn = setup_two_classes_one_prop();
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:A", "max_depth": 2}));
        assert!(result.success);
        let edges = result.result.as_ref().unwrap()["graphEdges"].as_array().unwrap();
        let matching: Vec<_> = edges.iter()
            .filter(|e| e["source"] == "test:A" && e["target"] == "test:B" && e["property"] == "test:linksTo")
            .collect();
        assert_eq!(matching.len(), 1, "edge must appear exactly once");
    }

    // ── property label included ───────────────────────────────────────────────

    #[test]
    fn test_edge_includes_property_label() {
        let conn = setup_two_classes_one_prop();
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:A", "max_depth": 1}));
        assert!(result.success);
        let edges = result.result.as_ref().unwrap()["graphEdges"].as_array().unwrap();
        let edge = edges.iter().find(|e| e["property"] == "test:linksTo").unwrap();
        assert_eq!(edge["propertyLabel"], "Links To");
    }

    // ── depth_reached reflects actual traversal ──────────────────────────────

    #[test]
    fn test_depth_reached_reflects_actual_depth() {
        let conn = setup_two_classes_one_prop();
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:A", "max_depth": 2}));
        assert!(result.success);
        let depth = result.result.as_ref().unwrap()["depthReached"].as_u64().unwrap();
        assert_eq!(depth, 1, "test:B is at depth 1 — depthReached must be 1");
    }

    // ── owl:Thing and xsd: are skipped ───────────────────────────────────────

    #[test]
    fn test_owl_thing_not_in_nodes() {
        let mut conn = setup_test_db();
        Class::new("test:X").assert(&mut conn, ClassType::OwlClass, "X", "x", None, "test").unwrap();
        // xsd:string range — must not appear as a node or enqueue owl:Thing
        Property::new("test:name")
            .assert(&mut conn, PropertyType::DatatypeProperty, "Name", None, &["test:X"], Some("xsd:string"), None, "test")
            .unwrap();
        let result = get_concept_graph(&conn, &serde_json::json!({"concept_iri": "test:X", "max_depth": 2}));
        assert!(result.success);
        let nodes = result.result.as_ref().unwrap()["graphNodes"].as_array().unwrap();
        assert!(nodes.iter().all(|n| !n["iri"].as_str().unwrap_or("").starts_with("xsd:")));
        assert!(nodes.iter().all(|n| n["iri"] != "owl:Thing"));
    }
}
