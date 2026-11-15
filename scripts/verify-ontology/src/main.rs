use foundation_app::owl::{Class, Individual, Object, Property};
use rayon::prelude::*;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const COMPONENT_CLASS: &str = "foundation:SemanticComponentDefinition";
const SPEC_CLASS: &str = "foundation:JsonRenderSpec";
const CORE_REGISTRY: &str = "foundation:CoreOntologyRegistry";
const CORE_INDIVIDUAL: &str = "foundation:includesIndividual";

#[derive(Debug, Deserialize)]
struct SemanticManifest {
    version: u64,
    components: Vec<SemanticManifestComponent>,
    actions: Vec<Value>,
    examples: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct SemanticManifestComponent {
    id: String,
    description: String,
    #[serde(rename = "contractVersion")]
    contract_version: u64,
    #[serde(rename = "propsSchema")]
    props_schema: Value,
    slots: Vec<Value>,
}

fn canonical_json(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn export_semantic_manifest(project_root: &Path) -> Result<SemanticManifest, String> {
    let path = project_root.join("src/lib/json-render/semantic/catalog-manifest.json");
    let content = std::fs::read(&path).map_err(|error| {
        format!(
            "Could not read semantic catalog manifest {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_slice(&content)
        .map_err(|error| format!("Semantic catalog manifest is invalid: {error}"))
}

fn current_literal(
    conn: &Connection,
    subject: &str,
    predicate: &str,
) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row(
        "SELECT object_value FROM triples_current
         WHERE subject = ?1 AND predicate = ?2 AND object_type = 'literal' LIMIT 1",
        [subject, predicate],
        |row| row.get::<_, String>(0),
    )
    .optional()
}

fn current_iri(
    conn: &Connection,
    subject: &str,
    predicate: &str,
) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row(
        "SELECT object FROM triples_current
         WHERE subject = ?1 AND predicate = ?2 AND object_type = 'iri' LIMIT 1",
        [subject, predicate],
        |row| row.get::<_, String>(0),
    )
    .optional()
}

fn required_literal(conn: &Connection, subject: &str, predicate: &str) -> Result<String, String> {
    current_literal(conn, subject, predicate)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("{subject} is missing required literal {predicate}"))
}

fn sanitized_component_id(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn verify_semantic_catalog(conn: &Connection, project_root: &Path) -> Result<(), String> {
    let manifest = export_semantic_manifest(project_root)?;

    for class in [COMPONENT_CLASS, SPEC_CLASS] {
        if current_iri(conn, class, "rdf:type")
            .map_err(|error| error.to_string())?
            .as_deref()
            != Some("owl:Class")
        {
            return Err(format!("Base ontology is missing class {class}"));
        }
    }

    for property in [
        "foundation:componentId",
        "foundation:componentIntent",
        "foundation:componentPropsSchema",
        "foundation:componentActionsSchema",
        "foundation:componentExamples",
        "foundation:componentManifestVersion",
        "foundation:componentContractVersion",
        "foundation:componentContractHash",
        "foundation:usesComponentContract",
        "foundation:defaultWidgetSpec",
    ] {
        if current_iri(conn, property, "rdf:type")
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Err(format!(
                "Base ontology is missing catalog property {property}"
            ));
        }
    }
    if current_iri(conn, "foundation:defaultWidgetSpec", "rdfs:domain")
        .map_err(|error| error.to_string())?
        .as_deref()
        != Some("owl:Class")
        || current_iri(conn, "foundation:defaultWidgetSpec", "rdfs:range")
            .map_err(|error| error.to_string())?
            .as_deref()
            != Some(SPEC_CLASS)
    {
        return Err("foundation:defaultWidgetSpec must have domain owl:Class and range foundation:JsonRenderSpec".to_string());
    }

    let mut actual_ids = HashSet::new();
    let mut query = conn
        .prepare("SELECT subject, object_value FROM triples_current WHERE predicate = 'foundation:componentId'")
        .map_err(|error| error.to_string())?;
    let contract_rows = query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    for (_, component_id) in &contract_rows {
        actual_ids.insert(component_id.clone());
    }
    let expected_ids: HashSet<String> = manifest
        .components
        .iter()
        .map(|component| component.id.clone())
        .collect();
    if actual_ids != expected_ids {
        let missing: Vec<_> = expected_ids.difference(&actual_ids).cloned().collect();
        let extra: Vec<_> = actual_ids.difference(&expected_ids).cloned().collect();
        return Err(format!(
            "Semantic catalog component set diverges from renderer. Missing: {missing:?}; extra: {extra:?}"
        ));
    }

    let actions_schema = canonical_json(&Value::Array(manifest.actions.clone()))?;
    for component in &manifest.components {
        let mut props_schema = component.props_schema.clone();
        if !component.slots.is_empty() {
            props_schema
                .as_object_mut()
                .ok_or_else(|| format!("Props schema for {} is not a JSON object", component.id))?
                .insert(
                    "x-foundation-slots".to_string(),
                    Value::Array(component.slots.clone()),
                );
        }
        let expected_props = canonical_json(&props_schema)?;
        let example = manifest
            .examples
            .get(&component.id)
            .ok_or_else(|| format!("Renderer manifest has no example for {}", component.id))?;
        let expected_example = canonical_json(example)?;
        let canonical_actions: Value =
            serde_json::from_str(&actions_schema).map_err(|error| error.to_string())?;
        let expected_hash = format!(
            "sha256:{:x}",
            Sha256::digest(
                canonical_json(&serde_json::json!({
                    "componentId": component.id,
                    "intent": component.description,
                    "propsSchema": props_schema,
                    "actionsSchema": canonical_actions,
                    "examples": example,
                }))?
                .as_bytes()
            )
        );

        let matching: Vec<&String> = contract_rows
            .iter()
            .filter(|(_, id)| id == &component.id)
            .map(|(subject, _)| subject)
            .collect();
        let mut latest: Option<(u64, &str)> = None;
        for iri in matching {
            let version: u64 = required_literal(conn, iri, "foundation:componentContractVersion")?
                .parse()
                .map_err(|_| format!("{iri} has an invalid component contract version"))?;
            if latest.is_none_or(|(latest_version, _)| version > latest_version) {
                latest = Some((version, iri));
            }
        }
        let (version, iri) =
            latest.ok_or_else(|| format!("No versioned contract exists for {}", component.id))?;
        if version != component.contract_version {
            return Err(format!(
                "Contract version mismatch for {}: renderer={}, ontology={version}",
                component.id, component.contract_version
            ));
        }
        let expected_iri = format!(
            "foundation:SemanticComponentDefinition_{}_v{version}",
            sanitized_component_id(&component.id)
        );
        if iri != expected_iri {
            return Err(format!(
                "Contract IRI/version mismatch: expected {expected_iri}, found {iri}"
            ));
        }

        let stored_hash = required_literal(conn, iri, "foundation:componentContractHash")?;
        if stored_hash != expected_hash {
            return Err(format!(
                "Contract hash mismatch for {iri}: renderer={expected_hash}, ontology={stored_hash}"
            ));
        }
        if required_literal(conn, iri, "foundation:componentManifestVersion")?
            != manifest.version.to_string()
        {
            return Err(format!(
                "Manifest version mismatch for {iri}: renderer={}, ontology is stale",
                manifest.version
            ));
        }
        if required_literal(conn, iri, "foundation:componentIntent")? != component.description {
            return Err(format!("Intent mismatch for {iri}"));
        }
        let stored_props: Value = serde_json::from_str(&required_literal(
            conn,
            iri,
            "foundation:componentPropsSchema",
        )?)
        .map_err(|error| format!("Invalid props schema in {iri}: {error}"))?;
        if canonical_json(&stored_props)? != expected_props {
            return Err(format!("Props schema mismatch for {iri}"));
        }
        let stored_actions: Value = serde_json::from_str(&required_literal(
            conn,
            iri,
            "foundation:componentActionsSchema",
        )?)
        .map_err(|error| format!("Invalid actions schema in {iri}: {error}"))?;
        if canonical_json(&stored_actions)? != actions_schema {
            return Err(format!("Actions schema mismatch for {iri}"));
        }
        let stored_example: Value = serde_json::from_str(&required_literal(
            conn,
            iri,
            "foundation:componentExamples",
        )?)
        .map_err(|error| format!("Invalid example in {iri}: {error}"))?;
        if canonical_json(&stored_example)? != expected_example {
            return Err(format!("Example mismatch for {iri}"));
        }

        let registered = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM triples_current WHERE subject = ?1 AND predicate = ?2 AND object = ?3)",
            [CORE_REGISTRY, CORE_INDIVIDUAL, iri],
            |row| row.get::<_, bool>(0),
        ).map_err(|error| error.to_string())?;
        if !registered {
            return Err(format!(
                "Versioned contract {iri} is not linked from {CORE_REGISTRY}"
            ));
        }
    }

