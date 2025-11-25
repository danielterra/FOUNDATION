// ============================================================================
// Ontology Import Module
// ============================================================================
// Imports OWL/TTL ontologies into the SQLite EAVTO fact store
//
// Import strategy (aligned with P3.md):
// - tx: 1-100      → RDF/RDFS/OWL meta-ontology
// - tx: 101-10000  → BFO (Basic Formal Ontology)
// - tx: 10001+     → CCO (Common Core Ontologies)
//
// All imported facts have origin: "core" and retracted: 0
// ============================================================================

use rusqlite::Connection;
use rio_turtle::{TurtleParser, TurtleError};
use rio_xml::{RdfXmlParser, RdfXmlError};
use rio_api::parser::TriplesParser;
use rio_api::model::{Term, Triple};
use std::path::Path;
use std::io::BufReader;
use std::fs::File;

/// Import error types
#[derive(Debug)]
pub enum ImportError {
    IoError(std::io::Error),
    TurtleError(TurtleError),
    XmlError(RdfXmlError),
    DatabaseError(rusqlite::Error),
}

impl From<std::io::Error> for ImportError {
    fn from(err: std::io::Error) -> Self {
        ImportError::IoError(err)
    }
}

impl From<TurtleError> for ImportError {
    fn from(err: TurtleError) -> Self {
        ImportError::TurtleError(err)
    }
}

impl From<RdfXmlError> for ImportError {
    fn from(err: RdfXmlError) -> Self {
        ImportError::XmlError(err)
    }
}

impl From<rusqlite::Error> for ImportError {
    fn from(err: rusqlite::Error) -> Self {
        ImportError::DatabaseError(err)
    }
}

/// Import statistics
#[derive(Debug, serde::Serialize)]
pub struct ImportStats {
    pub file: String,
    pub format: String,
    pub triples_processed: u64,
    pub facts_inserted: u64,
    pub tx_start: i64,
    pub tx_end: i64,
}

/// Determines value type from RDF term
fn determine_value_type(term_str: &str) -> &'static str {
    // Check if it's a URI reference
    if term_str.starts_with("http://") || term_str.starts_with("https://") || term_str.contains("://") {
        "ref"
    } else if term_str == "true" || term_str == "false" {
        "boolean"
    } else if term_str.parse::<i64>().is_ok() {
        "integer"
    } else if term_str.parse::<f64>().is_ok() {
        "number"
    } else {
        "string"
    }
}

/// Converts RIO Subject to string representation
fn subject_to_string(subject: &rio_api::model::Subject) -> String {
    match subject {
        rio_api::model::Subject::NamedNode(node) => node.iri.to_string(),
        rio_api::model::Subject::BlankNode(bn) => format!("_:{}", bn.id),
        rio_api::model::Subject::Triple(_) => "_:triple".to_string(), // RDF-star quoted triples
    }
}

/// Converts RIO Term to string representation
fn term_to_string(term: &Term) -> String {
    match term {
        Term::NamedNode(node) => node.iri.to_string(),
        Term::BlankNode(bn) => format!("_:{}", bn.id),
        Term::Literal(lit) => lit.to_string(),
        Term::Triple(_) => "_:triple".to_string(), // RDF-star quoted triples
    }
}

/// Import RDF triples from Turtle file
pub fn import_turtle_file(
    conn: &Connection,
    file_path: &Path,
    tx_start: i64,
    origin: &str,
) -> Result<ImportStats, ImportError> {
    let filename = file_path.file_name().unwrap().to_string_lossy().to_string();
    println!("Importing Turtle file: {}", filename);

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut triples_processed = 0u64;
    let mut facts_inserted = 0u64;
    let current_tx = tx_start;

    // Begin transaction
    conn.execute("BEGIN TRANSACTION", [])?;

    // Create transaction record
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    conn.execute(
        "INSERT INTO transactions (tx, origin, created_at) VALUES (?, ?, ?)",
        (current_tx, origin, now),
    )?;

    // Parse Turtle file
    TurtleParser::new(reader, None).parse_all(&mut |triple: Triple| {
        triples_processed += 1;

        let subject = subject_to_string(&triple.subject);
        let predicate = triple.predicate.iri.to_string();
        let object = term_to_string(&triple.object);

        let v_type = determine_value_type(&object);

        let v_number: Option<f64> = if v_type == "number" {
            object.parse().ok()
        } else {
            None
        };

        let v_integer: Option<i64> = if v_type == "integer" {
            object.parse().ok()
        } else {
            None
        };

        // Insert fact
        if let Err(e) = conn.execute(
            "INSERT INTO facts (e, a, v, v_number, v_integer, v_datetime, tx, origin, retracted, v_type, created_at)
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, 0, ?, ?)",
            rusqlite::params![
                &subject,
                &predicate,
                &object,
                v_number,
                v_integer,
                current_tx,
                origin,
                v_type,
                now,
            ],
        ) {
            eprintln!("Warning: Failed to insert fact: {}", e);
            return Ok(());
        }

        facts_inserted += 1;

        if triples_processed % 1000 == 0 {
            println!("  Processed {} triples...", triples_processed);
        }

        Ok(()) as Result<(), TurtleError>
    })?;

    // Commit transaction
    conn.execute("COMMIT", [])?;

    println!(
        "✅ Imported {} triples ({} facts) from {}",
        triples_processed, facts_inserted, filename
    );

    Ok(ImportStats {
        file: filename,
        format: "Turtle".to_string(),
        triples_processed,
        facts_inserted,
        tx_start: current_tx,
        tx_end: current_tx,
    })
}

