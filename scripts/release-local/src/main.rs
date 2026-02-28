use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
enum BumpType {
    Major,
    Minor,
    Patch,
}

impl BumpType {
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "major" => Ok(BumpType::Major),
            "minor" => Ok(BumpType::Minor),
            "patch" => Ok(BumpType::Patch),
            _ => anyhow::bail!("Invalid bump type. Use: major, minor, or patch"),
        }
    }
}

#[derive(Debug)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            anyhow::bail!("Invalid version format. Expected: X.Y.Z");
        }

        Ok(Version {
            major: parts[0].parse().context("Invalid major version")?,
            minor: parts[1].parse().context("Invalid minor version")?,
            patch: parts[2].parse().context("Invalid patch version")?,
        })
    }

    fn bump(&mut self, bump_type: BumpType) {
        match bump_type {
            BumpType::Major => {
                self.major += 1;
                self.minor = 0;
                self.patch = 0;
            }
            BumpType::Minor => {
                self.minor += 1;
                self.patch = 0;
            }
            BumpType::Patch => {
                self.patch += 1;
            }
        }
    }

    fn to_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn main() -> Result<()> {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    println!("🔨 FOUNDATION Local Build");
    println!("=========================\n");

    // Get current version just for display
    println!("📖 Current version...");
    let current_version = get_current_version(&project_root)?;
    println!("   Version: {}\n", current_version.to_string());

    // Build release
    println!("🔨 Building Tauri release...");
    run_tauri_build(&project_root)?;
    println!("   ✅ Build completed\n");

    println!("🎉 Build successful!");
    println!("\n📦 Executable location:");
    println!("   macOS: src-tauri/target/release/bundle/macos/");
    println!("   Linux: src-tauri/target/release/bundle/appimage/");
    println!("   Windows: src-tauri/target/release/bundle/msi/");

    Ok(())
}

fn get_current_version(project_root: &PathBuf) -> Result<Version> {
    let package_json_path = project_root.join("package.json");
    let content = fs::read_to_string(&package_json_path)
        .context("Failed to read package.json")?;

    let json: serde_json::Value = serde_json::from_str(&content)
        .context("Failed to parse package.json")?;

    let version_str = json["version"]
        .as_str()
        .context("Version field not found in package.json")?;

    Version::from_str(version_str)
}

fn update_package_json(project_root: &PathBuf, version: &Version) -> Result<()> {
    let package_json_path = project_root.join("package.json");
    let content = fs::read_to_string(&package_json_path)?;

    let mut json: serde_json::Value = serde_json::from_str(&content)?;
    json["version"] = serde_json::Value::String(version.to_string());

    let updated_content = serde_json::to_string_pretty(&json)?;
    fs::write(&package_json_path, updated_content + "\n")?;

    Ok(())
}

fn update_cargo_toml(project_root: &PathBuf, version: &Version) -> Result<()> {
    let cargo_toml_path = project_root.join("src-tauri/Cargo.toml");
    let content = fs::read_to_string(&cargo_toml_path)?;

    let mut doc = content.parse::<toml_edit::DocumentMut>()
        .context("Failed to parse Cargo.toml")?;

    doc["package"]["version"] = toml_edit::value(version.to_string());

    fs::write(&cargo_toml_path, doc.to_string())?;

    Ok(())
}

