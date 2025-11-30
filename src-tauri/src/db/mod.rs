// ============================================================================
// Database Module
// ============================================================================
// Manages SQLite database initialization and connection lifecycle
//
// The database file is created in the user's app data directory:
// - macOS: ~/Library/Application Support/com.supernova.app/supernova.db
// - Windows: %APPDATA%/com.supernova.app/supernova.db
// - Linux: ~/.config/com.supernova.app/supernova.db
//
// On first run, the schema is initialized and base ontologies are imported.
// ============================================================================

use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::fs;

/// Database initialization error types
#[derive(Debug)]
pub enum DbError {
    ConnectionError(rusqlite::Error),
    SchemaError(String),
    IoError(std::io::Error),
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

/// Get the path to the database file
/// In development: uses project root database
/// In production: uses user's app data directory
pub fn get_db_path() -> Result<PathBuf, DbError> {
    // In development, use project root database
    #[cfg(debug_assertions)]
    {
        // Get current executable path and navigate to project root
        let exe_path = std::env::current_exe()
            .map_err(|e| DbError::IoError(e))?;

        // Navigate up from target/debug/ to project root
        let project_root = exe_path
            .parent() // target/debug/
            .and_then(|p| p.parent()) // target/
            .and_then(|p| p.parent()) // src-tauri/
            .and_then(|p| p.parent()) // project root
            .ok_or_else(|| DbError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine project root"
            )))?;

        let db_path = project_root.join("supernova.db");
        println!("Development mode: using database at {:?}", db_path);
        return Ok(db_path);
    }

    // In production, use app data directory
    #[cfg(not(debug_assertions))]
    {
        let app_dir = dirs::data_dir()
            .ok_or_else(|| DbError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine app data directory"
            )))?
            .join("com.supernova.app");

        // Create directory if it doesn't exist
        if !app_dir.exists() {
            fs::create_dir_all(&app_dir)?;
        }

        Ok(app_dir.join("supernova.db"))
    }
}

/// Check if database file exists
pub fn db_exists() -> bool {
    match get_db_path() {
        Ok(path) => path.exists(),
        Err(_) => false,
    }
}

/// Initialize database with schema
pub fn initialize_db(db_path: &Path) -> Result<Connection, DbError> {
    // Database must exist - no runtime initialization
    if !db_path.exists() {
        return Err(DbError::SchemaError(format!(
            "Database not found at {:?}. Run 'npm run build:db' to create it.",
            db_path
        )));
    }

    println!("Using database at: {:?}", db_path);
    let conn = Connection::open(db_path)?;
    Ok(conn)
}

/// Get or create database connection
pub fn get_connection() -> Result<Connection, DbError> {
    let db_path = get_db_path()?;
    initialize_db(&db_path)
}

/// Database statistics
#[derive(Debug, serde::Serialize)]
pub struct DbStats {
    pub total_facts: u64,
    pub active_facts: u64,
    pub total_transactions: u64,
    pub entities_count: u64,
    pub ontology_imported: bool,
}

/// Get database statistics
pub fn get_stats(conn: &Connection) -> Result<DbStats, DbError> {
    let total_facts: u64 = conn.query_row(
        "SELECT COUNT(*) FROM triples",
        [],
        |row| row.get(0)
    )?;

    let active_facts: u64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE retracted = 0",
        [],
        |row| row.get(0)
    )?;

    let total_transactions: u64 = conn.query_row(
        "SELECT COUNT(*) FROM transactions",
        [],
        |row| row.get(0)
    )?;

    let entities_count: u64 = conn.query_row(
        "SELECT COUNT(DISTINCT subject) FROM triples WHERE retracted = 0",
        [],
        |row| row.get(0)
    )?;

    let ontology_imported_str: String = conn.query_row(
        "SELECT value FROM metadata WHERE key = 'ontology_imported'",
        [],
        |row| row.get(0)
    ).unwrap_or_else(|_| "false".to_string());

    let ontology_imported = ontology_imported_str == "true";

    Ok(DbStats {
        total_facts,
        active_facts,
        total_transactions,
        entities_count,
        ontology_imported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_db_initialization() {
        let temp_dir = std::env::temp_dir();
        let test_db_path = temp_dir.join("supernova_test.db");

        // Clean up if exists
        if test_db_path.exists() {
            fs::remove_file(&test_db_path).unwrap();
        }

        // Initialize database
        let conn = initialize_db(&test_db_path).expect("Failed to initialize database");

        // Verify tables exist
        let table_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('triples', 'transactions', 'metadata')",
            [],
            |row| row.get(0)
        ).unwrap();

        assert_eq!(table_count, 3, "Expected 3 tables to be created");

        // Verify metadata
        let schema_version: String = conn.query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0)
        ).unwrap();

        assert_eq!(schema_version, "2", "Expected schema version 2");

        // Clean up
        drop(conn);
        fs::remove_file(&test_db_path).unwrap();
    }

    #[test]
    fn test_db_stats() {
        let temp_dir = std::env::temp_dir();
        let test_db_path = temp_dir.join("supernova_test_stats.db");

        // Clean up if exists
        if test_db_path.exists() {
            fs::remove_file(&test_db_path).unwrap();
        }

        let conn = initialize_db(&test_db_path).expect("Failed to initialize database");
        let stats = get_stats(&conn).expect("Failed to get stats");

        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.active_facts, 0);
        assert_eq!(stats.total_transactions, 0);
        assert_eq!(stats.entities_count, 0);
        assert_eq!(stats.ontology_imported, false);

        // Clean up
        drop(conn);
        fs::remove_file(&test_db_path).unwrap();
    }
}
