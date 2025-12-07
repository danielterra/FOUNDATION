/// EVTO Store Functions
///
/// Functions for asserting and retracting triples (append-only, immutable)

use rusqlite::Connection;
use super::triple_type::Triple;
use super::object_type::Object;
use chrono;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Assert triples (add new facts to the store)
///
/// Returns the transaction ID of the assertion
pub fn assert_triples(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let tx = conn.transaction()?;

    // Create transaction record (AUTOINCREMENT generates tx_id)
    let now = now_millis();
    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;

    let tx_id = tx.last_insert_rowid();

    // Get or create origin_id
    let origin_id = get_or_create_origin(&tx, origin)?;

    // Insert each triple
    for triple in triples {
        insert_triple(&tx, triple, tx_id, origin_id, now)?;
    }

    // Before commit, validate numeric literals have typed columns
    {
        let mut stmt = tx.prepare(
            "SELECT subject, predicate, object_datatype, object_number, object_integer
             FROM triples
             WHERE tx = ?
             AND (
               (object_datatype IN ('xsd:decimal', 'xsd:double', 'xsd:float') AND object_number IS NULL) OR
               (object_datatype IN ('xsd:integer', 'xsd:int', 'xsd:long') AND object_integer IS NULL)
             )"
        )?;

        let bad_triples: Vec<(String, String, String, Option<f64>, Option<i64>)> = stmt.query_map([tx_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        if !bad_triples.is_empty() {
            eprintln!("\n⚠️  FOUND {} TRIPLES WITH NUMERIC DATATYPE BUT NO TYPED COLUMN:", bad_triples.len());
            for (idx, (subj, pred, dt, num, int)) in bad_triples.iter().enumerate().take(5) {
                eprintln!("  #{}: {} {} (datatype={}, object_number={:?}, object_integer={:?})",
                         idx + 1, subj, pred, dt, num, int);
            }
            if bad_triples.len() > 5 {
                eprintln!("  ... and {} more", bad_triples.len() - 5);
            }
            eprintln!();
        }
    } // stmt is dropped here

    tx.commit()?;
    Ok(tx_id)
}

/// Retract triples (mark as retracted, don't delete)
///
/// Returns the transaction ID of the retraction
pub fn retract_triples(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let tx = conn.transaction()?;

    // Create transaction record
    let now = now_millis();
    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;

    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(&tx, origin)?;

    // Mark matching triples as retracted
    for triple in triples {
        tx.execute(
            "UPDATE triples
             SET retracted = 1
             WHERE subject = ? AND predicate = ? AND retracted = 0",
            (&triple.subject, &triple.predicate),
        )?;
    }

    tx.commit()?;
    Ok(tx_id)
}

/// Insert a single triple into the database
fn insert_triple(
    tx: &rusqlite::Transaction,
    triple: &Triple,
    tx_id: i64,
    origin_id: i64,
    created_at: i64,
) -> rusqlite::Result<()> {
    // Convert object to SQL columns
    // Need to compute everything together to ensure datatype matches typed columns
    let int_str;
    let num_str;
    let bool_str;
    let dt_str;

    let (object, object_value, object_datatype, object_language, object_number, object_integer, object_datetime, object_boolean) = match &triple.object {
        Object::Iri(iri) => (Some(iri.as_str()), None, None, None, None, None, None, None),
        Object::Blank(blank) => (Some(blank.as_str()), None, None, None, None, None, None, None),

        Object::Integer(i) => {
            int_str = i.to_string();
            (None, Some(int_str.as_str()), Some("xsd:integer"), None, None, Some(*i), None, None)
        }
        Object::Number(n) => {
            num_str = n.to_string();
            (None, Some(num_str.as_str()), Some("xsd:decimal"), None, Some(*n), None, None, None)
        }
        Object::Boolean(b) => {
            bool_str = b.to_string();
            (None, Some(bool_str.as_str()), Some("xsd:boolean"), None, None, None, None, Some(if *b { 1 } else { 0 }))
        }
        Object::DateTime(dt) => {
            dt_str = dt.to_string();
            (None, Some(dt_str.as_str()), Some("xsd:dateTime"), None, None, None, Some(*dt), None)
        }

        Object::Literal { value, datatype, language } => {
            // Parse typed literals and populate typed columns
            // If parse fails, it's a BUG - we should NOT have invalid data
            match datatype.as_deref() {
                Some("xsd:decimal") | Some("xsd:double") | Some("xsd:float") => {
                    let n = value.parse::<f64>()
                        .unwrap_or_else(|e| panic!(
                            "PARSE ERROR: Failed to parse float literal\n\
                             Value: '{}'\n\
                             Datatype: {:?}\n\
                             Triple: {} {} {}\n\
                             Error: {:?}",
                            value, datatype, triple.subject, triple.predicate, value, e
                        ));
                    (None, Some(value.as_str()), datatype.as_deref(), language.as_deref(), Some(n), None, None, None)
                }
                Some("xsd:integer") | Some("xsd:int") | Some("xsd:long") => {
                    let i = value.parse::<i64>()
                        .unwrap_or_else(|e| panic!(
                            "PARSE ERROR: Failed to parse integer literal\n\
                             Value: '{}'\n\
                             Datatype: {:?}\n\
                             Triple: {} {} {}\n\
                             Error: {:?}",
                            value, datatype, triple.subject, triple.predicate, value, e
                        ));
                    (None, Some(value.as_str()), datatype.as_deref(), language.as_deref(), None, Some(i), None, None)
                }
                Some("xsd:boolean") => {
                    let b = match value.as_str() {
                        "true" | "1" => 1,
                        "false" | "0" => 0,
                        _ => panic!(
                            "PARSE ERROR: Invalid boolean literal\n\
                             Value: '{}'\n\
                             Datatype: {:?}\n\
                             Triple: {} {} {}\n\
                             Expected: 'true', 'false', '1', or '0'",
                            value, datatype, triple.subject, triple.predicate, value
                        ),
                    };
                    (None, Some(value.as_str()), datatype.as_deref(), language.as_deref(), None, None, None, Some(b))
                }
                Some("xsd:dateTime") => {
                    let timestamp = chrono::DateTime::parse_from_rfc3339(value)
                        .map(|dt| dt.timestamp())
                        .unwrap_or_else(|e| panic!(
                            "PARSE ERROR: Failed to parse dateTime literal\n\
                             Value: '{}'\n\
                             Datatype: {:?}\n\
                             Triple: {} {} {}\n\
                             Error: {:?}\n\
                             Expected ISO 8601 format (e.g., '2025-01-28T18:38:46Z')",
                            value, datatype, triple.subject, triple.predicate, value, e
                        ));
                    (None, Some(value.as_str()), datatype.as_deref(), language.as_deref(), None, None, Some(timestamp), None)
                }
                _ => {
                    // Other datatype - no typed column needed
                    (None, Some(value.as_str()), datatype.as_deref(), language.as_deref(), None, None, None, None)
                }
            }
        }
    };

    let object_type = triple.object.object_type();

    let result = tx.execute(
        "INSERT INTO triples (
            subject, predicate, object, object_value, object_datatype, object_language,
            object_type, object_number, object_integer, object_datetime, object_boolean,
            tx, origin_id, retracted, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
        rusqlite::params![
            &triple.subject,
            &triple.predicate,
            object,
            object_value,
            object_datatype,
            object_language,
            object_type,
            object_number,
            object_integer,
            object_datetime,
            object_boolean,
            tx_id,
            origin_id,
            created_at,
        ],
    );

    if let Err(e) = result {
        eprintln!("\n❌ INSERT FAILED:");
        eprintln!("   Subject: {}", triple.subject);
        eprintln!("   Predicate: {}", triple.predicate);
        eprintln!("   Object: {:?}", triple.object);
        eprintln!("   object_datatype: {:?}", object_datatype);
        eprintln!("   object_number: {:?}", object_number);
        eprintln!("   object_integer: {:?}", object_integer);
        eprintln!("   object_boolean: {:?}", object_boolean);
        eprintln!("   Error: {}\n", e);
        return Err(e);
    }

    Ok(())
}

/// Get or create origin ID
fn get_or_create_origin(tx: &rusqlite::Transaction, origin: &str) -> rusqlite::Result<i64> {
    // Try to get existing origin
    match tx.query_row(
        "SELECT id FROM origins WHERE name = ?",
        [origin],
        |row| row.get(0),
    ) {
        Ok(id) => Ok(id),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            // Create new origin
            tx.execute("INSERT INTO origins (name) VALUES (?)", [origin])?;
            Ok(tx.last_insert_rowid())
        }
        Err(e) => Err(e),
    }
}

/// Get current Unix time in milliseconds
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::{setup_test_db, create_test_triples, assert_triple_exists, get_active_triple_count};

    #[test]
    fn test_assert_triples_basic() {
        let mut conn = setup_test_db();
        let triples = create_test_triples();

        let tx_id = assert_triples(&mut conn, &triples, "test_origin")
            .expect("Failed to assert triples");

        assert!(tx_id > 0);
        assert_eq!(get_active_triple_count(&conn), 3);
    }

    #[test]
    fn test_assert_triples_creates_transaction() {
        let mut conn = setup_test_db();
        let triples = create_test_triples();

        let tx_id = assert_triples(&mut conn, &triples, "test_origin").unwrap();

        let tx_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions WHERE tx = ?", [tx_id], |row| row.get(0))
            .unwrap();

        assert_eq!(tx_count, 1);
    }

    #[test]
    fn test_assert_triples_creates_origin() {
        let mut conn = setup_test_db();
        let triples = create_test_triples();

        assert_triples(&mut conn, &triples, "new_origin").unwrap();

        let origin_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM origins WHERE name = 'new_origin'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(origin_exists);
    }

    #[test]
    fn test_assert_triples_with_different_object_types() {
        let mut conn = setup_test_db();

        let triples = vec![
            Triple {
                subject: "test:Subject1".to_string(),
                predicate: "test:hasIri".to_string(),
                object: Object::Iri("test:Object1".to_string()),
                tx: 0,
                created_at: 1000,
                origin_id: 1,
                retracted: false,
            },
            Triple {
                subject: "test:Subject2".to_string(),
                predicate: "test:hasInteger".to_string(),
                object: Object::Integer(42),
                tx: 0,
                created_at: 1000,
                origin_id: 1,
                retracted: false,
            },
            Triple {
                subject: "test:Subject3".to_string(),
                predicate: "test:hasNumber".to_string(),
                object: Object::Number(3.14),
                tx: 0,
                created_at: 1000,
                origin_id: 1,
                retracted: false,
            },
            Triple {
                subject: "test:Subject4".to_string(),
                predicate: "test:hasBoolean".to_string(),
                object: Object::Boolean(true),
                tx: 0,
                created_at: 1000,
                origin_id: 1,
                retracted: false,
            },
        ];

        assert_triples(&mut conn, &triples, "test").unwrap();

        assert_triple_exists(&conn, "test:Subject1", "test:hasIri");
        assert_triple_exists(&conn, "test:Subject2", "test:hasInteger");
        assert_triple_exists(&conn, "test:Subject3", "test:hasNumber");
        assert_triple_exists(&conn, "test:Subject4", "test:hasBoolean");
    }

    #[test]
    fn test_retract_triples() {
        let mut conn = setup_test_db();
        let triples = create_test_triples();

        // Assert triples first
        assert_triples(&mut conn, &triples, "test").unwrap();
        assert_eq!(get_active_triple_count(&conn), 3);

        // Retract one triple
        let to_retract = vec![triples[0].clone()];
        let retract_tx_id = retract_triples(&mut conn, &to_retract, "test").unwrap();

        assert!(retract_tx_id > 0);
        assert_eq!(get_active_triple_count(&conn), 2); // One should be retracted
    }

    #[test]
    fn test_retract_triples_multiple() {
        let mut conn = setup_test_db();
        let triples = create_test_triples();

        assert_triples(&mut conn, &triples, "test").unwrap();
        assert_eq!(get_active_triple_count(&conn), 3);

        // Retract all triples
        retract_triples(&mut conn, &triples, "test").unwrap();
        assert_eq!(get_active_triple_count(&conn), 0);
    }

    #[test]
    fn test_retract_nonexistent_triple_does_not_error() {
        let mut conn = setup_test_db();

        let triples = vec![Triple {
            subject: "nonexistent:Subject".to_string(),
            predicate: "nonexistent:predicate".to_string(),
            object: Object::Iri("nonexistent:Object".to_string()),
            tx: 0,
            created_at: 1000,
            origin_id: 1,
            retracted: false,
        }];

        // Should not error even though triple doesn't exist
        let result = retract_triples(&mut conn, &triples, "test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_or_create_origin_existing() {
        let mut conn = setup_test_db();
        let tx = conn.transaction().unwrap();

        // Origin "test" should already exist from setup_test_db
        let id1 = get_or_create_origin(&tx, "test").unwrap();
        let id2 = get_or_create_origin(&tx, "test").unwrap();

        assert_eq!(id1, id2); // Should return same ID
    }

    #[test]
    fn test_get_or_create_origin_new() {
        let mut conn = setup_test_db();
        let tx = conn.transaction().unwrap();

        let id = get_or_create_origin(&tx, "brand_new_origin").unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_now_millis() {
        let ts = now_millis();
        assert!(ts > 0);

        // Should be a reasonable timestamp (after 2020)
        assert!(ts > 1577836800000); // Jan 1, 2020 in milliseconds
    }
}
