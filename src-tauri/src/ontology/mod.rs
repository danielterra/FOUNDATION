// ============================================================================
// Ontology Import Module
// ============================================================================
// Imports OWL/TTL ontologies into the SQLite EAVTO fact store
//
// Import strategy:
// - tx: 1-100   → RDF/RDFS/OWL meta-ontology (essential primitives)
// - tx: 101+    → Reserved for FOUNDATION Base Ontology (to be created)
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

/// Determines object_type from RDF term
fn determine_object_type_from_term(term: &Term) -> &'static str {
    match term {
        Term::NamedNode(_) => "iri",
        Term::BlankNode(_) => "blank",
        Term::Literal(_) => "literal",
        Term::Triple(_) => "blank", // RDF-star quoted triples treated as blank nodes
    }
}

/// Extracts datatype IRI from a literal term and compresses it
fn get_literal_datatype(lit: &rio_api::model::Literal) -> String {
    let full_iri = match lit {
        rio_api::model::Literal::Simple { .. } => "http://www.w3.org/2001/XMLSchema#string",
        rio_api::model::Literal::LanguageTaggedString { .. } => "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString",
        rio_api::model::Literal::Typed { datatype, .. } => datatype.iri,
    };
    crate::namespaces::compress_iri(full_iri)
}

/// Extracts language tag from a literal term (if any)
fn get_literal_language(lit: &rio_api::model::Literal) -> Option<String> {
    match lit {
        rio_api::model::Literal::LanguageTaggedString { language, .. } => Some(language.to_string()),
        _ => None,
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

/// Extracts literal value (lexical form) from a literal term
fn get_literal_value(lit: &rio_api::model::Literal) -> String {
    match lit {
        rio_api::model::Literal::Simple { value } => value.to_string(),
        rio_api::model::Literal::LanguageTaggedString { value, .. } => value.to_string(),
        rio_api::model::Literal::Typed { value, .. } => value.to_string(),
    }
}

/// Get or create origin ID for a given origin name
fn get_or_create_origin(conn: &Connection, origin_name: &str) -> Result<i64, ImportError> {
    // Try to get existing origin
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM origins WHERE name = ?",
            [origin_name],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        return Ok(id);
    }

    // Create new origin
    conn.execute(
        "INSERT INTO origins (name, description) VALUES (?, ?)",
        (origin_name, format!("Auto-generated origin for {}", origin_name)),
    )?;

    // Get the ID of the newly inserted origin
    let id: i64 = conn.query_row(
        "SELECT id FROM origins WHERE name = ?",
        [origin_name],
        |row| row.get(0),
    )?;

    Ok(id)
}

/// Parses typed columns from literal value and datatype
fn parse_typed_columns(value: &str, datatype: &str) -> (Option<f64>, Option<i64>, Option<i64>, Option<i64>) {
    let mut object_number: Option<f64> = None;
    let mut object_integer: Option<i64> = None;
    let mut object_datetime: Option<i64> = None;
    let mut object_boolean: Option<i64> = None;

    match datatype {
        "http://www.w3.org/2001/XMLSchema#decimal" |
        "http://www.w3.org/2001/XMLSchema#double" |
        "http://www.w3.org/2001/XMLSchema#float" => {
            object_number = value.parse().ok();
        }
        "http://www.w3.org/2001/XMLSchema#integer" |
        "http://www.w3.org/2001/XMLSchema#int" |
        "http://www.w3.org/2001/XMLSchema#long" => {
            object_integer = value.parse().ok();
        }
        "http://www.w3.org/2001/XMLSchema#dateTime" => {
            // Parse ISO 8601 datetime to Unix epoch milliseconds
            // For now, we'll leave this as None and let the application handle it
            object_datetime = None;
        }
        "http://www.w3.org/2001/XMLSchema#boolean" => {
            object_boolean = match value {
                "true" | "1" => Some(1),
                "false" | "0" => Some(0),
                _ => None,
            };
        }
        _ => {}
    }

    (object_number, object_integer, object_datetime, object_boolean)
}

/// Import RDF triples from Turtle file
pub fn import_turtle_file(
    conn: &Connection,
    file_path: &Path,
    origin: &str,
) -> Result<ImportStats, ImportError> {
    let filename = file_path.file_name().unwrap().to_string_lossy().to_string();
    println!("Importing Turtle file: {}", filename);

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut triples_processed = 0u64;
    let mut facts_inserted = 0u64;

    // Get or create origin_id
    let origin_id = get_or_create_origin(conn, origin)?;

    // Begin transaction
    conn.execute("BEGIN TRANSACTION", [])?;

    // Create transaction record (let AUTOINCREMENT generate tx id)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES (?, ?)",
        (origin, now),
    )?;

    // Get the auto-generated tx id
    let current_tx = conn.last_insert_rowid();

    // Parse Turtle file
    TurtleParser::new(reader, None).parse_all(&mut |triple: Triple| {
        triples_processed += 1;

        // Compress IRIs to prefix notation for storage
        let subject_full = subject_to_string(&triple.subject);
        let subject = crate::namespaces::compress_iri(&subject_full);
        let predicate = crate::namespaces::compress_iri(triple.predicate.iri);
        let object_type = determine_object_type_from_term(&triple.object);

        // Extract object components based on type
        let (object, object_value, object_datatype, object_language, object_number, object_integer, object_datetime, object_boolean) = match &triple.object {
            Term::NamedNode(node) => {
                // IRI object - compress to prefix notation
                let compressed_iri = crate::namespaces::compress_iri(node.iri);
                (Some(compressed_iri), None, None, None, None, None, None, None)
            }
            Term::BlankNode(bn) => {
                // Blank node object
                (Some(format!("_:{}", bn.id)), None, None, None, None, None, None, None)
            }
            Term::Literal(lit) => {
                // Literal object
                let value = get_literal_value(lit);
                let datatype = get_literal_datatype(lit);
                let language = get_literal_language(lit);
                let (obj_num, obj_int, obj_dt, obj_bool) = parse_typed_columns(&value, &datatype);
                (None, Some(value), Some(datatype), language, obj_num, obj_int, obj_dt, obj_bool)
            }
            Term::Triple(_) => {
                // RDF-star quoted triple (treat as blank node)
                (Some("_:triple".to_string()), None, None, None, None, None, None, None)
            }
        };

        // Check if triple already exists (avoid duplicates)
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM triples WHERE subject = ? AND predicate = ? AND COALESCE(object, '') = ? AND COALESCE(object_value, '') = ? AND retracted = 0",
                rusqlite::params![&subject, &predicate, &object.as_ref().unwrap_or(&String::new()), &object_value.as_ref().unwrap_or(&String::new())],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if exists {
            return Ok(()); // Skip duplicate
        }

        // Insert triple
        if let Err(e) = conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_type, object_datatype, object_language, object_number, object_integer, object_datetime, object_boolean, tx, origin_id, retracted, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
            rusqlite::params![
                &subject,
                &predicate,
                object,
                object_value,
                object_type,
                object_datatype,
                object_language,
                object_number,
                object_integer,
                object_datetime,
                object_boolean,
                current_tx,
                origin_id,
                now,
            ],
        ) {
            eprintln!("Warning: Failed to insert triple: {}", e);
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

    // Get or create origin_id
    let origin_id = get_or_create_origin(conn, origin)?;

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

        // Compress IRIs to prefix notation for storage
        let subject_full = subject_to_string(&triple.subject);
        let subject = crate::namespaces::compress_iri(&subject_full);
        let predicate = crate::namespaces::compress_iri(triple.predicate.iri);
        let object_type = determine_object_type_from_term(&triple.object);

        // Extract object components based on type
        let (object, object_value, object_datatype, object_language, object_number, object_integer, object_datetime, object_boolean) = match &triple.object {
            Term::NamedNode(node) => {
                // IRI object - compress to prefix notation
                let compressed_iri = crate::namespaces::compress_iri(node.iri);
                (Some(compressed_iri), None, None, None, None, None, None, None)
            }
            Term::BlankNode(bn) => {
                // Blank node object
                (Some(format!("_:{}", bn.id)), None, None, None, None, None, None, None)
            }
            Term::Literal(lit) => {
                // Literal object
                let value = get_literal_value(lit);
                let datatype = get_literal_datatype(lit);
                let language = get_literal_language(lit);
                let (obj_num, obj_int, obj_dt, obj_bool) = parse_typed_columns(&value, &datatype);
                (None, Some(value), Some(datatype), language, obj_num, obj_int, obj_dt, obj_bool)
            }
            Term::Triple(_) => {
                // RDF-star quoted triple (treat as blank node)
                (Some("_:triple".to_string()), None, None, None, None, None, None, None)
            }
        };

        // Check if triple already exists (avoid duplicates)
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM triples WHERE subject = ? AND predicate = ? AND COALESCE(object, '') = ? AND COALESCE(object_value, '') = ? AND retracted = 0",
                rusqlite::params![&subject, &predicate, &object.as_ref().unwrap_or(&String::new()), &object_value.as_ref().unwrap_or(&String::new())],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if exists {
            return Ok(()); // Skip duplicate
        }

        // Insert triple
        if let Err(e) = conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_type, object_datatype, object_language, object_number, object_integer, object_datetime, object_boolean, tx, origin_id, retracted, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
            rusqlite::params![
                &subject,
                &predicate,
                object,
                object_value,
                object_type,
                object_datatype,
                object_language,
                object_number,
                object_integer,
                object_datetime,
                object_boolean,
                current_tx,
                origin_id,
                now,
            ],
        ) {
            eprintln!("Warning: Failed to insert triple: {}", e);
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

/// Import all FOUNDATION ontologies from filesystem
pub fn import_all_foundation_ontologies(conn: &Connection) -> Result<Vec<ImportStats>, ImportError> {
    let mut all_stats = Vec::new();

    println!("\n🏛️  Importing FOUNDATION ontologies...\n");

    // Get project root directory
    // In dev mode, CARGO_MANIFEST_DIR points to src-tauri, so we go up one level
    let project_root = std::env::var("CARGO_MANIFEST_DIR")
        .ok()
        .and_then(|manifest_dir| {
            let path = Path::new(&manifest_dir);
            path.parent().map(|p| p.to_path_buf())
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let core_ontology_dir = project_root.join("core-ontology");
    println!("📂 Reading from: {}", core_ontology_dir.display());

    // Read all .ttl files from core-ontology directory
    let mut ttl_files: Vec<std::path::PathBuf> = Vec::new();

    match std::fs::read_dir(&core_ontology_dir) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("ttl") {
                        // Skip rdf-rdfs-owl-core.ttl as it's imported separately
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            if filename != "rdf-rdfs-owl-core.ttl" {
                                ttl_files.push(path);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("⚠️  Error reading core-ontology directory: {}", e);
            return Err(ImportError::IoError(e));
        }
    }

    // Sort files for consistent import order
    ttl_files.sort();

    println!("📋 Found {} FOUNDATION ontology files\n", ttl_files.len());

    // Import each file with file-specific origin
    for file_path in ttl_files {
        let filename = file_path.file_name().unwrap().to_str().unwrap();
        let origin = format!("foundation:ontology:{}", filename);

        println!("📄 {}", filename);
        match import_turtle_file(conn, &file_path, &origin) {
            Ok(stats) => {
                all_stats.push(stats);
            }
            Err(e) => {
                eprintln!("⚠️  Failed to import {}: {:?}", filename, e);
            }
        }
    }

    println!("\n✅ FOUNDATION ontology import complete!");
    println!("📊 Summary:");
    for stats in &all_stats {
        println!(
            "  - {}: {} triples → {} facts (tx: {})",
            stats.file, stats.triples_processed, stats.facts_inserted, stats.tx_start
        );
    }

    Ok(all_stats)
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
        let stats = import_turtle_file(conn, &rdf_path, "core")?;
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
            "SELECT 1 FROM triples WHERE subject = 'http://www.w3.org/2002/07/owl#Thing' AND predicate = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type' AND object = 'http://www.w3.org/2002/07/owl#Class' AND retracted = 0",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if !owl_thing_exists {
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_type, object_datatype, object_language, object_number, object_integer, object_datetime, object_boolean, tx, origin, retracted, created_at)
             VALUES (?, ?, ?, NULL, ?, NULL, NULL, NULL, NULL, NULL, NULL, ?, ?, 0, ?)",
            rusqlite::params![
                "http://www.w3.org/2002/07/owl#Thing",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://www.w3.org/2002/07/owl#Class",
                "iri",
                1,  // tx=1 (meta-ontology layer)
                "core",
                now,
            ],
        )?;
        println!("  ✅ Added owl:Thing as owl:Class");
    }

    // Find all orphan classes (owl:Class without rdfs:subClassOf)
    let orphan_classes: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT subject FROM triples
             WHERE predicate = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
             AND object_type = 'iri'
             AND object = 'http://www.w3.org/2002/07/owl#Class'
             AND retracted = 0
             AND subject != 'http://www.w3.org/2002/07/owl#Thing'
             AND subject NOT IN (
                 SELECT DISTINCT subject FROM triples
                 WHERE predicate = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
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
            "INSERT INTO triples (subject, predicate, object, object_value, object_type, object_datatype, object_language, object_number, object_integer, object_datetime, object_boolean, tx, origin, retracted, created_at)
             VALUES (?, ?, ?, NULL, ?, NULL, NULL, NULL, NULL, NULL, NULL, ?, ?, 0, ?)",
            rusqlite::params![
                orphan_class,
                "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "http://www.w3.org/2002/07/owl#Thing",
                "iri",
                1,  // tx=1 (meta-ontology layer)
                "core",
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
