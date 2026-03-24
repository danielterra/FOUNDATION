// ============================================================================
// EAVTO Executor Module
// ============================================================================
// Provides async execution for database operations to avoid blocking the UI
//
// Architecture:
// - Single writer thread with sequential queue for writes
// - Each read opens its own connection (no locking — store is append-only)
// - WAL mode allows concurrent reads and writes at the SQLite file level
// - All operations are async to avoid blocking Tauri's event loop
// ============================================================================

use rusqlite::Connection;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

/// Executor for database operations.
/// Writes are sequential (single writer thread). Reads are fully concurrent
/// (each spawns its own connection — safe because the store is append-only).
pub struct DbExecutor {
    write_tx: mpsc::UnboundedSender<WriteTask>,
    db_path: PathBuf,
    /// Sends (subjects, iri_objects) written by each transaction so callers can emit events.
    notify_tx: Option<mpsc::UnboundedSender<(Vec<String>, Vec<String>)>>,
}

/// A write task to be executed sequentially
struct WriteTask {
    operation: Box<dyn FnOnce(&mut Connection) -> Result<String, String> + Send>,
    result_tx: oneshot::Sender<Result<String, String>>,
}

impl DbExecutor {
    /// Create a new executor. The given `conn` becomes the dedicated write connection.
    /// `db_path` is used by read operations to open independent connections.
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

        std::thread::spawn(move || {
            let mut write_conn = conn;
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
            }
        });

        Self { write_tx, db_path, notify_tx }
    }

    /// Create an executor backed by an in-memory database (for CI/test use only).
    /// Reads always open a fresh empty in-memory DB, so only the write connection
    /// holds state — reads will return empty results.
    pub fn new_in_memory(conn: Connection) -> Self {
        Self::new(conn, PathBuf::from(":memory:"))
    }

    /// Execute a read operation (fully concurrent — opens its own connection).
    pub async fn read<F, R>(&self, operation: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = Connection::open(&path).map_err(|e| e.to_string())?;
            conn.busy_timeout(std::time::Duration::from_secs(30)).map_err(|e| e.to_string())?;
            operation(&conn)
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
        }
    }
}
