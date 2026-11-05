use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::watch;

use super::setup_system_info::{get_cpu_info, get_locale_info, get_memory_info, get_os_info};

#[derive(Clone)]
pub struct StartupRetentionGate {
    sender: watch::Sender<bool>,
}

#[derive(Clone, Default)]
pub struct SetupEmailVerificationState {
    verifications: Arc<Mutex<HashMap<String, SetupEmailVerification>>>,
    sequence: Arc<AtomicU64>,
}

#[derive(Clone)]
struct SetupEmailVerification {
    email: String,
    account_fingerprint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupEmailTestResult {
    pub message: String,
    pub verification_token: String,
}

pub fn setup_email_account_fingerprint(
    host: &str,
    port: u16,
    use_tls: bool,
    username: &str,
    password: &str,
) -> String {
    let mut digest = Sha256::new();
    let port_bytes = port.to_be_bytes();
    let tls_byte = [u8::from(use_tls)];
    for part in [
        host.as_bytes(),
        port_bytes.as_slice(),
        tls_byte.as_slice(),
        username.as_bytes(),
        password.as_bytes(),
    ] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    format!("{:x}", digest.finalize())
}

impl SetupEmailVerificationState {
    pub fn record_success(&self, email: &str, account_fingerprint: &str) -> Result<String, String> {
        let email = email.trim().to_ascii_lowercase();
        if email.is_empty() {
            return Err("Email is required to verify the account".to_string());
        }

        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let token_material = format!(
            "{}:{}:{}:{}",
            email,
            account_fingerprint,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos(),
            sequence
        );
        let token = format!("{:x}", Sha256::digest(token_material.as_bytes()));
        let mut verifications = self
            .verifications
            .lock()
            .map_err(|_| "Email verification state is unavailable".to_string())?;
        verifications.retain(|_, verification| verification.email != email);
        verifications.insert(
            token.clone(),
            SetupEmailVerification {
                email,
                account_fingerprint: account_fingerprint.to_string(),
            },
        );
        Ok(token)
    }

    pub fn validate(
        &self,
        token: &str,
        email: &str,
        account_fingerprint: &str,
    ) -> Result<(), String> {
        let verifications = self
            .verifications
            .lock()
            .map_err(|_| "Email verification state is unavailable".to_string())?;
        match verifications.get(token) {
            Some(verification)
                if verification.email == email.trim().to_ascii_lowercase()
                    && verification.account_fingerprint == account_fingerprint =>
            {
                Ok(())
            }
            _ => Err(
                "A successful email account test matching the saved account is required"
                    .to_string(),
            ),
        }
    }

    pub fn consume(&self, token: &str) -> Result<(), String> {
        self.verifications
            .lock()
            .map_err(|_| "Email verification state is unavailable".to_string())?
            .remove(token);
        Ok(())
    }
}

impl Default for StartupRetentionGate {
    fn default() -> Self {
        let (sender, _) = watch::channel(false);
        Self { sender }
    }
}

impl StartupRetentionGate {
    pub async fn wait(&self) {
        let mut receiver = self.sender.subscribe();
        while !*receiver.borrow_and_update() {
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }

    fn mark_complete(&self) {
        self.sender.send_replace(true);
    }
}
use crate::owl::{self, Connection, DbExecutor, Individual, Object};

/// Initialize the application database
/// This MUST be called before any other commands that use the database
/// The frontend should call this on startup and handle permission requests
#[tauri::command]
#[allow(non_snake_case)]
pub async fn initialize_app(app: AppHandle) -> Result<(), String> {
    let retention_gate = StartupRetentionGate::default();

    // Skip initialization during CI/CD — the runner doesn't have a real user DB
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        super::log_backend("info", "Skipping database initialization (build/CI mode)");
        let dummy_conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to create in-memory connection: {}", e))?;
        let executor = DbExecutor::new_in_memory(dummy_conn);
        app.manage(executor);
        retention_gate.mark_complete();
        app.manage(retention_gate);
        let _ = app.emit("import-complete", ());
        return Ok(());
    }

