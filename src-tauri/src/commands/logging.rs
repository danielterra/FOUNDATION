use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use chrono::Local;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, serde::Deserialize)]
pub struct LogEntry {
    level: String,
    message: String,
    timestamp: String,
}

fn get_log_file_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let app_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create app data dir: {}", e))?;

    Ok(app_dir.join("application.log"))
}

/// Internal function to write log to file
fn write_log<R: Runtime>(app: &AppHandle<R>, source: &str, level: &str, message: &str) -> Result<(), String> {
    let log_path = get_log_file_path(app)?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file: {}", e))?;

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let log_line = format!("[{}] [{}] [{}] {}\n", timestamp, source, level.to_uppercase(), message);

    file.write_all(log_line.as_bytes())
        .map_err(|e| format!("Failed to write to log file: {}", e))?;

    Ok(())
}

/// Public function to log from Rust backend
pub fn log_backend<R: Runtime>(app: &AppHandle<R>, level: &str, message: &str) {
    if let Err(e) = write_log(app, "BACKEND", level, message) {
        eprintln!("Failed to write backend log: {}", e);
    }
}


#[tauri::command]
pub fn log_frontend<R: Runtime>(app: AppHandle<R>, level: String, message: String) -> Result<(), String> {
    write_log(&app, "FRONTEND", &level, &message)
}

#[tauri::command]
pub fn get_log_file_path_command<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let path = get_log_file_path(&app)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn clear_logs<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let log_path = get_log_file_path(&app)?;

    if log_path.exists() {
        std::fs::remove_file(&log_path)
            .map_err(|e| format!("Failed to clear log file: {}", e))?;
    }

    Ok(())
}
