use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::fs;
use std::fmt;
use std::error::Error;
use crate::commands::log_backend;

#[derive(Debug)]
pub enum DbError {
    ConnectionError(rusqlite::Error),
    SchemaError(String),
    IoError(std::io::Error),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::ConnectionError(e) => write!(f, "Database connection error: {}", e),
            DbError::SchemaError(msg) => write!(f, "Database schema error: {}", msg),
            DbError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl Error for DbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DbError::ConnectionError(e) => Some(e),
            DbError::IoError(e) => Some(e),
            DbError::SchemaError(_) => None,
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        DbError::ConnectionError(err)
    }
}

impl From<std::io::Error> for DbError {
    fn from(err: std::io::Error) -> Self {
        DbError::IoError(err)
    }
}

pub fn get_db_path() -> Result<PathBuf, DbError> {
    let documents_dir = dirs::document_dir()
        .ok_or_else(|| DbError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine Documents directory"
        )))?;

    let foundation_dir = documents_dir.join("Foundation");

    if !foundation_dir.exists() {
        log_backend("info", &format!("Creating Foundation directory: {:?}", foundation_dir));
        fs::create_dir_all(&foundation_dir)?;
    }

    let db_path = foundation_dir.join("FOUNDATION.db");
    log_backend("info", &format!("Using database: {:?}", db_path));

    Ok(db_path)
}

const SCHEMA_SQL: &str = include_str!("../../../db/schema.sql");
const ONTOLOGY_SQL: &str = include_str!("../../../core-ontology/ontology.sql");

fn create_schema(conn: &Connection) -> Result<(), DbError> {
    log_backend("info", "Creating schema");
    conn.execute_batch(SCHEMA_SQL)?;
    log_backend("info", "Schema created");
    Ok(())
}

fn import_ontology_sql(conn: &Connection) -> Result<(), DbError> {
    log_backend("info", "Importing core ontology from SQL");
    conn.execute_batch(ONTOLOGY_SQL)
        .map_err(|e| DbError::SchemaError(format!("Ontology import failed: {}", e)))?;
    log_backend("info", "Core ontology imported");
    Ok(())
}

#[allow(dead_code)]
pub fn initialize_db(db_path: &Path) -> Result<Connection, DbError> {
    initialize_db_with_progress(db_path, None)
}

fn initialize_db_with_progress(
    db_path: &Path,
    app: Option<&tauri::AppHandle>,
) -> Result<Connection, DbError> {
    use tauri::Emitter;

    let needs_initialization = !db_path.exists();

    log_backend("info", &format!("Using database: {:?}", db_path));
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

    if needs_initialization {
        log_backend("info", "Initializing new database");

        create_schema(&conn)?;
        import_ontology_sql(&conn)?;

        conn.execute(
            "UPDATE metadata SET value = 'true', updated_at = ? WHERE key = 'ontology_imported'",
            [std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock is before Unix epoch")
                .as_millis() as i64],
        )?;

        log_backend("info", "Database initialization complete");
    } else {
        log_backend("info", "Database exists, skipping ontology import");
    }

    run_migrations(&conn)?;

    if let Some(handle) = app {
        let _ = handle.emit("import-complete", ());
    }

    Ok(conn)
}

fn run_migrations(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS formula_recalc_jobs (
            id              TEXT    PRIMARY KEY,
            property_iri    TEXT    NOT NULL,
            property_label  TEXT,
            class_iri       TEXT    NOT NULL,
            class_label     TEXT,
            status          TEXT    NOT NULL DEFAULT 'pending',
            total           INTEGER NOT NULL DEFAULT 0,
            processed       INTEGER NOT NULL DEFAULT 0,
            last_offset     INTEGER NOT NULL DEFAULT 0,
            error_message   TEXT,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS formula_instance_errors (
            instance_iri    TEXT NOT NULL,
            property_iri    TEXT NOT NULL,
            error_message   TEXT NOT NULL,
            created_at      INTEGER NOT NULL,
            PRIMARY KEY (instance_iri, property_iri)
        );
    ")?;
    Ok(())
}

pub fn initialize_with_progress(app: tauri::AppHandle) -> Result<(Connection, PathBuf), DbError> {
    let db_path = get_db_path()?;
    let conn = initialize_db_with_progress(&db_path, Some(&app))?;
    Ok((conn, db_path))
}

#[allow(dead_code)]
pub fn get_connection() -> Result<Connection, DbError> {
    let db_path = get_db_path()?;
    initialize_db(&db_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_db_error_from_rusqlite() {
        let rusqlite_err = rusqlite::Error::InvalidQuery;
        let db_err: DbError = rusqlite_err.into();

        match db_err {
            DbError::ConnectionError(_) => {},
            _ => panic!("Expected ConnectionError"),
        }
    }

    #[test]
    fn test_db_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test error");
        let db_err: DbError = io_err.into();

        match db_err {
            DbError::IoError(_) => {},
            _ => panic!("Expected IoError"),
        }
    }

    #[test]
    fn test_create_schema() {
        let conn = Connection::open_in_memory().expect("Failed to create in-memory db");
        let result = create_schema(&conn);

        assert!(result.is_ok());

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get(0)
        ).expect("Failed to query tables");

        assert!(count > 0, "Schema should create tables");
    }

    #[test]
    fn test_initialize_db_creates_new_database() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");

        assert!(!db_path.exists(), "Database should not exist initially");

        let result = initialize_db(&db_path);

        assert!(result.is_ok(), "Database initialization should succeed");
        assert!(db_path.exists(), "Database file should be created");

        let conn = Connection::open(&db_path).expect("Should open created database");
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get(0)
        ).expect("Failed to query tables");

        assert!(count > 0, "Initialized database should have tables");
    }

    #[test]
    fn test_initialize_db_reuses_existing_database() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("existing.db");

        {
            let conn = Connection::open(&db_path).expect("Failed to create initial db");
            conn.execute_batch(SCHEMA_SQL).expect("Failed to create schema");
        }

        assert!(db_path.exists(), "Database should exist");

        let result = initialize_db(&db_path);

        assert!(result.is_ok(), "Should reuse existing database");
    }

    #[test]
    fn test_get_db_path_returns_path() {
        let result = get_db_path();
        assert!(result.is_ok(), "get_db_path should return a valid path");

        let path = result.unwrap();
        assert!(path.to_str().is_some(), "Path should be valid UTF-8");
        assert!(path.file_name().is_some(), "Path should have a filename");
    }
}