    let (mut conn, db_path) = owl::initialize_with_progress(app.clone()).map_err(|e| {
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
        let attachments_glob = format!(
            "{}/**",
            attachments.display().to_string().replace('\\', "/")
        );

        if let Err(e) = app
            .asset_protocol_scope()
            .allow_directory(&attachments, true)
        {
            super::log_backend(
                "warn",
                &format!(
                    "Failed to allow attachments dir in asset protocol scope ({}): {}",
                    attachments.display(),
                    e
                ),
            );
        }

        let cap = tauri::ipc::CapabilityBuilder::new("attachments-dir-runtime")
            .window("main")
            .permission_scoped(
                "opener:allow-open-path",
                vec![serde_json::json!({ "path": attachments_glob })],
                Vec::<serde_json::Value>::new(),
            );
        if let Err(e) = app.add_capability(cap) {
            super::log_backend(
                "warn",
                &format!(
                    "Failed to register runtime opener capability for {}: {}",
                    attachments.display(),
                    e
                ),
            );
        }
    }

    let _ = app.emit(
        "import-progress",
        serde_json::json!({ "stage": "Loading icon library" }),
    );
    owl::seed_icon_library(&mut conn);

    // Drain the WAL now — only one connection exists so no readers can block this.
    // Pool connections maintain WAL read marks that prevent PASSIVE checkpoints from advancing,
    // causing the WAL to grow unboundedly during normal operation.
    let _ = app.emit(
        "import-progress",
        serde_json::json!({ "stage": "Compacting transaction log" }),
    );
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

    let _ = app.emit(
        "import-progress",
        serde_json::json!({ "stage": "Cleaning up old data" }),
    );
    purge_old_retractions(&conn);

    let t0 = std::time::Instant::now();

    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel::<(
        std::collections::HashMap<String, Vec<String>>,
        Vec<String>,
        Vec<crate::eavto::WrittenTriple>,
    )>();
    let executor = DbExecutor::new_with_notify(conn, db_path.clone(), Some(notify_tx));
    executor
        .write(|conn| {
            crate::commands::migrate_legacy_active_ai_model(conn)?;
            Ok(String::new())
        })
        .await
        .map_err(|error| format!("Failed to migrate active AI model configuration: {}", error))?;
    let executor_for_worker = executor.clone();
    app.manage(executor);
    app.manage(retention_gate.clone());
    super::log_backend(
        "debug",
        &format!("[STARTUP] executor_ready={}ms", t0.elapsed().as_millis()),
    );

    // Log DB stats in the background — COUNT queries are slow (~500ms on large DBs)
    let stats_executor = app.state::<DbExecutor>().inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(stats) = stats_executor
            .read(|conn| owl::get_stats(conn).map_err(|e| e.to_string()))
            .await
        {
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
        while let Some((subject_predicates, iri_objects, written_triples)) = notify_rx.recv().await
        {
            // Derive the max tx from this batch for the ring and event payloads.
            let batch_tx = written_triples.iter().map(|t| t.tx).max().unwrap_or(0);

            let mut seen_subjects: Vec<String> = Vec::new();
            for (iri, predicates) in &subject_predicates {
                crate::realtime::emit_entity_updated_with_tx(
                    &app_for_notify,
                    iri,
                    predicates,
                    batch_tx,
                );
                // Non-gated backend signal: task execution / recurrence / scheduler must react
                // to every write, independent of whether the frontend is displaying the entity.
                let entity_tx = written_triples
                    .iter()
                    .filter(|triple| triple.subject.as_str() == iri)
                    .map(|triple| triple.tx)
                    .max()
                    .unwrap_or(batch_tx);
                crate::realtime::emit_internal_write_update(
                    &app_for_notify,
                    iri,
                    predicates,
                    entity_tx,
                );
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
            if let Some(registry) =
                app_for_notify.try_state::<crate::realtime::SubscriptionRegistry>()
            {
                let queries = registry.creation_queries();
                if !queries.is_empty() {
                    // Collect candidate (subject, predicate, object_value, tx) tuples whose
                    // predicate+object matches at least one creation-query.
                    let candidates: Vec<(String, String, String, i64)> = written_triples
                        .iter()
                        .filter_map(|t| {
                            let obj = t.object_iri.as_deref().or(t.object_value.as_deref())?;
                            if queries
                                .iter()
                                .any(|q| q.predicate == t.predicate && q.object_value == obj)
                            {
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
                            let _ = executor_for_join
                                .read(move |conn| {
                                    for (subject, predicate, object_value, tx) in &candidates {
                                        // Resolve the current rdf:type of the subject.
                                        let subject_type: Option<String> = conn
                                            .query_row(
                                                "SELECT object FROM triples
                                         WHERE subject = ?1 AND predicate = 'rdf:type'
                                           AND retracted = 0 AND is_current = 1",
                                                rusqlite::params![subject],
                                                |row| row.get(0),
                                            )
                                            .ok();

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
                                })
                                .await;
                        });
                    }
                }
            }

            // Search reindex is unconditional — always runs regardless of subscriptions.
            if !seen_subjects.is_empty() {
                let executor = executor_for_reindex.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = executor
                        .read(move |conn| {
                            crate::search::reindex_subjects(conn, &seen_subjects);
                            Ok(String::new())
                        })
                        .await;
                });
            }
        }
    });

