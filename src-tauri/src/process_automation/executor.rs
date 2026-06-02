use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::log_backend;
use crate::eavto::query;
use crate::owl::{DbExecutor, Individual, Object};

pub struct ActiveExecutions(Arc<Mutex<HashSet<String>>>);

impl ActiveExecutions {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(HashSet::new())))
    }

    fn insert(&self, iri: &str) {
        if let Ok(mut set) = self.0.lock() { set.insert(iri.to_string()); }
    }

    fn remove(&self, iri: &str) {
        if let Ok(mut set) = self.0.lock() { set.remove(iri); }
    }

    fn contains(&self, iri: &str) -> bool {
        self.0.lock().map(|s| s.contains(iri)).unwrap_or(false)
    }
}

use super::context::{evaluate_condition, reachable_from};
pub use super::context::{ExecutionContext, FlowMap, interpolate, interpolate_with_db};

type Result<T> = std::result::Result<T, String>;

/// Routing key injected into context when a SubProcess exits via a Segway.
/// The parent executor skips nodes until it reaches the one matching this IRI.
const SEGWAY_NEXT_KEY: &str = "__segwayNext";

fn load_flow_nodes(
    conn: &rusqlite::Connection,
    process_iri: &str,
) -> Result<(Vec<(String, String)>, FlowMap)> {
    let result = query::get_by_predicate_object(conn, "foundation:partOfProcess", process_iri)
        .map_err(|e| e.to_string())?;

    let mut node_meta: HashMap<String, String> = HashMap::new();
    let mut seq_flow_iris: Vec<String> = Vec::new();

    for triple in &result.triples {
        let node_iri = &triple.subject;
        let type_result = query::get_by_entity_predicate(conn, node_iri, "rdf:type")
            .map_err(|e| e.to_string())?;
        let node_type = type_result
            .triples
            .first()
            .and_then(|t| t.object.as_iri())
            .unwrap_or("foundation:automation_FlowNode")
            .to_string();

        if node_type == "foundation:automation_SequenceFlow" {
            seq_flow_iris.push(node_iri.to_string());
            continue;
        }

        node_meta.insert(node_iri.to_string(), node_type);
    }

    let mut adjacency: FlowMap = HashMap::new();
    for sf_iri in &seq_flow_iris {
        let source = query::get_by_entity_predicate(conn, sf_iri, "foundation:sourceRef")
            .map_err(|e| e.to_string())?
            .triples.first().and_then(|t| t.object.as_iri()).map(|s| s.to_string());
        let target = query::get_by_entity_predicate(conn, sf_iri, "foundation:targetRef")
            .map_err(|e| e.to_string())?
            .triples.first().and_then(|t| t.object.as_iri()).map(|s| s.to_string());
        let condition = query::get_by_entity_predicate(conn, sf_iri, "foundation:conditionExpression")
            .map_err(|e| e.to_string())?
            .triples.first().and_then(|t| t.object.as_literal()).map(|s| s.to_string());
        if let (Some(src), Some(tgt)) = (source, target) {
            adjacency.entry(src).or_default().push((tgt, condition));
        }
    }

    let mut ordered: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for iri in node_meta.keys() {
        if node_meta[iri].contains("StartEvent") {
            visited.insert(iri.clone());
            queue.push_back(iri.clone());
        }
    }

    while let Some(iri) = queue.pop_front() {
        ordered.push(iri.clone());
        if let Some(neighbors) = adjacency.get(&iri) {
            for (next, _) in neighbors {
                if node_meta.contains_key(next) && !visited.contains(next) {
                    visited.insert(next.clone());
                    queue.push_back(next.clone());
                }
            }
        }
    }

    for iri in node_meta.keys() {
        if !visited.contains(iri) {
            ordered.push(iri.clone());
        }
    }

    let nodes = ordered.into_iter()
        .filter_map(|iri| node_meta.remove(&iri).map(|t| (iri, t)))
        .collect();

    Ok((nodes, adjacency))
}


