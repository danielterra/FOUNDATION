use serde::Serialize;
use tauri::{State, AppHandle, Emitter, Manager};

use crate::owl::{self, Connection, DbExecutor, Individual, Object};
use crate::owl::formula_worker::FormulaWorker;
use super::setup_system_info::{get_cpu_info, get_memory_info, get_os_info, get_locale_info};

/// Initialize the application database
/// This MUST be called before any other commands that use the database
/// The frontend should call this on startup and handle permission requests
#[tauri::command]
#[allow(non_snake_case)]
pub async fn initialize_app(
    app: AppHandle,
) -> Result<(), String> {
    // Skip initialization during CI/CD — the runner doesn't have a real user DB
    if std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
    {
        super::log_backend("info", "Skipping database initialization (build/CI mode)");
        let dummy_conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to create in-memory connection: {}", e))?;
        let executor = DbExecutor::new_in_memory(dummy_conn);
        app.manage(executor);
        let _ = app.emit("import-complete", ());
        return Ok(());
    }

    let (mut conn, db_path) = owl::initialize_with_progress(app.clone())
        .map_err(|e| {
            let error_msg = format!("Failed to initialize database: {:?}", e);
            super::log_backend("error", &error_msg);
            let _ = app.emit("import-error", error_msg.clone());
            error_msg
        })?;

    // Capture foundation_dir for portable path resolution. Attachment helpers
    // store paths relative to this directory so the DB roams between machines.
    if let Some(dir) = db_path.parent() {
        crate::paths::set_foundation_dir(dir.to_path_buf());

        // Asset protocol scope (for `convertFileSrc`) and opener scope (for
        // `openPath`) are declared statically in tauri.conf.json /
        // capabilities/default.json with the default location, but
        // foundation_dir is user-configurable (iCloud, OneDrive, custom). Allow
        // the actual attachments dir at runtime so previews and the "open in
        // default app" button work from wherever the user picked. Without
        // this, paths under `iCloudDrive/...` etc. are blocked.
        let attachments = dir.join("attachments");
        let attachments_glob = format!("{}/**", attachments.display().to_string().replace('\\', "/"));

        if let Err(e) = app.asset_protocol_scope().allow_directory(&attachments, true) {
            super::log_backend("warn", &format!(
                "Failed to allow attachments dir in asset protocol scope ({}): {}",
                attachments.display(), e
            ));
        }

        let cap = tauri::ipc::CapabilityBuilder::new("attachments-dir-runtime")
            .window("main")
            .permission_scoped(
                "opener:allow-open-path",
                vec![serde_json::json!({ "path": attachments_glob })],
                Vec::<serde_json::Value>::new(),
            );
        if let Err(e) = app.add_capability(cap) {
            super::log_backend("warn", &format!(
                "Failed to register runtime opener capability for {}: {}",
                attachments.display(), e
            ));
        }
    }

    let _ = app.emit("import-progress", serde_json::json!({ "stage": "Loading icon library" }));
    owl::seed_icon_library(&mut conn);

    // Drain the WAL now — only one connection exists so no readers can block this.
    // Pool connections maintain WAL read marks that prevent PASSIVE checkpoints from advancing,
    // causing the WAL to grow unboundedly during normal operation.
    let _ = app.emit("import-progress", serde_json::json!({ "stage": "Compacting transaction log" }));
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

    let _ = app.emit("import-progress", serde_json::json!({ "stage": "Cleaning up old data" }));
    purge_old_retractions(&conn);

    let t0 = std::time::Instant::now();

    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel::<(std::collections::HashMap<String, Vec<String>>, Vec<String>, Vec<crate::eavto::WrittenTriple>)>();
    let executor = DbExecutor::new_with_notify(conn, db_path.clone(), Some(notify_tx));
    let executor_for_worker = executor.clone();
    let executor_for_recover = executor.clone();
    app.manage(executor);
    super::log_backend("debug", &format!("[STARTUP] executor_ready={}ms", t0.elapsed().as_millis()));

    // Log DB stats in the background — COUNT queries are slow (~500ms on large DBs)
    let stats_executor = app.state::<DbExecutor>().inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(stats) = stats_executor.read(|conn| owl::get_stats(conn).map_err(|e| e.to_string())).await {
            super::log_backend("info", &format!(
                "Database initialized - Triples: {}, Active: {}, Transactions: {}, Entities: {}",
                stats.total_facts, stats.active_facts, stats.total_transactions, stats.entities_count
            ));
        }
    });

    let app_for_notify = app.clone();
    let executor_for_reindex = app.state::<DbExecutor>().inner().clone();
    let executor_for_type_resolve = app.state::<DbExecutor>().inner().clone();
    tauri::async_runtime::spawn(async move {
        while let Some((subject_predicates, iri_objects, written_triples)) = notify_rx.recv().await {
            // Derive the max tx from this batch for the ring and event payloads.
            let batch_tx = written_triples.iter().map(|t| t.tx).max().unwrap_or(0);

            let mut seen_subjects: Vec<String> = Vec::new();
            for (iri, predicates) in &subject_predicates {
                crate::realtime::emit_entity_updated_with_tx(&app_for_notify, iri, predicates, batch_tx);
                // Non-gated backend signal: task execution / recurrence / scheduler must react
                // to every write, independent of whether the frontend is displaying the entity.
                crate::realtime::emit_entity_changed_internal(&app_for_notify, iri);
                seen_subjects.push(iri.clone());
            }

            let mut seen_objects = std::collections::HashSet::new();
            for iri in &iri_objects {
                if seen_objects.insert(iri.clone()) {
                    crate::realtime::emit_entity_referenced_with_tx(&app_for_notify, iri, batch_tx);
                }
            }

            // Match creation-queries against the written triples in memory.
            // For each written triple that matches (predicate, object_value), check whether
            // the subject has the required rdf:type. The type check goes to the DB only when
            // there is at least one creation-query registered — avoiding unnecessary reads.
            if let Some(registry) = app_for_notify.try_state::<crate::realtime::SubscriptionRegistry>() {
                let queries = registry.creation_queries();
                if !queries.is_empty() {
                    // Collect candidate (subject, predicate, object_value, tx) tuples whose
                    // predicate+object matches at least one creation-query.
                    let candidates: Vec<(String, String, String, i64)> = written_triples.iter()
                        .filter_map(|t| {
                            let obj = t.object_iri.as_deref().or(t.object_value.as_deref())?;
                            if queries.iter().any(|q| q.predicate == t.predicate && q.object_value == obj) {
                                Some((t.subject.clone(), t.predicate.clone(), obj.to_owned(), t.tx))
                            } else {
                                None
                            }
                        })
                        .collect();

                    if !candidates.is_empty() {
                        // Resolve rdf:type for candidate subjects via a single read.
                        let app_for_join = app_for_notify.clone();
                        let executor_for_join = executor_for_type_resolve.clone();
                        let queries_for_join = queries.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = executor_for_join.read(move |conn| {
                                for (subject, predicate, object_value, tx) in &candidates {
                                    // Resolve the current rdf:type of the subject.
                                    let subject_type: Option<String> = conn.query_row(
                                        "SELECT object FROM triples
                                         WHERE subject = ?1 AND predicate = 'rdf:type'
                                           AND retracted = 0
                                           AND tx = (SELECT MAX(tx) FROM triples
                                                      WHERE subject = ?1 AND predicate = 'rdf:type')",
                                        rusqlite::params![subject],
                                        |row| row.get(0),
                                    ).ok();

                                    if let Some(ref stype) = subject_type {
                                        for query in &queries_for_join {
                                            if query.class_iri == *stype
                                                && query.predicate == *predicate
                                                && query.object_value == *object_value
                                            {
                                                crate::realtime::emit_entity_joined_set(
                                                    &app_for_join,
                                                    subject,
                                                    &query.class_iri,
                                                    predicate,
                                                    object_value,
                                                    *tx,
                                                );
                                            }
                                        }
                                    }
                                }
                                Ok(String::new())
                            }).await;
                        });
                    }
                }
            }

            // Search reindex is unconditional — always runs regardless of subscriptions.
            if !seen_subjects.is_empty() {
                let executor = executor_for_reindex.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = executor.read(move |conn| {
                        crate::search::reindex_subjects(conn, &seen_subjects);
                        Ok(String::new())
                    }).await;
                });
            }
        }
    });

    let worker = FormulaWorker::spawn(app.clone(), executor_for_worker);
    super::log_backend("debug", &format!("[STARTUP] formula_worker={}ms", t0.elapsed().as_millis()));

    let _ = app.emit("import-progress", serde_json::json!({ "stage": "Recovering pending processes" }));
    recover_pending_jobs(&executor_for_recover, &worker).await;
    super::log_backend("debug", &format!("[STARTUP] recover_jobs={}ms", t0.elapsed().as_millis()));

    app.manage(worker);

    let query_worker_executor = app.state::<DbExecutor>().inner().clone();
    let query_worker = crate::owl::query_worker::QueryWorker::spawn(app.clone(), query_worker_executor);
    app.manage(query_worker);
    crate::owl::query_worker::register_listener(app.clone());
    super::log_backend("debug", &format!("[STARTUP] query_worker={}ms", t0.elapsed().as_millis()));

    app.manage(crate::process_automation::executor::ActiveExecutions::new());

    let app_for_executions = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::process_automation::executor::recover_interrupted_executions(&app_for_executions).await;
    });
    super::log_backend("debug", "[STARTUP] recover_executions=spawned (background)");

    let app_watchdog = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;
            crate::process_automation::executor::recover_interrupted_executions(&app_watchdog).await;
        }
    });
    super::log_backend("debug", "[STARTUP] execution_watchdog=spawned (10min interval)");

    tauri::async_runtime::spawn(crate::process_automation::scheduler::reload(app.clone()));
    tauri::async_runtime::spawn(crate::process_automation::task_scheduler::start(app.clone()));
    tauri::async_runtime::spawn(crate::imap::sync_worker::start_imap_sync(app.clone()));
    super::log_backend("debug", &format!("[STARTUP] total_before_emit={}ms", t0.elapsed().as_millis()));

    let _ = app.emit("import-complete", ());

    let retention_executor = app.state::<DbExecutor>().inner().clone();
    let retention_app = app.clone();
    tauri::async_runtime::spawn(async move {
        retention_app.emit("retention-started", ()).ok();
        crate::commands::chat::retention::run_retention_policy(&retention_executor).await;
        retention_app.emit("retention-complete", ()).ok();
    });

    Ok(())
}

