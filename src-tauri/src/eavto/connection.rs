// ============================================================================
// EAVTO Connection Module
// ============================================================================
// Manages SQLite database connection lifecycle and initialization
//
// Database location:
// - Dev: project_root/FOUNDATION.db
// - Prod: platform-specific app data directory
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

        if !app_dir.exists() {
            fs::create_dir_all(&app_dir)?;
        }

        Ok(app_dir.join("FOUNDATION.db"))
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

/// Import RDF/RDFS/OWL core ontology
fn import_rdf_core(conn: &mut Connection) -> Result<(), DbError> {
    println!("\n📚 Importing RDF/RDFS/OWL core ontology...");

    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("rdf-rdfs-owl-core.ttl");

    std::fs::write(&temp_file, RDF_CORE_TTL)
        .map_err(|e| DbError::IoError(e))?;

    let stats = crate::turtle::import_turtle_file(
        conn,
        &temp_file,
        "core"
    ).map_err(|e| DbError::SchemaError(format!("RDF core import failed: {:?}", e)))?;

    let _ = std::fs::remove_file(&temp_file);

    println!("✅ Imported {} triples from RDF/RDFS/OWL", stats.triples_processed);
    Ok(())
}

/// Import DTYPE ontology
fn import_dtype(conn: &mut Connection) -> Result<(), DbError> {
    println!("\n📚 Importing DTYPE ontology...");

    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("dtype.ttl");

    std::fs::write(&temp_file, DTYPE_TTL)
        .map_err(|e| DbError::IoError(e))?;

    let stats = crate::turtle::import_turtle_file(
        conn,
        &temp_file,
        "core"
    ).map_err(|e| DbError::SchemaError(format!("DTYPE import failed: {:?}", e)))?;

    let _ = std::fs::remove_file(&temp_file);

    println!("✅ Imported {} triples from DTYPE", stats.triples_processed);
    Ok(())
}

/// Initialize database with schema and ontologies
pub fn initialize_db(db_path: &Path) -> Result<Connection, DbError> {
    let needs_initialization = !db_path.exists();

    println!("Using database at: {:?}", db_path);
    let mut conn = Connection::open(db_path)?;

    if needs_initialization {
        println!("\n🚀 Initializing new database...\n");

        create_schema(&conn)?;
        import_rdf_core(&mut conn)?;
        import_dtype(&mut conn)?;

        crate::turtle::import_all_foundation_ontologies(&mut conn)
            .map_err(|e| DbError::SchemaError(format!("Ontology import failed: {:?}", e)))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

        // Verify schema was created by checking for tables
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

        // Verify we can connect to the created database
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

        // Create a database first
        {
            let conn = Connection::open(&db_path).expect("Failed to create initial db");
            conn.execute_batch(SCHEMA_SQL).expect("Failed to create schema");
        }

        assert!(db_path.exists(), "Database should exist");

        // Initialize should reuse existing database
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
