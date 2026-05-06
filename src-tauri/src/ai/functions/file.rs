use std::io::{BufRead, BufReader};
use serde_json::Value;
use rusqlite::Connection;
use super::ToolResult;

const DEFAULT_MAX_LINE_CHARS: usize = 4096;

fn resolve_file_path(conn: &Connection, file_iri: &str) -> Result<String, ToolResult> {
    let stored = match crate::owl::get_literal_property(conn, file_iri, "foundation:filePath") {
        Ok(Some(p)) => p,
        Ok(None) => return Err(ToolResult {
            success: false,
            result: None,
            error: Some(format!("No foundation:filePath found for '{}'", file_iri)),
            concept: None,
        }),
        Err(e) => return Err(ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to look up filePath for '{}': {}", file_iri, e)),
            concept: None,
        }),
    };
    // Accept both portable (relative to foundation_dir), legacy `file://` URIs,
    // and bare absolute paths. See `paths::resolve_path` for the rules.
    Ok(crate::paths::resolve_path(&stored).to_string_lossy().into_owned())
}

fn open_text_file(path: &str) -> Result<std::fs::File, ToolResult> {
    std::fs::File::open(path).map_err(|e| ToolResult {
        success: false,
        result: None,
        error: Some(format!("Failed to open file '{}': {}", path, e)),
        concept: None,
    })
}

/// Slice a line's content to [start_char-1 .. end_char] (1-based, inclusive).
/// Returns (sliced_content, total_chars, truncated).
fn slice_line(line: &str, start_char: usize, end_char: usize) -> (String, usize, bool) {
    let total_chars = line.chars().count();
    let start = (start_char.saturating_sub(1)).min(total_chars);
    let end = end_char.min(total_chars);
    let content: String = line.chars().skip(start).take(end.saturating_sub(start)).collect();
    let truncated = end < total_chars;
    (content, total_chars, truncated)
}

fn detect_mime_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"%PDF") {
        return "application/pdf";
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return "image/jpeg";
    }
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return "image/png";
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return "image/webp";
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return "image/gif";
    }
    "application/octet-stream"
}

pub fn read_binary_file(conn: &Connection, args: &Value) -> ToolResult {
    let file_iri = match args.get("file_iri").and_then(|v| v.as_str()) {
        Some(iri) if !iri.is_empty() => iri,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: file_iri".to_string()),
            concept: None,
        },
    };

    let file_path = match resolve_file_path(conn, file_iri) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let raw = match std::fs::read(&file_path) {
        Ok(bytes) => bytes,
        Err(e) => return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to read file '{}': {}", file_path, e)),
            concept: None,
        },
    };

    let media_type = detect_mime_type(&raw);

    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(&raw);

    ToolResult {
        success: true,
        result: Some(serde_json::json!({ "media_type": media_type, "data": data })),
        error: None,
        concept: None,
    }
}

