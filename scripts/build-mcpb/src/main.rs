use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn main() -> Result<()> {
    let repo_root = locate_repo_root()?;
    let mcpb_dir = repo_root.join("mcpb");
    let manifest_path = mcpb_dir.join("manifest.json");
    let package_json_path = repo_root.join("package.json");
    let output_dir = repo_root.join("src-tauri").join("resources");
    let output_path = output_dir.join("foundation.mcpb");

    let package_version = read_json_string(&package_json_path, "version")?;
    sync_manifest_version(&manifest_path, &package_version)?;

    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let file = File::create(&output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut entry_count = 0usize;
    for entry in WalkDir::new(&mcpb_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(&mcpb_dir)
            .with_context(|| format!("failed to strip prefix from {}", path.display()))?;
        let name = rel
            .to_str()
            .with_context(|| format!("non-UTF8 path: {}", rel.display()))?
            .replace('\\', "/");

        zip.start_file(&name, options)
            .with_context(|| format!("failed to start zip entry {name}"))?;
        let mut buf = Vec::new();
        File::open(path)
            .with_context(|| format!("failed to open {}", path.display()))?
            .read_to_end(&mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;
        zip.write_all(&buf)
            .with_context(|| format!("failed to write zip entry {name}"))?;
        entry_count += 1;
        println!("  + {name}");
    }
    zip.finish().context("failed to finalize zip")?;

    let size = fs::metadata(&output_path)?.len();
    println!(
        "\nfoundation.mcpb v{package_version} written: {} ({entry_count} files, {} bytes)",
        output_path.display(),
        size
    );
    Ok(())
}

fn locate_repo_root() -> Result<PathBuf> {
    let mut cur = std::env::current_dir()?;
    loop {
        if cur.join("package.json").is_file() && cur.join("src-tauri").is_dir() {
            return Ok(cur);
        }
        if !cur.pop() {
            bail!("could not locate repo root (looked for package.json + src-tauri)");
        }
    }
}

fn read_json_string(path: &Path, key: &str) -> Result<String> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let val: Value = serde_json::from_str(&raw)
        .with_context(|| format!("invalid JSON in {}", path.display()))?;
    val.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .with_context(|| format!("key '{key}' missing or not a string in {}", path.display()))
}

fn sync_manifest_version(manifest_path: &Path, version: &str) -> Result<()> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let mut val: Value = serde_json::from_str(&raw)
        .with_context(|| format!("invalid JSON in {}", manifest_path.display()))?;

    let current = val
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if current == version {
        return Ok(());
    }

    val["version"] = Value::String(version.to_string());
    let pretty = serde_json::to_string_pretty(&val)? + "\n";
    fs::write(manifest_path, pretty)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    println!("manifest.json version: {current} → {version}");
    Ok(())
}
