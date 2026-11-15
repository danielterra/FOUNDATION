use rusqlite::{Connection, OpenFlags};
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

fn collect_subjects(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    // The current property state is the highest transaction for each
    // (subject, predicate), including a retracted sentinel when the property
    // has been cleared. Filter retracted rows only after selecting that state;
    // filtering first resurrects a previous class type or registry link.
    // Pre-computing latest transactions also avoids the correlated MAX(tx)
    // subquery in triples_current, which was prohibitively slow for the large
    // CoreOntologyRegistry property.
    let mut stmt = conn.prepare(
        "
        WITH latest_tx AS (
            SELECT subject, predicate, MAX(tx) AS tx
            FROM triples
            GROUP BY subject, predicate
        ), current_triples AS (
            SELECT t.subject, t.predicate, t.object, t.object_type, t.retracted
            FROM triples t
            JOIN latest_tx m
              ON t.subject = m.subject
             AND t.predicate = m.predicate
             AND t.tx = m.tx
            WHERE t.retracted = 0
        )
        SELECT DISTINCT subject FROM current_triples
        WHERE predicate = 'rdf:type'
          AND object IN (
            'owl:Class', 'rdfs:Class', 'owl:ObjectProperty', 'owl:DatatypeProperty',
            'owl:AnnotationProperty', 'rdf:Property'
          )
        UNION
        SELECT DISTINCT object FROM current_triples
        WHERE subject = 'foundation:CoreOntologyRegistry'
          AND predicate = 'foundation:includesIndividual'
        UNION SELECT 'foundation:CoreOntologyRegistry'
        ORDER BY 1
    ",
    )?;

    let subjects: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(subjects)
}

#[cfg(test)]
mod tests {
    use super::collect_subjects;
    use rusqlite::Connection;

    fn fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE triples (
                subject TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object TEXT,
                object_type TEXT NOT NULL,
                tx INTEGER NOT NULL,
                retracted INTEGER NOT NULL DEFAULT 0
             );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn subject_collection_does_not_resurrect_retracted_latest_facts() {
        let conn = fixture();
        conn.execute_batch(
            "INSERT INTO triples VALUES
                ('ex:OldClass', 'rdf:type', 'owl:Class', 'iri', 1, 0),
                ('ex:OldClass', 'rdf:type', 'owl:Class', 'iri', 2, 1),
                ('foundation:CoreOntologyRegistry', 'foundation:includesIndividual', 'ex:OldIndividual', 'iri', 1, 0),
                ('foundation:CoreOntologyRegistry', 'foundation:includesIndividual', 'ex:OldIndividual', 'iri', 2, 1),
                ('ex:LiveClass', 'rdf:type', 'owl:Class', 'iri', 3, 0),
                ('foundation:CoreOntologyRegistry', 'foundation:includesIndividual', 'ex:LiveIndividual', 'iri', 3, 0);",
        ).unwrap();

        let subjects = collect_subjects(&conn).unwrap();
        assert!(!subjects.iter().any(|subject| subject == "ex:OldClass"));
        assert!(!subjects.iter().any(|subject| subject == "ex:OldIndividual"));
        assert!(subjects.iter().any(|subject| subject == "ex:LiveClass"));
        assert!(
            subjects
                .iter()
                .any(|subject| subject == "ex:LiveIndividual")
        );
    }
}

struct OriginInfo {
    original_id: i64,
    normalized_id: i64,
    name: String,
    description: Option<String>,
}

