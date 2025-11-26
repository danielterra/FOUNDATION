// ============================================================================
// Ontology Import Module
// ============================================================================
// Imports OWL/TTL ontologies into the SQLite EAVTO fact store
//
// Import strategy:
// - tx: 1-100   → RDF/RDFS/OWL meta-ontology (essential primitives)
// - tx: 101+    → Reserved for SuperNOVA Base Ontology (to be created)
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

/// Determines value type from RDF term (before string conversion)
fn determine_value_type_from_term(term: &Term) -> &'static str {
    match term {
        Term::NamedNode(_) | Term::BlankNode(_) => "ref",
        Term::Literal(lit) => {
            // Get the literal string representation and extract value
            let lit_str = lit.to_string();
            let value = if lit_str.starts_with('"') {
                if let Some(end_quote_idx) = lit_str[1..].find('"') {
                    &lit_str[1..1+end_quote_idx]
                } else {
                    &lit_str
                }
            } else {
                &lit_str
            };

            if value == "true" || value == "false" {
                "boolean"
            } else if value.parse::<i64>().is_ok() {
                "integer"
            } else if value.parse::<f64>().is_ok() {
                "number"
            } else {
                "string"
            }
        },
        Term::Triple(_) => "ref",
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
        Term::Literal(lit) => {
            // Get the literal value without quotes and language tags
            // Example: "entity"@en -> entity
            let lit_str = lit.to_string();
            if lit_str.starts_with('"') {
                if let Some(end_quote_idx) = lit_str[1..].find('"') {
                    return lit_str[1..1+end_quote_idx].to_string();
                }
            }
            lit_str
        },
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
        let v_type = determine_value_type_from_term(&triple.object);
        let object = term_to_string(&triple.object);

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

        // Check if fact already exists (avoid duplicates)
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM facts WHERE e = ? AND a = ? AND v = ? AND retracted = 0",
                rusqlite::params![&subject, &predicate, &object],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if exists {
            return Ok(()); // Skip duplicate
        }

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
        let v_type = determine_value_type_from_term(&triple.object);
        let object = term_to_string(&triple.object);

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

        // Check if fact already exists (avoid duplicates)
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM facts WHERE e = ? AND a = ? AND v = ? AND retracted = 0",
                rusqlite::params![&subject, &predicate, &object],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if exists {
            return Ok(()); // Skip duplicate
        }

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

    // Layer 1: RDF/RDFS/OWL meta-ontology (tx: 1-100)
    println!("📚 Importing RDF/RDFS/OWL meta-ontology...");
    let rdf_path = project_root.join("core-ontology/rdf-rdfs-owl-core.ttl");
    if rdf_path.exists() {
        let stats = import_turtle_file(conn, &rdf_path, 1, "core")?;
        all_stats.push(stats);
    } else {
        eprintln!("⚠️  Warning: {} not found", rdf_path.display());
    }

    // Post-processing: Add owl:Thing as explicit root for orphan classes
    println!("\n🔧 Post-processing: Adding owl:Thing as explicit root...");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    // Ensure owl:Thing exists as a class
    let owl_thing_exists: bool = conn
        .query_row(
            "SELECT 1 FROM facts WHERE e = 'http://www.w3.org/2002/07/owl#Thing' AND a = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type' AND v = 'http://www.w3.org/2002/07/owl#Class' AND retracted = 0",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if !owl_thing_exists {
        conn.execute(
            "INSERT INTO facts (e, a, v, v_number, v_integer, v_datetime, tx, origin, retracted, v_type, created_at)
             VALUES (?, ?, ?, NULL, NULL, NULL, ?, ?, 0, ?, ?)",
            rusqlite::params![
                "http://www.w3.org/2002/07/owl#Thing",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://www.w3.org/2002/07/owl#Class",
                1,  // tx=1 (meta-ontology layer)
                "core",
                "ref",
                now,
            ],
        )?;
        println!("  ✅ Added owl:Thing as owl:Class");
    }

    // Find all orphan classes (owl:Class without rdfs:subClassOf)
    let orphan_classes: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT e FROM facts
             WHERE a = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
             AND v = 'http://www.w3.org/2002/07/owl#Class'
             AND retracted = 0
             AND e != 'http://www.w3.org/2002/07/owl#Thing'
             AND e NOT IN (
                 SELECT DISTINCT e FROM facts
                 WHERE a = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
                 AND retracted = 0
             )"
        )?
        .query_map([], |row| row.get(0))?
        .filter_map(Result::ok)
        .collect();

    let orphan_count = orphan_classes.len();
    println!("  Found {} orphan classes", orphan_count);

    // Add rdfs:subClassOf owl:Thing for each orphan class
    for orphan_class in &orphan_classes {
        conn.execute(
            "INSERT INTO facts (e, a, v, v_number, v_integer, v_datetime, tx, origin, retracted, v_type, created_at)
             VALUES (?, ?, ?, NULL, NULL, NULL, ?, ?, 0, ?, ?)",
            rusqlite::params![
                orphan_class,
                "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "http://www.w3.org/2002/07/owl#Thing",
                1,  // tx=1 (meta-ontology layer)
                "core",
                "ref",
                now,
            ],
        )?;
    }

    if orphan_count > 0 {
        println!("  ✅ Added {} rdfs:subClassOf owl:Thing relationships", orphan_count);
    }

    // Mark as imported
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
