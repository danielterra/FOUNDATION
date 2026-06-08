use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tauri::{AppHandle, Emitter, Manager, State};
use futures_util::StreamExt;

const MODEL_DOWNLOAD_URL: &str = "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf";
const MODEL_IDENTIFIER: &str = "gemma-4-E4B-it-Q4_K_M.gguf";
const MODEL_EXPECTED_SIZE: u64 = 5_267_070_912;

pub struct LocalModelDownloadState {
    pub is_downloading: Arc<AtomicBool>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl Default for LocalModelDownloadState {
    fn default() -> Self {
        Self {
            is_downloading: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn user_model_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|p| p.join("models").join(MODEL_IDENTIFIER))
}

fn resource_model_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().resource_dir()
        .ok()
        .filter(|p| p.as_os_str() != "")
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"))
        .join("models")
        .join(MODEL_IDENTIFIER)
}

pub fn resolve_model_path(app: &AppHandle) -> std::path::PathBuf {
    if let Some(p) = user_model_path(app) {
        if p.exists() { return p; }
    }
    resource_model_path(app)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__get_local_model_status(
    app: AppHandle,
    download_state: State<'_, LocalModelDownloadState>,
) -> Result<serde_json::Value, String> {
    let user_path = user_model_path(&app);
    let resource_path = resource_model_path(&app);

    let (exists, path, size_bytes) =
        if let Some(ref p) = user_path {
            if p.exists() {
                let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                (true, p.to_string_lossy().into_owned(), size)
            } else if resource_path.exists() {
                let size = std::fs::metadata(&resource_path).map(|m| m.len()).unwrap_or(0);
                (true, resource_path.to_string_lossy().into_owned(), size)
            } else {
                // Check for partial download
                let partial = p.with_extension("gguf.part");
                let partial_size = if partial.exists() {
                    std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0)
                } else {
                    0
                };
                (false, String::new(), partial_size)
            }
        } else if resource_path.exists() {
            let size = std::fs::metadata(&resource_path).map(|m| m.len()).unwrap_or(0);
            (true, resource_path.to_string_lossy().into_owned(), size)
        } else {
            (false, String::new(), 0)
        };

    let size_human = if exists && size_bytes > 0 {
        format!("{:.1} GB", size_bytes as f64 / 1_073_741_824.0)
    } else {
        "~4.9 GB".to_string()
    };

    let is_downloading = download_state.is_downloading.load(Ordering::Relaxed);

    Ok(serde_json::json!({
        "identifier": MODEL_IDENTIFIER,
        "exists": exists,
        "path": path,
        "sizeBytes": size_bytes,
        "sizeHuman": size_human,
        "isDownloading": is_downloading,
    }))
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__download_local_model(
    app: AppHandle,
    download_state: State<'_, LocalModelDownloadState>,
) -> Result<(), String> {
    if download_state.is_downloading.swap(true, Ordering::SeqCst) {
        return Err("Download already in progress".to_string());
    }

    download_state.cancel_flag.store(false, Ordering::SeqCst);

    let is_downloading = download_state.is_downloading.clone();
    let cancel_flag = download_state.cancel_flag.clone();

    let download_path = user_model_path(&app)
        .ok_or_else(|| "Could not determine the data directory".to_string())?;

    let app_clone = app.clone();
    tokio::spawn(async move {
        let result = do_download(&app_clone, &download_path, cancel_flag).await;
        is_downloading.store(false, Ordering::SeqCst);

        match result {
            Ok(_) => {
                let _ = app_clone.emit("local-model-download-complete", serde_json::json!({
                    "path": download_path.to_string_lossy()
                }));
            }
            Err(ref e) if e == "cancelled" => {
                let _ = app_clone.emit("local-model-download-error", serde_json::json!({
                    "error": "Download cancelled",
                    "cancelled": true
                }));
            }
            Err(e) => {
                let _ = app_clone.emit("local-model-download-error", serde_json::json!({
                    "error": e
                }));
            }
        }
    });

    Ok(())
}

async fn do_download(
    app: &AppHandle,
    dest: &std::path::Path,
    cancel_flag: Arc<AtomicBool>,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Error creating directory: {}", e))?;
    }

    // Keep partial file alongside final path to support resume
    let partial_path = dest.with_extension("gguf.part");

    let existing_size = if partial_path.exists() {
        std::fs::metadata(&partial_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("Error creating HTTP client: {}", e))?;

    let mut req = client.get(MODEL_DOWNLOAD_URL);
    if existing_size > 0 {
        req = req.header("Range", format!("bytes={}-", existing_size));
    }

    let response = req.send().await
        .map_err(|e| format!("Connection error: {}", e))?;

    let status = response.status();
    if !status.is_success() && status.as_u16() != 206 {
        return Err(format!("Server returned: {}", status));
    }

    let is_resume = status.as_u16() == 206;
    let content_len = response.content_length().unwrap_or(0);
    let total_bytes = if is_resume {
        existing_size + content_len
    } else if content_len > 0 {
        content_len
    } else {
        MODEL_EXPECTED_SIZE
    };

    let mut file = if is_resume && existing_size > 0 {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&partial_path)
            .map_err(|e| format!("Error opening partial file: {}", e))?
    } else {
        std::fs::File::create(&partial_path)
            .map_err(|e| format!("Error creating file: {}", e))?
    };

    let mut downloaded = if is_resume { existing_size } else { 0 };
    let mut stream = response.bytes_stream();
    let mut last_emit = std::time::Instant::now();

    use std::io::Write;
    while let Some(chunk) = stream.next().await {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }

        let chunk = chunk.map_err(|e| format!("Error receiving data: {}", e))?;
        file.write_all(&chunk).map_err(|e| format!("Error writing file: {}", e))?;
        downloaded += chunk.len() as u64;

        if last_emit.elapsed().as_millis() >= 250 {
            let percentage = if total_bytes > 0 {
                (downloaded as f64 / total_bytes as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            let _ = app.emit("local-model-download-progress", serde_json::json!({
                "downloadedBytes": downloaded,
                "totalBytes": total_bytes,
                "percentage": percentage,
            }));
            last_emit = std::time::Instant::now();
        }
    }

    file.flush().map_err(|e| format!("Error finalizing write: {}", e))?;
    drop(file);

    std::fs::rename(&partial_path, dest)
        .map_err(|e| format!("Error moving file to destination: {}", e))?;

    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__cancel_local_model_download(
    download_state: State<'_, LocalModelDownloadState>,
) -> Result<(), String> {
    download_state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}
