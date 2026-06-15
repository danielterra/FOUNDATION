/// Migration: re-writes foundation:retryCount triples whose datatype is not
/// xsd:integer (legacy string "0") as proper xsd:integer literals.
///
/// The numeric value is preserved — only object_datatype and object_type are
/// corrected.  The immutable store contract is honoured: a new triple is
/// inserted with a higher tx, and the superseded triple is marked
/// is_current = 0.  No rows are deleted.
///
/// Usage:
///   cargo run --bin migrate_retry_count             # dry-run (default)
///   cargo run --bin migrate_retry_count -- --apply  # applies the changes

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

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let apply_mode = std::env::args().any(|a| a == "--apply");

    let db_path = get_db_path();
    println!("Database: {}", db_path.display());
    if !db_path.exists() {
        eprintln!("Error: database not found at {}", db_path.display());
        std::process::exit(1);
    }

    let mut conn = Connection::open(&db_path)?;
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

    // Candidates: current retryCount triples whose datatype is not xsd:integer.
    // "Current" = highest tx for (subject, predicate) and is_current = 1.
    let candidates: Vec<(String, String)> = {
        let mut stmt = conn.prepare("
            SELECT t.subject, t.object_value
            FROM triples t
            WHERE t.predicate = 'foundation:retryCount'
              AND t.is_current = 1
              AND t.retracted = 0
              AND t.tx = (
                  SELECT MAX(tx) FROM triples
                  WHERE subject = t.subject AND predicate = 'foundation:retryCount'
              )
              AND (t.object_datatype IS NULL OR t.object_datatype != 'xsd:integer')
            ORDER BY t.subject
        ")?;
        let rows: Vec<Result<(String, String)>> =
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect();
        rows.into_iter().collect::<Result<Vec<_>>>()?
    };

    if candidates.is_empty() {
        println!("No retryCount triples to migrate.");
        return Ok(());
    }

    println!("\n{} triple(s) to migrate:\n", candidates.len());
    for (iri, value) in &candidates {
        println!("  {} | retryCount=\"{}\" (string) → {} (integer)", iri, value, value);
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
        params!["migrate_retry_count", now],
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(&tx, "migrate_retry_count")?;

    let mut migrated = 0usize;
    for (iri, value) in &candidates {
        // Parse the string value to i64 to ensure a clean integer literal.
        let int_value: i64 = value.trim().parse().unwrap_or(0);

        // object_integer must be set for xsd:integer (CHECK constraint in schema).
        // object_value holds the canonical lexical form required by the triple store.
        tx.execute(
            "INSERT INTO triples
               (subject, predicate, object_value, object_datatype, object_type,
                object_integer, tx, origin_id, retracted, is_current, created_at)
             VALUES (?1, 'foundation:retryCount', ?2, 'xsd:integer', 'literal',
                     ?3, ?4, ?5, 0, 1, ?6)",
            params![iri, int_value.to_string(), int_value, tx_id, origin_id, now],
        )?;
        tx.execute(
            "UPDATE triples SET is_current = 0
             WHERE subject = ?1 AND predicate = 'foundation:retryCount'
               AND is_current = 1 AND tx < ?2",
            params![iri, tx_id],
        )?;
        migrated += 1;
    }

    tx.commit()?;

    println!("\nDone. {} triple(s) migrated.", migrated);
    Ok(())
}
