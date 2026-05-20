use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use crate::owl::{DbExecutor, get_literal_property, get_all_iri_properties, get_iri_property, find_entities_with_property, is_subclass_of};
use crate::owl::vocabulary::{rdf, rdfs};
use rusqlite::Connection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassRef {
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
    pub input_class_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_class_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_class_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_class_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_payload: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uses_tools: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assigned_to_roles: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assigned_to_users: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub output_classes: Vec<ClassRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer_cycle: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationGraph {
    pub process_label: String,
    pub is_executable: bool,
    pub nodes: Vec<AutomationGraphNode>,
    pub edges: Vec<AutomationGraphEdge>,
    pub sequence_flow_iris: Vec<String>,
}

fn extract_local_name(iri: &str) -> &str {
    iri.rsplit_once(':')
        .map(|(_, local)| local)
        .or_else(|| iri.rsplit_once('#').map(|(_, local)| local))
        .or_else(|| iri.rsplit_once('/').map(|(_, local)| local))
        .unwrap_or(iri)
}

/// Resolves the canonical bpmn_* render type for a node by walking rdfs:subClassOf
/// until a class with the "bpmn_" local-name prefix is found.
fn resolve_bpmn_render_type(conn: &Connection, type_iri: &str) -> String {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![type_iri.to_string()];
    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        let local = extract_local_name(&current);
        if local.starts_with("bpmn_") {
            return local.to_string();
        }
        if let Ok(supers) = get_all_iri_properties(conn, &current, "rdfs:subClassOf") {
            let bpmn_super = supers.iter().find(|s| extract_local_name(s).starts_with("bpmn_"));
            if let Some(bpmn) = bpmn_super {
                return extract_local_name(bpmn).to_string();
            }
            for s in supers {
                if s != "owl:Thing" {
                    queue.push(s);
                }
            }
        }
    }
    extract_local_name(type_iri).to_string()
}

fn get_members_by_part_of_process(
    conn: &Connection,
    process_iri: &str,
    is_sequence_flow: bool,
) -> Result<Vec<String>, String> {
    use crate::eavto::query;
    let result = query::get_by_predicate_object(conn, "foundation:partOfProcess", process_iri)
        .map_err(|e| e.to_string())?;
    let mut iris = Vec::new();
    for triple in &result.triples {
        let type_iri = get_iri_property(conn, &triple.subject, rdf::TYPE)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let is_sf = is_subclass_of(conn, &type_iri, "foundation:bpmn_SequenceFlow");
        if is_sf == is_sequence_flow {
            iris.push(triple.subject.clone());
        }
    }
    Ok(iris)
}

