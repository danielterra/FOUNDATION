use futures_util::StreamExt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Emitter, Manager, State};

const MODEL_DOWNLOAD_URL: &str =
    "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf";
pub(crate) const MODEL_IDENTIFIER: &str = "gemma-4-E4B-it-Q4_K_M.gguf";
const MODEL_EXPECTED_SIZE: u64 = 5_267_070_912;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalModelAvailability {
    Ready { path: PathBuf, size_bytes: u64 },
    Incomplete { size_bytes: u64 },
    Missing,
}

impl LocalModelAvailability {
    pub(crate) fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    fn status_fields(&self) -> (bool, String, u64) {
        match self {
            Self::Ready { path, size_bytes } => {
                (true, path.to_string_lossy().into_owned(), *size_bytes)
            }
            Self::Incomplete { size_bytes } => (false, String::new(), *size_bytes),
            Self::Missing => (false, String::new(), 0),
        }
    }
}

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
    app.path()
        .app_data_dir()
        .ok()
        .map(|p| p.join("models").join(MODEL_IDENTIFIER))
}

fn resource_model_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .resource_dir()
        .ok()
        .filter(|p| p.as_os_str() != "")
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"))
        .join("models")
        .join(MODEL_IDENTIFIER)
}

pub fn resolve_model_path(app: &AppHandle) -> std::path::PathBuf {
    resolve_model_path_from_paths(user_model_path(app).as_deref(), &resource_model_path(app))
}

fn resolve_model_path_from_paths(user_path: Option<&Path>, resource_path: &Path) -> PathBuf {
    user_path
        .filter(|path| path.exists())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| resource_path.to_path_buf())
}

pub(crate) fn local_model_availability(app: &AppHandle) -> LocalModelAvailability {
    inspect_local_model_paths(user_model_path(app).as_deref(), &resource_model_path(app))
}

fn inspect_local_model_paths(
    user_path: Option<&Path>,
    resource_path: &Path,
) -> LocalModelAvailability {
    if let Some(path) = user_path {
        if path.exists() {
            let size_bytes = model_file_size(path).unwrap_or_default();
            return if is_valid_gguf_model(path, size_bytes) {
                LocalModelAvailability::Ready {
                    path: path.to_path_buf(),
                    size_bytes,
                }
            } else {
                LocalModelAvailability::Incomplete { size_bytes }
            };
        }
    }

    if resource_path.exists() {
        let size_bytes = model_file_size(resource_path).unwrap_or_default();
        if is_valid_gguf_model(resource_path, size_bytes) {
            return LocalModelAvailability::Ready {
                path: resource_path.to_path_buf(),
                size_bytes,
            };
        }
        let mut incomplete_size = size_bytes;

        if let Some(path) = user_path {
            let partial_path = path.with_extension("gguf.part");
            if let Some(partial_size) = model_file_size(&partial_path) {
                incomplete_size = incomplete_size.max(partial_size);
            }
        }

        return LocalModelAvailability::Incomplete {
            size_bytes: incomplete_size,
        };
    }

    let mut incomplete_size: Option<u64> = None;

    if let Some(path) = user_path {
        let partial_path = path.with_extension("gguf.part");
        if let Some(size_bytes) = model_file_size(&partial_path) {
            incomplete_size =
                Some(incomplete_size.map_or(size_bytes, |current| current.max(size_bytes)));
        }
    }

    match incomplete_size {
        Some(size_bytes) => LocalModelAvailability::Incomplete { size_bytes },
        None => LocalModelAvailability::Missing,
    }
}

fn model_file_size(path: &Path) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata.is_file().then_some(metadata.len())
}

