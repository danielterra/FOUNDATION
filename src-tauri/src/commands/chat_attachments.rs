use crate::owl::DbExecutor;
use crate::owl::{Individual, Object};
use tauri::State;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use base64::Engine;

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
    } else if mime_type.starts_with("text/") {
        super::chat_storage::tokenize_text(&String::from_utf8_lossy(&raw))
    } else {
        0
    };

    let data = base64::engine::general_purpose::STANDARD.encode(&raw);

    let timestamp = chrono::Utc::now().timestamp_millis();
    let csv_prefix = is_csv(&mime_type, &file_name);
    let iri = if csv_prefix {
        format!("foundation:CSVFile_{}", timestamp)
    } else {
        format!("foundation:File_{}", timestamp)
    };

    let permanent_path = {
        let attachments_dir = crate::paths::attachments_dir();
        tokio::fs::create_dir_all(&attachments_dir).await
            .map_err(|e| format!("Failed to create attachments directory: {}", e))?;
        let safe_name = crate::files::sanitize_filename(&file_name);
        attachments_dir.join(format!("{}_{}", timestamp, safe_name))
    };
    tokio::fs::copy(&file_path, &permanent_path).await
        .map_err(|e| format!("Failed to copy file to attachments folder: {}", e))?;
    // Portable path (e.g. "attachments/123_file.pdf") instead of the raw absolute
    // path so the DB can roam between machines.
    let stored_path = crate::paths::to_portable_path(&permanent_path);

    let hash = crate::files::sha256_hex(&raw);
    let size = raw.len() as i64;
    let csv_meta = if csv_prefix {
        let text = String::from_utf8_lossy(&raw);
        Some(parse_csv_metadata(&text))
    } else {
        None
    };
    let file_name_clone = file_name.clone();
    let mime_type_clone = mime_type.clone();
    let iri_clone = iri.clone();

    executor.write(move |conn| {
        let (class_iri, icon) = if csv_meta.is_some() {
            ("foundation:CSVFile", "csv")
        } else {
            ("foundation:File", "attach_file")
        };

        crate::files::assert_file_individual(conn, &crate::files::FileMetadata {
            iri: &iri_clone,
            class_iri,
            icon,
            file_name: &file_name_clone,
            stored_path: &stored_path,
            size,
            hash: &hash,
            mime_type: &mime_type_clone,
            timestamp_ms: timestamp,
            origin: "chat",
        })?;

        if let Some(ref csv) = csv_meta {
            let ind = Individual::new(&iri_clone);

            ind.add_property(conn, "foundation:csvDelimiter", vec![Object::Literal {
                value: csv.delimiter.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }], "chat").map_err(|e| format!("Failed to set csvDelimiter: {}", e))?;

            ind.add_property(conn, "foundation:csvRowCount",
                vec![Object::Integer(csv.row_count as i64)], "chat")
                .map_err(|e| format!("Failed to set csvRowCount: {}", e))?;

            if !csv.columns.is_empty() {
                let col_objects: Vec<Object> = csv.columns.iter().map(|c| Object::Literal {
                    value: c.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }).collect();
                ind.add_property(conn, "foundation:csvColumns", col_objects, "chat")
                    .map_err(|e| format!("Failed to set csvColumns: {}", e))?;
            }
        }

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

pub async fn save_camera_frame(base64_data: &str, index: usize) -> Result<String, String> {
    let raw = base64::engine::general_purpose::STANDARD.decode(base64_data)
        .map_err(|e| format!("Failed to decode camera frame: {}", e))?;

    let timestamp = chrono::Utc::now().timestamp_millis();
    let attachments_dir = crate::paths::attachments_dir();
    tokio::fs::create_dir_all(&attachments_dir).await
        .map_err(|e| format!("Failed to create attachments directory: {}", e))?;

    let path = attachments_dir.join(format!("camera_{}_{}.jpg", timestamp, index));
    tokio::fs::write(&path, &raw).await
        .map_err(|e| format!("Failed to write camera frame: {}", e))?;

    // Caller writes this string into foundation:filePath; return portable form.
    Ok(crate::paths::to_portable_path(&path))
}

/// Claude scales images to fit within a 1568×1568 bounding box, then divides them
/// into 512×512 tiles. Each tile costs 170 tokens plus 85 base overhead per image.
pub fn estimate_image_tokens(raw: &[u8]) -> usize {
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

fn is_csv(mime_type: &str, file_name: &str) -> bool {
    mime_type == "text/csv"
        || mime_type == "application/csv"
        || mime_type == "application/vnd.ms-excel"
        || file_name.to_lowercase().ends_with(".csv")
}

struct CsvMetadata {
    delimiter: String,
    columns: Vec<String>,
    row_count: usize,
}

fn parse_csv_metadata(content: &str) -> CsvMetadata {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());

    let header_line = match lines.next() {
        Some(l) => l,
        None => return CsvMetadata { delimiter: ",".to_string(), columns: vec![], row_count: 0 },
    };

    let delimiter = detect_delimiter(header_line);
    let columns = split_csv_line(header_line, delimiter)
        .into_iter()
        .map(|c| c.trim().trim_matches('"').trim_matches('\'').trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();

    let row_count = lines.count();

    CsvMetadata {
        delimiter: delimiter.to_string(),
        columns,
        row_count,
    }
}

fn detect_delimiter(line: &str) -> char {
    let candidates = [(',', 0usize), (';', 0), ('\t', 0)];
    let counts = candidates.map(|(delim, _)| (delim, line.chars().filter(|&c| c == delim).count()));
    counts.into_iter().max_by_key(|&(_, count)| count)
        .filter(|&(_, count)| count > 0)
        .map(|(delim, _)| delim)
        .unwrap_or(',')
}

fn split_csv_line(line: &str, delimiter: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in line.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c == delimiter && !in_quotes => {
                fields.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}
