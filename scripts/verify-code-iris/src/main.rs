use regex::Regex;
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::PathBuf;

const EXCLUDED_IRIS: &[&str] = &[
    // Installation-specific entities created at setup time (not seeded from ontology)
    "foundation:ThisFoundationInstance",
    "foundation:ThisUser",
    "foundation:ThisComputer",
    "foundation:ThisMemory",
    "foundation:ThisOperatingSystem",
    "foundation:ThisProcessor",
    "foundation:LocalAIAssistant",
    // Example/placeholder IRIs in API description strings, docstrings, or test fixtures
    "foundation:A",
    "foundation:B",
    "foundation:X",
    "foundation:ClassName",
    "foundation:Optional_If_Matched",
    "foundation:File_1234567890",
    "foundation:Project_42",
    "foundation:Task_1",
    "foundation:Task_A",
    "foundation:Task_B",
    "foundation:Task_123",
    "foundation:Task_456",
    "foundation:Task_1234567890",
    "foundation:Widget_inspector_Task_123",
    "foundation:bpmn_Process_123",
    "foundation:myProp",
    "foundation:propName",
    // Doc-only example properties (referenced by name in MCP tool descriptions / examples,
    // never as real predicates or class IRIs in production code paths)
    "foundation:amount",
    "foundation:price",
    "foundation:deadline",
    "foundation:hasItems",
    "foundation:hasTasks",
    "foundation:icon",
    "foundation:rrule",
    "foundation:StatusActive",
    "foundation:automation_Process",
    // Instances that exist in the DB but with origin 'ai' instead of an ontology origin.
    // TODO: move these to setup.rs so they are created with origin 'setup' and seeded properly.
    "foundation:ClaudeAIService",
    "foundation:IconLibrary_1772733525675",
    "foundation:SoftwareAgent_1773313705318",
    "foundation:Status_1772993026091",
    "foundation:Status_1773016842120",
    "foundation:Status_1773183515321",
    "foundation:Status_1776975614793",
    // Singleton + property added by the Blackboard refactor (v0.18.1).
    // Created at runtime via lazy code or MCP; origin is 'widget'/'ai' rather
    // than an ontology TTL. TODO: seed in setup.rs with origin 'setup'.
    "foundation:DefaultBlackboard",
    "foundation:forConversation",
];

const EXCLUDED_PREFIXES: &[&str] = &[
    "rdf:", "rdfs:", "owl:", "xsd:", "unit:", "currency:", "qudt:",
];

fn is_excluded(iri: &str) -> bool {
    if iri.ends_with('_') {
        return true;
    }
    if EXCLUDED_IRIS.contains(&iri) {
        return true;
    }
    EXCLUDED_PREFIXES.iter().any(|p| iri.starts_with(p))
}

fn is_test_path(path: &std::path::Path) -> bool {
    if path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "tests" || s.ends_with("_tests")
    }) {
        return true;
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.ends_with("_tests.rs") || name.starts_with("test_") {
            return true;
        }
    }
    false
}

/// Strip content inside `#[cfg(test)]` blocks using brace-depth tracking.
/// After `#[cfg(test)]`, skip everything up to and including the associated `{ ... }` block.
fn strip_test_blocks(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let remaining = &src[i..];
        if remaining.starts_with("#[cfg(test)]") {
            i += 12; // skip "#[cfg(test)]"
            // Skip ahead to the first '{' (handles `mod tests {`, `fn foo() {`, etc.)
            while i < bytes.len() && bytes[i] != b'{' {
                i += 1;
            }
            // Track brace depth to find the end of the block
            if i < bytes.len() {
                let mut depth = 1usize;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'{' => { depth += 1; i += 1; }
                        b'}' => { depth -= 1; i += 1; }
                        _ => { i += 1; }
                    }
                }
            }
        } else {
            let ch = src[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }

    out
}

/// Replace line comments (`//`) with spaces to avoid matching IRIs inside them.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        if let Some(pos) = line.find("//") {
            out.push_str(&line[..pos]);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn scan_rust_iris(src_dir: &PathBuf) -> BTreeSet<String> {
    let re = Regex::new(r"foundation:[A-Za-z_][A-Za-z0-9_]*").unwrap();
    let mut iris = BTreeSet::new();

    for entry in walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |x| x == "rs"))
    {
        if is_test_path(entry.path()) {
            continue;
        }

        let content = match std::fs::read_to_string(entry.path()) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let content = strip_test_blocks(&content);
        let content = strip_comments(&content);

        for m in re.find_iter(&content) {
            let iri = m.as_str().to_string();
            if !is_excluded(&iri) {
                iris.insert(iri);
            }
        }
    }

    iris
}

fn is_core_iri(conn: &Connection, iri: &str) -> bool {
    let class_exists: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM triples
                WHERE retracted = 0
                  AND subject = ?1
                  AND predicate = 'rdf:type'
                  AND object IN (
                    'owl:Class', 'rdfs:Class', 'owl:ObjectProperty',
                    'owl:DatatypeProperty', 'owl:AnnotationProperty', 'rdf:Property'
                  )
            )",
            [iri],
            |r| r.get(0),
        )
        .unwrap_or(false);

    if class_exists {
        return true;
    }

    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM triples t
            JOIN origins o ON t.origin_id = o.id
            WHERE t.retracted = 0
              AND t.subject = ?1
              AND t.predicate = 'rdf:type'
              AND (o.name LIKE 'foundation:ontology:%' OR o.name = 'core')
        )",
        [iri],
        |r| r.get(0),
    )
    .unwrap_or(false)
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
        .expect("scripts/verify-code-iris has no parent")
        .parent()
        .expect("scripts has no parent");
    let src_dir = project_root.join("src-tauri").join("src");

    eprintln!("Using database: {}", db_path.display());
    eprintln!("Scanning: {}", src_dir.display());

    let iris = scan_rust_iris(&src_dir);
    eprintln!("Found {} unique foundation: IRIs in Rust source.", iris.len());

    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA query_only = ON;")?;

    let mut missing: Vec<String> = Vec::new();
    for iri in &iris {
        if !is_core_iri(&conn, iri) {
            missing.push(iri.clone());
        }
    }

    if missing.is_empty() {
        eprintln!("OK: all {} IRIs are present in the core ontology.", iris.len());
    } else {
        eprintln!("\nFAIL: {} IRI(s) not found in core ontology:\n", missing.len());
        for iri in &missing {
            eprintln!("  MISSING: {}", iri);
        }
        eprintln!();
        std::process::exit(1);
    }

    Ok(())
}
