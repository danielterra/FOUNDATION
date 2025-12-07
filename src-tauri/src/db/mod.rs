// ============================================================================
// Database Module
// ============================================================================
// Manages SQLite database initialization and connection lifecycle
//
// The database file is created in the user's app data directory:
// - macOS: ~/Library/Application Support/com.FOUNDATION.app/FOUNDATION.db
// - Windows: %APPDATA%/com.FOUNDATION.app/FOUNDATION.db
// - Linux: ~/.config/com.FOUNDATION.app/FOUNDATION.db
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

        let db_path = project_root.join("FOUNDATION.db");
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
            .join("com.FOUNDATION.app");

        // Create directory if it doesn't exist
        if !app_dir.exists() {
            fs::create_dir_all(&app_dir)?;
        }

        Ok(app_dir.join("FOUNDATION.db"))
    }
}

/// Check if database file exists
pub fn db_exists() -> bool {
    match get_db_path() {
        Ok(path) => path.exists(),
        Err(_) => false,
    }
}

/// SQL schema for database initialization
const SCHEMA_SQL: &str = include_str!("../../../db/schema.sql");

/// RDF/RDFS/OWL core ontology
const RDF_CORE_TTL: &str = include_str!("../../../core-ontology/rdf-rdfs-owl-core.ttl");

/// DTYPE (Datatype Schema) ontology
const DTYPE_TTL: &str = include_str!("../../../core-ontology/dtype.ttl");

/// Create database schema
fn create_schema(conn: &Connection) -> Result<(), DbError> {
    println!("📋 Creating schema...");
    conn.execute_batch(SCHEMA_SQL)?;
    println!("✅ Schema created");
    Ok(())
}

/// Import RDF/RDFS/OWL core ontology from embedded string
fn import_rdf_core(conn: &Connection) -> Result<(), DbError> {
    println!("\n📚 Importing RDF/RDFS/OWL core ontology...");

    // Write embedded content to a temporary file
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("rdf-rdfs-owl-core.ttl");

    std::fs::write(&temp_file, RDF_CORE_TTL)
        .map_err(|e| DbError::IoError(e))?;

    let stats = crate::ontology::import_turtle_file(
        conn,
        &temp_file,
        "core"
    ).map_err(|e| DbError::SchemaError(format!("RDF core import failed: {:?}", e)))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    println!("✅ Imported {} triples from RDF/RDFS/OWL", stats.triples_processed);
    Ok(())
}

/// Import DTYPE ontology from embedded string
fn import_dtype(conn: &Connection) -> Result<(), DbError> {
    println!("\n📚 Importing DTYPE ontology...");

    // Write embedded content to a temporary file
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("dtype.ttl");

    std::fs::write(&temp_file, DTYPE_TTL)
        .map_err(|e| DbError::IoError(e))?;

    let stats = crate::ontology::import_turtle_file(
        conn,
        &temp_file,
        "core"
    ).map_err(|e| DbError::SchemaError(format!("DTYPE import failed: {:?}", e)))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    println!("✅ Imported {} triples from DTYPE", stats.triples_processed);
    Ok(())
}

/// Initialize database with schema and ontology
pub fn initialize_db(db_path: &Path) -> Result<Connection, DbError> {
    let needs_initialization = !db_path.exists();

    println!("Using database at: {:?}", db_path);
    let conn = Connection::open(db_path)?;

    if needs_initialization {
        println!("\n🚀 Initializing new database...\n");

        // Create schema
        create_schema(&conn)?;

        // Import RDF/RDFS/OWL core
        import_rdf_core(&conn)?;

        // Import DTYPE ontology
        import_dtype(&conn)?;

        // Import FOUNDATION ontology
        crate::ontology::import_all_foundation_ontologies(&conn)
            .map_err(|e| DbError::SchemaError(format!("Ontology import failed: {:?}", e)))?;

        // Set metadata
        println!("\n⚙️  Setting metadata...");
        conn.execute(
            "UPDATE metadata SET value = 'true', updated_at = ? WHERE key = 'ontology_imported'",
            [std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64],
        )?;
        println!("✅ Metadata updated");

        println!("\n✅ Database initialization complete!");
    }

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
        let test_db_path = temp_dir.join("FOUNDATION_test.db");

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
        let test_db_path = temp_dir.join("FOUNDATION_test_stats.db");

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
