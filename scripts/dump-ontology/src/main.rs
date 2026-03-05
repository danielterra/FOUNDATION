use rusqlite::Connection;
use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

fn sql_str(s: &str) -> String {
    format!("'{}'", escape_sql(s))
}

fn sql_opt_str(s: Option<&str>) -> String {
    match s {
        Some(v) => sql_str(v),
        None => "NULL".to_string(),
    }
}

fn sql_opt_i64(v: Option<i64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "NULL".to_string(),
    }
}

fn sql_opt_f64(v: Option<f64>) -> String {
    match v {
        Some(n) if n.is_finite() => format!("{}", n),
        _ => "NULL".to_string(),
    }
}

fn bootstrap_registry(conn: &Connection) -> Result<(), rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:CoreOntologyRegistry' AND retracted = 0",
        [],
        |r| r.get(0),
    )?;

    if count == 0 {
        eprintln!("Creating foundation:CoreOntologyRegistry...");
        conn.execute_batch("
            BEGIN;

            INSERT OR IGNORE INTO triples
              (subject, predicate, object, object_type, tx, origin_id, retracted, created_at)
            SELECT 'foundation:CoreOntologyRegistry', 'rdf:type', 'owl:NamedIndividual', 'iri',
                   1, id, 0, 0
            FROM origins WHERE name = 'core' LIMIT 1;

            INSERT OR IGNORE INTO triples
              (subject, predicate, object, object_type, tx, origin_id, retracted, created_at)
            SELECT DISTINCT
              'foundation:CoreOntologyRegistry', 'foundation:includesIndividual', t.subject, 'iri',
              1, o.id, 0, 0
            FROM triples t
            JOIN origins o ON t.origin_id = o.id
            WHERE t.retracted = 0
              AND t.predicate = 'rdf:type'
              AND (o.name LIKE 'foundation:ontology:%' OR o.name = 'core')
              AND t.object NOT IN (
                'owl:Class', 'rdfs:Class', 'owl:ObjectProperty', 'owl:DatatypeProperty',
                'owl:AnnotationProperty', 'rdf:Property', 'owl:Ontology', 'owl:NamedIndividual'
              )
              AND t.subject NOT LIKE '_:%';

            COMMIT;
        ")?;

        let linked: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:CoreOntologyRegistry' AND predicate = 'foundation:includesIndividual' AND retracted = 0",
            [],
            |r| r.get(0),
        )?;
        eprintln!("Registry created with {} individuals.", linked);
    } else {
        let linked: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:CoreOntologyRegistry' AND predicate = 'foundation:includesIndividual' AND retracted = 0",
            [],
            |r| r.get(0),
        )?;
        eprintln!("Registry already exists with {} individuals.", linked);
    }

    Ok(())
}

