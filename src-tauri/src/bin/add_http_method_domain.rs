/// Migration: ensures foundation:httpMethod has the three correct domains:
///   foundation:ConnectorEndpoint, foundation:ConnectorCapability, foundation:HTTPRequest.
///
/// Multi-value semantics in the IMMUTABLE STORE:
///   triples_current filters by the MAX(tx) of each (subject, predicate).
///   For multiple domains to coexist, all of them must be in the same TX (the highest).
///   This migration writes the three domains in a new TX — the previous ones are
///   automatically superseded by the new, higher TX. No retract, just append.
///
/// Canonical domains expected after the migration:
///   - foundation:ConnectorEndpoint  (pre-existing)
///   - foundation:ConnectorCapability (pre-existing)
///   - foundation:HTTPRequest         (new, US foundation:UserStory_1773785323638)
///
/// Usage:
///   cargo run --bin add_http_method_domain             # dry-run (default)
///   cargo run --bin add_http_method_domain -- --apply  # applies the change

use rusqlite::{Connection, Result, params};
use std::collections::HashSet;
use std::path::PathBuf;

const MIGRATION_ORIGIN: &str = "add_http_method_domain";
const PROP_IRI: &str = "foundation:httpMethod";

const EXPECTED_DOMAINS: &[&str] = &[
    "foundation:ConnectorEndpoint",
    "foundation:ConnectorCapability",
    "foundation:HTTPRequest",
];

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

/// Active domains (set from the highest TX) via triples_current.
fn current_domains(conn: &Connection, prop: &str) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT object FROM triples_current
         WHERE subject = ?1 AND predicate = 'rdfs:domain'",
    )?;
    let rows: Vec<Result<String>> = stmt
        .query_map(params![prop], |row| row.get(0))?
        .collect();
    rows.into_iter().collect::<Result<HashSet<_>>>()
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

    let current = current_domains(&conn, PROP_IRI)?;
    println!("Active domains of {}: {:?}", PROP_IRI, current);

    let expected: HashSet<&str> = EXPECTED_DOMAINS.iter().copied().collect();
    let current_strs: HashSet<&str> = current.iter().map(|s| s.as_str()).collect();

    if expected == current_strs {
        println!("Domain set is already correct — nothing to do.");
        return Ok(());
    }

    let missing: Vec<&&str> = expected.iter()
        .filter(|d| !current_strs.contains(*d))
        .collect();
    println!("Domains missing from the active set: {:?}", missing);
    println!(
        "A new TX will be written with the {} canonical domains: {:?}",
        EXPECTED_DOMAINS.len(),
        EXPECTED_DOMAINS
    );

    if !apply_mode {
        println!(
            "\n[DRY RUN] No changes made.\n\
             Run with --apply to apply."
        );
        return Ok(());
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;

    let tx = conn.transaction()?;

    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        params![MIGRATION_ORIGIN, now],
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(&tx, MIGRATION_ORIGIN)?;

    for domain in EXPECTED_DOMAINS {
        tx.execute(
            "INSERT INTO triples
               (subject, predicate, object, object_type,
                tx, origin_id, retracted, created_at)
             VALUES (?1, 'rdfs:domain', ?2, 'iri', ?3, ?4, 0, ?5)",
            params![PROP_IRI, domain, tx_id, origin_id, now],
        )?;
    }

    tx.commit()?;

    println!(
        "\nDone. {} domains written for {} (tx={}).",
        EXPECTED_DOMAINS.len(), PROP_IRI, tx_id
    );
    Ok(())
}
