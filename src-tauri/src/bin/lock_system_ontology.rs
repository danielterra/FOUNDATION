/// Migration: set foundation:isSystemLocked = true on system ontology IRIs
/// that are explicitly referenced in the application source code.
///
/// An IRI qualifies when ALL of:
///   1. It starts with "foundation:"
///   2. It appears as a string literal in at least one .rs / .ts / .svelte file
///      under src/ or src-tauri/src/
///   3. It is defined in the database as an ontology entity
///      (rdf:type in {owl:Class, rdfs:Class, owl:ObjectProperty,
///                    owl:DatatypeProperty, owl:AnnotationProperty, rdf:Property})
///
/// Usage:
///   cargo run --bin lock_system_ontology             # dry-run (default)
///   cargo run --bin lock_system_ontology -- --apply  # write locks
///   cargo run --bin lock_system_ontology -- --revert # remove all locks written by this script

use rusqlite::{Connection, Result, params};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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

const MIGRATION_ORIGIN: &str = "lock_system_ontology";

/// Walk a directory tree and collect production source files with the given extensions.
/// Excludes test files: names ending in `_tests.rs`, named `mod_tests.rs`,
/// named `*_test.rs`, or inside a `tests/` directory.
fn collect_source_files(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if dir_name == "tests" {
                    continue;
                }
                files.extend(collect_source_files(&path, extensions));
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext) {
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if stem.ends_with("_tests") || stem.ends_with("_test") || stem == "mod_tests" {
                        continue;
                    }
                    files.push(path);
                }
            }
        }
    }
    files
}

/// Extract all `foundation:Xyz` tokens from a file's text.
fn extract_foundation_iris(text: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    let bytes = text.as_bytes();
    let prefix = b"foundation:";
    let mut i = 0;
    while i + prefix.len() < bytes.len() {
        if &bytes[i..i + prefix.len()] == prefix {
            let start = i + prefix.len();
            let end = bytes[start..]
                .iter()
                .position(|&b| !b.is_ascii_alphanumeric() && b != b'_')
                .map(|p| start + p)
                .unwrap_or(bytes.len());
            if end > start {
                if let Ok(local) = std::str::from_utf8(&bytes[start..end]) {
                    if local.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                        found.insert(format!("foundation:{}", local));
                    }
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    found
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let apply_mode = args.iter().any(|a| a == "--apply");
    let revert_mode = args.iter().any(|a| a == "--revert");

    let db_path = get_db_path();
    println!("Database: {}", db_path.display());
    if !db_path.exists() {
        eprintln!("Error: database not found at {}", db_path.display());
        std::process::exit(1);
    }

    let mut conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

    if revert_mode {
        let origin_id: Option<i64> = conn.query_row(
            "SELECT id FROM origins WHERE name = ?",
            [MIGRATION_ORIGIN],
            |row| row.get(0),
        ).ok();
        if let Some(oid) = origin_id {
            let deleted = conn.execute(
                "DELETE FROM triples WHERE predicate = 'foundation:isSystemLocked' AND origin_id = ?",
                params![oid],
            )?;
            conn.execute(
                "DELETE FROM transactions WHERE origin = ?",
                [MIGRATION_ORIGIN],
            )?;
            println!("Reverted {} isSystemLocked triples.", deleted);
        } else {
            println!("No locks from this migration found — nothing to revert.");
        }
        return Ok(());
    }

    // Locate the workspace root relative to this binary's manifest.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent()
        .expect("expected src-tauri parent")
        .to_path_buf();

    let src_dirs = [
        workspace_root.join("src"),
        workspace_root.join("src-tauri").join("src"),
    ];

    // Collect all foundation: IRIs referenced in source code.
    let mut code_iris: HashSet<String> = HashSet::new();
    let extensions = ["rs", "ts", "svelte"];
    let mut file_count = 0;
    for dir in &src_dirs {
        if dir.exists() {
            for file in collect_source_files(dir, &extensions) {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    code_iris.extend(extract_foundation_iris(&text));
                    file_count += 1;
                }
            }
        }
    }
    println!("Scanned {} source files, found {} unique foundation: IRIs in code.", file_count, code_iris.len());

    // Collect IRIs defined in the DB as ontology entities.
    let db_ontology_iris: HashSet<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT subject FROM triples
             WHERE predicate = 'rdf:type' AND retracted = 0",
        )?;
        let rows: Vec<Result<String>> = stmt.query_map([], |row| row.get(0))?.collect();
        let candidates = rows.into_iter().collect::<Result<Vec<_>>>()?;

        let mut defined = HashSet::new();
        for iri in candidates {
            if !iri.starts_with("foundation:") {
                continue;
            }
            let is_ontology_entity: bool = conn.query_row(
                "SELECT COUNT(*) FROM triples
                 WHERE subject = ?1 AND predicate = 'rdf:type' AND retracted = 0
                   AND object IN ('owl:Class','rdfs:Class','owl:ObjectProperty',
                                  'owl:DatatypeProperty','owl:AnnotationProperty','rdf:Property')",
                params![iri],
                |row| row.get::<_, i64>(0),
            ).map(|c| c > 0).unwrap_or(false);
            if is_ontology_entity {
                defined.insert(iri);
            }
        }
        defined
    };
    println!("DB ontology IRIs (classes + properties): {}", db_ontology_iris.len());

    // Intersection: referenced in code AND defined as ontology entity.
    let mut to_lock: Vec<String> = code_iris.intersection(&db_ontology_iris)
        .cloned()
        .collect();
    to_lock.sort();

    // Remove already-locked ones.
    let already_locked: HashSet<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT subject FROM triples
             WHERE predicate = 'foundation:isSystemLocked'
               AND object_boolean = 1
               AND retracted = 0",
        )?;
        let rows: Vec<Result<String>> = stmt.query_map([], |row| row.get(0))?.collect();
        rows.into_iter().collect::<Result<HashSet<_>>>()?
    };

    let pending: Vec<String> = to_lock.iter()
        .filter(|iri| !already_locked.contains(*iri))
        .cloned()
        .collect();

    println!(
        "To lock: {} (code ∩ ontology), {} already locked, {} pending",
        to_lock.len(), already_locked.len(), pending.len()
    );

    if pending.is_empty() {
        println!("Nothing to do.");
        return Ok(());
    }

    if !apply_mode {
        println!("\nDry-run. IRIs that would be locked:");
        for iri in &pending {
            println!("  {}", iri);
        }
        println!("\nRun with --apply to write the locks.");
        return Ok(());
    }

    let origin_id = get_or_create_origin(&conn, MIGRATION_ORIGIN)?;

    let tx = conn.transaction()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        params![MIGRATION_ORIGIN, now],
    )?;
    let tx_id = tx.last_insert_rowid();

    let mut locked = 0usize;
    for iri in &pending {
        tx.execute(
            "INSERT INTO triples
               (subject, predicate, object_value, object_datatype, object_type,
                object_boolean, retracted, origin_id, tx, created_at)
             VALUES
               (?1, 'foundation:isSystemLocked', 'true', 'xsd:boolean', 'literal',
                1, 0, ?2, ?3, ?4)",
            params![iri, origin_id, tx_id, now],
        )?;
        tx.execute(
            "UPDATE triples SET is_current = 0
             WHERE subject = ?1 AND predicate = 'foundation:isSystemLocked'
               AND is_current = 1 AND tx < ?2",
            params![iri, tx_id],
        )?;
        locked += 1;
    }
    tx.commit()?;

    println!("Locked {} system ontology IRIs.", locked);
    Ok(())
}
