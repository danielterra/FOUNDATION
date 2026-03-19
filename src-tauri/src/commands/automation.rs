use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use crate::owl::{DbExecutor, get_literal_property, get_all_iri_properties, get_iri_property};
use crate::owl::vocabulary::{rdf, rdfs};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRef {
    pub iri: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationGraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invokes_process: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_concept_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_concept_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_concept_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_concept_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_payload: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationGraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationGraph {
    pub process_label: String,
    pub nodes: Vec<AutomationGraphNode>,
    pub edges: Vec<AutomationGraphEdge>,
}

fn extract_local_name(iri: &str) -> &str {
    iri.rsplit_once(':')
        .map(|(_, local)| local)
        .or_else(|| iri.rsplit_once('#').map(|(_, local)| local))
        .or_else(|| iri.rsplit_once('/').map(|(_, local)| local))
        .unwrap_or(iri)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn automation__get_graph(
    automation_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let process_label = get_literal_property(conn, &automation_iri, rdfs::LABEL)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| automation_iri.clone());

        let flow_node_iris = get_all_iri_properties(conn, &automation_iri, "foundation:hasFlowNode")
            .map_err(|e| e.to_string())?;

        let sequence_flow_iris = get_all_iri_properties(conn, &automation_iri, "foundation:hasSequenceFlow")
            .map_err(|e| e.to_string())?;

        let mut nodes: Vec<AutomationGraphNode> = Vec::new();
        for node_iri in flow_node_iris {
            let type_iri = get_iri_property(conn, &node_iri, rdf::TYPE)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let node_type = extract_local_name(&type_iri).to_string();

            let label = get_literal_property(conn, &node_iri, rdfs::LABEL)
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| node_iri.clone());

            let (status, status_color, status_icon) = match crate::owl::get_entity_status_info(conn, &node_iri) {
                Some((_, label, color, icon)) => (Some(label), color, icon),
                None => (None, None, None),
            };

            let assigned_agent = if node_type == "automation_AgentTask" {
                let agent_iri = get_iri_property(conn, &node_iri, "foundation:assignedAgent")
                    .map_err(|e| e.to_string())?;
                if let Some(iri) = agent_iri {
                    get_literal_property(conn, &iri, rdfs::LABEL)
                        .map_err(|e| e.to_string())?
                } else {
                    None
                }
            } else {
                None
            };

            let invokes_process = if node_type == "automation_SubProcess" {
                get_iri_property(conn, &node_iri, "foundation:invokesProcess")
                    .map_err(|e| e.to_string())?
            } else {
                None
            };

            let (input_concept_label, input_concept_icon) =
                if let Ok(Some(iri)) = get_iri_property(conn, &node_iri, "foundation:inputConcept") {
                    let lbl = get_literal_property(conn, &iri, rdfs::LABEL)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_else(|| iri.clone());
                    let icon = get_literal_property(conn, &iri, "foundation:icon")
                        .map_err(|e| e.to_string())?;
                    (Some(lbl), icon)
                } else {
                    (None, None)
                };

            let (output_concept_label, output_concept_icon) =
                if let Ok(Some(iri)) = get_iri_property(conn, &node_iri, "foundation:outputConcept") {
                    let lbl = get_literal_property(conn, &iri, rdfs::LABEL)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_else(|| iri.clone());
                    let icon = get_literal_property(conn, &iri, "foundation:icon")
                        .map_err(|e| e.to_string())?;
                    (Some(lbl), icon)
                } else {
                    (None, None)
                };

            let message_payload = if node_type == "automation_NOVAMessageTask" {
                get_literal_property(conn, &node_iri, "foundation:messagePayload")
                    .map_err(|e| e.to_string())?
            } else {
                None
            };

            nodes.push(AutomationGraphNode {
                id: node_iri,
                node_type,
                label,
                assigned_agent,
                invokes_process,
                status,
                status_color,
                status_icon,
                input_concept_label,
                input_concept_icon,
                output_concept_label,
                output_concept_icon,
                message_payload,
            });
        }

        let mut edges: Vec<AutomationGraphEdge> = Vec::new();
        for (i, flow_iri) in sequence_flow_iris.iter().enumerate() {
            let source = get_iri_property(conn, flow_iri, "foundation:sourceRef")
                .map_err(|e| e.to_string())?;
            let target = get_iri_property(conn, flow_iri, "foundation:targetRef")
                .map_err(|e| e.to_string())?;

            if let (Some(source), Some(target)) = (source, target) {
                let label = get_literal_property(conn, flow_iri, rdfs::LABEL)
                    .map_err(|e| e.to_string())?;

                edges.push(AutomationGraphEdge {
                    id: format!("e{}", i + 1),
                    source,
                    target,
                    label,
                });
            }
        }

        let graph = AutomationGraph {
            process_label,
            nodes,
            edges,
        };

        serde_json::to_string(&graph).map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn automation__run(
    automation_iri: String,
    input_iri: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::process_automation::executor::run_process(&app, &automation_iri, input_iri).await {
            crate::commands::log_backend("error", &format!("[automation] Run failed for {}: {}", automation_iri, e));
        }
    });
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn automation__find_for_types(
    type_iris: Vec<String>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let mut results: Vec<AutomationRef> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for type_iri in &type_iris {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT t.subject FROM triples t
                 WHERE t.predicate = 'foundation:inputClass'
                   AND t.object = ?1
                   AND t.retracted = 0"
            ).map_err(|e| e.to_string())?;
            let rows: Vec<String> = stmt.query_map([type_iri], |row| row.get(0))
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();

            for iri in rows {
                if seen.contains(&iri) { continue; }
                let rdf_type = get_iri_property(conn, &iri, rdf::TYPE)
                    .map_err(|e| e.to_string())?;
                if rdf_type.as_deref() != Some("foundation:Automation") { continue; }
                let label = get_literal_property(conn, &iri, rdfs::LABEL)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| iri.clone());
                seen.insert(iri.clone());
                results.push(AutomationRef { iri, label });
            }
        }

        serde_json::to_string(&results).map_err(|e| e.to_string())
    }).await
}