    let worker =
        crate::commands::formula_worker::FormulaWorker::spawn(app.clone(), executor_for_worker);
    super::log_backend(
        "debug",
        &format!("[STARTUP] formula_worker={}ms", t0.elapsed().as_millis()),
    );

    app.manage(worker);
    crate::commands::formula_worker::spawn_startup_recovery(
        app.clone(),
        app.state::<DbExecutor>().inner().clone(),
    );

    // Values built on TODAY()/NOW() have no write to react to — they go stale when
    // the date changes. Refresh them at startup and once a day.
    crate::commands::formula_worker::spawn_daily_clock_refresh(
        app.clone(),
        app.state::<DbExecutor>().inner().clone(),
    );

    let query_worker_executor = app.state::<DbExecutor>().inner().clone();
    let query_worker =
        crate::commands::query_worker::QueryWorker::spawn(app.clone(), query_worker_executor);
    app.manage(query_worker);
    crate::commands::query_worker::register_listener(app.clone());
    super::log_backend(
        "debug",
        &format!("[STARTUP] query_worker={}ms", t0.elapsed().as_millis()),
    );

    app.manage(crate::process_automation::executor::ActiveExecutions::new());

    let app_for_executions = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::process_automation::executor::recover_interrupted_executions(&app_for_executions)
            .await;
    });
    super::log_backend("debug", "[STARTUP] recover_executions=spawned (background)");

    let app_watchdog = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;
            crate::process_automation::executor::recover_interrupted_executions(&app_watchdog)
                .await;
        }
    });
    super::log_backend(
        "debug",
        "[STARTUP] execution_watchdog=spawned (10min interval)",
    );

    tauri::async_runtime::spawn(crate::process_automation::scheduler::reload(app.clone()));
    tauri::async_runtime::spawn(crate::process_automation::task_scheduler::start(
        app.clone(),
    ));
    tauri::async_runtime::spawn(crate::imap::sync_worker::start_imap_sync(app.clone()));
    crate::data_sync::request_worker::start_request_worker(app.clone());
    super::log_backend(
        "debug",
        &format!("[STARTUP] total_before_emit={}ms", t0.elapsed().as_millis()),
    );

    let retention_executor = app.state::<DbExecutor>().inner().clone();
    app.emit("retention-started", ()).ok();
    crate::commands::chat::retention::run_retention_policy(&retention_executor).await;
    retention_gate.mark_complete();
    app.emit("retention-complete", ()).ok();

    let _ = app.emit("import-complete", ());

    Ok(())
}

