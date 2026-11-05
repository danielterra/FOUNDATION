use crate::owl::materialization::{emit_progress, MaterializationCorrelation};
use crate::owl::{query_property, DbExecutor};
use crate::owl::{Object, Triple};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Listener, Manager, Runtime};
use tokio::sync::{mpsc::unbounded_channel, mpsc::UnboundedSender, oneshot};

/// Predicates that always belong in the allowlist, independent of queryConfig contents.
/// - `rdf:type`: collect_affected_jobs uses get_types for scenario A (entity is targetClass)
///   and B (entity is owner). Without it, typing a target entity never triggers recompute.
/// - `foundation:queryConfig`: a newly written queryConfig must force cache reload so the
///   new query-property begins reacting immediately. Excluding it would silently skip the
///   very first write that activates a query-property.
/// - `rdfs:domain`: domain changes alter which owners are in scope; must also force reload.
const ALWAYS_RELEVANT: &[&str] = &["rdf:type", "foundation:queryConfig", "rdfs:domain"];

#[derive(Debug)]
pub enum WorkerCommand {
    Enqueue {
        entity_iri: String,
        /// Predicates written in this batch for this entity. Empty when enqueued
        /// via `await_drained` (those are synchronous callers that always process).
        written_predicates: Vec<String>,
        /// True when the listener detected `foundation:queryConfig` or `rdfs:domain`
        /// in the written predicates, signalling that the cache must be reloaded
        /// unconditionally before processing — without requiring a `has_query_config`
        /// DB check inside the read.
        force_reload: bool,
        barrier_id: Option<u64>,
        correlations: Vec<MaterializationCorrelation>,
    },
}

pub struct QueryWorker {
    sender: UnboundedSender<WorkerCommand>,
    waiters: Mutex<HashMap<u64, oneshot::Sender<Result<(), String>>>>,
    next_barrier_id: AtomicU64,
    pending_commands: Arc<AtomicUsize>,
    active_entity: Arc<Mutex<Option<String>>>,
    startup_error: Arc<Mutex<Option<String>>>,
}

/// Parsed query property with its domain classes, loaded once and cached between writes.
struct CachedQueryProperty {
    property_iri: String,
    config: query_property::QueryConfig,
    domains: Vec<String>,
}

/// Derives the set of predicates that matter for query-property recompute from
/// a loaded cache snapshot, adding the always-relevant fixed predicates on top.
///
/// The allowlist is rebuilt atomically every time the cache reloads — the two
/// are always in sync. Any predicate NOT in this set means the written entity
/// cannot possibly affect any query result, so we skip the read entirely.
fn build_relevant_predicates(entries: &[CachedQueryProperty]) -> HashSet<String> {
    let mut set: HashSet<String> = ALWAYS_RELEVANT.iter().map(|s| s.to_string()).collect();
    for entry in entries {
        for f in &entry.config.filters {
            set.insert(f.property_iri.clone());
        }
        for o in &entry.config.order_by {
            set.insert(o.property_iri.clone());
        }
    }
    set
}

impl QueryWorker {
    pub fn spawn<R: Runtime + 'static>(app: AppHandle<R>, executor: DbExecutor) -> Self {
        let (tx, mut rx) = unbounded_channel::<WorkerCommand>();
        let pending_commands = Arc::new(AtomicUsize::new(0));
        let active_entity = Arc::new(Mutex::new(None));
        let startup_error = Arc::new(Mutex::new(None));

        let worker = QueryWorker {
            sender: tx,
            waiters: Mutex::new(HashMap::new()),
            next_barrier_id: AtomicU64::new(1),
            pending_commands: pending_commands.clone(),
            active_entity: active_entity.clone(),
            startup_error: startup_error.clone(),
        };

        let worker_app = app.clone();
        let worker_task = tauri::async_runtime::spawn(async move {
            // Eager cache init: populate before the loop so the in-memory guard
            // (relevant_predicates) is available from the very first event, at the cost
            // of one read on boot — far cheaper than 307 reads per batch without a guard.
            let cache_started = Instant::now();
            let initial_cache_result = executor
                .read_unmetered(|conn| load_cache_and_predicates(conn))
                .await;
            let (initial_cache, initial_predicates) = match initial_cache_result {
                Ok(cache) => cache,
                Err(error) => {
                    if let Ok(mut startup_error) = startup_error.lock() {
                        *startup_error =
                            Some(format!("query worker initial cache load failed: {}", error));
                    }
                    crate::diagnostics::log_backend(
                        "error",
                        &format!(
                            "[QUERY_WORKER] initial cache load failed duration_ms={} error={}",
                            cache_started.elapsed().as_millis(),
                            error
                        ),
                    );
                    (Arc::new(vec![]), Arc::new(HashSet::new()))
                }
            };
            crate::diagnostics::log_backend(
                "debug",
                &format!(
                    "[QUERY_WORKER] initial cache load completed duration_ms={} query_properties={} relevant_predicates={}",
                    cache_started.elapsed().as_millis(),
                    initial_cache.len(),
                    initial_predicates.len()
                ),
            );

            let mut cache: Arc<Vec<CachedQueryProperty>> = initial_cache;
            let mut relevant_predicates: Arc<HashSet<String>> = initial_predicates;

            while let Some(cmd) = rx.recv().await {
                // Batch-drain: consume all pending commands before processing, deduplicating
                // by entity_iri. This cuts re-saturation during boot recovery — a burst of
                // automation writes produces dozens of enqueues for the same entity; we
                // collapse them into one evaluation per entity. force_reload is OR-merged
                // so a single invalidating event in the batch still triggers the reload.
                let mut batch: HashMap<
                    String,
                    (Vec<String>, bool, Vec<u64>, Vec<MaterializationCorrelation>),
                > = HashMap::new();
                let mut received_commands = 1usize;
                let first = cmd;
                let WorkerCommand::Enqueue {
                    entity_iri,
                    written_predicates,
                    force_reload,
                    barrier_id,
                    correlations,
                } = first;
                let entry = batch
                    .entry(entity_iri)
                    .or_insert((vec![], false, vec![], vec![]));
                merge_into_batch_entry(
                    entry,
                    written_predicates,
                    force_reload,
                    barrier_id,
                    correlations,
                );

                loop {
                    match rx.try_recv() {
                        Ok(WorkerCommand::Enqueue {
                            entity_iri,
                            written_predicates,
                            force_reload,
                            barrier_id,
                            correlations,
                        }) => {
                            received_commands += 1;
                            let entry =
                                batch
                                    .entry(entity_iri)
                                    .or_insert((vec![], false, vec![], vec![]));
                            merge_into_batch_entry(
                                entry,
                                written_predicates,
                                force_reload,
                                barrier_id,
                                correlations,
                            );
                        }
                        Err(_) => break,
                    }
                }

                let deduplicated_entities = batch.len();
                let remaining_commands =
                    complete_pending_commands(&pending_commands, received_commands);
                crate::diagnostics::log_backend(
                    "debug",
                    &format!(
                        "[QUERY_WORKER] batch drained commands={} deduplicated_entities={} queued_remaining={} counter_underflow={}",
                        received_commands,
                        deduplicated_entities,
                        remaining_commands.0,
                        remaining_commands.1
                    ),
                );

                for (entity_iri, (written_predicates, force_reload, barrier_ids, correlations)) in
                    batch
                {
                    let startup_failure = startup_error.lock().ok().and_then(|error| error.clone());
                    if let Some(error) = startup_failure {
                        resolve_waiters(&app, barrier_ids, Err(error));
                        continue;
                    }
                    set_active_entity(&active_entity, Some(&entity_iri));
                    crate::diagnostics::log_backend(
                        "debug",
                        &format!(
                            "[QUERY_WORKER] entity started iri={} written_predicates={} force_reload={}",
                            entity_iri,
                            written_predicates.len(),
                            force_reload
                        ),
                    );
                    // Pre-read guard: if no written predicate intersects the allowlist and
                    // this is not a cache-invalidating write, skip the read entirely.
                    // await_drained callers send empty written_predicates — they always proceed.
                    if !written_predicates.is_empty()
                        && !force_reload
                        && barrier_ids.is_empty()
                        && correlations.is_empty()
                        && !written_predicates
                            .iter()
                            .any(|p| relevant_predicates.contains(p))
                    {
                        crate::diagnostics::log_backend(
                            "debug",
                            &format!(
                                "[QUERY_WORKER] entity skipped by predicate guard iri={} written_predicates={}",
                                entity_iri,
                                written_predicates.len()
                            ),
                        );
                        set_active_entity(&active_entity, None);
                        resolve_waiters(&app, barrier_ids, Ok(()));
                        continue;
                    }

                    let entity_started = Instant::now();
                    if let Err(error) = process_entity_update(
                        &app,
                        &executor,
                        &entity_iri,
                        &mut cache,
                        &mut relevant_predicates,
                        force_reload,
                        &correlations,
                    )
                    .await
                    {
                        set_active_entity(&active_entity, None);
                        resolve_waiters(&app, barrier_ids, Err(error.clone()));
                        crate::diagnostics::log_backend(
                            "error",
                            &format!(
                                "[QUERY_WORKER] entity materialization failed iri={} error={}",
                                entity_iri, error
                            ),
                        );
                        continue;
                    }

                    crate::diagnostics::log_backend(
                        "debug",
                        &format!(
                            "[QUERY_WORKER] entity completed iri={} duration_ms={}",
                            entity_iri,
                            entity_started.elapsed().as_millis()
                        ),
                    );
                    set_active_entity(&active_entity, None);
                    resolve_waiters(&app, barrier_ids, Ok(()));
                }
            }
        });
        tauri::async_runtime::spawn(async move {
            if let Err(error) = worker_task.await {
                fail_all_waiters(
                    &worker_app,
                    format!("query worker terminated unexpectedly: {}", error),
                );
            } else {
                fail_all_waiters(
                    &worker_app,
                    "query worker command channel closed".to_string(),
                );
            }
        });