fn update_software_release_ttl(project_root: &PathBuf, version: &Version) -> Result<()> {
    let ttl_path = project_root.join("core-ontology/SoftwareRelease.ttl");
    let content = fs::read_to_string(&ttl_path)?;

    let version_str = version.to_string();
    let version_iri = format!("FoundationRelease_{}_{}_{}",  version.major, version.minor, version.patch);
    let today = Utc::now().format("%Y-%m-%d").to_string();

    // Get changelog from git commits since last tag
    let changelog = get_changelog_from_git().unwrap_or_else(|_| format!("Release version {}", version_str));

    // Find the last release definition and add new one after it
    let new_release = format!(
        r#"
foundation:{} a foundation:SoftwareRelease , owl:NamedIndividual ;
    rdfs:label "FOUNDATION v{}" ;
    rdfs:comment "Release version {}" ;
    foundation:releaseOf foundation:FoundationProduct ;
    foundation:versionNumber "{}" ;
    foundation:licenseType "MIT" ;
    foundation:releaseDate "{}"^^xsd:date ;
    foundation:changelog "{}" .
"#,
        version_iri, version_str, version_str, version_str, today, changelog
    );

    // Append to end of file
    let updated_content = content.trim_end().to_string() + "\n" + &new_release;
    fs::write(&ttl_path, updated_content)?;

    Ok(())
}

fn get_changelog_from_git() -> Result<String> {
    // Get last tag
    let last_tag_output = Command::new("git")
        .args(&["describe", "--tags", "--abbrev=0"])
        .output()
        .context("Failed to get last tag")?;

    let last_tag = if last_tag_output.status.success() {
        String::from_utf8_lossy(&last_tag_output.stdout).trim().to_string()
    } else {
        // No tags yet, get all commits
        String::new()
    };

    // Get commits since last tag
    let log_args = if last_tag.is_empty() {
        vec!["log", "--pretty=format:%s", "--no-merges"]
    } else {
        vec!["log", &format!("{}..HEAD", last_tag), "--pretty=format:%s", "--no-merges"]
    };

    let output = Command::new("git")
        .args(&log_args)
        .output()
        .context("Failed to get git log")?;

    if !output.status.success() {
        anyhow::bail!("Git log failed");
    }

    let commits = String::from_utf8_lossy(&output.stdout);
    let changelog = commits
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("; ");

    if changelog.is_empty() {
        Ok("Minor updates and improvements".to_string())
    } else {
        Ok(changelog)
    }
}

fn run_update_models(project_root: &PathBuf) -> Result<()> {
    let update_models_dir = project_root.join("scripts/update-models");

    let output = Command::new("cargo")
        .args(&["run", "--release"])
        .current_dir(&update_models_dir)
        .output()
        .context("Failed to run update-models script")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Update models failed: {}", stderr);
    }

    Ok(())
}

fn check_clean_working_tree() -> Result<()> {
    let output = Command::new("git")
        .args(&["status", "--porcelain"])
        .output()
        .context("Failed to check git status")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Filter out expected changes (version files and models)
    let re = Regex::new(r"(?m)^\s*[MAD]\s+(package\.json|src-tauri/Cargo\.toml|core-ontology/.*\.ttl)$")?;
    let unexpected_changes: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.is_empty() && !re.is_match(line))
        .collect();

    if !unexpected_changes.is_empty() {
        anyhow::bail!(
            "Working tree has unexpected uncommitted changes:\n{}",
            unexpected_changes.join("\n")
        );
    }

    Ok(())
}

fn git_add_all() -> Result<()> {
    let output = Command::new("git")
        .args(&["add", "package.json", "src-tauri/Cargo.toml", "core-ontology/"])
        .output()
        .context("Failed to stage changes")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git add failed: {}", stderr);
    }

    Ok(())
}

fn git_commit(message: &str) -> Result<()> {
    let output = Command::new("git")
        .args(&["commit", "-m", message])
        .output()
        .context("Failed to commit")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git commit failed: {}", stderr);
    }

    Ok(())
}

fn git_tag(tag_name: &str, message: &str) -> Result<()> {
    let output = Command::new("git")
        .args(&["tag", "-a", tag_name, "-m", message])
        .output()
        .context("Failed to create tag")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git tag failed: {}", stderr);
    }

    Ok(())
}

fn run_tauri_build(project_root: &PathBuf) -> Result<()> {
    let output = Command::new("npm")
        .args(&["run", "tauri", "build"])
        .current_dir(project_root)
        .output()
        .context("Failed to run Tauri build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Tauri build failed: {}", stderr);
    }

    Ok(())
}
