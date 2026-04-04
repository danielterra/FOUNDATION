/// EVTO Store Functions
///
/// Functions for asserting and retracting triples (append-only, immutable)

use rusqlite::Connection;
use super::triple_type::Triple;
use super::object_type::Object;
use crate::commands::log_backend;
use chrono;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

std::thread_local! {
    /// Set to true while batch_operations holds an outer transaction open.
    /// When true, assert_triples/retract_triples use SAVEPOINTs instead of BEGIN
    /// so that all operations participate in the same atomic transaction.
    static IN_BATCH_TX: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// Accumulates subjects written during assert_triples on the write thread.
    /// Drained by DbExecutor after each write to emit entity-updated notifications.
    static WRITTEN_SUBJECTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };

    /// Accumulates IRI objects written during assert_triples on the write thread.
    /// Drained by DbExecutor after each write to emit entity-referenced notifications.
    static WRITTEN_IRI_OBJECTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Returns all subjects accumulated since the last drain, removing them from the buffer.
/// Only meaningful when called from the write thread.
pub fn drain_written_subjects() -> Vec<String> {
    WRITTEN_SUBJECTS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

/// Returns all IRI objects accumulated since the last drain, removing them from the buffer.
/// Only meaningful when called from the write thread.
pub fn drain_written_iri_objects() -> Vec<String> {
    WRITTEN_IRI_OBJECTS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

/// Marks the current thread as being inside a batch transaction.
/// Returns a guard that clears the flag on drop.
pub fn enter_batch_transaction() -> BatchTransactionGuard {
    IN_BATCH_TX.with(|f| f.set(true));
    BatchTransactionGuard(())
}

pub struct BatchTransactionGuard(());

impl Drop for BatchTransactionGuard {
    fn drop(&mut self) {
        IN_BATCH_TX.with(|f| f.set(false));
    }
}

/// Assert triples (add new facts to the store)
///
/// Returns the transaction ID of the assertion.
/// If called from within batch_operations (enter_batch_transaction was called),
/// uses a nested SAVEPOINT so all calls participate in a single atomic transaction.
pub fn assert_triples(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let tx_id = if IN_BATCH_TX.with(|f| f.get()) {
        assert_triples_savepoint(conn, triples, origin)?
    } else {
        assert_triples_begin(conn, triples, origin)?
    };
    if tx_id != 0 {
        let subjects: Vec<String> = triples.iter().map(|t| t.subject.clone()).collect();
        #[cfg(not(test))]
        crate::search::reindex_subjects(conn, &subjects);

        WRITTEN_SUBJECTS.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if !is_vocabulary_iri(&triple.subject) {
                    buf.push(triple.subject.clone());
                }
            }
        });
        WRITTEN_IRI_OBJECTS.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if let Object::Iri(iri) = &triple.object {
                    if !is_vocabulary_iri(iri) {
                        buf.push(iri.clone());
                    }
                }
            }
        });
    }
    Ok(tx_id)
}

fn is_vocabulary_iri(iri: &str) -> bool {
    iri.starts_with("rdf:") || iri.starts_with("rdfs:") || iri.starts_with("owl:") ||
    iri.starts_with("xsd:") || iri.starts_with("unit:") || iri.starts_with("currency:")
}

fn assert_triples_begin(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let tx_id = do_assert_triples(&tx, triples, origin)?;
    tx.commit()?;
    Ok(tx_id)
}

fn assert_triples_savepoint(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let sp = conn.savepoint()?;
    let tx_id = do_assert_triples(&sp, triples, origin)?;
    sp.commit()?;
    Ok(tx_id)
}