/// Returns the `segwayToNode` IRI for the Segway whose `segwayFromEndEvent` matches `end_event_iri`,
/// or None if no matching Segway exists on the SubProcess node.
fn resolve_segway(
    conn: &rusqlite::Connection,
    subprocess_iri: &str,
    end_event_iri: &str,
) -> Result<Option<String>> {
    let segway_result = query::get_by_entity_predicate(conn, subprocess_iri, "foundation:hasSegway")
        .map_err(|e| e.to_string())?;

    for triple in &segway_result.triples {
        let Some(segway_iri) = triple.object.as_iri() else { continue };

        let from_event = query::get_by_entity_predicate(conn, segway_iri, "foundation:segwayFromEndEvent")
            .map_err(|e| e.to_string())?;
        let matches = from_event.triples.first()
            .and_then(|t| t.object.as_iri())
            .map(|iri| iri == end_event_iri)
            .unwrap_or(false);

        if matches {
            let to_node = query::get_by_entity_predicate(conn, segway_iri, "foundation:segwayToNode")
                .map_err(|e| e.to_string())?;
            return Ok(to_node.triples.first()
                .and_then(|t| t.object.as_iri())
                .map(|s| s.to_string()));
        }
    }
    Ok(None)
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
    let now_ms = Utc::now().timestamp_millis();
    let exec_iri = format!("foundation:WorkflowExecution_{}", now_ms);
    let process_label = crate::owl::get_literal_property(conn, process_iri, "rdfs:label")
        .ok()
        .flatten()
        .unwrap_or_else(|| process_iri.to_string());
    let exec_label = format!("Execução: {}", process_label);
    let ind = Individual::new(&exec_iri);
    ind.assert(conn, "foundation:WorkflowExecution", &exec_label, "play_circle", "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:executesProcess",
        vec![Object::Iri(process_iri.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(STATUS_IN_PROGRESS.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:hasStartTime",
        vec![lit_datetime(now_ms)], "process_automation")
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
    let now_ms = Utc::now().timestamp_millis();
    let step_iri = format!("foundation:StepExecution_{}", now_ms);
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
        vec![lit_datetime(now_ms)], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:hasStartTime",
        vec![lit_datetime(now_ms)], "process_automation")
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
    let now_ms = Utc::now().timestamp_millis();
    let status = if error.is_some() { STATUS_FAILED } else { STATUS_COMPLETED };
    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(status.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:stepFinishedAt",
        vec![lit_datetime(now_ms)], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:hasEndTime",
        vec![lit_datetime(now_ms)], "process_automation")
        .map_err(|e| e.to_string())?;
    if let Some(val) = output.filter(|v| !v.is_empty()) {
        if looks_like_iri(val) {
            ind.add_property(conn, "foundation:outputValue",
                vec![Object::Iri(val.to_string())], "process_automation")
                .map_err(|e| e.to_string())?;
        } else {
            ind.add_property(conn, "foundation:stepOutput",
                vec![lit_str(val)], "process_automation")
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
    ind.add_property(conn, "foundation:hasEndTime",
        vec![lit_datetime(Utc::now().timestamp_millis())], "process_automation")
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
/// Set `dry_run = true` to execute read operations normally but skip all writes in CodeTask scripts.
pub async fn run_process(app: &AppHandle, process_iri: &str, input_iri: Option<String>, dry_run: bool) -> Result<()> {
    let mut ctx = ExecutionContext::new();
    if let Some(iri) = input_iri {
        ctx.insert("inputIRIs".to_string(), iri);
    }
    if dry_run {
        ctx.insert("dryRun".to_string(), "true".to_string());
    }
    run_process_with_context(app, process_iri, &mut ctx).await.map(|_| ())
}

/// Runs a BPMN process, propagating and updating the provided execution context.
/// Returns the IRI of the EndEvent that terminated the process, if any.
pub async fn run_process_with_context(
    app: &AppHandle,
    process_iri: &str,
    ctx: &mut ExecutionContext,
) -> Result<Option<String>> {
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

    if let Err(e) = app.emit("automation-execution-started", serde_json::json!({
        "processIri": process_iri,
        "executionIri": exec_iri,
    })) {
        log_backend("warn", &format!("[executor] Failed to emit automation-execution-started for {}: {}", process_iri, e));
    }

    if let Some(active) = app.try_state::<ActiveExecutions>() {
        active.insert(&exec_iri);
    }

    let (nodes, adjacency) = executor
        .read({
            let process_iri = process_iri.clone();
            move |conn| load_flow_nodes(conn, &process_iri)
        })
        .await?;

    let run_result = execute_nodes(app, &process_iri, &exec_iri, nodes, adjacency, ctx, HashSet::new(), None).await;

    if let Some(active) = app.try_state::<ActiveExecutions>() {
        active.remove(&exec_iri);
    }

    executor
        .write({
            let exec_iri = exec_iri.clone();
            let error = run_result.as_ref().err().cloned();
            move |conn| finish_execution_record(conn, &exec_iri, error.as_deref())
        })
        .await?;

    if let Some(error_msg) = run_result.as_ref().err() {
        let error_msg = error_msg.clone();
        let exec_iri_notif = exec_iri.clone();
        let process_iri_notif = process_iri.clone();
        executor
            .write(move |conn| {
                let label = crate::owl::get_literal_property(conn, &process_iri_notif, "rdfs:label")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| process_iri_notif.clone());
                create_failure_notification(conn, &exec_iri_notif, &label, &error_msg)
            })
            .await
            .ok();
    }

    let finished_status = if run_result.is_ok() { "completed" } else { "failed" };
    if let Err(e) = app.emit("automation-execution-finished", serde_json::json!({
        "processIri": process_iri,
        "executionIri": exec_iri,
        "status": finished_status,
        "error": run_result.as_ref().err().map(|e| e.as_str()),
    })) {
        log_backend("warn", &format!("[executor] Failed to emit automation-execution-finished for {}: {}", process_iri, e));
    }

    run_result
}

fn create_failure_notification(
    conn: &mut rusqlite::Connection,
    exec_iri: &str,
    process_label: &str,
    error: &str,
) -> Result<String> {
    let notif_iri = format!("foundation:AINotification_{}", chrono::Utc::now().timestamp_millis());
    let title = format!("Automation failed: {}", process_label);
    let ind = Individual::new(&notif_iri);
    ind.assert(conn, "foundation:AINotification", &title, "notifications_active", "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:notificationType",
        vec![lit_str("error")], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "rdfs:comment",
        vec![lit_str(error)], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:notificationSource",
        vec![Object::Iri(exec_iri.to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri("foundation:Pending".to_string())], "process_automation")
        .map_err(|e| e.to_string())?;
    Ok(notif_iri)
}

/// Looks up an ErrorHandler individual whose `foundation:appliesTo` points to `node_iri`.
/// Returns the `foundation:fallbackNode` IRI from that handler, if one exists.
fn resolve_error_handler(
    conn: &rusqlite::Connection,
    node_iri: &str,
) -> Result<Option<String>> {
    let handlers = query::get_by_predicate_object(conn, "foundation:appliesTo", node_iri)
        .map_err(|e| e.to_string())?;

    for triple in &handlers.triples {
        let handler_iri = &triple.subject;
        let type_result = query::get_by_entity_predicate(conn, handler_iri, "rdf:type")
            .map_err(|e| e.to_string())?;
        let is_error_handler = type_result.triples.first()
            .and_then(|t| t.object.as_iri())
            .map(|iri| iri == "foundation:ErrorHandler")
            .unwrap_or(false);

        if !is_error_handler {
            continue;
        }

        let fallback = query::get_by_entity_predicate(conn, handler_iri, "foundation:fallbackNode")
            .map_err(|e| e.to_string())?;
        if let Some(iri) = fallback.triples.first().and_then(|t| t.object.as_iri()) {
            return Ok(Some(iri.to_string()));
        }
    }

    Ok(None)
}

/// Returns `Ok(Some(end_event_iri))` when an EndEvent is reached (halting execution),
/// or `Ok(None)` when all nodes complete without hitting an EndEvent.
async fn execute_nodes(
    app: &AppHandle,
    process_iri: &str,
    exec_iri: &str,
    nodes: Vec<(String, String)>,
    adjacency: FlowMap,
    ctx: &mut ExecutionContext,
    initial_skip_set: HashSet<String>,
    resume_from: Option<String>,
) -> Result<Option<String>> {
    let executor = app.state::<DbExecutor>();
    let mut skip_set: HashSet<String> = initial_skip_set;
    let mut resuming = resume_from.is_some();

    for (node_iri, node_type) in nodes {
        if resuming {
            if resume_from.as_deref() == Some(node_iri.as_str()) {
                resuming = false;
            } else {
                continue;
            }
        }

        // Segway routing: skip nodes until we reach the one a SubProcess routed to.
        if let Some(target) = ctx.get(SEGWAY_NEXT_KEY).cloned() {
            if node_iri != target {
                continue;
            }
            ctx.remove(SEGWAY_NEXT_KEY);
        }

        if skip_set.contains(&node_iri) {
            log_backend("info", &format!("[executor] Gateway skip: {}", node_iri));
            continue;
        }

        let kind = normalize_node_type(&node_type);

        if kind == "StartEvent" {
            continue;
        }

        if kind == "EndEvent" {
            log_backend("info", &format!("[executor] Process {} reached EndEvent {}", process_iri, node_iri));
            return Ok(Some(node_iri));
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

        {
            let step = step_iri.clone();
            let ctx_json = serde_json::to_string(ctx).unwrap_or_default();
            let skip_json = serde_json::to_string(&skip_set.iter().collect::<Vec<_>>()).unwrap_or_default();
            executor.write(move |conn| {
                let triples = vec![
                    crate::eavto::Triple::new(
                        &step,
                        "foundation:inputContext",
                        Object::Literal { value: ctx_json, datatype: Some("xsd:string".to_string()), language: None },
                    ),
                    crate::eavto::Triple::new(
                        &step,
                        "foundation:inputSkipSet",
                        Object::Literal { value: skip_json, datatype: Some("xsd:string".to_string()), language: None },
                    ),
                ];
                crate::eavto::store::assert_triples(conn, &triples, "process_automation")
                    .map(|_| String::new())
                    .map_err(|e| e.to_string())
            }).await.ok();
        }

        let step_result = match kind {
            "RequestTask" => {
                super::request_task::execute_request_task(app, &node_iri, ctx).await
            }
            "CodeTask" => {
                super::code_task::execute_code_task(app, &node_iri, ctx).await
            }
            "ServiceTask" | "ScriptTask" => {
                dispatch_ai_task(app, process_iri, &node_iri, &node_type, ctx).await
            }
            "AgentTask" => {
                super::agent_task::execute_agent_task(app, &node_iri, ctx, &step_iri, exec_iri).await
            }
            "NOVAMessageTask" => {
                super::nova_message_task::execute_nova_message_task(app, &node_iri, ctx)
                    .await
                    .map(|_| String::new())
            }
            "SubProcess" => {
                run_sub_process(app, process_iri, &node_iri, ctx).await.map(|_| String::new())
            }
            "Gateway" => {
                let outgoing = adjacency.get(&node_iri).cloned().unwrap_or_default();

                let mut taken: Option<String> = None;
                for (target, cond) in &outgoing {
                    let matches = match cond {
                        Some(expr) => evaluate_condition(expr, ctx, &executor).await,
                        None => false,
                    };
                    if matches {
                        taken = Some(target.clone());
                        break;
                    }
                }
                if taken.is_none() {
                    taken = outgoing.iter().find(|(_, cond)| cond.is_none()).map(|(t, _)| t.clone());
                }

                if let Some(ref taken_target) = taken {
                    let taken_reachable = reachable_from(taken_target, &adjacency);
                    for (other_target, _) in &outgoing {
                        if other_target == taken_target { continue; }
                        for node in reachable_from(other_target, &adjacency) {
                            if !taken_reachable.contains(&node) {
                                skip_set.insert(node);
                            }
                        }
                    }
                    log_backend("info", &format!(
                        "[executor] Gateway {} → {} (skipping {} nodes)",
                        node_iri, taken_target, skip_set.len()
                    ));
                } else {
                    log_backend("warn", &format!(
                        "[executor] Gateway {} — no condition matched, executing all branches",
                        node_iri
                    ));
                }
                Ok(String::new())
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

            let fallback = executor
                .read({
                    let node_iri = node_iri.clone();
                    move |conn| resolve_error_handler(conn, &node_iri)
                })
                .await?;

            if let Some(fallback_iri) = fallback {
                log_backend("info", &format!(
                    "[executor] Node {} failed — ErrorHandler routing to {}",
                    node_iri, fallback_iri
                ));
                ctx.insert(SEGWAY_NEXT_KEY.to_string(), fallback_iri);
                continue;
            }

            return Err(e.clone());
        }

        app.emit("automation-step-progress", serde_json::json!({
            "executionIri": exec_iri,
            "stepIri": step_iri,
            "nodeIri": node_iri,
            "nodeLabel": node_label,
            "status": "completed",
        })).ok();

        if let Some(value) = output.filter(|v| !v.is_empty()) {
            ctx.insert("inputIRIs".to_string(), value);
        }

        // If the node wrote foundation:outputIRIs directly (e.g. an AgentTask that
        // calls replace_property_values on itself), inject it into the context so
        // downstream gateways can evaluate conditions against it.
        // Tries IRI first (ObjectProperty), then literal (DatatypeProperty).
        let node_iri_read = node_iri.clone();
        if let Ok(Some(val)) = executor.read(move |conn| {
            if let Ok(Some(v)) = crate::owl::get_iri_property(conn, &node_iri_read, "foundation:outputIRIs") {
                return Ok(Some(v));
            }
            crate::owl::get_literal_property(conn, &node_iri_read, "foundation:outputIRIs")
                .map_err(|e| e.to_string())
        }).await {
            ctx.insert("outputIRIs".to_string(), val);
        }
    }

    Ok(None)
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

    let Some(called) = called_iri else {
        log_backend("warn", &format!(
            "[executor] SubProcess {} has no calledElement — skipping (parent: {})",
            node_iri, parent_process_iri
        ));
        return Ok(());
    };

    log_backend("info", &format!("[executor] SubProcess {} calling {}", node_iri, called));
    let end_event_iri = Box::pin(run_process_with_context(app, &called, ctx)).await?;

    if let Some(ref end_iri) = end_event_iri {
        let next_node = executor
            .read({
                let subprocess_iri = node_iri.to_string();
                let end_iri = end_iri.clone();
                move |conn| resolve_segway(conn, &subprocess_iri, &end_iri)
            })
            .await?;

        if let Some(next_iri) = next_node {
            log_backend("info", &format!(
                "[executor] SubProcess {} routed via EndEvent {} → {}",
                node_iri, end_iri, next_iri
            ));
            ctx.insert(SEGWAY_NEXT_KEY.to_string(), next_iri);
        } else {
            log_backend("warn", &format!(
                "[executor] SubProcess {} has no Segway for EndEvent {} — continuing linearly",
                node_iri, end_iri
            ));
        }
    }

    Ok(())
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

async fn resume_workflow_execution(app: &AppHandle, exec_iri: &str) -> Result<()> {
    let executor = app.state::<DbExecutor>();

    let result = executor
        .read({
            let exec_iri = exec_iri.to_string();
            move |conn| {
                let process_iri = crate::owl::get_iri_property(conn, &exec_iri, "foundation:executesProcess")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Execution {} has no executesProcess", exec_iri))?;

                let step_iris = crate::owl::get_all_iri_properties(conn, &exec_iri, "foundation:hasStepExecutions")
                    .unwrap_or_default();

                for step_iri in step_iris {
                    let status = crate::owl::get_iri_property(conn, &step_iri, "foundation:hasStatus")
                        .map_err(|e| e.to_string())?;
                    if status.as_deref() != Some("foundation:InProgress") {
                        continue;
                    }
                    let node_iri = crate::owl::get_iri_property(conn, &step_iri, "foundation:executesStep")
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| format!("Step {} has no executesStep", step_iri))?;
                    let ctx_json = crate::owl::get_literal_property(conn, &step_iri, "foundation:inputContext")
                        .map_err(|e| e.to_string())?
                        .unwrap_or_else(|| "{}".to_string());
                    let skip_json = crate::owl::get_literal_property(conn, &step_iri, "foundation:inputSkipSet")
                        .map_err(|e| e.to_string())?
                        .unwrap_or_else(|| "[]".to_string());
                    return Ok(Some((process_iri, step_iri, node_iri, ctx_json, skip_json)));
                }
                Ok(None)
            }
        })
        .await?;

    let Some((process_iri, step_iri, node_iri, ctx_json, skip_json)) = result else {
        executor
            .write({
                let exec_iri = exec_iri.to_string();
                move |conn| finish_execution_record(conn, &exec_iri, Some("Interrupted: no recoverable step found"))
            })
            .await?;
        return Ok(());
    };

    executor
        .write({
            let step_iri = step_iri.clone();
            move |conn| finish_step_record(conn, &step_iri, None, Some("Interrupted: app was closed"))
        })
        .await?;

    let mut ctx: ExecutionContext = serde_json::from_str(&ctx_json).unwrap_or_default();
    let skip_vec: Vec<String> = serde_json::from_str(&skip_json).unwrap_or_default();
    let skip_set: HashSet<String> = skip_vec.into_iter().collect();

    log_backend("info", &format!(
        "[executor] Resuming execution {} from node {} (skip_set: {} nodes)",
        exec_iri, node_iri, skip_set.len()
    ));

    app.emit("automation-execution-started", serde_json::json!({
        "processIri": process_iri,
        "executionIri": exec_iri,
        "resumed": true,
    })).ok();

    if let Some(active) = app.try_state::<ActiveExecutions>() {
        active.insert(exec_iri);
    }

    let (nodes, adjacency) = executor
        .read({
            let process_iri = process_iri.clone();
            move |conn| load_flow_nodes(conn, &process_iri)
        })
        .await?;

    let run_result = execute_nodes(
        app, &process_iri, exec_iri, nodes, adjacency, &mut ctx,
        skip_set, Some(node_iri),
    )
    .await;

    if let Some(active) = app.try_state::<ActiveExecutions>() {
        active.remove(exec_iri);
    }

    executor
        .write({
            let exec_iri = exec_iri.to_string();
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

    run_result.map(|_| ())
}

pub async fn recover_interrupted_executions(app: &AppHandle) {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return,
    };

    let exec_iris: Vec<String> = executor
        .read(|conn| {
            let all = crate::owl::find_entities_with_property(conn, "rdf:type", "foundation:WorkflowExecution")
                .map_err(|e| e.to_string())?;
            let mut in_progress = Vec::new();
            for iri in all {
                if let Ok(Some(status)) = crate::owl::get_iri_property(conn, &iri, "foundation:hasStatus") {
                    if status == "foundation:InProgress" {
                        in_progress.push(iri);
                    }
                }
            }
            Ok(in_progress)
        })
        .await
        .unwrap_or_default();

    let exec_iris: Vec<String> = exec_iris
        .into_iter()
        .filter(|iri| {
            app.try_state::<ActiveExecutions>()
                .map(|a| !a.contains(iri))
                .unwrap_or(true)
        })
        .collect();

    if exec_iris.is_empty() {
        return;
    }

    log_backend("info", &format!("[executor] Recovering {} interrupted execution(s)", exec_iris.len()));

    for exec_iri in exec_iris {
        if let Err(e) = resume_workflow_execution(app, &exec_iri).await {
            log_backend("error", &format!("[executor] Failed to resume execution {}: {}", exec_iri, e));
        }
    }
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod executor_tests;
