use std::collections::HashMap;
use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::log_backend;
use crate::eavto::query;
use crate::owl::{DbExecutor, Individual, Object};

type Result<T> = std::result::Result<T, String>;

/// Shared execution context that threads named outputs between flow nodes.
/// Keys come from a node's `foundation:outputKey` property.
pub type ExecutionContext = HashMap<String, String>;

/// Loads all flow nodes for a process, returning (node_iri, node_type, output_key).
fn load_flow_nodes(
    conn: &rusqlite::Connection,
    process_iri: &str,
) -> Result<Vec<(String, String, Option<String>)>> {
    let result = query::get_by_entity_predicate(conn, process_iri, "foundation:hasFlowNode")
        .map_err(|e| e.to_string())?;

    let mut nodes = Vec::new();
    for triple in &result.triples {
        if let Some(node_iri) = triple.object.as_iri() {
            let type_result = query::get_by_entity_predicate(conn, node_iri, "rdf:type")
                .map_err(|e| e.to_string())?;
            let node_type = type_result
                .triples
                .first()
                .and_then(|t| t.object.as_iri())
                .unwrap_or("foundation:automation_FlowNode")
                .to_string();

            let key_result = query::get_by_entity_predicate(conn, node_iri, "foundation:outputKey")
                .map_err(|e| e.to_string())?;
            let output_key = key_result
                .triples
                .first()
                .and_then(|t| t.object.as_literal())
                .map(|s| s.to_string());

            nodes.push((node_iri.to_string(), node_type, output_key));
        }
    }
    Ok(nodes)
}

