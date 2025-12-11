// ============================================================================
// Turtle Import Module
// ============================================================================
// Imports RDF/Turtle ontologies into the EAVTO fact store
//
// This module parses Turtle files and converts them to EAVTO triples
// ============================================================================

use rusqlite::Connection;
use rio_turtle::{TurtleParser, TurtleError};
use rio_xml::RdfXmlError;
use rio_api::parser::TriplesParser;
use rio_api::model::{Term, Triple as RioTriple};
use std::path::Path;
use std::io::BufReader;
use std::fs::File;
use crate::eavto::{Triple, Object};
use chrono;
use sha2::{Sha256, Digest};

/// Import error types
#[derive(Debug)]
pub enum ImportError {
    IoError(std::io::Error),
    TurtleError(TurtleError),
    XmlError(RdfXmlError),
    DatabaseError(String),
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

impl From<Box<dyn std::error::Error>> for ImportError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        ImportError::DatabaseError(err.to_string())
    }
}

impl From<rusqlite::Error> for ImportError {
    fn from(err: rusqlite::Error) -> Self {
        ImportError::DatabaseError(err.to_string())
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

/// Converts RIO Subject to string representation
fn subject_to_string(subject: &rio_api::model::Subject) -> String {
    match subject {
        rio_api::model::Subject::NamedNode(node) => node.iri.to_string(),
        rio_api::model::Subject::BlankNode(bn) => format!("_:{}", bn.id),
        rio_api::model::Subject::Triple(_) => "_:triple".to_string(),
    }
}

/// Extracts literal value from a literal term
fn get_literal_value(lit: &rio_api::model::Literal) -> String {
    match lit {
        rio_api::model::Literal::Simple { value } => value.to_string(),
        rio_api::model::Literal::LanguageTaggedString { value, .. } => value.to_string(),
        rio_api::model::Literal::Typed { value, .. } => value.to_string(),
    }
}

/// Extracts datatype IRI from a literal term
fn get_literal_datatype(lit: &rio_api::model::Literal) -> String {
    let full_iri = match lit {
        rio_api::model::Literal::Simple { .. } => "http://www.w3.org/2001/XMLSchema#string",
        rio_api::model::Literal::LanguageTaggedString { .. } => "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString",
        rio_api::model::Literal::Typed { datatype, .. } => datatype.iri,
    };
    crate::namespaces::compress_iri(full_iri)
}

/// Extracts language tag from a literal term
fn get_literal_language(lit: &rio_api::model::Literal) -> Option<String> {
    match lit {
        rio_api::model::Literal::LanguageTaggedString { language, .. } => Some(language.to_string()),
        _ => None,
    }
}

/// Converts RIO triple to EAVTO Triple
fn rio_to_eavto_triple(rio_triple: &RioTriple, tx: i64, origin_id: i64, created_at: i64) -> Triple {
    let subject_full = subject_to_string(&rio_triple.subject);
    let subject = crate::namespaces::compress_iri(&subject_full);
    let predicate = crate::namespaces::compress_iri(rio_triple.predicate.iri);

    let object = match &rio_triple.object {
        Term::NamedNode(node) => {
            let compressed_iri = crate::namespaces::compress_iri(node.iri);
            Object::Iri(compressed_iri)
        }
        Term::BlankNode(bn) => {
            Object::Blank(format!("_:{}", bn.id))
        }
        Term::Literal(lit) => {
            let value = get_literal_value(lit);
            let datatype = get_literal_datatype(lit);
            let language = get_literal_language(lit);

            // Parse typed literals into native types
            // If parse fails, it's invalid RDF data - we should fail fast
            match datatype.as_str() {
                "xsd:integer" | "xsd:int" | "xsd:long" => {
                    value.parse::<i64>()
                        .map(Object::Integer)
                        .unwrap_or_else(|e| panic!(
                            "TURTLE PARSE ERROR: Invalid integer literal in input file\n\
                             Value: '{}'\n\
                             Datatype: {}\n\
                             Error: {:?}\n\
                             This indicates malformed RDF data in the source file.",
                            value, datatype, e
                        ))
                }
                "xsd:decimal" | "xsd:double" | "xsd:float" => {
                    value.parse::<f64>()
                        .map(Object::Number)
                        .unwrap_or_else(|e| panic!(
                            "TURTLE PARSE ERROR: Invalid float literal in input file\n\
                             Value: '{}'\n\
                             Datatype: {}\n\
                             Error: {:?}\n\
                             This indicates malformed RDF data in the source file.",
                            value, datatype, e
                        ))
                }
                "xsd:boolean" => {
                    match value.as_str() {
                        "true" | "1" => Object::Boolean(true),
                        "false" | "0" => Object::Boolean(false),
                        _ => panic!(
                            "TURTLE PARSE ERROR: Invalid boolean literal in input file\n\
                             Value: '{}'\n\
                             Datatype: {}\n\
                             Expected: 'true', 'false', '1', or '0'\n\
                             This indicates malformed RDF data in the source file.",
                            value, datatype
                        )
                    }
                }
                "xsd:dateTime" => {
                    // Parse ISO 8601 datetime to Unix timestamp
                    // Format: 2025-01-28T18:38:46Z
                    chrono::DateTime::parse_from_rfc3339(&value)
                        .map(|dt| Object::DateTime(dt.timestamp()))
                        .unwrap_or_else(|e| panic!(
                            "TURTLE PARSE ERROR: Invalid dateTime literal in input file\n\
                             Value: '{}'\n\
                             Datatype: {}\n\
                             Error: {:?}\n\
                             Expected ISO 8601 format (e.g., '2025-01-28T18:38:46Z')\n\
                             This indicates malformed RDF data in the source file.",
                            value, datatype, e
                        ))
                }
                _ => Object::Literal { value, datatype: Some(datatype), language }
            }
        }
        Term::Triple(_) => {
            Object::Blank("_:triple".to_string())
        }
    };

    Triple {
        subject,
        predicate,
        object,
        tx,
        created_at,
        origin_id,
        retracted: false,
    }
}

/// Get or create origin ID
fn get_or_create_origin(conn: &Connection, origin_name: &str) -> Result<i64, ImportError> {
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

    conn.execute(
        "INSERT INTO origins (name, description) VALUES (?, ?)",
        (origin_name, format!("Auto-generated origin for {}", origin_name)),
    )?;

    let id: i64 = conn.query_row(
        "SELECT id FROM origins WHERE name = ?",
        [origin_name],
        |row| row.get(0),
    )?;

    Ok(id)
}

/// Import RDF triples from Turtle file using EAVTO
pub fn import_turtle_file(
    conn: &mut Connection,
    file_path: &Path,
    origin: &str,
) -> Result<ImportStats, ImportError> {
    let filename = file_path.file_name().unwrap().to_string_lossy().to_string();
    println!("Importing Turtle file: {}", filename);

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut triples_processed = 0u64;
    let mut eavto_triples = Vec::new();

    // Get or create origin_id
    let origin_id = get_or_create_origin(conn, origin)?;

    // Get current timestamp
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    // Parse Turtle file and collect triples
    let parse_result = TurtleParser::new(reader, None).parse_all(&mut |rio_triple: RioTriple| {
        triples_processed += 1;

        // Convert RIO triple to EAVTO triple (tx will be set later)
        let eavto_triple = rio_to_eavto_triple(&rio_triple, 0, origin_id, created_at);
        eavto_triples.push(eavto_triple);

        if triples_processed % 1000 == 0 {
            println!("  Parsed {} triples...", triples_processed);
        }

        Ok(()) as Result<(), TurtleError>
    });

    if let Err(e) = parse_result {
        return Err(ImportError::TurtleError(e));
    }

    // Store triples directly to EAVTO
    println!("  Asserting {} triples to database...", eavto_triples.len());
    let facts_inserted = crate::eavto::store::assert_triples(conn, &eavto_triples, origin)
        .map_err(|e| ImportError::DatabaseError(format!("Store error: {:?}", e)))?;
    let facts_inserted = facts_inserted as u64;

    // Get the last transaction ID for stats
    let tx_id: i64 = conn.query_row(
        "SELECT MAX(id) FROM transactions",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    println!(
        "✅ Imported {} triples ({} facts) from {}",
        triples_processed, facts_inserted, filename
    );

    Ok(ImportStats {
        file: filename,
        format: "Turtle".to_string(),
        triples_processed,
        facts_inserted,
        tx_start: tx_id,
        tx_end: tx_id,
    })
}

/// Import all FOUNDATION ontologies from filesystem with progress events
pub fn import_all_foundation_ontologies(
    conn: &mut Connection,
    app: Option<&tauri::AppHandle>,
    base_triples: u64
) -> Result<u64, ImportError> {
    use tauri::{Manager, Emitter};

    let mut total_triples = 0u64;

    println!("\n🏛️  Importing FOUNDATION ontologies...\n");

    // Get project root directory
    let project_root = std::env::var("CARGO_MANIFEST_DIR")
        .ok()
        .and_then(|manifest_dir| {
            let path = Path::new(&manifest_dir);
            path.parent().map(|p| p.to_path_buf())
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let core_ontology_dir = project_root.join("core-ontology");
    println!("📂 Reading from: {}", core_ontology_dir.display());

    // Read all .ttl files
    let mut ttl_files: Vec<std::path::PathBuf> = Vec::new();

    match std::fs::read_dir(&core_ontology_dir) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("ttl") {
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            // Skip dtype.ttl (reserved for future use)
                            if filename != "dtype.ttl" {
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

    ttl_files.sort();

    let total_files = ttl_files.len() as u32;
    println!("📋 Found {} FOUNDATION ontology files\n", total_files);

    // Import each file (with incremental import check)
    for (index, file_path) in ttl_files.iter().enumerate() {
        let filename = file_path.file_name().unwrap().to_str().unwrap();
        let origin = format!("foundation:ontology:{}", filename);

        // Check if file needs reimport
        let should_import = match needs_reimport(conn, &file_path) {
            Ok(needs) => needs,
            Err(e) => {
                eprintln!("⚠️  Error checking {}: {:?}, importing anyway", filename, e);
                true
            }
        };

        if !should_import {
            println!("⏭️  {} (unchanged, skipping)", filename);
            continue;
        }

        // Emit progress event BEFORE importing
        if let Some(handle) = app {
            let _ = handle.emit("import-progress", crate::ImportProgress {
                stage: "foundation".to_string(),
                current_file: filename.to_string(),
                current: 3 + index as u32,  // +3 because core and dtype are 1 and 2
                total: 3 + total_files,      // core + dtype + foundation files
                triples: base_triples + total_triples,
            });
        }

        println!("📄 {}", filename);
        match import_turtle_file(conn, &file_path, &origin) {
            Ok(stats) => {
                total_triples += stats.triples_processed;
                println!("   ✓ {} triples", stats.triples_processed);

                // Register imported file
                if let Err(e) = register_imported_file(conn, &file_path, &stats) {
                    eprintln!("⚠️  Failed to register {}: {:?}", filename, e);
                }

                // Emit progress event AFTER importing with updated triples
                if let Some(handle) = app {
                    let _ = handle.emit("import-progress", crate::ImportProgress {
                        stage: "foundation".to_string(),
                        current_file: filename.to_string(),
                        current: 3 + index as u32 + 1,  // Mark as completed
                        total: 3 + total_files,
                        triples: base_triples + total_triples,
                    });
                }
            }
            Err(e) => {
                eprintln!("⚠️  Failed to import {}: {:?}", filename, e);
            }
        }
    }

    println!("\n✅ FOUNDATION ontology import complete!");
    println!("📊 Total triples from foundation files: {}", total_triples);

    Ok(total_triples)
}

/// Check if a file needs to be reimported
/// Returns true if:
/// - File is not tracked yet
/// - Checksum has changed
/// - File modification time is newer than last import
pub fn needs_reimport(
    conn: &Connection,
    file_path: &Path,
) -> Result<bool, ImportError> {
    let file_path_str = file_path.to_string_lossy().to_string();

    // Query existing record
    let result: Option<(i64, String)> = conn
        .query_row(
            "SELECT last_modified, checksum FROM ontology_files WHERE file_path = ?1",
            [&file_path_str],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    // If not tracked yet, needs import
    let Some((stored_modified, stored_checksum)) = result else {
        return Ok(true);
    };

    // Get current file metadata
    let metadata = std::fs::metadata(file_path)?;
    let current_modified = metadata.modified()?
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Calculate current checksum
    let current_checksum = calculate_file_checksum(file_path)?;

    // Check if checksums differ
    if current_checksum != stored_checksum {
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        println!("🔄 File {} has changed (checksum mismatch)", file_name);
        return Ok(true);
    }

    // Check if modification time is newer
    if current_modified > stored_modified {
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        println!("🔄 File {} has been modified", file_name);
        return Ok(true);
    }

    // File hasn't changed
    Ok(false)
}

/// Calculate SHA-256 checksum of a file
fn calculate_file_checksum(path: &Path) -> Result<String, ImportError> {
    let file_content = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&file_content);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Register an imported file in the tracking table
pub fn register_imported_file(
    conn: &mut Connection,
    file_path: &Path,
    stats: &ImportStats,
) -> Result<(), ImportError> {
    let file_name = file_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ImportError::DatabaseError("Invalid file name".to_string()))?;

    let file_path_str = file_path.to_string_lossy().to_string();

    // Get file metadata
    let metadata = std::fs::metadata(file_path)?;
    let last_modified = metadata.modified()?
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Calculate checksum
    let checksum = calculate_file_checksum(file_path)?;

    // Get current timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Insert or replace record
    conn.execute(
        "INSERT OR REPLACE INTO ontology_files (file_path, file_name, last_modified, last_imported, checksum, triple_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (
            &file_path_str,
            file_name,
            last_modified,
            now,
            &checksum,
            stats.triples_processed as i64,
        ),
    )?;

    Ok(())
}