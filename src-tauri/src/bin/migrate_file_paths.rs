// One-shot migration: convert legacy `file://...absolute...` paths in
// foundation:filePath into portable relative form (e.g. `attachments/foo.pdf`).
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

    // Collect filePath triples that look like legacy absolute storage.
    let mut stmt = conn.prepare(
        "SELECT rowid, subject, object_value FROM triples \
         WHERE predicate = 'foundation:filePath' AND retracted = 0 AND object_value IS NOT NULL",
    )?;
    let rows: Vec<(i64, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut converted = 0usize;
    let mut skipped_already = 0usize;
    let mut skipped_external = 0usize;

    for (rowid, subject, stored) in rows {
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
            println!("  {} → {}", subject, portable);
        }
    }

    println!();
    println!("Converted:        {}", converted);
    println!("Already portable: {}", skipped_already);
    println!("External (kept):  {}", skipped_external);

    Ok(())
}