fn pdf_fingerprint(conn: &Connection, file_iri: &str, file_path: &str) -> String {
    if let Ok(Some(hash)) = crate::owl::get_literal_property(conn, file_iri, "foundation:fileHash") {
        let hex = hash.split(':').next_back().unwrap_or(&hash);
        let trimmed: String = hex.chars().take(16).collect();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    // Fallback when foundation:fileHash is missing — mtime in milliseconds is enough
    // to invalidate a stale cache entry across re-imports.
    std::fs::metadata(file_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| format!("m{}", d.as_millis()))
        .unwrap_or_else(|| "nofp".to_string())
}

pub fn read_pdf_page(conn: &Connection, args: &Value) -> ToolResult {
    let file_iri = match args.get("file_iri").and_then(|v| v.as_str()) {
        Some(iri) if !iri.is_empty() => iri,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: file_iri".to_string()),
            concept: None,
        },
    };

    let page = match args.get("page").and_then(|v| v.as_u64()) {
        Some(p) if p >= 1 => p as u32,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing or invalid required parameter: page (must be a positive integer, 1-based)".to_string()),
            concept: None,
        },
    };

    let file_path = match resolve_file_path(conn, file_iri) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let mut doc = match lopdf::Document::load(&file_path) {
        Ok(d) => d,
        Err(e) => return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to load PDF '{}': {}", file_path, e)),
            concept: None,
        },
    };

    let pages = doc.get_pages();
    let total_pages = pages.len() as u32;

    if page > total_pages {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Page {} out of range (PDF has {} pages)", page, total_pages)),
            concept: None,
        };
    }

    let to_delete: Vec<u32> = pages.into_keys().filter(|n| *n != page).collect();
    doc.delete_pages(&to_delete);

    // Inlining the page bytes (base64) in the JSON tool result balloons context for any
    // caller that relays it as text — instead, persist to the OS temp dir (cleared by
    // the OS) and return only the path. Callers that need a content block load lazily.
    let temp_root = std::env::temp_dir().join("foundation-pdf-pages");
    if let Err(e) = std::fs::create_dir_all(&temp_root) {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to create temp dir '{}': {}", temp_root.display(), e)),
            concept: None,
        };
    }

    // Include a content fingerprint in the filename so distinct IRIs that sanitise to
    // the same string don't collide, and so a re-imported PDF (same IRI, new bytes)
    // produces a fresh cache entry instead of serving stale data.
    let fingerprint = pdf_fingerprint(conn, file_iri, &file_path);
    let safe_iri: String = file_iri.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let out_path = temp_root.join(format!("{safe_iri}_{fingerprint}_p{page}.pdf"));

    if let Err(e) = doc.save(&out_path) {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to write single-page PDF '{}': {}", out_path.display(), e)),
            concept: None,
        };
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "media_type": "application/pdf",
            "page_path": out_path.to_string_lossy(),
            "page": page,
            "total_pages": total_pages,
        })),
        error: None,
        concept: None,
    }
}

pub fn head_text_file(conn: &Connection, args: &Value) -> ToolResult {
    let file_iri = match args.get("file_iri").and_then(|v| v.as_str()) {
        Some(iri) if !iri.is_empty() => iri,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: file_iri".to_string()),
            concept: None,
        },
    };

    let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let start_char = args.get("start_char").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let end_char = args.get("end_char").and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(start_char + DEFAULT_MAX_LINE_CHARS - 1);

    let file_path = match resolve_file_path(conn, file_iri) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let file = match open_text_file(&file_path) {
        Ok(f) => f,
        Err(e) => return e,
    };

    let reader = BufReader::new(file);
    let mut head_lines: Vec<serde_json::Value> = Vec::new();
    let mut total_lines: usize = 0;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => return ToolResult {
                success: false,
                result: None,
                error: Some(format!("Failed to read file '{}': {}", file_path, e)),
                concept: None,
            },
        };
        total_lines += 1;
        if head_lines.len() < n {
            let (content, total_chars, truncated) = slice_line(&line, start_char, end_char);
            let mut entry = serde_json::json!({
                "number": total_lines,
                "content": content,
                "total_chars": total_chars,
            });
            if truncated {
                entry["truncated"] = serde_json::json!(true);
            }
            head_lines.push(entry);
        }
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "lines": head_lines,
            "total_lines": total_lines,
        })),
        error: None,
        concept: None,
    }
}