async fn recover_pending_jobs(
    executor: &crate::owl::DbExecutor,
    worker: &FormulaWorker,
) {
    use crate::owl::formula_worker::WorkerCommand;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let _ = executor.write(move |conn| {
        conn.execute(
            "UPDATE formula_recalc_jobs SET status = 'pending', updated_at = ? WHERE status = 'running'",
            rusqlite::params![now],
        ).map(|_| String::new()).map_err(|e| e.to_string())
    }).await;

    let job_ids: Vec<String> = executor.read(|conn| {
        let mut stmt = match conn.prepare(
            "SELECT id FROM formula_recalc_jobs WHERE status = 'pending' ORDER BY created_at",
        ) {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let rows = match stmt.query_map([], |row| row.get(0)) {
            Ok(r) => r,
            Err(_) => return Ok(vec![]),
        };
        Ok(rows.filter_map(|r| r.ok()).collect())
    }).await.unwrap_or_default();

    for job_id in job_ids {
        let _ = worker.sender.send(WorkerCommand::Enqueue { job_id }).await;
    }
}

fn purge_old_retractions(conn: &Connection) {
    const SEVEN_DAYS_MS: i64 = 7 * 24 * 60 * 60 * 1000;
    let cutoff_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
        - SEVEN_DAYS_MS;

    match purge_superseded_triples(conn, cutoff_ms) {
        Ok(n) if n > 0 => super::log_backend("info", &format!("[STARTUP] Purged {} superseded triples older than 7 days", n)),
        Ok(_) => {}
        Err(e) => super::log_backend("warn", &format!("[STARTUP] Failed to purge old retractions: {}", e)),
    }
}

fn purge_superseded_triples(conn: &Connection, cutoff_ms: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM triples WHERE created_at < ?1 AND is_current = 0",
        rusqlite::params![cutoff_ms],
    )
}

