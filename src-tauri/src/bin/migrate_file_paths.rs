// One-shot migration: convert legacy `file://...absolute...` paths into
// portable relative form (e.g. `attachments/foo.pdf`).
//
// Covers both foundation:filePath (file entities) and foundation:hasIcon
// (custom file-based icons like a user's profile photo). hasIcon literals
// that aren't file paths (http://, https://, data:) are left untouched.
//
// Safe to re-run: rows already in portable form are skipped. Rows pointing
// outside foundation_dir are kept as absolute paths (they're external files,
// not roamable).
//
// Run: cargo run --release --bin migrate_file_paths
//   FOUNDATION_DB=path/to/FOUNDATION.db cargo run ...   # to override DB path

use rusqlite::{Connection, params};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path: PathBuf = std::env::var("FOUNDATION_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::document_dir()
                .or_else(|| dirs::home_dir().map(|h| h.join("Documents")))
                .expect("no Documents dir")
                .join("Foundation")
                .join("FOUNDATION.db")
        });

    let icloud_alt = dirs::home_dir().map(|h| {
        h.join("iCloudDrive")
            .join("Documents")
            .join("Foundation")
            .join("FOUNDATION.db")
    });

    let path = if db_path.exists() {
        db_path
    } else if let Some(p) = icloud_alt.filter(|p| p.exists()) {
        p
    } else {
        eprintln!("DB not found at {:?}", db_path);
        std::process::exit(2);
    };
    println!("Using DB: {}", path.display());

    let foundation_dir = path
        .parent()
        .expect("DB has no parent dir")
        .to_path_buf();
    println!("foundation_dir: {}", foundation_dir.display());

    let conn = Connection::open(&path)?;
    conn.busy_timeout(std::time::Duration::from_secs(30))?;

    // Collect filePath, hasIcon (literal), and iconKey triples that look like
    // legacy absolute storage. hasIcon stored as an IRI (foundation:icon-file-*)
    // keeps its actual path in iconKey — that variant is also covered here.
    let mut stmt = conn.prepare(
        "SELECT rowid, subject, predicate, object_value FROM triples \
         WHERE predicate IN ('foundation:filePath', 'foundation:hasIcon', 'foundation:iconKey') \
         AND retracted = 0 AND object_value IS NOT NULL",
    )?;
    let rows: Vec<(i64, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut converted = 0usize;
    let mut skipped_already = 0usize;
    let mut skipped_non_file = 0usize;
    let mut skipped_external = 0usize;

    for (rowid, subject, predicate, stored) in rows {
        // hasIcon literals can be http(s), data:, or file: URLs — only the file ones
        // are migration candidates. filePath only ever holds file paths.
        if stored.starts_with("http://")
            || stored.starts_with("https://")
            || stored.starts_with("data:")
        {
            skipped_non_file += 1;
            continue;
        }

        let stripped = stored.strip_prefix("file://").unwrap_or(&stored);
        let p = PathBuf::from(stripped);

        // Already relative (i.e. portable) — nothing to do.
        if !p.is_absolute() && !stored.starts_with("file://") {
            skipped_already += 1;
            continue;
        }

        // Try strict prefix match first (path inside this machine's foundation_dir).
        let portable = if let Ok(rel) = p.strip_prefix(&foundation_dir) {
            rel.to_string_lossy().replace('\\', "/")
        } else {
            // Fallback: many DBs roamed across machines (macOS → Windows via
            // iCloud), so the absolute prefix doesn't match anymore. If the
            // path contains a known Foundation sub-directory anchor, we can
            // safely extract the relative tail. The arrangement on disk is
            // identical between machines because the data folder syncs.
            let normalized = stripped.replace('\\', "/");
            let anchors = [
                "/Foundation/attachments/",
                "/Documents/Foundation/attachments/",
            ];
            let mut found_rel: Option<String> = None;
            for anchor in &anchors {
                if let Some(idx) = normalized.rfind(anchor) {
                    // Skip past the trailing slash of the anchor so we keep
                    // "attachments/file.pdf" (or whatever subdir).
                    let tail_start = idx + anchor.len() - "attachments/".len();
                    found_rel = Some(normalized[tail_start..].to_string());
                    break;
                }
            }
            match found_rel {
                Some(rel) => rel,
                None => {
                    if skipped_external < 5 {
                        println!("  [external] stored={:?} resolved={:?}", stored, p);
                    }
                    skipped_external += 1;
                    continue;
                }
            }
        };

        // In-place UPDATE is safe: this attribute is never queried by the new
        // path itself, and replacing the lexical value preserves tx/origin/etc.
        conn.execute(
            "UPDATE triples SET object_value = ?1 WHERE rowid = ?2",
            params![portable, rowid],
        )?;
        converted += 1;
        if converted <= 10 {
            println!("  [{}] {} → {}", predicate, subject, portable);
        }
    }

    println!();
    println!("Converted:        {}", converted);
    println!("Already portable: {}", skipped_already);
    println!("Non-file URLs:    {}", skipped_non_file);
    println!("External (kept):  {}", skipped_external);

    // Phase 2: flatten legacy IRI-based file icons into literals.
    // Old format: entity → hasIcon → foundation:icon-file-* → iconKey → "file:///..."
    // New format: entity → hasIcon → "attachments/foo.png"  (literal, portable)
    println!("\nPhase 2: flattening IRI-based file icons → literals");
    let iri_icon_rows: Vec<(i64, String, String)> = conn
        .prepare(
            "SELECT rowid, subject, object FROM triples \
             WHERE predicate = 'foundation:hasIcon' \
             AND object LIKE 'foundation:icon-file-%' \
             AND retracted = 0 AND object IS NOT NULL",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut converted_iri = 0usize;
    let mut skipped_iri = 0usize;
    for (rowid, subject, icon_iri) in iri_icon_rows {
        let key: Option<String> = conn.query_row(
            "SELECT object_value FROM triples \
             WHERE subject = ?1 AND predicate = 'foundation:iconKey' \
             AND retracted = 0 AND object_value IS NOT NULL LIMIT 1",
            rusqlite::params![&icon_iri],
            |r| r.get(0),
        ).ok();

        let Some(stored) = key else {
            println!("  [iri-icon] no iconKey for {} on {}, skipping", icon_iri, subject);
            skipped_iri += 1;
            continue;
        };

        if stored.starts_with("http://") || stored.starts_with("https://") || stored.starts_with("data:") {
            conn.execute(
                "UPDATE triples SET object_type = 'literal', object_value = ?1, object = NULL WHERE rowid = ?2",
                rusqlite::params![stored, rowid],
            )?;
            converted_iri += 1;
            continue;
        }

        let stripped = stored.strip_prefix("file://").unwrap_or(&stored);
        let p = PathBuf::from(stripped);

        let portable = if let Ok(rel) = p.strip_prefix(&foundation_dir) {
            rel.to_string_lossy().replace('\\', "/")
        } else {
            let normalized = stripped.replace('\\', "/");
            let anchors = ["/Foundation/attachments/", "/Documents/Foundation/attachments/"];
            let mut found_rel: Option<String> = None;
            for anchor in &anchors {
                if let Some(idx) = normalized.rfind(anchor) {
                    let tail_start = idx + anchor.len() - "attachments/".len();
                    found_rel = Some(normalized[tail_start..].to_string());
                    break;
                }
            }
            match found_rel {
                Some(rel) => rel,
                None => {
                    println!("  [iri-icon-external] {} on {}: {:?}", icon_iri, subject, stored);
                    skipped_iri += 1;
                    continue;
                }
            }
        };

        conn.execute(
            "UPDATE triples SET object_type = 'literal', object_value = ?1, object_datatype = 'xsd:string', object = NULL WHERE rowid = ?2",
            rusqlite::params![portable, rowid],
        )?;
        converted_iri += 1;
        println!("  [iri→literal] {} → {}", subject, portable);
    }

    println!("IRI icons flattened: {}", converted_iri);
    println!("IRI icons skipped:   {}", skipped_iri);

    // Phase 3: convert hasIcon triples stored as file:// IRIs (object column)
    // into portable literals. These were written by old code that classified
    // file:// values as IRIs instead of literals.
    println!("\nPhase 3: converting file:// IRI icons → portable literals");
    let file_iri_rows: Vec<(i64, String, String)> = conn
        .prepare(
            "SELECT rowid, subject, object FROM triples \
             WHERE predicate = 'foundation:hasIcon' \
             AND object LIKE 'file://%' \
             AND retracted = 0 AND object IS NOT NULL",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut converted_file_iri = 0usize;
    let mut skipped_file_iri = 0usize;
    for (rowid, subject, stored) in file_iri_rows {
        let stripped = stored.strip_prefix("file://").unwrap_or(&stored);
        let p = PathBuf::from(stripped);

        let portable = if let Ok(rel) = p.strip_prefix(&foundation_dir) {
            rel.to_string_lossy().replace('\\', "/")
        } else {
            let normalized = stripped.replace('\\', "/");
            let anchors = ["/Foundation/attachments/", "/Documents/Foundation/attachments/"];
            let mut found_rel: Option<String> = None;
            for anchor in &anchors {
                if let Some(idx) = normalized.rfind(anchor) {
                    let tail_start = idx + anchor.len() - "attachments/".len();
                    found_rel = Some(normalized[tail_start..].to_string());
                    break;
                }
            }
            match found_rel {
                Some(rel) => rel,
                None => {
                    println!("  [file-iri-external] {} on {}: {:?}", stored, subject, p);
                    skipped_file_iri += 1;
                    continue;
                }
            }
        };

        conn.execute(
            "UPDATE triples SET object_type = 'literal', object_value = ?1, object_datatype = 'xsd:string', object = NULL WHERE rowid = ?2",
            rusqlite::params![portable, rowid],
        )?;
        converted_file_iri += 1;
        println!("  [file-iri→literal] {} → {}", subject, portable);
    }

    println!("File IRI icons converted: {}", converted_file_iri);
    println!("File IRI icons skipped:   {}", skipped_file_iri);

    // Phase 4: collapse "attachments/foo.pdf" → "foo.pdf".
    // Every attachment lives directly under attachments/, so the prefix is
    // redundant — storing the bare filename makes resolution unambiguous and
    // eliminates the cross-platform separator-mixing bug (Windows Explorer
    // rejects paths like `C:\...\Foundation\attachments/foo.pdf`).
    println!("\nPhase 4: collapsing attachments/foo → foo");
    let prefix_rows: Vec<(i64, String, String, String)> = conn
        .prepare(
            "SELECT rowid, subject, predicate, object_value FROM triples \
             WHERE predicate IN ('foundation:filePath', 'foundation:hasIcon', 'foundation:iconKey') \
             AND retracted = 0 AND object_value IS NOT NULL \
             AND (object_value LIKE 'attachments/%' OR object_value LIKE 'attachments\\%')",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut collapsed = 0usize;
    let mut collapsed_skipped = 0usize;
    for (rowid, subject, predicate, stored) in prefix_rows {
        let normalized = stored.replace('\\', "/");
        let Some(rest) = normalized.strip_prefix("attachments/") else {
            collapsed_skipped += 1;
            continue;
        };
        // Subdir under attachments/ is unexpected; preserve it untouched and
        // log so the user can investigate. Filename-only is the new format.
        if rest.contains('/') {
            println!("  [subdir] keeping {} on {} ({}): {}", predicate, subject, rowid, stored);
            collapsed_skipped += 1;
            continue;
        }
        conn.execute(
            "UPDATE triples SET object_value = ?1 WHERE rowid = ?2",
            params![rest, rowid],
        )?;
        collapsed += 1;
        if collapsed <= 10 {
            println!("  [{}] {} → {}", predicate, subject, rest);
        }
    }
    println!("attachments/-prefix collapsed: {}", collapsed);
    println!("attachments/-prefix kept:      {}", collapsed_skipped);

    Ok(())
}
