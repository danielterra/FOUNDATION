//! Regenerates `assets/material_symbols_names.txt` inside a local checkout of
//! the `foundation-core` crate, from this app's `node_modules/material-symbols`.
//!
//! foundation-core is a standalone crate with no knowledge of npm or this app;
//! it only ships the committed asset. This bridge lives here, on the app side,
//! because only the app has `node_modules`. It reads the npm package and writes
//! the refreshed asset back into a foundation-core checkout so the change can be
//! committed there.
//!
//! Run from the Foundation project root:
//!   npm run update:material-symbols
//! or directly:
//!   cd scripts/update-material-symbols && cargo run --release
//!
//! Inputs (all optional, sensible defaults):
//!   $MATERIAL_SYMBOLS_DIR   path to the `material-symbols` npm package
//!                           (default: <app_root>/node_modules/material-symbols)
//!   $FOUNDATION_CORE_DIR    path to the foundation-core checkout, or pass it as
//!                           the first CLI argument
//!                           (default: sibling <app_root>/../foundation-core)
//!
//! After a successful run, commit the updated asset in the foundation-core repo.

use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let app_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("update-material-symbols must live under <app_root>/scripts/")
        .to_path_buf();

    let pkg_dir = std::env::var_os("MATERIAL_SYMBOLS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| app_root.join("node_modules").join("material-symbols"));

    let core_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("FOUNDATION_CORE_DIR").map(PathBuf::from))
        .unwrap_or_else(|| {
            app_root
                .parent()
                .map(|p| p.join("foundation-core"))
                .expect("app_root has no parent to locate sibling foundation-core")
        });

    let asset_path = core_dir.join("assets").join("material_symbols_names.txt");

    // A wrong core path would otherwise write the asset into a stray directory
    // that never reaches the crate's build — reject it up front.
    if !core_dir.join("Cargo.toml").is_file() || !core_dir.join("assets").is_dir() {
        eprintln!(
            "ERROR: '{}' does not look like a foundation-core checkout \
             (expected Cargo.toml and assets/).",
            core_dir.display()
        );
        eprintln!(
            "  Point at one via the first argument or $FOUNDATION_CORE_DIR, e.g.:"
        );
        eprintln!("    cargo run --release -- /path/to/foundation-core");
        std::process::exit(1);
    }

    let dts_path = pkg_dir.join("index.d.ts");
    let pkg_path = pkg_dir.join("package.json");

    let dts_content = std::fs::read_to_string(&dts_path).unwrap_or_else(|e| {
        eprintln!("ERROR: Cannot read {}: {}", dts_path.display(), e);
        eprintln!("  Run `npm install` in the Foundation app first, or set $MATERIAL_SYMBOLS_DIR.");
        std::process::exit(1);
    });

    let pkg_content = std::fs::read_to_string(&pkg_path).unwrap_or_else(|e| {
        eprintln!("ERROR: Cannot read {}: {}", pkg_path.display(), e);
        std::process::exit(1);
    });

    let version = pkg_content
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("\"version\"") {
                trimmed
                    .split(':')
                    .nth(1)
                    .map(|v| v.trim().trim_matches(|c| c == '"' || c == ',').to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            eprintln!("ERROR: version field not found in package.json");
            std::process::exit(1);
        });

    let mut names: Vec<&str> = dts_content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('"') && trimmed.ends_with("\",") {
                Some(&trimmed[1..trimmed.len() - 2])
            } else {
                None
            }
        })
        .collect();

    names.sort_unstable();
    names.dedup();

    let mut output = String::new();
    output.push_str(&format!("# material-symbols version: {}\n", version));
    output.push_str("# source: node_modules/material-symbols/index.d.ts\n");
    for name in &names {
        output.push_str(name);
        output.push('\n');
    }

    std::fs::write(&asset_path, &output).unwrap_or_else(|e| {
        eprintln!("ERROR: Cannot write {}: {}", asset_path.display(), e);
        std::process::exit(1);
    });

    eprintln!(
        "Updated {} — {} icons, version {}",
        asset_path.display(),
        names.len(),
        version
    );
    eprintln!("  Commit this asset in the foundation-core repo to publish the change.");
}
