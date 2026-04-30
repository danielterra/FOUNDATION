use std::path::PathBuf;
use tauri::AppHandle;

#[tauri::command]
pub async fn settings__is_folder_configured(app: AppHandle) -> bool {
    use tauri::Manager;
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

/// Salva a pasta sem reiniciar — usado na primeira configuração.
#[tauri::command]
pub async fn settings__save_foundation_dir(app: AppHandle, path: String) -> Result<(), String> {
    crate::config::set_foundation_dir(&app, PathBuf::from(&path))
}

/// Salva a pasta e reinicia o app — usado nas configurações após o setup.
#[tauri::command]
pub async fn settings__set_foundation_dir(app: AppHandle, path: String) -> Result<(), String> {
    crate::config::set_foundation_dir(&app, PathBuf::from(&path))?;
    app.restart();
}