fn do_assert_triples(
    tx: &rusqlite::Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let now = now_millis();

    // Group incoming triples by (subject, predicate), preserving order.
    let mut groups: Vec<((&str, &str), Vec<usize>)> = Vec::new();
    let mut group_index: std::collections::HashMap<(&str, &str), usize> =
        std::collections::HashMap::new();
    for (i, triple) in triples.iter().enumerate() {
        let key = (triple.subject.as_str(), triple.predicate.as_str());
        if let Some(&idx) = group_index.get(&key) {
            groups[idx].1.push(i);
        } else {
            group_index.insert(key, groups.len());
            groups.push((key, vec![i]));
        }
    }

    // Compare incoming with existing to find what actually needs to change.
    let mut rows_to_retract: Vec<i64> = Vec::new();
    let mut indices_to_insert: Vec<usize> = Vec::new();

    for ((subject, predicate), incoming_indices) in &groups {
        let existing = fetch_existing_rows(tx, subject, predicate)?;
        let incoming: Vec<&Object> = incoming_indices.iter().map(|&i| &triples[i].object).collect();

        for row in &existing {
            if !incoming.iter().any(|obj| object_matches_row(obj, row)) {
                rows_to_retract.push(row.rowid);
            }
        }
        for &idx in incoming_indices {
            if !existing.iter().any(|row| object_matches_row(&triples[idx].object, row)) {
                indices_to_insert.push(idx);
            }
        }
    }

    // Nothing to do — return 0 to signal no-op without writing any rows.
    if rows_to_retract.is_empty() && indices_to_insert.is_empty() {
        return Ok(0);
    }

    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(tx, origin)?;

    for id in &rows_to_retract {
        tx.execute(
            "INSERT INTO triples (
                 subject, predicate, object, object_value, object_datatype,
                 object_language, object_type, object_number, object_integer,
                 object_boolean, tx, origin_id, retracted, created_at
             )
             SELECT subject, predicate, object, object_value, object_datatype,
                    object_language, object_type, object_number, object_integer,
                    object_boolean, ?1, ?2, 1, ?3
             FROM triples WHERE rowid = ?4",
            rusqlite::params![tx_id, origin_id, now, id],
        )?;
    }
    for &idx in &indices_to_insert {
        insert_triple(tx, &triples[idx], tx_id, origin_id, now)?;
    }

    {
        let mut stmt = tx.prepare(
            "SELECT subject, predicate, object_datatype, object_number, object_integer
             FROM triples
             WHERE tx = ?
             AND (
               (object_datatype IN ('xsd:decimal', 'xsd:double', 'xsd:float')
                AND object_number IS NULL) OR
               (object_datatype IN ('xsd:integer', 'xsd:int', 'xsd:long')
                AND object_integer IS NULL)
             )"
        )?;

        let bad_triples: Vec<(String, String, String, Option<f64>, Option<i64>)> =
            stmt.query_map([tx_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if !bad_triples.is_empty() {
            log_backend(
                "warn",
                &format!(
                    "\n⚠️  FOUND {} TRIPLES WITH NUMERIC DATATYPE BUT NO TYPED COLUMN:",
                    bad_triples.len(),
                ),
            );
            for (idx, (subj, pred, dt, num, int)) in bad_triples.iter().enumerate().take(5) {
                log_backend(
                    "warn",
                    &format!(
                        "  #{}: {} {} (datatype={}, object_number={:?}, object_integer={:?})",
                        idx + 1, subj, pred, dt, num, int,
                    ),
                );
            }
            if bad_triples.len() > 5 {
                log_backend("warn", &format!("  ... and {} more", bad_triples.len() - 5));
            }
        }
    }

    Ok(tx_id)
}

/// Append triples without retracting existing (subject, predicate) pairs.
///
/// Unlike `assert_triples`, this does NOT retract existing values for the same
/// (subject, predicate) before inserting. Use this when you need to add triples
/// that share a predicate with existing triples that must be preserved.
pub fn append_triples(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let tx_id = if IN_BATCH_TX.with(|f| f.get()) {
        let sp = conn.savepoint()?;
        let tx_id = do_append_triples(&sp, triples, origin)?;
        sp.commit()?;
        tx_id
    } else {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let tx_id = do_append_triples(&tx, triples, origin)?;
        tx.commit()?;
        tx_id
    };
    if tx_id != 0 {
        let subjects: Vec<String> = triples.iter().map(|t| t.subject.clone()).collect();
        #[cfg(not(test))]
        crate::search::reindex_subjects(conn, &subjects);

        WRITTEN_SUBJECTS.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if !is_vocabulary_iri(&triple.subject) {
                    buf.push(triple.subject.clone());
                }
            }
        });
        WRITTEN_IRI_OBJECTS.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if let Object::Iri(iri) = &triple.object {
                    if !is_vocabulary_iri(iri) {
                        buf.push(iri.clone());
                    }
                }
            }
        });
    }
    Ok(tx_id)
}

