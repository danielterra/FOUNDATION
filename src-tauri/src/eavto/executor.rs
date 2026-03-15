// ============================================================================
// EAVTO Executor Module
// ============================================================================
// Provides async execution for database operations.
//
// Architecture:
// - Writes are serialized via a Mutex to prevent concurrent write conflicts.
// - Each read opens its own Connection (safe — store is append-only, WAL mode).
// - All operations are natively async (no spawn_blocking needed with turso).
// ============================================================================

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use turso::{Connection, Database};

/// Executor for database operations.
/// Writes are serialized via a Mutex. Reads open independent connections concurrently.
pub struct DbExecutor {
    db: Arc<Database>,
    write_lock: Arc<Mutex<()>>,
    db_path: PathBuf,
}

impl DbExecutor {
    /// Create a new executor backed by the given database.
    /// `db_path` is stored for reference; all connections are opened via `db.connect()`.
    pub fn new(db: Database, db_path: PathBuf) -> Self {
        Self {
            db: Arc::new(db),
            write_lock: Arc::new(Mutex::new(())),
            db_path,
        }
    }

    /// Create an executor backed by an in-memory database (for CI/test use only).
    pub fn new_in_memory(db: Database) -> Self {
        Self::new(db, PathBuf::from(":memory:"))
    }

    /// Execute a read operation (fully concurrent — opens its own connection).
    pub async fn read<F, Fut, R>(&self, operation: F) -> Result<R, String>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: Future<Output = Result<R, String>>,
    {
        let conn = self.db.connect().map_err(|e| e.to_string())?;
        operation(conn).await
    }

    /// Execute a write operation (serialized via Mutex).
    pub async fn write<F, Fut>(&self, operation: F) -> Result<String, String>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: Future<Output = Result<String, String>>,
    {
        let _lock = self.write_lock.lock().await;
        let conn = self.db.connect().map_err(|e| e.to_string())?;
        operation(conn).await
    }
}

impl Clone for DbExecutor {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            write_lock: self.write_lock.clone(),
            db_path: self.db_path.clone(),
        }
    }
}
