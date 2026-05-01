use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::fs;
use std::fmt;
use std::error::Error;
use crate::commands::log_backend;

const DB_BUSY_TIMEOUT_SECS: u64 = 30;

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
    let foundation_dir = dirs::document_dir()
        .ok_or_else(|| DbError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine Documents directory"
        )))?
        .join("Foundation");
    get_db_path_for_dir(&foundation_dir)
}

pub fn get_db_path_for_dir(foundation_dir: &std::path::Path) -> Result<PathBuf, DbError> {
    if !foundation_dir.exists() {
        log_backend("info", &format!("Creating Foundation directory: {:?}", foundation_dir));
        fs::create_dir_all(foundation_dir)?;
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
    // Foreign keys must be off during bulk import — the ontology SQL does not
    // guarantee insertion order across tables (e.g. triples reference origin rows
    // that may not yet exist at insert time).
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    conn.execute_batch(ONTOLOGY_SQL)
        .map_err(|e| DbError::SchemaError(format!("Ontology import failed: {}", e)))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
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

    log_backend("info", &format!("Using database: {:?}", db_path));
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    conn.busy_timeout(std::time::Duration::from_secs(DB_BUSY_TIMEOUT_SECS)).map_err(|e| DbError::ConnectionError(e))?;

    // Check if the ontology is actually present by looking for a core class.
    // This is more reliable than checking file existence or a metadata flag,
    // since the file can be created empty (crash before import) or the flag
    // may not exist in older databases.
    let ontology_present = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='foundation:Person' AND predicate='rdf:type' AND object='owl:Class' LIMIT 1",
        [],
        |row| row.get::<_, i64>(0),
    ).map(|c| c > 0).unwrap_or(false);

    if !ontology_present {
        log_backend("info", "Ontology not present — initializing database");

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
        log_backend("info", "Ontology already present, skipping import");
    }

    let t_startup = std::time::Instant::now();

    run_migrations(&conn)?;
    log_backend("info", &format!("[STARTUP] migrations={}ms", t_startup.elapsed().as_millis()));

    let search_dir = db_path.parent()
        .map(|p| p.join("search"))
        .unwrap_or_else(|| std::path::PathBuf::from("search"));
    let search_db_path = db_path.to_path_buf();
    std::thread::spawn(move || {
        let t = std::time::Instant::now();
        log_backend("info", "[SEARCH] Background init starting");
        match Connection::open(&search_db_path) {
            Ok(bg_conn) => {
                crate::search::init(&search_dir, &bg_conn);
                log_backend("info", &format!("[SEARCH] Background init complete in {}ms", t.elapsed().as_millis()));
            }
            Err(e) => {
                log_backend("error", &format!("[SEARCH] Background init failed: {}", e));
            }
        }
    });
    log_backend("info", &format!("[STARTUP] search_init=spawned ({}ms)", t_startup.elapsed().as_millis()));

    if let Some(handle) = app {
        let _ = handle.emit("import-complete", ());
    }

    Ok(conn)
}