/// Import RDF triples from OWL/XML file
pub fn import_owl_file(
    conn: &Connection,
    file_path: &Path,
    tx_start: i64,
    origin: &str,
) -> Result<ImportStats, ImportError> {
    let filename = file_path.file_name().unwrap().to_string_lossy().to_string();
    println!("Importing OWL/XML file: {}", filename);

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut triples_processed = 0u64;
    let mut facts_inserted = 0u64;
    let current_tx = tx_start;

    // Begin transaction
    conn.execute("BEGIN TRANSACTION", [])?;

    // Create transaction record
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    conn.execute(
        "INSERT INTO transactions (tx, origin, created_at) VALUES (?, ?, ?)",
        (current_tx, origin, now),
    )?;

    // Parse RDF/XML file
    RdfXmlParser::new(reader, None).parse_all(&mut |triple: Triple| {
        triples_processed += 1;

        let subject = subject_to_string(&triple.subject);
        let predicate = triple.predicate.iri.to_string();
        let object = term_to_string(&triple.object);

        let v_type = determine_value_type(&object);

        let v_number: Option<f64> = if v_type == "number" {
            object.parse().ok()
        } else {
            None
        };

        let v_integer: Option<i64> = if v_type == "integer" {
            object.parse().ok()
        } else {
            None
        };

        // Insert fact
        if let Err(e) = conn.execute(
            "INSERT INTO facts (e, a, v, v_number, v_integer, v_datetime, tx, origin, retracted, v_type, created_at)
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, 0, ?, ?)",
            rusqlite::params![
                &subject,
                &predicate,
                &object,
                v_number,
                v_integer,
                current_tx,
                origin,
                v_type,
                now,
            ],
        ) {
            eprintln!("Warning: Failed to insert fact: {}", e);
            return Ok(());
        }

        facts_inserted += 1;

        if triples_processed % 1000 == 0 {
            println!("  Processed {} triples...", triples_processed);
        }

        Ok(()) as Result<(), RdfXmlError>
    })?;

    // Commit transaction
    conn.execute("COMMIT", [])?;

    println!(
        "✅ Imported {} triples ({} facts) from {}",
        triples_processed, facts_inserted, filename
    );

    Ok(ImportStats {
        file: filename,
        format: "OWL/XML".to_string(),
        triples_processed,
        facts_inserted,
        tx_start: current_tx,
        tx_end: current_tx,
    })
}

/// Import all core ontologies in the correct order
pub fn import_all_core_ontologies(conn: &Connection) -> Result<Vec<ImportStats>, ImportError> {
    let mut all_stats = Vec::new();

    println!("\n🔄 Starting core ontology import...\n");

    // Check if already imported
    let already_imported: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'ontology_imported'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "false".to_string());

    if already_imported == "true" {
        println!("⚠️  Core ontologies already imported. Skipping.");
        return Ok(all_stats);
    }

    // Get project root directory
    // In dev mode, CARGO_MANIFEST_DIR points to src-tauri, so we go up one level
    let project_root = std::env::var("CARGO_MANIFEST_DIR")
        .ok()
        .and_then(|manifest_dir| {
            let path = Path::new(&manifest_dir);
            path.parent().map(|p| p.to_path_buf())
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    println!("📂 Project root: {}", project_root.display());

    // Layer 1: RDF/RDFS/OWL (tx: 1-100)
    println!("📚 Layer 1: Importing RDF/RDFS/OWL meta-ontology...");
    let rdf_path = project_root.join("core-ontology/rdf-rdfs-owl-core.ttl");
    if rdf_path.exists() {
        let stats = import_turtle_file(conn, &rdf_path, 1, "core")?;
        all_stats.push(stats);
    } else {
        eprintln!("⚠️  Warning: {} not found", rdf_path.display());
    }

    // Layer 2: BFO (tx: 101-10000)
    println!("\n📚 Layer 2: Importing BFO (Basic Formal Ontology)...");
    let bfo_path = project_root.join("core-ontology/bfo.owl");
    if bfo_path.exists() {
        let stats = import_owl_file(conn, &bfo_path, 101, "core")?;
        all_stats.push(stats);
    } else {
        eprintln!("⚠️  Warning: {} not found", bfo_path.display());
    }

    // Layer 3: CCO (tx: 10001+)
    println!("\n📚 Layer 3: Importing Common Core Ontologies...");
    let cco_merged_path = project_root.join("core-ontology/CommonCoreOntologies-2.0-2024-11-06/src/cco-merged/CommonCoreOntologiesMerged.ttl");
    if cco_merged_path.exists() {
        let stats = import_turtle_file(conn, &cco_merged_path, 10001, "core")?;
        all_stats.push(stats);
    } else {
        eprintln!("⚠️  Warning: {} not found", cco_merged_path.display());
    }

    // Mark as imported
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    conn.execute(
        "UPDATE metadata SET value = 'true', updated_at = ? WHERE key = 'ontology_imported'",
        (now,),
    )?;

    println!("\n✅ Core ontology import complete!");
    println!("📊 Summary:");
    for stats in &all_stats {
        println!(
            "  - {}: {} triples → {} facts (tx: {})",
            stats.file, stats.triples_processed, stats.facts_inserted, stats.tx_start
        );
    }

    Ok(all_stats)
}
