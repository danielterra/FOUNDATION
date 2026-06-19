#[cfg(any(test, feature = "test-helpers"))]
use rusqlite::Connection;

#[cfg(any(test, feature = "test-helpers"))]
use super::{Triple, Object};

#[cfg(any(test, feature = "test-helpers"))]
pub fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().expect("Failed to open in-memory database");

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS origins (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE NOT NULL,
            description TEXT
        );

        CREATE TABLE IF NOT EXISTS transactions (
            tx INTEGER PRIMARY KEY AUTOINCREMENT,
            origin TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS triples (
            subject TEXT NOT NULL,
            predicate TEXT NOT NULL,
            object TEXT,
            object_value TEXT,
            object_type TEXT NOT NULL CHECK(object_type IN ('iri', 'literal', 'blank')),
            object_datatype TEXT,
            object_language TEXT,
            object_number REAL,
            object_integer INTEGER,
            object_boolean INTEGER,
            tx INTEGER NOT NULL,
            origin_id INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            retracted INTEGER NOT NULL DEFAULT 0,
            is_current INTEGER NOT NULL DEFAULT 1,
            FOREIGN KEY (tx) REFERENCES transactions(tx),
            FOREIGN KEY (origin_id) REFERENCES origins(id)
        );

        CREATE INDEX IF NOT EXISTS idx_triples_subject ON triples(subject);
        CREATE INDEX IF NOT EXISTS idx_triples_predicate ON triples(predicate);
        CREATE INDEX IF NOT EXISTS idx_triples_object ON triples(object);
        CREATE INDEX IF NOT EXISTS idx_triples_tx ON triples(tx);
        CREATE INDEX IF NOT EXISTS idx_triples_retracted ON triples(retracted);
        CREATE INDEX IF NOT EXISTS idx_spr ON triples(subject, predicate, retracted, tx);

        CREATE VIEW IF NOT EXISTS triples_current AS
        SELECT subject, predicate, object, object_value, object_datatype, object_language,
               object_number, object_integer, object_boolean, tx, origin_id, object_type, created_at
        FROM triples
        WHERE is_current = 1 AND retracted = 0;

        CREATE TABLE IF NOT EXISTS metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );

        INSERT INTO metadata (key, value, updated_at) VALUES
            ('schema_version', '2', 0),
            ('ontology_imported', 'false', 0);

        INSERT INTO origins (name, description) VALUES
            ('test', 'Test origin for unit tests');
        "#
    ).expect("Failed to create test schema");

    conn
}

#[cfg(any(test, feature = "test-helpers"))]
pub fn create_test_triples() -> Vec<Triple> {
    vec![
        Triple {
            subject: "foundation:TestClass".to_string(),
            predicate: "rdf:type".to_string(),
            object: Object::Iri("owl:Class".to_string()),
            tx: 0,
            created_at: 1000,
            origin_id: 1,
            retracted: false,
        },
        Triple {
            subject: "foundation:TestClass".to_string(),
            predicate: "rdfs:label".to_string(),
            object: Object::Literal {
                value: "Test Class".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            tx: 0,
            created_at: 1000,
            origin_id: 1,
            retracted: false,
        },
        Triple {
            subject: "foundation:TestProperty".to_string(),
            predicate: "foundation:someValue".to_string(),
            object: Object::Integer(42),
            tx: 0,
            created_at: 1000,
            origin_id: 1,
            retracted: false,
        },
    ]
}

#[cfg(any(test, feature = "test-helpers"))]
pub fn assert_triple_exists(conn: &Connection, subject: &str, predicate: &str) {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM triples_current WHERE subject = ? AND predicate = ?",
            [subject, predicate],
            |row| row.get(0),
        )
        .expect("Failed to query triple");

    assert!(count > 0, "Triple not found: {} {}", subject, predicate);
}

#[cfg(any(test, feature = "test-helpers"))]
pub fn get_active_triple_count(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM triples_current",
        [],
        |row| row.get(0),
    )
    .expect("Failed to count triples")
}
