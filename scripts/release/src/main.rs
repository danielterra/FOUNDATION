use anyhow::{Context, Result};
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

#[derive(Debug, Clone)]
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
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <major|minor|patch>", args[0]);
        std::process::exit(1);
    }

    let bump_type = BumpType::from_str(&args[1])?;

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    println!("🚀 FOUNDATION Release Script");
    println!("============================\n");

    // Step 1: Get current version from package.json
    println!("📖 Reading current version...");
    let current_version = get_current_version(&project_root)?;
    println!("   Current version: {}", current_version.to_string());

    // Step 2: Bump version
    let mut new_version = current_version.clone();
    new_version.bump(bump_type);
    println!("   New version: {} ({:?})\n", new_version.to_string(), bump_type);

    // Step 3: Update version in files
    println!("📝 Updating version in files...");
    update_package_json(&project_root, &new_version)?;
    println!("   ✅ package.json");

    update_cargo_toml(&project_root, &new_version)?;
    println!("   ✅ src-tauri/Cargo.toml\n");

    // Step 4: Update AI models
    println!("🤖 Updating AI models from Claude API...");
    run_update_models(&project_root)?;
    println!("   ✅ Models updated\n");

    // Step 5: Git operations
    println!("🔧 Git operations...");

    // Check for uncommitted changes
    check_clean_working_tree()?;

    // Stage changes
    git_add_all()?;
    println!("   ✅ Changes staged");

    // Commit
    let commit_msg = format!("chore: Bump version to {}", new_version.to_string());
    git_commit(&commit_msg)?;
    println!("   ✅ Committed: {}", commit_msg);

    // Create tag
    let tag_name = format!("v{}", new_version.to_string());
    git_tag(&tag_name, &format!("Release version {}", new_version.to_string()))?;
    println!("   ✅ Tagged: {}\n", tag_name);

    // Step 6: Push to remote
    println!("📤 Pushing to remote...");
    git_push()?;
    println!("   ✅ Pushed commits");

    git_push_tags()?;
    println!("   ✅ Pushed tags\n");

    println!("🎉 Release {} created and pushed successfully!", new_version.to_string());
    println!("\n✨ GitHub Actions will build the release automatically.");
    println!("   Check: https://github.com/danielterra/FOUNDATION/actions");

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

    // Use regex to replace version without reordering JSON
    let re = Regex::new(r#"("version"\s*:\s*")([^"]+)(")"#)?;
    let updated_content = re.replace(&content, |caps: &regex::Captures| {
        format!("{}{}{}", &caps[1], version.to_string(), &caps[3])
    });

    fs::write(&package_json_path, updated_content.as_ref())?;

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
    let re = Regex::new(r"(?m)^\s*[MAD]\s+(package\.json|src-tauri/Cargo\.toml|src-tauri/Cargo\.lock)$")?;
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
        .args(&["add", "-A"])
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

fn git_push() -> Result<()> {
    let output = Command::new("git")
        .args(&["push"])
        .output()
        .context("Failed to push commits")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git push failed: {}", stderr);
    }

    Ok(())
}

fn git_push_tags() -> Result<()> {
    let output = Command::new("git")
        .args(&["push", "--tags"])
        .output()
        .context("Failed to push tags")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git push tags failed: {}", stderr);
    }

    Ok(())
}
