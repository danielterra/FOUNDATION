/// Migration: copies foundation:dueDate → foundation:scheduledAt for tasks.
///
/// Rule: if a foundation:Task has a dueDate but no scheduledAt,
/// copy the value with a noon time (12:00:00+00:00) as the default.
/// Tasks that already have scheduledAt are ignored.
///
/// Usage:
///   cargo run --bin migrate_due_date             # dry-run (default)
///   cargo run --bin migrate_due_date -- --apply  # applies the changes

use rusqlite::{Connection, Result, params};
use std::path::PathBuf;

fn get_db_path() -> PathBuf {
    dirs::document_dir()
        .expect("Could not find Documents directory")
        .join("Foundation")
        .join("FOUNDATION.db")
}

fn get_or_create_origin(conn: &Connection, name: &str) -> Result<i64> {
    match conn.query_row("SELECT id FROM origins WHERE name = ?", [name], |row| row.get(0)) {
        Ok(id) => Ok(id),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            conn.execute("INSERT INTO origins (name) VALUES (?)", [name])?;
            Ok(conn.last_insert_rowid())
        }
        Err(e) => Err(e),
    }
}

/// Converts "2026-04-08" → "2026-04-08T12:00:00+00:00".
/// If the value already has a 'T' (already a datetime), returns it as is.
fn to_noon_datetime(date_str: &str) -> String {
    if date_str.contains('T') {
        date_str.to_string()
    } else {
        format!("{}T12:00:00+00:00", date_str)
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let apply_mode = std::env::args().any(|a| a == "--apply");

    let db_path = get_db_path();
    println!("Database: {}", db_path.display());
    if !db_path.exists() {
        eprintln!("Error: database not found at {}", db_path.display());
        std::process::exit(1);
    }

    let mut conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

    // Tasks with dueDate but no scheduledAt (latest dueDate value, ignoring retracted).
    let candidates: Vec<(String, String)> = {
        let mut stmt = conn.prepare("
            SELECT t.subject, t.object_value
            FROM triples t
            WHERE t.predicate = 'foundation:dueDate'
              AND t.retracted = 0
              AND t.tx = (
                  SELECT MAX(tx) FROM triples
                  WHERE subject = t.subject AND predicate = 'foundation:dueDate'
              )
              AND EXISTS (
                  SELECT 1 FROM triples t2
                  WHERE t2.subject = t.subject
                    AND t2.predicate = 'rdf:type'
                    AND t2.object = 'foundation:Task'
                    AND t2.retracted = 0
              )
              AND NOT EXISTS (
                  SELECT 1 FROM triples t3
                  WHERE t3.subject = t.subject
                    AND t3.predicate = 'foundation:scheduledAt'
                    AND t3.retracted = 0
              )
            ORDER BY t.subject
        ")?;
        let rows: Vec<Result<(String, String)>> =
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect();
        rows.into_iter().collect::<Result<Vec<_>>>()?
    };

    if candidates.is_empty() {
        println!("No tasks to migrate.");
        return Ok(());
    }

    println!("\n{} task(s) to migrate:\n", candidates.len());
    for (iri, due_date) in &candidates {
        let scheduled_at = to_noon_datetime(due_date);
        println!("  {} | dueDate={} → scheduledAt={}", iri, due_date, scheduled_at);
    }

    if !apply_mode {
        println!(
            "\n[DRY RUN] No changes made.\n\
             Run with --apply to apply the migration."
        );
        return Ok(());
    }

    println!("\nApplying migration...");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;

    let tx = conn.transaction()?;

    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        params!["migrate_due_date", now],
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(&tx, "migrate_due_date")?;

    let mut migrated = 0usize;
    for (iri, due_date) in &candidates {
        let scheduled_at = to_noon_datetime(due_date);
        tx.execute(
            "INSERT INTO triples
               (subject, predicate, object_value, object_datatype, object_type,
                tx, origin_id, retracted, created_at)
             VALUES (?1, 'foundation:scheduledAt', ?2, 'xsd:dateTime', 'literal',
                     ?3, ?4, 0, ?5)",
            params![iri, scheduled_at, tx_id, origin_id, now],
        )?;
        tx.execute(
            "UPDATE triples SET is_current = 0
             WHERE subject = ?1 AND predicate = 'foundation:scheduledAt'
               AND is_current = 1 AND tx < ?2",
            params![iri, tx_id],
        )?;
        migrated += 1;
    }

    tx.commit()?;

    println!("\nDone. {} task(s) migrated.", migrated);
    Ok(())
}