        worker
    }

    /// Enqueue `entity_iri` for recompute and wait until the worker has processed
    /// all outstanding jobs for it. Returns `Ok(())` on success.
    pub async fn await_drained(
        &self,
        executor: &DbExecutor,
        entity_iri: &str,
        execution_iri: &str,
        step_iri: &str,
        node_iri: &str,
        source_entity_iri: Option<&str>,
        source_update_id: Option<&str>,
    ) -> Result<(), String> {
        let wait_started = Instant::now();
        if let Some(error) = self
            .startup_error
            .lock()
            .map_err(|error| error.to_string())?
            .clone()
        {
            return Err(error);
        }
        crate::diagnostics::log_backend(
            "debug",
            &format!(
                "[QUERY_WORKER] await_drained started entity={} execution={} step={} queued={}",
                entity_iri,
                execution_iri,
                step_iri,
                self.pending_commands.load(Ordering::Acquire)
            ),
        );
        let snapshot_tx = executor.current_snapshot_tx().await?;
        let (tx, rx) = oneshot::channel::<Result<(), String>>();
        let barrier_id = self.next_barrier_id.fetch_add(1, Ordering::AcqRel);
        {
            let mut map = self.waiters.lock().map_err(|e| e.to_string())?;
            map.insert(barrier_id, tx);
        }
        if let Err(error) = self.enqueue(WorkerCommand::Enqueue {
            entity_iri: entity_iri.to_string(),
            // Synchronous callers always need the full evaluation — empty written_predicates
            // bypasses the guard (the worker treats empty as "unconditional").
            written_predicates: vec![],
            force_reload: false,
            barrier_id: Some(barrier_id),
            correlations: vec![MaterializationCorrelation {
                execution_iri: execution_iri.to_string(),
                step_iri: step_iri.to_string(),
                node_iri: node_iri.to_string(),
                source_entity_iri: source_entity_iri.map(str::to_string),
                source_update_id: source_update_id.map(str::to_string),
                source_tx: Some(snapshot_tx),
            }],
        }) {
            if let Ok(mut waiters) = self.waiters.lock() {
                waiters.remove(&barrier_id);
            }
            return Err(error);
        }

        match rx.await {
            Ok(Ok(())) => {
                crate::diagnostics::log_backend(
                    "debug",
                    &format!(
                        "[QUERY_WORKER] await_drained completed entity={} execution={} step={} duration_ms={} queued={}",
                        entity_iri,
                        execution_iri,
                        step_iri,
                        wait_started.elapsed().as_millis(),
                        self.pending_commands.load(Ordering::Acquire)
                    ),
                );
                Ok(())
            }
            Ok(Err(error)) => {
                log_drain_failure(
                    &self.active_entity,
                    &self.pending_commands,
                    entity_iri,
                    execution_iri,
                    step_iri,
                    wait_started.elapsed().as_millis(),
                    &error,
                );
                Err(error)
            }
            Err(_) => {
                let error = format!("query worker stopped before materializing {}", entity_iri);
                log_drain_failure(
                    &self.active_entity,
                    &self.pending_commands,
                    entity_iri,
                    execution_iri,
                    step_iri,
                    wait_started.elapsed().as_millis(),
                    &error,
                );
                Err(error)
            }
        }
    }

    fn enqueue(&self, command: WorkerCommand) -> Result<(), String> {
        self.pending_commands.fetch_add(1, Ordering::AcqRel);
        match self.sender.send(command) {
            Ok(()) => Ok(()),
            Err(_) => {
                complete_pending_commands(&self.pending_commands, 1);
                let error = "query_worker channel closed".to_string();
                crate::diagnostics::log_backend(
                    "error",
                    &format!("[QUERY_WORKER] enqueue failed error={}", error),
                );
                Err(error)
            }
        }
    }
}

fn complete_pending_commands(
    pending_commands: &AtomicUsize,
    completed_commands: usize,
) -> (usize, bool) {
    let mut pending = pending_commands.load(Ordering::Acquire);
    loop {
        let counter_underflow = pending < completed_commands;
        let remaining = pending.saturating_sub(completed_commands);
        match pending_commands.compare_exchange_weak(
            pending,
            remaining,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return (remaining, counter_underflow),
            Err(observed) => pending = observed,
        }
    }
}

fn set_active_entity(active_entity: &Mutex<Option<String>>, entity_iri: Option<&str>) {
    match active_entity.lock() {
        Ok(mut active) => *active = entity_iri.map(String::from),
        Err(error) => crate::diagnostics::log_backend(
            "error",
            &format!("[QUERY_WORKER] active entity lock failed error={}", error),
        ),
    }
}

fn active_entity_snapshot(active_entity: &Mutex<Option<String>>) -> String {
    match active_entity.lock() {
        Ok(active) => active.as_deref().unwrap_or("none").to_string(),
        Err(_) => "unavailable".to_string(),
    }
}

fn log_drain_failure(
    active_entity: &Mutex<Option<String>>,
    pending_commands: &AtomicUsize,
    entity_iri: &str,
    execution_iri: &str,
    step_iri: &str,
    duration_ms: u128,
    error: &str,
) {
    crate::diagnostics::log_backend(
        "error",
        &format!(
            "[QUERY_WORKER] await_drained failed entity={} execution={} step={} active_entity={} queued={} duration_ms={} error={}",
            entity_iri,
            execution_iri,
            step_iri,
            active_entity_snapshot(active_entity),
            pending_commands.load(Ordering::Acquire),
            duration_ms,
            error
        ),
    );
}