fn collect_origins(conn: &Connection) -> Result<Vec<OriginInfo>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "
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
    ",
    )?;

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

    writeln!(
        w,
        "-- ============================================================================"
    )?;
    writeln!(w, "-- FOUNDATION Core Ontology Dump")?;
    writeln!(w, "-- Subjects: {}", subject_count)?;
    writeln!(
        w,
        "-- ============================================================================"
    )?;
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
    // Avoid `triples_current` view: its per-row correlated MAX(tx) subquery
    // makes this scan O(N²). We pre-compute the latest tx per (subject, predicate)
    // in a CTE that runs ONCE, then flat-scan with a join.
    let mut stmt = conn.prepare("
        WITH max_tx AS (
            SELECT subject, predicate, MAX(tx) AS tx
            FROM triples
            WHERE subject IN (SELECT subject FROM _dump_subjects)
            GROUP BY subject, predicate
        )
        SELECT t.subject, t.predicate, t.object, t.object_value, t.object_datatype, t.object_language,
               t.object_type, t.object_number, t.object_integer, t.object_boolean,
               t.origin_id
        FROM triples t
        JOIN max_tx m
          ON t.subject = m.subject
         AND t.predicate = m.predicate
         AND t.tx = m.tx
        WHERE t.retracted = 0
        ORDER BY t.subject, t.predicate
    ")?;

    // Emit multi-row VALUES (one INSERT per BATCH rows). SQLite parses each
    // INSERT statement once and amortizes the parse over many rows, drastically
    // cutting load time vs one INSERT per row.
    const BATCH: usize = 500;
    const INSERT_HEADER: &str = "INSERT OR IGNORE INTO triples (subject, predicate, object, object_value, object_datatype, object_language, object_type, object_number, object_integer, object_boolean, tx, origin_id, retracted, created_at) VALUES";

    let mut triple_count = 0u64;
    let mut rows = stmt.query([])?;
    let mut in_batch = 0usize;

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
        let object_boolean: Option<i64> = row.get(9)?;
        let origin_id: i64 = row.get(10)?;

        let normalized_origin = *origin_map.get(&origin_id).unwrap_or(&origin_id);

        if in_batch == 0 {
            write!(w, "{}\n  ", INSERT_HEADER)?;
        } else {
            write!(w, ",\n  ")?;
        }

        write!(
            w,
            "({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 1, {}, 0, 0)",
            sql_str(&subject),
            sql_str(&predicate),
            sql_opt_str(object.as_deref()),
            sql_opt_str(object_value.as_deref()),
            sql_opt_str(object_datatype.as_deref()),
            sql_opt_str(object_language.as_deref()),
            sql_str(&object_type),
            sql_opt_f64(object_number),
            sql_opt_i64(object_integer),
            sql_opt_i64(object_boolean),
            normalized_origin,
        )?;

        in_batch += 1;
        triple_count += 1;

        if in_batch >= BATCH {
            writeln!(w, ";")?;
            in_batch = 0;
        }
    }

    if in_batch > 0 {
        writeln!(w, ";")?;
    }

    writeln!(w)?;
    writeln!(w, "COMMIT;")?;

    Ok(triple_count)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::var("FOUNDATION_DB")
        .map(PathBuf::from)
        .map_err(|_| "Set FOUNDATION_DB to a clean, ontology-only database; the user's default database is never used for a seed dump")?;

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
    // ontology.sql now lives in the separate foundation-core repo (sibling clone at ../foundation-core)
    let output = project_root
        .parent()
        .expect("app repo has no parent dir")
        .join("foundation-core")
        .join("assets")
        .join("ontology.sql");

    let t_total = std::time::Instant::now();
    eprintln!("Using database: {}", db_path.display());
    eprintln!("Output: {}", output.display());

    let t = std::time::Instant::now();
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    eprintln!("[timing] open DB: {:?}", t.elapsed());

    let registry_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM triples_current WHERE subject = 'foundation:CoreOntologyRegistry' AND predicate = 'rdf:type')",
        [],
        |row| row.get(0),
    )?;
    if !registry_exists {
        return Err("Clean ontology database is missing foundation:CoreOntologyRegistry; prepare the seed database through the normal ontology bootstrap before dumping".into());
    }

    let t = std::time::Instant::now();
    let subjects = collect_subjects(&conn)?;
    eprintln!(
        "[timing] collect_subjects: {:?} ({} subjects)",
        t.elapsed(),
        subjects.len()
    );

    let t = std::time::Instant::now();
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
    eprintln!("[timing] populate _dump_subjects: {:?}", t.elapsed());

    let t = std::time::Instant::now();
    let origins = collect_origins(&conn)?;
    eprintln!(
        "[timing] collect_origins: {:?} ({} origins)",
        t.elapsed(),
        origins.len()
    );

    let t = std::time::Instant::now();
    std::fs::create_dir_all(output.parent().unwrap())?;
    let triple_count = write_sql(&conn, &origins, subjects.len(), &output)?;
    eprintln!(
        "[timing] write_sql: {:?} ({} triples)",
        t.elapsed(),
        triple_count
    );

    let t = std::time::Instant::now();
    conn.execute_batch("DROP TABLE IF EXISTS _dump_subjects;")?;
    eprintln!("[timing] cleanup: {:?}", t.elapsed());

    eprintln!("[timing] TOTAL: {:?}", t_total.elapsed());
    eprintln!(
        "Done. {} triples written to {}",
        triple_count,
        output.display()
    );

    Ok(())
}
