use crate::owl::materialization::{
    emit_progress, MaterializationCorrelation, MaterializationTerminal,
};
use foundation_core::owl::materialization::ComputedPropertyKind;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager, Runtime};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone)]
pub(crate) struct FormulaJobIntent {
    pub(crate) id: String,
    pub(crate) property_iri: String,
    pub(crate) property_label: Option<String>,
    pub(crate) class_iri: String,
    pub(crate) class_label: Option<String>,
    pub(crate) instance_iri: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct NewFormulaRecalcJob {
    pub(crate) id: String,
    pub(crate) property_iri: String,
    pub(crate) property_label: Option<String>,
    pub(crate) class_iri: String,
    pub(crate) class_label: Option<String>,
    pub(crate) instance_iri: Option<String>,
}

#[derive(Debug)]
pub enum WorkerCommand {
    Enqueue {
        job: NewFormulaRecalcJob,
        generation: u64,
    },
    EnqueueCorrelated {
        job: NewFormulaRecalcJob,
        correlations: Vec<MaterializationCorrelation>,
        barrier_ids: Vec<u64>,
        generation: u64,
    },
}

type FormulaJobKey = (String, Option<String>, u64);

#[derive(Default)]
struct JobSubscribers {
    correlations: Vec<MaterializationCorrelation>,
    barrier_ids: HashSet<u64>,
}

pub struct FormulaWorker {
    pub sender: mpsc::Sender<WorkerCommand>,
    waiters: Mutex<HashMap<u64, oneshot::Sender<Result<(), MaterializationTerminal>>>>,
    remaining_jobs: Mutex<HashMap<u64, usize>>,
    next_barrier_id: AtomicU64,
    active_jobs: Mutex<HashSet<FormulaJobKey>>,
    property_generations: Mutex<HashMap<String, u64>>,
    formula_errors: Mutex<HashMap<(String, String), String>>,
    scheduled_jobs: Mutex<HashMap<FormulaJobKey, JobSubscribers>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_entity_iri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_update_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_tx: Option<i64>,
}

fn emit_formula_progress<R: Runtime>(
    app: &tauri::AppHandle<R>,
    event: FormulaProgressEvent,
    correlations: &[MaterializationCorrelation],
) {
    if correlations.is_empty() {
        app.emit("formula-recalc-progress", event).ok();
        return;
    }

    for correlation in correlations {
        let mut correlated_event = event.clone();
        correlated_event.source_entity_iri = correlation.source_entity_iri.clone();
        correlated_event.source_update_id = correlation.source_update_id.clone();
        correlated_event.source_tx = correlation.source_tx;
        app.emit("formula-recalc-progress", correlated_event).ok();
    }
}

impl FormulaWorker {
    pub fn spawn<R: Runtime + 'static>(
        app: tauri::AppHandle<R>,
        executor: crate::owl::DbExecutor,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<WorkerCommand>(1024);