fn collect_subjects(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare("
        SELECT DISTINCT subject FROM triples
        WHERE retracted = 0
          AND predicate = 'rdf:type'
          AND object IN (
            'owl:Class', 'rdfs:Class', 'owl:ObjectProperty', 'owl:DatatypeProperty',
            'owl:AnnotationProperty', 'rdf:Property'
          )
        UNION
        SELECT DISTINCT object FROM triples
        WHERE retracted = 0
          AND subject = 'foundation:CoreOntologyRegistry'
          AND predicate = 'foundation:includesIndividual'
        UNION
        SELECT 'foundation:CoreOntologyRegistry'
        ORDER BY 1
    ")?;

    let subjects: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(subjects)
}

struct OriginInfo {
    original_id: i64,
    normalized_id: i64,
    name: String,
    description: Option<String>,
}

fn collect_origins(conn: &Connection) -> Result<Vec<OriginInfo>, rusqlite::Error> {
    let mut stmt = conn.prepare("
        SELECT o.id,
               ROW_NUMBER() OVER (ORDER BY o.name) + 100,
               o.name,
               o.description
        FROM origins o
        WHERE o.id IN (
          SELECT DISTINCT t.origin_id FROM triples t
          WHERE t.retracted = 0
            AND t.subject IN (SELECT subject FROM _dump_subjects)
        )
        ORDER BY o.name
    ")?;

    let origins: Vec<OriginInfo> = stmt
        .query_map([], |r| {
            Ok(OriginInfo {
                original_id: r.get(0)?,
                normalized_id: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(origins)
}

fn write_sql(
    conn: &Connection,
    origins: &[OriginInfo],
    subject_count: usize,
    output: &Path,
) -> Result<u64, Box<dyn std::error::Error>> {
    let origin_map: HashMap<i64, i64> = origins
        .iter()
        .map(|o| (o.original_id, o.normalized_id))
        .collect();

    let file = std::fs::File::create(output)?;
    let mut w = BufWriter::new(file);

    writeln!(w, "-- ============================================================================")?;
    writeln!(w, "-- FOUNDATION Core Ontology Dump")?;
    writeln!(w, "-- Subjects: {}", subject_count)?;
    writeln!(w, "-- ============================================================================")?;
    writeln!(w)?;
    writeln!(w, "BEGIN;")?;
    writeln!(w)?;

    writeln!(w, "-- Origins")?;
    for o in origins {
        writeln!(
            w,
            "INSERT OR IGNORE INTO origins (id, name, description) VALUES ({}, {}, {});",
            o.normalized_id,
            sql_str(&o.name),
            sql_opt_str(o.description.as_deref()),
        )?;
    }
    writeln!(w)?;

    writeln!(w, "-- Ontology import transaction")?;
    writeln!(
        w,
        "INSERT OR IGNORE INTO transactions (tx, origin, created_at) VALUES (1, 'ontology-import', 0);"
    )?;
    writeln!(w)?;

    writeln!(w, "-- Triples")?;
    let mut stmt = conn.prepare("
        SELECT subject, predicate, object, object_value, object_datatype, object_language,
               object_type, object_number, object_integer, object_datetime, object_boolean,
               origin_id
        FROM triples
        WHERE retracted = 0
          AND subject IN (SELECT subject FROM _dump_subjects)
        ORDER BY subject, predicate
    ")?;

    let mut triple_count = 0u64;
    let mut rows = stmt.query([])?;

    while let Some(row) = rows.next()? {
        let subject: String = row.get(0)?;
        let predicate: String = row.get(1)?;
        let object: Option<String> = row.get(2)?;
        let object_value: Option<String> = row.get(3)?;
        let object_datatype: Option<String> = row.get(4)?;
        let object_language: Option<String> = row.get(5)?;
        let object_type: String = row.get(6)?;
        let object_number: Option<f64> = row.get(7)?;
        let object_integer: Option<i64> = row.get(8)?;
        let object_datetime: Option<i64> = row.get(9)?;
        let object_boolean: Option<i64> = row.get(10)?;
        let origin_id: i64 = row.get(11)?;

        let normalized_origin = *origin_map.get(&origin_id).unwrap_or(&origin_id);

        writeln!(
            w,
            "INSERT OR IGNORE INTO triples (subject, predicate, object, object_value, object_datatype, object_language, object_type, object_number, object_integer, object_datetime, object_boolean, tx, origin_id, retracted, created_at) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 1, {}, 0, 0);",
            sql_str(&subject),
            sql_str(&predicate),
            sql_opt_str(object.as_deref()),
            sql_opt_str(object_value.as_deref()),
            sql_opt_str(object_datatype.as_deref()),
            sql_opt_str(object_language.as_deref()),
            sql_str(&object_type),
            sql_opt_f64(object_number),
            sql_opt_i64(object_integer),
            sql_opt_i64(object_datetime),
            sql_opt_i64(object_boolean),
            normalized_origin,
        )?;

        triple_count += 1;
    }

    writeln!(w)?;
    writeln!(w, "COMMIT;")?;

    Ok(triple_count)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::var("FOUNDATION_DB")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::document_dir()
                .expect("Could not find Documents directory")
                .join("Foundation")
                .join("FOUNDATION.db")
        });

    if !db_path.exists() {
        eprintln!("ERROR: Database not found at {}", db_path.display());
        std::process::exit(1);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir
        .parent()
        .expect("scripts/dump-ontology has no parent")
        .parent()
        .expect("scripts has no parent");
    let output = project_root.join("core-ontology").join("ontology.sql");

    eprintln!("Using database: {}", db_path.display());
    eprintln!("Output: {}", output.display());

    let conn = Connection::open(&db_path)?;

    bootstrap_registry(&conn)?;

    eprintln!("Collecting subjects...");
    let subjects = collect_subjects(&conn)?;
    eprintln!("Subjects to dump: {}", subjects.len());

    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS _dump_subjects (subject TEXT PRIMARY KEY);",
    )?;

    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare("INSERT OR IGNORE INTO _dump_subjects VALUES (?1)")?;
        for s in &subjects {
            stmt.execute([s])?;
        }
    }
    tx.commit()?;

    let origins = collect_origins(&conn)?;

    eprintln!("Generating {}...", output.display());
    std::fs::create_dir_all(output.parent().unwrap())?;
    let triple_count = write_sql(&conn, &origins, subjects.len(), &output)?;

    conn.execute_batch("DROP TABLE IF EXISTS _dump_subjects;")?;

    eprintln!("Done. {} triples written to {}", triple_count, output.display());

    Ok(())
}
