use crate::owl::formula_job_store::{
    FormulaInstanceError, FormulaJobIntent, FormulaJobStatus, FormulaJobStore, FormulaResultCommit,
    NewFormulaRecalcJob,
};
use crate::owl::materialization::{
    emit_progress, MaterializationCorrelation, MaterializationTerminal,
};
use crate::owl::{Object, Triple};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager, Runtime};
use tokio::sync::{mpsc, oneshot};

#[derive(serde::Serialize, serde::Deserialize)]
struct FormulaJobData {
    id: String,
    property_iri: String,
    property_label: Option<String>,
    class_iri: String,
    class_label: Option<String>,
    instance_iri: Option<String>,
    created_at: i64,
}

impl From<NewFormulaRecalcJob> for FormulaJobData {
    fn from(job: NewFormulaRecalcJob) -> Self {
        Self {
            id: job.id,
            property_iri: job.property_iri,
            property_label: job.property_label,
            class_iri: job.class_iri,
            class_label: job.class_label,
            instance_iri: job.instance_iri,
            created_at: job.created_at,
        }
    }
}

impl From<FormulaJobData> for NewFormulaRecalcJob {
    fn from(job: FormulaJobData) -> Self {
        Self {
            id: job.id,
            property_iri: job.property_iri,
            property_label: job.property_label,
            class_iri: job.class_iri,
            class_label: job.class_label,
            instance_iri: job.instance_iri,
            created_at: job.created_at,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FormulaJobIntentData {
    id: String,
    property_iri: String,
    property_label: Option<String>,
    class_iri: String,
    class_label: Option<String>,
    instance_iri: Option<String>,
}

impl From<FormulaJobIntent> for FormulaJobIntentData {
    fn from(job: FormulaJobIntent) -> Self {
        Self {
            id: job.id,
            property_iri: job.property_iri,
            property_label: job.property_label,
            class_iri: job.class_iri,
            class_label: job.class_label,
            instance_iri: job.instance_iri,
        }
    }
}

impl From<FormulaJobIntentData> for FormulaJobIntent {
    fn from(job: FormulaJobIntentData) -> Self {
        Self {
            id: job.id,
            property_iri: job.property_iri,
            property_label: job.property_label,
            class_iri: job.class_iri,
            class_label: job.class_label,
            instance_iri: job.instance_iri,
        }
    }
}

#[derive(Debug)]
pub enum WorkerCommand {
    Enqueue {
        job_id: String,
    },
    EnqueueCorrelated {
        job_id: String,
        correlations: Vec<MaterializationCorrelation>,
        barrier_ids: Vec<u64>,
    },
}

pub struct FormulaWorker {
    pub sender: mpsc::Sender<WorkerCommand>,
    waiters: Mutex<HashMap<u64, oneshot::Sender<Result<(), MaterializationTerminal>>>>,
    remaining_jobs: Mutex<HashMap<u64, usize>>,
    next_barrier_id: AtomicU64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FormulaProgressEvent {
    pub job_id: String,
    pub property_iri: String,
    pub property_label: String,
    pub class_iri: String,
    pub class_label: String,
    pub percent: u32,
    pub status: String,
}

impl FormulaWorker {
    pub fn spawn(app: tauri::AppHandle, executor: crate::owl::DbExecutor) -> Self {
        let (tx, mut rx) = mpsc::channel::<WorkerCommand>(1024);
        let enqueue_tx = tx.clone();

        let supervisor_app = app.clone();
        let supervisor_executor = executor.clone();
        let worker_task = tauri::async_runtime::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    WorkerCommand::Enqueue { job_id } => {
                        if let Err(terminal) =
                            process_job(&app, &executor, &job_id, &enqueue_tx, &[], &[]).await
                        {
                            if let MaterializationTerminal::Failed(error) = &terminal {
                                persist_job_failure(&executor, &job_id, error).await;
                            }
                        }
                    }
                    WorkerCommand::EnqueueCorrelated {
                        job_id,
                        correlations,
                        barrier_ids,
                    } => {
                        if let Err(terminal) = process_job(
                            &app,
                            &executor,
                            &job_id,
                            &enqueue_tx,
                            &correlations,
                            &barrier_ids,
                        )
                        .await
                        {
                            if let MaterializationTerminal::Failed(error) = &terminal {
                                persist_job_failure(&executor, &job_id, error).await;
                            }
                            fail_barriers(&app, &barrier_ids, terminal);
                        }
                    }
                }
            }
        });
        tauri::async_runtime::spawn(async move {
            let failure = if let Err(error) = worker_task.await {
                Some(format!("formula worker terminated unexpectedly: {}", error))
            } else {
                Some("formula worker command channel closed".to_string())
            };
            if let Some(error) = failure {
                persist_running_jobs_failure(&supervisor_executor, &error).await;
                fail_all_waiters(&supervisor_app, MaterializationTerminal::Failed(error));
            }
        });

        FormulaWorker {
            sender: tx,
            waiters: Mutex::new(HashMap::new()),
            remaining_jobs: Mutex::new(HashMap::new()),
            next_barrier_id: AtomicU64::new(1),
        }
    }