#[cfg(test)]
mod tests {
    use super::purge_superseded_triples;
    use crate::eavto::test_helpers::setup_test_db;

    fn insert_triple(conn: &rusqlite::Connection, subject: &str, predicate: &str, value: &str, tx: i64, created_at: i64, retracted: i64, is_current: i64) {
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, tx, origin_id, created_at, retracted, is_current)
             VALUES (?1, ?2, ?3, 'literal', ?4, 1, ?5, ?6, ?7)",
            rusqlite::params![subject, predicate, value, tx, created_at, retracted, is_current],
        ).unwrap();
    }

    fn insert_tx(conn: &rusqlite::Connection, tx: i64) {
        conn.execute(
            "INSERT INTO transactions (tx, origin, created_at) VALUES (?1, 'test', 0)",
            rusqlite::params![tx],
        ).unwrap();
    }

    fn count_rows(conn: &rusqlite::Connection, subject: &str, predicate: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject=?1 AND predicate=?2",
            rusqlite::params![subject, predicate],
            |r| r.get(0),
        ).unwrap()
    }

    const OLD: i64 = 1_000_000;
    const RECENT: i64 = i64::MAX;
    const CUTOFF: i64 = 1_000_000_000;

    #[test]
    fn test_purge_deletes_superseded_old_rows() {
        let conn = setup_test_db();
        insert_tx(&conn, 1);
        insert_tx(&conn, 2);
        // tx=1 group (old, superseded by tx=2)
        insert_triple(&conn, "ex:A", "ex:p", "v1", 1, OLD, 0, 0);
        // tx=2 group (current)
        insert_triple(&conn, "ex:A", "ex:p", "v2", 2, OLD, 0, 1);

        let deleted = purge_superseded_triples(&conn, CUTOFF).unwrap();

        assert_eq!(deleted, 1);
        assert_eq!(count_rows(&conn, "ex:A", "ex:p"), 1);
        let val: String = conn.query_row(
            "SELECT object_value FROM triples WHERE subject='ex:A' AND predicate='ex:p'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "v2");
    }

    #[test]
    fn test_purge_never_deletes_current_group_even_if_ancient() {
        let conn = setup_test_db();
        insert_tx(&conn, 1);
        // Only one tx — it IS the max, so it must never be deleted
        insert_triple(&conn, "ex:A", "ex:p", "v1", 1, OLD, 0, 1);

        let deleted = purge_superseded_triples(&conn, CUTOFF).unwrap();

        assert_eq!(deleted, 0);
        assert_eq!(count_rows(&conn, "ex:A", "ex:p"), 1);
    }

    #[test]
    fn test_purge_never_deletes_sentinel_even_if_ancient() {
        // Sentinel = retracted=1 row at MAX(tx); property is empty after purge
        let conn = setup_test_db();
        insert_tx(&conn, 1);
        insert_tx(&conn, 2);
        // tx=1 group (old active values, now superseded)
        insert_triple(&conn, "ex:A", "ex:p", "v1", 1, OLD, 0, 0);
        // tx=2 sentinel (marks property as empty, retracted=1, it IS the current max)
        insert_triple(&conn, "ex:A", "ex:p", "sentinel", 2, OLD, 1, 1);

        let deleted = purge_superseded_triples(&conn, CUTOFF).unwrap();

        // Only tx=1 row (superseded) must be deleted; sentinel at tx=2 survives
        assert_eq!(deleted, 1);
        assert_eq!(count_rows(&conn, "ex:A", "ex:p"), 1);
        let (val, retracted): (String, i64) = conn.query_row(
            "SELECT object_value, retracted FROM triples WHERE subject='ex:A' AND predicate='ex:p'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(val, "sentinel");
        assert_eq!(retracted, 1, "sentinel row must survive");
        // Confirm triples_current returns nothing (property is empty)
        let current_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples_current WHERE subject='ex:A' AND predicate='ex:p'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(current_count, 0, "property must still appear empty after purge");
    }

    #[test]
    fn test_purge_does_not_delete_recent_superseded_rows() {
        let conn = setup_test_db();
        insert_tx(&conn, 1);
        insert_tx(&conn, 2);
        // tx=1 group is superseded but was created recently — must not be deleted
        insert_triple(&conn, "ex:A", "ex:p", "v1", 1, RECENT, 0, 0);
        insert_triple(&conn, "ex:A", "ex:p", "v2", 2, RECENT, 0, 1);

        let deleted = purge_superseded_triples(&conn, CUTOFF).unwrap();

        assert_eq!(deleted, 0);
        assert_eq!(count_rows(&conn, "ex:A", "ex:p"), 2);
    }

    #[test]
    fn test_purge_deletes_entire_old_superseded_multivalue_group() {
        let conn = setup_test_db();
        insert_tx(&conn, 1);
        insert_tx(&conn, 2);
        // tx=1 group has 3 values (old, superseded)
        insert_triple(&conn, "ex:A", "ex:tags", "a", 1, OLD, 0, 0);
        insert_triple(&conn, "ex:A", "ex:tags", "b", 1, OLD, 0, 0);
        insert_triple(&conn, "ex:A", "ex:tags", "c", 1, OLD, 0, 0);
        // tx=2 group is current
        insert_triple(&conn, "ex:A", "ex:tags", "d", 2, OLD, 0, 1);

        let deleted = purge_superseded_triples(&conn, CUTOFF).unwrap();

        assert_eq!(deleted, 3, "all 3 rows of the superseded group must be deleted");
        assert_eq!(count_rows(&conn, "ex:A", "ex:tags"), 1);
    }
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub already_setup: bool,
    pub user: UserInfo,
    pub computer: ComputerInfo,
    pub foundation: FoundationInfo,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub iri: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessorInfo {
    pub iri: String,
    pub model: String,
    pub cores: Option<i64>,
    pub architecture: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    pub iri: String,
    pub capacity_gb: i64,
    pub memory_type: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatingSystemInfo {
    pub iri: String,
    pub name: String,
    pub version: String,
    pub kernel: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerInfo {
    pub iri: String,
    pub hostname: String,
    pub operating_system: OperatingSystemInfo,
    pub processor: ProcessorInfo,
    pub memory: MemoryInfo,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareReleaseInfo {
    pub iri: String,
    pub version_number: String,
    pub license_type: Option<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundationInfo {
    pub iri: String,
    pub release: SoftwareReleaseInfo,
}

/// Check if setup has been completed
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__check(
    executor: State<'_, DbExecutor>,
) -> Result<bool, String> {
    executor.read(|conn| {
        // The ontology references foundation:ThisUser as an object, but never asserts
        // rdf:type for it. setup__init creates ThisUser as foundation:Person — that
        // triple is the reliable signal that the wizard was completed.
        conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject='foundation:ThisUser' AND predicate='rdf:type' AND object='foundation:Person' AND retracted=0",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .map_err(|e| format!("Failed to check setup status: {}", e))
    }).await
}

/// Deletes foundation:ThisFoundationInstance to force the setup wizard on next launch.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__reset(
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(|conn| {
        Individual::retract(conn, "foundation:ThisFoundationInstance", "setup")
            .map_err(|e| format!("Failed to retract setup: {}", e))
            .map(|_| String::new())
    }).await?;
    app.restart();
}

/// Initialize setup: detect system, create instances, establish relationships
/// Should only be called when setup__check returns false
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__init(
    user_name: String,
    email: Option<String>,
    ai_service_iri: Option<String>,
    ai_model_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<SetupResult, String> {
    // Setup involves writes, so we use the write executor
    let result_json = executor.write(move |conn| {

    // Don't check again - assume caller used setup__check first

    let hostname = hostname::get()
        .map_err(|e| format!("Failed to get hostname: {}", e))?
        .to_string_lossy()
        .to_string();

    let os_info = get_os_info();
    let cpu_info = get_cpu_info();
    let memory_info = get_memory_info();

    let user = Individual::new("foundation:ThisUser");
    user.assert(conn, "foundation:Person", &user_name, "person", "setup")
        .map_err(|e| format!("Failed to create Person: {}", e))?;

    let name_obj = Object::Literal {
        value: user_name.clone(),
        datatype: Some("xsd:string".to_string()),
        language: Some("en".to_string()),
    };
    user.add_property(conn, "foundation:name", vec![name_obj], "setup")
        .map_err(|e| format!("Failed to add user name: {}", e))?;

    if let Some(ref email_val) = email {
        let email_obj = Object::Literal {
            value: email_val.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        user.add_property(conn, "foundation:email", vec![email_obj], "setup")
            .map_err(|e| format!("Failed to add user email: {}", e))?;
    }

    let processor = Individual::new("foundation:ThisProcessor");
    processor.assert(conn, "foundation:Processor", &cpu_info.model, "computer", "setup")
        .map_err(|e| format!("Failed to create Processor: {}", e))?;

    let model_obj = Object::Literal {
        value: cpu_info.model.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    processor.add_property(conn, "foundation:processorModel", vec![model_obj], "setup")
        .map_err(|e| format!("Failed to add processor model: {}", e))?;

    if let Some(cores) = cpu_info.cores {
        processor.add_property(conn, "foundation:coreCount", vec![Object::Integer(cores)], "setup")
            .map_err(|e| format!("Failed to add core count: {}", e))?;
    }

    let arch_obj = Object::Literal {
        value: cpu_info.architecture.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    processor.add_property(conn, "foundation:architecture", vec![arch_obj], "setup")
        .map_err(|e| format!("Failed to add architecture: {}", e))?;

    let memory = Individual::new("foundation:ThisMemory");
    let memory_label = format!("{}GB RAM", memory_info.capacity_gb);
    memory.assert(conn, "foundation:Memory", &memory_label, "computer", "setup")
        .map_err(|e| format!("Failed to create Memory: {}", e))?;

    memory.add_property(
        conn,
        "foundation:memoryCapacity",
        vec![Object::Integer(memory_info.capacity_gb)],
        "setup",
    ).map_err(|e| format!("Failed to add memory capacity: {}", e))?;

    let mem_type_obj = Object::Literal {
        value: memory_info.memory_type.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    memory.add_property(conn, "foundation:memoryType", vec![mem_type_obj], "setup")
        .map_err(|e| format!("Failed to add memory type: {}", e))?;

    let os = Individual::new("foundation:ThisOperatingSystem");
    let os_label = format!("{} {}", os_info.name, os_info.version);
    os.assert(conn, "foundation:OperatingSystem", &os_label, "computer", "setup")
        .map_err(|e| format!("Failed to create OperatingSystem: {}", e))?;

    let os_name_obj = Object::Literal {
        value: os_info.name.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osName", vec![os_name_obj], "setup")
        .map_err(|e| format!("Failed to add OS name: {}", e))?;

    let os_version_obj = Object::Literal {
        value: os_info.version.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osVersion", vec![os_version_obj], "setup")
        .map_err(|e| format!("Failed to add OS version: {}", e))?;

    let os_kernel_obj = Object::Literal {
        value: os_info.kernel.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osKernel", vec![os_kernel_obj], "setup")
        .map_err(|e| format!("Failed to add OS kernel: {}", e))?;

    let computer = Individual::new("foundation:ThisComputer");
    computer.assert(conn, "foundation:Computer", &hostname, "computer", "setup")
        .map_err(|e| format!("Failed to create Computer: {}", e))?;

    let hostname_obj = Object::Literal {
        value: hostname.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    computer.add_property(conn, "foundation:hostname", vec![hostname_obj], "setup")
        .map_err(|e| format!("Failed to add hostname: {}", e))?;

    computer.add_property(
        conn,
        "foundation:hasProcessor",
        vec![Object::Iri("foundation:ThisProcessor".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link Computer -> Processor: {}", e))?;

    computer.add_property(
        conn,
        "foundation:hasMemory",
        vec![Object::Iri("foundation:ThisMemory".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link Computer -> Memory: {}", e))?;

    computer.add_property(
        conn,
        "foundation:hasOperatingSystem",
        vec![Object::Iri("foundation:ThisOperatingSystem".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link Computer -> OperatingSystem: {}", e))?;

    let version = env!("CARGO_PKG_VERSION").to_string();

    let releases = Individual::find_by_class_and_properties(
        conn,
        "foundation:SoftwareRelease",
        &[
            ("foundation:versionNumber", &version),
            ("foundation:releaseOf", "foundation:FoundationProduct"),
        ]
    ).map_err(|e| format!("Failed to query for release: {}", e))?;

    let release_iri = releases.first().ok_or_else(|| {
        format!(
            "SoftwareRelease for FOUNDATION version {} not found in ontology. \
             Please create it via MCP tools before releasing.",
            version
        )
    })?.clone();

    let foundation_label = format!("FOUNDATION v{}", version);
    let foundation = Individual::new("foundation:ThisFoundationInstance");
    foundation.assert(conn, "foundation:Application", &foundation_label, "apps", "setup")
        .map_err(|e| format!("Failed to create Application instance: {}", e))?;

    foundation.add_property(
        conn,
        "foundation:installedFrom",
        vec![Object::Iri(release_iri.clone())],
        "setup",
    ).map_err(|e| format!("Failed to link to SoftwareRelease: {}", e))?;

    let ai_assistant = Individual::new("foundation:LocalAIAssistant");
    ai_assistant.assert(conn, "foundation:SoftwareAgent", "FOUNDATION AI Assistant", "smart_toy", "setup")
        .map_err(|e| format!("Failed to create AI assistant: {}", e))?;

    let service_iri = ai_service_iri.unwrap_or_else(|| "foundation:ClaudeAIService".to_string());

    let ai_description = Object::Literal {
        value: format!("AI assistant powered by {}", service_iri),
        datatype: Some("xsd:string".to_string()),
        language: Some("en".to_string()),
    };
    ai_assistant.add_property(conn, "rdfs:comment", vec![ai_description], "setup")
        .map_err(|e| format!("Failed to add AI description: {}", e))?;

    ai_assistant.add_property(
        conn,
        "foundation:usesService",
        vec![Object::Iri(service_iri.clone())],
        "setup"
    ).map_err(|e| format!("Failed to link AI to service: {}", e))?;

    if let Some(model_iri) = ai_model_iri {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let model_setting_iri = format!("foundation:AIModelSetting_{}", timestamp);
        let model_setting = Individual::new(&model_setting_iri);
        model_setting.assert(
            conn,
            "foundation:SoftwareSetting",
            "Selected AI Model",
            "settings",
            "setup",
        ).map_err(|e| format!("Failed to create model setting: {}", e))?;

        model_setting.add_property(conn, "foundation:settingKey", vec![Object::Literal {
            value: "aiModel".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "setup").map_err(|e| format!("Failed to set settingKey: {}", e))?;

        model_setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
            value: model_iri,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "setup").map_err(|e| format!("Failed to set settingValue: {}", e))?;

        model_setting.add_property(conn, "foundation:settingCategory", vec![Object::Literal {
            value: "ai".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "setup").map_err(|e| format!("Failed to set settingCategory: {}", e))?;

        model_setting.add_property(
            conn,
            "foundation:origin",
            vec![Object::Iri("foundation:ThisFoundationInstance".to_string())],
            "setup"
        ).map_err(|e| format!("Failed to set origin: {}", e))?;

        model_setting.add_property(
            conn,
            "foundation:appliedTo",
            vec![Object::Iri(service_iri)],
            "setup"
        ).map_err(|e| format!("Failed to set appliedTo: {}", e))?;
    }

    // retraction is automatic when setting the same property, so this is idempotent
    let locale_info = get_locale_info();

    let language_setting = Individual::get(conn, "foundation:DefaultLanguageSetting")
        .map_err(|e| format!("Failed to get DefaultLanguageSetting: {}", e))?
        .ok_or_else(|| "DefaultLanguageSetting not found".to_string())?;
    language_setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
        value: locale_info.language.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "setup").map_err(|e| format!("Failed to update language setting: {}", e))?;

    let locale_setting = Individual::get(conn, "foundation:DefaultLocaleSetting")
        .map_err(|e| format!("Failed to get DefaultLocaleSetting: {}", e))?
        .ok_or_else(|| "DefaultLocaleSetting not found".to_string())?;
    locale_setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
        value: locale_info.locale.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "setup").map_err(|e| format!("Failed to update locale setting: {}", e))?;

    let country_setting = Individual::get(conn, "foundation:DefaultCountrySetting")
        .map_err(|e| format!("Failed to get DefaultCountrySetting: {}", e))?
        .ok_or_else(|| "DefaultCountrySetting not found".to_string())?;
    country_setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
        value: locale_info.country.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "setup").map_err(|e| format!("Failed to update country setting: {}", e))?;

    computer.add_property(
        conn,
        "foundation:hasUser",
        vec![Object::Iri("foundation:ThisUser".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link Computer -> User: {}", e))?;
    foundation.add_property(
        conn,
        "foundation:runsOn",
        vec![Object::Iri("foundation:ThisComputer".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link FOUNDATION -> Computer: {}", e))?;

    let result = SetupResult {
        already_setup: false,
        user: UserInfo {
            iri: "foundation:ThisUser".to_string(),
            name: user_name,
            email,
        },
        computer: ComputerInfo {
            iri: "foundation:ThisComputer".to_string(),
            hostname,
            operating_system: OperatingSystemInfo {
                iri: "foundation:ThisOperatingSystem".to_string(),
                name: os_info.name,
                version: os_info.version,
                kernel: os_info.kernel,
            },
            processor: ProcessorInfo {
                iri: "foundation:ThisProcessor".to_string(),
                model: cpu_info.model,
                cores: cpu_info.cores,
                architecture: cpu_info.architecture,
            },
            memory: MemoryInfo {
                iri: "foundation:ThisMemory".to_string(),
                capacity_gb: memory_info.capacity_gb,
                memory_type: memory_info.memory_type,
            },
        },
        foundation: FoundationInfo {
            iri: "foundation:ThisFoundationInstance".to_string(),
            release: SoftwareReleaseInfo {
                iri: release_iri,
                version_number: version,
                license_type: Some("MIT".to_string()),
            },
        },
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await?;

    serde_json::from_str(&result_json).map_err(|e| e.to_string())
}

