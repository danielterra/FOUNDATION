// ============================================================================
// EAVTO Executor Module
// ============================================================================
// Provides async execution for database operations to avoid blocking the UI
//
// Architecture:
// - Single writer thread with sequential queue for writes
// - Read pool of N pre-warmed persistent connections (N = clamp(2× logical cores,
//   min 8, max 16)) — avoids the WAL scan overhead on every call (SQLite must scan
//   the entire WAL to build a read snapshot when opening a new connection; with a
//   large WAL this dominates read latency)
// - Admission control: read() acquires a semaphore permit (capacity = N) BEFORE
//   spawn_blocking, so at most N reads are in flight at any time. Callers queue
//   instead of opening unbounded temporary connections, which would cause WAL
//   oversubscription and read-mark buildup
// - Pool drain after a TRUNCATE checkpoint leaves it empty; read() rehydrates by
//   opening a fresh connection (bounded by the semaphore, not by pool size)
// - WAL mode allows concurrent reads and writes at the SQLite file level
// - All operations are async to avoid blocking Tauri's event loop
// ============================================================================

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot, Semaphore, OwnedSemaphorePermit};
use crate::eavto::store::WrittenTriple;

const WAL_TRUNCATE_INTERVAL: u32 = 200;
const WAL_PASSIVE_INTERVAL: u32 = 50;

/// Warn when a read caller waits this long for a semaphore permit — indicates
/// sustained overload or a permit leak.
const PERMIT_WARN_SECS: u64 = 1;

fn compute_pool_size() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (cores * 2).clamp(8, 16)
}

/// Executor for database operations.
/// Writes are sequential (single writer thread). Reads reuse a pool of
/// pre-warmed persistent connections; admission is controlled by a semaphore
/// so at most N reads are in flight concurrently.
pub struct DbExecutor {
    write_tx: mpsc::UnboundedSender<WriteTask>,
    db_path: PathBuf,
    /// Sends (subject_predicates, iri_objects, written_triples) written by each transaction so callers can emit events.
    notify_tx: Option<mpsc::UnboundedSender<(HashMap<String, Vec<String>>, Vec<String>, Vec<WrittenTriple>)>>,
    read_pool: Arc<Mutex<Vec<Connection>>>,
    /// Limits concurrent reads to pool capacity — callers wait here instead of
    /// opening unbounded temporary connections.
    read_semaphore: Arc<Semaphore>,
    /// Pre-warmed pool capacity (= semaphore permits = N).
    read_pool_cap: usize,
    /// Dedicated read connection for background reactors (e.g. query_worker).
    /// Held outside the shared pool and semaphore so reactor reads never compete
    /// with foreground reads for permits, preventing pool starvation on bulk writes.
    /// The Mutex is the single-at-a-time guard; the worker is sequential so there
    /// is no real contention — the lock just upholds the "one reader per connection"
    /// SQLite invariant.
    unmetered_conn: Arc<Mutex<Connection>>,
}

/// A write task to be executed sequentially
struct WriteTask {
    operation: Box<dyn FnOnce(&mut Connection) -> Result<String, String> + Send>,
    result_tx: oneshot::Sender<Result<String, String>>,
}

fn open_pool_connection(db_path: &PathBuf) -> Option<Connection> {
    let conn = Connection::open(db_path).ok()?;
    conn.busy_timeout(std::time::Duration::from_secs(30)).ok()?;
    Some(conn)
}

impl DbExecutor {
    /// Create a new executor. The given `conn` becomes the dedicated write connection.
    /// `db_path` is used by the read pool to open persistent connections at startup.
    pub fn new(conn: Connection, db_path: PathBuf) -> Self {
        Self::new_with_notify(conn, db_path, None)
    }

