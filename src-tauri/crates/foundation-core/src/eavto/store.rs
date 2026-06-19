/// EVTO Store Functions
///
/// Functions for asserting and retracting triples (append-only, immutable)

use rusqlite::Connection;
use std::collections::HashMap;
use super::triple_type::Triple;
use super::object_type::Object;
use crate::diagnostics::log_backend;
use chrono;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// A single written triple carried in the notify payload, enabling creation-query
/// matching and per-entity reactive updates in the receiver.
///
/// `subject_type` is left empty by the store (no DB access on the write thread);
/// the notify receiver resolves it from the triple store when matching creation-queries.
#[derive(Debug, Clone)]
pub struct WrittenTriple {
    pub subject: String,
    pub predicate: String,
    /// IRI object (when the triple points to another entity).
    pub object_iri: Option<String>,
    /// Literal value (when the triple carries a data value).
    pub object_value: Option<String>,
    /// The tx of the transaction that wrote this triple.
    pub tx: i64,
}

std::thread_local! {
    /// Set to true while batch_operations holds an outer transaction open.
    /// When true, assert_triples/retract_triples use SAVEPOINTs instead of BEGIN
    /// so that all operations participate in the same atomic transaction.
    static IN_BATCH_TX: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// Accumulates subject→predicates written during assert_triples on the write thread.
    /// Drained by DbExecutor after each write to emit entity-updated notifications.
    static WRITTEN_SUBJECT_PREDICATES: std::cell::RefCell<HashMap<String, Vec<String>>> = std::cell::RefCell::new(HashMap::new());

    /// Accumulates IRI objects written during assert_triples on the write thread.
    /// Drained by DbExecutor after each write to emit entity-referenced notifications.
    static WRITTEN_IRI_OBJECTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };

    /// Accumulates the full triple details written during this write batch.
    /// Carries (predicate, object_iri, object_value, tx) per subject for creation-query
    /// matching and cursor-based replay in the notify receiver.
    static WRITTEN_TRIPLES: std::cell::RefCell<Vec<WrittenTriple>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Returns all subject→predicates accumulated since the last drain, removing them from the buffer.