/// Interpolates `{{key}}` placeholders in `template` using values from `ctx`.
pub(super) fn interpolate(template: &str, ctx: &ExecutionContext) -> String {
    let mut result = template.to_string();
    for (key, value) in ctx {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

/// Normalises a full type IRI to the bare task kind, stripping the `automation_` prefix.
fn normalize_node_type(full_type: &str) -> &str {
    let local = full_type.rsplit_once(':').map(|(_, l)| l).unwrap_or(full_type);
    local.strip_prefix("automation_").unwrap_or(local)
}

const STATUS_IN_PROGRESS: &str = "foundation:InProgress";
const STATUS_COMPLETED: &str = "foundation:Completed";
const STATUS_FAILED: &str = "foundation:Status_1772993026091";

fn create_execution_record(
    conn: &mut rusqlite::Connection,
    process_iri: &str,
) -> Result<String> {
    let exec_iri = format!("foundation:WorkflowExecution_{}", Utc::now().timestamp_millis());
    let ind = Individual::new(&exec_iri);
    ind.assert(conn, "foundation:WorkflowExecution", &exec_iri, "play_circle", "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:executesProcess",
        vec![Object::Iri(process_iri.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(STATUS_IN_PROGRESS.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    Ok(exec_iri)
}

fn lit_datetime(ms: i64) -> Object {
    let dt = chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now);
    Object::Literal { value: dt.to_rfc3339(), datatype: Some("xsd:dateTime".to_string()), language: None }
}

fn lit_str(v: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None }
}

fn looks_like_iri(s: &str) -> bool {
    s.contains(':') && !s.contains(' ')
}

fn create_step_record(
    conn: &mut rusqlite::Connection,
    exec_iri: &str,
    node_iri: &str,
    node_label: &str,
) -> Result<String> {
    let step_iri = format!("foundation:StepExecution_{}", Utc::now().timestamp_millis());
    let ind = Individual::new(&step_iri);
    ind.assert(conn, "foundation:StepExecution", node_label, "check_circle", "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:executesStep",
        vec![Object::Iri(node_iri.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:belongsToExecution",
        vec![Object::Iri(exec_iri.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(STATUS_IN_PROGRESS.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:stepStartedAt",
        vec![lit_datetime(Utc::now().timestamp_millis())], "process_automation")
        .map_err(|e| e.to_string())?;

    Individual::new(exec_iri)
        .append_property(conn, "foundation:hasStepExecutions",
            vec![Object::Iri(step_iri.clone())], "process_automation")
        .map_err(|e| e.to_string())?;

    Ok(step_iri)
}

fn finish_step_record(
    conn: &mut rusqlite::Connection,
    step_iri: &str,
    output: Option<&str>,
    error: Option<&str>,
) -> Result<String> {
    let ind = Individual::new(step_iri);
    let status = if error.is_some() { STATUS_FAILED } else { STATUS_COMPLETED };
    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(status.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:stepFinishedAt",
        vec![lit_datetime(Utc::now().timestamp_millis())], "process_automation")
        .map_err(|e| e.to_string())?;
    if let Some(val) = output.filter(|v| !v.is_empty()) {
        if looks_like_iri(val) {
            ind.add_property(conn, "foundation:outputValue",
                vec![Object::Iri(val.to_string())], "process_automation")
                .map_err(|e| e.to_string())?;
        } else {
            let comment_triple = crate::eavto::Triple::new(
                step_iri, "rdfs:comment",
                Object::Literal { value: val.to_string(), datatype: Some("xsd:string".to_string()), language: None },
            );
            crate::eavto::store::assert_triples(conn, &[comment_triple], "process_automation")
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some(msg) = error {
        ind.add_property(conn, "foundation:stepError",
            vec![lit_str(msg)], "process_automation")
            .map_err(|e| e.to_string())?;
    }
    Ok(step_iri.to_string())
}

fn finish_execution_record(
    conn: &mut rusqlite::Connection,
    exec_iri: &str,
    error: Option<&str>,
) -> Result<String> {
    let ind = Individual::new(exec_iri);
    let status = if error.is_some() { STATUS_FAILED } else { STATUS_COMPLETED };
    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(status.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    if let Some(msg) = error {
        ind.add_property(conn, "foundation:errorMessage",
            vec![Object::Literal { value: msg.to_string(), datatype: Some("xsd:string".to_string()), language: None }],
            "process_automation")
            .map_err(|e| e.to_string())?;
    }
    Ok(exec_iri.to_string())
}

/// Runs a BPMN process from the start, threading outputs through an ExecutionContext.
pub async fn run_process(app: &AppHandle, process_iri: &str, input_iri: Option<String>) -> Result<()> {
    let mut ctx = ExecutionContext::new();
    if let Some(iri) = input_iri {
        ctx.insert("inputIRIs".to_string(), iri);
    }
    run_process_with_context(app, process_iri, &mut ctx).await
}

/// Runs a BPMN process, propagating and updating the provided execution context.
pub async fn run_process_with_context(
    app: &AppHandle,
    process_iri: &str,
    ctx: &mut ExecutionContext,
) -> Result<()> {
    let process_iri = process_iri.to_string();
    let executor = app.state::<DbExecutor>();

    let triggered_by = ctx.get("inputIRIs").cloned();
    let exec_iri = executor
        .write({
            let process_iri = process_iri.clone();
            move |conn| {
                let exec_iri = create_execution_record(conn, &process_iri)?;
                if let Some(iri) = triggered_by {
                    Individual::new(&exec_iri)
                        .add_property(conn, "foundation:triggeredBy",
                            vec![Object::Iri(iri)], "process_automation")
                        .map_err(|e| e.to_string())?;
                }
                Ok(exec_iri)
            }
        })
        .await?;

    app.emit("automation-execution-started", serde_json::json!({
        "processIri": process_iri,
        "executionIri": exec_iri,
    })).ok();

    let nodes = executor
        .read({
            let process_iri = process_iri.clone();
            move |conn| load_flow_nodes(conn, &process_iri)
        })
        .await?;

    let run_result = execute_nodes(app, &process_iri, &exec_iri, nodes, ctx).await;

    executor
        .write({
            let exec_iri = exec_iri.clone();
            let error = run_result.as_ref().err().cloned();
            move |conn| finish_execution_record(conn, &exec_iri, error.as_deref())
        })
        .await?;

    let finished_status = if run_result.is_ok() { "completed" } else { "failed" };
    app.emit("automation-execution-finished", serde_json::json!({
        "processIri": process_iri,
        "executionIri": exec_iri,
        "status": finished_status,
        "error": run_result.as_ref().err().map(|e| e.as_str()),
    })).ok();

    run_result
}

async fn execute_nodes(
    app: &AppHandle,
    process_iri: &str,
    exec_iri: &str,
    nodes: Vec<(String, String, Option<String>)>,
    ctx: &mut ExecutionContext,
) -> Result<()> {
    let executor = app.state::<DbExecutor>();

    for (node_iri, node_type, output_key) in nodes {
        let kind = normalize_node_type(&node_type);

        if kind == "StartEvent" {
            continue;
        }

        if kind == "EndEvent" {
            log_backend("info", &format!("[executor] Process {} reached EndEvent {}", process_iri, node_iri));
            continue;
        }

        let node_label = executor
            .read({
                let node_iri = node_iri.clone();
                move |conn| {
                    Ok(query::get_by_entity_predicate(conn, &node_iri, "rdfs:label")
                        .ok()
                        .and_then(|r| r.triples.into_iter().next())
                        .and_then(|t| t.object.as_literal().map(|s| s.to_string()))
                        .unwrap_or_else(|| node_iri.clone()))
                }
            })
            .await
            .unwrap_or_else(|_| node_iri.clone());

        let step_iri = executor
            .write({
                let exec_iri = exec_iri.to_string();
                let node_iri = node_iri.clone();
                let node_label = node_label.clone();
                move |conn| create_step_record(conn, &exec_iri, &node_iri, &node_label)
            })
            .await?;

        app.emit("automation-step-progress", serde_json::json!({
            "executionIri": exec_iri,
            "stepIri": step_iri,
            "nodeIri": node_iri,
            "nodeLabel": node_label,
            "status": "started",
        })).ok();

        if !ctx.is_empty() {
            let step = step_iri.clone();
            let ctx_json = serde_json::to_string(ctx).unwrap_or_default();
            executor.write(move |conn| {
                let triple = crate::eavto::Triple::new(
                    &step,
                    "foundation:inputContext",
                    Object::Literal { value: ctx_json, datatype: Some("xsd:string".to_string()), language: None },
                );
                crate::eavto::store::assert_triples(conn, &[triple], "process_automation")
                    .map(|_| String::new())
                    .map_err(|e| e.to_string())
            }).await.ok();
        }

        let step_result = match kind {
            "RequestTask" => {
                super::request_task::execute_request_task(app, &node_iri, ctx).await
            }
            "ServiceTask" | "ScriptTask" => {
                dispatch_ai_task(app, process_iri, &node_iri, &node_type, ctx).await
            }
            "AgentTask" => {
                super::agent_task::execute_agent_task(app, &node_iri, ctx, &step_iri).await
            }
            "NOVAMessageTask" => {
                super::nova_message_task::execute_nova_message_task(app, &node_iri, ctx)
                    .await
                    .map(|_| String::new())
            }
            "SubProcess" => {
                run_sub_process(app, process_iri, &node_iri, ctx).await.map(|_| String::new())
            }
            _ => {
                log_backend("warn", &format!("[executor] Skipping unhandled node type {} ({})", node_type, node_iri));
                Ok(String::new())
            }
        };

        let (output, step_error) = match step_result {
            Ok(val) => (Some(val), None),
            Err(e) => (None, Some(e)),
        };

        executor
            .write({
                let step_iri = step_iri.clone();
                let out = output.clone();
                let err = step_error.clone();
                move |conn| finish_step_record(conn, &step_iri, out.as_deref(), err.as_deref())
            })
            .await?;

        if let Some(ref e) = step_error {
            app.emit("automation-step-progress", serde_json::json!({
                "executionIri": exec_iri,
                "stepIri": step_iri,
                "nodeIri": node_iri,
                "nodeLabel": node_label,
                "status": "failed",
                "error": e,
            })).ok();
            return Err(e.clone());
        }

        app.emit("automation-step-progress", serde_json::json!({
            "executionIri": exec_iri,
            "stepIri": step_iri,
            "nodeIri": node_iri,
            "nodeLabel": node_label,
            "status": "completed",
        })).ok();

        if let (Some(key), Some(value)) = (output_key, output) {
            ctx.insert(key, value);
        }
    }

    Ok(())
}

async fn run_sub_process(
    app: &AppHandle,
    parent_process_iri: &str,
    node_iri: &str,
    ctx: &mut ExecutionContext,
) -> Result<()> {
    let executor = app.state::<DbExecutor>();

    let called_iri = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                let result = query::get_by_entity_predicate(conn, &node_iri, "foundation:calledElement")
                    .map_err(|e| e.to_string())?;
                Ok(result
                    .triples
                    .first()
                    .and_then(|t| t.object.as_iri())
                    .map(|s| s.to_string()))
            }
        })
        .await?;

    match called_iri {
        Some(called) => {
            log_backend("info", &format!("[executor] SubProcess {} calling {}", node_iri, called));
            Box::pin(run_process_with_context(app, &called, ctx)).await
        }
        None => {
            log_backend("warn", &format!(
                "[executor] SubProcess {} has no calledElement — skipping (parent: {})",
                node_iri, parent_process_iri
            ));
            Ok(())
        }
    }
}

/// Dispatches an AI task (ServiceTask or ScriptTask) and returns its output string.
async fn dispatch_ai_task(
    app: &AppHandle,
    process_iri: &str,
    node_iri: &str,
    node_type: &str,
    ctx: &ExecutionContext,
) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let (label, description) = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                let label = query::get_by_entity_predicate(conn, &node_iri, "rdfs:label")
                    .map_err(|e| e.to_string())?
                    .triples
                    .first()
                    .and_then(|t| t.object.as_literal())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| node_iri.clone());

                let description = query::get_by_entity_predicate(conn, &node_iri, "rdfs:comment")
                    .map_err(|e| e.to_string())?
                    .triples
                    .first()
                    .and_then(|t| t.object.as_literal())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                Ok((label, description))
            }
        })
        .await?;

    let resolved_description = interpolate(&description, ctx);

    log_backend("info", &format!(
        "[executor] Dispatching {} '{}' ({}) in process {} — description: {}",
        node_type, label, node_iri, process_iri, resolved_description
    ));

    Ok(format!("output_of_{}", node_iri))
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod executor_tests;
