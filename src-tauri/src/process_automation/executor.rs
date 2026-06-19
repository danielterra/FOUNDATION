use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use chrono::Utc;
use futures_util::FutureExt;
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
pub use super::context::{ExecutionContext, FlowMap, interpolate};
pub use super::template_render::interpolate_with_db;

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
    let exec_label = format!("Execution: {}", process_label);
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

/// Creates the control instance for a new execution and links it to the WorkflowExecution.
///
/// `controlClass` MUST be set on the process. A new individual of that class is created with
/// `hasStatus=InProgress` and `executesProcess=process_iri`, and `controlInstance` is written
/// on the `exec_iri`. The IRI of the typed control instance is returned.
///
/// Returns a validation error if `controlClass` is absent — the automation cannot execute.
fn create_control_instance(
    conn: &mut rusqlite::Connection,
    exec_iri: &str,
    process_iri: &str,
) -> Result<String> {
    let class_iri = crate::owl::get_iri_property(conn, process_iri, "foundation:controlClass")
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!(
            "Automation {} has no controlClass defined and cannot execute. Configure controlClass before starting.",
            process_iri
        ))?;

    let now_ms = Utc::now().timestamp_millis();
    let class_local = class_iri.rsplit_once(':').map(|(_, l)| l).unwrap_or(&class_iri);
    let instance_iri = format!("foundation:{}__{}", class_local, now_ms);

    let process_label = crate::owl::get_literal_property(conn, process_iri, "rdfs:label")
        .ok()
        .flatten()
        .unwrap_or_else(|| process_iri.to_string());

    let ind = Individual::new(&instance_iri);
    ind.assert(conn, &class_iri, &process_label, "play_circle", "process_automation")
        .map_err(|e| e.to_string())?;
    Individual::add_iri_value(conn, &instance_iri, "foundation:hasStatus", STATUS_IN_PROGRESS, "process_automation")
        .map_err(|e| e.to_string())?;
    Individual::add_iri_value(conn, &instance_iri, "foundation:executesProcess", process_iri, "process_automation")
        .map_err(|e| e.to_string())?;

    Individual::new(exec_iri)
        .add_property(conn, "foundation:controlInstance",
            vec![Object::Iri(instance_iri.clone())], "process_automation")
        .map_err(|e| e.to_string())?;

    Ok(instance_iri)
}

fn lit_datetime(ms: i64) -> Object {
    let dt = chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now);
    Object::Literal { value: dt.to_rfc3339(), datatype: Some("xsd:dateTime".to_string()), language: None }
}

fn lit_str(v: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None }
}

pub(super) fn looks_like_iri(s: &str) -> bool {
    s.contains(':') && !s.contains(' ')
}