fn do_append_triples(
    tx: &rusqlite::Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let now = now_millis();

    // Only insert triples whose exact (subject, predicate, object) doesn't already exist.
    let mut indices_to_insert: Vec<usize> = Vec::new();
    for (i, triple) in triples.iter().enumerate() {
        let existing = fetch_existing_rows(tx, &triple.subject, &triple.predicate)?;
        if !existing.iter().any(|row| object_matches_row(&triple.object, row)) {
            indices_to_insert.push(i);
        }
    }

    if indices_to_insert.is_empty() {
        return Ok(0);
    }

    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(tx, origin)?;
    for &idx in &indices_to_insert {
        insert_triple(tx, &triples[idx], tx_id, origin_id, now)?;
    }
    Ok(tx_id)
}

/// Retract triples (mark as retracted, don't delete)
///
/// Returns the transaction ID of the retraction.
/// If called from within batch_operations (enter_batch_transaction was called),
/// uses a nested SAVEPOINT so all calls participate in a single atomic transaction.
pub fn retract_triples(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let tx_id = if IN_BATCH_TX.with(|f| f.get()) {
        let sp = conn.savepoint()?;
        let tx_id = do_retract_triples(&sp, triples, origin)?;
        sp.commit()?;
        tx_id
    } else {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let tx_id = do_retract_triples(&tx, triples, origin)?;
        tx.commit()?;
        tx_id
    };
    if tx_id != 0 {
        let subjects: Vec<String> = triples.iter().map(|t| t.subject.clone()).collect();
        #[cfg(not(test))]
        crate::search::reindex_subjects(conn, &subjects);

        WRITTEN_SUBJECTS.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if !is_vocabulary_iri(&triple.subject) {
                    buf.push(triple.subject.clone());
                }
            }
        });
        WRITTEN_IRI_OBJECTS.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if let Object::Iri(iri) = &triple.object {
                    if !is_vocabulary_iri(iri) {
                        buf.push(iri.clone());
                    }
                }
            }
        });
    }
    Ok(tx_id)
}

