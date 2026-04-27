use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<()> {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    println!("🔨 FOUNDATION Local Build");
    println!("=========================\n");

    println!("📖 Current version...");
    let version = read_package_version(&project_root)?;
    println!("   Version: {version}\n");

    println!("🧹 Cleaning previous bundle output...");
    clean_bundle(&project_root)?;

    println!("\n🔨 Building Tauri release...");
    run_tauri_build(&project_root)?;

    println!("\n🎉 Build successful!");
    println!("\n📦 Bundle location: <cargo target-dir>/release/bundle/");
    Ok(())
}

fn read_package_version(project_root: &Path) -> Result<String> {
    let path = project_root.join("package.json");
    let content = fs::read_to_string(&path).context("Failed to read package.json")?;
    let json: serde_json::Value =
        serde_json::from_str(&content).context("Failed to parse package.json")?;
    json["version"]
        .as_str()
        .map(|s| s.to_string())
        .context("Version field not found in package.json")
}

fn run_tauri_build(project_root: &Path) -> Result<()> {
    let status = Command::new("npm")
        .args(["run", "tauri:build"])
        .current_dir(project_root)
        .status()
        .context("Failed to run Tauri build")?;

    if !status.success() {
        anyhow::bail!("Tauri build failed with exit code {:?}", status.code());
    }

    Ok(())
}

fn clean_bundle(project_root: &Path) -> Result<()> {
    let target_dir = resolve_cargo_target_dir(project_root)?;
    let bundle_dir = target_dir.join("release").join("bundle");
    if bundle_dir.exists() {
        fs::remove_dir_all(&bundle_dir)
            .with_context(|| format!("Failed to remove {}", bundle_dir.display()))?;
        println!("   Removed: {}", bundle_dir.display());
    } else {
        println!("   Nothing to clean: {}", bundle_dir.display());
    }
    Ok(())
}

fn resolve_cargo_target_dir(project_root: &Path) -> Result<PathBuf> {
    if let Ok(env_dir) = std::env::var("CARGO_TARGET_DIR") {
        if !env_dir.is_empty() {
            return Ok(PathBuf::from(env_dir));
        }
    }

    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(project_root.join("src-tauri"))
        .output()
        .context("Failed to invoke cargo metadata")?;

    if !output.status.success() {
        anyhow::bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse cargo metadata")?;
    let target_dir = metadata["target_directory"]
        .as_str()
        .context("target_directory not found in cargo metadata")?;
    Ok(PathBuf::from(target_dir))
}