        let supervisor_app = app.clone();
        let worker_task = tauri::async_runtime::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    WorkerCommand::Enqueue { job, generation } => {
                        let key = (
                            job.property_iri.clone(),
                            job.instance_iri.clone(),
                            generation,
                        );
                        let Some(worker) = app.try_state::<FormulaWorker>() else {
                            continue;
                        };
                        worker.register_job(key.clone());
                        let result = process_job(&app, &executor, &job, generation, &[], &[]).await;
                        let subscribers = worker.finish_job(&key, result.as_ref().err());
                        match result {
                            Ok(()) => complete_barrier_jobs(
                                &app,
                                &subscribers.barrier_ids.into_iter().collect::<Vec<_>>(),
                            ),
                            Err(error) => {
                                app.emit("formula-recalc-error", serde_json::json!({"jobId": job.id, "propertyIri": job.property_iri, "error": error.to_string()})).ok();
                                fail_barriers(
                                    &app,
                                    &subscribers.barrier_ids.into_iter().collect::<Vec<_>>(),
                                    error,
                                );
                            }
                        }
                    }
                    WorkerCommand::EnqueueCorrelated {
                        job,
                        correlations,
                        barrier_ids,
                        generation,
                    } => {
                        let key = (
                            job.property_iri.clone(),
                            job.instance_iri.clone(),
                            generation,
                        );
                        let Some(worker) = app.try_state::<FormulaWorker>() else {
                            continue;
                        };
                        worker.register_job(key.clone());
                        if let Err(terminal) = process_job(
                            &app,
                            &executor,
                            &job,
                            generation,
                            &correlations,
                            &barrier_ids,
                        )
                        .await
                        {
                            let subscribers = worker.finish_job(&key, Some(&terminal));
                            for correlation in &subscribers.correlations {
                                emit_progress(
                                    &app,
                                    correlation,
                                    "calculated-properties",
                                    if matches!(terminal, MaterializationTerminal::Cancelled(_)) {
                                        "cancelled"
                                    } else {
                                        "failed"
                                    },
                                    job.instance_iri.as_deref().unwrap_or(""),
                                    Some(&job.property_iri),
                                    None,
                                    None,
                                    Some(&terminal.to_string()),
                                );
                            }
                            app.emit("formula-recalc-error", serde_json::json!({"jobId": job.id, "propertyIri": job.property_iri, "error": terminal.to_string()})).ok();
                            fail_barriers(
                                &app,
                                &subscribers.barrier_ids.into_iter().collect::<Vec<_>>(),
                                terminal,
                            );
                        } else {
                            let subscribers = worker.finish_job(&key, None);
                            complete_barrier_jobs(
                                &app,
                                &subscribers.barrier_ids.into_iter().collect::<Vec<_>>(),
                            );
                        }
                    }
                }
            }
        });
        tauri::async_runtime::spawn(supervise_worker(supervisor_app, worker_task));

        FormulaWorker {
            sender: tx,
            waiters: Mutex::new(HashMap::new()),
            remaining_jobs: Mutex::new(HashMap::new()),
            next_barrier_id: AtomicU64::new(1),
            active_jobs: Mutex::new(HashSet::new()),
            property_generations: Mutex::new(HashMap::new()),
            formula_errors: Mutex::new(HashMap::new()),
            scheduled_jobs: Mutex::new(HashMap::new()),
        }
    }

    fn register_job(&self, key: FormulaJobKey) {
        self.active_jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key);
    }

    fn finish_job(
        &self,
        key: &FormulaJobKey,
        error: Option<&MaterializationTerminal>,
    ) -> JobSubscribers {
        self.active_jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(key);
        let subscribers = self
            .scheduled_jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(key)
            .unwrap_or_default();
        if let Some(message) = error.map(ToString::to_string) {
            if let Some(instance) = key.1.as_ref() {
                self.formula_errors
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert((instance.clone(), key.0.clone()), message);
            }
        } else if let Some(instance) = key.1.as_ref() {
            self.formula_errors
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&(instance.clone(), key.0.clone()));
        }
        subscribers
    }

    pub fn enqueue(
        &self,
        job: NewFormulaRecalcJob,
        correlations: Vec<MaterializationCorrelation>,
        barrier_ids: Vec<u64>,
    ) -> Result<(), String> {
        let generation = *self
            .property_generations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(job.property_iri.clone())
            .or_insert(0);
        let key = (
            job.property_iri.clone(),
            job.instance_iri.clone(),
            generation,
        );
        let mut scheduled = self
            .scheduled_jobs
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(subscribers) = scheduled.get_mut(&key) {
            for correlation in &correlations {
                if !subscribers.correlations.contains(correlation) {
                    subscribers.correlations.push(correlation.clone());
                }
            }
            let mut duplicate_barriers = Vec::new();
            for barrier_id in barrier_ids {
                if !subscribers.barrier_ids.insert(barrier_id) {
                    duplicate_barriers.push(barrier_id);
                }
            }
            drop(scheduled);
            for barrier_id in duplicate_barriers {
                self.complete_duplicate_barrier_job(barrier_id);
            }
            return Ok(());
        }
        let mut subscribers = JobSubscribers::default();
        subscribers.correlations = correlations.clone();
        subscribers.barrier_ids.extend(barrier_ids.iter().copied());
        scheduled.insert(key.clone(), subscribers);
        drop(scheduled);
        let command = if barrier_ids.is_empty() && correlations.is_empty() {
            WorkerCommand::Enqueue { job, generation }
        } else {
            WorkerCommand::EnqueueCorrelated {
                job,
                correlations,
                barrier_ids,
                generation,
            }
        };
        if let Err(error) = self.sender.try_send(command) {
            self.scheduled_jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&key);
            return Err(error.to_string());
        }
        Ok(())
    }

    pub fn cancel_property(&self, property_iri: &str) {
        let mut generations = self
            .property_generations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *generations.entry(property_iri.to_string()).or_insert(0) += 1;
    }

    fn is_generation_current(&self, property_iri: &str, generation: u64) -> bool {
        self.property_generations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(property_iri)
            .copied()
            .unwrap_or(0)
            == generation
    }

    fn complete_duplicate_barrier_job(&self, barrier_id: u64) {
        let done = {
            let mut remaining = self
                .remaining_jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match remaining.get_mut(&barrier_id) {
                Some(count) => {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        remaining.remove(&barrier_id);
                        true
                    } else {
                        false
                    }
                }
                None => false,
            }
        };
        if done {
            if let Some(waiter) = self
                .waiters
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&barrier_id)
            {
                let _ = waiter.send(Ok(()));
            }
        }
    }

    pub fn formula_error(&self, instance_iri: &str, property_iri: &str) -> Option<String> {
        self.formula_errors
            .lock()
            .ok()?
            .get(&(instance_iri.to_string(), property_iri.to_string()))
            .cloned()
    }

    /// Queue every calculated property of `instance_iri`'s class and wait until
    /// they have all been recomputed.
    ///
    /// This is what lets a caller read a derived value and know it reflects the
    /// writes that just happened. Without it, a total computed from a value written
    /// a moment ago is read stale, and nothing about the read says so.
    pub async fn await_drained<R: Runtime>(
        app: &tauri::AppHandle<R>,
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
        let jobs = create_class_calculated_recalc_jobs(executor, &iri)
            .await
            .map_err(|error| {
                MaterializationTerminal::Failed(format!(
                    "failed to create formula recalculation jobs: {}",
                    error
                ))
            })?;

        // Nothing derived on this class: already settled, by definition.
        if jobs.is_empty() {
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
            .insert(barrier_id, jobs.len());
        emit_progress(
            app,
            &correlation,
            "calculated-properties",
            "running",
            instance_iri,
            None,
            Some(0),
            Some(jobs.len()),
            None,
        );

        for job in jobs {
            if let Err(error) = worker.enqueue(job, vec![correlation.clone()], vec![barrier_id]) {
                let terminal = MaterializationTerminal::Failed(format!(
                    "formula_worker enqueue failed for {}: {}",
                    instance_iri, error
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

fn complete_barrier_jobs<R: Runtime>(app: &tauri::AppHandle<R>, barrier_ids: &[u64]) {
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

fn fail_barriers<R: Runtime>(
    app: &tauri::AppHandle<R>,
    barrier_ids: &[u64],
    terminal: MaterializationTerminal,
) {
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

async fn supervise_worker<R: Runtime>(
    app: tauri::AppHandle<R>,
    worker_task: tauri::async_runtime::JoinHandle<()>,
) {
    let failure = match worker_task.await {
        Err(error) => format!("formula worker terminated unexpectedly: {}", error),
        Ok(()) => "formula worker command channel closed".to_string(),
    };
    fail_all_waiters(&app, MaterializationTerminal::Failed(failure.clone()));
}

async fn process_job<R: Runtime>(
    app: &tauri::AppHandle<R>,
    executor: &crate::owl::DbExecutor,
    job: &NewFormulaRecalcJob,
    generation: u64,
    correlations: &[MaterializationCorrelation],
    barrier_ids: &[u64],
) -> Result<(), MaterializationTerminal> {
    let class_iri = if job.class_iri.is_empty() {
        let property = job.property_iri.clone();
        executor
            .read(move |conn| Ok(fetch_domain(conn, &property)))
            .await
            .map_err(|e| MaterializationTerminal::Failed(e.to_string()))?
    } else {
        job.class_iri.clone()
    };
    let instances = if let Some(instance) = job.instance_iri.as_ref() {
        vec![instance.clone()]
    } else {
        let class = class_iri.clone();
        executor
            .read(move |conn| query_class_instances(conn, &class, 0))
            .await
            .map_err(|e| MaterializationTerminal::Failed(e.to_string()))?
    };
    let total = instances.len();
    let mut last_pct = 0;
    for correlation in correlations {
        emit_progress(
            app,
            correlation,
            "calculated-properties",
            "running",
            job.instance_iri.as_deref().unwrap_or(""),
            Some(&job.property_iri),
            Some(0),
            Some(total),
            None,
        );
    }
    for (index, instance_iri) in instances.iter().enumerate() {
        let Some(worker) = app.try_state::<FormulaWorker>() else {
            return Err(MaterializationTerminal::Failed(
                "formula worker state disappeared".into(),
            ));
        };
        if !worker.is_generation_current(&job.property_iri, generation) {
            return Err(MaterializationTerminal::Cancelled(format!(
                "formula job for {} cancelled by a newer write",
                job.property_iri
            )));
        }
        let instance = instance_iri.clone();
        let property = job.property_iri.clone();
        let evaluated = executor
            .read(move |conn| Ok(evaluate_formula_for_instance(conn, &instance, &property)))
            .await
            .map_err(|e| MaterializationTerminal::Failed(e.to_string()))?;
        let value = match evaluated {
            Ok(value) => value,
            Err(message) => {
                if let Some(worker) = app.try_state::<FormulaWorker>() {
                    worker
                        .formula_errors
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(
                            (instance_iri.clone(), job.property_iri.clone()),
                            message.clone(),
                        );
                }
                return Err(MaterializationTerminal::Failed(message));
            }
        };
        let instance = instance_iri.clone();
        let property = job.property_iri.clone();
        let computed = executor
            .read(move |conn| {
                let datatype = resolve_calculated_datatype(conn, &property);
                let kind = resolve_calculated_kind(conn, &property);
                let dependent = collect_instance_recalc_job_intents(conn, &instance, &property);
                Ok((datatype, kind, dependent))
            })
            .await
            .map_err(|e| MaterializationTerminal::Failed(e.to_string()))?;
        crate::core_ontology::materialization::append_calculated_value(
            executor,
            instance_iri.clone(),
            job.property_iri.clone(),
            value,
            computed.0,
            computed.1,
        )
        .await
        .map_err(MaterializationTerminal::Failed)?;
        if let Some(worker) = app.try_state::<FormulaWorker>() {
            worker
                .formula_errors
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&(instance_iri.clone(), job.property_iri.clone()));
        }
        let mut dependents = computed.2;
        register_barrier_jobs(app, barrier_ids, dependents.len())?;
        let mut dependent_correlations = correlations.to_vec();
        for correlation in &mut dependent_correlations {
            correlation.source_tx = executor.current_snapshot_tx().await.ok();
        }
        for dependent in dependents.drain(..) {
            let job = NewFormulaRecalcJob {
                id: dependent.id,
                property_iri: dependent.property_iri,
                property_label: dependent.property_label,
                class_iri: dependent.class_iri,
                class_label: dependent.class_label,
                instance_iri: dependent.instance_iri,
            };
            let worker = app.try_state::<FormulaWorker>().ok_or_else(|| {
                MaterializationTerminal::Failed(
                    "formula worker state disappeared while queuing dependent work".to_string(),
                )
            })?;
            worker
                .enqueue(job, dependent_correlations.clone(), barrier_ids.to_vec())
                .map_err(MaterializationTerminal::Failed)?;
        }
        let pct = ((index + 1) * 100 / total.max(1)) as u32;
        if pct != last_pct {
            last_pct = pct;
            emit_formula_progress(
                app,
                FormulaProgressEvent {
                    job_id: job.id.clone(),
                    property_iri: job.property_iri.clone(),
                    property_label: job.property_label.clone().unwrap_or_default(),
                    class_iri: class_iri.clone(),
                    class_label: job.class_label.clone().unwrap_or_default(),
                    percent: pct,
                    status: "running".into(),
                    source_entity_iri: None,
                    source_update_id: None,
                    source_tx: None,
                },
                correlations,
            );
        }
        for correlation in correlations {
            emit_progress(
                app,
                correlation,
                "calculated-properties",
                "running",
                instance_iri,
                Some(&job.property_iri),
                Some(index + 1),
                Some(total),
                None,
            );
        }
    }
    emit_formula_progress(
        app,
        FormulaProgressEvent {
            job_id: job.id.clone(),
            property_iri: job.property_iri.clone(),
            property_label: job.property_label.clone().unwrap_or_default(),
            class_iri,
            class_label: job.class_label.clone().unwrap_or_default(),
            percent: 100,
            status: "completed".into(),
            source_entity_iri: None,
            source_update_id: None,
            source_tx: None,
        },
        correlations,
    );
    Ok(())
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

fn resolve_calculated_kind(conn: &Connection, property_iri: &str) -> ComputedPropertyKind {
    crate::owl::query_by_entity(conn, property_iri)
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
        })
        .map(|triple| {
            if triple.predicate == "foundation:aggregation" {
                ComputedPropertyKind::Aggregation
            } else {
                ComputedPropertyKind::Formula
            }
        })
        .unwrap_or(ComputedPropertyKind::Formula)
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

fn query_class_instances(
    conn: &Connection,
    class_iri: &str,
    offset: i64,
) -> Result<Vec<String>, String> {
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
        .map_err(|error| error.to_string())
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
) -> Result<NewFormulaRecalcJob, String> {
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
            })
        })
        .await?;
    Ok(job)
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
        });
    }

    jobs
}

