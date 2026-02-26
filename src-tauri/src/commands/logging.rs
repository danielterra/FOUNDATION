use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use chrono::Local;

#[derive(Debug, serde::Deserialize)]
pub struct LogEntry {
    level: String,
    message: String,
    timestamp: String,
}

fn get_log_file_path() -> Result<PathBuf, String> {
    // Use dirs::data_local_dir() which is cross-platform:
    // - macOS: ~/Library/Application Support
    // - Linux: ~/.local/share
    // - Windows: C:\Users\<User>\AppData\Local
    let data_dir = dirs::data_local_dir()
        .ok_or_else(|| "Failed to get app data directory".to_string())?;

    let app_dir = data_dir.join("org.w3id.foundation");

    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create app data dir: {}", e))?;

    Ok(app_dir.join("application.log"))
}

/// Internal function to write log to file
fn write_log(source: &str, level: &str, message: &str) -> Result<(), String> {
    let log_path = get_log_file_path()?;

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
pub fn log_backend(level: &str, message: &str) {
    if let Err(e) = write_log("BACKEND", level, message) {
        eprintln!("Failed to write backend log: {}", e);
    }
}


#[tauri::command]
pub fn log_frontend(level: String, message: String) -> Result<(), String> {
    write_log("FRONTEND", &level, &message)
}

#[tauri::command]
pub fn get_log_file_path_command() -> Result<String, String> {
    let path = get_log_file_path()?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn clear_logs() -> Result<(), String> {
    let log_path = get_log_file_path()?;

    if log_path.exists() {
        std::fs::remove_file(&log_path)
            .map_err(|e| format!("Failed to clear log file: {}", e))?;
    }

    Ok(())
}