    /// Queue every calculated property of `instance_iri`'s class and wait until
    /// they have all been recomputed.
    ///
    /// This is what lets a caller read a derived value and know it reflects the
    /// writes that just happened. Without it, a total computed from a value written
    /// a moment ago is read stale, and nothing about the read says so.
    pub async fn await_drained(
        app: &tauri::AppHandle,
        executor: &crate::owl::DbExecutor,
        instance_iri: &str,
        mut correlation: MaterializationCorrelation,
    ) -> Result<(), MaterializationTerminal> {
        let Some(worker) = app.try_state::<FormulaWorker>() else {
            return Err(MaterializationTerminal::Failed(
                "formula worker is unavailable during materialization barrier".to_string(),
            ));
        };

        correlation.source_tx = Some(executor.current_snapshot_tx().await.map_err(|error| {
            MaterializationTerminal::Failed(format!(
                "failed to capture formula source revision: {}",
                error
            ))
        })?);

        let iri = instance_iri.to_string();
        let job_ids: Vec<String> = create_class_calculated_recalc_jobs(executor, &iri)
            .await
            .map_err(|error| {
                MaterializationTerminal::Failed(format!(
                    "failed to create formula recalculation jobs: {}",
                    error
                ))
            })?;

        // Nothing derived on this class: already settled, by definition.
        if job_ids.is_empty() {
            emit_progress(
                app,
                &correlation,
                "calculated-properties",
                "completed",
                instance_iri,
                None,
                Some(0),
                Some(0),
                None,
            );
            return Ok(());
        }

        let (tx, rx) = oneshot::channel::<Result<(), MaterializationTerminal>>();
        let barrier_id = worker.next_barrier_id.fetch_add(1, Ordering::AcqRel);
        worker
            .waiters
            .lock()
            .map_err(|error| MaterializationTerminal::Failed(error.to_string()))?
            .insert(barrier_id, tx);
        worker
            .remaining_jobs
            .lock()
            .map_err(|error| MaterializationTerminal::Failed(error.to_string()))?
            .insert(barrier_id, job_ids.len());
        emit_progress(
            app,
            &correlation,
            "calculated-properties",
            "running",
            instance_iri,
            None,
            Some(0),
            Some(job_ids.len()),
            None,
        );

        for job_id in job_ids {
            if worker
                .sender
                .send(WorkerCommand::EnqueueCorrelated {
                    job_id,
                    correlations: vec![correlation.clone()],
                    barrier_ids: vec![barrier_id],
                })
                .await
                .is_err()
            {
                let terminal = MaterializationTerminal::Failed(format!(
                    "formula_worker channel closed for {}",
                    instance_iri
                ));
                fail_barriers(app, &[barrier_id], terminal.clone());
                return Err(terminal);
            }
        }

        match rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(MaterializationTerminal::Failed(format!(
                "formula worker stopped before calculating properties of {}",
                instance_iri
            ))),
        }
    }
}

fn complete_barrier_jobs(app: &tauri::AppHandle, barrier_ids: &[u64]) {
    let Some(worker) = app.try_state::<FormulaWorker>() else {
        return;
    };
    let mut completed = Vec::new();
    let mut remaining = worker
        .remaining_jobs
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for barrier_id in barrier_ids {
        if let Some(count) = remaining.get_mut(barrier_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                remaining.remove(barrier_id);
                completed.push(*barrier_id);
            }
        }
    }
    let mut waiters = worker
        .waiters
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for barrier_id in completed {
        if let Some(sender) = waiters.remove(&barrier_id) {
            let _ = sender.send(Ok(()));
        }
    }
}

fn register_barrier_jobs<R: Runtime>(
    app: &tauri::AppHandle<R>,
    barrier_ids: &[u64],
    count: usize,
) -> Result<(), String> {
    if count == 0 || barrier_ids.is_empty() {
        return Ok(());
    }
    let Some(worker) = app.try_state::<FormulaWorker>() else {
        return Err(
            "formula worker state disappeared while registering dependent work".to_string(),
        );
    };
    let mut remaining = worker
        .remaining_jobs
        .lock()
        .map_err(|error| format!("formula barrier lock failed: {}", error))?;
    for barrier_id in barrier_ids {
        let count_for_barrier = remaining.get_mut(barrier_id).ok_or_else(|| {
            format!(
                "formula barrier {} disappeared before dependent work was registered",
                barrier_id
            )
        })?;
        *count_for_barrier += count;
    }
    Ok(())
}

fn fail_barriers(app: &tauri::AppHandle, barrier_ids: &[u64], terminal: MaterializationTerminal) {
    let Some(worker) = app.try_state::<FormulaWorker>() else {
        return;
    };
    let mut senders = Vec::new();
    let mut waiters = worker
        .waiters
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut remaining = worker
        .remaining_jobs
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for barrier_id in barrier_ids {
        remaining.remove(barrier_id);
        if let Some(sender) = waiters.remove(barrier_id) {
            senders.push(sender);
        }
    }
    for sender in senders {
        let _ = sender.send(Err(terminal.clone()));
    }
}

fn fail_all_waiters<R: Runtime>(app: &tauri::AppHandle<R>, terminal: MaterializationTerminal) {
    let Some(worker) = app.try_state::<FormulaWorker>() else {
        return;
    };
    if let Ok(mut remaining) = worker.remaining_jobs.lock() {
        remaining.clear();
    }
    let mut waiters = worker
        .waiters
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let senders = waiters
        .drain()
        .map(|(_, sender)| sender)
        .collect::<Vec<_>>();
    for sender in senders {
        let _ = sender.send(Err(terminal.clone()));
    }
}

