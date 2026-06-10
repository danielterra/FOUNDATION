use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::owl::{DbExecutor, Individual, Object};

const HISTORY_RETENTION_DAYS_SETTING_IRI: &str = "foundation:SoftwareSetting_1781110491934";

#[tauri::command]
pub async fn settings__is_folder_configured(app: AppHandle) -> bool {
    let Ok(config_path) = app.path().app_config_dir().map(|d| d.join("config.json")) else {
        return false;
    };
    if let Ok(contents) = std::fs::read_to_string(&config_path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents) {
            return val.get("foundation_dir").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
        }
    }
    false
}

#[tauri::command]
pub async fn settings__get_foundation_dir(app: AppHandle) -> Result<String, String> {
    let dir = crate::config::get_foundation_dir(&app);
    Ok(dir.to_string_lossy().to_string())
}

/// Saves the folder without restarting — used during initial configuration.
#[tauri::command]
pub async fn settings__save_foundation_dir(app: AppHandle, path: String) -> Result<(), String> {
    crate::config::set_foundation_dir(&app, PathBuf::from(&path))
}

/// Saves the folder and restarts the app — used in settings after setup.
#[tauri::command]
pub async fn settings__set_foundation_dir(app: AppHandle, path: String) -> Result<(), String> {
    crate::config::set_foundation_dir(&app, PathBuf::from(&path))?;
    app.restart();
}

/// Copies the bundled foundation.mcpb to the user's Downloads folder and
/// opens the file manager with the file selected. The user
/// needs to drag the file into the Claude Desktop window or use
/// Settings → Extensions → "Install Extension" — there is no official deep link
/// nor `.mcpb` association registered in the OS (especially in the MSIX
/// build on Windows).
#[tauri::command]
pub async fn settings__connect_claude_desktop(app: AppHandle) -> Result<String, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("could not locate resource_dir: {e}"))?;
    let bundled = resource_dir.join("foundation.mcpb");
    if !bundled.is_file() {
        return Err(format!(
            "foundation.mcpb not found in {}. Generate the bundle with `cargo run --release --manifest-path scripts/build-mcpb/Cargo.toml`.",
            bundled.display()
        ));
    }

    let downloads = app
        .path()
        .download_dir()
        .map_err(|e| format!("could not locate the Downloads folder: {e}"))?;
    let target = downloads.join("foundation.mcpb");

    std::fs::copy(&bundled, &target)
        .map_err(|e| format!("failed to copy to {}: {e}", target.display()))?;

    app.opener()
        .reveal_item_in_dir(&target)
        .map_err(|e| format!("failed to open the folder in Explorer: {e}"))?;

    Ok(target.to_string_lossy().to_string())
}

/// Returns the currently configured history retention period in days (0 = never purge).
#[tauri::command]
#[allow(non_snake_case)]
pub async fn settings__get_history_retention_days(
    executor: State<'_, DbExecutor>,
) -> Result<u32, String> {
    executor.read(|conn| {
        let days = Individual::get(conn, HISTORY_RETENTION_DAYS_SETTING_IRI)
            .ok()
            .flatten()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| v.as_literal())
                .and_then(|v| v.parse::<u32>().ok()))
            .unwrap_or(7);
        Ok(days)
    }).await
}

/// Persists the history retention period in days (0 = never purge).
#[tauri::command]
#[allow(non_snake_case)]
pub async fn settings__set_history_retention_days(
    days: u32,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        let setting = Individual::get(conn, HISTORY_RETENTION_DAYS_SETTING_IRI)
            .map_err(|e| format!("Failed to get HistoryRetentionDaysSetting: {}", e))?
            .ok_or_else(|| "HistoryRetentionDaysSetting not found".to_string())?;

        setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
            value: days.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "settings").map_err(|e| format!("Failed to update settingValue: {}", e))?;

        Ok(String::new())
    }).await?;
    Ok(())
}
