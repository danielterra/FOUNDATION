use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use crate::owl::{DbExecutor, get_literal_property, get_all_iri_properties, get_iri_property};
use crate::owl::vocabulary::{rdf, rdfs};
use rusqlite::Connection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptRef {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uses_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to_user: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub output_concepts: Vec<ConceptRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationGraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition_expression: Option<String>,
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
                get_iri_property(conn, &node_iri, "foundation:calledElement")
                    .map_err(|e| e.to_string())?
            } else {
                None
            };

            let (input_concept_label, input_concept_icon, output_concept_label, output_concept_icon, output_concepts) =
                if node_type == "automation_SubProcess" {
                    let (mut in_lbl, mut in_icon, mut out_lbl, mut out_icon) = (None, None, None, None);
                    let mut out_concepts: Vec<ConceptRef> = Vec::new();

                    if let Some(ref process_iri) = invokes_process {
                        let sub_nodes = get_all_iri_properties(conn, process_iri, "foundation:hasFlowNode")
                            .map_err(|e| e.to_string())?;
                        for sub_iri in &sub_nodes {
                            let sub_type = get_iri_property(conn, sub_iri, rdf::TYPE)
                                .map_err(|e| e.to_string())?
                                .map(|t| extract_local_name(&t).to_string())
                                .unwrap_or_default();
                            if sub_type == "automation_StartEvent" {
                                if let Ok(Some(c)) = get_iri_property(conn, sub_iri, "foundation:inputConcept") {
                                    in_lbl = get_literal_property(conn, &c, rdfs::LABEL).map_err(|e| e.to_string())?;
                                    in_icon = get_literal_property(conn, &c, "foundation:icon").map_err(|e| e.to_string())?;
                                }
                            } else if sub_type == "automation_EndEvent" {
                                let concept_iris = get_all_iri_properties(conn, sub_iri, "foundation:outputConcept")
                                    .map_err(|e| e.to_string())?;
                                for c in concept_iris {
                                    let lbl = get_literal_property(conn, &c, rdfs::LABEL)
                                        .map_err(|e| e.to_string())?
                                        .unwrap_or_else(|| c.clone());
                                    let icon = get_literal_property(conn, &c, "foundation:icon").map_err(|e| e.to_string())?;
                                    if !out_concepts.iter().any(|x| x.label == lbl) {
                                        out_concepts.push(ConceptRef { label: lbl, icon });
                                    }
                                }
                            }
                        }
                    }

                    if out_concepts.len() == 1 {
                        let single = out_concepts.remove(0);
                        out_lbl = Some(single.label);
                        out_icon = single.icon;
                    }

                    (in_lbl, in_icon, out_lbl, out_icon, out_concepts)
                } else {
                    let (in_lbl, in_icon) = if let Ok(Some(iri)) = get_iri_property(conn, &node_iri, "foundation:inputConcept") {
                        let lbl = get_literal_property(conn, &iri, rdfs::LABEL)
                            .map_err(|e| e.to_string())?
                            .unwrap_or_else(|| iri.clone());
                        let icon = get_literal_property(conn, &iri, "foundation:icon")
                            .map_err(|e| e.to_string())?;
                        (Some(lbl), icon)
                    } else {
                        (None, None)
                    };
                    let (out_lbl, out_icon) = if let Ok(Some(iri)) = get_iri_property(conn, &node_iri, "foundation:outputConcept") {
                        let lbl = get_literal_property(conn, &iri, rdfs::LABEL)
                            .map_err(|e| e.to_string())?
                            .unwrap_or_else(|| iri.clone());
                        let icon = get_literal_property(conn, &iri, "foundation:icon")
                            .map_err(|e| e.to_string())?;
                        (Some(lbl), icon)
                    } else {
                        (None, None)
                    };
                    (in_lbl, in_icon, out_lbl, out_icon, vec![])
                };

            let message_payload = if node_type == "automation_NOVAMessageTask" {
                get_literal_property(conn, &node_iri, "foundation:messagePayload")
                    .map_err(|e| e.to_string())?
            } else {
                None
            };

            let (uses_tools, assigned_to_role, assigned_to_user) = if node_type == "automation_UserTask" {
                let tool_iris = get_all_iri_properties(conn, &node_iri, "foundation:usesTool")
                    .map_err(|e| e.to_string())?;
                let tools: Vec<String> = tool_iris.iter()
                    .filter_map(|iri| get_literal_property(conn, iri, rdfs::LABEL).ok().flatten())
                    .collect();

                let role = if let Ok(Some(iri)) = get_iri_property(conn, &node_iri, "foundation:assignedToRole") {
                    get_literal_property(conn, &iri, rdfs::LABEL).map_err(|e| e.to_string())?
                } else {
                    None
                };

                let user = if let Ok(Some(iri)) = get_iri_property(conn, &node_iri, "foundation:assignedToUser") {
                    get_literal_property(conn, &iri, rdfs::LABEL).map_err(|e| e.to_string())?
                } else {
                    None
                };

                (tools, role, user)
            } else {
                (vec![], None, None)
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
                uses_tools,
                assigned_to_role,
                assigned_to_user,
                output_concepts,
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
                let condition_expression = get_literal_property(conn, flow_iri, "foundation:conditionExpression")
                    .map_err(|e| e.to_string())?;

                edges.push(AutomationGraphEdge {
                    id: format!("e{}", i + 1),
                    source,
                    target,
                    label,
                    condition_expression,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionDetail {
    pub iri: String,
    pub node_iri: String,
    pub node_label: String,
    pub node_type: String,
    pub status_label: String,
    pub status_color: Option<String>,
    pub status_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_iri: Option<String>,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecutionDetail {
    pub iri: String,
    pub process_iri: String,
    pub process_label: String,
    pub status_label: String,
    pub status_color: Option<String>,
    pub status_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub steps: Vec<StepExecutionDetail>,
}

fn load_step_messages(
    conn: &Connection,
    conv_iri: &str,
) -> std::result::Result<Vec<ConversationMessage>, String> {
    let mut stmt = conn.prepare(
        "SELECT tr.object_value, tc.object_value \
         FROM triples tm \
         JOIN triples tr ON tr.subject = tm.subject \
           AND tr.predicate = 'foundation:role' AND tr.retracted = 0 \
         JOIN triples tc ON tc.subject = tm.subject \
           AND tc.predicate = 'foundation:content' AND tc.retracted = 0 \
         WHERE tm.predicate = 'foundation:partOfConversation' \
           AND tm.object = ?1 AND tm.retracted = 0 \
         ORDER BY tm.rowid ASC"
    ).map_err(|e| e.to_string())?;
    let messages = stmt.query_map([conv_iri], |row| {
        Ok(ConversationMessage {
            role: row.get(0)?,
            content: row.get(1)?,
        })
    })
    .map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();
    Ok(messages)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn automation__get_execution(
    execution_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let process_iri = get_iri_property(conn, &execution_iri, "foundation:executesProcess")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        let process_label = get_literal_property(conn, &process_iri, rdfs::LABEL)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| process_iri.clone());

        let (exec_status_label, exec_status_color, exec_status_icon) =
            match crate::owl::get_entity_status_info(conn, &execution_iri) {
                Some((_, label, color, icon)) => (label, color, icon),
                None => ("Unknown".to_string(), None, None),
            };

        let error_message = get_literal_property(conn, &execution_iri, "foundation:errorMessage")
            .map_err(|e| e.to_string())?;

        let step_iris = get_all_iri_properties(conn, &execution_iri, "foundation:hasStepExecutions")
            .map_err(|e| e.to_string())?;

        let mut steps = Vec::new();
        for step_iri in step_iris {
            let node_iri = get_iri_property(conn, &step_iri, "foundation:executesStep")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();

            let node_label = get_literal_property(conn, &node_iri, rdfs::LABEL)
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| node_iri.clone());

            let node_type = get_iri_property(conn, &node_iri, rdf::TYPE)
                .map_err(|e| e.to_string())?
                .map(|t| extract_local_name(&t).to_string())
                .unwrap_or_default();

            let (step_status_label, step_status_color, step_status_icon) =
                match crate::owl::get_entity_status_info(conn, &step_iri) {
                    Some((_, label, color, icon)) => (label, color, icon),
                    None => ("Unknown".to_string(), None, None),
                };

            let started_at = get_literal_property(conn, &step_iri, "foundation:stepStartedAt")
                .map_err(|e| e.to_string())?;

            let finished_at = get_literal_property(conn, &step_iri, "foundation:stepFinishedAt")
                .map_err(|e| e.to_string())?;

            let output_iri = get_iri_property(conn, &step_iri, "foundation:outputValue")
                .map_err(|e| e.to_string())?;
            let output_text = get_literal_property(conn, &step_iri, "rdfs:comment")
                .map_err(|e| e.to_string())?;
            let output = output_iri.or(output_text);

            let error = get_literal_property(conn, &step_iri, "foundation:stepError")
                .map_err(|e| e.to_string())?;

            let conversation_iri = get_iri_property(conn, &step_iri, "foundation:hasConversation")
                .map_err(|e| e.to_string())?;

            let messages = if let Some(ref conv_iri) = conversation_iri {
                load_step_messages(conn, conv_iri)?
            } else {
                Vec::new()
            };

            steps.push(StepExecutionDetail {
                iri: step_iri,
                node_iri,
                node_label,
                node_type,
                status_label: step_status_label,
                status_color: step_status_color,
                status_icon: step_status_icon,
                started_at,
                finished_at,
                output,
                error,
                conversation_iri,
                messages,
            });
        }

        steps.sort_by(|a, b| a.started_at.cmp(&b.started_at));

        let detail = WorkflowExecutionDetail {
            iri: execution_iri,
            process_iri,
            process_label,
            status_label: exec_status_label,
            status_color: exec_status_color,
            status_icon: exec_status_icon,
            error_message,
            steps,
        };

        serde_json::to_string(&detail).map_err(|e| e.to_string())
    }).await
}
