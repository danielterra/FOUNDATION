/// EVTO Store Functions
///
/// Functions for asserting and retracting triples (append-only, immutable)

use turso::{Connection, params};
use super::triple_type::Triple;
use super::object_type::Object;
use crate::commands::log_backend;
use chrono;
use std::sync::atomic::{AtomicU64, Ordering};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

static SP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_sp_name() -> String {
    format!("eavto_sp_{}", SP_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Assert triples (add new facts to the store).
///
/// Returns the transaction ID of the assertion.
/// Uses a SAVEPOINT, which is safe to call both inside and outside an outer transaction.
pub async fn assert_triples(
    conn: &Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let sp = next_sp_name();
    conn.execute(&format!("SAVEPOINT {}", sp), ()).await?;
    match do_assert_triples(conn, triples, origin).await {
        Ok(tx_id) => {
            conn.execute(&format!("RELEASE {}", sp), ()).await?;
            Ok(tx_id)
        }
        Err(e) => {
            let _ = conn.execute(&format!("ROLLBACK TO {}", sp), ()).await;
            Err(e)
        }
    }
}

async fn do_assert_triples(
    conn: &Connection,
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

    let mut rows_to_retract: Vec<i64> = Vec::new();
    let mut indices_to_insert: Vec<usize> = Vec::new();

    for ((subject, predicate), incoming_indices) in &groups {
        let existing = fetch_existing_rows(conn, subject, predicate).await?;
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

    if rows_to_retract.is_empty() && indices_to_insert.is_empty() {
        return Ok(0);
    }

    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        params![origin, now],
    ).await?;
    let tx_id = last_insert_rowid(conn).await?;
    let origin_id = get_or_create_origin(conn, origin).await?;

    for id in &rows_to_retract {
        conn.execute("UPDATE triples SET retracted = 1 WHERE rowid = ?", params![id]).await?;
    }
    for &idx in &indices_to_insert {
        insert_triple(conn, &triples[idx], tx_id, origin_id, now).await?;
    }

    Ok(tx_id)
}

/// Append triples without retracting existing (subject, predicate) pairs.
///
/// Unlike `assert_triples`, this does NOT retract existing values for the same
/// (subject, predicate) before inserting.
/// Uses a SAVEPOINT, which is safe to call both inside and outside an outer transaction.
pub async fn append_triples(
    conn: &Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let sp = next_sp_name();
    conn.execute(&format!("SAVEPOINT {}", sp), ()).await?;
    match do_append_triples(conn, triples, origin).await {
        Ok(tx_id) => {
            conn.execute(&format!("RELEASE {}", sp), ()).await?;
            Ok(tx_id)
        }
        Err(e) => {
            let _ = conn.execute(&format!("ROLLBACK TO {}", sp), ()).await;
            Err(e)
        }
    }
}

async fn do_append_triples(
    conn: &Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let now = now_millis();

    let mut indices_to_insert: Vec<usize> = Vec::new();
    for (i, triple) in triples.iter().enumerate() {
        let existing = fetch_existing_rows(conn, &triple.subject, &triple.predicate).await?;
        if !existing.iter().any(|row| object_matches_row(&triple.object, row)) {
            indices_to_insert.push(i);
        }
    }

    if indices_to_insert.is_empty() {
        return Ok(0);
    }

    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        params![origin, now],
    ).await?;
    let tx_id = last_insert_rowid(conn).await?;
    let origin_id = get_or_create_origin(conn, origin).await?;
    for &idx in &indices_to_insert {
        insert_triple(conn, &triples[idx], tx_id, origin_id, now).await?;
    }
    Ok(tx_id)
}

/// Retract triples (mark as retracted, don't delete).
///
/// Returns the transaction ID of the retraction.
/// Uses a SAVEPOINT, which is safe to call both inside and outside an outer transaction.
pub async fn retract_triples(
    conn: &Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let sp = next_sp_name();
    conn.execute(&format!("SAVEPOINT {}", sp), ()).await?;
    match do_retract_triples(conn, triples, origin).await {
        Ok(tx_id) => {
            conn.execute(&format!("RELEASE {}", sp), ()).await?;
            Ok(tx_id)
        }
        Err(e) => {
            let _ = conn.execute(&format!("ROLLBACK TO {}", sp), ()).await;
            Err(e)
        }
    }
}

async fn do_retract_triples(
    conn: &Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    let now = now_millis();
    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        params![origin, now],
    ).await?;

    let tx_id = last_insert_rowid(conn).await?;
    let _origin_id = get_or_create_origin(conn, origin).await?;

    for triple in triples {
        match &triple.object {
            Object::Iri(iri) | Object::Blank(iri) => {
                conn.execute(
                    "UPDATE triples
                     SET retracted = 1
                     WHERE subject = ? AND predicate = ? AND object = ? AND retracted = 0",
                    params![triple.subject.as_str(), triple.predicate.as_str(), iri.as_str()],
                ).await?;
            }
            Object::Literal { value, datatype, language } => {
                let dt = datatype.as_deref().unwrap_or("xsd:string");
                let lang = language.as_deref().unwrap_or("");
                conn.execute(
                    "UPDATE triples
                     SET retracted = 1
                     WHERE subject = ? AND predicate = ? AND object_value = ?
                       AND COALESCE(object_datatype, 'xsd:string') = ?
                       AND COALESCE(object_language, '') = ?
                       AND retracted = 0",
                    params![
                        triple.subject.as_str(),
                        triple.predicate.as_str(),
                        value.as_str(),
                        dt,
                        lang,
                    ],
                ).await?;
            }
            Object::Integer(i) => {
                conn.execute(
                    "UPDATE triples
                     SET retracted = 1
                     WHERE subject = ? AND predicate = ? AND object_integer = ? AND retracted = 0",
                    params![triple.subject.as_str(), triple.predicate.as_str(), i],
                ).await?;
            }
            Object::Number(n) => {
                conn.execute(
                    "UPDATE triples
                     SET retracted = 1
                     WHERE subject = ? AND predicate = ? AND object_number = ? AND retracted = 0",
                    params![triple.subject.as_str(), triple.predicate.as_str(), n],
                ).await?;
            }
            Object::Boolean(b) => {
                let bval: i64 = if *b { 1 } else { 0 };
                conn.execute(
                    "UPDATE triples
                     SET retracted = 1
                     WHERE subject = ? AND predicate = ? AND object_boolean = ? AND retracted = 0",
                    params![triple.subject.as_str(), triple.predicate.as_str(), bval],
                ).await?;
            }
            Object::DateTime(rfc3339) => {
                conn.execute(
                    "UPDATE triples
                     SET retracted = 1
                     WHERE subject = ? AND predicate = ? AND object_value = ? AND retracted = 0",
                    params![triple.subject.as_str(), triple.predicate.as_str(), rfc3339.as_str()],
                ).await?;
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

/// Fetch all active (retracted = 0) rows for a given (subject, predicate) pair.
async fn fetch_existing_rows(
    conn: &Connection,
    subject: &str,
    predicate: &str,
) -> Result<Vec<ExistingRow>> {
    let mut rows = conn.query(
        "SELECT rowid, object, object_value, object_datatype, object_language,
                object_integer, object_number, object_boolean
         FROM triples
         WHERE subject = ? AND predicate = ? AND retracted = 0",
        params![subject, predicate],
    ).await?;
    let mut result = Vec::new();

    while let Some(row) = rows.next().await? {
        result.push(ExistingRow {
            rowid: row.get_value(0)?.as_integer().copied().unwrap_or(0),
            object: opt_text(&row, 1)?,
            object_value: opt_text(&row, 2)?,
            object_datatype: opt_text(&row, 3)?,
            object_language: opt_text(&row, 4)?,
            object_integer: row.get_value(5)?.as_integer().copied(),
            object_number: row.get_value(6)?.as_real().copied(),
            object_boolean: row.get_value(7)?.as_integer().copied(),
        });
    }

    Ok(result)
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

/// Insert a single triple into the database.
async fn insert_triple(
    conn: &Connection,
    triple: &Triple,
    tx_id: i64,
    origin_id: i64,
    created_at: i64,
) -> Result<()> {
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
    ): (Option<&str>, Option<&str>, Option<&str>, Option<&str>, Option<f64>, Option<i64>, Option<i64>) =
        match &triple.object {
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
                    .unwrap_or_else(|_| chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00+00:00").unwrap())
                    .with_timezone(&chrono::Utc)
                    .to_rfc3339();
                (None, Some(dt_str.as_str()), Some("xsd:dateTime"), None, None, None, None)
            }

            Object::Literal { value, datatype, language } => {
                match datatype.as_deref() {
                    Some("xsd:decimal") | Some("xsd:double") | Some("xsd:float") => {
                        let n = value.parse::<f64>().map_err(|e| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Failed to parse float literal '{}' for triple: \
                                     {} {} {} - Error: {}",
                                    value, triple.subject, triple.predicate, value, e,
                                ),
                            )
                        })?;
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
                        let i = value.parse::<i64>().map_err(|e| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Failed to parse integer literal '{}' for triple: \
                                     {} {} {} - Error: {}",
                                    value, triple.subject, triple.predicate, value, e,
                                ),
                            )
                        })?;
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
                        let b: i64 = match value.as_str() {
                            "true" | "1" => 1,
                            "false" | "0" => 0,
                            _ => {
                                return Err(Box::new(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    format!(
                                        "Invalid boolean literal '{}' for triple: {} {} {} \
                                         - Expected: 'true', 'false', '1', or '0'",
                                        value, triple.subject, triple.predicate, value,
                                    ),
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
                        let parsed = chrono::DateTime::parse_from_rfc3339(value).map_err(|_| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Failed to parse dateTime literal '{}' for triple: \
                                     {} {} {} - Expected RFC3339 string",
                                    value, triple.subject, triple.predicate, value,
                                ),
                            )
                        })?;
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
                        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|e| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "Failed to parse date literal '{}' for triple: \
                                     {} {} {} - Error: {} - Expected format: YYYY-MM-DD",
                                    value, triple.subject, triple.predicate, value, e,
                                ),
                            )
                        })?;
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

    let result = conn.execute(
        "INSERT INTO triples (
            subject, predicate, object, object_value, object_datatype, object_language,
            object_type, object_number, object_integer, object_boolean,
            tx, origin_id, retracted, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
        params![
            triple.subject.as_str(),
            triple.predicate.as_str(),
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
    ).await;

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
        return Err(Box::new(e));
    }

    Ok(())
}

