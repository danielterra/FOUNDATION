use serde::{Deserialize, Serialize};
use tauri::State;
use crate::owl::{DbExecutor, get_iri_property, get_literal_property, get_all_iri_properties};
use crate::owl::vocabulary::{rdf, rdfs};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invokes_process: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessGraph {
    pub process_label: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

fn extract_local_name(iri: &str) -> &str {
    iri.rsplit_once(':')
        .map(|(_, local)| local)
        .or_else(|| iri.rsplit_once('#').map(|(_, local)| local))
        .or_else(|| iri.rsplit_once('/').map(|(_, local)| local))
        .unwrap_or(iri)
}

/// Build a map of target_iri -> edge_label for a gateway node by reading its hasCondition instances.
fn build_condition_labels(
    conn: &crate::owl::Connection,
    node_iri: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut map = std::collections::HashMap::new();
    let condition_iris = get_all_iri_properties(conn, node_iri, "foundation:hasCondition")
        .map_err(|e| e.to_string())?;
    for cond_iri in condition_iris {
        let target = get_iri_property(conn, &cond_iri, "foundation:conditionTarget")
            .map_err(|e| e.to_string())?;
        let label = get_literal_property(conn, &cond_iri, rdfs::LABEL)
            .map_err(|e| e.to_string())?;
        if let (Some(target), Some(label)) = (target, label) {
            map.insert(target, label);
        }
    }
    Ok(map)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn meta_process__get_graph(
    process_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let process_label = get_literal_property(conn, &process_iri, rdfs::LABEL)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| process_iri.clone());

        let start_node = get_iri_property(conn, &process_iri, "foundation:metaStartNode")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("No metaStartNode for {}", process_iri))?;

        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        let mut nodes: Vec<GraphNode> = Vec::new();
        let mut edges: Vec<GraphEdge> = Vec::new();
        let mut edge_counter = 0usize;

        queue.push_back(start_node);

        while let Some(current) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            let type_iri = get_iri_property(conn, &current, rdf::TYPE)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let node_type = extract_local_name(&type_iri).to_string();

            let label = get_literal_property(conn, &current, rdfs::LABEL)
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| current.clone());

            let invokes_process = get_iri_property(conn, &current, "foundation:invokesProcess")
                .map_err(|e| e.to_string())?;

            let status = {
                let status_iri = get_iri_property(conn, &current, "foundation:hasStatus")
                    .map_err(|e| e.to_string())?;
                if let Some(iri) = status_iri {
                    get_literal_property(conn, &iri, rdfs::LABEL)
                        .map_err(|e| e.to_string())?
                } else {
                    None
                }
            };

            nodes.push(GraphNode {
                id: current.clone(),
                node_type: node_type.clone(),
                label,
                invokes_process,
                status,
            });

            let is_gateway = node_type == "MetaExclusiveGateway" || node_type == "MetaParallelGateway";
            let condition_labels = if is_gateway {
                build_condition_labels(conn, &current)?
            } else {
                std::collections::HashMap::new()
            };

            let next_nodes = get_all_iri_properties(conn, &current, "foundation:nextNode")
                .map_err(|e| e.to_string())?;

            for target in next_nodes {
                edge_counter += 1;
                let edge_label = condition_labels.get(&target).cloned();
                edges.push(GraphEdge {
                    id: format!("e{}", edge_counter),
                    source: current.clone(),
                    target: target.clone(),
                    label: edge_label,
                });
                if !visited.contains(&target) {
                    queue.push_back(target);
                }
            }
        }

        let graph = ProcessGraph {
            process_label,
            nodes,
            edges,
        };

        serde_json::to_string(&graph).map_err(|e| e.to_string())
    }).await
}