fn drop_object_datetime_if_exists(conn: &Connection) -> Result<(), DbError> {
    let col_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('triples') WHERE name = 'object_datetime'",
        [],
        |row| row.get::<_, i64>(0),
    ).map(|c| c > 0).unwrap_or(false);

    if !col_exists {
        return Ok(());
    }

    log_backend("info", "Migrating: dropping object_datetime column from triples table");

    conn.execute_batch("PRAGMA foreign_keys = OFF")?;
    conn.execute_batch("
        BEGIN;

        DROP VIEW IF EXISTS triples_current;
        DROP VIEW IF EXISTS entities;
        DROP VIEW IF EXISTS ontology_classes;
        DROP VIEW IF EXISTS ontology_properties;

        CREATE TABLE triples_new (
            subject TEXT NOT NULL,
            predicate TEXT NOT NULL,
            object TEXT,
            object_value TEXT,
            object_datatype TEXT,
            object_language TEXT,
            object_type TEXT NOT NULL CHECK(object_type IN ('iri', 'literal', 'blank')),
            object_number REAL,
            object_integer INTEGER,
            object_boolean INTEGER,
            tx INTEGER NOT NULL,
            origin_id INTEGER NOT NULL,
            retracted INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (origin_id) REFERENCES origins(id),
            CHECK (
                (object_type = 'iri' AND object IS NOT NULL AND object_value IS NULL) OR
                (object_type = 'literal' AND object_value IS NOT NULL AND object_datatype IS NOT NULL AND object IS NULL) OR
                (object_type = 'blank' AND object IS NOT NULL AND object_value IS NULL)
            ),
            CHECK (
                (object_datatype IN ('xsd:decimal', 'xsd:double', 'xsd:float') AND object_number IS NOT NULL) OR
                (object_datatype IN ('xsd:integer', 'xsd:int', 'xsd:long') AND object_integer IS NOT NULL) OR
                (object_datatype = 'xsd:boolean' AND object_boolean IS NOT NULL) OR
                (object_datatype NOT IN ('xsd:decimal', 'xsd:double', 'xsd:float', 'xsd:integer', 'xsd:int', 'xsd:long', 'xsd:boolean'))
            )
        );

        INSERT INTO triples_new
        SELECT subject, predicate, object, object_value, object_datatype, object_language,
               object_type, object_number, object_integer, object_boolean,
               tx, origin_id, retracted, created_at
        FROM triples;

        DROP TABLE triples;
        ALTER TABLE triples_new RENAME TO triples;

        DROP INDEX IF EXISTS idx_predicate_datetime;

        CREATE INDEX IF NOT EXISTS idx_spo ON triples(subject, predicate, object, object_value, tx, origin_id);
        CREATE INDEX IF NOT EXISTS idx_pos ON triples(predicate, object, object_value, subject, tx, origin_id);
        CREATE INDEX IF NOT EXISTS idx_osp ON triples(object, subject, predicate, tx, origin_id) WHERE object_type = 'iri';
        CREATE INDEX IF NOT EXISTS idx_ops ON triples(object, predicate, subject, tx, origin_id) WHERE object_type = 'iri';
        CREATE INDEX IF NOT EXISTS idx_predicate_number ON triples(predicate, object_number, tx)
            WHERE object_type = 'literal' AND object_datatype IN ('xsd:decimal', 'xsd:double', 'xsd:float') AND retracted = 0;
        CREATE INDEX IF NOT EXISTS idx_predicate_integer ON triples(predicate, object_integer, tx)
            WHERE object_type = 'literal' AND object_datatype IN ('xsd:integer', 'xsd:int', 'xsd:long') AND retracted = 0;
        CREATE INDEX IF NOT EXISTS idx_subject_retracted ON triples(subject, retracted, tx);
        CREATE INDEX IF NOT EXISTS idx_tx ON triples(tx);

        CREATE VIEW triples_current AS
        SELECT DISTINCT
            subject, predicate, object, object_value, object_datatype, object_language,
            object_number, object_integer, object_boolean,
            FIRST_VALUE(tx) OVER (PARTITION BY subject, predicate, origin_id ORDER BY tx DESC) as tx,
            origin_id, object_type, created_at
        FROM triples
        WHERE retracted = 0;

        CREATE VIEW entities AS
        SELECT DISTINCT subject
        FROM triples
        WHERE retracted = 0;

        CREATE VIEW ontology_classes AS
        SELECT DISTINCT subject as class_id,
            (SELECT object_value FROM triples WHERE subject = class_id AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1) as label,
            (SELECT object_value FROM triples WHERE subject = class_id AND predicate = 'rdfs:comment' AND retracted = 0 LIMIT 1) as comment,
            (SELECT object FROM triples WHERE subject = class_id AND predicate = 'rdfs:subClassOf' AND retracted = 0 LIMIT 1) as parent_class
        FROM triples
        WHERE predicate = 'rdf:type'
            AND object IN ('owl:Class', 'rdfs:Class')
            AND retracted = 0;

        CREATE VIEW ontology_properties AS
        SELECT DISTINCT subject as property_id,
            (SELECT object FROM triples WHERE subject = property_id AND predicate = 'rdf:type' AND retracted = 0 LIMIT 1) as property_type,
            (SELECT object_value FROM triples WHERE subject = property_id AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1) as label,
            (SELECT object FROM triples WHERE subject = property_id AND predicate = 'rdfs:domain' AND retracted = 0 LIMIT 1) as domain,
            (SELECT object FROM triples WHERE subject = property_id AND predicate = 'rdfs:range' AND retracted = 0 LIMIT 1) as range
        FROM triples
        WHERE predicate = 'rdf:type'
            AND object IN ('owl:ObjectProperty', 'owl:DatatypeProperty', 'owl:AnnotationProperty', 'rdf:Property')
            AND retracted = 0;

        COMMIT;
    ")?;
    conn.execute_batch("PRAGMA foreign_keys = ON")?;

    log_backend("info", "Migration complete: object_datetime column removed");
    Ok(())
}

