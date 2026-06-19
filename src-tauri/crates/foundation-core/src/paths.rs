//! Centralized path helpers for the Foundation runtime directory.
//!
//! Stores and retrieves paths in a portable form so the same DB can move
//! between machines (iCloud / OneDrive / fresh installs) without breaking
//! attachment references.
//!
//! Storage convention:
//!
//! - **Files in `attachments/`** are stored by **bare filename** only
//!   (e.g. `123_invoice.pdf`). Every attachment lives directly under
//!   `attachments/`, so the prefix is implicit.
//! - **External files** (outside foundation_dir) keep their absolute path.
//! - **Legacy values** like `attachments/foo.pdf` or `file:///abs/path` are
//!   still accepted on read so older DBs keep working until migrated.
//!
//! At read time, [`resolve_path`] normalizes all of the above into an absolute
//! native path the OS can open directly.
//!
//! The foundation_dir is captured once during DB initialization (see
//! `commands/setup.rs`).

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

static FOUNDATION_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Capture the foundation_dir at startup. Idempotent — only the first call wins.
/// Eagerly creates `attachments/` and `inbox/` so MCP clients (Claude Desktop's
/// Filesystem feature, custom agents) can drop files there before they exist as
/// attachments — `attach_file_to_individual` copies them into attachments/ and
/// deletes the source only after the file entity is asserted and linked.
pub fn set_foundation_dir(dir: PathBuf) {
    if FOUNDATION_DIR.set(dir.clone()).is_err() {
        return;
    }
    let _ = std::fs::create_dir_all(dir.join("attachments"));
    let _ = std::fs::create_dir_all(dir.join("inbox"));
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

/// Drop-zone for files about to be attached via `attach_file_to_individual`.
/// MCP clients (Claude Desktop's Filesystem feature, custom agents) write here;
/// the MCP tool copies the file into `attachments/` and removes the source
/// after the file entity is fully asserted and linked.
pub fn inbox_dir() -> PathBuf {
    foundation_dir().join("inbox")
}

/// Convert an absolute path to its stored form.
///
/// - Files inside `attachments/` collapse to **just their filename**
///   (`/.../attachments/foo.pdf` → `foo.pdf`).
/// - Anything else is returned as an absolute path.
pub fn to_portable_path(absolute: &Path) -> String {
    if let Ok(rel) = absolute.strip_prefix(attachments_dir()) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    absolute.to_string_lossy().into_owned()
}

/// Resolve a stored path string back to an absolute filesystem path with
/// native separators.
///
/// Accepts (in priority order):
/// 1. `file://` URIs — strip the prefix.
/// 2. Absolute paths — normalize separators and return.
/// 3. Legacy relative paths with a separator (`attachments/foo.pdf`) — join
///    onto foundation_dir.
/// 4. Bare filenames (`foo.pdf`) — join onto attachments_dir (the new storage
///    form for files written by foundation itself).
pub fn resolve_path(stored: &str) -> PathBuf {
    let stripped = stored.strip_prefix("file://").unwrap_or(stored);
    let p = PathBuf::from(stripped);

    let resolved = if p.is_absolute() {
        p
    } else if stripped.contains('/') || stripped.contains('\\') {
        foundation_dir().join(p)
    } else {
        attachments_dir().join(p)
    };

    normalize_separators(&resolved)
}

/// Reassemble a path so all separators match the platform's native one.
/// `PathBuf::join` preserves whatever separators the input fragments had,
/// which can produce mixed `C:\foo\bar/baz.pdf` on Windows — some shell
/// surfaces (Explorer's "open file") reject those. Walking the components
/// and rebuilding gives a clean native path.
fn normalize_separators(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in p.components() {
        match component {
            Component::Normal(seg) => {
                let s = seg.to_string_lossy();
                for part in s.split(['/', '\\']).filter(|x| !x.is_empty()) {
                    out.push(part);
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