/// Strips leading and trailing punctuation that prose or markdown can attach to
/// an IRI token (backticks, brackets, parentheses, quotes, commas, periods,
/// semicolons). The inner colon that separates namespace from local-name is
/// never removed: `strip_token_punctuation` only peels characters from the
/// edges, so `foundation:Task_123` is left intact while `` `foundation:Task_123` ``
/// becomes `foundation:Task_123`.
pub(super) fn strip_token_punctuation(s: &str) -> &str {
    s.trim_matches(|c: char| matches!(c, '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | '.' | ';' | ':'))
}

/// Parses `controlInstanceDefaults` JSON from `start_event_iri` and writes each key→value
/// pair onto `instance_iri`. Values that look like IRIs are written as ObjectProperty triples;
/// all others are written as xsd:string literals.
///
/// A missing, empty, or malformed defaults JSON is non-fatal: the instance is left unseeded
/// and a warning is logged. Nothing is written for absent/null values.
fn seed_control_instance_defaults(
    conn: &mut rusqlite::Connection,
    instance_iri: &str,
    start_event_iri: &str,
) -> Result<()> {
    let defaults_json = match crate::owl::get_literal_property(conn, start_event_iri, "foundation:controlInstanceDefaults")
        .map_err(|e| e.to_string())?
    {
        Some(v) if !v.trim().is_empty() => v,
        _ => return Ok(()),
    };

    let map: std::collections::HashMap<String, serde_json::Value> =
        match serde_json::from_str(&defaults_json) {
            Ok(m) => m,
            Err(e) => {
                log_backend("warn", &format!(
                    "[executor] invalid controlInstanceDefaults JSON in {}: {} — instance created without defaults",
                    start_event_iri, e
                ));
                return Ok(());
            }
        };

    for (prop_iri, val) in &map {
        let value_str = match val {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if value_str.is_empty() {
            continue;
        }
        if looks_like_iri(&value_str) {
            crate::owl::replace_all_property_iris(conn, instance_iri, prop_iri, &[value_str.as_str()], "process_automation")
                .map_err(|e| e.to_string())?;
        } else {
            crate::owl::replace_all_property_literals(conn, instance_iri, prop_iri, &[value_str.as_str()], "process_automation")
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

pub(super) fn split_iri_lines(s: &str) -> Option<Vec<String>> {
    let lines: Vec<String> = s
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    if lines.iter().all(|l| looks_like_iri(l)) {
        Some(lines)
    } else {
        None
    }
}

fn create_step_record(
    conn: &mut rusqlite::Connection,
    exec_iri: &str,
    node_iri: &str,
    node_label: &str,
    control_instance_iri: Option<&str>,
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
    if let Some(ci) = control_instance_iri {
        ind.add_property(conn, "foundation:controlInstance",
            vec![Object::Iri(ci.to_string())], "process_automation")
            .map_err(|e| e.to_string())?;
    }
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
        match split_iri_lines(val) {
            Some(iris) => {
                let iri_objects: Vec<Object> = iris.into_iter()
                    .map(Object::Iri)
                    .collect();
                ind.add_property(conn, "foundation:executionOutput",
                    iri_objects, "process_automation")
                    .map_err(|e| e.to_string())?;
            }
            None => {
                ind.add_property(conn, "foundation:executionSummary",
                    vec![lit_str(val)], "process_automation")
                    .map_err(|e| e.to_string())?;
                let embedded_iris: Vec<Object> = val
                    .split_whitespace()
                    .filter_map(|token| {
                        let clean = strip_token_punctuation(token);
                        if !looks_like_iri(clean) {
                            return None;
                        }
                        let exists = crate::owl::get_iri_property(conn, clean, "rdf:type")
                            .map(|opt| opt.is_some())
                            .unwrap_or(false);
                        if exists { Some(Object::Iri(clean.to_string())) } else { None }
                    })
                    .collect();
                if !embedded_iris.is_empty() {
                    ind.add_property(conn, "foundation:executionOutput",
                        embedded_iris, "process_automation")
                        .map_err(|e| e.to_string())?;
                }
            }
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

/// Runs a BPMN process triggered by a TimerStartEvent, seeding controlInstanceDefaults
/// onto the newly-created control instance before the first node executes.
/// Only timer-triggered dispatches go through this path; external/manual runs use run_process.
pub async fn run_process_from_timer(app: &AppHandle, process_iri: &str, start_event_iri: &str) -> Result<()> {
    let mut ctx = ExecutionContext::new();
    ctx.insert("__timerStartEvent".to_string(), start_event_iri.to_string());
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
    let init_json = executor
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
                let control_instance_iri = match create_control_instance(conn, &exec_iri, &process_iri) {
                    Ok(iri) => iri,
                    Err(e) => {
                        let _ = finish_execution_record(conn, &exec_iri, Some(&e));
                        return Err(e);
                    }
                };
                serde_json::to_string(&(exec_iri, control_instance_iri))
                    .map_err(|e| e.to_string())
            }
        })
        .await?;
    let (exec_iri, control_instance_iri): (String, String) =
        serde_json::from_str(&init_json).map_err(|e| e.to_string())?;

    ctx.insert("self".to_string(), control_instance_iri.clone());

    if let Some(start_event_iri) = ctx.get("__timerStartEvent").cloned() {
        let instance_iri_clone = control_instance_iri.clone();
        executor
            .write(move |conn| {
                seed_control_instance_defaults(conn, &instance_iri_clone, &start_event_iri)?;
                Ok(String::new())
            })
            .await?;
    }

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
            let control_instance_iri = control_instance_iri.clone();
            let error = run_result.as_ref().err().cloned();
            move |conn| {
                finish_execution_record(conn, &exec_iri, error.as_deref())?;
                let terminal_status = if error.is_none() { STATUS_COMPLETED } else { STATUS_FAILED };
                crate::owl::replace_all_property_iris(conn, &control_instance_iri, "foundation:hasStatus", &[terminal_status], "process_automation")
                    .map_err(|e| e.to_string())?;
                Ok(exec_iri)
            }
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

        let control_instance_iri = ctx.get("self").cloned();
        let iteration_of = ctx.get("__iterationOf").cloned();
        let iteration_index: Option<usize> = ctx.get("__iterationIndex")
            .and_then(|v| v.parse().ok());
        let iteration_total: Option<usize> = ctx.get("__iterationTotal")
            .and_then(|v| v.parse().ok());
        let step_iri = executor
            .write({
                let exec_iri = exec_iri.to_string();
                let node_iri = node_iri.clone();
                let node_label = node_label.clone();
                move |conn| create_step_record(conn, &exec_iri, &node_iri, &node_label, control_instance_iri.as_deref())
            })
            .await?;

        {
            let mut payload = serde_json::json!({
                "executionIri": exec_iri,
                "stepIri": step_iri,
                "nodeIri": node_iri,
                "nodeLabel": node_label,
                "status": "started",
            });
            if let Some(ref iri) = iteration_of {
                payload["iterationOf"] = serde_json::Value::String(iri.clone());
                if let Some(idx) = iteration_index {
                    payload["iterationIndex"] = serde_json::Value::Number(idx.into());
                }
                if let Some(total) = iteration_total {
                    payload["iterationTotal"] = serde_json::Value::Number(total.into());
                }
            }
            app.emit("automation-step-progress", payload).ok();
        }

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
            "TemplateTask" => {
                super::template_task::execute_template_task(app, &node_iri, ctx)
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

        // Wait for the QueryWorker to recompute all query/formula-derived properties
        // on the control instance before advancing to the next node. Without this
        // barrier, a headless execution would read stale (empty) query-property values
        // because `entity-changed-internal` is async and the worker may not have
        // finished materialising when the next node reads `ctx["self"]`.
        if let Some(ref ci_iri) = ctx.get("self").cloned() {
            if let Some(worker) = app.try_state::<crate::owl::query_worker::QueryWorker>() {
                if let Err(timeout_msg) = worker.await_drained(ci_iri).await {
                    return Err(timeout_msg);
                }
            }
        }

        if let Some(ref e) = step_error {
            {
                let mut payload = serde_json::json!({
                    "executionIri": exec_iri,
                    "stepIri": step_iri,
                    "nodeIri": node_iri,
                    "nodeLabel": node_label,
                    "status": "failed",
                    "error": e,
                });
                if let Some(ref iri) = iteration_of {
                    payload["iterationOf"] = serde_json::Value::String(iri.clone());
                    if let Some(idx) = iteration_index {
                        payload["iterationIndex"] = serde_json::Value::Number(idx.into());
                    }
                    if let Some(total) = iteration_total {
                        payload["iterationTotal"] = serde_json::Value::Number(total.into());
                    }
                }
                app.emit("automation-step-progress", payload).ok();
            }

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

        {
            let mut payload = serde_json::json!({
                "executionIri": exec_iri,
                "stepIri": step_iri,
                "nodeIri": node_iri,
                "nodeLabel": node_label,
                "status": "completed",
            });
            if let Some(ref iri) = iteration_of {
                payload["iterationOf"] = serde_json::Value::String(iri.clone());
                if let Some(idx) = iteration_index {
                    payload["iterationIndex"] = serde_json::Value::Number(idx.into());
                }
                if let Some(total) = iteration_total {
                    payload["iterationTotal"] = serde_json::Value::Number(total.into());
                }
            }
            app.emit("automation-step-progress", payload).ok();
        }

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

    let (called_iri, loop_collection, is_sequential, max_concurrency) = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                let called = crate::owl::get_iri_property(conn, &node_iri, "foundation:calledElement")
                    .map_err(|e| e.to_string())?;
                let loop_collection = crate::owl::get_literal_property(conn, &node_iri, "foundation:loopCollection")
                    .map_err(|e| e.to_string())?;
                let is_sequential = crate::owl::get_literal_property(conn, &node_iri, "foundation:isSequential")
                    .map_err(|e| e.to_string())?
                    .map(|v| v != "false")
                    .unwrap_or(true);
                let max_concurrency = crate::owl::get_literal_property(conn, &node_iri, "foundation:maxConcurrency")
                    .map_err(|e| e.to_string())?
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(4);
                Ok((called, loop_collection, is_sequential, max_concurrency))
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

    let Some(collection_predicate) = loop_collection else {
        // Single-instance path: unchanged behaviour.
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

        return Ok(());
    };

    // Loop path: iterate over the collection stored in the parent control instance.
    let parent_self_iri = ctx.get("self").cloned().unwrap_or_default();
    let items: Vec<String> = executor
        .read({
            let parent_self_iri = parent_self_iri.clone();
            let collection_predicate = collection_predicate.clone();
            move |conn| {
                crate::owl::get_all_property_values(conn, &parent_self_iri, &collection_predicate)
                    .map_err(|e| e.to_string())
            }
        })
        .await?;

    if items.is_empty() {
        log_backend("info", &format!(
            "[executor] SubProcess {} loop on '{}': collection empty — 0 iterations, continuing",
            node_iri, collection_predicate
        ));
        return Ok(());
    }

    log_backend("info", &format!(
        "[executor] SubProcess {} loop on '{}': {} items, sequential={}, max_concurrency={}",
        node_iri, collection_predicate, items.len(), is_sequential, max_concurrency
    ));

    // Each iteration result: (child_control_instance_iri_or_empty, optional_error).
    let mut iteration_results: Vec<(String, Option<String>)> = Vec::with_capacity(items.len());

    if is_sequential {
        for (idx, item) in items.iter().enumerate() {
            let mut child_ctx = ctx.clone();
            child_ctx.insert("inputIRIs".to_string(), item.clone());
            child_ctx.insert("__iterationOf".to_string(), parent_self_iri.clone());
            child_ctx.insert("__iterationIndex".to_string(), idx.to_string());
            child_ctx.insert("__iterationTotal".to_string(), items.len().to_string());

            log_backend("info", &format!(
                "[executor] SubProcess {} loop iteration {}/{}: item={}",
                node_iri, idx + 1, items.len(), item
            ));

            let outcome = Box::pin(run_process_with_context(app, &called, &mut child_ctx)).await;
            let child_self = child_ctx.get("self").cloned().unwrap_or_default();

            match outcome {
                Ok(_) => iteration_results.push((child_self, None)),
                Err(e) => {
                    log_backend("warn", &format!(
                        "[executor] SubProcess {} loop iteration {} failed: {}",
                        node_iri, idx + 1, e
                    ));
                    iteration_results.push((child_self, Some(e)));
                }
            }
        }
    } else {
        // Parallel path: run up to max_concurrency iterations concurrently per window.
        // Uses chunked join_all (no Send required) instead of tokio::spawn.
        let total = items.len();
        for (chunk_start, chunk) in items.chunks(max_concurrency).enumerate() {
            let base_idx = chunk_start * max_concurrency;
            let mut child_ctxs: Vec<ExecutionContext> = chunk
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let mut c = ctx.clone();
                    c.insert("inputIRIs".to_string(), item.clone());
                    c.insert("__iterationOf".to_string(), parent_self_iri.clone());
                    c.insert("__iterationIndex".to_string(), (base_idx + i).to_string());
                    c.insert("__iterationTotal".to_string(), total.to_string());
                    c
                })
                .collect();

            for (i, ctx_ref) in child_ctxs.iter().enumerate() {
                log_backend("info", &format!(
                    "[executor] SubProcess {} loop iteration {}/{}: item={}",
                    node_iri, base_idx + i + 1, total,
                    ctx_ref.get("inputIRIs").cloned().unwrap_or_default()
                ));
            }

            // Drive all futures in this window concurrently on the current task.
            // Each future has its own child_ctx so there is no shared mutable state.
            // AssertUnwindSafe + catch_unwind isolates panics inside a single iteration:
            // a panic is mapped to Err for that slot and the join_all continues normally.
            let futures: Vec<_> = child_ctxs
                .iter_mut()
                .map(|c| {
                    let fut = Box::pin(run_process_with_context(app, &called, c));
                    std::panic::AssertUnwindSafe(fut).catch_unwind()
                })
                .collect();

            let chunk_results = futures_util::future::join_all(futures).await;

            for (i, unwind_result) in chunk_results.into_iter().enumerate() {
                let child_self = child_ctxs[i].get("self").cloned().unwrap_or_default();
                let outcome: Result<Option<String>> = match unwind_result {
                    Ok(r) => r,
                    Err(panic_payload) => {
                        let msg = panic_payload
                            .downcast_ref::<&str>()
                            .copied()
                            .or_else(|| panic_payload.downcast_ref::<String>().map(|s| s.as_str()))
                            .unwrap_or("unknown panic");
                        Err(format!("iteration panicked: {}", msg))
                    }
                };
                match outcome {
                    Ok(_) => iteration_results.push((child_self, None)),
                    Err(e) => {
                        log_backend("warn", &format!(
                            "[executor] SubProcess {} loop iteration {} failed: {}",
                            node_iri, base_idx + i + 1, e
                        ));
                        iteration_results.push((child_self, Some(e)));
                    }
                }
            }
        }
    }

    // Link each child control instance back to the parent via foundation:iterationOf.
    let parent_self_for_link = parent_self_iri.clone();
    let children_for_link: Vec<String> = iteration_results
        .iter()
        .filter(|(iri, _)| !iri.is_empty())
        .map(|(iri, _)| iri.clone())
        .collect();

    if !children_for_link.is_empty() {
        let job_ids_json = executor
            .write({
                let parent_self_for_link = parent_self_for_link.clone();
                move |conn| {
                    let mut all_job_ids: Vec<String> = Vec::new();
                    for child_iri in &children_for_link {
                        Individual::add_iri_value(
                            conn,
                            child_iri,
                            "foundation:iterationOf",
                            &parent_self_for_link,
                            "process_automation",
                        )
                        .map_err(|e| e.to_string())?;
                        let job_ids = crate::owl::formula_worker::create_instance_recalc_jobs(
                            conn,
                            child_iri,
                            "foundation:iterationOf",
                        );
                        all_job_ids.extend(job_ids);
                    }
                    serde_json::to_string(&all_job_ids).map_err(|e| e.to_string())
                }
            })
            .await?;

        let job_ids: Vec<String> = serde_json::from_str(&job_ids_json).unwrap_or_default();
        if let Some(worker) = app.try_state::<crate::owl::formula_worker::FormulaWorker>() {
            for job_id in job_ids {
                let _ = worker.sender.try_send(
                    crate::owl::formula_worker::WorkerCommand::Enqueue { job_id },
                );
            }
        }
    }

    let failure_count = iteration_results.iter().filter(|(_, e)| e.is_some()).count();
    if failure_count > 0 {
        log_backend("warn", &format!(
            "[executor] SubProcess {} loop completed: {}/{} iterations failed (errors captured for US3 aggregation)",
            node_iri, failure_count, iteration_results.len()
        ));
    } else {
        log_backend("info", &format!(
            "[executor] SubProcess {} loop completed: all {} iterations succeeded",
            node_iri, iteration_results.len()
        ));
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

    let control_self = executor
        .read({
            let exec_iri = exec_iri.to_string();
            move |conn| {
                crate::owl::get_iri_property(conn, &exec_iri, "foundation:controlInstance")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!(
                        "Execution {} has no controlInstance — cannot resume without a typed instance.",
                        exec_iri
                    ))
            }
        })
        .await?;
    ctx.insert("self".to_string(), control_self);

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
            move |conn| {
                finish_execution_record(conn, &exec_iri, error.as_deref())?;
                let terminal_status = if error.is_none() { STATUS_COMPLETED } else { STATUS_FAILED };
                if let Ok(Some(ci_iri)) = crate::owl::get_iri_property(conn, &exec_iri, "foundation:controlInstance") {
                    crate::owl::replace_all_property_iris(conn, &ci_iri, "foundation:hasStatus", &[terminal_status], "process_automation")
                        .map_err(|e| e.to_string())?;
                }
                Ok(exec_iri)
            }
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
