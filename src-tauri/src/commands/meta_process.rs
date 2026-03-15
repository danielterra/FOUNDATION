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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition_operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renders_component: Option<String>,
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

fn get_gateway_condition_iris(
    conn: &crate::owl::Connection,
    node_iri: &str,
) -> Result<Vec<String>, String> {
    get_all_iri_properties(conn, node_iri, "foundation:gatewayCondition")
        .map_err(|e| e.to_string())
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

            let is_condition = node_type == "MetaGatewayCondition";
            let condition_operator = if is_condition {
                let op_iri = get_iri_property(conn, &current, "foundation:conditionOperator")
                    .map_err(|e| e.to_string())?;
                if let Some(iri) = op_iri {
                    get_literal_property(conn, &iri, rdfs::LABEL)
                        .map_err(|e| e.to_string())?
                } else {
                    None
                }
            } else {
                None
            };
            let condition_value = if is_condition {
                get_literal_property(conn, &current, "foundation:conditionValue")
                    .map_err(|e| e.to_string())?
            } else {
                None
            };

            let is_user_task = node_type == "MetaUserTask";
            let renders_component = if is_user_task {
                let comp_iri = get_iri_property(conn, &current, "foundation:rendersComponent")
                    .map_err(|e| e.to_string())?;
                if let Some(iri) = comp_iri {
                    get_literal_property(conn, &iri, rdfs::LABEL)
                        .map_err(|e| e.to_string())?
                } else {
                    None
                }
            } else {
                None
            };

            let is_boundary = node_type == "MetaBoundaryEvent";
            let event_type = if is_boundary {
                get_literal_property(conn, &current, "foundation:eventType")
                    .map_err(|e| e.to_string())?
            } else {
                None
            };

            let is_event = matches!(
                node_type.as_str(),
                "MetaStartEvent" | "MetaEndEvent" | "MetaIntermediateEvent"
            );
            let trigger_type = if is_event {
                let trigger_iri = get_iri_property(conn, &current, "foundation:triggerType")
                    .map_err(|e| e.to_string())?;
                if let Some(iri) = trigger_iri {
                    get_literal_property(conn, &iri, rdfs::LABEL)
                        .map_err(|e| e.to_string())?
                } else {
                    None
                }
            } else {
                None
            };

            nodes.push(GraphNode {
                id: current.clone(),
                node_type: node_type.clone(),
                label,
                invokes_process,
                status,
                condition_operator,
                condition_value,
                event_type,
                trigger_type,
                renders_component,
            });

            let is_gateway = matches!(
                node_type.as_str(),
                "MetaExclusiveGateway" | "MetaEventBasedGateway" | "MetaInclusiveGateway"
            );

            let next_targets: Vec<String> = if is_gateway {
                get_gateway_condition_iris(conn, &current)?
            } else {
                get_all_iri_properties(conn, &current, "foundation:nextNode")
                    .map_err(|e| e.to_string())?
            };

            for target in next_targets {
                edge_counter += 1;
                edges.push(GraphEdge {
                    id: format!("e{}", edge_counter),
                    source: current.clone(),
                    target: target.clone(),
                    label: None,
                });
                if !visited.contains(&target) {
                    queue.push_back(target);
                }
            }

            let is_task = matches!(
                node_type.as_str(),
                "MetaSystemTask" | "MetaUserTask" | "MetaSubProcess"
            );
            if is_task {
                let boundary_events = get_all_iri_properties(conn, &current, "foundation:hasBoundaryEvent")
                    .map_err(|e| e.to_string())?;
                for be in boundary_events {
                    edge_counter += 1;
                    edges.push(GraphEdge {
                        id: format!("e{}", edge_counter),
                        source: current.clone(),
                        target: be.clone(),
                        label: None,
                    });
                    if !visited.contains(&be) {
                        queue.push_back(be);
                    }
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