async fn process_job(
    app: &tauri::AppHandle,
    executor: &crate::owl::DbExecutor,
    job_id: &str,
    sender: &mpsc::Sender<WorkerCommand>,
    correlations: &[MaterializationCorrelation],
    barrier_ids: &[u64],
) -> Result<(), MaterializationTerminal> {
    let job_store = FormulaJobStore::new(executor.clone());
    let required_tx = correlations
        .iter()
        .filter_map(|correlation| correlation.source_tx)
        .max()
        .unwrap_or(0);
    let observed_tx = executor.current_snapshot_tx().await.map_err(|error| {
        MaterializationTerminal::Failed(format!(
            "failed to inspect formula source revision: {}",
            error
        ))
    })?;
    if observed_tx < required_tx {
        return Err(MaterializationTerminal::Failed(format!(
            "formula worker observed tx {} before required tx {}",
            observed_tx, required_tx
        )));
    }
    let job = match job_store.get(job_id.to_string()).await {
        Ok(Some(job)) => job,
        Err(error) => {
            return Err(MaterializationTerminal::Failed(format!(
                "failed to load formula job {}: {}",
                job_id, error
            )))
        }
        Ok(None) => {
            return Err(MaterializationTerminal::Failed(format!(
                "formula job {} is missing or not pending",
                job_id
            )));
        }
    };

    match job.status {
        FormulaJobStatus::Pending => {}
        FormulaJobStatus::Cancelled => {
            return Err(MaterializationTerminal::Cancelled(format!(
                "formula job {} was cancelled before processing",
                job_id
            )))
        }
        _ => {
            return Err(MaterializationTerminal::Failed(format!(
                "formula job {} is not pending",
                job_id
            )))
        }
    }

    if !job_store
        .mark_running(job_id.to_string(), now_millis())
        .await
        .unwrap_or(false)
    {
        return Err(MaterializationTerminal::Failed(format!(
            "failed to mark formula job {} running",
            job_id
        )));
    }

    let prop_iri = job.property_iri.clone();
    let class_iri = if job.class_iri.is_empty() {
        let prop = prop_iri.clone();
        let prop_for_read = prop.clone();
        executor
            .read(move |conn| Ok(fetch_domain(conn, &prop_for_read)))
            .await
            .map_err(|error| format!("failed to load formula domain for {}: {}", prop, error))?
    } else {
        job.class_iri.clone()
    };

    let instance_iris: Vec<String> = if let Some(ref inst) = job.instance_iri {
        vec![inst.clone()]
    } else {
        let ci = class_iri.clone();
        let class_for_read = ci.clone();
        let offset = job.last_offset;
        executor
            .read(move |conn| Ok(query_class_instances(conn, &class_for_read, offset)))
            .await
            .map_err(|error| format!("failed to load formula instances for {}: {}", ci, error))?
    };

    let total: i64 = if job.instance_iri.is_some() {
        1
    } else {
        let ci = class_iri.clone();
        let class_for_read = ci.clone();
        executor
            .read(move |conn| Ok(query_class_instances(conn, &class_for_read, 0).len() as i64))
            .await
            .map_err(|error| format!("failed to count formula instances for {}: {}", ci, error))?
    };

    job_store
        .set_total(job_id.to_string(), total, now_millis())
        .await
        .map_err(|error| format!("failed to persist formula job total {}: {}", job_id, error))?;

    let mut processed = job.processed;
    let mut last_offset = job.last_offset;
    let mut last_pct: u32 = (processed * 100 / total.max(1)) as u32;
    let mut cancelled = false;
    for correlation in correlations {
        emit_progress(
            app,
            correlation,
            "calculated-properties",
            "running",
            job.instance_iri.as_deref().unwrap_or(""),
            Some(&job.property_iri),
            Some(processed.max(0) as usize),
            Some(total.max(0) as usize),
            None,
        );
    }

    for instance_iri in &instance_iris {
        let instance = instance_iri.clone();
        let prop = job.property_iri.clone();
        let current_job = job_store.get(job_id.to_string()).await.map_err(|error| {
            MaterializationTerminal::Failed(format!(
                "failed to inspect formula job {}: {}",
                job_id, error
            ))
        })?;
        if current_job
            .as_ref()
            .is_some_and(|current| current.status == FormulaJobStatus::Cancelled)
        {
            cancelled = true;
            break;
        }
        let eval_result = executor
            .read(move |conn| Ok(evaluate_formula_for_instance(conn, &instance, &prop)))
            .await
            .map_err(|error| {
                MaterializationTerminal::Failed(format!(
                    "failed to evaluate formula job {}: {}",
                    job_id, error
                ))
            })?;

        match eval_result {
            Ok(value) => {
                let inst = instance_iri.clone();
                let prop = job.property_iri.clone();
                let value_for_read = value.clone();
                let result_data = executor
                    .read(move |conn| {
                        let datatype = resolve_calculated_datatype(conn, &prop);
                        let jobs = collect_instance_recalc_job_intents(conn, &inst, &prop);
                        let jobs = jobs
                            .into_iter()
                            .map(FormulaJobIntentData::from)
                            .collect::<Vec<_>>();
                        serde_json::to_string(&(value_for_read, datatype, jobs))
                            .map_err(|error| error.to_string())
                    })
                    .await
                    .map_err(|error| {
                        MaterializationTerminal::Failed(format!(
                            "failed to prepare formula result: {}",
                            error
                        ))
                    })?;
                let (fact_value, datatype, dependent_jobs): (
                    String,
                    String,
                    Vec<FormulaJobIntentData>,
                ) = serde_json::from_str(&result_data).map_err(|error| {
                    MaterializationTerminal::Failed(format!(
                        "failed to decode formula intents: {}",
                        error
                    ))
                })?;
                let fact = Triple::new(
                    instance_iri,
                    &job.property_iri,
                    Object::Literal {
                        value: fact_value,
                        datatype: Some(datatype),
                        language: None,
                    },
                );
                let dependent_jobs = dependent_jobs
                    .into_iter()
                    .map(FormulaJobIntent::from)
                    .collect::<Vec<_>>();
                let commit_id = format!("{}:{}", job_id, instance_iri);
                let dependent_job_ids = dependent_jobs
                    .iter()
                    .map(|intent| intent.id.clone())
                    .collect::<Vec<_>>();
                let committed = job_store
                    .commit_result(FormulaResultCommit {
                        commit_id,
                        fact,
                        job_intents: dependent_jobs,
                        committed_at: now_millis(),
                    })
                    .await
                    .map_err(|error| {
                        format!(
                            "failed to persist calculated value for {} / {}: {}",
                            instance_iri, job.property_iri, error
                        )
                    })?;
                let calculated_tx = committed.fact_tx;
                register_barrier_jobs(app, barrier_ids, dependent_job_ids.len())?;
                let mut dependent_correlations = correlations.to_vec();
                for correlation in &mut dependent_correlations {
                    correlation.source_tx =
                        Some(correlation.source_tx.unwrap_or(0).max(calculated_tx));
                }
                for jid in dependent_job_ids {
                    sender
                        .try_send(WorkerCommand::EnqueueCorrelated {
                            job_id: jid,
                            correlations: dependent_correlations.clone(),
                            barrier_ids: barrier_ids.to_vec(),
                        })
                        .map_err(|error| {
                            format!("failed to enqueue dependent formula job: {}", error)
                        })?;
                }
                if job.instance_iri.is_some() {
                    crate::realtime::emit_entity_updated(&app, &instance_iri);
                }
            }
            Err(msg) => {
                job_store
                    .upsert_error(FormulaInstanceError {
                        instance_iri: instance_iri.clone(),
                        property_iri: job.property_iri.clone(),
                        error_message: msg.clone(),
                        created_at: now_millis(),
                    })
                    .await
                    .map_err(|error| format!("failed to persist formula error: {}", error))?;
                return Err(MaterializationTerminal::Failed(msg));
            }
        }

        processed += 1;
        last_offset += 1;

        job_store
            .record_progress(job_id.to_string(), processed, last_offset, now_millis())
            .await
            .map_err(|error| {
                format!(
                    "failed to persist formula job progress {}: {}",
                    job_id, error
                )
            })?;

        let pct = (processed * 100 / total.max(1)) as u32;
        if pct != last_pct {
            last_pct = pct;
            let event = FormulaProgressEvent {
                job_id: job_id.to_string(),
                property_iri: job.property_iri.clone(),
                property_label: job.property_label.clone().unwrap_or_default(),
                class_iri: class_iri.clone(),
                class_label: job.class_label.clone().unwrap_or_default(),
                percent: pct,
                status: "running".to_string(),
            };
            app.emit("formula-recalc-progress", event).ok();
        }
        for correlation in correlations {
            emit_progress(
                app,
                correlation,
                "calculated-properties",
                "running",
                instance_iri,
                Some(&job.property_iri),
                Some(processed.max(0) as usize),
                Some(total.max(0) as usize),
                None,
            );
        }
    }

    let final_status = if cancelled { "cancelled" } else { "completed" };
    let transition = if cancelled {
        job_store
            .cancel(
                job_id.to_string(),
                Some("Cancelled by newer write".into()),
                now_millis(),
            )
            .await
    } else {
        job_store.complete(job_id.to_string(), now_millis()).await
    };
    transition.map_err(|error| {
        format!(
            "failed to persist formula job terminal state {}: {}",
            job_id, error
        )
    })?;

    if !cancelled {
        complete_barrier_jobs(app, barrier_ids);
    }

    let event = FormulaProgressEvent {
        job_id: job_id.to_string(),
        property_iri: job.property_iri.clone(),
        property_label: job.property_label.clone().unwrap_or_default(),
        class_iri: class_iri.clone(),
        class_label: job.class_label.clone().unwrap_or_default(),
        percent: last_pct,
        status: final_status.to_string(),
    };
    app.emit("formula-recalc-progress", event).ok();
    for correlation in correlations {
        let terminal = if cancelled { "cancelled" } else { "completed" };
        let reason = cancelled.then_some("Formula recalculation was cancelled by a newer write");
        emit_progress(
            app,
            correlation,
            "calculated-properties",
            terminal,
            job.instance_iri.as_deref().unwrap_or(""),
            Some(&job.property_iri),
            Some(processed.max(0) as usize),
            Some(total.max(0) as usize),
            reason,
        );
    }
    if cancelled {
        Err(MaterializationTerminal::Cancelled(format!(
            "formula job {} was cancelled by a newer write",
            job_id
        )))
    } else {
        Ok(())
    }
}

