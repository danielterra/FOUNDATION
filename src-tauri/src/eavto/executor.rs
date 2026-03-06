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
        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<WriteTask>();

        std::thread::spawn(move || {
            let mut write_conn = conn;
            while let Some(task) = write_rx.blocking_recv() {
                let result = (task.operation)(&mut write_conn);
                let _ = task.result_tx.send(result);
            }
        });

        Self { write_tx, db_path }
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
            conn.execute_batch("PRAGMA busy_timeout=5000;").map_err(|e| e.to_string())?;
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
        }
    }
}
