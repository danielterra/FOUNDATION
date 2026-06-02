use std::fs::OpenOptions;
use std::io::Write;
use chrono::Utc;

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct LogEntry {
    level: String,
    message: String,
    timestamp: String,
}

pub use crate::diagnostics::log_backend;

fn write_frontend_log(level: &str, message: &str) -> Result<(), String> {
    let log_path = crate::diagnostics::get_log_file_path()?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file: {}", e))?;
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f UTC");
    let log_line = format!("[{}] [FRONTEND] [{}] {}\n", timestamp, level.to_uppercase(), message);
    file.write_all(log_line.as_bytes())
        .map_err(|e| format!("Failed to write to log file: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn log_frontend(level: String, message: String) -> Result<(), String> {
    write_frontend_log(&level, &message)
}

#[tauri::command]
pub fn get_log_file_path_command() -> Result<String, String> {
    let path = crate::diagnostics::get_log_file_path()?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn clear_logs() -> Result<(), String> {
    let log_path = crate::diagnostics::get_log_file_path()?;
    if log_path.exists() {
        std::fs::remove_file(&log_path)
            .map_err(|e| format!("Failed to clear log file: {}", e))?;
    }
    Ok(())
}