pub fn build_automation_graph(conn: &rusqlite::Connection, automation_iri: &str) -> Result<AutomationGraph, String> {
        let process_label = get_literal_property(conn, automation_iri, rdfs::LABEL)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| automation_iri.to_string());

        let flow_node_iris = get_members_by_part_of_process(conn, automation_iri, false)?;
        let sequence_flow_iris = get_members_by_part_of_process(conn, automation_iri, true)?;

        let mut nodes: Vec<AutomationGraphNode> = Vec::new();
        for node_iri in flow_node_iris {
            let type_iri = get_iri_property(conn, &node_iri, rdf::TYPE)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            // bpmn_* canonical type sent to the frontend renderer
            let node_type = resolve_bpmn_render_type(conn, &type_iri);
            // original local name used for property-specific reads
            let raw_type = extract_local_name(&type_iri).to_string();

            let mut label = get_literal_property(conn, &node_iri, rdfs::LABEL)
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| node_iri.clone());

            let (status, status_color, status_icon) = match crate::owl::get_entity_status_info(conn, &node_iri) {
                Some((_, label, color, icon)) => (Some(label), color, icon),
                None => (None, None, None),
            };

            let assigned_agent = if raw_type == "automation_AgentTask" {
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

            let invokes_process = if raw_type == "automation_SubProcess" {
                get_iri_property(conn, &node_iri, "foundation:calledElement")
                    .map_err(|e| e.to_string())?
            } else if raw_type == "process_SubProcess" {
                get_iri_property(conn, &node_iri, "foundation:invokesProcess")
                    .map_err(|e| e.to_string())?
            } else {
                None
            };

            if let Some(ref process_iri) = invokes_process {
                if let Ok(Some(process_label)) = get_literal_property(conn, process_iri, rdfs::LABEL) {
                    label = process_label;
                }
            }

            let (input_class_label, input_class_icon, output_class_label, output_class_icon, output_classes) =
                if raw_type == "automation_SubProcess" {
                    let (mut in_lbl, mut in_icon, mut out_lbl, mut out_icon) = (None, None, None, None);
                    let mut out_classes: Vec<ClassRef> = Vec::new();

                    // Input: from the StartEvent of the called process
                    if let Some(ref process_iri) = invokes_process {
                        let sub_nodes = get_members_by_part_of_process(conn, process_iri, false)?;
                        for sub_iri in &sub_nodes {
                            let sub_type = get_iri_property(conn, sub_iri, rdf::TYPE)
                                .map_err(|e| e.to_string())?
                                .map(|t| extract_local_name(&t).to_string())
                                .unwrap_or_default();
                            if sub_type == "automation_StartEvent" || sub_type == "process_StartEvent" {
                                if let Ok(Some(c)) = get_iri_property(conn, sub_iri, "foundation:inputClass") {
                                    in_lbl = get_literal_property(conn, &c, rdfs::LABEL).map_err(|e| e.to_string())?;
                                    in_icon = {
                                        use crate::eavto::{query, Object};
                                        query::get_by_entity_predicate(conn, &c, "foundation:hasIcon")
                                            .map_err(|e| e.to_string())
                                            .map(|r| r.triples.into_iter().next().and_then(|t| match t.object {
                                                Object::Iri(icon_iri) => crate::owl::icon_iri_to_display(conn, &icon_iri),
                                                Object::Literal { value, .. } =>
                                                    Some(crate::owl::icon_literal_to_display(&value)),
                                                _ => None,
                                            }))?
                                    };
                                }
                                break;
                            }
                        }
                    }

                    // Outputs: one handle per Segway, labelled by the triggering EndEvent
                    let segway_iris = get_all_iri_properties(conn, &node_iri, "foundation:hasSegway")
                        .map_err(|e| e.to_string())?;
                    for segway_iri in &segway_iris {
                        if let Ok(Some(end_iri)) = get_iri_property(conn, segway_iri, "foundation:segwayFromEndEvent") {
                            let end_lbl = get_literal_property(conn, &end_iri, rdfs::LABEL)
                                .map_err(|e| e.to_string())?
                                .unwrap_or_else(|| end_iri.clone());
                            let (lbl, icon) = if let Ok(Some(class_iri)) = get_iri_property(conn, &end_iri, "foundation:outputClass") {
                                let class_lbl = get_literal_property(conn, &class_iri, rdfs::LABEL)
                                    .map_err(|e| e.to_string())?
                                    .unwrap_or_else(|| class_iri.clone());
                                let i = {
                                    use crate::eavto::{query, Object};
                                    query::get_by_entity_predicate(conn, &class_iri, "foundation:hasIcon")
                                        .map_err(|e| e.to_string())
                                        .map(|r| r.triples.into_iter().next().and_then(|t| match t.object {
                                            Object::Iri(icon_iri) => crate::owl::icon_iri_to_display(conn, &icon_iri),
                                            Object::Literal { value, .. } =>
                                                Some(crate::owl::icon_literal_to_display(&value)),
                                            _ => None,
                                        }))?
                                };
                                (format!("{} ({})", class_lbl, end_lbl), i)
                            } else {
                                (end_lbl, None)
                            };
                            out_classes.push(ClassRef { label: lbl, icon });
                        }
                    }

                    if out_classes.len() == 1 {
                        let single = out_classes.remove(0);
                        out_lbl = Some(single.label);
                        out_icon = single.icon;
                    }

                    (in_lbl, in_icon, out_lbl, out_icon, out_classes)
                } else {
                    let (in_lbl, in_icon) = if let Ok(Some(iri)) = get_iri_property(conn, &node_iri, "foundation:inputClass") {
                        let lbl = get_literal_property(conn, &iri, rdfs::LABEL)
                            .map_err(|e| e.to_string())?
                            .unwrap_or_else(|| iri.clone());
                        let icon = {
                            use crate::eavto::{query, Object};
                            query::get_by_entity_predicate(conn, &iri, "foundation:hasIcon")
                                .map_err(|e| e.to_string())
                                .map(|r| r.triples.into_iter().next().and_then(|t| match t.object {
                                    Object::Iri(icon_iri) => crate::owl::icon_iri_to_display(conn, &icon_iri),
                                    Object::Literal { value, .. } => Some(value),
                                    _ => None,
                                }))?
                        };
                        (Some(lbl), icon)
                    } else {
                        (None, None)
                    };
                    let (out_lbl, out_icon) = if let Ok(Some(iri)) = get_iri_property(conn, &node_iri, "foundation:outputClass") {
                        let lbl = get_literal_property(conn, &iri, rdfs::LABEL)
                            .map_err(|e| e.to_string())?
                            .unwrap_or_else(|| iri.clone());
                        let icon = {
                            use crate::eavto::{query, Object};
                            query::get_by_entity_predicate(conn, &iri, "foundation:hasIcon")
                                .map_err(|e| e.to_string())
                                .map(|r| r.triples.into_iter().next().and_then(|t| match t.object {
                                    Object::Iri(icon_iri) => crate::owl::icon_iri_to_display(conn, &icon_iri),
                                    Object::Literal { value, .. } => Some(value),
                                    _ => None,
                                }))?
                        };
                        (Some(lbl), icon)
                    } else {
                        (None, None)
                    };
                    (in_lbl, in_icon, out_lbl, out_icon, vec![])
                };

            let message_payload = if raw_type == "automation_NOVAMessageTask" {
                get_literal_property(conn, &node_iri, "foundation:messagePayload")
                    .map_err(|e| e.to_string())?
            } else {
                None
            };

            let timer_cycle = if raw_type == "automation_TimerStartEvent" {
                let event_def_iri = get_iri_property(conn, &node_iri, "foundation:eventDefinition")
                    .map_err(|e| e.to_string())?;
                if let Some(def_iri) = event_def_iri {
                    get_literal_property(conn, &def_iri, "foundation:timeCycle")
                        .map_err(|e| e.to_string())?
                } else {
                    None
                }
            } else {
                None
            };

            let (uses_tools, assigned_to_roles, assigned_to_users) = if raw_type == "automation_UserTask" {
                let tool_iris = get_all_iri_properties(conn, &node_iri, "foundation:usesTool")
                    .map_err(|e| e.to_string())?;
                let tools: Vec<String> = tool_iris.iter()
                    .filter_map(|iri| get_literal_property(conn, iri, rdfs::LABEL).ok().flatten())
                    .collect();

                let role_iris = get_all_iri_properties(conn, &node_iri, "foundation:assignedToRole")
                    .map_err(|e| e.to_string())?;
                let roles: Vec<String> = role_iris.iter()
                    .filter_map(|iri| get_literal_property(conn, iri, rdfs::LABEL).ok().flatten())
                    .collect();

                let user_iris = get_all_iri_properties(conn, &node_iri, "foundation:assignedToUser")
                    .map_err(|e| e.to_string())?;
                let users: Vec<String> = user_iris.iter()
                    .filter_map(|iri| get_literal_property(conn, iri, rdfs::LABEL).ok().flatten())
                    .collect();

                (tools, roles, users)
            } else {
                (vec![], vec![], vec![])
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
                input_class_label,
                input_class_icon,
                output_class_label,
                output_class_icon,
                message_payload,
                uses_tools,
                assigned_to_roles,
                assigned_to_users,
                output_classes,
                timer_cycle,
            });
        }

        let mut edges: Vec<AutomationGraphEdge> = Vec::new();
        for (i, flow_iri) in sequence_flow_iris.iter().enumerate() {
            // automation_SequenceFlow uses sourceRef/targetRef;
            // process_SequenceFlow uses process_sourceRef/process_targetRef
            let source = get_iri_property(conn, flow_iri, "foundation:sourceRef")
                .map_err(|e| e.to_string())?
                .or_else(|| get_iri_property(conn, flow_iri, "foundation:process_sourceRef").ok().flatten());
            let target = get_iri_property(conn, flow_iri, "foundation:targetRef")
                .map_err(|e| e.to_string())?
                .or_else(|| get_iri_property(conn, flow_iri, "foundation:process_targetRef").ok().flatten());

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
                    source_handle: None,
                });
            }
        }

        // Synthetic edges from SubProcess Segways (replace SequenceFlow-based output routing)
        let subprocess_nodes: Vec<&str> = nodes.iter()
            .filter(|n| n.node_type == "bpmn_SubProcess")
            .map(|n| n.id.as_str())
            .collect();
        let edge_offset = edges.len();
        for (si, &sp_iri) in subprocess_nodes.iter().enumerate() {
            let segway_iris = get_all_iri_properties(conn, sp_iri, "foundation:hasSegway")
                .map_err(|e| e.to_string())?;
            for (gi, segway_iri) in segway_iris.iter().enumerate() {
                let to_node = get_iri_property(conn, segway_iri, "foundation:segwayToNode")
                    .map_err(|e| e.to_string())?;
                let seg_label = get_literal_property(conn, segway_iri, rdfs::LABEL)
                    .map_err(|e| e.to_string())?;
                if let Some(target) = to_node {
                    edges.push(AutomationGraphEdge {
                        id: format!("seg{}_{}", si, gi + edge_offset),
                        source: sp_iri.to_string(),
                        target,
                        label: seg_label,
                        condition_expression: None,
                        source_handle: Some(format!("output-{}", gi)),
                    });
                }
            }
        }

        // Synthetic edges from ErrorHandlers attached to nodes in this process
        let node_iris: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        let error_edge_offset = edges.len();
        for (ei, &failed_node_iri) in node_iris.iter().enumerate() {
            use crate::eavto::query;
            let handlers = query::get_by_predicate_object(conn, "foundation:appliesTo", failed_node_iri)
                .map_err(|e| e.to_string())?;
            for (hi, triple) in handlers.triples.iter().enumerate() {
                let handler_iri = &triple.subject;
                let type_iri = get_iri_property(conn, handler_iri, rdf::TYPE)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                if type_iri != "foundation:ErrorHandler" {
                    continue;
                }
                if let Ok(Some(fallback_iri)) = get_iri_property(conn, handler_iri, "foundation:fallbackNode") {
                    edges.push(AutomationGraphEdge {
                        id: format!("err{}_{}", ei, hi + error_edge_offset),
                        source: failed_node_iri.to_string(),
                        target: fallback_iri,
                        label: Some("on error".to_string()),
                        condition_expression: None,
                        source_handle: Some("error".to_string()),
                    });
                }
            }
        }

        let root_type = get_iri_property(conn, automation_iri, rdf::TYPE)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let is_executable = is_subclass_of(conn, &root_type, "foundation:Automation");

        Ok(AutomationGraph {
            process_label,
            is_executable,
            nodes,
            edges,
            sequence_flow_iris,
        })
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn automation__get_graph(
    process_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let graph = build_automation_graph(conn, &process_iri)?;
        serde_json::to_string(&graph).map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn automation__run(
    automation_iri: String,
    input_iri: Option<String>,
    dry_run: Option<bool>,
    app: AppHandle,
) -> Result<(), String> {
    let dry_run = dry_run.unwrap_or(false);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::process_automation::executor::run_process(&app, &automation_iri, input_iri, dry_run).await {
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
            let output_text = get_literal_property(conn, &step_iri, "foundation:stepOutput")
                .map_err(|e| e.to_string())?;
            let output = output_iri.or(output_text);

            let error = get_literal_property(conn, &step_iri, "foundation:stepError")
                .map_err(|e| e.to_string())?;

            let conversation_iri = find_entities_with_property(conn, "foundation:generatedByStep", &step_iri)
                .map_err(|e| e.to_string())?
                .into_iter().next();

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