fn do_retract_triples(
    tx: &Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let now = now_millis();
    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;

    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(tx, origin)?;

    for triple in triples {
        match &triple.object {
            Object::Iri(iri) | Object::Blank(iri) => {
                tx.execute(
                    "INSERT INTO triples (
                         subject, predicate, object, object_value, object_datatype,
                         object_language, object_type, object_number, object_integer,
                         object_boolean, tx, origin_id, retracted, created_at
                     )
                     SELECT subject, predicate, object, object_value, object_datatype,
                            object_language, object_type, object_number, object_integer,
                            object_boolean, ?1, ?2, 1, ?3
                     FROM triples
                     WHERE subject = ?4 AND predicate = ?5 AND object = ?6
                       AND retracted = 0
                       AND tx = (
                           SELECT MAX(tx) FROM triples
                           WHERE subject = ?4 AND predicate = ?5 AND object = ?6
                       )",
                    rusqlite::params![tx_id, origin_id, now,
                                      &triple.subject, &triple.predicate, iri],
                )?;
            }
            Object::Literal { value, datatype, language } => {
                let dt = datatype.as_deref().unwrap_or("xsd:string");
                let lang = language.as_deref().unwrap_or("");
                tx.execute(
                    "INSERT INTO triples (
                         subject, predicate, object, object_value, object_datatype,
                         object_language, object_type, object_number, object_integer,
                         object_boolean, tx, origin_id, retracted, created_at
                     )
                     SELECT subject, predicate, object, object_value, object_datatype,
                            object_language, object_type, object_number, object_integer,
                            object_boolean, ?1, ?2, 1, ?3
                     FROM triples
                     WHERE subject = ?4 AND predicate = ?5 AND object_value = ?6
                       AND COALESCE(object_datatype, 'xsd:string') = ?7
                       AND COALESCE(object_language, '') = ?8
                       AND retracted = 0
                       AND tx = (
                           SELECT MAX(tx) FROM triples
                           WHERE subject = ?4 AND predicate = ?5 AND object_value = ?6
                             AND COALESCE(object_datatype, 'xsd:string') = ?7
                             AND COALESCE(object_language, '') = ?8
                       )",
                    rusqlite::params![tx_id, origin_id, now,
                                      &triple.subject, &triple.predicate, value, dt, lang],
                )?;
            }
            Object::Integer(i) => {
                tx.execute(
                    "INSERT INTO triples (
                         subject, predicate, object, object_value, object_datatype,
                         object_language, object_type, object_number, object_integer,
                         object_boolean, tx, origin_id, retracted, created_at
                     )
                     SELECT subject, predicate, object, object_value, object_datatype,
                            object_language, object_type, object_number, object_integer,
                            object_boolean, ?1, ?2, 1, ?3
                     FROM triples
                     WHERE subject = ?4 AND predicate = ?5 AND object_integer = ?6
                       AND retracted = 0
                       AND tx = (
                           SELECT MAX(tx) FROM triples
                           WHERE subject = ?4 AND predicate = ?5 AND object_integer = ?6
                       )",
                    rusqlite::params![tx_id, origin_id, now,
                                      &triple.subject, &triple.predicate, i],
                )?;
            }
            Object::Number(n) => {
                tx.execute(
                    "INSERT INTO triples (
                         subject, predicate, object, object_value, object_datatype,
                         object_language, object_type, object_number, object_integer,
                         object_boolean, tx, origin_id, retracted, created_at
                     )
                     SELECT subject, predicate, object, object_value, object_datatype,
                            object_language, object_type, object_number, object_integer,
                            object_boolean, ?1, ?2, 1, ?3
                     FROM triples
                     WHERE subject = ?4 AND predicate = ?5 AND object_number = ?6
                       AND retracted = 0
                       AND tx = (
                           SELECT MAX(tx) FROM triples
                           WHERE subject = ?4 AND predicate = ?5 AND object_number = ?6
                       )",
                    rusqlite::params![tx_id, origin_id, now,
                                      &triple.subject, &triple.predicate, n],
                )?;
            }
            Object::Boolean(b) => {
                let bval = if *b { 1i64 } else { 0i64 };
                tx.execute(
                    "INSERT INTO triples (
                         subject, predicate, object, object_value, object_datatype,
                         object_language, object_type, object_number, object_integer,
                         object_boolean, tx, origin_id, retracted, created_at
                     )
                     SELECT subject, predicate, object, object_value, object_datatype,
                            object_language, object_type, object_number, object_integer,
                            object_boolean, ?1, ?2, 1, ?3
                     FROM triples
                     WHERE subject = ?4 AND predicate = ?5 AND object_boolean = ?6
                       AND retracted = 0
                       AND tx = (
                           SELECT MAX(tx) FROM triples
                           WHERE subject = ?4 AND predicate = ?5 AND object_boolean = ?6
                       )",
                    rusqlite::params![tx_id, origin_id, now,
                                      &triple.subject, &triple.predicate, bval],
                )?;
            }
            Object::DateTime(rfc3339) => {
                tx.execute(
                    "INSERT INTO triples (
                         subject, predicate, object, object_value, object_datatype,
                         object_language, object_type, object_number, object_integer,
                         object_boolean, tx, origin_id, retracted, created_at
                     )
                     SELECT subject, predicate, object, object_value, object_datatype,
                            object_language, object_type, object_number, object_integer,
                            object_boolean, ?1, ?2, 1, ?3
                     FROM triples
                     WHERE subject = ?4 AND predicate = ?5 AND object_value = ?6
                       AND retracted = 0
                       AND tx = (
                           SELECT MAX(tx) FROM triples
                           WHERE subject = ?4 AND predicate = ?5 AND object_value = ?6
                       )",
                    rusqlite::params![tx_id, origin_id, now,
                                      &triple.subject, &triple.predicate, rfc3339],
                )?;
            }
        }
    }

    Ok(tx_id)
}