/// Merges a single Enqueue payload into a batch-accumulation entry.
fn merge_into_batch_entry(
    entry: &mut (Vec<String>, bool, Vec<u64>, Vec<MaterializationCorrelation>),
    predicates: Vec<String>,
    force: bool,
    barrier_id: Option<u64>,
    correlations: Vec<MaterializationCorrelation>,
) {
    entry.1 |= force;
    for p in predicates {
        if !entry.0.contains(&p) {
            entry.0.push(p);
        }
    }
    if let Some(id) = barrier_id {
        if !entry.2.contains(&id) {
            entry.2.push(id);
        }
    }
    for correlation in correlations {
        if !entry.3.contains(&correlation) {
            entry.3.push(correlation);
        }
    }
}

fn resolve_waiters<R: Runtime>(
    app: &AppHandle<R>,
    barrier_ids: Vec<u64>,
    result: Result<(), String>,
) {
    if let Some(w) = app.try_state::<QueryWorker>() {
        let mut map = w
            .waiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let senders: Vec<oneshot::Sender<Result<(), String>>> =
            barrier_ids.iter().filter_map(|id| map.remove(id)).collect();
        for s in senders {
            let _ = s.send(result.clone());
        }
    }
}

fn fail_all_waiters<R: Runtime>(app: &AppHandle<R>, error: String) {
    if let Some(worker) = app.try_state::<QueryWorker>() {
        let mut waiters = worker
            .waiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let senders = waiters
            .drain()
            .map(|(_, sender)| sender)
            .collect::<Vec<_>>();
        for sender in senders {
            let _ = sender.send(Err(error.clone()));
        }
    }
}

pub fn register_listener<R: Runtime + 'static>(app: AppHandle<R>) {
    let app_clone = app.clone();
    app.listen("entity-changed-internal", move |event| {
        let Some((entity_iri, written_predicates)) = parse_payload(event.payload()) else {
            return;
        };

        // Cache-invalidating predicates: force reload regardless of the current allowlist.
        // These writes change the structure of what queries exist and which owners are in scope.
        let force_reload = written_predicates
            .iter()
            .any(|p| p == "foundation:queryConfig" || p == "rdfs:domain");

        let worker = match app_clone.try_state::<QueryWorker>() {
            Some(w) => w,
            None => return,
        };
        if let Err(error) = worker.enqueue(WorkerCommand::Enqueue {
            entity_iri,
            written_predicates,
            force_reload,
            barrier_id: None,
            correlations: vec![],
        }) {
            crate::diagnostics::log_backend(
                "error",
                &format!("[QUERY_WORKER] listener enqueue failed error={}", error),
            );
        }
    });
}

