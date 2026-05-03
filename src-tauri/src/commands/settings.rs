use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

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

/// Copia o foundation.mcpb embarcado para a pasta Downloads do usuário e
/// abre o gerenciador de arquivos com o arquivo selecionado. O usuário
/// precisa arrastar o arquivo para a janela do Claude Desktop ou usar
/// Settings → Extensions → "Install Extension" — não há deep link oficial
/// nem associação `.mcpb` registrada no SO (especialmente na versão MSIX
/// no Windows).
#[tauri::command]
pub async fn settings__connect_claude_desktop(app: AppHandle) -> Result<String, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("não foi possível localizar resource_dir: {e}"))?;
    let bundled = resource_dir.join("foundation.mcpb");
    if !bundled.is_file() {
        return Err(format!(
            "foundation.mcpb não encontrado em {}. Gere o bundle com `cargo run --release --manifest-path scripts/build-mcpb/Cargo.toml`.",
            bundled.display()
        ));
    }

    let downloads = app
        .path()
        .download_dir()
        .map_err(|e| format!("não foi possível localizar a pasta Downloads: {e}"))?;
    let target = downloads.join("foundation.mcpb");

    std::fs::copy(&bundled, &target)
        .map_err(|e| format!("falha ao copiar para {}: {e}", target.display()))?;

    app.opener()
        .reveal_item_in_dir(&target)
        .map_err(|e| format!("falha ao abrir a pasta no Explorer: {e}"))?;

    Ok(target.to_string_lossy().to_string())
}
