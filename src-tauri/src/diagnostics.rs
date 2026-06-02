use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use chrono::Utc;

pub fn get_log_file_path() -> Result<PathBuf, String> {
    let data_dir = dirs::data_local_dir()
        .ok_or_else(|| "Failed to get app data directory".to_string())?;
    let app_dir = data_dir.join("org.w3id.foundation");
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create app data dir: {}", e))?;
    Ok(app_dir.join("application.log"))
}

fn write_log(source: &str, level: &str, message: &str) -> Result<(), String> {
    let log_path = get_log_file_path()?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file: {}", e))?;
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f UTC");
    let log_line = format!("[{}] [{}] [{}] {}\n", timestamp, source, level.to_uppercase(), message);
    file.write_all(log_line.as_bytes())
        .map_err(|e| format!("Failed to write to log file: {}", e))?;
    Ok(())
}

pub fn log_backend(level: &str, message: &str) {
    if let Err(e) = write_log("BACKEND", level, message) {
        eprintln!("Failed to write backend log: {}", e);
    }
}
