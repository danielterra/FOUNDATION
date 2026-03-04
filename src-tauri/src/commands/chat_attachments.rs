use crate::owl::DbExecutor;
use crate::owl::{Individual, Object};
use tauri::State;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use base64::Engine;
use sha2::{Sha256, Digest};

pub struct AttachmentData {
    pub mime_type: String,
    pub data: String,
    pub file_iri: String,
    pub file_name: String,
    pub token_estimate: usize,
}

const IMAGE_FALLBACK_TOKENS: usize = 1500;
const CLAUDE_MAX_IMAGE_DIM: usize = 1568;
const IMAGE_TILE_SIZE: usize = 512;
const TOKENS_PER_IMAGE_TILE: usize = 170;
const IMAGE_BASE_TOKENS: usize = 85;
const PDF_FALLBACK_TOKENS: usize = 2000;

lazy_static::lazy_static! {
    pub static ref PENDING_ATTACHMENTS: Arc<Mutex<HashMap<String, AttachmentData>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

#[tauri::command]
pub async fn chat__attach_file(
    file_path: String,
    file_name: String,
    mime_type: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let raw = tokio::fs::read(&file_path).await
        .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

    let token_estimate = if mime_type.starts_with("image/") {
        estimate_image_tokens(&raw)
    } else if mime_type == "application/pdf" {
        estimate_pdf_tokens(&raw)
    } else {
        0
    };

    let data = base64::engine::general_purpose::STANDARD.encode(&raw);

    let timestamp = chrono::Utc::now().timestamp_millis();
    let iri = format!("foundation:File_{}", timestamp);

    let permanent_path = {
        let attachments_dir = dirs::document_dir()
            .ok_or("Could not find documents directory")?
            .join("Foundation")
            .join("attachments");
        tokio::fs::create_dir_all(&attachments_dir).await
            .map_err(|e| format!("Failed to create attachments directory: {}", e))?;
        let safe_name = file_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        attachments_dir.join(format!("{}_{}", timestamp, safe_name))
    };
    tokio::fs::copy(&file_path, &permanent_path).await
        .map_err(|e| format!("Failed to copy file to attachments folder: {}", e))?;
    let permanent_path_str = permanent_path.to_string_lossy().into_owned();

    let hash = format!("sha256:{:x}", Sha256::digest(&raw));
    let size = raw.len() as i64;
    let file_type_iri = mime_to_file_type_iri(&mime_type).map(|s| s.to_string());
    let file_name_clone = file_name.clone();
    let hash_clone = hash.clone();
    let iri_clone = iri.clone();

    executor.write(move |conn| {
        let ind = Individual::new(&iri_clone);

        ind.assert(conn, "foundation:File", &file_name_clone, "insert_drive_file", "chat")
            .map_err(|e| format!("Failed to create File entity: {}", e))?;

        ind.add_property(conn, "foundation:fileName", vec![Object::Literal {
            value: file_name_clone.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "chat").map_err(|e| format!("Failed to set fileName: {}", e))?;

        ind.add_property(conn, "foundation:filePath", vec![Object::Literal {
            value: format!("file://{}", permanent_path_str),
            datatype: Some("xsd:anyURI".to_string()),
            language: None,
        }], "chat").map_err(|e| format!("Failed to set filePath: {}", e))?;

        ind.add_property(conn, "foundation:fileSize", vec![Object::Integer(size)], "chat")
            .map_err(|e| format!("Failed to set fileSize: {}", e))?;

        ind.add_property(conn, "foundation:fileHash", vec![Object::Literal {
            value: hash_clone,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "chat").map_err(|e| format!("Failed to set fileHash: {}", e))?;

        if let Some(ref ft_iri) = file_type_iri {
            ind.add_property(conn, "foundation:hasFileType",
                vec![Object::Iri(ft_iri.clone())], "chat")
                .map_err(|e| format!("Failed to set hasFileType: {}", e))?;
        }

        ind.add_property(conn, "foundation:uploadDate", vec![Object::DateTime(timestamp)], "chat")
            .map_err(|e| format!("Failed to set uploadDate: {}", e))?;

        Ok(iri_clone)
    }).await?;

    PENDING_ATTACHMENTS.lock().await.insert(iri.clone(), AttachmentData {
        mime_type,
        data,
        file_iri: iri.clone(),
        file_name: file_name.clone(),
        token_estimate,
    });

    super::log_backend("info", &format!(
        "[CHAT] Persisted File entity and registered attachment: {} ({})", file_name, iri
    ));

    Ok(iri)
}

/// Claude scales images to fit within a 1568×1568 bounding box, then divides them
/// into 512×512 tiles. Each tile costs 170 tokens plus 85 base overhead per image.
fn estimate_image_tokens(raw: &[u8]) -> usize {
    let cursor = std::io::Cursor::new(raw);
    let (width, height) = match image::ImageReader::new(cursor)
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
    {
        Some((w, h)) => (w as usize, h as usize),
        None => return IMAGE_FALLBACK_TOKENS,
    };

    let (w, h) = if width > CLAUDE_MAX_IMAGE_DIM || height > CLAUDE_MAX_IMAGE_DIM {
        let scale = (CLAUDE_MAX_IMAGE_DIM as f64 / width as f64)
            .min(CLAUDE_MAX_IMAGE_DIM as f64 / height as f64);
        ((width as f64 * scale) as usize, (height as f64 * scale) as usize)
    } else {
        (width, height)
    };

    let tiles = ((w + IMAGE_TILE_SIZE - 1) / IMAGE_TILE_SIZE)
        * ((h + IMAGE_TILE_SIZE - 1) / IMAGE_TILE_SIZE);
    tiles * TOKENS_PER_IMAGE_TILE + IMAGE_BASE_TOKENS
}

fn estimate_pdf_tokens(raw: &[u8]) -> usize {
    match pdf_extract::extract_text_from_mem(raw) {
        Ok(text) => super::chat_storage::tokenize_text(&text),
        Err(_) => PDF_FALLBACK_TOKENS,
    }
}

fn mime_to_file_type_iri(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        "image/jpeg" => Some("foundation:FileType_JPEG"),
        "image/png"  => Some("foundation:FileType_PNG"),
        "image/gif"  => Some("foundation:FileType_GIF"),
        "image/webp" => Some("foundation:FileType_WEBP"),
        "image/bmp"  => Some("foundation:FileType_BMP"),
        "image/tiff" => Some("foundation:FileType_TIFF"),
        "image/svg+xml" => Some("foundation:FileType_SVG"),
        "application/pdf" => Some("foundation:FileType_PDF"),
        "text/plain" => Some("foundation:FileType_TXT"),
        _ => None,
    }
}