fn run_migrations(conn: &Connection) -> Result<(), DbError> {
    drop_object_datetime_if_exists(conn)?;
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

    let has_instance_iri: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('formula_recalc_jobs') WHERE name = 'instance_iri'",
        [],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;
    if !has_instance_iri {
        conn.execute_batch("ALTER TABLE formula_recalc_jobs ADD COLUMN instance_iri TEXT")?;
    }

    conn.execute_batch("
        DROP VIEW IF EXISTS triples_current;
        DROP VIEW IF EXISTS entities;
        DROP VIEW IF EXISTS ontology_classes;
        DROP VIEW IF EXISTS ontology_properties;

        CREATE VIEW IF NOT EXISTS triples_current AS
        SELECT subject, predicate, object, object_value, object_datatype, object_language,
               object_number, object_integer, object_boolean, tx, origin_id, object_type, created_at
        FROM triples t
        WHERE t.retracted = 0
          AND t.tx = (
              SELECT MAX(tx) FROM triples
              WHERE subject = t.subject AND predicate = t.predicate
          );

        CREATE VIEW IF NOT EXISTS entities AS
        SELECT DISTINCT subject FROM triples_current;

        CREATE VIEW IF NOT EXISTS ontology_classes AS
        SELECT DISTINCT subject as class_id,
          (SELECT object_value FROM triples_current
           WHERE subject = class_id AND predicate = 'rdfs:label' LIMIT 1) as label,
          (SELECT object_value FROM triples_current
           WHERE subject = class_id AND predicate = 'rdfs:comment' LIMIT 1) as comment,
          (SELECT object FROM triples_current
           WHERE subject = class_id AND predicate = 'rdfs:subClassOf' LIMIT 1) as parent_class
        FROM triples_current
        WHERE predicate = 'rdf:type'
          AND object IN ('owl:Class', 'rdfs:Class');

        CREATE VIEW IF NOT EXISTS ontology_properties AS
        SELECT DISTINCT subject as property_id,
          (SELECT object FROM triples_current
           WHERE subject = property_id AND predicate = 'rdf:type' LIMIT 1) as property_type,
          (SELECT object_value FROM triples_current
           WHERE subject = property_id AND predicate = 'rdfs:label' LIMIT 1) as label,
          (SELECT object FROM triples_current
           WHERE subject = property_id AND predicate = 'rdfs:domain' LIMIT 1) as domain,
          (SELECT object FROM triples_current
           WHERE subject = property_id AND predicate = 'rdfs:range' LIMIT 1) as range
        FROM triples_current
        WHERE predicate = 'rdf:type'
          AND object IN ('owl:ObjectProperty', 'owl:DatatypeProperty',
                         'owl:AnnotationProperty', 'rdf:Property');
    ")?;

    conn.execute_batch("
        CREATE INDEX IF NOT EXISTS idx_backlinks_active ON triples(object, subject, predicate, tx)
        WHERE retracted = 0 AND object_type = 'iri' AND predicate != 'rdf:type';
    ")?;

    crate::search::ensure_access_table(conn).map_err(DbError::ConnectionError)?;

    Ok(())
}

pub fn initialize_with_progress(app: tauri::AppHandle) -> Result<(Connection, PathBuf), DbError> {
    let foundation_dir = crate::config::get_foundation_dir(&app);
    let db_path = get_db_path_for_dir(&foundation_dir)?;
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

        assert!(result.is_ok(), "Database initialization should succeed: {:?}", result.err());
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
