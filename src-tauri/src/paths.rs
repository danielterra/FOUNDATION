//! Centralized path helpers for the Foundation runtime directory.
//!
//! Stores and retrieves paths in a portable, relocatable form so the same DB
//! can move between machines (iCloud / OneDrive / fresh installs) without
//! breaking attachment references. The convention:
//!
//! - **At write time** we store relative paths like `attachments/foo.pdf`.
//! - **At read time** [`resolve_path`] joins them onto the *current*
//!   foundation_dir. Absolute paths and `file://` URIs are accepted as-is for
//!   backwards compatibility with DBs created before this change.
//!
//! The foundation_dir is captured once during DB initialization (see
//! `commands/setup.rs`) and used across the rest of the app via this module —
//! avoids re-reading config.json on every attachment lookup.
//!
//! There's no automatic in-place migration of existing absolute paths; legacy
//! values keep working through the lenient [`resolve_path`] semantics. New
//! attachments are stored relative.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static FOUNDATION_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Capture the foundation_dir at startup. Idempotent — only the first call wins.
pub fn set_foundation_dir(dir: PathBuf) {
    let _ = FOUNDATION_DIR.set(dir);
}

/// Returns the configured foundation_dir, or a sensible fallback when the
/// app hasn't initialized yet (e.g. tests, CLI tools).
pub fn foundation_dir() -> PathBuf {
    if let Some(dir) = FOUNDATION_DIR.get() {
        return dir.clone();
    }
    dirs::document_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Documents")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Foundation")
}

pub fn attachments_dir() -> PathBuf {
    foundation_dir().join("attachments")
}

/// Convert an absolute path to a portable relative form (e.g.
/// `attachments/file.pdf`). Returns an absolute path if the input lives
/// outside foundation_dir — this preserves the URI for genuinely external
/// files but lets us roam attachments between machines.
///
/// Always uses forward slashes in the relative form so paths written on
/// Windows are readable on macOS/Linux.
pub fn to_portable_path(absolute: &Path) -> String {
    match absolute.strip_prefix(foundation_dir()) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => absolute.to_string_lossy().into_owned(),
    }
}

/// Resolve a stored path string back to an absolute filesystem path.
///
/// Accepts (in priority order):
/// 1. Relative paths like `attachments/file.pdf` — joined onto foundation_dir.
/// 2. `file://` URIs — strips the prefix and returns the rest as-is.
/// 3. Absolute paths — returned as-is.
pub fn resolve_path(stored: &str) -> PathBuf {
    let stripped = stored.strip_prefix("file://").unwrap_or(stored);
    let p = PathBuf::from(stripped);
    if p.is_absolute() {
        p
    } else {
        foundation_dir().join(p)
    }
}
