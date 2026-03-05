use foundation_app::owl::{Class, Individual, Object};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::PathBuf;

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

    Ok(stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect())
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
    let schema_sql_path = project_root.join("db").join("schema.sql");
    let ontology_sql_path = project_root.join("core-ontology").join("ontology.sql");

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

    eprintln!("Verifying ontology dump via OWL API...");
    eprintln!("  Original DB: {}", db_path.display());
    eprintln!("  Schema:      {}", schema_sql_path.display());
    eprintln!("  Ontology:    {}", ontology_sql_path.display());

    let schema_sql = std::fs::read_to_string(&schema_sql_path)?;
    let ontology_sql = std::fs::read_to_string(&ontology_sql_path)?;

    let temp_conn = Connection::open_in_memory()?;
    temp_conn.execute_batch(&schema_sql).map_err(|e| format!("Schema load failed: {}", e))?;
    temp_conn.execute_batch(&ontology_sql).map_err(|e| format!("Ontology load failed: {}", e))?;

    let orig_conn = Connection::open(&db_path)?;

    let subjects = collect_core_subjects(&orig_conn)?;
    eprintln!("Checking {} core subjects via OWL API...", subjects.len());

    let mut diff_subjects = 0usize;

    for subject in &subjects {
        let orig_class = Class::get(&orig_conn, subject.as_str())
            .map_err(|e| format!("Class::get failed for {}: {}", subject, e))?;
        let temp_class = Class::get(&temp_conn, subject.as_str())
            .map_err(|e| format!("Class::get failed for {}: {}", subject, e))?;

        let diffs = match (orig_class, temp_class) {
            (Some(o), Some(t)) => compare_class(&o, &t),
            (Some(_), None) => vec!["  MISSING IN DUMP as Class".to_string()],
            (None, Some(_)) => vec!["  EXTRA IN DUMP as Class".to_string()],
            (None, None) => {
                let orig_ind = Individual::get(&orig_conn, subject.as_str())
                    .map_err(|e| format!("Individual::get failed for {}: {}", subject, e))?;
                let temp_ind = Individual::get(&temp_conn, subject.as_str())
                    .map_err(|e| format!("Individual::get failed for {}: {}", subject, e))?;

                match (orig_ind, temp_ind) {
                    (Some(o), Some(t)) => compare_individual(&o, &t),
                    (Some(_), None) => vec!["  MISSING IN DUMP as Individual".to_string()],
                    (None, Some(_)) => vec!["  EXTRA IN DUMP as Individual".to_string()],
                    (None, None) => vec!["  NOT FOUND IN EITHER DB".to_string()],
                }
            }
        };

        if !diffs.is_empty() {
            diff_subjects += 1;
            eprintln!("DIFF: {}", subject);
            for d in &diffs {
                eprintln!("{}", d);
            }
        }
    }

    let orig_triple_count: i64 = orig_conn.query_row(
        "SELECT COUNT(*) FROM triples
         WHERE retracted = 0
           AND subject IN (
             SELECT DISTINCT subject FROM triples
             WHERE retracted = 0 AND predicate = 'rdf:type'
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
           )",
        [],
        |r| r.get(0),
    )?;

    let temp_triple_count: i64 = temp_conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE retracted = 0",
        [],
        |r| r.get(0),
    )?;

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
