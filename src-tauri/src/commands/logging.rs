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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tempfile::TempDir;
    use serial_test::serial;

    // Helper function to write log to a custom path for testing
    fn write_log_to_path(path: &PathBuf, source: &str, level: &str, message: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create log dir: {}", e))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("Failed to open log file: {}", e))?;

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let log_line = format!("[{}] [{}] [{}] {}\n", timestamp, source, level.to_uppercase(), message);

        file.write_all(log_line.as_bytes())
            .map_err(|e| format!("Failed to write to log file: {}", e))?;

        Ok(())
    }

    #[test]
    fn test_log_frontend_command() {
        let _temp_dir = TempDir::new().unwrap();
        let log_path = _temp_dir.path().join("test_frontend.log");

        // Test logging
        let result = write_log_to_path(&log_path, "FRONTEND", "info", "Test message");
        assert!(result.is_ok());

        // Verify log file was created
        assert!(log_path.exists());

        // Verify content
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("[FRONTEND]"));
        assert!(content.contains("[INFO]"));
        assert!(content.contains("Test message"));
    }

    #[test]
    fn test_log_backend_function() {
        let _temp_dir = TempDir::new().unwrap();
        let log_path = _temp_dir.path().join("test_backend.log");

        // Test logging from backend
        let result = write_log_to_path(&log_path, "BACKEND", "error", "Backend error");
        assert!(result.is_ok());

        // Verify log file was created
        assert!(log_path.exists());

        // Verify content
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("[BACKEND]"));
        assert!(content.contains("[ERROR]"));
        assert!(content.contains("Backend error"));
    }

    #[test]
    #[serial]
    fn test_get_log_file_path_command() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .unwrap();

        let result = get_log_file_path_command(app.handle().clone());
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.ends_with("application.log"));
    }

    #[test]
    #[serial]
    fn test_tauri_log_frontend_command() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .unwrap();

        // Clean up first and wait
        let log_path = get_log_file_path(app.handle()).unwrap();
        let _ = fs::remove_file(&log_path);
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Test the actual Tauri command
        let result = log_frontend(app.handle().clone(), "warn".to_string(), "Frontend warning".to_string());
        assert!(result.is_ok());

        // Small delay to ensure write completes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Verify
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("[FRONTEND]"));
        assert!(content.contains("[WARN]"));
        assert!(content.contains("Frontend warning"));

        // Cleanup
        let _ = fs::remove_file(&log_path);
    }

    #[test]
    #[serial]
    fn test_tauri_log_backend_function() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .unwrap();

        // Clean up first and wait
        let log_path = get_log_file_path(app.handle()).unwrap();
        let _ = fs::remove_file(&log_path);
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Test the actual backend logging function
        log_backend(app.handle(), "info", "Backend info message");

        // Small delay to ensure write completes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Verify
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("[BACKEND]"));
        assert!(content.contains("[INFO]"));
        assert!(content.contains("Backend info message"));

        // Cleanup
        let _ = fs::remove_file(&log_path);
    }

    #[test]
    #[serial]
    fn test_tauri_clear_logs_command() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .unwrap();

        let log_path = get_log_file_path(app.handle()).unwrap();
        let _ = fs::remove_file(&log_path);
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Create a log
        log_frontend(app.handle().clone(), "debug".to_string(), "Debug msg".to_string()).unwrap();
        assert!(log_path.exists());

        // Test the clear command
        let result = clear_logs(app.handle().clone());
        assert!(result.is_ok());
        assert!(!log_path.exists());
    }

    #[test]
    fn test_clear_logs() {
        let _temp_dir = TempDir::new().unwrap();
        let log_path = _temp_dir.path().join("test_clear.log");

        // Create a log first
        write_log_to_path(&log_path, "FRONTEND", "info", "Test").unwrap();
        assert!(log_path.exists());

        // Clear logs (manually remove file)
        let result = std::fs::remove_file(&log_path);
        assert!(result.is_ok());

        // Verify file was deleted
        assert!(!log_path.exists());
    }

    #[test]
    fn test_multiple_logs_append() {
        let _temp_dir = TempDir::new().unwrap();
        let log_path = _temp_dir.path().join("test_multiple.log");

        // Write multiple logs
        write_log_to_path(&log_path, "FRONTEND", "info", "First").unwrap();
        write_log_to_path(&log_path, "FRONTEND", "warn", "Second").unwrap();
        write_log_to_path(&log_path, "BACKEND", "error", "Third").unwrap();

        // Verify all logs are in file
        let content = fs::read_to_string(&log_path).unwrap();

        assert!(content.contains("First"), "Content should contain 'First': {}", content);
        assert!(content.contains("Second"), "Content should contain 'Second': {}", content);
        assert!(content.contains("Third"), "Content should contain 'Third': {}", content);
        assert!(content.contains("[FRONTEND]"));
        assert!(content.contains("[BACKEND]"));
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