fn purge_old_retractions(conn: &Connection) {
    let retention_days = Individual::get(conn, "foundation:SoftwareSetting_1781110491934")
        .ok()
        .flatten()
        .and_then(|s| s.properties.iter()
            .find(|(k, _)| k == "foundation:settingValue")
            .and_then(|(_, v)| v.as_literal())
            .and_then(|v| v.parse::<u64>().ok()))
        .unwrap_or_else(|| {
            super::log_backend("warn", "[STARTUP] HistoryRetentionDaysSetting missing or unparseable — defaulting to 7 days");
            7
        });

    if retention_days == 0 {
        super::log_backend(
            "info",
            "[STARTUP] History retention set to 0 — skipping purge (eternal history)",
        );
        return;
    }

    let retention_ms = (retention_days as i64) * 24 * 60 * 60 * 1000;
    let cutoff_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
        - retention_ms;

    match purge_superseded_triples(conn, cutoff_ms) {
        Ok(n) if n > 0 => super::log_backend(
            "info",
            &format!(
                "[STARTUP] Purged {} superseded triples older than {} days",
                n, retention_days
            ),
        ),
        Ok(_) => {}
        Err(e) => super::log_backend(
            "warn",
            &format!("[STARTUP] Failed to purge old retractions: {}", e),
        ),
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
    use super::{purge_superseded_triples, StartupRetentionGate};
    use crate::eavto::test_helpers::setup_test_db;

    #[tokio::test]
    async fn retention_gate_releases_recovery_only_after_completion() {
        let gate = StartupRetentionGate::default();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), gate.wait())
                .await
                .is_err()
        );

        gate.mark_complete();

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), gate.wait())
                .await
                .is_ok()
        );
    }

    fn insert_triple(
        conn: &rusqlite::Connection,
        subject: &str,
        predicate: &str,
        value: &str,
        tx: i64,
        created_at: i64,
        retracted: i64,
        is_current: i64,
    ) {
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
        )
        .unwrap();
    }

    fn count_rows(conn: &rusqlite::Connection, subject: &str, predicate: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject=?1 AND predicate=?2",
            rusqlite::params![subject, predicate],
            |r| r.get(0),
        )
        .unwrap()
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
        let val: String = conn
            .query_row(
                "SELECT object_value FROM triples WHERE subject='ex:A' AND predicate='ex:p'",
                [],
                |r| r.get(0),
            )
            .unwrap();
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
        let current_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples_current WHERE subject='ex:A' AND predicate='ex:p'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            current_count, 0,
            "property must still appear empty after purge"
        );
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

        assert_eq!(
            deleted, 3,
            "all 3 rows of the superseded group must be deleted"
        );
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
fn setup_check_operation(conn: &mut crate::owl::Connection) -> Result<String, String> {
    match crate::core_ontology::setup::ensure_setup_complete(conn) {
        Ok(is_complete) => Ok(is_complete.to_string()),
        Err(error) => Err(format!("Failed to check setup status: {}", error)),
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__check(executor: State<'_, DbExecutor>) -> Result<bool, String> {
    let result = executor.write(setup_check_operation).await?;
    match result.parse::<bool>() {
        Ok(is_complete) => Ok(is_complete),
        Err(error) => Err(format!("Failed to parse setup status: {}", error)),
    }
}

/// Deletes foundation:ThisFoundationInstance to force the setup wizard on next launch.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__reset(
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(|conn| {
            Individual::retract(conn, "foundation:ThisFoundationInstance", "setup")
                .map_err(|e| format!("Failed to retract setup: {}", e))
                .map(|_| String::new())
        })
        .await?;
    app.restart();
}

/// Initialize setup: detect system, create instances, establish relationships
/// Should only be called when setup__check returns false
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__init(
    user_name: String,
    email: Option<String>,
    email_account_iri: String,
    email_test_token: String,
    ai_integration_mode: String,
    ai_service_iri: Option<String>,
    ai_model_iri: Option<String>,
    app: tauri::AppHandle,
    verification_state: State<'_, SetupEmailVerificationState>,
    executor: State<'_, DbExecutor>,
) -> Result<SetupResult, String> {
    let email = email
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Email is required to complete setup".to_string())?;
    if user_name.trim().is_empty() {
        return Err("Name is required to complete setup".to_string());
    }
    let ai_mode = crate::commands::SetupAIIntegrationMode::parse(&ai_integration_mode)?;
    let verification_state = verification_state.inner().clone();
    let verification_state_for_write = verification_state.clone();
    let token_to_consume = email_test_token.clone();
    let app_for_ai_validation = app.clone();

    // Setup involves writes, so we use the write executor
    let result_json = executor
        .write(move |conn| {
            // Don't check again - assume caller used setup__check first

            crate::commands::validate_setup_ai_selection(
                conn,
                ai_mode,
                ai_service_iri.as_deref(),
                ai_model_iri.as_deref(),
                &app_for_ai_validation,
            )?;
            let email_account = Individual::get(conn, &email_account_iri)
                .map_err(|error| format!("Failed to load tested email account: {}", error))?
                .ok_or_else(|| format!("Tested email account '{}' not found", email_account_iri))?;
            let account_class_iris =
                owl::find_entities_with_property(conn, "rdf:type", "foundation:IMAPAccount")
                    .map_err(|error| {
                        format!("Failed to validate tested email account: {}", error)
                    })?;
            if !account_class_iris
                .iter()
                .any(|iri| iri == &email_account_iri)
            {
                return Err(format!(
                    "Entity '{}' is not an IMAP account",
                    email_account_iri
                ));
            }
            let account_string = |property: &str| {
                email_account
                    .properties
                    .iter()
                    .find(|(key, _)| key == property)
                    .and_then(|(_, value)| value.as_literal())
            };
            let account_host = account_string("foundation:imapHost")
                .ok_or_else(|| "Tested email account has no IMAP host".to_string())?;
            let account_port = account_string("foundation:imapPort")
                .and_then(|value| value.parse::<u16>().ok())
                .ok_or_else(|| "Tested email account has no valid IMAP port".to_string())?;
            let account_tls = account_string("foundation:imapUseTLS")
                .and_then(|value| value.parse::<bool>().ok())
                .ok_or_else(|| "Tested email account has no valid TLS setting".to_string())?;
            let account_username = account_string("foundation:imapUsername")
                .ok_or_else(|| "Tested email account has no IMAP username".to_string())?;
            let account_credential_iri = email_account
                .properties
                .iter()
                .find(|(key, _)| key == "foundation:hasCredential")
                .and_then(|(_, value)| value.as_iri())
                .ok_or_else(|| "Tested email account has no password credential".to_string())?;
            let account_password = owl::get_literal_property(
                conn,
                account_credential_iri,
                "foundation:credentialValue",
            )
            .map_err(|error| format!("Failed to read email account credential: {}", error))?
            .ok_or_else(|| "Tested email account password is missing".to_string())?;
            if account_username.trim().to_ascii_lowercase() != email.to_ascii_lowercase() {
                return Err(
                    "The tested email login must match the person's canonical email".to_string(),
                );
            }
            let account_fingerprint = setup_email_account_fingerprint(
                &account_host,
                account_port,
                account_tls,
                &account_username,
                &account_password,
            );
            verification_state_for_write.validate(
                &email_test_token,
                &email,
                &account_fingerprint,
            )?;

            let hostname = hostname::get()
                .map_err(|e| format!("Failed to get hostname: {}", e))?
                .to_string_lossy()
                .to_string();

            let os_info = get_os_info();
            let cpu_info = get_cpu_info();
            let memory_info = get_memory_info();

            let processor = Individual::new("foundation:ThisProcessor");
            processor
                .assert(
                    conn,
                    "foundation:Processor",
                    &cpu_info.model,
                    "computer",
                    "setup",
                )
                .map_err(|e| format!("Failed to create Processor: {}", e))?;

            let model_obj = Object::Literal {
                value: cpu_info.model.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            };
            processor
                .add_property(conn, "foundation:processorModel", vec![model_obj], "setup")
                .map_err(|e| format!("Failed to add processor model: {}", e))?;

            if let Some(cores) = cpu_info.cores {
                processor
                    .add_property(
                        conn,
                        "foundation:coreCount",
                        vec![Object::Integer(cores)],
                        "setup",
                    )
                    .map_err(|e| format!("Failed to add core count: {}", e))?;
            }

            let arch_obj = Object::Literal {
                value: cpu_info.architecture.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            };
            processor
                .add_property(conn, "foundation:architecture", vec![arch_obj], "setup")
                .map_err(|e| format!("Failed to add architecture: {}", e))?;

            let memory = Individual::new("foundation:ThisMemory");
            let memory_label = format!("{}GB RAM", memory_info.capacity_gb);
            memory
                .assert(
                    conn,
                    "foundation:Memory",
                    &memory_label,
                    "computer",
                    "setup",
                )
                .map_err(|e| format!("Failed to create Memory: {}", e))?;

            memory
                .add_property(
                    conn,
                    "foundation:memoryCapacity",
                    vec![Object::Integer(memory_info.capacity_gb)],
                    "setup",
                )
                .map_err(|e| format!("Failed to add memory capacity: {}", e))?;

            let mem_type_obj = Object::Literal {
                value: memory_info.memory_type.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            };
            memory
                .add_property(conn, "foundation:memoryType", vec![mem_type_obj], "setup")
                .map_err(|e| format!("Failed to add memory type: {}", e))?;

            let os = Individual::new("foundation:ThisOperatingSystem");
            let os_label = format!("{} {}", os_info.name, os_info.version);
            os.assert(
                conn,
                "foundation:OperatingSystem",
                &os_label,
                "computer",
                "setup",
            )
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
            computer
                .assert(conn, "foundation:Computer", &hostname, "computer", "setup")
                .map_err(|e| format!("Failed to create Computer: {}", e))?;

            let hostname_obj = Object::Literal {
                value: hostname.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            };
            computer
                .add_property(conn, "foundation:hostname", vec![hostname_obj], "setup")
                .map_err(|e| format!("Failed to add hostname: {}", e))?;

            computer
                .add_property(
                    conn,
                    "foundation:hasProcessor",
                    vec![Object::Iri("foundation:ThisProcessor".to_string())],
                    "setup",
                )
                .map_err(|e| format!("Failed to link Computer -> Processor: {}", e))?;

            computer
                .add_property(
                    conn,
                    "foundation:hasMemory",
                    vec![Object::Iri("foundation:ThisMemory".to_string())],
                    "setup",
                )
                .map_err(|e| format!("Failed to link Computer -> Memory: {}", e))?;

            computer
                .add_property(
                    conn,
                    "foundation:hasOperatingSystem",
                    vec![Object::Iri("foundation:ThisOperatingSystem".to_string())],
                    "setup",
                )
                .map_err(|e| format!("Failed to link Computer -> OperatingSystem: {}", e))?;

            let version = env!("CARGO_PKG_VERSION").to_string();

            let releases = Individual::find_by_class_and_properties(
                conn,
                "foundation:SoftwareRelease",
                &[
                    ("foundation:versionNumber", &version),
                    ("foundation:releaseOf", "foundation:FoundationProduct"),
                ],
            )
            .map_err(|e| format!("Failed to query for release: {}", e))?;

            let release_iri = releases
                .first()
                .ok_or_else(|| {
                    format!(
                        "SoftwareRelease for FOUNDATION version {} not found in ontology. \
             Please create it via MCP tools before releasing.",
                        version
                    )
                })?
                .clone();

            let foundation_label = format!("FOUNDATION v{}", version);
            let foundation = Individual::new("foundation:ThisFoundationInstance");
            foundation
                .assert(
                    conn,
                    "foundation:Application",
                    &foundation_label,
                    "apps",
                    "setup",
                )
                .map_err(|e| format!("Failed to create Application instance: {}", e))?;

            foundation
                .add_property(
                    conn,
                    "foundation:installedFrom",
                    vec![Object::Iri(release_iri.clone())],
                    "setup",
                )
                .map_err(|e| format!("Failed to link to SoftwareRelease: {}", e))?;

            let ai_assistant = Individual::new("foundation:LocalAIAssistant");
            ai_assistant
                .assert(
                    conn,
                    "foundation:SoftwareAgent",
                    "FOUNDATION AI Assistant",
                    "smart_toy",
                    "setup",
                )
                .map_err(|e| format!("Failed to create AI assistant: {}", e))?;

            crate::commands::persist_setup_ai_mode(conn, ai_mode)?;
            if ai_mode == crate::commands::SetupAIIntegrationMode::Chat {
                let service_iri = ai_service_iri
                    .as_deref()
                    .ok_or_else(|| "Chat mode requires an AI service".to_string())?;
                let model_iri = ai_model_iri
                    .as_deref()
                    .ok_or_else(|| "Chat mode requires an AI model".to_string())?;
                let ai_description = Object::Literal {
                    value: format!("AI assistant powered by {}", service_iri),
                    datatype: Some("xsd:string".to_string()),
                    language: Some("en".to_string()),
                };
                ai_assistant
                    .add_property(conn, "rdfs:comment", vec![ai_description], "setup")
                    .map_err(|e| format!("Failed to add AI description: {}", e))?;
                ai_assistant
                    .add_property(
                        conn,
                        "foundation:usesService",
                        vec![Object::Iri(service_iri.to_string())],
                        "setup",
                    )
                    .map_err(|e| format!("Failed to link AI to service: {}", e))?;
                ai_assistant
                    .add_property(
                        conn,
                        "foundation:usesModel",
                        vec![Object::Iri(model_iri.to_string())],
                        "setup",
                    )
                    .map_err(|e| format!("Failed to link AI to model: {}", e))?;
                crate::commands::persist_setup_ai_service(conn, service_iri)?;
                crate::commands::set_setup_ai_model_for_service(conn, service_iri, model_iri)?;
                let model_setting = Individual::get(conn, "foundation:DefaultAIModelSetting")
                    .map_err(|e| format!("Failed to get DefaultAIModelSetting: {}", e))?
                    .ok_or_else(|| "DefaultAIModelSetting not found".to_string())?;
                model_setting
                    .add_property(
                        conn,
                        "foundation:settingValue",
                        vec![Object::Literal {
                            value: model_iri.to_string(),
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        }],
                        "setup",
                    )
                    .map_err(|e| format!("Failed to set selected AI model: {}", e))?;
                model_setting
                    .add_property(
                        conn,
                        "foundation:appliedTo",
                        vec![Object::Iri(service_iri.to_string())],
                        "setup",
                    )
                    .map_err(|e| format!("Failed to set AI model service: {}", e))?;
            }

            // retraction is automatic when setting the same property, so this is idempotent
            let locale_info = get_locale_info();

            let language_setting = Individual::get(conn, "foundation:DefaultLanguageSetting")
                .map_err(|e| format!("Failed to get DefaultLanguageSetting: {}", e))?
                .ok_or_else(|| "DefaultLanguageSetting not found".to_string())?;
            language_setting
                .add_property(
                    conn,
                    "foundation:settingValue",
                    vec![Object::Literal {
                        value: locale_info.language.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "setup",
                )
                .map_err(|e| format!("Failed to update language setting: {}", e))?;

            let locale_setting = Individual::get(conn, "foundation:DefaultLocaleSetting")
                .map_err(|e| format!("Failed to get DefaultLocaleSetting: {}", e))?
                .ok_or_else(|| "DefaultLocaleSetting not found".to_string())?;
            locale_setting
                .add_property(
                    conn,
                    "foundation:settingValue",
                    vec![Object::Literal {
                        value: locale_info.locale.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "setup",
                )
                .map_err(|e| format!("Failed to update locale setting: {}", e))?;

            let country_setting = Individual::get(conn, "foundation:DefaultCountrySetting")
                .map_err(|e| format!("Failed to get DefaultCountrySetting: {}", e))?
                .ok_or_else(|| "DefaultCountrySetting not found".to_string())?;
            country_setting
                .add_property(
                    conn,
                    "foundation:settingValue",
                    vec![Object::Literal {
                        value: locale_info.country.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "setup",
                )
                .map_err(|e| format!("Failed to update country setting: {}", e))?;

            let user = Individual::new("foundation:ThisUser");
            user.assert(conn, "foundation:Person", &user_name, "person", "setup")
                .map_err(|e| format!("Failed to create Person: {}", e))?;
            user.add_property(
                conn,
                "foundation:name",
                vec![Object::Literal {
                    value: user_name.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: Some("en".to_string()),
                }],
                "setup",
            )
            .map_err(|e| format!("Failed to add user name: {}", e))?;
            user.add_property(
                conn,
                "foundation:email",
                vec![Object::Literal {
                    value: email.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "setup",
            )
            .map_err(|e| format!("Failed to add user email: {}", e))?;

            foundation
                .add_property(
                    conn,
                    "foundation:runsOn",
                    vec![Object::Iri("foundation:ThisComputer".to_string())],
                    "setup",
                )
                .map_err(|e| format!("Failed to link FOUNDATION -> Computer: {}", e))?;

            let result = SetupResult {
                already_setup: false,
                user: UserInfo {
                    iri: "foundation:ThisUser".to_string(),
                    name: user_name,
                    email: Some(email),
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

            let result_json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
            computer
                .add_property(
                    conn,
                    "foundation:hasUser",
                    vec![Object::Iri("foundation:ThisUser".to_string())],
                    "setup",
                )
                .map_err(|e| format!("Failed to link Computer -> User: {}", e))?;

            Ok(result_json)
        })
        .await?;

    verification_state.consume(&token_to_consume)?;

    serde_json::from_str(&result_json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod setup_email_verification_tests {
    use super::{setup_email_account_fingerprint, SetupEmailVerificationState};

    #[test]
    fn setup_email_test_is_bound_to_email_and_saved_credentials() {
        let state = SetupEmailVerificationState::default();
        let fingerprint = setup_email_account_fingerprint(
            "imap.example.test",
            993,
            true,
            "person@example.test",
            "password-one",
        );
        let token = state
            .record_success("person@example.test", &fingerprint)
            .unwrap();

        assert!(state
            .validate(&token, "PERSON@example.test", &fingerprint)
            .is_ok());
        assert!(state
            .validate(&token, "other@example.test", &fingerprint)
            .is_err());
        assert!(state
            .validate(
                &token,
                "person@example.test",
                &setup_email_account_fingerprint(
                    "imap.example.test",
                    993,
                    true,
                    "person@example.test",
                    "password-two",
                ),
            )
            .is_err());
    }

    #[test]
    fn newer_email_test_replaces_previous_token_for_same_identity() {
        let state = SetupEmailVerificationState::default();
        let first_fingerprint = setup_email_account_fingerprint(
            "imap.example.test",
            993,
            true,
            "person@example.test",
            "password-one",
        );
        let second_fingerprint = setup_email_account_fingerprint(
            "imap.example.test",
            993,
            true,
            "person@example.test",
            "password-two",
        );
        let first_token = state
            .record_success("person@example.test", &first_fingerprint)
            .unwrap();
        let second_token = state
            .record_success("person@example.test", &second_fingerprint)
            .unwrap();

        assert!(state
            .validate(&first_token, "person@example.test", &first_fingerprint)
            .is_err());
        assert!(state
            .validate(&second_token, "person@example.test", &second_fingerprint)
            .is_ok());
    }
}