/// Represents an existing active triple row fetched from the DB for comparison.
struct ExistingRow {
    rowid: i64,
    object: Option<String>,
    object_value: Option<String>,
    object_datatype: Option<String>,
    object_language: Option<String>,
    object_integer: Option<i64>,
    object_number: Option<f64>,
    object_boolean: Option<i64>,
}

/// Fetch all currently-active rows for a given (subject, predicate) pair.
/// A row is active when retracted=0 AND its tx equals the MAX(tx) for that specific
/// (subject, predicate, object) combination — meaning no newer retraction supersedes it.
fn fetch_existing_rows(
    tx: &rusqlite::Connection,
    subject: &str,
    predicate: &str,
) -> rusqlite::Result<Vec<ExistingRow>> {
    let mut stmt = tx.prepare(
        "SELECT rowid, object, object_value, object_datatype, object_language,
                object_integer, object_number, object_boolean
         FROM triples t1
         WHERE subject = ?1 AND predicate = ?2 AND retracted = 0
           AND tx = (
               SELECT MAX(tx) FROM triples t2
               WHERE t2.subject = ?1 AND t2.predicate = ?2
                 AND t2.object IS t1.object
                 AND t2.object_value IS t1.object_value
                 AND t2.object_datatype IS t1.object_datatype
                 AND t2.object_language IS t1.object_language
           )",
    )?;
    let rows = stmt.query_map([subject, predicate], |row| {
        Ok(ExistingRow {
            rowid: row.get(0)?,
            object: row.get(1)?,
            object_value: row.get(2)?,
            object_datatype: row.get(3)?,
            object_language: row.get(4)?,
            object_integer: row.get(5)?,
            object_number: row.get(6)?,
            object_boolean: row.get(7)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Returns true if an incoming `Object` is semantically identical to a DB row.
fn object_matches_row(obj: &Object, row: &ExistingRow) -> bool {
    match obj {
        Object::Iri(iri) | Object::Blank(iri) => row.object.as_deref() == Some(iri.as_str()),
        Object::Literal { value, datatype, language } => {
            row.object_value.as_deref() == Some(value.as_str())
                && row.object_datatype.as_deref().unwrap_or("xsd:string")
                    == datatype.as_deref().unwrap_or("xsd:string")
                && row.object_language.as_deref().unwrap_or("")
                    == language.as_deref().unwrap_or("")
        }
        Object::Integer(i) => row.object_integer == Some(*i),
        Object::Number(n) => row.object_number == Some(*n),
        Object::Boolean(b) => row.object_boolean == Some(if *b { 1 } else { 0 }),
        Object::DateTime(rfc3339) => row.object_value.as_deref() == Some(rfc3339.as_str()),
    }
}

/// Insert a single triple into the database
fn insert_triple(
    tx: &rusqlite::Connection,
    triple: &Triple,
    tx_id: i64,
    origin_id: i64,
    created_at: i64,
) -> rusqlite::Result<()> {
    // Need to compute everything together to ensure datatype matches typed columns
    let int_str;
    let num_str;
    let bool_str;
    let dt_str;

    let (
        object,
        object_value,
        object_datatype,
        object_language,
        object_number,
        object_integer,
        object_boolean,
    ) = match &triple.object {
        Object::Iri(iri) => (Some(iri.as_str()), None, None, None, None, None, None),
        Object::Blank(blank) => (Some(blank.as_str()), None, None, None, None, None, None),

        Object::Integer(i) => {
            int_str = i.to_string();
            (None, Some(int_str.as_str()), Some("xsd:integer"), None, None, Some(*i), None)
        }
        Object::Number(n) => {
            num_str = n.to_string();
            (None, Some(num_str.as_str()), Some("xsd:decimal"), None, Some(*n), None, None)
        }
        Object::Boolean(b) => {
            bool_str = b.to_string();
            (
                None,
                Some(bool_str.as_str()),
                Some("xsd:boolean"),
                None,
                None,
                None,
                Some(if *b { 1 } else { 0 }),
            )
        }
        Object::DateTime(rfc3339) => {
            dt_str = chrono::DateTime::parse_from_rfc3339(rfc3339)
                .unwrap_or(chrono::DateTime::UNIX_EPOCH.into())
                .with_timezone(&chrono::Utc)
                .to_rfc3339();
            (None, Some(dt_str.as_str()), Some("xsd:dateTime"), None, None, None, None)
        }

        Object::Literal { value, datatype, language } => {
            // If parse fails, return an error (user provided invalid data)
            match datatype.as_deref() {
                Some("xsd:decimal") | Some("xsd:double") | Some("xsd:float") => {
                    let n = value.parse::<f64>()
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Failed to parse float literal '{}' for triple: \
                                     {} {} {} - Error: {}",
                                    value, triple.subject, triple.predicate, value, e,
                                ),
                            )
                        )))?;
                    (
                        None,
                        Some(value.as_str()),
                        datatype.as_deref(),
                        language.as_deref(),
                        Some(n),
                        None,
                        None,
                    )
                }
                Some("xsd:integer") | Some("xsd:int") | Some("xsd:long") => {
                    let i = value.parse::<i64>()
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Failed to parse integer literal '{}' for triple: \
                                     {} {} {} - Error: {}",
                                    value, triple.subject, triple.predicate, value, e,
                                ),
                            )
                        )))?;
                    (
                        None,
                        Some(value.as_str()),
                        datatype.as_deref(),
                        language.as_deref(),
                        None,
                        Some(i),
                        None,
                    )
                }
                Some("xsd:boolean") => {
                    let b = match value.as_str() {
                        "true" | "1" => 1,
                        "false" | "0" => 0,
                        _ => {
                            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                                std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    format!(
                                        "Invalid boolean literal '{}' for triple: {} {} {} \
                                         - Expected: 'true', 'false', '1', or '0'",
                                        value, triple.subject, triple.predicate, value,
                                    ),
                                )
                            )));
                        }
                    };
                    (
                        None,
                        Some(value.as_str()),
                        datatype.as_deref(),
                        language.as_deref(),
                        None,
                        None,
                        Some(b),
                    )
                }
                Some("xsd:dateTime") => {
                    let parsed = chrono::DateTime::parse_from_rfc3339(value)
                        .map_err(|_| rusqlite::Error::ToSqlConversionFailure(Box::new(
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Failed to parse dateTime literal '{}' for triple: \
                                     {} {} {} - Expected RFC3339 string (e.g. '2026-03-08T00:00:00+00:00')",
                                    value, triple.subject, triple.predicate, value,
                                ),
                            )
                        )))?;
                    dt_str = parsed.with_timezone(&chrono::Utc).to_rfc3339();
                    (
                        None,
                        Some(dt_str.as_str()),
                        datatype.as_deref(),
                        language.as_deref(),
                        None,
                        None,
                        None,
                    )
                }
                Some("xsd:date") => {
                    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Failed to parse date literal '{}' for triple: \
                                     {} {} {} - Error: {} - Expected format: YYYY-MM-DD \
                                     (e.g., '2020-11-17')",
                                    value, triple.subject, triple.predicate, value, e,
                                ),
                            )
                        )))?;
                    (
                        None,
                        Some(value.as_str()),
                        datatype.as_deref(),
                        language.as_deref(),
                        None,
                        None,
                        None,
                    )
                }
                _ => {
                    (
                        None,
                        Some(value.as_str()),
                        datatype.as_deref(),
                        language.as_deref(),
                        None,
                        None,
                        None,
                    )
                }
            }
        }
    };

    let object_type = triple.object.object_type();

    let result = tx.execute(
        "INSERT INTO triples (
            subject, predicate, object, object_value, object_datatype, object_language,
            object_type, object_number, object_integer, object_boolean,
            tx, origin_id, retracted, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
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
            object_boolean,
            tx_id,
            origin_id,
            created_at,
        ],
    );

    if let Err(e) = result {
        log_backend("error", &format!("\n❌ INSERT FAILED:
   Subject: {}
   Predicate: {}
   Object: {:?}
   object_datatype: {:?}
   object_number: {:?}
   object_integer: {:?}
   object_boolean: {:?}
   Error: {}\n",
            triple.subject,
            triple.predicate,
            triple.object,
            object_datatype,
            object_number,
            object_integer,
            object_boolean,
            e));
        return Err(e);
    }

    Ok(())
}