    eprintln!(
        "PASS: semantic catalog matches renderer ({} components, manifest v{}); defaultWidgetSpec is present.",
        manifest.components.len(),
        manifest.version
    );
    Ok(())
}

fn object_sort_key(o: &Object) -> String {
    match o {
        Object::Iri(s) => format!("1:{}", s),
        Object::Blank(s) => format!("2:{}", s),
        Object::Literal {
            value,
            datatype,
            language,
        } => format!(
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
        diffs.push(format!(
            "  comment: {:?} ≠ {:?}",
            orig.comment, temp.comment
        ));
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
        diffs.push(format!(
            "  comment: {:?} ≠ {:?}",
            orig.comment, temp.comment
        ));
    }

    let mut o_props: Vec<String> = orig
        .properties
        .iter()
        .map(|(p, o)| format!("{}={}", p, object_sort_key(o)))
        .collect();
    let mut t_props: Vec<String> = temp
        .properties
        .iter()
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
    // Select the authoritative state first, then filter retracted rows. A
    // retracted latest row is a tombstone; filtering it before MAX(tx) would
    // incorrectly bring an older class type or registry membership back.
    // The grouped CTE also avoids the expensive correlated MAX(tx) in the view.
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

    Ok(stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect())
}

#[cfg(test)]
mod current_fact_tests {
    use super::collect_core_subjects;
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
    fn verifier_subject_collection_does_not_resurrect_retracted_latest_facts() {
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

        let subjects = collect_core_subjects(&conn).unwrap();
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

fn compare_property(orig: &Property, temp: &Property) -> Vec<String> {
    let mut diffs = Vec::new();
    if orig.label != temp.label {
        diffs.push(format!("  label: {:?} ≠ {:?}", orig.label, temp.label));
    }
    if orig.comment != temp.comment {
        diffs.push(format!(
            "  comment: {:?} ≠ {:?}",
            orig.comment, temp.comment
        ));
    }
    if orig.property_type != temp.property_type {
        diffs.push(format!(
            "  property_type: {:?} ≠ {:?}",
            orig.property_type, temp.property_type
        ));
    }
    let mut o_dom = orig.domains.clone();
    o_dom.sort();
    let mut t_dom = temp.domains.clone();
    t_dom.sort();
    if o_dom != t_dom {
        diffs.push(format!("  domains: {:?} ≠ {:?}", o_dom, t_dom));
    }
    let mut o_rng = orig.ranges.clone();
    o_rng.sort();
    let mut t_rng = temp.ranges.clone();
    t_rng.sort();
    if o_rng != t_rng {
        diffs.push(format!("  ranges: {:?} ≠ {:?}", o_rng, t_rng));
    }
    if orig.unit != temp.unit {
        diffs.push(format!("  unit: {:?} ≠ {:?}", orig.unit, temp.unit));
    }
    if orig.formula != temp.formula {
        diffs.push(format!(
            "  formula: {:?} ≠ {:?}",
            orig.formula, temp.formula
        ));
    }
    if orig.aggregation != temp.aggregation {
        diffs.push(format!(
            "  aggregation: {:?} ≠ {:?}",
            orig.aggregation, temp.aggregation
        ));
    }
    if orig.query_config != temp.query_config {
        diffs.push(format!(
            "  query_config: {:?} ≠ {:?}",
            orig.query_config, temp.query_config
        ));
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
fn open_read_pair(db_path: &Path, temp_db_path: &Path) -> Result<(Connection, Connection), String> {
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
    // schema.sql / ontology.sql now live in the separate foundation-core repo (sibling clone at ../foundation-core)
    let core_assets = project_root
        .parent()
        .expect("app repo has no parent dir")
        .join("foundation-core")
        .join("assets");
    let schema_sql_path = core_assets.join("schema.sql");
    let ontology_sql_path = core_assets.join("ontology.sql");

    if !db_path.exists() {
        eprintln!("ERROR: Database not found at {}", db_path.display());
        std::process::exit(2);
    }
    if !schema_sql_path.exists() {
        eprintln!("ERROR: Schema not found at {}", schema_sql_path.display());
        std::process::exit(2);
    }
    if !ontology_sql_path.exists() {
        eprintln!(
            "ERROR: ontology.sql not found at {}",
            ontology_sql_path.display()
        );
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
        temp_conn
            .execute_batch(&schema_sql)
            .map_err(|e| format!("Schema load failed: {}", e))?;
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
        temp_conn
            .execute_batch(&ontology_sql)
            .map_err(|e| format!("Ontology load failed: {}", e))?;
        eprintln!(
            "[timing]   ontology dump (no indices): {:?}",
            t_dump.elapsed()
        );

        // Recreate the indices in one shot.
        let t_idx = std::time::Instant::now();
        let mut create_all = String::new();
        for (_, sql) in &index_defs {
            create_all.push_str(sql);
            create_all.push_str(";\n");
        }
        temp_conn.execute_batch(&create_all)?;
        eprintln!(
            "[timing]   recreate {} indices: {:?}",
            index_defs.len(),
            t_idx.elapsed()
        );
    }
    eprintln!("[timing] load dump into temp DB: {:?}", t.elapsed());

    let _temp_db_guard = TempFileGuard(&temp_db_path);

    let temp_catalog_conn =
        Connection::open_with_flags(&temp_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_semantic_catalog(&temp_catalog_conn, &project_root)
        .map_err(|error| format!("Semantic catalog verification failed: {error}"))?;
    drop(temp_catalog_conn);

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
                let (orig_conn, temp_conn) = conns.as_ref().map_err(|e| e.clone())?;
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

    // Count only facts from the latest (subject, predicate) transaction.
    // Retraction markers are part of that state, so MAX(tx) must be computed
    // over all rows before applying `retracted = 0`.
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
             WHERE subject IN (SELECT subject FROM dump_subjects)
             GROUP BY subject, predicate
         )
         SELECT COUNT(*)
         FROM triples t
         JOIN max_tx m ON t.subject = m.subject AND t.predicate = m.predicate AND t.tx = m.tx
         WHERE t.retracted = 0",
        [],
        |r| r.get(0),
    )?;

    let temp_triple_count: i64 =
        temp_conn.query_row("SELECT COUNT(*) FROM triples_current", [], |r| r.get(0))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn clean_seed_connection() -> Connection {
        let root = project_root();
        let core_assets = root.parent().unwrap().join("foundation-core/assets");
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&std::fs::read_to_string(core_assets.join("schema.sql")).unwrap())
            .unwrap();
        conn.execute_batch(&std::fs::read_to_string(core_assets.join("ontology.sql")).unwrap())
            .unwrap();
        conn
    }

    #[test]
    fn clean_seed_matches_current_runtime_contract_hashes_end_to_end() {
        let conn = clean_seed_connection();
        verify_semantic_catalog(&conn, &project_root()).unwrap();
    }

    #[test]
    fn stale_intent_in_clean_seed_is_rejected() {
        let conn = clean_seed_connection();
        conn.execute(
            "UPDATE triples SET object_value='mutated intent' WHERE subject='foundation:SemanticComponentDefinition_Action_v1' AND predicate='foundation:componentIntent' AND object_type='literal'",
            [],
        )
        .unwrap();
        let error = verify_semantic_catalog(&conn, &project_root()).unwrap_err();
        assert!(error.contains("Intent mismatch") || error.contains("Contract hash mismatch"));
    }

    #[test]
    fn stale_example_in_clean_seed_is_rejected() {
        let conn = clean_seed_connection();
        conn.execute(
            "UPDATE triples SET object_value='{}' WHERE subject='foundation:SemanticComponentDefinition_Action_v1' AND predicate='foundation:componentExamples' AND object_type='literal'",
            [],
        )
        .unwrap();
        let error = verify_semantic_catalog(&conn, &project_root()).unwrap_err();
        assert!(error.contains("Example mismatch") || error.contains("Contract hash mismatch"));
    }
}
