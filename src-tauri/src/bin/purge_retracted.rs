/// Purge script for retracted triples.
///
/// Permanently deletes all rows marked as retracted = 1 from the triples table,
/// then runs VACUUM to reclaim disk space.
///
/// Usage:
///   cargo run --bin purge_retracted             # dry-run (default)
///   cargo run --bin purge_retracted -- --delete  # permanently delete retracted rows

use rusqlite::Connection;
use std::path::PathBuf;

fn get_db_path() -> PathBuf {
    dirs::document_dir()
        .expect("Could not find Documents directory")
        .join("Foundation")
        .join("FOUNDATION.db")
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let delete_mode = std::env::args().any(|a| a == "--delete");

    let db_path = get_db_path();
    println!("Database: {}", db_path.display());
    if !db_path.exists() {
        eprintln!("Error: database not found at {}", db_path.display());
        std::process::exit(1);
    }

    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

    let total_triples: i64 =
        conn.query_row("SELECT COUNT(*) FROM triples", [], |r| r.get(0))?;
    let retracted_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM triples WHERE retracted = 1", [], |r| r.get(0))?;
    let db_size_kb: i64 = conn.query_row(
        "SELECT page_count * page_size / 1024 FROM pragma_page_count(), pragma_page_size()",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    println!(
        "Total triples : {}\nRetracted     : {} ({:.1}%)\nDatabase size : {} KB",
        total_triples,
        retracted_count,
        retracted_count as f64 / total_triples.max(1) as f64 * 100.0,
        db_size_kb,
    );

    if retracted_count == 0 {
        println!("\nNothing to purge.");
        return Ok(());
    }

    if !delete_mode {
        println!(
            "\n[DRY RUN] No changes made.\n\
             Run with --delete to permanently remove {} retracted rows and run VACUUM.",
            retracted_count
        );
        return Ok(());
    }

    println!("\nDeleting retracted triples...");
    let deleted = conn.execute("DELETE FROM triples WHERE retracted = 1", [])?;
    println!("Deleted {} rows.", deleted);

    println!("Running VACUUM...");
    conn.execute_batch("VACUUM;")?;

    let db_size_after_kb: i64 = conn.query_row(
        "SELECT page_count * page_size / 1024 FROM pragma_page_count(), pragma_page_size()",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    println!(
        "Done. Database size: {} KB → {} KB (freed {} KB).",
        db_size_kb,
        db_size_after_kb,
        db_size_kb - db_size_after_kb,
    );

    Ok(())
}
