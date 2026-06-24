use foundation_app::owl::{Class, Individual, Object, Property};
use rayon::prelude::*;
use rusqlite::{Connection, OpenFlags};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn object_sort_key(o: &Object) -> String {
    match o {
        Object::Iri(s) => format!("1:{}", s),
        Object::Blank(s) => format!("2:{}", s),
        Object::Literal { value, datatype, language } => format!(
            "3:{}:{}:{}",
            value,
            datatype.as_deref().unwrap_or(""),
            language.as_deref().unwrap_or("")
        ),
        Object::Integer(i) => format!("4:{}", i),
        Object::Number(n) => format!("5:{}", n),
        Object::Boolean(b) => format!("6:{}", b),
        Object::DateTime(dt) => format!("7:{}", dt),
    }
}

fn sorted_iris<'a>(items: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut v: Vec<&'a str> = items.collect();
    v.sort();
    v
}

fn compare_class(orig: &Class, temp: &Class) -> Vec<String> {
    let mut diffs = Vec::new();

    if orig.label != temp.label {
        diffs.push(format!("  label: {:?} ≠ {:?}", orig.label, temp.label));
    }
    if orig.icon != temp.icon {
        diffs.push(format!("  icon: {:?} ≠ {:?}", orig.icon, temp.icon));
    }
    if orig.comment != temp.comment {
        diffs.push(format!("  comment: {:?} ≠ {:?}", orig.comment, temp.comment));
    }

    let o_types = sorted_iris(orig.types.iter().map(|t| t.iri.as_str()));
    let t_types = sorted_iris(temp.types.iter().map(|t| t.iri.as_str()));
    if o_types != t_types {
        diffs.push(format!("  types: {:?} ≠ {:?}", o_types, t_types));
    }

    let o_super = sorted_iris(orig.super_classes.iter().map(|t| t.iri.as_str()));
    let t_super = sorted_iris(temp.super_classes.iter().map(|t| t.iri.as_str()));
    if o_super != t_super {
        diffs.push(format!("  super_classes: {:?} ≠ {:?}", o_super, t_super));
    }

    let o_sub = sorted_iris(orig.sub_classes.iter().map(|t| t.iri.as_str()));
    let t_sub = sorted_iris(temp.sub_classes.iter().map(|t| t.iri.as_str()));
    if o_sub != t_sub {
        diffs.push(format!("  sub_classes: {:?} ≠ {:?}", o_sub, t_sub));
    }

    let mut o_oof = orig.one_of_values.clone();
    let mut t_oof = temp.one_of_values.clone();
    o_oof.sort();
    t_oof.sort();
    if o_oof != t_oof {
        diffs.push(format!("  one_of: {:?} ≠ {:?}", o_oof, t_oof));
    }

    diffs
}

fn compare_individual(orig: &Individual, temp: &Individual) -> Vec<String> {
    let mut diffs = Vec::new();

    if orig.label != temp.label {
        diffs.push(format!("  label: {:?} ≠ {:?}", orig.label, temp.label));
    }
    if orig.icon != temp.icon {
        diffs.push(format!("  icon: {:?} ≠ {:?}", orig.icon, temp.icon));
    }
    if orig.comment != temp.comment {
        diffs.push(format!("  comment: {:?} ≠ {:?}", orig.comment, temp.comment));
    }

    let mut o_props: Vec<String> = orig.properties.iter()
        .map(|(p, o)| format!("{}={}", p, object_sort_key(o)))
        .collect();
    let mut t_props: Vec<String> = temp.properties.iter()
        .map(|(p, o)| format!("{}={}", p, object_sort_key(o)))
        .collect();
    o_props.sort();
    t_props.sort();

    if o_props != t_props {
        let o_set: HashSet<&String> = o_props.iter().collect();
        let t_set: HashSet<&String> = t_props.iter().collect();
        for x in o_set.difference(&t_set) {
            diffs.push(format!("  - ONLY IN ORIGINAL: {}", x));
        }
        for x in t_set.difference(&o_set) {
            diffs.push(format!("  + ONLY IN DUMP:     {}", x));
        }
    }

    diffs
}