pub fn read_text_lines(conn: &Connection, args: &Value) -> ToolResult {
    let file_iri = match args.get("file_iri").and_then(|v| v.as_str()) {
        Some(iri) if !iri.is_empty() => iri,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: file_iri".to_string()),
            concept: None,
        },
    };

    let start_line = match args.get("start_line").and_then(|v| v.as_u64()) {
        Some(v) if v >= 1 => v as usize,
        Some(_) => return ToolResult {
            success: false,
            result: None,
            error: Some("start_line must be >= 1".to_string()),
            concept: None,
        },
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: start_line".to_string()),
            concept: None,
        },
    };

    let end_line = match args.get("end_line").and_then(|v| v.as_u64()) {
        Some(v) if v as usize >= start_line => v as usize,
        Some(_) => return ToolResult {
            success: false,
            result: None,
            error: Some("end_line must be >= start_line".to_string()),
            concept: None,
        },
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: end_line".to_string()),
            concept: None,
        },
    };

    let start_char = args.get("start_char").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let end_char = args.get("end_char").and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(start_char + DEFAULT_MAX_LINE_CHARS - 1);

    let file_path = match resolve_file_path(conn, file_iri) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let file = match open_text_file(&file_path) {
        Ok(f) => f,
        Err(e) => return e,
    };

    let reader = BufReader::new(file);
    let mut result_lines: Vec<serde_json::Value> = Vec::new();
    let mut total_lines: usize = 0;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => return ToolResult {
                success: false,
                result: None,
                error: Some(format!("Failed to read file '{}': {}", file_path, e)),
                concept: None,
            },
        };
        total_lines += 1;
        if total_lines >= start_line && total_lines <= end_line {
            let (content, total_chars, truncated) = slice_line(&line, start_char, end_char);
            let mut entry = serde_json::json!({
                "number": total_lines,
                "content": content,
                "total_chars": total_chars,
            });
            if truncated {
                entry["truncated"] = serde_json::json!(true);
            }
            result_lines.push(entry);
        }
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "lines": result_lines,
            "total_lines": total_lines,
        })),
        error: None,
        concept: None,
    }
}

fn err(message: String) -> ToolResult {
    ToolResult { success: false, result: None, error: Some(message), concept: None }
}

fn target_individual_exists(conn: &Connection, target_iri: &str) -> Result<bool, String> {
    crate::owl::get_iri_property(conn, target_iri, "rdf:type")
        .map(|opt| opt.is_some())
        .map_err(|e| format!("Failed to look up target IRI '{}': {}", target_iri, e))
}

fn log_step(stage: &str, file_name: &str, detail: &str) {
    crate::commands::log_backend(
        "info",
        &format!("[MCP attach_file] {} | {} | {}", stage, file_name, detail),
    );
}

