use serde_json::Value;
use rusqlite::Connection;
use crate::owl::vocabulary::{rdf, rdfs};
use super::ToolResult;

pub fn get_process(conn: &Connection, args: &Value) -> ToolResult {
    let process_iri = match args.get("process_iri").and_then(|v| v.as_str()) {
        Some(iri) if !iri.is_empty() => iri,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: process_iri".to_string()),
            concept: None,
        },
    };

    let process_label = match crate::owl::get_literal_property(conn, process_iri, rdfs::LABEL) {
        Ok(Some(label)) => label,
        Ok(None) => return ToolResult {
            success: false,
            result: None,
            error: Some(format!("entity_not_found: {process_iri}")),
            concept: None,
        },
        Err(e) => return ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    };

    let process_comment = crate::owl::get_literal_property(conn, process_iri, rdfs::COMMENT)
        .ok()
        .flatten();

    let process_status = resolve_status_label(conn, process_iri);

    let start_node = match crate::owl::get_iri_property(conn, process_iri, "foundation:metaStartNode") {
        Ok(Some(node)) => node,
        Ok(None) => return ToolResult {
            success: false,
            result: None,
            error: Some(format!("No metaStartNode for {process_iri}")),
            concept: None,
        },
        Err(e) => return ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
            concept: None,
        },
    };

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut edge_counter = 0usize;

    queue.push_back(start_node);

    while let Some(current) = queue.pop_front() {
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());

        let type_iri = match crate::owl::get_iri_property(conn, &current, rdf::TYPE) {
            Ok(Some(t)) => t,
            Ok(None) => continue,
            Err(e) => return ToolResult {
                success: false,
                result: None,
                error: Some(e.to_string()),
                concept: None,
            },
        };
        let node_type = local_name(&type_iri).to_string();

        let label = crate::owl::get_literal_property(conn, &current, rdfs::LABEL)
            .ok()
            .flatten()
            .unwrap_or_else(|| current.clone());

        let icon = crate::owl::Thing::get(conn, &current).icon
            .or_else(|| crate::owl::Thing::get(conn, &type_iri).icon);

        let mut node = serde_json::json!({
            "iri": current,
            "type": node_type,
            "label": label,
        });

        if let Some(icon_val) = icon {
            node["icon"] = Value::String(icon_val);
        }

        if let Some(status) = resolve_status_label(conn, &current) {
            node["status"] = Value::String(status);
        }

        let is_condition = matches!(node_type.as_str(), "MetaGatewayCondition" | "MetaBoundaryCondition");
        if is_condition {
            if let Some(op) = resolve_label_via_iri(conn, &current, "foundation:conditionOperator") {
                node["conditionOperator"] = Value::String(op);
            }
            if let Ok(Some(val)) = crate::owl::get_literal_property(conn, &current, "foundation:conditionValue") {
                node["conditionValue"] = Value::String(val);
            }
        }

        let is_event = matches!(
            node_type.as_str(),
            "MetaStartEvent" | "MetaEndEvent" | "MetaIntermediateEvent" | "MetaBoundaryEvent"
        );
        if is_event {
            if let Some(trigger) = resolve_label_via_iri(conn, &current, "foundation:triggerType") {
                node["triggerType"] = Value::String(trigger);
            }
            if let Ok(Some(event_type)) = crate::owl::get_literal_property(conn, &current, "foundation:eventType") {
                node["eventType"] = Value::String(event_type);
            }
        }

        if node_type == "MetaSubProcess" {
            if let Ok(Some(inv)) = crate::owl::get_iri_property(conn, &current, "foundation:invokesProcess") {
                node["invokesProcess"] = Value::String(inv);
            }
        }

        nodes.push(node);

        let is_gateway = matches!(
            node_type.as_str(),
            "MetaExclusiveGateway" | "MetaEventBasedGateway" | "MetaInclusiveGateway"
        );

        let (next_prop, next_targets) = if is_gateway {
            let targets = crate::owl::get_all_iri_properties(conn, &current, "foundation:gatewayCondition")
                .unwrap_or_default();
            ("foundation:gatewayCondition", targets)
        } else {
            let targets = crate::owl::get_all_iri_properties(conn, &current, "foundation:nextNode")
                .unwrap_or_default();
            ("foundation:nextNode", targets)
        };

        for target in next_targets {
            edge_counter += 1;
            edges.push(serde_json::json!({
                "id": format!("e{edge_counter}"),
                "property": next_prop,
                "source": current,
                "target": target,
            }));
            if !visited.contains(&target) {
                queue.push_back(target);
            }
        }

        let is_task = matches!(node_type.as_str(), "MetaSystemTask" | "MetaUserTask" | "MetaSubProcess");
        if is_task {
            for bc in crate::owl::get_all_iri_properties(conn, &current, "foundation:boundaryCondition")
                .unwrap_or_default()
            {
                edge_counter += 1;
                edges.push(serde_json::json!({
                    "id": format!("e{edge_counter}"),
                    "property": "foundation:boundaryCondition",
                    "source": current,
                    "target": bc,
                }));
                if !visited.contains(&bc) {
                    queue.push_back(bc);
                }
            }
        }
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "processDetails": {
                "iri": process_iri,
                "label": process_label,
                "comment": process_comment,
                "status": process_status,
            },
            "graphNodes": nodes,
            "graphEdges": edges,
        })),
        error: None,
        concept: None,
    }
}

fn local_name(iri: &str) -> &str {
    iri.rsplit_once(':')
        .map(|(_, local)| local)
        .or_else(|| iri.rsplit_once('#').map(|(_, local)| local))
        .or_else(|| iri.rsplit_once('/').map(|(_, local)| local))
        .unwrap_or(iri)
}

fn resolve_label_via_iri(conn: &Connection, subject: &str, predicate: &str) -> Option<String> {
    let target_iri = crate::owl::get_iri_property(conn, subject, predicate).ok()??;
    crate::owl::get_literal_property(conn, &target_iri, rdfs::LABEL).ok()?
}

fn resolve_status_label(conn: &Connection, subject: &str) -> Option<String> {
    resolve_label_via_iri(conn, subject, "foundation:hasStatus")
}