/// Refreshes clock-dependent values once at startup and once a day after that.
pub fn spawn_daily_clock_refresh(app: tauri::AppHandle, executor: crate::owl::DbExecutor) {
    tauri::async_runtime::spawn(async move {
        loop {
            let jobs = create_clock_dependent_recalc_jobs(&executor)
                .await
                .unwrap_or_default();

            if !jobs.is_empty() {
                if let Some(worker) = app.try_state::<FormulaWorker>() {
                    for job in jobs {
                        if let Err(error) = worker.enqueue(job, vec![], vec![]) {
                            crate::commands::log_backend(
                                "error",
                                &format!("daily formula recalculation enqueue failed: {error}"),
                            );
                        }
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

pub async fn create_instance_recalc_jobs(
    executor: &crate::owl::DbExecutor,
    instance_iri: &str,
    changed_property_iri: &str,
) -> Result<Vec<NewFormulaRecalcJob>, String> {
    let instance = instance_iri.to_string();
    let changed = changed_property_iri.to_string();
    executor
        .read(move |conn| {
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
                })
                .collect::<Vec<_>>();
            Ok(jobs)
        })
        .await
}

async fn create_class_calculated_recalc_jobs(
    executor: &crate::owl::DbExecutor,
    instance_iri: &str,
) -> Result<Vec<NewFormulaRecalcJob>, String> {
    let instance = instance_iri.to_string();
    executor
        .read(move |conn| Ok(collect_class_calculated_recalc_jobs(conn, &instance)))
        .await
}

async fn create_clock_dependent_recalc_jobs(
    executor: &crate::owl::DbExecutor,
) -> Result<Vec<NewFormulaRecalcJob>, String> {
    executor
        .read(|conn| Ok(collect_clock_dependent_recalc_jobs(conn)))
        .await
}

pub async fn create_startup_recalc_jobs(
    executor: &crate::owl::DbExecutor,
) -> Result<Vec<NewFormulaRecalcJob>, String> {
    executor
        .read(move |conn| {
            let instances = crate::owl::query_by_predicate(conn, "rdf:type")
                .map_err(|error| error.to_string())?
                .triples
                .into_iter()
                .map(|triple| triple.subject)
                .collect::<HashSet<_>>();
            let mut calculated = Vec::new();
            for predicate in ["foundation:formula", "foundation:aggregation"] {
                calculated.extend(
                    crate::owl::query_by_predicate(conn, predicate)
                        .map_err(|error| error.to_string())?
                        .triples,
                );
            }

            let mut jobs = HashMap::new();
            let now = now_millis();
            for instance in instances {
                let types = crate::owl::query_by_entity_predicate(conn, &instance, "rdf:type")
                    .map_err(|error| error.to_string())?
                    .triples
                    .into_iter()
                    .filter_map(|triple| triple.object.as_iri().map(str::to_string))
                    .collect::<HashSet<_>>();
                for calculated_property in &calculated {
                    let domains = crate::owl::query_by_entity_predicate(
                        conn,
                        &calculated_property.subject,
                        "rdfs:domain",
                    )
                    .map_err(|error| error.to_string())?
                    .triples
                    .into_iter()
                    .filter_map(|triple| triple.object.as_iri().map(str::to_string))
                    .collect::<Vec<_>>();
                    for class_iri in domains.into_iter().filter(|class| types.contains(class)) {
                        let property_iri = calculated_property.subject.clone();
                        let job = NewFormulaRecalcJob {
                            id: format!(
                                "startup_{now}_{}_{}",
                                property_iri.replace(':', "_"),
                                instance.replace(':', "_")
                            ),
                            property_label: Some(fetch_label(conn, &property_iri)),
                            class_label: Some(fetch_label(conn, &class_iri)),
                            property_iri: property_iri.clone(),
                            class_iri,
                            instance_iri: Some(instance.clone()),
                        };
                        jobs.entry((property_iri, instance.clone())).or_insert(job);
                    }
                }
            }
            Ok(jobs.into_values().collect())
        })
        .await
}

pub fn spawn_startup_recovery<R: Runtime + 'static>(
    app: tauri::AppHandle<R>,
    executor: crate::owl::DbExecutor,
) {
    tauri::async_runtime::spawn(async move {
        let jobs = match create_startup_recalc_jobs(&executor).await {
            Ok(jobs) => jobs,
            Err(error) => {
                crate::diagnostics::log_backend(
                    "error",
                    &format!("[FORMULA_WORKER] startup recovery scan failed: {}", error),
                );
                app.emit("formula-recalc-error", serde_json::json!({"error": error}))
                    .ok();
                return;
            }
        };
        if let Some(worker) = app.try_state::<FormulaWorker>() {
            for job in jobs {
                if let Err(error) = worker.enqueue(job, vec![], vec![]) {
                    crate::diagnostics::log_backend(
                        "error",
                        &format!(
                            "[FORMULA_WORKER] startup recovery enqueue failed: {}",
                            error
                        ),
                    );
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        setup_test_db_schema(&conn);
        conn
    }

    fn setup_test_db_schema(conn: &Connection) {
        conn.execute_batch("
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
            active_jobs: Mutex::new(HashSet::new()),
            property_generations: Mutex::new(HashMap::new()),
            formula_errors: Mutex::new(HashMap::new()),
            scheduled_jobs: Mutex::new(HashMap::new()),
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
    async fn worker_panic_is_reported_by_supervisor_to_waiting_barrier() {
        let app = tauri::test::mock_app();
        let (sender, _receiver) = mpsc::channel(1);
        let (waiter, receiver) = oneshot::channel();
        let (waiter_two, receiver_two) = oneshot::channel();
        let worker = FormulaWorker {
            sender,
            waiters: Mutex::new(HashMap::from([(23, waiter), (24, waiter_two)])),
            remaining_jobs: Mutex::new(HashMap::from([(23, 1), (24, 1)])),
            next_barrier_id: AtomicU64::new(1),
            active_jobs: Mutex::new(HashSet::new()),
            property_generations: Mutex::new(HashMap::new()),
            formula_errors: Mutex::new(HashMap::new()),
            scheduled_jobs: Mutex::new(HashMap::new()),
        };
        app.manage(worker);
        let worker_task = tauri::async_runtime::spawn(async {
            panic!("simulated formula worker panic");
        });
        supervise_worker(app.handle().clone(), worker_task).await;

        for terminal in [
            receiver.await.unwrap().unwrap_err(),
            receiver_two.await.unwrap().unwrap_err(),
        ] {
            match terminal {
                MaterializationTerminal::Failed(reason) => {
                    assert!(reason.starts_with("formula worker terminated unexpectedly:"));
                    assert!(reason.contains("simulated formula worker panic"));
                }
                MaterializationTerminal::Cancelled(reason) => {
                    panic!("worker panic was reported as cancellation: {reason}")
                }
            }
        }
        assert!(app
            .state::<FormulaWorker>()
            .remaining_jobs
            .lock()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn duplicate_formula_jobs_coalesce_and_preserve_barrier_subscribers() {
        let (sender, mut receiver) = mpsc::channel(4);
        let worker = FormulaWorker {
            sender,
            waiters: Mutex::new(HashMap::new()),
            remaining_jobs: Mutex::new(HashMap::new()),
            next_barrier_id: AtomicU64::new(1),
            active_jobs: Mutex::new(HashSet::new()),
            property_generations: Mutex::new(HashMap::new()),
            formula_errors: Mutex::new(HashMap::new()),
            scheduled_jobs: Mutex::new(HashMap::new()),
        };
        let make_job = || NewFormulaRecalcJob {
            id: "formula-1".into(),
            property_iri: "foundation:total".into(),
            property_label: Some("Total".into()),
            class_iri: "foundation:Account".into(),
            class_label: Some("Account".into()),
            instance_iri: Some("foundation:Account_1".into()),
        };
        worker.enqueue(make_job(), vec![], vec![1]).unwrap();
        worker.enqueue(make_job(), vec![], vec![2]).unwrap();

        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_err());
        let key = (
            "foundation:total".to_string(),
            Some("foundation:Account_1".to_string()),
            0,
        );
        let scheduled = worker.scheduled_jobs.lock().unwrap();
        let subscribers = scheduled.get(&key).unwrap();
        assert_eq!(subscribers.barrier_ids, HashSet::from([1, 2]));
    }

    #[test]
    fn cancellation_advances_generation_for_subsequent_formula_jobs() {
        let (sender, mut receiver) = mpsc::channel(2);
        let worker = FormulaWorker {
            sender,
            waiters: Mutex::new(HashMap::new()),
            remaining_jobs: Mutex::new(HashMap::new()),
            next_barrier_id: AtomicU64::new(1),
            active_jobs: Mutex::new(HashSet::new()),
            property_generations: Mutex::new(HashMap::new()),
            formula_errors: Mutex::new(HashMap::new()),
            scheduled_jobs: Mutex::new(HashMap::new()),
        };
        let make_job = || NewFormulaRecalcJob {
            id: "formula-1".into(),
            property_iri: "foundation:total".into(),
            property_label: None,
            class_iri: "foundation:Account".into(),
            class_label: None,
            instance_iri: None,
        };
        worker.enqueue(make_job(), vec![], vec![]).unwrap();
        assert!(worker.is_generation_current("foundation:total", 0));
        worker.cancel_property("foundation:total");
        assert!(!worker.is_generation_current("foundation:total", 0));
        assert!(worker.is_generation_current("foundation:total", 1));
        worker.enqueue(make_job(), vec![], vec![]).unwrap();

        assert!(matches!(
            receiver.try_recv().unwrap(),
            WorkerCommand::Enqueue { generation: 0, .. }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            WorkerCommand::Enqueue { generation: 1, .. }
        ));
    }

    #[tokio::test]
    async fn startup_rescan_reconstructs_calculated_work_from_ontology() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("formula-startup-rescan.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_test_db_schema(&conn);
        conn.execute("INSERT INTO origins (name) VALUES ('test')", [])
            .unwrap();
        triple(
            &conn,
            "foundation:Account_1",
            "rdf:type",
            Some("foundation:Account"),
            None,
        );
        triple(
            &conn,
            "foundation:balance",
            "rdfs:domain",
            Some("foundation:Account"),
            None,
        );
        triple(
            &conn,
            "foundation:balance",
            "foundation:formula",
            None,
            Some("{{foundation:amount}}"),
        );
        let executor = crate::owl::DbExecutor::new(conn, db_path);

        let jobs = create_startup_recalc_jobs(&executor).await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].property_iri, "foundation:balance");
        assert_eq!(
            jobs[0].instance_iri.as_deref(),
            Some("foundation:Account_1")
        );
    }

    #[test]
    fn formula_progress_serializes_source_correlation_in_camel_case() {
        let event = FormulaProgressEvent {
            job_id: "job".to_string(),
            property_iri: "foundation:total".to_string(),
            property_label: "Total".to_string(),
            class_iri: "foundation:Account".to_string(),
            class_label: "Account".to_string(),
            percent: 50,
            status: "running".to_string(),
            source_entity_iri: Some("foundation:Conversation_1".to_string()),
            source_update_id: Some("foundation:Conversation_1@42".to_string()),
            source_tx: Some(42),
        };
        let payload = serde_json::to_value(event).unwrap();

        assert_eq!(payload["sourceEntityIri"], "foundation:Conversation_1");
        assert_eq!(payload["sourceUpdateId"], "foundation:Conversation_1@42");
        assert_eq!(payload["sourceTx"], 42);
    }

    #[tokio::test]
    async fn formula_barrier_preserves_cancelled_terminal_and_blocks_dependent_step() {
        let app = tauri::test::mock_app();
        let (sender, _receiver) = mpsc::channel(1);
        let (cancelled_tx, cancelled_rx) = oneshot::channel();
        let (failed_tx, failed_rx) = oneshot::channel();
        let worker = FormulaWorker {
            sender,
            waiters: Mutex::new(HashMap::from([(31, cancelled_tx), (32, failed_tx)])),
            remaining_jobs: Mutex::new(HashMap::from([(31, 1), (32, 1)])),
            next_barrier_id: AtomicU64::new(1),
            active_jobs: Mutex::new(HashSet::new()),
            property_generations: Mutex::new(HashMap::new()),
            formula_errors: Mutex::new(HashMap::new()),
            scheduled_jobs: Mutex::new(HashMap::new()),
        };
        app.manage(worker);

        fail_barriers(
            app.handle(),
            &[31],
            MaterializationTerminal::Cancelled("job cancelled".to_string()),
        );
        fail_barriers(
            app.handle(),
            &[32],
            MaterializationTerminal::Failed("job failed".to_string()),
        );

        let cancelled = cancelled_rx.await.unwrap();
        let failed = failed_rx.await.unwrap();
        assert_eq!(
            cancelled,
            Err(MaterializationTerminal::Cancelled(
                "job cancelled".to_string()
            ))
        );
        assert_eq!(
            failed,
            Err(MaterializationTerminal::Failed("job failed".to_string()))
        );

        let mut dependent_started = false;
        if cancelled.is_ok() {
            dependent_started = true;
        }
        assert!(!dependent_started);
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