async fn persist_job_failure(executor: &crate::owl::DbExecutor, job_id: &str, error: &str) {
    let _ = FormulaJobStore::new(executor.clone())
        .fail(job_id.to_string(), error.to_string(), now_millis())
        .await;
}

async fn persist_running_jobs_failure(executor: &crate::owl::DbExecutor, error: &str) {
    let _ = FormulaJobStore::new(executor.clone())
        .recover_interrupted(error.to_string(), now_millis())
        .await;
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn evaluate_formula_for_instance(
    conn: &Connection,
    instance_iri: &str,
    property_iri: &str,
) -> Result<String, String> {
    // Respect the immutable TX model: the predicate with the highest tx is the current intent.
    // A property may have historical aggregation triples with retracted=0 that predate a formula
    // update — checking COUNT(*) would incorrectly prefer the stale aggregation.
    let mode = crate::owl::query_by_entity(conn, property_iri)
        .ok()
        .and_then(|result| {
            result
                .triples
                .into_iter()
                .filter(|triple| {
                    triple.predicate == "foundation:formula"
                        || triple.predicate == "foundation:aggregation"
                })
                .max_by_key(|triple| triple.tx)
                .map(|triple| triple.predicate)
        });

    match mode.as_deref() {
        Some("foundation:aggregation") => {
            crate::owl::aggregation::evaluate_aggregation_for_instance(
                conn,
                instance_iri,
                property_iri,
            )
        }
        Some("foundation:formula") => {
            crate::owl::formula::evaluate_formula_for_instance_raw(conn, instance_iri, property_iri)
        }
        _ => Err(format!(
            "Property '{}' has no formula or aggregation",
            property_iri
        )),
    }
}

/// Decides the `object_datatype` to store for a calculated value, from the
/// property's declared `rdfs:range`. Numbers stay numbers (integer ranges become
/// `xsd:integer`, the rest `xsd:decimal`) so downstream formats them correctly;
/// text, boolean and date ranges are stored as themselves.
fn resolve_calculated_datatype(conn: &Connection, property_iri: &str) -> String {
    const INTEGER_RANGES: &[&str] = &[
        "xsd:integer",
        "xsd:int",
        "xsd:long",
        "xsd:short",
        "xsd:byte",
        "xsd:nonNegativeInteger",
        "xsd:positiveInteger",
    ];
    let range: Option<String> =
        crate::owl::query_by_entity_predicate(conn, property_iri, "rdfs:range")
            .ok()
            .and_then(|r| r.triples.into_iter().next())
            .and_then(|t| t.object.as_iri().map(|s| s.to_string()));
    match range.as_deref() {
        Some(r) if INTEGER_RANGES.contains(&r) => "xsd:integer".to_string(),
        // A calculated property is no longer always a number: a formula can build
        // text or answer a yes/no. Storing "Overdue" as xsd:decimal would make every
        // reader downstream try to parse it as one.
        Some("xsd:string") => "xsd:string".to_string(),
        Some("xsd:boolean") => "xsd:boolean".to_string(),
        Some("xsd:date") => "xsd:date".to_string(),
        Some("xsd:dateTime") => "xsd:dateTime".to_string(),
        _ => "xsd:decimal".to_string(),
    }
}

fn fetch_label(conn: &Connection, iri: &str) -> String {
    crate::owl::query_by_entity_predicate(conn, iri, "rdfs:label")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_literal().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn fetch_domain(conn: &Connection, iri: &str) -> String {
    crate::owl::query_by_entity_predicate(conn, iri, "rdfs:domain")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_iri().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn query_class_instances(conn: &Connection, class_iri: &str, offset: i64) -> Vec<String> {
    crate::owl::query_by_predicate_object(conn, "rdf:type", class_iri)
        .map(|result| {
            let mut instances = result
                .triples
                .into_iter()
                .map(|triple| triple.subject)
                .collect::<Vec<_>>();
            instances.sort();
            instances.dedup();
            instances.into_iter().skip(offset.max(0) as usize).collect()
        })
        .unwrap_or_default()
}

fn query_formula_properties_by_reference(
    conn: &Connection,
    placeholder: &str,
) -> Vec<(String, String, String)> {
    ["foundation:formula", "foundation:aggregation"]
        .into_iter()
        .flat_map(|predicate| {
            crate::owl::query_by_predicate(conn, predicate)
                .map(|result| result.triples)
                .unwrap_or_default()
                .into_iter()
                .filter_map(move |triple| {
                    triple
                        .object
                        .as_literal()
                        .map(|formula| (triple.subject, formula.to_string(), predicate.to_string()))
                })
                .filter(|(_, formula, _)| formula.contains(placeholder))
                .collect::<Vec<_>>()
        })
        .collect()
}

pub async fn cancel_and_create_property_job(
    executor: &crate::owl::DbExecutor,
    property_iri: &str,
) -> Result<String, String> {
    let store = FormulaJobStore::new(executor.clone());
    store
        .cancel_active_for_property(property_iri.to_string(), None, now_millis())
        .await?;
    let property = property_iri.to_string();
    let job: NewFormulaRecalcJob = executor
        .read(move |conn| {
            let class_iri = fetch_domain(conn, &property);
            let class_label = (!class_iri.is_empty()).then(|| fetch_label(conn, &class_iri));
            let now = now_millis();
            Ok(NewFormulaRecalcJob {
                id: format!("job_{}_{}", now, property.replace(':', "_")),
                property_iri: property.clone(),
                property_label: Some(fetch_label(conn, &property)),
                class_iri,
                class_label,
                instance_iri: None,
                created_at: now,
            })
        })
        .await?;
    let id = job.id.clone();
    store.enqueue(job).await?;
    Ok(id)
}

/// Find all (aggregation_property_iri, source_prop_iri) pairs where the aggregation formula
/// references `subprop_iri` as its sub-property. Used to trigger reverse cache invalidation
/// when a sub-property value changes on a related instance.
fn find_aggregation_props_by_subprop(
    conn: &Connection,
    subprop_iri: &str,
) -> Vec<(String, String)> {
    crate::owl::query_by_predicate(conn, "foundation:aggregation")
        .map(|result| {
            result
                .triples
                .into_iter()
                .filter_map(|triple| {
                    let formula = triple.object.as_literal()?;
                    crate::owl::aggregation::parse_aggregation_call(formula.trim())
                        .ok()
                        .filter(|call| call.sub_prop.as_deref() == Some(subprop_iri))
                        .map(|call| (triple.subject, call.source_prop))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Create recalc jobs for all parent instances that aggregate a sub-property that just changed.
///
/// When `changed_subprop_iri` changes on `changed_instance_iri`, this function finds all
/// aggregation properties whose sub-property is `changed_subprop_iri`, then for each one
/// locates parent instances that link to `changed_instance_iri` via the source ObjectProperty,
/// and creates recalc jobs for those parents.
fn collect_reverse_aggregation_recalc_jobs(
    conn: &Connection,
    changed_instance_iri: &str,
    changed_subprop_iri: &str,
) -> Vec<NewFormulaRecalcJob> {
    let agg_props = find_aggregation_props_by_subprop(conn, changed_subprop_iri);
    let now = now_millis();
    let mut jobs = Vec::new();

    for (agg_prop_iri, source_prop_iri) in agg_props {
        // Determine traversal direction from the formula's OWNER class, not the changed
        // instance type. When source_prop has the same class on both sides (e.g.
        // previousMonthBudget: MonthlyBudget→MonthlyBudget), comparing domain to the
        // instance type is ambiguous and picks the wrong direction.
        //
        // - If the aggregation owner sits at source_prop.domain, the formula walks
        //   Owner --source_prop--> Target (forward). The changed instance is the Target,
        //   so we find Owners by reverse lookup (WHERE object = changed_instance).
        // - Otherwise the formula uses inverse navigation (Target --source_prop--> Owner,
        //   e.g. Obligation aggregating SOMA({{settles}}.amount) where Payment.settles
        //   points TO Obligation). The changed instance is the source, so we find Owners
        //   by forward lookup (WHERE subject = changed_instance).
        let source_domain = fetch_domain(conn, &source_prop_iri);
        let agg_owner_class = fetch_domain(conn, &agg_prop_iri);
        let owner_at_source_domain = !source_domain.is_empty()
            && !agg_owner_class.is_empty()
            && agg_owner_class == source_domain;
        let prop_from_instance = !owner_at_source_domain;

        // The direction above is only trustworthy when BOTH domains are declared.
        // With either missing, the comparison degrades to "not equal" and always
        // picks the same branch — so an undeclared ontology silently produces no
        // job at all, and the aggregate never updates.
        let direction_is_grounded = !source_domain.is_empty() && !agg_owner_class.is_empty();

        let lookup_forward = |conn: &Connection| -> Vec<String> {
            crate::owl::query_by_entity_predicate(conn, changed_instance_iri, &source_prop_iri)
                .map(|result| {
                    result
                        .triples
                        .into_iter()
                        .filter_map(|triple| triple.object.as_iri().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        let lookup_reverse = |conn: &Connection| -> Vec<String> {
            crate::owl::query_by_predicate_object(conn, &source_prop_iri, changed_instance_iri)
                .map(|result| {
                    result
                        .triples
                        .into_iter()
                        .map(|triple| triple.subject)
                        .collect()
                })
                .unwrap_or_default()
        };

        let mut parent_iris: Vec<String> = if prop_from_instance {
            lookup_forward(conn)
        } else {
            lookup_reverse(conn)
        };

        // Ungrounded guess that found nothing: the link exists in the other
        // direction. Try it rather than dropping the recalculation on the floor.
        if parent_iris.is_empty() && !direction_is_grounded {
            parent_iris = if prop_from_instance {
                lookup_reverse(conn)
            } else {
                lookup_forward(conn)
            };
        }

        let property_label = fetch_label(conn, &agg_prop_iri);
        let class_iri = fetch_domain(conn, &agg_prop_iri);
        let class_label = if class_iri.is_empty() {
            String::new()
        } else {
            fetch_label(conn, &class_iri)
        };

        for parent_iri in &parent_iris {
            let job_id = format!(
                "job_{}_{}_{}",
                now,
                agg_prop_iri.replace(':', "_"),
                parent_iri.replace(':', "_")
            );
            jobs.push(NewFormulaRecalcJob {
                id: job_id,
                property_iri: agg_prop_iri.clone(),
                property_label: Some(property_label.clone()),
                class_iri: class_iri.clone(),
                class_label: Some(class_label.clone()),
                instance_iri: Some(parent_iri.clone()),
                created_at: now,
            });
        }
    }

    jobs
}

fn entity_exists(conn: &Connection, iri: &str) -> bool {
    crate::owl::query_by_entity_predicate(conn, iri, "rdf:type")
        .map(|result| !result.triples.is_empty())
        .unwrap_or(false)
}

/// Queues a whole-class recompute for every property whose formula reads the clock.
///
/// These values go stale on their own: nothing writes to the instance, but "days
/// overdue" is wrong the moment the date rolls over. Every other calculated value
/// is refreshed by the write that invalidated it — these are the only ones with no
/// write to react to, so they need a clock of their own.
fn collect_clock_dependent_recalc_jobs(conn: &Connection) -> Vec<NewFormulaRecalcJob> {
    let calculated = crate::owl::query_by_predicate(conn, "foundation:formula")
        .map(|result| result.triples)
        .unwrap_or_default();
    let mut jobs = Vec::new();

    let now = now_millis();
    for triple in calculated {
        let prop_iri = triple.subject;
        let Some(formula) = triple.object.as_literal() else {
            continue;
        };
        if !crate::owl::expr::depends_on_clock(&formula) {
            continue;
        }
        let class_iri = fetch_domain(conn, &prop_iri);
        if class_iri.is_empty() {
            continue;
        }
        // instance_iri stays NULL: this is the whole-class recompute path, which
        // pages through instances instead of loading them all at once.
        let job_id = format!("job_clock_{}_{}", now, prop_iri.replace(':', "_"));
        jobs.push(NewFormulaRecalcJob {
            id: job_id,
            property_iri: prop_iri.clone(),
            property_label: Some(fetch_label(conn, &prop_iri)),
            class_iri: class_iri.clone(),
            class_label: Some(fetch_label(conn, &class_iri)),
            instance_iri: None,
            created_at: now,
        });
    }

    jobs
}

/// Refreshes clock-dependent values once at startup and once a day after that.
pub fn spawn_daily_clock_refresh(app: tauri::AppHandle, executor: crate::owl::DbExecutor) {
    tauri::async_runtime::spawn(async move {
        loop {
            let job_ids = create_clock_dependent_recalc_jobs(&executor)
                .await
                .unwrap_or_default();

            if !job_ids.is_empty() {
                if let Some(worker) = app.try_state::<FormulaWorker>() {
                    for job_id in job_ids {
                        let _ = worker.sender.send(WorkerCommand::Enqueue { job_id }).await;
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}

/// Every calculated property declared on an instance's own class, queued for
/// recompute against that instance.
///
/// The reference-driven path only fires when someone names the property that
/// changed, which misses the case the automation engine needs: a node writes to
/// the control instance and the NEXT node has to read totals derived from that
/// write. Here the trigger is the class, not the write — so the engine can settle
/// every derived value it owns before advancing, without knowing what changed.
fn collect_class_calculated_recalc_jobs(
    conn: &Connection,
    instance_iri: &str,
) -> Vec<NewFormulaRecalcJob> {
    let types = crate::owl::query_by_entity_predicate(conn, instance_iri, "rdf:type")
        .map(|result| {
            result
                .triples
                .into_iter()
                .filter_map(|triple| triple.object.as_iri().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut jobs = Vec::new();
    if types.is_empty() {
        return jobs;
    }

    let now = now_millis();
    for class_iri in types {
        let props = ["foundation:formula", "foundation:aggregation"]
            .into_iter()
            .flat_map(|predicate| {
                crate::owl::query_by_predicate(conn, predicate)
                    .map(|result| result.triples)
                    .unwrap_or_default()
            })
            .filter(|triple| {
                crate::owl::query_by_entity_predicate(conn, &triple.subject, "rdfs:domain")
                    .map(|result| {
                        result
                            .triples
                            .iter()
                            .any(|domain| domain.object.as_iri() == Some(class_iri.as_str()))
                    })
                    .unwrap_or(false)
            })
            .map(|triple| triple.subject)
            .collect::<Vec<_>>();

        for prop_iri in props {
            let property_label = fetch_label(conn, &prop_iri);
            let class_label = fetch_label(conn, &class_iri);
            let job_id = format!(
                "job_{}_{}_{}",
                now,
                prop_iri.replace(':', "_"),
                instance_iri.replace(':', "_"),
            );
            jobs.push(NewFormulaRecalcJob {
                id: job_id,
                property_iri: prop_iri,
                property_label: Some(property_label),
                class_iri: class_iri.clone(),
                class_label: Some(class_label),
                instance_iri: Some(instance_iri.to_string()),
                created_at: now,
            });
        }
    }

    jobs
}

pub(crate) fn collect_instance_recalc_job_intents(
    conn: &Connection,
    instance_iri: &str,
    changed_property_iri: &str,
) -> Vec<FormulaJobIntent> {
    let placeholder = format!("{{{{{}}}}}", changed_property_iri);
    let formula_rows = query_formula_properties_by_reference(conn, &placeholder);

    let now = now_millis();
    let mut intents = Vec::new();
    let instance_type = crate::owl::query_by_entity_predicate(conn, instance_iri, "rdf:type")
        .ok()
        .and_then(|result| {
            result
                .triples
                .into_iter()
                .filter_map(|triple| triple.object.as_iri().map(str::to_string))
                .next()
        });

    for (formula_prop_iri, _, pred) in formula_rows {
        let target_iri: String = if pred == "foundation:aggregation" {
            // An aggregate lives on ONE side of a link and the write happens on the
            // other. Getting the side wrong recalculates an instance nobody asked
            // about and leaves the real total stale.
            let formula_domain = fetch_domain(conn, &formula_prop_iri);
            let changed_domain = fetch_domain(conn, changed_property_iri);
            let is_inverse = match (&instance_type, formula_domain.as_str()) {
                // The aggregate is declared on a different class than the instance
                // that changed: the instance sits on the far side of the link.
                (Some(t), d) if !d.is_empty() => t.as_str() != d,
                // The aggregate's own domain is undeclared, so it can't name the
                // side. Fall back to the link being aggregated: when the changed
                // property is declared on the changed instance's class, this
                // instance is the SOURCE and the aggregate lives on its target.
                (Some(t), _) => !changed_domain.is_empty() && changed_domain == *t,
                _ => false,
            };
            if is_inverse {
                match crate::owl::query_by_entity_predicate(
                    conn,
                    instance_iri,
                    changed_property_iri,
                )
                .ok()
                .and_then(|result| {
                    result
                        .triples
                        .into_iter()
                        .filter_map(|triple| triple.object.as_iri().map(str::to_string))
                        .next()
                }) {
                    // Only hop when the link actually landed on an entity. An
                    // aggregation over a literal-valued property has no far side,
                    // and a literal treated as an IRI would target a job at nothing.
                    Some(parent) if entity_exists(conn, &parent) => parent,
                    _ => continue,
                }
            } else {
                instance_iri.to_string()
            }
        } else {
            instance_iri.to_string()
        };

        let property_label = fetch_label(conn, &formula_prop_iri);
        let class_iri = fetch_domain(conn, &formula_prop_iri);
        let class_label = if class_iri.is_empty() {
            String::new()
        } else {
            fetch_label(conn, &class_iri)
        };

        let job_id = format!(
            "job_{}_{}_{}",
            now,
            formula_prop_iri.replace(':', "_"),
            target_iri.replace(':', "_"),
        );
        intents.push(FormulaJobIntent {
            id: job_id,
            property_iri: formula_prop_iri,
            property_label: Some(property_label),
            class_iri,
            class_label: Some(class_label),
            instance_iri: Some(target_iri),
        });
    }

    intents.extend(
        collect_reverse_aggregation_recalc_jobs(conn, instance_iri, changed_property_iri)
            .into_iter()
            .map(|job| FormulaJobIntent {
                id: job.id,
                property_iri: job.property_iri,
                property_label: job.property_label,
                class_iri: job.class_iri,
                class_label: job.class_label,
                instance_iri: job.instance_iri,
            }),
    );

    intents
}

async fn enqueue_jobs(
    executor: &crate::owl::DbExecutor,
    jobs: Vec<NewFormulaRecalcJob>,
) -> Result<Vec<String>, String> {
    let store = FormulaJobStore::new(executor.clone());
    let mut ids = Vec::with_capacity(jobs.len());
    for job in jobs {
        ids.push(job.id.clone());
        store.enqueue(job).await?;
    }
    Ok(ids)
}

pub async fn create_instance_recalc_jobs(
    executor: &crate::owl::DbExecutor,
    instance_iri: &str,
    changed_property_iri: &str,
) -> Result<Vec<String>, String> {
    let instance = instance_iri.to_string();
    let changed = changed_property_iri.to_string();
    let jobs = executor
        .read(move |conn| {
            let now = now_millis();
            let intents = collect_instance_recalc_job_intents(conn, &instance, &changed);
            let jobs = intents
                .into_iter()
                .map(|intent| NewFormulaRecalcJob {
                    id: intent.id,
                    property_iri: intent.property_iri,
                    property_label: intent.property_label,
                    class_iri: intent.class_iri,
                    class_label: intent.class_label,
                    instance_iri: intent.instance_iri,
                    created_at: now,
                })
                .collect::<Vec<_>>();
            serde_json::to_string(
                &jobs
                    .into_iter()
                    .map(FormulaJobData::from)
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| error.to_string())
        })
        .await?;
    let jobs = serde_json::from_str::<Vec<FormulaJobData>>(&jobs)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(NewFormulaRecalcJob::from)
        .collect();
    enqueue_jobs(executor, jobs).await
}

async fn create_class_calculated_recalc_jobs(
    executor: &crate::owl::DbExecutor,
    instance_iri: &str,
) -> Result<Vec<String>, String> {
    let instance = instance_iri.to_string();
    let jobs = executor
        .read(move |conn| {
            serde_json::to_string(
                &collect_class_calculated_recalc_jobs(conn, &instance)
                    .into_iter()
                    .map(FormulaJobData::from)
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| error.to_string())
        })
        .await?;
    let jobs = serde_json::from_str::<Vec<FormulaJobData>>(&jobs)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(NewFormulaRecalcJob::from)
        .collect();
    enqueue_jobs(executor, jobs).await
}

async fn create_clock_dependent_recalc_jobs(
    executor: &crate::owl::DbExecutor,
) -> Result<Vec<String>, String> {
    let jobs = executor
        .read(|conn| {
            serde_json::to_string(
                &collect_clock_dependent_recalc_jobs(conn)
                    .into_iter()
                    .map(FormulaJobData::from)
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| error.to_string())
        })
        .await?;
    let jobs = serde_json::from_str::<Vec<FormulaJobData>>(&jobs)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(NewFormulaRecalcJob::from)
        .collect();
    enqueue_jobs(executor, jobs).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE formula_recalc_jobs (
                id              TEXT    PRIMARY KEY,
                property_iri    TEXT    NOT NULL,
                property_label  TEXT,
                class_iri       TEXT    NOT NULL,
                class_label     TEXT,
                status          TEXT    NOT NULL DEFAULT 'pending',
                total           INTEGER NOT NULL DEFAULT 0,
                processed       INTEGER NOT NULL DEFAULT 0,
                last_offset     INTEGER NOT NULL DEFAULT 0,
                error_message   TEXT,
                created_at      INTEGER NOT NULL,
                updated_at      INTEGER NOT NULL,
                instance_iri    TEXT
            );
            CREATE TABLE formula_instance_errors (
                instance_iri    TEXT NOT NULL,
                property_iri    TEXT NOT NULL,
                error_message   TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                PRIMARY KEY (instance_iri, property_iri)
            );
            CREATE TABLE transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                origin TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE origins (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE triples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                subject TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object TEXT,
                object_value TEXT,
                object_type TEXT NOT NULL,
                object_datatype TEXT,
                object_language TEXT,
                object_number REAL,
                object_integer INTEGER,
                object_datetime INTEGER,
                object_boolean INTEGER,
                tx INTEGER NOT NULL DEFAULT 0,
                origin_id INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL DEFAULT 0,
                retracted INTEGER NOT NULL DEFAULT 0,
                is_current INTEGER NOT NULL DEFAULT 1
            );
            CREATE VIEW IF NOT EXISTS triples_current AS
            SELECT subject, predicate, object, object_value, object_datatype, object_language,
                   object_number, object_integer, object_boolean, tx, origin_id, object_type, created_at
            FROM triples
            WHERE is_current = 1 AND retracted = 0;
        ").unwrap();
        conn
    }

    #[test]
    fn recursively_enqueued_jobs_increase_the_same_barrier_and_missing_barrier_fails() {
        let app = tauri::test::mock_app();
        let (sender, _receiver) = mpsc::channel(1);
        let worker = FormulaWorker {
            sender,
            waiters: Mutex::new(HashMap::new()),
            remaining_jobs: Mutex::new(HashMap::from([(17, 2)])),
            next_barrier_id: AtomicU64::new(1),
        };
        app.manage(worker);

        register_barrier_jobs(&app.handle(), &[17], 3).unwrap();
        assert_eq!(
            app.state::<FormulaWorker>()
                .remaining_jobs
                .lock()
                .unwrap()
                .get(&17),
            Some(&5)
        );
        assert!(register_barrier_jobs(&app.handle(), &[18], 1).is_err());
    }

    #[tokio::test]
    async fn worker_termination_failure_resolves_formula_barrier() {
        let app = tauri::test::mock_app();
        let (sender, _receiver) = mpsc::channel(1);
        let (waiter, receiver) = oneshot::channel();
        let worker = FormulaWorker {
            sender,
            waiters: Mutex::new(HashMap::from([(23, waiter)])),
            remaining_jobs: Mutex::new(HashMap::from([(23, 1)])),
            next_barrier_id: AtomicU64::new(1),
        };
        app.manage(worker);

        fail_all_waiters(
            app.handle(),
            MaterializationTerminal::Failed("formula worker terminated unexpectedly: panic".into()),
        );
        assert_eq!(
            receiver.await.unwrap(),
            Err(MaterializationTerminal::Failed(
                "formula worker terminated unexpectedly: panic".into()
            ))
        );
        assert!(app
            .state::<FormulaWorker>()
            .remaining_jobs
            .lock()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_now_millis_is_positive() {
        assert!(now_millis() > 0);
    }

    #[test]
    fn test_now_millis_increases_over_time() {
        let t1 = now_millis();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let t2 = now_millis();
        assert!(t2 >= t1);
    }

    #[test]
    fn test_create_reverse_aggregation_recalc_jobs_creates_job_for_parent() {
        let conn = setup_test_db();

        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, origin_id, tx, created_at, retracted) \
             VALUES ('p:total', 'foundation:aggregation', 'SOMA({{p:hasItem}}.p:subVal)', 'literal', 1, 1, 0, 0)",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, origin_id, tx, created_at, retracted) \
             VALUES ('inst:parent', 'p:hasItem', 'inst:child1', 'iri', 1, 1, 0, 0)",
            [],
        ).unwrap();

        let jobs = collect_reverse_aggregation_recalc_jobs(&conn, "inst:child1", "p:subVal");
        assert!(
            !jobs.is_empty(),
            "expected at least one recalc job for parent"
        );
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].instance_iri.as_deref(), Some("inst:parent"));
        assert_eq!(jobs[0].property_iri, "p:total");
    }

    fn triple(
        conn: &Connection,
        subject: &str,
        predicate: &str,
        object: Option<&str>,
        value: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_type) \
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![
                subject,
                predicate,
                object,
                value,
                if object.is_some() { "iri" } else { "literal" },
            ],
        )
        .unwrap();
    }

    #[test]
    fn class_discovery_queues_every_calculated_property_of_the_instance_class() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:Run_1",
            "rdf:type",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:total",
            "rdfs:domain",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:total",
            "foundation:formula",
            None,
            Some("{{foundation:a}} + {{foundation:b}}"),
        );
        triple(
            &conn,
            "foundation:seen",
            "rdfs:domain",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:seen",
            "foundation:aggregation",
            None,
            Some("CONTAR({{foundation:items}})"),
        );

        let jobs = collect_class_calculated_recalc_jobs(&conn, "foundation:Run_1");

        assert_eq!(
            jobs.len(),
            2,
            "both the formula and the aggregation must be queued"
        );
        assert_eq!(
            jobs.iter()
                .filter(|job| job.instance_iri.as_deref() == Some("foundation:Run_1"))
                .count(),
            2,
            "jobs must target the instance that was passed in"
        );
    }

    #[test]
    fn class_discovery_ignores_calculated_properties_of_other_classes() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:Run_1",
            "rdf:type",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:otherTotal",
            "rdfs:domain",
            Some("foundation:SomethingElse"),
            None,
        );
        triple(
            &conn,
            "foundation:otherTotal",
            "foundation:formula",
            None,
            Some("{{foundation:x}} * 2"),
        );

        assert!(collect_class_calculated_recalc_jobs(&conn, "foundation:Run_1").is_empty());
    }

    #[test]
    fn class_discovery_is_empty_for_an_untyped_instance() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:total",
            "rdfs:domain",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:total",
            "foundation:formula",
            None,
            Some("{{foundation:a}}"),
        );

        assert!(collect_class_calculated_recalc_jobs(&conn, "foundation:Nothing").is_empty());
    }

    #[test]
    fn class_discovery_ignores_plain_properties() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:Run_1",
            "rdf:type",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:label",
            "rdfs:domain",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:label",
            "rdfs:range",
            Some("xsd:string"),
            None,
        );

        assert!(
            collect_class_calculated_recalc_jobs(&conn, "foundation:Run_1").is_empty(),
            "a property with no formula or aggregation is not calculated"
        );
    }

    #[test]
    fn clock_dependent_formulas_get_a_whole_class_refresh_job() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:daysOverdue",
            "rdfs:domain",
            Some("foundation:Task"),
            None,
        );
        triple(
            &conn,
            "foundation:daysOverdue",
            "foundation:formula",
            None,
            Some("DAYS_SINCE({{foundation:dueDate}})"),
        );

        let jobs = collect_clock_dependent_recalc_jobs(&conn);

        assert_eq!(jobs.len(), 1);
        assert!(
            jobs[0].instance_iri.is_none(),
            "a clock refresh covers the whole class, not one instance"
        );
    }

    #[test]
    fn a_formula_that_does_not_read_the_clock_is_left_alone() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:total",
            "rdfs:domain",
            Some("foundation:Task"),
            None,
        );
        triple(
            &conn,
            "foundation:total",
            "foundation:formula",
            None,
            Some("{{foundation:a}} + {{foundation:b}}"),
        );

        assert!(
            collect_clock_dependent_recalc_jobs(&conn).is_empty(),
            "refreshing a value nothing changed is wasted work on every instance of the class"
        );
    }

    #[test]
    fn a_clock_formula_with_no_declared_domain_is_skipped_rather_than_guessed() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:orphan",
            "foundation:formula",
            None,
            Some("TODAY()"),
        );

        assert!(collect_clock_dependent_recalc_jobs(&conn).is_empty());
    }
}
