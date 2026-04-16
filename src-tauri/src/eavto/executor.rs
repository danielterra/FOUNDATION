// ============================================================================
// EAVTO Executor Module
// ============================================================================
// Provides async execution for database operations to avoid blocking the UI
//
// Architecture:
// - Single writer thread with sequential queue for writes
// - Read pool of N persistent connections — avoids the WAL scan overhead on
//   every call (SQLite must scan the entire WAL to build a read snapshot when
//   opening a new connection; with a large WAL this dominates read latency)
// - WAL mode allows concurrent reads and writes at the SQLite file level
// - All operations are async to avoid blocking Tauri's event loop
// ============================================================================

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

const READ_POOL_SIZE: usize = 6;
const WAL_TRUNCATE_INTERVAL: u32 = 200;
const WAL_PASSIVE_INTERVAL: u32 = 50;

/// Executor for database operations.
/// Writes are sequential (single writer thread). Reads reuse a pool of
/// persistent connections so the WAL scan happens once at startup, not per call.
pub struct DbExecutor {
    write_tx: mpsc::UnboundedSender<WriteTask>,
    db_path: PathBuf,
    /// Sends (subjects, iri_objects) written by each transaction so callers can emit events.
    notify_tx: Option<mpsc::UnboundedSender<(Vec<String>, Vec<String>)>>,
    read_pool: Arc<Mutex<Vec<Connection>>>,
}

/// A write task to be executed sequentially
struct WriteTask {
    operation: Box<dyn FnOnce(&mut Connection) -> Result<String, String> + Send>,
    result_tx: oneshot::Sender<Result<String, String>>,
}

impl DbExecutor {
    /// Create a new executor. The given `conn` becomes the dedicated write connection.
    /// `db_path` is used by the read pool to open persistent connections at startup.
    pub fn new(conn: Connection, db_path: PathBuf) -> Self {
        Self::new_with_notify(conn, db_path, None)
    }

    /// Like `new`, but also sends (subjects, iri_objects) to `notify_tx` after each write.
    /// The receiver emits `entity-updated` for subjects and `entity-referenced` for iri_objects.
    pub fn new_with_notify(
        conn: Connection,
        db_path: PathBuf,
        notify_tx: Option<mpsc::UnboundedSender<(Vec<String>, Vec<String>)>>,
    ) -> Self {
        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<WriteTask>();
        let notify_tx_thread = notify_tx.clone();

        // Pool starts empty — connections are added lazily as reads complete.
        // Every 200 writes the pool is drained and a TRUNCATE checkpoint runs so the
        // WAL does not grow unboundedly (pool read-marks would otherwise block PASSIVE
        // checkpoints indefinitely).
        let read_pool = Arc::new(Mutex::new(Vec::<Connection>::new()));
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
                    let subjects = crate::eavto::store::drain_written_subjects();
                    let iri_objects = crate::eavto::store::drain_written_iri_objects();
                    if !subjects.is_empty() || !iri_objects.is_empty() {
                        let _ = tx.send((subjects, iri_objects));
                    }
                }
                let _ = task.result_tx.send(result);

                write_count += 1;
                if write_count % WAL_TRUNCATE_INTERVAL == 0 {
                    // Pool read-marks block TRUNCATE checkpoints indefinitely — drain the
                    // pool first so the WAL can be zeroed and does not grow unboundedly.
                    let old_conns = {
                        let mut guard = pool_for_checkpoint
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        std::mem::take(&mut *guard)
                    };
                    drop(old_conns);
                    let _ = write_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                } else if write_count % WAL_PASSIVE_INTERVAL == 0 {
                    let _ = write_conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
                }
            }
        });

        Self { write_tx, db_path, notify_tx, read_pool }
    }

    /// Create an executor backed by an in-memory database (for CI/test use only).
    /// Reads always open a fresh empty in-memory DB, so only the write connection
    /// holds state — reads will return empty results.
    pub fn new_in_memory(conn: Connection) -> Self {
        Self::new(conn, PathBuf::from(":memory:"))
    }

    /// Execute a read operation using a pooled connection.
    /// If all pool connections are in use, opens a temporary one.
    pub async fn read<F, R>(&self, operation: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let path = self.db_path.clone();
        let pool = self.read_pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn_opt = pool.lock().ok().and_then(|mut g| g.pop());

            let (conn, from_pool) = match conn_opt {
                Some(c) => (c, true),
                None => {
                    let c = Connection::open(&path).map_err(|e| e.to_string())?;
                    c.busy_timeout(std::time::Duration::from_secs(30)).map_err(|e| e.to_string())?;
                    (c, false)
                }
            };

            let result = operation(&conn);

            if from_pool {
                if let Ok(mut guard) = pool.lock() {
                    if guard.len() < READ_POOL_SIZE {
                        guard.push(conn);
                    }
                }
            }

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
        }
    }
}
