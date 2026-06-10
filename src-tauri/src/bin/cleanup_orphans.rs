/// Cleanup script for orphan instances.
///
/// An orphan instance is one whose rdf:type class no longer exists in the ontology
/// (the class IRI has no active `rdf:type owl:Class` or `rdf:type rdfs:Class` triple).
///
/// Usage:
///   cargo run --bin cleanup_orphans             # dry-run (default)
///   cargo run --bin cleanup_orphans -- --delete  # retract orphan triples

use rusqlite::{Connection, Result, params};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn get_db_path() -> PathBuf {
    dirs::document_dir()
        .expect("Could not find Documents directory")
        .join("Foundation")
        .join("FOUNDATION.db")
}

/// Type IRIs from standard vocabularies that exist implicitly without owl:Class declarations.
const BUILTIN_TYPES: &[&str] = &[
    "owl:Class",
    "rdfs:Class",
    "owl:Thing",
    "rdfs:Resource",
    "owl:NamedIndividual",
    "owl:ObjectProperty",
    "owl:DatatypeProperty",
    "owl:AnnotationProperty",
    "owl:TransitiveProperty",
    "owl:SymmetricProperty",
    "owl:FunctionalProperty",
    "owl:InverseFunctionalProperty",
    "rdf:Property",
    "owl:Ontology",
    "owl:Restriction",
    "owl:AllDifferent",
    "owl:AllDisjointClasses",
];

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
    let delete_mode = std::env::args().any(|a| a == "--delete");

    let db_path = get_db_path();
    println!("Database: {}", db_path.display());
    if !db_path.exists() {
        eprintln!("Error: database not found at {}", db_path.display());
        std::process::exit(1);
    }

    let mut conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

    let builtin_set: HashSet<&str> = BUILTIN_TYPES.iter().copied().collect();

    // All class IRIs that currently exist in the ontology.
    let live_classes: HashSet<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT subject FROM triples
             WHERE predicate = 'rdf:type'
               AND object IN ('owl:Class', 'rdfs:Class')
               AND retracted = 0",
        )?;
        let rows: Vec<Result<String>> = stmt.query_map([], |row| row.get(0))?.collect();
        rows.into_iter().collect::<Result<HashSet<_>>>()?
    };
    println!("Live classes in ontology: {}", live_classes.len());

    // All (subject, type) pairs for non-retracted instances.
    let instance_types: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT subject, object FROM triples
             WHERE predicate = 'rdf:type'
               AND object IS NOT NULL
               AND retracted = 0",
        )?;
        let rows: Vec<Result<(String, String)>> =
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect();
        rows.into_iter().collect::<Result<Vec<_>>>()?
    };

    // Group types by subject.
    let mut subject_types: HashMap<String, Vec<String>> = HashMap::new();
    for (subject, type_iri) in &instance_types {
        subject_types.entry(subject.clone()).or_default().push(type_iri.clone());
    }

    // An instance is orphan when it has NO valid rdf:type — neither a builtin
    // vocabulary type nor a class that currently exists in the ontology.
    let mut orphans: Vec<(String, Vec<String>)> = Vec::new();
    for (subject, types) in &subject_types {
        let has_valid_type = types.iter()
            .any(|t| builtin_set.contains(t.as_str()) || live_classes.contains(t));

        if has_valid_type {
            continue;
        }

        orphans.push((subject.clone(), types.clone()));
    }

    if orphans.is_empty() {
        println!("\nNo orphan instances found.");
        return Ok(());
    }

    // Group orphans by class for the report.
    let mut by_class: HashMap<String, Vec<String>> = HashMap::new();
    for (subject, types) in &orphans {
        for t in types {
            by_class.entry(t.clone()).or_default().push(subject.clone());
        }
    }

    println!("\nOrphan instances found: {} total\n", orphans.len());
    let mut sorted_classes: Vec<&String> = by_class.keys().collect();
    sorted_classes.sort();
    for class in &sorted_classes {
        let subjects = &by_class[*class];
        println!("  {} ({} instances)", class, subjects.len());
        for s in subjects.iter().take(5) {
            println!("    - {}", s);
        }
        if subjects.len() > 5 {
            println!("    ... and {} more", subjects.len() - 5);
        }
    }

    if !delete_mode {
        println!(
            "\n[DRY RUN] No changes made.\n\
             Run with --delete to retract all triples for these orphan instances."
        );
        return Ok(());
    }

    println!("\nRetracting orphan instances...");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;

    let tx = conn.transaction()?;

    tx.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        params!["cleanup_orphans", now],
    )?;
    let tx_id = tx.last_insert_rowid();
    let origin_id = get_or_create_origin(&tx, "cleanup_orphans")?;

    let mut total_retracted: usize = 0;
    for (subject, _) in &orphans {
        // Immutable model: insert one retracted tombstone per predicate (latest-tx row as template)
        // so that the new highest tx for each (subject, predicate) is a tombstone.
        let n = tx.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_datatype,
                                  object_language, object_type, object_number, object_integer,
                                  object_boolean, tx, origin_id, retracted, created_at)
             SELECT subject, predicate, object, object_value, object_datatype,
                    object_language, object_type, object_number, object_integer,
                    object_boolean, ?1, ?2, 1, ?3
             FROM triples t
             WHERE t.subject = ?4 AND t.is_current = 1
             GROUP BY t.predicate",
            params![tx_id, origin_id, now, subject],
        )?;
        tx.execute(
            "UPDATE triples SET is_current = 0
             WHERE subject = ?1 AND is_current = 1 AND tx < ?2",
            params![subject, tx_id],
        )?;
        total_retracted += n;
    }

    // Report dangling references (inbound links) but do not auto-retract them.
    let mut dangling: usize = 0;
    let orphan_subjects: Vec<&str> = orphans.iter().map(|(s, _)| s.as_str()).collect();
    for subject in &orphan_subjects {
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM triples WHERE object = ? AND retracted = 0",
            [subject],
            |row| row.get(0),
        )?;
        dangling += count as usize;
    }

    tx.commit()?;

    println!(
        "\nDone. Retracted {} triples across {} orphan instances.",
        total_retracted,
        orphans.len()
    );
    if dangling > 0 {
        println!(
            "Warning: {} dangling reference(s) still point to removed instances. \
             Run again to inspect or clean them manually.",
            dangling
        );
    }

    Ok(())
}