fn collect_core_subjects(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    // Avoid `triples_current` view: its per-row correlated MAX(tx) subquery
    // makes the registry UNION re-evaluate MAX(tx) for every one of ~38k rows
    // even though they all share the same (subject, predicate) pair. Use an
    // uncorrelated subquery instead — runs once.
    let mut stmt = conn.prepare("
        SELECT DISTINCT t.subject FROM triples t
        WHERE t.retracted = 0
          AND t.predicate = 'rdf:type'
          AND t.object IN (
            'owl:Class', 'rdfs:Class', 'owl:ObjectProperty', 'owl:DatatypeProperty',
            'owl:AnnotationProperty', 'rdf:Property'
          )
          AND t.tx = (
            SELECT MAX(tx) FROM triples
            WHERE subject = t.subject AND predicate = 'rdf:type'
          )
        UNION
        SELECT DISTINCT t.object FROM triples t
        WHERE t.retracted = 0
          AND t.subject = 'foundation:CoreOntologyRegistry'
          AND t.predicate = 'foundation:includesIndividual'
          AND t.tx = (
            SELECT MAX(tx) FROM triples
            WHERE subject = 'foundation:CoreOntologyRegistry'
              AND predicate = 'foundation:includesIndividual'
          )
        UNION
        SELECT 'foundation:CoreOntologyRegistry'
        ORDER BY 1
    ")?;

    Ok(stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect())
}

fn compare_property(orig: &Property, temp: &Property) -> Vec<String> {
    let mut diffs = Vec::new();
    if orig.label != temp.label {
        diffs.push(format!("  label: {:?} ≠ {:?}", orig.label, temp.label));
    }
    if orig.comment != temp.comment {
        diffs.push(format!("  comment: {:?} ≠ {:?}", orig.comment, temp.comment));
    }
    if orig.property_type != temp.property_type {
        diffs.push(format!("  property_type: {:?} ≠ {:?}", orig.property_type, temp.property_type));
    }
    let mut o_dom = orig.domains.clone(); o_dom.sort();
    let mut t_dom = temp.domains.clone(); t_dom.sort();
    if o_dom != t_dom {
        diffs.push(format!("  domains: {:?} ≠ {:?}", o_dom, t_dom));
    }
    let mut o_rng = orig.ranges.clone(); o_rng.sort();
    let mut t_rng = temp.ranges.clone(); t_rng.sort();
    if o_rng != t_rng {
        diffs.push(format!("  ranges: {:?} ≠ {:?}", o_rng, t_rng));
    }
    if orig.unit != temp.unit {
        diffs.push(format!("  unit: {:?} ≠ {:?}", orig.unit, temp.unit));
    }
    if orig.formula != temp.formula {
        diffs.push(format!("  formula: {:?} ≠ {:?}", orig.formula, temp.formula));
    }
    if orig.aggregation != temp.aggregation {
        diffs.push(format!("  aggregation: {:?} ≠ {:?}", orig.aggregation, temp.aggregation));
    }
    if orig.query_config != temp.query_config {
        diffs.push(format!("  query_config: {:?} ≠ {:?}", orig.query_config, temp.query_config));
    }
    diffs
}

/// Compute diffs for a single subject between original DB and dump.
///
/// Tries Class first, then Individual, then Property. Many subjects in the
/// registry are properties (DatatypeProperty/ObjectProperty/etc.) that aren't
/// modeled as Class or Individual — without the Property fallback they were
/// reported as "NOT FOUND IN EITHER DB" false positives.
fn compute_subject_diffs(
    orig_conn: &Connection,
    temp_conn: &Connection,
    subject: &str,
) -> Result<Vec<String>, String> {
    let orig_class = Class::get(orig_conn, subject)
        .map_err(|e| format!("Class::get failed for {}: {}", subject, e))?;
    let temp_class = Class::get(temp_conn, subject)
        .map_err(|e| format!("Class::get failed for {}: {}", subject, e))?;

    if orig_class.is_some() || temp_class.is_some() {
        return Ok(match (orig_class, temp_class) {
            (Some(o), Some(t)) => compare_class(&o, &t),
            (Some(_), None) => vec!["  MISSING IN DUMP as Class".to_string()],
            (None, Some(_)) => vec!["  EXTRA IN DUMP as Class".to_string()],
            (None, None) => unreachable!(),
        });
    }

    let orig_ind = Individual::get(orig_conn, subject)
        .map_err(|e| format!("Individual::get failed for {}: {}", subject, e))?;
    let temp_ind = Individual::get(temp_conn, subject)
        .map_err(|e| format!("Individual::get failed for {}: {}", subject, e))?;

    if orig_ind.is_some() || temp_ind.is_some() {
        return Ok(match (orig_ind, temp_ind) {
            (Some(o), Some(t)) => compare_individual(&o, &t),
            (Some(_), None) => vec!["  MISSING IN DUMP as Individual".to_string()],
            (None, Some(_)) => vec!["  EXTRA IN DUMP as Individual".to_string()],
            (None, None) => unreachable!(),
        });
    }

    let orig_prop = Property::get(orig_conn, subject)
        .map_err(|e| format!("Property::get failed for {}: {}", subject, e))?;
    let temp_prop = Property::get(temp_conn, subject)
        .map_err(|e| format!("Property::get failed for {}: {}", subject, e))?;

    Ok(match (orig_prop, temp_prop) {
        (Some(o), Some(t)) => compare_property(&o, &t),
        (Some(_), None) => vec!["  MISSING IN DUMP as Property".to_string()],
        (None, Some(_)) => vec!["  EXTRA IN DUMP as Property".to_string()],
        // Both DBs agree the subject isn't reachable via the OWL API. This is
        // typically stale-registry zombies (older TXs whose entries are no longer
        // accessible via triples_current). Since both DBs match, it's not a diff —
        // the dump faithfully preserved whatever the original DB had.
        (None, None) => Vec::new(),
    })
}

/// Open a read-only Connection pair (orig DB + dump). Used by rayon's `map_init`
/// so each worker thread has its own pair (Connection isn't Sync).
fn open_read_pair(
    db_path: &Path,
    temp_db_path: &Path,
) -> Result<(Connection, Connection), String> {
    let orig = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("open orig DB: {}", e))?;
    let temp = Connection::open_with_flags(temp_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("open temp DB: {}", e))?;
    Ok((orig, temp))
}