/// Get or create origin ID
fn get_or_create_origin(tx: &rusqlite::Connection, origin: &str) -> rusqlite::Result<i64> {
    match tx.query_row(
        "SELECT id FROM origins WHERE name = ?",
        [origin],
        |row| row.get(0),
    ) {
        Ok(id) => Ok(id),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            tx.execute("INSERT INTO origins (name) VALUES (?)", [origin])?;
            Ok(tx.last_insert_rowid())
        }
        Err(e) => Err(e),
    }
}

/// Rename an IRI throughout the store.
///
/// Retracts all active triples that reference `old_iri` (as subject or IRI object)
/// and re-inserts them with `new_iri`. No-op if there are no matching triples.
pub fn rename_iri(
    conn: &mut Connection,
    old_iri: &str,
    new_iri: &str,
    origin: &str,
) -> Result<()> {
    if IN_BATCH_TX.with(|f| f.get()) {
        let sp = conn.savepoint()?;
        do_rename_iri(&sp, old_iri, new_iri, origin)?;
        sp.commit()?;
    } else {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        do_rename_iri(&tx, old_iri, new_iri, origin)?;
        tx.commit()?;
    }
    #[cfg(not(test))]
    crate::search::reindex_subjects(conn, &[old_iri.to_string(), new_iri.to_string()]);
    Ok(())
}