fn is_valid_gguf_model(path: &Path, size_bytes: u64) -> bool {
    if size_bytes != MODEL_EXPECTED_SIZE {
        return false;
    }
    let mut header = [0; 4];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && &header == b"GGUF"
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__get_local_model_status(
    app: AppHandle,
    download_state: State<'_, LocalModelDownloadState>,
) -> Result<serde_json::Value, String> {
    let (exists, path, size_bytes) = local_model_availability(&app).status_fields();

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
                let _ = app_clone.emit(
                    "local-model-download-complete",
                    serde_json::json!({
                        "path": download_path.to_string_lossy()
                    }),
                );
            }
            Err(ref e) if e == "cancelled" => {
                let _ = app_clone.emit(
                    "local-model-download-error",
                    serde_json::json!({
                        "error": "Download cancelled",
                        "cancelled": true
                    }),
                );
            }
            Err(e) => {
                let _ = app_clone.emit(
                    "local-model-download-error",
                    serde_json::json!({
                        "error": e
                    }),
                );
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
        std::fs::create_dir_all(parent).map_err(|e| format!("Error creating directory: {}", e))?;
    }

    // Keep partial file alongside final path to support resume
    let partial_path = dest.with_extension("gguf.part");

    let existing_size = if partial_path.exists() {
        std::fs::metadata(&partial_path)
            .map(|m| m.len())
            .unwrap_or(0)
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

    let response = req
        .send()
        .await
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
        std::fs::File::create(&partial_path).map_err(|e| format!("Error creating file: {}", e))?
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
        file.write_all(&chunk)
            .map_err(|e| format!("Error writing file: {}", e))?;
        downloaded += chunk.len() as u64;

        if last_emit.elapsed().as_millis() >= 250 {
            let percentage = if total_bytes > 0 {
                (downloaded as f64 / total_bytes as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            let _ = app.emit(
                "local-model-download-progress",
                serde_json::json!({
                    "downloadedBytes": downloaded,
                    "totalBytes": total_bytes,
                    "percentage": percentage,
                }),
            );
            last_emit = std::time::Instant::now();
        }
    }

    file.flush()
        .map_err(|e| format!("Error finalizing write: {}", e))?;
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

#[cfg(test)]
mod tests {
    use super::{
        inspect_local_model_paths, resolve_model_path_from_paths, LocalModelAvailability,
        MODEL_EXPECTED_SIZE, MODEL_IDENTIFIER,
    };
    use std::path::PathBuf;

    fn test_directory(name: &str) -> PathBuf {
        let unique = format!(
            "foundation-local-model-{}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            name
        );
        std::env::temp_dir().join(unique)
    }

    fn write_sparse_model(path: &std::path::Path, size_bytes: u64) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::File::create(path).unwrap();
        std::io::Write::write_all(&mut file, b"GGUF").unwrap();
        file.set_len(size_bytes).unwrap();
    }

    #[test]
    fn complete_gguf_file_is_ready() {
        let directory = test_directory("ready");
        let user_path = directory.join("models").join(MODEL_IDENTIFIER);
        let resource_path = directory.join("resources").join(MODEL_IDENTIFIER);
        write_sparse_model(&user_path, MODEL_EXPECTED_SIZE);

        let availability = inspect_local_model_paths(Some(&user_path), &resource_path);

        assert_eq!(
            availability,
            LocalModelAvailability::Ready {
                path: user_path,
                size_bytes: MODEL_EXPECTED_SIZE,
            }
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_existing_user_model_does_not_fall_back_to_valid_resource_model() {
        let directory = test_directory("invalid-user-model");
        let user_path = directory.join("models").join(MODEL_IDENTIFIER);
        let resource_path = directory.join("resources").join(MODEL_IDENTIFIER);
        write_sparse_model(&user_path, 16);
        write_sparse_model(&resource_path, MODEL_EXPECTED_SIZE);

        let selected_path = resolve_model_path_from_paths(Some(&user_path), &resource_path);
        let availability = inspect_local_model_paths(Some(&user_path), &resource_path);

        assert_eq!(selected_path, user_path);
        assert_eq!(
            availability,
            LocalModelAvailability::Incomplete { size_bytes: 16 }
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_user_model_uses_valid_resource_model() {
        let directory = test_directory("missing-user-model");
        let user_path = directory.join("models").join(MODEL_IDENTIFIER);
        let resource_path = directory.join("resources").join(MODEL_IDENTIFIER);
        write_sparse_model(&resource_path, MODEL_EXPECTED_SIZE);

        let selected_path = resolve_model_path_from_paths(Some(&user_path), &resource_path);
        let availability = inspect_local_model_paths(Some(&user_path), &resource_path);

        assert_eq!(selected_path, resource_path);
        assert!(
            matches!(availability, LocalModelAvailability::Ready { path, .. } if path == selected_path)
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn partial_download_is_not_ready() {
        let directory = test_directory("partial");
        let user_path = directory.join("models").join(MODEL_IDENTIFIER);
        let partial_path = user_path.with_extension("gguf.part");
        let resource_path = directory.join("resources").join(MODEL_IDENTIFIER);
        std::fs::create_dir_all(partial_path.parent().unwrap()).unwrap();
        std::fs::write(&partial_path, b"partial GGUF download").unwrap();

        let availability = inspect_local_model_paths(Some(&user_path), &resource_path);

        assert_eq!(
            availability,
            LocalModelAvailability::Incomplete {
                size_bytes: b"partial GGUF download".len() as u64,
            }
        );
        assert!(!availability.is_ready());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn truncated_installed_model_is_not_ready() {
        let directory = test_directory("truncated");
        let user_path = directory.join("models").join(MODEL_IDENTIFIER);
        let resource_path = directory.join("resources").join(MODEL_IDENTIFIER);
        write_sparse_model(&user_path, 16);

        let availability = inspect_local_model_paths(Some(&user_path), &resource_path);

        assert_eq!(
            availability,
            LocalModelAvailability::Incomplete { size_bytes: 16 }
        );
        assert!(!availability.is_ready());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
