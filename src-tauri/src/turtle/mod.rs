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

            // Try to parse typed literals into native types
            match datatype.as_str() {
                "xsd:integer" | "xsd:int" | "xsd:long" => {
                    if let Ok(i) = value.parse::<i64>() {
                        Object::Integer(i)
                    } else {
                        Object::Literal { value, datatype: Some(datatype), language }
                    }
                }
                "xsd:decimal" | "xsd:double" | "xsd:float" => {
                    if let Ok(f) = value.parse::<f64>() {
                        Object::Number(f)
                    } else {
                        Object::Literal { value, datatype: Some(datatype), language }
                    }
                }
                "xsd:boolean" => {
                    match value.as_str() {
                        "true" | "1" => Object::Boolean(true),
                        "false" | "0" => Object::Boolean(false),
                        _ => Object::Literal { value, datatype: Some(datatype), language }
                    }
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

    // Use OWL semantic import (intelligently routes to appropriate operations)
    println!("  Asserting {} triples to database with semantic awareness...", eavto_triples.len());
    let facts_inserted = crate::owl::import_triples(conn, &eavto_triples, origin)
        .map_err(|e| ImportError::DatabaseError(format!("OWL import error: {:?}", e)))?;

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

/// Import all FOUNDATION ontologies from filesystem
pub fn import_all_foundation_ontologies(conn: &mut Connection) -> Result<Vec<ImportStats>, ImportError> {
    let mut all_stats = Vec::new();

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
                            // Skip files already imported
                            if filename != "rdf-rdfs-owl-core.ttl" && filename != "dtype.ttl" {
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

    println!("📋 Found {} FOUNDATION ontology files\n", ttl_files.len());

    // Import each file
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{test_helpers::setup_test_db, query};
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Create a temporary Turtle file with test content
    fn create_test_turtle_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        file.write_all(content.as_bytes()).expect("Failed to write to temp file");
        file
    }

    #[test]
    fn test_subject_to_string_named_node() {
        let subject = rio_api::model::Subject::NamedNode(rio_api::model::NamedNode {
            iri: "http://example.org/test",
        });
        assert_eq!(subject_to_string(&subject), "http://example.org/test");
    }

    #[test]
    fn test_subject_to_string_blank_node() {
        let subject = rio_api::model::Subject::BlankNode(rio_api::model::BlankNode {
            id: "b1",
        });
        assert_eq!(subject_to_string(&subject), "_:b1");
    }

    #[test]
    fn test_get_literal_value_simple() {
        let lit = rio_api::model::Literal::Simple {
            value: "Hello World",
        };
        assert_eq!(get_literal_value(&lit), "Hello World");
    }

    #[test]
    fn test_get_literal_value_language_tagged() {
        let lit = rio_api::model::Literal::LanguageTaggedString {
            value: "Bonjour",
            language: "fr",
        };
        assert_eq!(get_literal_value(&lit), "Bonjour");
    }

    #[test]
    fn test_get_literal_datatype() {
        let lit = rio_api::model::Literal::Typed {
            value: "42",
            datatype: rio_api::model::NamedNode {
                iri: "http://www.w3.org/2001/XMLSchema#integer",
            },
        };
        assert_eq!(get_literal_datatype(&lit), "xsd:integer");
    }

    #[test]
    fn test_get_literal_language_some() {
        let lit = rio_api::model::Literal::LanguageTaggedString {
            value: "Hello",
            language: "en",
        };
        assert_eq!(get_literal_language(&lit), Some("en".to_string()));
    }

    #[test]
    fn test_get_literal_language_none() {
        let lit = rio_api::model::Literal::Simple {
            value: "Plain text",
        };
        assert_eq!(get_literal_language(&lit), None);
    }

    #[test]
    fn test_import_turtle_file_basic_class() {
        let mut conn = setup_test_db();

        let turtle_content = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix test: <http://example.org/test#> .

test:Person a owl:Class ;
    rdfs:label "Person"@en .
"#;

        let temp_file = create_test_turtle_file(turtle_content);
        let result = import_turtle_file(&mut conn, temp_file.path(), "test");

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.triples_processed, 2);
        assert_eq!(stats.facts_inserted, 2);

        // Verify the class was imported using OWL abstraction
        let class = crate::owl::Class::new("http://example.org/test#Person");
        assert!(class.exists(&conn).unwrap());

        // Verify label was added
        let labels = class.get_labels(&conn).unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "Person");
        assert_eq!(labels[0].1, Some("en".to_string()));
    }

    #[test]
    fn test_import_turtle_file_with_properties() {
        let mut conn = setup_test_db();

        let turtle_content = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix test: <http://example.org/test#> .

test:hasName a owl:DatatypeProperty ;
    rdfs:domain test:Person ;
    rdfs:range <http://www.w3.org/2001/XMLSchema#string> .
"#;

        let temp_file = create_test_turtle_file(turtle_content);
        let result = import_turtle_file(&mut conn, temp_file.path(), "test");

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.triples_processed, 3);

        // Verify property was imported
        let prop = crate::owl::Property::new("http://example.org/test#hasName");
        assert!(prop.exists(&conn).unwrap());

        // Verify domain
        let domains = prop.get_domains(&conn).unwrap();
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0], "http://example.org/test#Person");

        // Verify range
        let ranges = prop.get_ranges(&conn).unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], "xsd:string");
    }

    #[test]
    fn test_import_turtle_file_with_hierarchy() {
        let mut conn = setup_test_db();

        let turtle_content = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix test: <http://example.org/test#> .

test:Animal a owl:Class .
test:Dog a owl:Class ;
    rdfs:subClassOf test:Animal .
"#;

        let temp_file = create_test_turtle_file(turtle_content);
        let result = import_turtle_file(&mut conn, temp_file.path(), "test");

        assert!(result.is_ok());

        // Verify hierarchy
        let dog = crate::owl::Class::new("http://example.org/test#Dog");
        let supers = dog.get_super_classes(&conn).unwrap();
        assert_eq!(supers.len(), 1);
        assert_eq!(supers[0], "http://example.org/test#Animal");
    }

    #[test]
    fn test_import_turtle_file_with_individuals() {
        let mut conn = setup_test_db();

        let turtle_content = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix test: <http://example.org/test#> .

test:Person a owl:Class .
test:john a test:Person .
"#;

        let temp_file = create_test_turtle_file(turtle_content);
        let result = import_turtle_file(&mut conn, temp_file.path(), "test");

        assert!(result.is_ok());

        // Verify individual
        let john = crate::owl::Individual::new("http://example.org/test#john");
        assert!(john.is_instance_of(&conn, "http://example.org/test#Person").unwrap());
    }

    #[test]
    fn test_import_turtle_file_with_literals() {
        let mut conn = setup_test_db();

        let turtle_content = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix test: <http://example.org/test#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

test:Person a owl:Class .
test:john a test:Person ;
    test:age 30 ;
    test:height 1.75 ;
    test:active true .
"#;

        let temp_file = create_test_turtle_file(turtle_content);
        let result = import_turtle_file(&mut conn, temp_file.path(), "test");

        assert!(result.is_ok());

        // Verify literals were converted correctly
        let john = crate::owl::Individual::new("http://example.org/test#john");

        let age_values = john.get_property_values(&conn, "http://example.org/test#age").unwrap();
        assert_eq!(age_values.len(), 1);
        assert!(matches!(age_values[0], crate::eavto::Object::Integer(30)));

        let height_values = john.get_property_values(&conn, "http://example.org/test#height").unwrap();
        assert_eq!(height_values.len(), 1);
        assert!(matches!(height_values[0], crate::eavto::Object::Number(_)));

        let active_values = john.get_property_values(&conn, "http://example.org/test#active").unwrap();
        assert_eq!(active_values.len(), 1);
        assert!(matches!(active_values[0], crate::eavto::Object::Boolean(true)));
    }

    #[test]
    fn test_rio_to_eavto_triple_conversion() {
        use rio_api::model::{NamedNode, Literal, Subject, Triple as RioTriple, Term};

        let rio_triple = RioTriple {
            subject: Subject::NamedNode(NamedNode { iri: "http://example.org/test#Person" }),
            predicate: NamedNode { iri: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" },
            object: Term::NamedNode(NamedNode { iri: "http://www.w3.org/2002/07/owl#Class" }),
        };

        let eavto_triple = rio_to_eavto_triple(&rio_triple, 1, 1, 1000);

        assert_eq!(eavto_triple.subject, "http://example.org/test#Person");
        assert_eq!(eavto_triple.predicate, "rdf:type");
        assert!(matches!(eavto_triple.object, crate::eavto::Object::Iri(_)));
        assert_eq!(eavto_triple.tx, 1);
        assert_eq!(eavto_triple.origin_id, 1);
        assert_eq!(eavto_triple.created_at, 1000);
    }

    #[test]
    fn test_get_or_create_origin_new() {
        let conn = setup_test_db();

        let origin_id = get_or_create_origin(&conn, "test-origin").unwrap();
        assert!(origin_id > 0);

        // Verify it was created
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM origins WHERE name = ?",
            ["test-origin"],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_get_or_create_origin_existing() {
        let conn = setup_test_db();

        let origin_id1 = get_or_create_origin(&conn, "test-origin").unwrap();
        let origin_id2 = get_or_create_origin(&conn, "test-origin").unwrap();

        // Should return same ID
        assert_eq!(origin_id1, origin_id2);

        // Verify only one was created
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM origins WHERE name = ?",
            ["test-origin"],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_import_stats_structure() {
        let stats = ImportStats {
            file: "test.ttl".to_string(),
            format: "Turtle".to_string(),
            triples_processed: 100,
            facts_inserted: 95,
            tx_start: 1,
            tx_end: 1,
        };

        assert_eq!(stats.file, "test.ttl");
        assert_eq!(stats.format, "Turtle");
        assert_eq!(stats.triples_processed, 100);
        assert_eq!(stats.facts_inserted, 95);
    }
}