/// Only meaningful when called from the write thread.
pub fn drain_written_subject_predicates() -> HashMap<String, Vec<String>> {
    WRITTEN_SUBJECT_PREDICATES.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

/// Returns all IRI objects accumulated since the last drain, removing them from the buffer.
/// Only meaningful when called from the write thread.
pub fn drain_written_iri_objects() -> Vec<String> {
    WRITTEN_IRI_OBJECTS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

/// Returns all written triples accumulated since the last drain, removing them from the buffer.
/// Only meaningful when called from the write thread.
pub fn drain_written_triples() -> Vec<WrittenTriple> {
    WRITTEN_TRIPLES.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

/// Marks the current thread as being inside a batch transaction.
/// Returns a guard that restores the previous flag on drop, so nested
/// calls don't prematurely clear the flag for the outer scope.
pub fn enter_batch_transaction() -> BatchTransactionGuard {
    let prev = IN_BATCH_TX.with(|f| f.replace(true));
    BatchTransactionGuard(prev)
}

pub struct BatchTransactionGuard(bool);

impl Drop for BatchTransactionGuard {
    fn drop(&mut self) {
        let prev = self.0;
        IN_BATCH_TX.with(|f| f.set(prev));
    }
}

/// Runs `f` inside a single SQLite transaction so all writes done via
/// `assert_triples`/`retract_triples` participate in one atomic commit.
/// Rolls back on Err. Must NOT be called from within another transaction
/// (use `enter_batch_transaction` directly if you need nested savepoints).
pub fn with_transaction<T, F>(
    conn: &mut Connection,
    f: F,
) -> std::result::Result<T, String>
where
    F: FnOnce(&mut Connection) -> std::result::Result<T, String>,
{
    let _batch = enter_batch_transaction();
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| format!("begin tx: {}", e))?;
    match f(conn) {
        Ok(value) => {
            conn.execute_batch("COMMIT")
                .map_err(|e| format!("commit tx: {}", e))?;
            Ok(value)
        }
        Err(e) => {
            conn.execute_batch("ROLLBACK").ok();
            Err(e)
        }
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
        WRITTEN_SUBJECT_PREDICATES.with(|v| {
            let mut map = v.borrow_mut();
            for triple in triples {
                if !is_vocabulary_iri(&triple.subject) {
                    map.entry(triple.subject.clone())
                        .or_default()
                        .push(triple.predicate.clone());
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
        WRITTEN_TRIPLES.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if is_vocabulary_iri(&triple.subject) {
                    continue;
                }
                buf.push(WrittenTriple {
                    subject: triple.subject.clone(),
                    predicate: triple.predicate.clone(),
                    object_iri: triple.object.as_iri().map(str::to_owned),
                    object_value: triple.object.as_literal(),
                    tx: tx_id,
                });
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

    let mut indices_to_insert: Vec<usize> = Vec::new();

    for ((subject, predicate), incoming_indices) in &groups {
        let existing = fetch_existing_rows(tx, subject, predicate)?;
        let incoming: Vec<&Object> = incoming_indices.iter().map(|&i| &triples[i].object).collect();

        let is_noop = existing.len() == incoming.len()
            && incoming.iter().all(|obj| existing.iter().any(|row| object_matches_row(obj, row)));

        if !is_noop {
            indices_to_insert.extend(incoming_indices);
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

    for ((subject, predicate), incoming_indices) in &groups {
        let had_inserts = incoming_indices.iter().any(|i| indices_to_insert.contains(i));
        if had_inserts {
            demote_superseded(tx, subject, predicate, tx_id)?;
        }
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
        WRITTEN_SUBJECT_PREDICATES.with(|v| {
            let mut map = v.borrow_mut();
            for triple in triples {
                if !is_vocabulary_iri(&triple.subject) {
                    map.entry(triple.subject.clone())
                        .or_default()
                        .push(triple.predicate.clone());
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
        WRITTEN_TRIPLES.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if is_vocabulary_iri(&triple.subject) {
                    continue;
                }
                buf.push(WrittenTriple {
                    subject: triple.subject.clone(),
                    predicate: triple.predicate.clone(),
                    object_iri: triple.object.as_iri().map(str::to_owned),
                    object_value: triple.object.as_literal(),
                    tx: tx_id,
                });
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

    let mut groups_to_extend: Vec<(&str, &str, Vec<usize>)> = Vec::new();
    for ((subject, predicate), incoming_indices) in &groups {
        let existing = fetch_existing_rows(tx, subject, predicate)?;
        let new_indices: Vec<usize> = incoming_indices
            .iter()
            .filter(|&&idx| !existing.iter().any(|row| object_matches_row(&triples[idx].object, row)))
            .copied()
            .collect();
        if !new_indices.is_empty() {
            groups_to_extend.push((subject, predicate, new_indices));
        }
    }

    if groups_to_extend.is_empty() {
        return Ok(0);
    }

    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(tx, origin)?;

    for (subject, predicate, new_indices) in &groups_to_extend {
        tx.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_datatype,
                                  object_language, object_type, object_number, object_integer,
                                  object_boolean, tx, origin_id, retracted, created_at)
             SELECT subject, predicate, object, object_value, object_datatype,
                    object_language, object_type, object_number, object_integer,
                    object_boolean, ?1, ?2, 0, ?3
             FROM triples
             WHERE subject = ?4 AND predicate = ?5 AND retracted = 0
               AND is_current = 1 AND tx < ?1",
            rusqlite::params![tx_id, origin_id, now, subject, predicate],
        )?;
        for &idx in new_indices {
            insert_triple(tx, &triples[idx], tx_id, origin_id, now)?;
        }
        demote_superseded(tx, subject, predicate, tx_id)?;
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
        WRITTEN_SUBJECT_PREDICATES.with(|v| {
            let mut map = v.borrow_mut();
            for triple in triples {
                if !is_vocabulary_iri(&triple.subject) {
                    map.entry(triple.subject.clone())
                        .or_default()
                        .push(triple.predicate.clone());
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
        WRITTEN_TRIPLES.with(|v| {
            let mut buf = v.borrow_mut();
            for triple in triples {
                if is_vocabulary_iri(&triple.subject) {
                    continue;
                }
                buf.push(WrittenTriple {
                    subject: triple.subject.clone(),
                    predicate: triple.predicate.clone(),
                    object_iri: triple.object.as_iri().map(str::to_owned),
                    object_value: triple.object.as_literal(),
                    tx: tx_id,
                });
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

    let mut groups: Vec<((String, String), Vec<usize>)> = Vec::new();
    let mut group_index: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for (i, triple) in triples.iter().enumerate() {
        let key = (triple.subject.clone(), triple.predicate.clone());
        if let Some(&idx) = group_index.get(&key) {
            groups[idx].1.push(i);
        } else {
            group_index.insert(key.clone(), groups.len());
            groups.push((key, vec![i]));
        }
    }

    struct GroupWork {
        subject: String,
        predicate: String,
        remaining: Vec<Object>,
    }

    let mut work: Vec<GroupWork> = Vec::new();

    for ((subject, predicate), incoming_indices) in &groups {
        let existing = fetch_existing_rows(tx, subject, predicate)?;
        if existing.is_empty() {
            continue;
        }

        let remaining: Vec<Object> = existing
            .iter()
            .filter(|row| {
                !incoming_indices
                    .iter()
                    .any(|&i| object_matches_row(&triples[i].object, row))
            })
            .map(existing_row_to_object)
            .collect();

        work.push(GroupWork {
            subject: subject.clone(),
            predicate: predicate.clone(),
            remaining,
        });
    }

    if work.is_empty() {
        return Ok(0);
    }

    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(tx, origin)?;

    for w in work {
        if w.remaining.is_empty() {
            tx.execute(
                "INSERT INTO triples (subject, predicate, object, object_value, object_datatype,
                                      object_language, object_type, object_number, object_integer,
                                      object_boolean, tx, origin_id, retracted, created_at)
                 SELECT subject, predicate, object, object_value, object_datatype,
                        object_language, object_type, object_number, object_integer,
                        object_boolean, ?1, ?2, 1, ?3
                 FROM triples
                 WHERE subject = ?4 AND predicate = ?5 AND retracted = 0
                   AND is_current = 1 AND tx < ?1
                 LIMIT 1",
                rusqlite::params![tx_id, origin_id, now, w.subject, w.predicate],
            )?;
        } else {
            for obj in w.remaining {
                let triple = Triple::new(&w.subject, &w.predicate, obj);
                insert_triple(tx, &triple, tx_id, origin_id, now)?;
            }
        }
        demote_superseded(tx, &w.subject, &w.predicate, tx_id)?;
    }

    Ok(tx_id)
}

/// Mark all rows for (subject, predicate) with tx < tx_id as non-current.
///
/// Called once per (subject, predicate) group immediately after the new rows are
/// inserted, within the same transaction. Tombstones are demoted too — a superseded
/// tombstone is no longer current even though retracted=1.
///
/// Uses idx_spr (subject, predicate, retracted, tx); the omitted retracted filter is
/// intentional so superseded tombstones are also covered.
fn demote_superseded(
    tx: &rusqlite::Connection,
    subject: &str,
    predicate: &str,
    tx_id: i64,
) -> rusqlite::Result<usize> {
    tx.execute(
        "UPDATE triples SET is_current = 0
         WHERE subject = ?1 AND predicate = ?2 AND is_current = 1 AND tx < ?3",
        rusqlite::params![subject, predicate, tx_id],
    )
}

/// Represents an existing active triple row fetched from the DB for comparison.
struct ExistingRow {
    object: Option<String>,
    object_value: Option<String>,
    object_datatype: Option<String>,
    object_language: Option<String>,
    object_integer: Option<i64>,
    object_number: Option<f64>,
    object_boolean: Option<i64>,
}

/// Fetch all currently-active rows for a given (subject, predicate) pair.
/// Uses `is_current = 1` maintained by the write path instead of a correlated MAX(tx) subquery.
fn fetch_existing_rows(
    tx: &rusqlite::Connection,
    subject: &str,
    predicate: &str,
) -> rusqlite::Result<Vec<ExistingRow>> {
    let mut stmt = tx.prepare(
        "SELECT object, object_value, object_datatype, object_language,
                object_integer, object_number, object_boolean
         FROM triples
         WHERE subject = ?1 AND predicate = ?2 AND retracted = 0 AND is_current = 1",
    )?;
    let rows = stmt.query_map([subject, predicate], |row| {
        Ok(ExistingRow {
            object: row.get(0)?,
            object_value: row.get(1)?,
            object_datatype: row.get(2)?,
            object_language: row.get(3)?,
            object_integer: row.get(4)?,
            object_number: row.get(5)?,
            object_boolean: row.get(6)?,
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

fn existing_row_to_object(row: &ExistingRow) -> Object {
    if let Some(ref iri) = row.object {
        if iri.starts_with("_:") {
            Object::Blank(iri.clone())
        } else {
            Object::Iri(iri.clone())
        }
    } else if let Some(i) = row.object_integer {
        Object::Integer(i)
    } else if let Some(n) = row.object_number {
        Object::Number(n)
    } else if let Some(b) = row.object_boolean {
        Object::Boolean(b != 0)
    } else {
        let value = row.object_value.clone().unwrap_or_default();
        let dt = row.object_datatype.clone();
        if dt.as_deref() == Some("xsd:dateTime") {
            Object::DateTime(value)
        } else {
            Object::Literal {
                value,
                datatype: dt,
                language: row.object_language.clone(),
            }
        }
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

        demote_superseded(tx, &row.subject, &row.predicate, tx_id)?;
        demote_superseded(tx, new_subject, &row.predicate, tx_id)?;
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
