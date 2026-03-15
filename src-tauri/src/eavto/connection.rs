use turso::{Builder, Connection, Database};
use std::path::{Path, PathBuf};
use std::fs;
use std::fmt;
use std::error::Error;
use crate::commands::log_backend;

#[derive(Debug)]
pub enum DbError {
    ConnectionError(turso::Error),
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

impl From<turso::Error> for DbError {
    fn from(err: turso::Error) -> Self {
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

/// Split SQL text into individual statements, respecting single-quoted string literals.
/// Semicolons inside string literals are not treated as statement separators.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = sql.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            current.push(c);
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    current.push(chars.next().unwrap());
                } else {
                    in_string = false;
                }
            }
        } else if c == '\'' {
            in_string = true;
            current.push(c);
        } else if c == ';' {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                statements.push(trimmed);
            }
            current = String::new();
        } else {
            current.push(c);
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        statements.push(trimmed);
    }
    statements
}

/// Execute multiple SQL statements, respecting string literals and skipping PRAGMAs.
async fn execute_batch(conn: &Connection, sql: &str) -> Result<(), DbError> {
    for stmt in split_sql_statements(sql) {
        let first_meaningful = stmt.lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with("--"));
        match first_meaningful {
            None => continue,
            Some(line) if line.to_ascii_uppercase().starts_with("PRAGMA") => continue,
            Some(_) => {}
        }
        conn.execute(&stmt, ()).await
            .map_err(|e| DbError::SchemaError(format!("{}: {}", e, stmt.chars().take(80).collect::<String>())))?;
    }
    Ok(())
}

async fn create_schema(conn: &Connection) -> Result<(), DbError> {
    log_backend("info", "Creating schema");
    execute_batch(conn, SCHEMA_SQL).await?;
    log_backend("info", "Schema created");
    Ok(())
}

async fn import_ontology_sql(conn: &Connection) -> Result<(), DbError> {
    log_backend("info", "Importing core ontology from SQL");
    execute_batch(conn, ONTOLOGY_SQL).await
        .map_err(|e| DbError::SchemaError(format!("Ontology import failed: {}", e)))?;
    log_backend("info", "Core ontology imported");
    Ok(())
}

#[allow(dead_code)]
pub async fn initialize_db(db_path: &Path) -> Result<Database, DbError> {
    initialize_db_with_progress(db_path, None).await
}

async fn initialize_db_with_progress(
    db_path: &Path,
    app: Option<&tauri::AppHandle>,
) -> Result<Database, DbError> {
    use tauri::Emitter;

    let needs_initialization = !db_path.exists();

    log_backend("info", &format!("Using database: {:?}", db_path));

    let path_str = db_path.to_str()
        .ok_or_else(|| DbError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Database path is not valid UTF-8"
        )))?;

    let db = Builder::new_local(path_str)
        .build()
        .await
        .map_err(DbError::ConnectionError)?;

    let conn = db.connect().map_err(DbError::ConnectionError)?;

    // PRAGMA journal_mode returns a row — use query() to consume it safely.
    conn.query("PRAGMA journal_mode=WAL", ()).await.map_err(DbError::ConnectionError)?;
    conn.execute("PRAGMA synchronous=NORMAL", ()).await?;

    if needs_initialization {
        log_backend("info", "Initializing new database");

        create_schema(&conn).await?;
        import_ontology_sql(&conn).await?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is before Unix epoch")
            .as_millis() as i64;

        conn.execute(
            "UPDATE metadata SET value = 'true', updated_at = ? WHERE key = 'ontology_imported'",
            turso::params![now],
        ).await?;

        log_backend("info", "Database initialization complete");
    } else {
        log_backend("info", "Database exists, skipping ontology import");
    }

    run_migrations(&conn).await?;

    if let Some(handle) = app {
        let _ = handle.emit("import-complete", ());
    }

    Ok(db)
}

async fn drop_object_datetime_if_exists(conn: &Connection) -> Result<(), DbError> {
    let mut stmt = conn.prepare(
        "SELECT COUNT(*) FROM pragma_table_info('triples') WHERE name = 'object_datetime'"
    ).await?;

    let col_exists = match stmt.query_row(()).await {
        Ok(row) => row.get_value(0)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0) > 0,
        Err(_) => false,
    };

    if !col_exists {
        return Ok(());
    }

    log_backend("info", "Migrating: dropping object_datetime column from triples table");

    conn.execute("PRAGMA foreign_keys = OFF", ()).await?;

    let migration_sql = "
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
    ";

    execute_batch(conn, migration_sql).await?;
    conn.execute("PRAGMA foreign_keys = ON", ()).await?;

    log_backend("info", "Migration complete: object_datetime column removed");
    Ok(())
}

async fn run_migrations(conn: &Connection) -> Result<(), DbError> {
    drop_object_datetime_if_exists(conn).await?;
    execute_batch(conn, "
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
    ").await?;
    Ok(())
}

pub async fn initialize_with_progress(app: tauri::AppHandle) -> Result<(Database, PathBuf), DbError> {
    let db_path = get_db_path()?;
    let db = initialize_db_with_progress(&db_path, Some(&app)).await?;
    Ok((db, db_path))
}

#[allow(dead_code)]
pub async fn get_connection() -> Result<Database, DbError> {
    let db_path = get_db_path()?;
    initialize_db(&db_path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_db_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test error");
        let db_err: DbError = io_err.into();

        match db_err {
            DbError::IoError(_) => {},
            _ => panic!("Expected IoError"),
        }
    }

    #[tokio::test]
    async fn test_create_schema() {
        let db = Builder::new_local(":memory:")
            .build()
            .await
            .expect("Failed to create in-memory db");
        let conn = db.connect().expect("Failed to connect");
        let result = create_schema(&conn).await;

        assert!(result.is_ok());

        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'"
        ).await.expect("Failed to prepare");

        let row = stmt.query_row(()).await.expect("Failed to query");
        let count = row.get_value(0)
            .expect("Failed to get value")
            .as_integer()
            .copied()
            .unwrap_or(0);

        assert!(count > 0, "Schema should create tables");
    }

    #[tokio::test]
    async fn test_initialize_db_creates_new_database() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");

        assert!(!db_path.exists(), "Database should not exist initially");

        let result = initialize_db(&db_path).await;

        assert!(result.is_ok(), "Database initialization should succeed: {:?}", result.err());
        assert!(db_path.exists(), "Database file should be created");

        let db = result.unwrap();
        let conn = db.connect().expect("Should connect to created database");

        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'"
        ).await.expect("Failed to prepare");

        let row = stmt.query_row(()).await.expect("Failed to query");
        let count = row.get_value(0)
            .expect("Failed to get value")
            .as_integer()
            .copied()
            .unwrap_or(0);

        assert!(count > 0, "Initialized database should have tables");
    }

    #[tokio::test]
    async fn test_initialize_db_reuses_existing_database() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("existing.db");

        {
            let db = Builder::new_local(db_path.to_str().unwrap())
                .build()
                .await
                .expect("Failed to create initial db");
            let conn = db.connect().expect("Failed to connect");
            execute_batch(&conn, SCHEMA_SQL).await.expect("Failed to create schema");
        }

        assert!(db_path.exists(), "Database should exist");

        let result = initialize_db(&db_path).await;
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
