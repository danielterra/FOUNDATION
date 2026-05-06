use rusqlite::Connection;
use sha2::{Sha256, Digest};

use crate::owl::{Individual, Object};

pub fn mime_to_file_type_iri(mime_type: &str) -> Option<&'static str> {
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
        "text/csv" => Some("foundation:FileType_CSV"),
        _ => None,
    }
}

pub fn mime_from_extension(file_name: &str) -> &'static str {
    let ext = std::path::Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    match ext.as_deref() {
        Some("pdf")  => "application/pdf",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png")  => "image/png",
        Some("gif")  => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp")  => "image/bmp",
        Some("tif") | Some("tiff") => "image/tiff",
        Some("svg")  => "image/svg+xml",
        Some("txt") | Some("md") | Some("log") => "text/plain",
        Some("csv")  => "text/csv",
        Some("json") => "application/json",
        Some("xml")  => "application/xml",
        _ => "application/octet-stream",
    }
}

pub fn sha256_hex(raw: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(raw))
}

pub fn sanitize_filename(file_name: &str) -> String {
    file_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

#[derive(Clone, Copy)]
pub struct FileMetadata<'a> {
    pub iri: &'a str,
    pub class_iri: &'a str,
    pub icon: &'a str,
    pub file_name: &'a str,
    pub stored_path: &'a str,
    pub size: i64,
    pub hash: &'a str,
    pub mime_type: &'a str,
    pub timestamp_ms: i64,
    pub origin: &'a str,
}

pub fn assert_file_individual(
    conn: &mut Connection,
    meta: &FileMetadata<'_>,
) -> Result<(), String> {
    let ind = Individual::new(meta.iri);
    let FileMetadata { class_iri, icon, file_name, stored_path, size, hash, mime_type, timestamp_ms, origin, .. } = *meta;

    ind.assert(conn, class_iri, file_name, icon, origin)
        .map_err(|e| format!("Failed to create File entity: {}", e))?;

    ind.add_property(conn, "foundation:fileName", vec![Object::Literal {
        value: file_name.to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], origin).map_err(|e| format!("Failed to set fileName: {}", e))?;

    ind.add_property(conn, "foundation:filePath", vec![Object::Literal {
        value: stored_path.to_string(),
        datatype: Some("xsd:anyURI".to_string()),
        language: None,
    }], origin).map_err(|e| format!("Failed to set filePath: {}", e))?;

    ind.add_property(conn, "foundation:fileSize", vec![Object::Integer(size)], origin)
        .map_err(|e| format!("Failed to set fileSize: {}", e))?;

    ind.add_property(conn, "foundation:fileHash", vec![Object::Literal {
        value: hash.to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], origin).map_err(|e| format!("Failed to set fileHash: {}", e))?;

    if let Some(ft_iri) = mime_to_file_type_iri(mime_type) {
        ind.add_property(conn, "foundation:hasFileType",
            vec![Object::Iri(ft_iri.to_string())], origin)
            .map_err(|e| format!("Failed to set hasFileType: {}", e))?;
    }

    let date = chrono::DateTime::from_timestamp_millis(timestamp_ms)
        .unwrap_or_default()
        .to_rfc3339();
    ind.add_property(conn, "foundation:uploadDate",
        vec![Object::DateTime(date)], origin)
        .map_err(|e| format!("Failed to set uploadDate: {}", e))?;

    Ok(())
}
