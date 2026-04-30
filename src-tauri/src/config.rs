use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Serialize, Deserialize, Default)]
struct FoundationConfig {
    foundation_dir: Option<String>,
}

fn config_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("config.json"))
        .map_err(|e| format!("Não foi possível determinar o diretório de configuração: {}", e))
}

pub fn get_foundation_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(config_path) = config_file_path(app) {
        if let Ok(contents) = std::fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str::<FoundationConfig>(&contents) {
                if let Some(dir) = config.foundation_dir {
                    return PathBuf::from(dir);
                }
            }
        }
    }
    dirs::document_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Documents")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Foundation")
}

pub fn set_foundation_dir(app: &tauri::AppHandle, path: PathBuf) -> Result<(), String> {
    let config_path = config_file_path(app)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let config = FoundationConfig {
        foundation_dir: Some(path.to_string_lossy().to_string()),
    };
    let contents = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, contents).map_err(|e| e.to_string())?;
    Ok(())
}