struct FullRow {
    rowid: i64,
    subject: String,
    predicate: String,
    object: Option<String>,
    object_value: Option<String>,
    object_datatype: Option<String>,
    object_language: Option<String>,
    object_type: String,
    object_number: Option<f64>,
    object_integer: Option<i64>,
    object_boolean: Option<i64>,
}

fn do_rename_iri(
    tx: &rusqlite::Connection,
    old_iri: &str,
    new_iri: &str,
    origin: &str,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "SELECT rowid, subject, predicate, object, object_value, object_datatype,
                object_language, object_type, object_number, object_integer, object_boolean
         FROM triples
         WHERE (subject = ?1 OR object = ?1) AND retracted = 0"
    )?;

    let rows: Vec<FullRow> = stmt.query_map([old_iri], |row| {
        Ok(FullRow {
            rowid: row.get(0)?,
            subject: row.get(1)?,
            predicate: row.get(2)?,
            object: row.get(3)?,
            object_value: row.get(4)?,
            object_datatype: row.get(5)?,
            object_language: row.get(6)?,
            object_type: row.get(7)?,
            object_number: row.get(8)?,
            object_integer: row.get(9)?,
            object_boolean: row.get(10)?,
        })
    })?.collect::<rusqlite::Result<Vec<_>>>()?;

    if rows.is_empty() {
        return Ok(());
    }

    let now = now_millis();
    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(tx, origin)?;

    for row in &rows {
        tx.execute(
            "INSERT INTO triples (
                 subject, predicate, object, object_value, object_datatype,
                 object_language, object_type, object_number, object_integer,
                 object_boolean, tx, origin_id, retracted, created_at
             )
             SELECT subject, predicate, object, object_value, object_datatype,
                    object_language, object_type, object_number, object_integer,
                    object_boolean, ?1, ?2, 1, ?3
             FROM triples WHERE rowid = ?4",
            rusqlite::params![tx_id, origin_id, now, row.rowid],
        )?;

        let new_subject: &str = if row.subject == old_iri { new_iri } else { &row.subject };
        let new_object_owned: Option<String> = row.object.as_ref().map(|o| {
            if o == old_iri { new_iri.to_string() } else { o.clone() }
        });
        let new_object: Option<&str> = new_object_owned.as_deref();

        tx.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_datatype,
                                  object_language, object_type, object_number, object_integer,
                                  object_boolean, tx, origin_id, retracted, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
            rusqlite::params![
                new_subject,
                row.predicate,
                new_object,
                row.object_value,
                row.object_datatype,
                row.object_language,
                row.object_type,
                row.object_number,
                row.object_integer,
                row.object_boolean,
                tx_id,
                origin_id,
                now,
            ],
        )?;
    }

    Ok(())
}

/// Get current Unix time in milliseconds
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_millis() as i64
}


#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
