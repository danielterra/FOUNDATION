// App-crate owl façade: re-exports everything from foundation-core's owl module
// and adds the two tauri-dependent reactor workers (formula_worker, query_worker)
// that must live in the app crate because they depend on tauri::AppHandle/Emitter.
pub use foundation_core::owl::*;

// Re-export specific items that are shadowed by glob to ensure they are accessible.
pub use foundation_core::owl::{OwlError, Result};

pub mod formula_worker;
pub mod query_worker;

/// Initialize the database with Tauri AppHandle for progress emission.
/// This wrapper lives in the app crate because it depends on tauri::AppHandle.
pub fn initialize_with_progress(
    app: tauri::AppHandle,
) -> std::result::Result<(foundation_core::eavto::Connection, std::path::PathBuf), foundation_core::eavto::connection::DbError> {
    use tauri::Emitter;
    let foundation_dir = crate::config::get_foundation_dir(&app);
    let db_path = foundation_core::eavto::connection::get_db_path_for_dir(&foundation_dir)?;
    let app_clone = app.clone();
    let emit_fn = move |event: &str, payload: serde_json::Value| {
        let _ = app_clone.emit(event, payload);
    };
    let conn = foundation_core::eavto::connection::initialize_db_with_emit(&db_path, Some(emit_fn))?;
    Ok((conn, db_path))
}