/// Removes the temp dump file when dropped (covers both success and panic paths).
struct TempFileGuard<'a>(&'a Path);
impl Drop for TempFileGuard<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
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

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir
        .parent()
        .expect("scripts/verify-ontology has no parent")
        .parent()
        .expect("scripts has no parent");
    let schema_sql_path = project_root
        .join("src-tauri").join("crates").join("foundation-core")
        .join("assets").join("schema.sql");
    let ontology_sql_path = project_root
        .join("src-tauri").join("crates").join("foundation-core")
        .join("assets").join("ontology.sql");

    if !db_path.exists() {
        eprintln!("ERROR: Database not found at {}", db_path.display());
        std::process::exit(2);
    }
    if !schema_sql_path.exists() {
        eprintln!("ERROR: Schema not found at {}", schema_sql_path.display());
        std::process::exit(2);
    }
    if !ontology_sql_path.exists() {
        eprintln!("ERROR: ontology.sql not found at {}", ontology_sql_path.display());
        eprintln!("  Run: cargo run --manifest-path scripts/dump-ontology/Cargo.toml");
        std::process::exit(2);
    }

    let t_total = std::time::Instant::now();
    eprintln!("Verifying ontology dump via OWL API...");
    eprintln!("  Original DB: {}", db_path.display());
    eprintln!("  Schema:      {}", schema_sql_path.display());
    eprintln!("  Ontology:    {}", ontology_sql_path.display());

    let t = std::time::Instant::now();
    let schema_sql = std::fs::read_to_string(&schema_sql_path)?;
    let ontology_sql = std::fs::read_to_string(&ontology_sql_path)?;
    eprintln!(
        "[timing] read SQL files: {:?} (schema={}KB, ontology={}KB)",
        t.elapsed(),
        schema_sql.len() / 1024,
        ontology_sql.len() / 1024,
    );

    // Persist the dump to a temp file so each rayon worker can open its own
    // read-only connection. SQLite supports concurrent reads but a single
    // Connection isn't Sync, and the in-memory variant can't be shared across
    // threads at all.
    let temp_db_path = std::env::temp_dir().join(format!(
        "foundation-verify-ontology-{}.db",
        std::process::id()
    ));
    if temp_db_path.exists() {
        std::fs::remove_file(&temp_db_path).ok();
    }
    let t = std::time::Instant::now();
    {
        let temp_conn = Connection::open(&temp_db_path)?;
        // The temp DB is throwaway, so disable durability for the bulk load.
        temp_conn.execute_batch(
            "PRAGMA journal_mode = OFF;
             PRAGMA synchronous = OFF;
             PRAGMA temp_store = MEMORY;
             PRAGMA locking_mode = EXCLUSIVE;",
        )?;
        let t_schema = std::time::Instant::now();
        temp_conn.execute_batch(&schema_sql).map_err(|e| format!("Schema load failed: {}", e))?;
        eprintln!("[timing]   schema: {:?}", t_schema.elapsed());

        // Drop user-created indices before bulk insert. The triples table has 10+
        // indices that would each be updated for every INSERT (63k × 10 = ~640k
        // index ops); dropping and recreating them after the load is much faster.
        let t_drop = std::time::Instant::now();
        let index_defs: Vec<(String, String)> = {
            let mut stmt = temp_conn.prepare(
                "SELECT name, sql FROM sqlite_master \
                 WHERE type = 'index' AND sql IS NOT NULL",
            )?;
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|r| r.ok())
                .collect()
        };
        for (name, _) in &index_defs {
            temp_conn.execute_batch(&format!("DROP INDEX IF EXISTS \"{}\";", name))?;
        }
        eprintln!(
            "[timing]   drop {} indices: {:?}",
            index_defs.len(),
            t_drop.elapsed(),
        );

        let t_dump = std::time::Instant::now();
        temp_conn.execute_batch(&ontology_sql).map_err(|e| format!("Ontology load failed: {}", e))?;
        eprintln!("[timing]   ontology dump (no indices): {:?}", t_dump.elapsed());

        // Recreate the indices in one shot.
        let t_idx = std::time::Instant::now();
        let mut create_all = String::new();
        for (_, sql) in &index_defs {
            create_all.push_str(sql);
            create_all.push_str(";\n");
        }
        temp_conn.execute_batch(&create_all)?;
        eprintln!("[timing]   recreate {} indices: {:?}", index_defs.len(), t_idx.elapsed());
    }
    eprintln!("[timing] load dump into temp DB: {:?}", t.elapsed());

    let _temp_db_guard = TempFileGuard(&temp_db_path);

    let t = std::time::Instant::now();
    let orig_conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let subjects = collect_core_subjects(&orig_conn)?;
    drop(orig_conn);
    eprintln!(
        "[timing] collect_core_subjects: {:?} ({} subjects)",
        t.elapsed(),
        subjects.len(),
    );
    eprintln!("Checking {} core subjects via OWL API...", subjects.len());

    let t = std::time::Instant::now();
    let num_threads = rayon::current_num_threads();
    let results: Vec<(String, Vec<String>)> = subjects
        .par_iter()
        .map_init(
            || open_read_pair(&db_path, &temp_db_path),
            |conns, subject| -> Result<Option<(String, Vec<String>)>, String> {
                let (orig_conn, temp_conn) = conns
                    .as_ref()
                    .map_err(|e| e.clone())?;
                let diffs = compute_subject_diffs(orig_conn, temp_conn, subject)?;
                if diffs.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some((subject.clone(), diffs)))
                }
            },
        )
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|x| x)
        .collect();
    eprintln!(
        "[timing] parallel verification ({} threads): {:?}",
        num_threads,
        t.elapsed(),
    );

    let mut diff_subjects = 0usize;
    for (subject, diffs) in &results {
        diff_subjects += 1;
        eprintln!("DIFF: {}", subject);
        for d in diffs {
            eprintln!("{}", d);
        }
    }

    let t = std::time::Instant::now();
    let orig_conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let temp_conn = Connection::open_with_flags(&temp_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    // Same anti-pattern fix as collect_core_subjects: avoid `triples_current`'s
    // correlated MAX(tx) subquery. Pre-compute the latest tx per (subject,
    // predicate) once via a CTE, then count.
    let orig_triple_count: i64 = orig_conn.query_row(
        "WITH dump_subjects AS (
            SELECT DISTINCT t.subject FROM triples t
            WHERE t.retracted = 0
              AND t.predicate = 'rdf:type'
              AND t.object IN (
                'owl:Class', 'rdfs:Class', 'owl:ObjectProperty', 'owl:DatatypeProperty',
                'owl:AnnotationProperty', 'rdf:Property'
              )
              AND t.tx = (
                SELECT MAX(tx) FROM triples
                WHERE subject = t.subject AND predicate = 'rdf:type'
              )
            UNION
            SELECT DISTINCT t.object FROM triples t
            WHERE t.retracted = 0
              AND t.subject = 'foundation:CoreOntologyRegistry'
              AND t.predicate = 'foundation:includesIndividual'
              AND t.tx = (
                SELECT MAX(tx) FROM triples
                WHERE subject = 'foundation:CoreOntologyRegistry'
                  AND predicate = 'foundation:includesIndividual'
              )
            UNION
            SELECT 'foundation:CoreOntologyRegistry'
         ),
         max_tx AS (
             SELECT subject, predicate, MAX(tx) AS tx
             FROM triples
             WHERE retracted = 0
               AND subject IN (SELECT subject FROM dump_subjects)
             GROUP BY subject, predicate
         )
         SELECT COUNT(*)
         FROM triples t
         JOIN max_tx m ON t.subject = m.subject AND t.predicate = m.predicate AND t.tx = m.tx
         WHERE t.retracted = 0",
        [],
        |r| r.get(0),
    )?;

    let temp_triple_count: i64 = temp_conn.query_row(
        "SELECT COUNT(*) FROM triples_current",
        [],
        |r| r.get(0),
    )?;
    eprintln!("[timing] triple count queries: {:?}", t.elapsed());
    eprintln!("[timing] TOTAL: {:?}", t_total.elapsed());

    eprintln!();
    eprintln!("Summary:");
    eprintln!("  Subjects checked:  {}", subjects.len());
    eprintln!("  Original triples:  {}", orig_triple_count);
    eprintln!("  Dump triples:      {}", temp_triple_count);
    eprintln!("  Subject diffs:     {}", diff_subjects);

    if diff_subjects == 0 && orig_triple_count == temp_triple_count {
        eprintln!("PASS: Perfect match.");
        Ok(())
    } else {
        if diff_subjects > 0 {
            eprintln!("FAIL: {} subjects have differences.", diff_subjects);
        }
        if orig_triple_count != temp_triple_count {
            eprintln!(
                "FAIL: Triple count mismatch (original={}, dump={}).",
                orig_triple_count, temp_triple_count
            );
        }
        std::process::exit(1);
    }
}