/// Get or create origin ID.
async fn get_or_create_origin(conn: &Connection, origin: &str) -> Result<i64> {
    let mut rows = conn.query("SELECT id FROM origins WHERE name = ?", params![origin]).await?;

    if let Some(row) = rows.next().await? {
        return Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0));
    }

    conn.execute("INSERT INTO origins (name) VALUES (?)", params![origin]).await?;
    last_insert_rowid(conn).await
}

/// Rename an IRI throughout the store.
///
/// Retracts all active triples that reference `old_iri` (as subject or IRI object)
/// and re-inserts them with `new_iri`. No-op if there are no matching triples.
/// Uses a SAVEPOINT, which is safe to call both inside and outside an outer transaction.
pub async fn rename_iri(
    conn: &Connection,
    old_iri: &str,
    new_iri: &str,
    origin: &str,
) -> Result<()> {
    let sp = next_sp_name();
    conn.execute(&format!("SAVEPOINT {}", sp), ()).await?;
    match do_rename_iri(conn, old_iri, new_iri, origin).await {
        Ok(()) => {
            conn.execute(&format!("RELEASE {}", sp), ()).await?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute(&format!("ROLLBACK TO {}", sp), ()).await;
            Err(e)
        }
    }
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

async fn do_rename_iri(
    conn: &Connection,
    old_iri: &str,
    new_iri: &str,
    origin: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT rowid, subject, predicate, object, object_value, object_datatype,
                object_language, object_type, object_number, object_integer, object_boolean
         FROM triples
         WHERE (subject = ?1 OR object = ?1) AND retracted = 0"
    ).await?;

    let mut rows = stmt.query(params![old_iri]).await?;
    let mut full_rows: Vec<FullRow> = Vec::new();

    while let Some(row) = rows.next().await? {
        full_rows.push(FullRow {
            rowid: row.get_value(0)?.as_integer().copied().unwrap_or(0),
            subject: row.get_value(1)?.as_text().cloned().unwrap_or_default(),
            predicate: row.get_value(2)?.as_text().cloned().unwrap_or_default(),
            object: opt_text(&row, 3)?,
            object_value: opt_text(&row, 4)?,
            object_datatype: opt_text(&row, 5)?,
            object_language: opt_text(&row, 6)?,
            object_type: row.get_value(7)?.as_text().cloned().unwrap_or_default(),
            object_number: row.get_value(8)?.as_real().copied(),
            object_integer: row.get_value(9)?.as_integer().copied(),
            object_boolean: row.get_value(10)?.as_integer().copied(),
        });
    }

    if full_rows.is_empty() {
        return Ok(());
    }

    let now = now_millis();
    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        params![origin, now],
    ).await?;
    let tx_id = last_insert_rowid(conn).await?;
    let origin_id = get_or_create_origin(conn, origin).await?;

    for row in &full_rows {
        conn.execute("UPDATE triples SET retracted = 1 WHERE rowid = ?", params![row.rowid]).await?;

        let new_subject: &str = if row.subject == old_iri { new_iri } else { &row.subject };
        let new_object_owned: Option<String> = row.object.as_ref().map(|o| {
            if o == old_iri { new_iri.to_string() } else { o.clone() }
        });
        let new_object: Option<&str> = new_object_owned.as_deref();

        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_datatype,
                                  object_language, object_type, object_number, object_integer,
                                  object_boolean, tx, origin_id, retracted, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
            params![
                new_subject,
                row.predicate.as_str(),
                new_object,
                row.object_value.as_deref(),
                row.object_datatype.as_deref(),
                row.object_language.as_deref(),
                row.object_type.as_str(),
                row.object_number,
                row.object_integer,
                row.object_boolean,
                tx_id,
                origin_id,
                now,
            ],
        ).await?;
    }

    Ok(())
}

/// Get the rowid of the last inserted row.
async fn last_insert_rowid(conn: &Connection) -> Result<i64> {
    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows.next().await?.ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
        "last_insert_rowid() returned no rows".into()
    })?;
    Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0))
}

/// Extract an optional text column value from a row.
fn opt_text(row: &turso::Row, idx: usize) -> Result<Option<String>> {
    Ok(match row.get_value(idx)? {
        turso::Value::Null => None,
        v => v.as_text().cloned(),
    })
}

/// Get current Unix time in milliseconds.
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_millis() as i64
}


#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