fn parse_payload(payload: &str) -> Option<(String, Vec<String>)> {
    let v: serde_json::Value = match serde_json::from_str(payload) {
        Ok(value) => value,
        Err(error) => {
            crate::diagnostics::log_backend(
                "error",
                &format!(
                    "[QUERY_WORKER] change event payload parse failed error={}",
                    error
                ),
            );
            return None;
        }
    };
    let entity_id = v["entityId"].as_str().map(String::from)?;
    let written_predicates = v["writtenPredicates"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Some((entity_id, written_predicates))
}

/// Loads the cache and derives the relevant-predicate allowlist in a single pass.
fn load_cache_and_predicates(
    conn: &rusqlite::Connection,
) -> Result<(Arc<Vec<CachedQueryProperty>>, Arc<HashSet<String>>), String> {
    let entries = try_load_query_property_cache(conn)?;
    let predicates = build_relevant_predicates(&entries);
    Ok((Arc::new(entries), Arc::new(predicates)))
}

async fn process_entity_update(
    app: &AppHandle<impl Runtime>,
    executor: &DbExecutor,
    entity_iri: &str,
    cache: &mut Arc<Vec<CachedQueryProperty>>,
    relevant_predicates: &mut Arc<HashSet<String>>,
    force_reload: bool,
    correlations: &[MaterializationCorrelation],
) -> Result<(), String> {
    let entity = entity_iri.to_string();
    let cache_snapshot = cache.clone();
    let required_tx = correlations
        .iter()
        .filter_map(|c| c.source_tx)
        .max()
        .unwrap_or(0);

    type ReadResult = (
        Vec<(String, String, query_property::QueryConfig)>,
        Option<(Arc<Vec<CachedQueryProperty>>, Arc<HashSet<String>>)>,
    );

    let read_started = Instant::now();
    let observed_tx = executor.current_snapshot_tx().await?;
    let result: ReadResult = match executor
        .read_unmetered(move |conn| {
            let phase_started = Instant::now();
            ensure_revision_observed(observed_tx, required_tx)?;
            // needs_reload conditions (checked without a DB call when possible):
            // 1. No cache yet (first run, though eager init handles this in practice).
            // 2. The entity IS a query-property (its IRI is in the cache): editing the
            //    property's own attributes (e.g. domain change) must re-derive the cache.
            // 3. force_reload: the listener already saw queryConfig/rdfs:domain in the batch;
            //    skip the has_query_config DB check — the flag carries the information.
            // has_query_config is retained as fallback for entities that somehow arrive here
            // without the flag set (e.g. future callers that bypass the listener).
            let needs_reload = cache_snapshot.is_empty()
                || cache_snapshot.iter().any(|e| e.property_iri == entity)
                || force_reload
                || try_has_query_config(conn, &entity)?;

            let entries: Arc<Vec<CachedQueryProperty>> = if needs_reload {
                Arc::new(try_load_query_property_cache(conn)?)
            } else {
                cache_snapshot
            };

            let refreshed = if needs_reload {
                let predicates = build_relevant_predicates(&entries);
                Some((entries.clone(), Arc::new(predicates)))
            } else {
                None
            };

            let jobs = try_collect_affected_jobs(conn, &entity, &entries)?;
            crate::diagnostics::log_backend(
                "debug",
                &format!(
                    "[QUERY_WORKER] materialization phase=collect_jobs entity={} duration_ms={} jobs={} cache_reload={}",
                    entity,
                    phase_started.elapsed().as_millis(),
                    jobs.len(),
                    needs_reload
                ),
            );
            Ok((jobs, refreshed))
        })
        .await {
            Ok(result) => result,
            Err(error) => {
                crate::diagnostics::log_backend(
                    "error",
                    &format!(
                        "[QUERY_WORKER] materialization phase=collect_jobs failed entity={} duration_ms={} error={}",
                        entity_iri,
                        read_started.elapsed().as_millis(),
                        error
                    ),
                );
                return Err(format!("query worker failed to collect affected jobs for {}: {}", entity_iri, error));
            }
        };

    let (jobs, refreshed) = result;

    if let Some((new_cache, new_predicates)) = refreshed {
        *cache = new_cache;
        *relevant_predicates = new_predicates;
    }

    if jobs.is_empty() {
        for correlation in correlations {
            emit_progress(
                app,
                correlation,
                "query-properties",
                "completed",
                entity_iri,
                None,
                Some(0),
                Some(0),
                None,
            );
        }
        return Ok(());
    }

    let total_jobs = jobs.len();
    let mut completed_jobs = 0usize;
    for correlation in correlations {
        emit_progress(
            app,
            correlation,
            "query-properties",
            "running",
            entity_iri,
            None,
            Some(0),
            Some(total_jobs),
            None,
        );
    }

    let formula_worker = app.try_state::<crate::commands::formula_worker::FormulaWorker>();

    for (owner_iri, property_iri, config) in jobs {
        for correlation in correlations {
            emit_progress(
                app,
                correlation,
                "query-properties",
                "running",
                &owner_iri,
                Some(&property_iri),
                Some(completed_jobs),
                Some(total_jobs),
                None,
            );
        }
        let owner = owner_iri.clone();
        let cfg = config.clone();

        let evaluation_started = Instant::now();
        let new_results: Vec<String> = match executor
            .read_unmetered(move |conn| {
                query_property::evaluate_query(conn, &owner, &cfg).map_err(|e| e.to_string())
            })
            .await
        {
            Ok(r) => r,
            Err(error) => {
                crate::diagnostics::log_backend(
                    "error",
                    &format!(
                        "[QUERY_WORKER] materialization phase=evaluate_query failed entity={} owner={} property={} duration_ms={} error={}",
                        entity_iri,
                        owner_iri,
                        property_iri,
                        evaluation_started.elapsed().as_millis(),
                        error
                    ),
                );
                return Err(format!(
                    "query evaluation failed for {} / {}: {}",
                    owner_iri, property_iri, error
                ));
            }
        };
        crate::diagnostics::log_backend(
            "debug",
            &format!(
                "[QUERY_WORKER] materialization phase=evaluate_query entity={} owner={} property={} duration_ms={} results={}",
                entity_iri,
                owner_iri,
                property_iri,
                evaluation_started.elapsed().as_millis(),
                new_results.len()
            ),
        );

        let owner_c = owner_iri.clone();
        let prop_c = property_iri.clone();
        let results_c = new_results.clone();

        let write_started = Instant::now();
        let changed: bool = match executor
            .write(move |conn| {
                update_query_property_triples(conn, &owner_c, &prop_c, &results_c)
                    .map(|b| b.to_string())
                    .map_err(|e| e.to_string())
            })
            .await
        {
            Ok(value) => value == "true",
            Err(error) => {
                crate::diagnostics::log_backend(
                    "error",
                    &format!(
                        "[QUERY_WORKER] materialization phase=write_property failed entity={} owner={} property={} duration_ms={} error={}",
                        entity_iri,
                        owner_iri,
                        property_iri,
                        write_started.elapsed().as_millis(),
                        error
                    ),
                );
                return Err(format!(
                    "query result write failed for {} / {}: {}",
                    owner_iri, property_iri, error
                ));
            }
        };
        crate::diagnostics::log_backend(
            "debug",
            &format!(
                "[QUERY_WORKER] materialization phase=write_property entity={} owner={} property={} duration_ms={} changed={}",
                entity_iri,
                owner_iri,
                property_iri,
                write_started.elapsed().as_millis(),
                changed
            ),
        );

        if changed {
            if let Some(ref worker) = formula_worker {
                let formula_queue_started = Instant::now();
                let jobs = match crate::commands::formula_worker::create_instance_recalc_jobs(
                    executor,
                    &owner_iri,
                    &property_iri,
                )
                .await
                {
                    Ok(ids) => ids,
                    Err(error) => {
                        crate::diagnostics::log_backend(
                            "error",
                            &format!(
                                "[QUERY_WORKER] formula job creation failed entity={} owner={} property={} duration_ms={} error={}",
                                entity_iri,
                                owner_iri,
                                property_iri,
                                formula_queue_started.elapsed().as_millis(),
                                error
                            ),
                        );
                        return Err(format!(
                            "formula job creation failed for {} / {}: {}",
                            owner_iri, property_iri, error
                        ));
                    }
                };

                crate::diagnostics::log_backend(
                    "debug",
                    &format!(
                        "[QUERY_WORKER] materialization phase=create_formula_jobs entity={} owner={} property={} duration_ms={} jobs={}",
                        entity_iri,
                        owner_iri,
                        property_iri,
                        formula_queue_started.elapsed().as_millis(),
                        jobs.len()
                    ),
                );

                for job in jobs {
                    let job_id = job.id.clone();
                    if let Err(error) = worker.enqueue(job, correlations.to_vec(), vec![]) {
                        crate::diagnostics::log_backend(
                            "error",
                            &format!(
                                "[QUERY_WORKER] formula job enqueue failed entity={} job={} error={}",
                                entity_iri, job_id, error
                            ),
                        );
                        return Err(format!(
                            "formula worker enqueue failed for job {}: {}",
                            job_id, error
                        ));
                    }
                }
            }

            crate::realtime::emit_entity_updated(&app, &owner_iri);
        }
        completed_jobs += 1;
        for correlation in correlations {
            emit_progress(
                app,
                correlation,
                "query-properties",
                "running",
                &owner_iri,
                Some(&property_iri),
                Some(completed_jobs),
                Some(total_jobs),
                None,
            );
        }
    }
    for correlation in correlations {
        emit_progress(
            app,
            correlation,
            "query-properties",
            "completed",
            entity_iri,
            None,
            Some(total_jobs),
            Some(total_jobs),
            None,
        );
    }
    Ok(())
}

fn ensure_revision_observed(observed_tx: i64, required_tx: i64) -> Result<(), String> {
    if observed_tx < required_tx {
        return Err(format!(
            "query worker observed tx {} before required tx {}",
            observed_tx, required_tx
        ));
    }
    Ok(())
}

/// Returns true if `entity_iri` is the subject of any current `foundation:queryConfig` triple.
/// Used as a fallback inside the read when force_reload is false but the entity might have
/// acquired a queryConfig via a code path that doesn't set the flag.
fn try_has_query_config(conn: &rusqlite::Connection, entity_iri: &str) -> Result<bool, String> {
    crate::owl::query_by_entity_predicate(conn, entity_iri, "foundation:queryConfig")
        .map(|result| !result.triples.is_empty())
        .map_err(|error| format!("query config lookup failed for {}: {}", entity_iri, error))
}

/// Loads all query properties with their domain classes into a flat cache.
/// Called only on cache miss or invalidation — not on every entity write.
fn try_load_query_property_cache(
    conn: &rusqlite::Connection,
) -> Result<Vec<CachedQueryProperty>, String> {
    crate::owl::query_by_predicate(conn, "foundation:queryConfig")
        .map_err(|error| format!("query property cache read failed: {}", error))?
        .triples
        .into_iter()
        .filter_map(|triple| {
            triple
                .object
                .as_literal()
                .map(|json| (triple.subject, json.to_string()))
        })
        .into_iter()
        .map(|(iri, json)| {
            let config = query_property::parse_query_config(&json).map_err(|error| {
                format!("query property config parse failed for {}: {}", iri, error)
            })?;
            let domains = try_get_property_domains(conn, &iri)?;
            Ok(CachedQueryProperty {
                property_iri: iri,
                config,
                domains,
            })
        })
        .collect()
}

fn try_collect_affected_jobs(
    conn: &rusqlite::Connection,
    entity_iri: &str,
    entries: &[CachedQueryProperty],
) -> Result<Vec<(String, String, query_property::QueryConfig)>, String> {
    let entity_types = try_get_types(conn, entity_iri)?;
    let mut jobs: Vec<(String, String, query_property::QueryConfig)> = Vec::new();

    for entry in entries {
        let entity_is_target = entity_types.contains(&entry.config.target_class);

        if entity_is_target {
            // Scenario A: entity is of targetClass → re-eval all owner instances
            let owners = try_find_owners_for_domains(conn, &entry.domains)?;
            for owner_iri in owners {
                if !jobs
                    .iter()
                    .any(|(o, p, _)| o == &owner_iri && p == &entry.property_iri)
                {
                    jobs.push((owner_iri, entry.property_iri.clone(), entry.config.clone()));
                }
            }
        }

        // Scenario B: entity itself is an owner of this property
        if entity_types.iter().any(|t| entry.domains.contains(t))
            && !jobs
                .iter()
                .any(|(o, p, _)| o == entity_iri && p == &entry.property_iri)
        {
            jobs.push((
                entity_iri.to_string(),
                entry.property_iri.clone(),
                entry.config.clone(),
            ));
        }
    }

    Ok(jobs)
}

fn try_get_types(conn: &rusqlite::Connection, iri: &str) -> Result<Vec<String>, String> {
    crate::owl::query_by_entity_predicate(conn, iri, "rdf:type")
        .map(|result| {
            result
                .triples
                .into_iter()
                .filter_map(|triple| triple.object.as_iri().map(str::to_string))
                .collect()
        })
        .map_err(|error| format!("entity type read failed for {}: {}", iri, error))
}

fn try_get_property_domains(
    conn: &rusqlite::Connection,
    property_iri: &str,
) -> Result<Vec<String>, String> {
    crate::owl::query_by_entity_predicate(conn, property_iri, "rdfs:domain")
        .map(|result| {
            result
                .triples
                .into_iter()
                .filter_map(|triple| triple.object.as_iri().map(str::to_string))
                .collect()
        })
        .map_err(|error| {
            format!(
                "property domains read failed for {}: {}",
                property_iri, error
            )
        })
}

fn try_find_owners_for_domains(
    conn: &rusqlite::Connection,
    domains: &[String],
) -> Result<Vec<String>, String> {
    let mut owners = Vec::new();
    for class_iri in domains {
        let result = crate::owl::query_by_predicate_object(conn, "rdf:type", class_iri)
            .map_err(|error| format!("owners read failed for {}: {}", class_iri, error))?;
        owners.extend(result.triples.into_iter().map(|triple| triple.subject));
    }
    Ok(owners)
}

#[cfg(test)]
fn load_query_property_cache(conn: &rusqlite::Connection) -> Vec<CachedQueryProperty> {
    try_load_query_property_cache(conn).unwrap()
}

#[cfg(test)]
fn has_query_config(conn: &rusqlite::Connection, entity_iri: &str) -> bool {
    try_has_query_config(conn, entity_iri).unwrap()
}

#[cfg(test)]
fn collect_affected_jobs(
    conn: &rusqlite::Connection,
    entity_iri: &str,
    entries: &[CachedQueryProperty],
) -> Vec<(String, String, query_property::QueryConfig)> {
    try_collect_affected_jobs(conn, entity_iri, entries).unwrap()
}

fn update_query_property_triples(
    conn: &mut rusqlite::Connection,
    owner_iri: &str,
    property_iri: &str,
    new_results: &[String],
) -> Result<bool, crate::owl::OwlError> {
    // is_current marks the single latest TX per (subject, predicate). Combined with
    // retracted=0, this is precisely the set of current values — no correlated
    // MAX(tx) subquery needed. The invariant is maintained eagerly by assert_triples
    // and retract_triples on every write.
    let current: Vec<String> = crate::owl::query_by_entity_predicate(conn, owner_iri, property_iri)
        .map_err(|error| crate::owl::OwlError::DatabaseError(error.to_string()))?
        .triples
        .into_iter()
        .filter_map(|triple| triple.object.as_iri().map(str::to_string))
        .collect();

    let current_set: std::collections::HashSet<&str> = current.iter().map(|s| s.as_str()).collect();
    let new_set: std::collections::HashSet<&str> = new_results.iter().map(|s| s.as_str()).collect();

    if current_set == new_set {
        return Ok(false);
    }

    if new_results.is_empty() {
        // Clearing the property: retract is the documented "fact no longer true"
        // operation. Without a non-empty new TX, MAX(tx) would still point to old data.
        let old_triples: Vec<Triple> = current
            .iter()
            .map(|obj| Triple::new(owner_iri, property_iri, Object::Iri(obj.clone())))
            .collect();
        if !old_triples.is_empty() {
            crate::owl::retract_raw_triples(conn, &old_triples, "query_worker")?;
        }
    } else {
        // Just assert a new TX with the new values. The latest TX defines the truth —
        // retracting old values is unnecessary and contradicts the immutability model.
        let new_triples: Vec<Triple> = new_results
            .iter()
            .map(|obj| Triple::new(owner_iri, property_iri, Object::Iri(obj.clone())))
            .collect();
        crate::owl::assert_raw_triples_with_tx(conn, &new_triples, "query_worker")?;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::store;
    use futures_util::stream::{FuturesUnordered, StreamExt};
    use rusqlite::Connection;

    #[test]
    fn revision_fence_rejects_stale_snapshot_and_accepts_target_or_newer() {
        assert!(ensure_revision_observed(41, 42).is_err());
        assert!(ensure_revision_observed(42, 42).is_ok());
        assert!(ensure_revision_observed(43, 42).is_ok());
    }

    #[test]
    fn enqueue_failure_is_reported_and_does_not_leave_pending_count() {
        let (sender, receiver) = unbounded_channel();
        drop(receiver);
        let worker = QueryWorker {
            sender,
            waiters: Mutex::new(HashMap::new()),
            next_barrier_id: AtomicU64::new(1),
            pending_commands: Arc::new(AtomicUsize::new(0)),
            active_entity: Arc::new(Mutex::new(None)),
            startup_error: Arc::new(Mutex::new(None)),
        };
        let result = worker.enqueue(WorkerCommand::Enqueue {
            entity_iri: "foundation:test".into(),
            written_predicates: vec![],
            force_reload: false,
            barrier_id: Some(7),
            correlations: vec![],
        });
        assert!(result.is_err());
        assert_eq!(worker.pending_commands.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn failed_cache_bootstrap_is_returned_to_waiting_barrier() {
        let directory = tempfile::tempdir().unwrap();
        let db_path = directory.path().join("query-worker-bootstrap-failure.db");
        let connection = crate::eavto::connection::initialize_db(&db_path).unwrap();
        let executor = DbExecutor::new(connection, db_path);
        let (sender, _receiver) = unbounded_channel();
        let worker = QueryWorker {
            sender,
            waiters: Mutex::new(HashMap::new()),
            next_barrier_id: AtomicU64::new(1),
            pending_commands: Arc::new(AtomicUsize::new(0)),
            active_entity: Arc::new(Mutex::new(None)),
            startup_error: Arc::new(Mutex::new(Some(
                "query worker initial cache load failed: fixture".into(),
            ))),
        };
        let result = worker
            .await_drained(
                &executor,
                "foundation:test",
                "execution",
                "step",
                "node",
                None,
                None,
            )
            .await;
        assert_eq!(
            result,
            Err("query worker initial cache load failed: fixture".into())
        );
    }

    #[tokio::test]
    async fn worker_termination_failure_resolves_query_barrier() {
        let app = tauri::test::mock_app();
        let (sender, _receiver) = unbounded_channel();
        let (waiter, receiver) = oneshot::channel();
        let worker = QueryWorker {
            sender,
            waiters: Mutex::new(HashMap::from([(9, waiter)])),
            next_barrier_id: AtomicU64::new(1),
            pending_commands: Arc::new(AtomicUsize::new(0)),
            active_entity: Arc::new(Mutex::new(None)),
            startup_error: Arc::new(Mutex::new(None)),
        };
        app.manage(worker);

        fail_all_waiters(
            app.handle(),
            "query worker terminated unexpectedly: panic".into(),
        );
        assert_eq!(
            receiver.await.unwrap(),
            Err("query worker terminated unexpectedly: panic".into())
        );
    }

    async fn measured_read(
        executor: DbExecutor,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    ) -> Result<(), String> {
        executor
            .read(move |_conn| {
                let current = active.fetch_add(1, Ordering::AcqRel) + 1;
                update_atomic_max(&peak, current);
                std::thread::sleep(std::time::Duration::from_millis(20));
                active.fetch_sub(1, Ordering::AcqRel);
                Ok(String::new())
            })
            .await
            .map(|_| ())
    }

    fn update_atomic_max(target: &AtomicUsize, candidate: usize) {
        let mut current = target.load(Ordering::Acquire);
        while candidate > current {
            match target.compare_exchange_weak(
                current,
                candidate,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    #[tokio::test]
    async fn test_query_worker_executor_notify_to_drain_with_170_entities() {
        let directory = tempfile::tempdir().unwrap();
        let db_path = directory.path().join("query-worker-harness.db");
        let mut connection = crate::eavto::connection::initialize_db(&db_path).unwrap();
        let query_property = "foundation:harnessPaymentsQuery";
        let account = "foundation:harnessAccount";
        let mut setup_triples = vec![
            Triple::new(
                query_property,
                "foundation:queryConfig",
                Object::Literal {
                    value: r#"{"targetClass":"foundation:HarnessPayment","filters":[]}"#.into(),
                    datatype: Some("xsd:string".into()),
                    language: None,
                },
            ),
            Triple::new(
                query_property,
                "rdfs:domain",
                Object::Iri("foundation:HarnessAccount".into()),
            ),
            Triple::new(
                account,
                "rdf:type",
                Object::Iri("foundation:HarnessAccount".into()),
            ),
        ];
        store::assert_triples(&mut connection, &setup_triples, "query_worker_test").unwrap();
        setup_triples.clear();

        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel();
        let executor = DbExecutor::new_with_notify(connection, db_path, Some(notify_tx));
        let app = tauri::test::mock_app();
        let app_handle = app.handle().clone();
        let worker = QueryWorker::spawn(app_handle.clone(), executor.clone());
        app.manage(worker);
        register_listener(app_handle.clone());

        let notifications_seen = Arc::new(AtomicUsize::new(0));
        let seen_by_forwarder = notifications_seen.clone();
        let app_for_forwarder = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            while let Some((subjects, _, written_triples)) = notify_rx.recv().await {
                let batch_tx = written_triples
                    .iter()
                    .map(|triple| triple.tx)
                    .max()
                    .unwrap_or(0);
                for (entity, predicates) in subjects {
                    crate::realtime::emit_entity_changed_internal(
                        &app_for_forwarder,
                        &entity,
                        &predicates,
                        batch_tx,
                    );
                    seen_by_forwarder.fetch_add(1, Ordering::Release);
                }
            }
        });

        let test_started = Instant::now();
        for index in 0..170usize {
            let entity_iri = format!("foundation:harnessPayment-{index}");
            let entity_for_write = entity_iri.clone();
            executor
                .write(move |conn| {
                    let triple = Triple::new(
                        entity_for_write,
                        "rdf:type",
                        Object::Iri("foundation:HarnessPayment".into()),
                    );
                    store::assert_triples(conn, &[triple], "query_worker_test")
                        .map(|tx| tx.to_string())
                        .map_err(|error| error.to_string())
                })
                .await
                .unwrap();
        }

        while notifications_seen.load(Ordering::Acquire) < 170 {
            tokio::task::yield_now().await;
        }
        let worker = app.state::<QueryWorker>();
        for index in 0..170usize {
            let entity_iri = format!("foundation:harnessPayment-{index}");
            worker
                .await_drained(
                    &executor,
                    &entity_iri,
                    "synthetic-execution",
                    "synthetic-step",
                    "synthetic-node",
                    None,
                    None,
                )
                .await
                .unwrap();
        }

        let query_property_for_read = query_property.to_string();
        let account_for_read = account.to_string();
        let materialized_count: i64 = executor
            .read(move |conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM triples WHERE subject = ?1 AND predicate = ?2 AND is_current = 1 AND retracted = 0",
                    rusqlite::params![account_for_read, query_property_for_read],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .await
            .unwrap();
        assert_eq!(materialized_count, 170);

        let query_tx_before_burst: i64 = executor
            .read(move |conn| {
                conn.query_row(
                    "SELECT COALESCE(MAX(tx), 0) FROM triples WHERE subject = ?1 AND predicate = ?2",
                    rusqlite::params![account, query_property],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .await
            .unwrap();
        for _ in 0..170 {
            crate::realtime::emit_entity_changed_internal(
                &app_handle,
                "foundation:harnessPayment-0",
                &["rdf:type".to_string()],
                0,
            );
        }
        worker
            .await_drained(
                &executor,
                "foundation:harnessPayment-0",
                "synthetic-burst",
                "synthetic-step",
                "synthetic-node",
                None,
                None,
            )
            .await
            .unwrap();
        let query_tx_after_burst: i64 = executor
            .read(move |conn| {
                conn.query_row(
                    "SELECT COALESCE(MAX(tx), 0) FROM triples WHERE subject = ?1 AND predicate = ?2",
                    rusqlite::params!["foundation:harnessAccount", "foundation:harnessPaymentsQuery"],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .await
            .unwrap();
        assert_eq!(query_tx_before_burst, query_tx_after_burst);

        let active_reads = Arc::new(AtomicUsize::new(0));
        let peak_reads = Arc::new(AtomicUsize::new(0));
        let read_started = Instant::now();
        let mut reads = FuturesUnordered::new();
        for _ in 0..48 {
            reads.push(measured_read(
                executor.clone(),
                active_reads.clone(),
                peak_reads.clone(),
            ));
        }
        while let Some(result) = reads.next().await {
            result.unwrap();
        }
        let read_batch_ms = read_started.elapsed().as_millis();
        eprintln!(
            "[QUERY_WORKER_INTEGRATION_TEST] writes=170 notifications={} materialized={} duplicate_burst=170 output_tx_unchanged=true concurrent_reads=48 peak_reads={} read_batch_ms={} total_ms={}",
            notifications_seen.load(Ordering::Acquire),
            materialized_count,
            peak_reads.load(Ordering::Acquire),
            read_batch_ms,
            test_started.elapsed().as_millis()
        );
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE transactions (
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
             WHERE is_current = 1 AND retracted = 0;",
        )
        .unwrap();
        conn
    }

    fn insert_triple_iri(conn: &Connection, subject: &str, predicate: &str, object: &str, tx: i64) {
        conn.execute(
            "UPDATE triples SET is_current = 0 WHERE subject = ?1 AND predicate = ?2",
            rusqlite::params![subject, predicate],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted, is_current)
             VALUES (?1, ?2, ?3, 'iri', ?4, 1, 0, 0, 1)",
            rusqlite::params![subject, predicate, object, tx],
        ).unwrap();
    }

    fn insert_triple_literal(
        conn: &Connection,
        subject: &str,
        predicate: &str,
        value: &str,
        tx: i64,
    ) {
        conn.execute(
            "UPDATE triples SET is_current = 0 WHERE subject = ?1 AND predicate = ?2",
            rusqlite::params![subject, predicate],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, tx, origin_id, created_at, retracted, is_current)
             VALUES (?1, ?2, ?3, 'literal', ?4, 1, 0, 0, 1)",
            rusqlite::params![subject, predicate, value, tx],
        ).unwrap();
    }

    fn count_triples(conn: &Connection, subject: &str, predicate: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM triples_current WHERE subject = ? AND predicate = ?",
            rusqlite::params![subject, predicate],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    fn max_tx(conn: &Connection, subject: &str, predicate: &str) -> i64 {
        conn.query_row(
            "SELECT COALESCE(MAX(tx), 0) FROM triples WHERE subject = ? AND predicate = ?",
            rusqlite::params![subject, predicate],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    fn seed_query_materialization_db(conn: &Connection, target_count: usize) -> String {
        let property = "foundation:paymentsQuery";
        insert_triple_literal(
            conn,
            property,
            "foundation:queryConfig",
            r#"{"targetClass":"foundation:Payment","filters":[]}"#,
            1,
        );
        insert_triple_iri(conn, property, "rdfs:domain", "foundation:Account", 1);
        insert_triple_iri(
            conn,
            "foundation:account-1",
            "rdf:type",
            "foundation:Account",
            1,
        );
        for index in 0..target_count {
            insert_triple_iri(
                conn,
                &format!("foundation:payment-{index}"),
                "rdf:type",
                "foundation:Payment",
                index as i64 + 2,
            );
        }
        property.to_string()
    }

    fn materialize_for_event(
        conn: &mut Connection,
        entity_iri: &str,
        cache: &[CachedQueryProperty],
    ) -> (u128, u128, u128) {
        let collect_started = Instant::now();
        let jobs = collect_affected_jobs(conn, entity_iri, cache);
        let collect_ms = collect_started.elapsed().as_micros();
        let mut evaluation_ms = 0;
        let mut write_ms = 0;
        for (owner, property, config) in jobs {
            let evaluation_started = Instant::now();
            let results = query_property::evaluate_query(conn, &owner, &config).unwrap();
            evaluation_ms += evaluation_started.elapsed().as_micros();
            let write_started = Instant::now();
            update_query_property_triples(conn, &owner, &property, &results).unwrap();
            write_ms += write_started.elapsed().as_micros();
        }
        (collect_ms, evaluation_ms, write_ms)
    }

    async fn signal_drain_after_delay(sender: tokio::sync::oneshot::Sender<()>) {
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let _ = sender.send(());
    }

    /// Exercita coleta, avaliação e escrita de query-property com carga isolada.
    /// Não sobe automação nem chama o banco vivo; a etapa de barrier é representada
    /// por oneshot após todo o lote ser materializado.
    #[test]
    fn test_query_materialization_load_harness_170_writes() {
        use tokio::sync::oneshot;

        for target_count in [1usize, 170usize] {
            let mut conn = setup_db();
            let property = seed_query_materialization_db(&conn, target_count);
            let cache = load_query_property_cache(&conn);
            let queue_started = Instant::now();
            let mut batch: HashMap<
                String,
                (Vec<String>, bool, Vec<u64>, Vec<MaterializationCorrelation>),
            > = HashMap::new();
            let mut queued_at_by_entity = HashMap::new();
            for index in 0..target_count {
                let entity = format!("foundation:payment-{index}");
                queued_at_by_entity.insert(entity.clone(), Instant::now());
                let entry = batch
                    .entry(entity)
                    .or_insert((vec![], false, vec![], vec![]));
                merge_into_batch_entry(entry, vec!["rdf:type".to_string()], false, None, vec![]);
            }
            let enqueue_duration_us = queue_started.elapsed().as_micros();
            let queue_depth = batch.len();
            let mut collect_ms = 0;
            let mut evaluation_ms = 0;
            let mut write_ms = 0;
            let mut queue_wait_us = 0;
            let materialization_started = Instant::now();
            let mut active_entity = None;
            for entity in batch.keys() {
                active_entity = Some(entity.clone());
                assert_eq!(active_entity.as_deref().unwrap_or_default(), entity);
                queue_wait_us += queued_at_by_entity[entity].elapsed().as_micros();
                let durations = materialize_for_event(&mut conn, entity, &cache);
                collect_ms += durations.0;
                evaluation_ms += durations.1;
                write_ms += durations.2;
                active_entity = None;
            }
            let materialization_ms = materialization_started.elapsed().as_micros();
            let (barrier_tx, barrier_rx) = oneshot::channel::<()>();
            let barrier_started = Instant::now();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap();
            runtime.spawn(signal_drain_after_delay(barrier_tx));
            runtime.block_on(barrier_rx).unwrap();
            let barrier_ms = barrier_started.elapsed().as_micros();
            assert!(active_entity.is_none());
            assert_eq!(
                count_triples(&conn, "foundation:account-1", &property),
                target_count as i64
            );
            eprintln!(
                "[QUERY_WORKER_TEST] targets={} enqueue_us={} queue_wait_us={} queue_depth={} active_entity={} collect_us={} evaluation_us={} write_us={} materialization_us={} await_drained_us={}",
                target_count, enqueue_duration_us, queue_wait_us, queue_depth, active_entity.as_deref().unwrap_or("none"),
                collect_ms, evaluation_ms, write_ms, materialization_ms, barrier_ms
            );
        }

        let mut conn = setup_db();
        let property = seed_query_materialization_db(&conn, 1);
        let cache = load_query_property_cache(&conn);
        let entity = "foundation:payment-0";
        let mut entry = (vec![], false, vec![], vec![]);
        let mut duplicate_count = 0;
        for _ in 0..170 {
            duplicate_count += 1;
            merge_into_batch_entry(
                &mut entry,
                vec!["rdf:type".to_string()],
                false,
                None,
                vec![],
            );
        }
        assert_eq!(duplicate_count, 170);
        assert_eq!(entry.0.len(), 1);
        let (collect_ms, evaluation_ms, write_ms) =
            materialize_for_event(&mut conn, entity, &cache);
        assert_eq!(count_triples(&conn, "foundation:account-1", &property), 1);
        eprintln!(
            "[QUERY_WORKER_TEST] duplicate_burst=170 deduplicated_entities=1 collect_us={} evaluation_us={} write_us={}",
            collect_ms, evaluation_ms, write_ms
        );
    }

    /// Calling `update_query_property_triples` twice with the same set must not
    /// write a new TX on the second call (idempotent — prevents oscillation loops).
    #[test]
    fn test_no_write_when_set_unchanged() {
        let mut conn = setup_db();
        let owner = "foundation:Owner1";
        let prop = "foundation:queryProp";
        let values = vec!["foundation:A".to_string(), "foundation:B".to_string()];

        let changed_first = update_query_property_triples(&mut conn, owner, prop, &values).unwrap();
        assert!(changed_first, "first write must report changed=true");

        let tx_after_first = max_tx(&conn, owner, prop);
        let changed_second =
            update_query_property_triples(&mut conn, owner, prop, &values).unwrap();
        assert!(
            !changed_second,
            "second write with identical set must not write (changed=false)"
        );

        let tx_after_second = max_tx(&conn, owner, prop);
        assert_eq!(
            tx_after_first, tx_after_second,
            "TX must not advance when the set is already current"
        );
    }

    /// When the result set changes (A,B → A,B,C), a new TX is written and `changed=true`.
    #[test]
    fn test_write_when_set_grows() {
        let mut conn = setup_db();
        let owner = "foundation:Owner2";
        let prop = "foundation:queryProp";

        let first = vec!["foundation:A".to_string(), "foundation:B".to_string()];
        let _ = update_query_property_triples(&mut conn, owner, prop, &first).unwrap();
        let tx_v1 = max_tx(&conn, owner, prop);

        let second = vec![
            "foundation:A".to_string(),
            "foundation:B".to_string(),
            "foundation:C".to_string(),
        ];
        let changed = update_query_property_triples(&mut conn, owner, prop, &second).unwrap();
        assert!(changed);
        assert!(
            max_tx(&conn, owner, prop) > tx_v1,
            "TX must advance when the result set grows"
        );
    }

    /// Clearing the set (new_results = []) must retract and report changed=true,
    /// without generating a new IRI triple.
    #[test]
    fn test_clear_retracts_and_reports_changed() {
        let mut conn = setup_db();
        let owner = "foundation:Owner3";
        let prop = "foundation:queryProp";

        let initial = vec!["foundation:A".to_string()];
        let _ = update_query_property_triples(&mut conn, owner, prop, &initial).unwrap();

        let changed = update_query_property_triples(&mut conn, owner, prop, &[]).unwrap();
        assert!(changed, "clearing must report changed=true");
        assert_eq!(
            count_triples(&conn, owner, prop),
            0,
            "all triples must be retracted after clearing"
        );
    }

    /// Calling update once with A then again with A must converge: second call
    /// is a no-op, ensuring the query→formula→query chain terminates in ≤ 2 cycles.
    #[test]
    fn test_convergence_idempotent_in_two_cycles() {
        let mut conn = setup_db();
        let owner = "foundation:Owner4";
        let prop = "foundation:queryPropCycle";
        let values = vec!["foundation:X".to_string()];

        let c1 = update_query_property_triples(&mut conn, owner, prop, &values).unwrap();
        assert!(c1);
        let c2 = update_query_property_triples(&mut conn, owner, prop, &values).unwrap();
        assert!(
            !c2,
            "second cycle with same result must not trigger further writes"
        );
    }

    /// `load_query_property_cache` retorna config + domains a partir de triples inseridas.
    #[test]
    fn test_load_query_property_cache_returns_config_and_domains() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;

        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);
        insert_triple_iri(&conn, prop, "rdfs:domain", "foundation:Project", 1);

        let cache = load_query_property_cache(&conn);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache[0].property_iri, prop);
        assert_eq!(cache[0].config.target_class, "foundation:Task");
        assert_eq!(cache[0].domains, vec!["foundation:Project".to_string()]);
    }

    /// `collect_affected_jobs` cenário A: entity é do targetClass → owner recebe job.
    #[test]
    fn test_collect_affected_jobs_scenario_a_entity_is_target() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        let owner = "foundation:proj1";
        let task = "foundation:task1";

        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);
        insert_triple_iri(&conn, prop, "rdfs:domain", "foundation:Project", 1);
        insert_triple_iri(&conn, owner, "rdf:type", "foundation:Project", 1);
        insert_triple_iri(&conn, task, "rdf:type", "foundation:Task", 1);

        let cache = load_query_property_cache(&conn);
        let jobs = collect_affected_jobs(&conn, task, &cache);

        assert_eq!(jobs.len(), 1, "one job expected for owner");
        assert_eq!(jobs[0].0, owner);
        assert_eq!(jobs[0].1, prop);
    }

    /// `collect_affected_jobs` cenário B: entity é o owner → recebe job para si.
    #[test]
    fn test_collect_affected_jobs_scenario_b_entity_is_owner() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        let owner = "foundation:proj1";

        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);
        insert_triple_iri(&conn, prop, "rdfs:domain", "foundation:Project", 1);
        insert_triple_iri(&conn, owner, "rdf:type", "foundation:Project", 1);

        let cache = load_query_property_cache(&conn);

        // No task instances → scenario A produces nothing.
        // Owner itself triggers scenario B.
        let jobs = collect_affected_jobs(&conn, owner, &cache);
        assert!(jobs.iter().any(|(o, p, _)| o == owner && p == prop));
    }

    /// Invalidação: entity cujo IRI está no cache como property_iri → needs_reload=true.
    #[test]
    fn test_needs_reload_when_entity_is_cached_property() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);

        let cache = Arc::new(load_query_property_cache(&conn));

        let needs_reload =
            cache.iter().any(|e| e.property_iri == prop) || has_query_config(&conn, prop);

        assert!(
            needs_reload,
            "property IRI presente no cache deve forçar reload"
        );
    }

    /// Invalidação: entity alheia ao cache sem queryConfig → sem reload.
    #[test]
    fn test_no_reload_for_unrelated_entity() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);

        let cache = Arc::new(load_query_property_cache(&conn));
        let unrelated = "foundation:someOtherEntity";

        let needs_reload =
            cache.iter().any(|e| e.property_iri == unrelated) || has_query_config(&conn, unrelated);

        assert!(!needs_reload, "entity alheia não deve forçar reload");
    }

    /// Invalidação: entity com triple queryConfig → needs_reload=true mesmo ausente do cache anterior.
    #[test]
    fn test_needs_reload_when_entity_has_query_config_triple() {
        let conn = setup_db();
        let prop = "foundation:existingProp";
        let new_prop = "foundation:newQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;

        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);
        let cache = Arc::new(load_query_property_cache(&conn));

        insert_triple_literal(&conn, new_prop, "foundation:queryConfig", config_json, 2);

        let needs_reload =
            cache.iter().any(|e| e.property_iri == new_prop) || has_query_config(&conn, new_prop);

        assert!(needs_reload, "nova queryConfig triple deve forçar reload");
    }

    // ── Novos testes para o guard pré-read ──────────────────────────────────────

    /// (a) Guard descarta quando writtenPredicates não intersecta relevant_predicates.
    #[test]
    fn test_guard_discards_irrelevant_predicates() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        // Config com filtro em foundation:dueDate
        let config_json = r#"{"targetClass":"foundation:Task","filters":[{"propertyIri":"foundation:dueDate","operator":"exists"}]}"#;
        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);

        let entries = load_query_property_cache(&conn);
        let relevant = build_relevant_predicates(&entries);

        let written: Vec<String> = vec!["foundation:someUnrelatedProp".to_string()];
        let force_reload = false;

        let should_skip =
            !written.is_empty() && !force_reload && !written.iter().any(|p| relevant.contains(p));

        assert!(
            should_skip,
            "escrita em predicate irrelevante deve ser descartada sem read"
        );
    }

    /// (b) Guard NÃO descarta para `rdf:type` — necessário para cenário A (tipar entidade-alvo).
    #[test]
    fn test_guard_passes_rdf_type() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);

        let entries = load_query_property_cache(&conn);
        let relevant = build_relevant_predicates(&entries);

        let written: Vec<String> = vec!["rdf:type".to_string()];
        let force_reload = false;

        let should_skip =
            !written.is_empty() && !force_reload && !written.iter().any(|p| relevant.contains(p));

        assert!(!should_skip, "rdf:type deve sempre passar pelo guard");
        assert!(
            relevant.contains("rdf:type"),
            "rdf:type deve estar no allowlist"
        );
    }

    /// (c) Guard NÃO descarta e sinaliza reload para `foundation:queryConfig`.
    #[test]
    fn test_guard_passes_and_forces_reload_for_query_config() {
        let conn = setup_db();
        let entries = load_query_property_cache(&conn); // cache vazio
        let relevant = build_relevant_predicates(&entries);

        let written: Vec<String> = vec!["foundation:queryConfig".to_string()];
        // O listener seta force_reload=true quando detecta foundation:queryConfig
        let force_reload = written
            .iter()
            .any(|p| p == "foundation:queryConfig" || p == "rdfs:domain");

        let should_skip =
            !written.is_empty() && !force_reload && !written.iter().any(|p| relevant.contains(p));

        assert!(
            !should_skip,
            "foundation:queryConfig não deve ser descartado"
        );
        assert!(
            force_reload,
            "foundation:queryConfig deve ativar force_reload"
        );
        assert!(
            relevant.contains("foundation:queryConfig"),
            "foundation:queryConfig deve estar no allowlist fixo"
        );
    }

    /// (d) Derivação do HashSet: filters + order_by + predicados fixos todos presentes.
    #[test]
    fn test_build_relevant_predicates_includes_all_sources() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{
            "targetClass": "foundation:Task",
            "filters": [{"propertyIri": "foundation:dueDate", "operator": "exists"}],
            "orderBy": [{"propertyIri": "foundation:priority", "direction": "asc"}]
        }"#;
        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);

        let entries = load_query_property_cache(&conn);
        let relevant = build_relevant_predicates(&entries);

        // Predicados fixos
        assert!(
            relevant.contains("rdf:type"),
            "rdf:type deve estar presente"
        );
        assert!(
            relevant.contains("foundation:queryConfig"),
            "foundation:queryConfig deve estar presente"
        );
        assert!(
            relevant.contains("rdfs:domain"),
            "rdfs:domain deve estar presente"
        );
        // Predicados do config
        assert!(
            relevant.contains("foundation:dueDate"),
            "filter propertyIri deve estar presente"
        );
        assert!(
            relevant.contains("foundation:priority"),
            "orderBy propertyIri deve estar presente"
        );
    }
}