    /// Like `new`, but also sends (subject_predicates, iri_objects, written_triples) to `notify_tx` after each write.
    /// The receiver emits `entity-updated` for subjects, `entity-referenced` for iri_objects, and
    /// matches creation-queries using written_triples.
    pub fn new_with_notify(
        conn: Connection,
        db_path: PathBuf,
        notify_tx: Option<mpsc::UnboundedSender<(HashMap<String, Vec<String>>, Vec<String>, Vec<WrittenTriple>)>>,
    ) -> Self {
        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<WriteTask>();
        let notify_tx_thread = notify_tx.clone();

        let pool_cap = compute_pool_size();
        let read_semaphore = Arc::new(Semaphore::new(pool_cap));

        // Pre-warm the pool with N persistent connections so the first burst of
        // reads does not pay the WAL-scan cost of opening fresh connections.
        let read_pool = Arc::new(Mutex::new(Vec::<Connection>::with_capacity(pool_cap)));
        {
            let mut guard = read_pool.lock().unwrap_or_else(|e| e.into_inner());
            let mut opened = 0usize;
            for _ in 0..pool_cap {
                match open_pool_connection(&db_path) {
                    Some(c) => { guard.push(c); opened += 1; }
                    None => {
                        crate::diagnostics::log_backend("warn",
                            "[DB] Pre-warm: failed to open a connection, continuing with fewer");
                    }
                }
            }
            crate::diagnostics::log_backend("info", &format!(
                "[DB] Read pool ready: cap={} warmed={}", pool_cap, opened
            ));
        }

        // Open a dedicated read connection for background reactors (query_worker).
        // Kept outside the shared pool and semaphore so reactor reads never starve
        // foreground callers. PRAGMA query_only prevents accidental writes through
        // this connection; busy_timeout is already set by open_pool_connection.
        let unmetered_conn: Arc<Mutex<Connection>> = {
            let conn = open_pool_connection(&db_path)
                .expect("[DB] Failed to open dedicated unmetered read connection — cannot start");
            let _ = conn.execute_batch("PRAGMA query_only = ON;");
            Arc::new(Mutex::new(conn))
        };

        let pool_for_checkpoint = read_pool.clone();

        std::thread::spawn(move || {
            let mut write_conn = conn;
            // Disable SQLite's built-in auto-checkpoint (default: 1000 pages).
            // Without this, every `COMMIT` that crosses the 1000-page WAL threshold
            // runs a passive checkpoint synchronously inside the commit, causing
            // unpredictable multi-hundred-ms stalls on the write thread. Our own
            // explicit checkpoints every WAL_PASSIVE_INTERVAL / WAL_TRUNCATE_INTERVAL
            // writes replace this behaviour and run safely after each task completes.
            let _ = write_conn.execute_batch("PRAGMA wal_autocheckpoint = 0;");
            let mut write_count: u32 = 0;
            while let Some(task) = write_rx.blocking_recv() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    (task.operation)(&mut write_conn)
                })).unwrap_or_else(|e| {
                    let msg = e.downcast_ref::<&str>().copied()
                        .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                        .unwrap_or("unknown panic");
                    Err(format!("write operation panicked: {}", msg))
                });
                if let Some(ref tx) = notify_tx_thread {
                    let subject_predicates = crate::eavto::store::drain_written_subject_predicates();
                    let iri_objects = crate::eavto::store::drain_written_iri_objects();
                    let written_triples = crate::eavto::store::drain_written_triples();
                    if !subject_predicates.is_empty() || !iri_objects.is_empty() || !written_triples.is_empty() {
                        let _ = tx.send((subject_predicates, iri_objects, written_triples));
                    }
                }
                let _ = task.result_tx.send(result);

