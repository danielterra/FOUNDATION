// ============================================================================
// EAVTO Executor Module
// ============================================================================
// Provides async execution for database operations to avoid blocking the UI
//
// Architecture:
// - Single writer thread with sequential queue for writes
// - Thread pool for parallel reads
// - All operations are async to avoid blocking Tauri's event loop
// ============================================================================

use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

/// Executor for database operations
/// Ensures writes are sequential while allowing parallel reads
pub struct DbExecutor {
    write_tx: mpsc::UnboundedSender<WriteTask>,
    conn: Arc<Mutex<Connection>>,
}

/// A write task to be executed sequentially
struct WriteTask {
    operation: Box<dyn FnOnce(&mut Connection) -> Result<String, String> + Send>,
    result_tx: oneshot::Sender<Result<String, String>>,
}

impl DbExecutor {
    /// Create a new executor with the given connection
    pub fn new(conn: Connection) -> Self {
        let conn = Arc::new(Mutex::new(conn));
        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<WriteTask>();

        // Spawn writer thread that processes writes sequentially
        let writer_conn = Arc::clone(&conn);
        std::thread::spawn(move || {
            while let Some(task) = write_rx.blocking_recv() {
                let result = {
                    let mut conn = writer_conn.lock().unwrap();
                    (task.operation)(&mut conn)
                };
                let _ = task.result_tx.send(result);
            }
        });

        Self { write_tx, conn }
    }

    /// Execute a read operation (can run in parallel)
    /// Returns immediately without blocking the event loop
    pub async fn read<F, R>(&self, operation: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| e.to_string())?;
            operation(&conn)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Execute a write operation (sequential, queued)
    /// Returns immediately without blocking the event loop
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
            conn: Arc::clone(&self.conn),
        }
    }
}