pub fn attach_file_to_individual(conn: &mut Connection, args: &Value) -> ToolResult {
    let target_iri = match args.get("target_iri").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return err("Missing required parameter: target_iri".to_string()),
    };

    let link_property = args.get("link_property")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("foundation:hasFile")
        .to_string();

    let file_path_arg = args.get("file_path").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let file_data_b64 = args.get("file_data_base64").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let file_name_arg = args.get("file_name").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let mime_override = args.get("mime_type").and_then(|v| v.as_str()).filter(|s| !s.is_empty());

    let (source_path_opt, raw, file_name) = match (file_path_arg, file_data_b64) {
        (Some(path), _) => {
            let src = std::path::PathBuf::from(path);
            let derived_name = file_name_arg
                .map(String::from)
                .or_else(|| src.file_name().and_then(|n| n.to_str()).map(String::from))
                .unwrap_or_else(|| "unnamed".to_string());
            log_step("read", &derived_name, &format!("source={}", path));
            let bytes = match std::fs::read(&src) {
                Ok(b) => b,
                Err(e) => return err(format!("Failed to read file '{}': {}", path, e)),
            };
            (Some(src), bytes, derived_name)
        }
        (None, Some(b64)) => {
            let name = match file_name_arg {
                Some(n) => n.to_string(),
                None => return err("file_name is required when using file_data_base64".to_string()),
            };
            log_step("decode", &name, &format!("base64_chars={}", b64.len()));
            use base64::Engine;
            let bytes = match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(b) => b,
                Err(e) => return err(format!("Invalid base64 payload: {}", e)),
            };
            (None, bytes, name)
        }
        (None, None) => return err(
            "Either file_path or (file_data_base64 + file_name) is required".to_string()
        ),
    };

    match target_individual_exists(conn, &target_iri) {
        Ok(true) => {}
        Ok(false) => return err(format!("Target individual '{}' not found", target_iri)),
        Err(e) => return err(e),
    }

    let mime_type = mime_override
        .map(String::from)
        .unwrap_or_else(|| crate::files::mime_from_extension(&file_name).to_string());

    let attachments_dir = crate::paths::attachments_dir();
    if let Err(e) = std::fs::create_dir_all(&attachments_dir) {
        return err(format!("Failed to create attachments directory: {}", e));
    }

    let timestamp = chrono::Utc::now().timestamp_millis();
    let safe_name = crate::files::sanitize_filename(&file_name);
    let permanent_path = attachments_dir.join(format!("{}_{}", timestamp, safe_name));

    let hash = crate::files::sha256_hex(&raw);
    let size = raw.len() as i64;
    log_step("hash", &file_name, &format!("size={} hash={}", size, hash));

    if let Err(e) = std::fs::write(&permanent_path, &raw) {
        return err(format!("Failed to write file to attachments dir: {}", e));
    }
    log_step("store", &file_name, &format!("dest={}", permanent_path.display()));

    let stored_path = crate::paths::to_portable_path(&permanent_path);
    let file_iri = format!("foundation:File_{}", timestamp);

    if let Err(e) = crate::files::assert_file_individual(conn, &crate::files::FileMetadata {
        iri: &file_iri,
        class_iri: "foundation:File",
        icon: "attach_file",
        file_name: &file_name,
        stored_path: &stored_path,
        size,
        hash: &hash,
        mime_type: &mime_type,
        timestamp_ms: timestamp,
        origin: "mcp",
    }) {
        let _ = std::fs::remove_file(&permanent_path);
        return err(e);
    }
    log_step("assert", &file_name, &format!("file_iri={}", file_iri));

    let target = crate::owl::Individual::new(&target_iri);
    if let Err(e) = target.append_property(
        conn,
        &link_property,
        vec![crate::owl::Object::Iri(file_iri.clone())],
        "mcp",
    ) {
        let _ = crate::owl::Individual::retract(conn, &file_iri, "mcp");
        let _ = std::fs::remove_file(&permanent_path);
        return err(format!(
            "Failed to link file via '{}': {}. The file entity and the copy in attachments/ were rolled back; the source file at the original location is untouched. Retry with a different link_property.",
            link_property, e
        ));
    }
    log_step("link", &file_name, &format!("{} <{}> {}", target_iri, link_property, file_iri));

    if let Some(src) = &source_path_opt {
        if let Err(e) = std::fs::remove_file(src) {
            log_step("cleanup", &file_name, &format!("failed to remove source {}: {}", src.display(), e));
        }
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "file_iri": file_iri,
            "file_name": file_name,
            "file_path": stored_path,
            "file_size": size,
            "file_hash": hash,
            "mime_type": mime_type,
            "linked_to": target_iri,
            "via_property": link_property,
        })),
        error: None,
        concept: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::functions::{execute_tool, ToolCall};
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Individual, Object};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_file(lines: &[&str]) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp file");
        for line in lines {
            writeln!(f, "{}", line).expect("write line");
        }
        f
    }

    fn setup_file_ontology(conn: &mut rusqlite::Connection) {
        let define_prop = ToolCall {
            name: "define_property".to_string(),
            arguments: serde_json::json!({
                "operations": [{"iri": "foundation:filePath", "label": "file path", "property_type": "datatype", "range": "xsd:anyURI"}]
            }),
        };
        execute_tool(conn, &define_prop, None, None);

        let define_class = ToolCall {
            name: "define_class".to_string(),
            arguments: serde_json::json!({
                "operations": [{
                    "iri": "foundation:File",
                    "label": "File",
                    "icon": "https://example.com/icon.png",
                    "super_classes": ["owl:Thing"],
                    "add_properties": ["foundation:filePath"]
                }]
            }),
        };
        execute_tool(conn, &define_class, None, None);
    }

    fn register_file(conn: &mut rusqlite::Connection, file_iri: &str, path: &str) {
        let ind = Individual::new(file_iri);
        ind.assert(conn, "foundation:File", "test file", "https://example.com/icon.png", "test")
            .expect("assert individual");
        ind.add_property(conn, "foundation:filePath", vec![Object::Literal {
            value: format!("file://{}", path),
            datatype: Some("xsd:anyURI".to_string()),
            language: None,
        }], "test").expect("add filePath");
    }

    // ── detect_mime_type ─────────────────────────────────────────────────────

    #[test]
    fn detect_mime_type_identifies_pdf_by_magic_bytes() {
        assert_eq!(detect_mime_type(b"%PDF-1.4 content"), "application/pdf");
    }

    #[test]
    fn detect_mime_type_identifies_jpeg_by_magic_bytes() {
        let jpeg: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_mime_type(jpeg), "image/jpeg");
    }

    #[test]
    fn detect_mime_type_identifies_png_by_magic_bytes() {
        let png: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_mime_type(png), "image/png");
    }

    #[test]
    fn detect_mime_type_identifies_webp_by_magic_bytes() {
        let mut webp = b"RIFF\x00\x00\x00\x00WEBP".to_vec();
        webp.extend_from_slice(b"extra");
        assert_eq!(detect_mime_type(&webp), "image/webp");
    }

    #[test]
    fn detect_mime_type_identifies_gif_by_magic_bytes() {
        assert_eq!(detect_mime_type(b"GIF89a\x01\x00"), "image/gif");
        assert_eq!(detect_mime_type(b"GIF87a\x01\x00"), "image/gif");
    }

    #[test]
    fn detect_mime_type_returns_octet_stream_for_unknown_bytes() {
        assert_eq!(detect_mime_type(&[0x00, 0x01, 0x02, 0x03]), "application/octet-stream");
    }

    // ── read_binary_file ─────────────────────────────────────────────────────

    #[test]
    fn read_binary_file_returns_pdf_media_type_for_pdf_content() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let pdf_bytes = b"%PDF-1.4 minimal test content";
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(pdf_bytes).expect("write pdf");
        register_file(&mut conn, "foundation:File_bin_pdf", tmp.path().to_str().unwrap());

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_pdf" }));

        assert!(result.success, "Expected success but got error: {:?}", result.error);
        let data = result.result.unwrap();
        assert_eq!(data["media_type"].as_str().unwrap(), "application/pdf");
        assert!(data["data"].as_str().is_some());
    }

    #[test]
    fn read_binary_file_returns_jpeg_media_type_for_jpeg_content() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let jpeg: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(jpeg).expect("write jpeg");
        register_file(&mut conn, "foundation:File_bin_jpeg", tmp.path().to_str().unwrap());

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_jpeg" }));

        assert!(result.success);
        assert_eq!(result.result.unwrap()["media_type"].as_str().unwrap(), "image/jpeg");
    }

    #[test]
    fn read_binary_file_base64_decodes_back_to_original_bytes() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let original = b"%PDF-1.4 round-trip test";
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(original).expect("write bytes");
        register_file(&mut conn, "foundation:File_bin_rt", tmp.path().to_str().unwrap());

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_rt" }));

        assert!(result.success);
        let encoded = result.result.unwrap()["data"].as_str().unwrap().to_string();
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn read_binary_file_returns_octet_stream_for_unknown_format() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let unknown: &[u8] = &[0x00, 0x01, 0x02, 0x03, 0x04];
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(unknown).expect("write bytes");
        register_file(&mut conn, "foundation:File_bin_unk", tmp.path().to_str().unwrap());

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_unk" }));

        assert!(result.success);
        assert_eq!(result.result.unwrap()["media_type"].as_str().unwrap(), "application/octet-stream");
    }

    #[test]
    fn read_binary_file_returns_error_when_no_file_path_property() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let ind = Individual::new("foundation:File_bin_no_path");
        ind.assert(&mut conn, "foundation:File", "no path", "https://example.com/icon.png", "test").unwrap();

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_no_path" }));

        assert!(!result.success);
        assert!(result.error.unwrap().contains("foundation:filePath"));
    }

    #[test]
    fn read_binary_file_returns_error_when_file_does_not_exist_on_disk() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let ind = Individual::new("foundation:File_bin_missing");
        ind.assert(&mut conn, "foundation:File", "missing", "https://example.com/icon.png", "test").unwrap();
        ind.add_property(&mut conn, "foundation:filePath", vec![Object::Literal {
            value: "file:///nonexistent/path/to/file.bin".to_string(),
            datatype: Some("xsd:anyURI".to_string()),
            language: None,
        }], "test").unwrap();

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_missing" }));

        assert!(!result.success);
    }

    // ── head_text_file ───────────────────────────────────────────────────────

    #[test]
    fn head_text_file_returns_first_n_lines_with_1_based_numbers() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["alpha", "beta", "gamma", "delta", "epsilon"]);
        register_file(&mut conn, "foundation:File_h1", tmp.path().to_str().unwrap());

        let result = head_text_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_h1", "n": 3 }));

        assert!(result.success);
        let data = result.result.unwrap();
        let lines = data["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["number"].as_u64().unwrap(), 1);
        assert_eq!(lines[0]["content"].as_str().unwrap(), "alpha");
        assert_eq!(lines[2]["number"].as_u64().unwrap(), 3);
        assert_eq!(data["total_lines"].as_u64().unwrap(), 5);
    }

    #[test]
    fn head_text_file_returns_all_lines_when_file_shorter_than_n() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["one", "two", "three", "four"]);
        register_file(&mut conn, "foundation:File_h2", tmp.path().to_str().unwrap());

        let result = head_text_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_h2", "n": 20 }));

        assert!(result.success);
        let data = result.result.unwrap();
        assert_eq!(data["lines"].as_array().unwrap().len(), 4);
        assert_eq!(data["total_lines"].as_u64().unwrap(), 4);
    }

    #[test]
    fn head_text_file_uses_default_n_10_when_omitted() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let content: Vec<String> = (1..=15).map(|i| format!("line{}", i)).collect();
        let refs: Vec<&str> = content.iter().map(|s| s.as_str()).collect();
        let tmp = write_temp_file(&refs);
        register_file(&mut conn, "foundation:File_h3", tmp.path().to_str().unwrap());

        let result = head_text_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_h3" }));

        assert!(result.success);
        let data = result.result.unwrap();
        assert_eq!(data["lines"].as_array().unwrap().len(), 10);
        assert_eq!(data["total_lines"].as_u64().unwrap(), 15);
    }

    #[test]
    fn head_text_file_returns_error_when_no_file_path_property() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let ind = Individual::new("foundation:File_no_path");
        ind.assert(&mut conn, "foundation:File", "no path", "https://example.com/icon.png", "test").unwrap();

        let result = head_text_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_no_path" }));

        assert!(!result.success);
        assert!(result.error.unwrap().contains("foundation:filePath"));
    }

    // ── read_text_lines ──────────────────────────────────────────────────────

    #[test]
    fn read_text_lines_returns_requested_range_with_correct_line_numbers() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["a", "b", "c", "d", "e", "f"]);
        register_file(&mut conn, "foundation:File_r1", tmp.path().to_str().unwrap());

        let result = read_text_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r1", "start_line": 2, "end_line": 4
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        let lines = data["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["number"].as_u64().unwrap(), 2);
        assert_eq!(lines[0]["content"].as_str().unwrap(), "b");
        assert_eq!(lines[2]["number"].as_u64().unwrap(), 4);
        assert_eq!(data["total_lines"].as_u64().unwrap(), 6);
    }

    #[test]
    fn read_text_lines_clips_to_end_of_file_when_end_line_exceeds_length() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["a", "b", "c", "d", "e"]);
        register_file(&mut conn, "foundation:File_r2", tmp.path().to_str().unwrap());

        let result = read_text_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r2", "start_line": 3, "end_line": 100
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        let lines = data["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 3); // lines 3, 4, 5
        assert_eq!(data["total_lines"].as_u64().unwrap(), 5);
    }

    #[test]
    fn read_text_lines_always_returns_total_lines_for_pagination_planning() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let content: Vec<String> = (1..=10).map(|i| format!("row{}", i)).collect();
        let refs: Vec<&str> = content.iter().map(|s| s.as_str()).collect();
        let tmp = write_temp_file(&refs);
        register_file(&mut conn, "foundation:File_r3", tmp.path().to_str().unwrap());

        let result = read_text_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r3", "start_line": 1, "end_line": 2
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        assert_eq!(data["lines"].as_array().unwrap().len(), 2);
        assert_eq!(data["total_lines"].as_u64().unwrap(), 10);
    }

    #[test]
    fn read_text_lines_returns_empty_lines_when_start_exceeds_file_length() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["a", "b", "c", "d", "e"]);
        register_file(&mut conn, "foundation:File_r4", tmp.path().to_str().unwrap());

        let result = read_text_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r4", "start_line": 50, "end_line": 60
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        assert!(data["lines"].as_array().unwrap().is_empty());
        assert_eq!(data["total_lines"].as_u64().unwrap(), 5);
    }

    #[test]
    fn read_text_lines_returns_error_when_start_line_greater_than_end_line() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["a", "b", "c"]);
        register_file(&mut conn, "foundation:File_r5", tmp.path().to_str().unwrap());

        let result = read_text_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r5", "start_line": 5, "end_line": 2
        }));

        assert!(!result.success);
        assert!(result.error.unwrap().contains("end_line"));
    }

    #[test]
    fn read_text_lines_truncates_long_lines_and_reports_total_chars() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let long_line = "x".repeat(8000);
        let tmp = write_temp_file(&[&long_line]);
        register_file(&mut conn, "foundation:File_r6", tmp.path().to_str().unwrap());

        let result = read_text_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r6", "start_line": 1, "end_line": 1
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        let lines = data["lines"].as_array().unwrap();
        let line = &lines[0];
        assert_eq!(line["total_chars"].as_u64().unwrap(), 8000);
        assert_eq!(line["content"].as_str().unwrap().len(), DEFAULT_MAX_LINE_CHARS);
        assert_eq!(line["truncated"].as_bool().unwrap(), true);
    }

    #[test]
    fn read_text_lines_returns_char_slice_when_start_char_and_end_char_are_provided() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["abcdefghij"]);
        register_file(&mut conn, "foundation:File_r7", tmp.path().to_str().unwrap());

        let result = read_text_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r7", "start_line": 1, "end_line": 1,
            "start_char": 5, "end_char": 10
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        let line = &data["lines"].as_array().unwrap()[0];
        assert_eq!(line["content"].as_str().unwrap(), "efghij");
        assert_eq!(line["total_chars"].as_u64().unwrap(), 10);
    }

    #[test]
    fn read_text_lines_supports_iterative_reading_of_long_line_via_start_char() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let long_line = (0..10000u32).map(|i| char::from_digit(i % 10, 10).unwrap()).collect::<String>();
        let tmp = write_temp_file(&[&long_line]);
        register_file(&mut conn, "foundation:File_r8", tmp.path().to_str().unwrap());

        let result = read_text_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r8", "start_line": 1, "end_line": 1,
            "start_char": 4097, "end_char": 8192
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        let line = &data["lines"].as_array().unwrap()[0];
        assert_eq!(line["total_chars"].as_u64().unwrap(), 10000);
        assert_eq!(line["content"].as_str().unwrap().len(), 4096); // 8192 - 4097 + 1
    }
}