                write_count += 1;
                if write_count % WAL_TRUNCATE_INTERVAL == 0 {
                    // Pool read-marks block TRUNCATE checkpoints indefinitely — drain the
                    // pool first so the WAL can be zeroed and does not grow unboundedly.
                    // read() rehydrates after the drain: with a permit in hand and an empty
                    // pool it opens a fresh connection rather than waiting, eliminating the
                    // deadlock that existed when read() blocked waiting for a pool connection.
                    let old_conns = {
                        let mut guard = pool_for_checkpoint
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        std::mem::take(&mut *guard)
                    };
                    drop(old_conns);
                    // Retry up to 3× with a brief pause to let in-progress readers finish.
                    let mut truncated = false;
                    for attempt in 0..3u8 {
                        if attempt > 0 {
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }
                        let ckpt = write_conn.query_row(
                            "PRAGMA wal_checkpoint(TRUNCATE)",
                            [],
                            |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?, row.get::<_, i32>(2)?)),
                        );
                        match ckpt {
                            Ok((0, log, done)) => {
                                crate::diagnostics::log_backend("debug", &format!(
                                    "[WAL] TRUNCATE ok (attempt={} log={} done={})", attempt + 1, log, done
                                ));
                                truncated = true;
                                break;
                            }
                            Ok((busy, log, done)) => {
                                crate::diagnostics::log_backend("warn", &format!(
                                    "[WAL] TRUNCATE busy (attempt={} busy={} log={} done={})", attempt + 1, busy, log, done
                                ));
                            }
                            Err(e) => {
                                crate::diagnostics::log_backend("warn", &format!(
                                    "[WAL] TRUNCATE error (attempt={}): {}", attempt + 1, e
                                ));
                            }
                        }
                    }
                    if !truncated {
                        // Fall back to RESTART: resets write position so WAL space is reused
                        // even if it can't be physically truncated right now.
                        let _ = write_conn.execute_batch("PRAGMA wal_checkpoint(RESTART);");
                        crate::diagnostics::log_backend("warn", "[WAL] fell back to RESTART checkpoint");
                    }
                } else if write_count % WAL_PASSIVE_INTERVAL == 0 {
                    let ckpt = write_conn.query_row(
                        "PRAGMA wal_checkpoint(PASSIVE)",
                        [],
                        |row| Ok((row.get::<_, i32>(1)?, row.get::<_, i32>(2)?)),
                    );
                    if let Ok((log, done)) = ckpt {
                        crate::diagnostics::log_backend("debug", &format!(
                            "[WAL] PASSIVE checkpoint log={} done={}", log, done
                        ));
                    }
                }
            }
            // Runs once as the app exits: asks SQLite to update sqlite_stat1 only
            // for tables/indexes that accumulated enough new data to be worth it.
            // Cheap (sub-millisecond when nothing changed significantly) and keeps
            // query-planner estimates fresh across sessions without a full ANALYZE.
            let _ = write_conn.execute_batch("PRAGMA optimize;");
        });

        // Idle WAL checkpoint: every 30 s, drain the pool and attempt TRUNCATE even
        // when no writes are happening. Without this, a large WAL from a busy session
        // never drains during idle periods, causing slow read-connection startup.
        {
            let pool_for_idle = read_pool.clone();
            let write_tx_idle = write_tx.clone();
            std::thread::Builder::new()
                .name("wal-idle-checkpoint".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(30));
                        let pool = pool_for_idle.clone();
                        let (result_tx, _) = oneshot::channel::<Result<String, String>>();
                        let sent = write_tx_idle.send(WriteTask {
                            operation: Box::new(move |conn| {
                                // Check WAL page count before draining the pool.
                                // Draining is only needed to release read marks so TRUNCATE can
                                // zero the WAL file. If the WAL is already empty there is nothing
                                // to do — skip drain + TRUNCATE to keep pool connections alive.
                                let wal_log_pages = conn.query_row(
                                    "PRAGMA wal_checkpoint(PASSIVE)", [],
                                    |row| row.get::<_, i32>(1), // column 1 = wal_frames written
                                ).unwrap_or(1);

                                if wal_log_pages == 0 {
                                    crate::diagnostics::log_backend("debug",
                                        "[WAL] Idle checkpoint skipped: WAL already empty, pool untouched");
                                    return Ok(String::new());
                                }

                                let pool_size_before = pool.lock()
                                    .map(|g| g.len()).unwrap_or(0);
                                let old = {
                                    let mut g = pool.lock().unwrap_or_else(|e| e.into_inner());
                                    std::mem::take(&mut *g)
                                };
                                drop(old);
                                crate::diagnostics::log_backend("debug", &format!(
                                    "[WAL] Idle checkpoint: drained {} pool conns (wal_pages={})",
                                    pool_size_before, wal_log_pages
                                ));
                                // Brief pause so in-progress reads can release their WAL marks.
                                std::thread::sleep(std::time::Duration::from_millis(20));
                                let r = conn.query_row(
                                    "PRAGMA wal_checkpoint(TRUNCATE)", [],
                                    |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?, row.get::<_, i32>(2)?)),
                                );
                                match r {
                                    Ok((0, log, done)) => crate::diagnostics::log_backend("info", &format!(
                                        "[WAL] Idle TRUNCATE ok (log={} done={})", log, done
                                    )),
                                    Ok((_, log, done)) => {
                                        crate::diagnostics::log_backend("warn", &format!(
                                            "[WAL] Idle TRUNCATE busy -- RESTART (log={} done={})", log, done
                                        ));
                                        let _ = conn.execute_batch("PRAGMA wal_checkpoint(RESTART);");
                                    }
                                    Err(e) => crate::diagnostics::log_backend("warn", &format!(
                                        "[WAL] Idle checkpoint error: {}", e
                                    )),
                                }
                                Ok(String::new())
                            }),
                            result_tx,
                        });
                        if sent.is_err() {
                            break; // write channel closed — app is shutting down
                        }
                    }
                })
                .ok();
        }

        Self {
            write_tx,
            db_path,
            notify_tx,
            read_pool,
            read_semaphore,
            read_pool_cap: pool_cap,
            unmetered_conn,
        }
    }

    /// Create an executor backed by an in-memory database (for CI/test use only).
    /// Each read connection is an independent empty in-memory DB — only the write
    /// connection holds state, so reads return empty results. This matches the
    /// previous behaviour and is intentional for isolation in unit tests.
    pub fn new_in_memory(conn: Connection) -> Self {
        Self::new(conn, PathBuf::from(":memory:"))
    }

    /// Execute a read operation using a pooled connection.
    ///
    /// Acquires a semaphore permit before spawning the blocking task so at most
    /// N reads are in flight concurrently. If the pool is empty (e.g. right after
    /// a checkpoint drain) a new connection is opened — the semaphore ensures this
    /// is bounded.
    pub async fn read<F, R>(&self, operation: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let path = self.db_path.clone();
        let pool = self.read_pool.clone();
        let cap = self.read_pool_cap;

        // Acquire permit before spawn_blocking. This is the admission gate: if all
        // N connections are in use, the caller suspends here rather than opening an
        // extra connection that would inflate WAL read-marks and cause oversubscription.
        let t0 = std::time::Instant::now();
        let permit: OwnedSemaphorePermit = self.read_semaphore.clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let wait = t0.elapsed();
        if wait.as_secs() >= PERMIT_WARN_SECS {
            crate::diagnostics::log_backend("warn", &format!(
                "[DB] Read waited {:.1}s for semaphore permit — sustained overload or permit leak",
                wait.as_secs_f64()
            ));
        }

        tokio::task::spawn_blocking(move || {
            // Permit lives inside spawn_blocking so it is released only after the
            // connection is back in the pool, keeping the invariant: permit held ↔
            // connection in use.
            let _permit = permit;

            let conn = match pool.lock().map(|mut g| g.pop()).unwrap_or(None) {
                Some(c) => c,
                None => {
                    // Pool was drained by a checkpoint. Open a fresh connection — bounded
                    // by the semaphore, so this cannot cause unbounded oversubscription.
                    let c = Connection::open(&path).map_err(|e| e.to_string())?;
                    c.busy_timeout(std::time::Duration::from_secs(30)).map_err(|e| e.to_string())?;
                    c
                }
            };

            let result = operation(&conn);

            // Release WAL read mark before returning to pool so idle connections
            // do not block TRUNCATE checkpoints and grow the WAL.
            let _ = conn.execute_batch("BEGIN DEFERRED; COMMIT;");
            if let Ok(mut guard) = pool.lock() {
                if guard.len() < cap {
                    guard.push(conn);
                }
                // Silently drop if pool is at capacity (can happen transiently after
                // a checkpoint re-opens connections beyond the original warmed set).
            }

            result
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Execute a read operation using the dedicated unmetered connection.
    ///
    /// Unlike `read()`, this method does NOT acquire a semaphore permit and does NOT
    /// use the shared pool. It is intended for background reactors (e.g. query_worker)
    /// that must not compete with foreground reads for pool permits. The Mutex ensures
    /// only one unmetered read runs at a time, matching the sequential nature of the
    /// query_worker loop.
    ///
    /// WAL read-mark release (BEGIN DEFERRED; COMMIT;) is performed after each operation
    /// to prevent the persistent connection from pinning a WAL snapshot and blocking
    /// TRUNCATE checkpoints.
    pub async fn read_unmetered<F, R>(&self, operation: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let conn_arc = self.unmetered_conn.clone();

        tokio::task::spawn_blocking(move || {
            let guard = conn_arc.lock().map_err(|e| e.to_string())?;
            let result = operation(&*guard);
            let _ = guard.execute_batch("BEGIN DEFERRED; COMMIT;");
            result
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Execute a write operation (sequential, queued).
    pub async fn write<F>(&self, operation: F) -> Result<String, String>
    where
        F: FnOnce(&mut Connection) -> Result<String, String> + Send + 'static,
    {
        let (result_tx, result_rx) = oneshot::channel();

        let task = WriteTask {
            operation: Box::new(operation),
            result_tx,
        };

        self.write_tx.send(task).map_err(|e| e.to_string())?;
        result_rx.await.map_err(|e| e.to_string())?
    }
}

// Make DbExecutor cloneable so it can be shared across commands
impl Clone for DbExecutor {
    fn clone(&self) -> Self {
        Self {
            write_tx: self.write_tx.clone(),
            db_path: self.db_path.clone(),
            notify_tx: self.notify_tx.clone(),
            read_pool: self.read_pool.clone(),
            read_semaphore: self.read_semaphore.clone(),
            read_pool_cap: self.read_pool_cap,
            unmetered_conn: self.unmetered_conn.clone(),
        }
    }
}
