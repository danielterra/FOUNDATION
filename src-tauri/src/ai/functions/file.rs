use std::io::{BufRead, BufReader};
use serde_json::Value;
use rusqlite::Connection;
use super::ToolResult;

const DEFAULT_MAX_LINE_CHARS: usize = 4096;
// Fallback when no model/agent/global config is available.
const FALLBACK_MAX_BINARY_BYTES: u64 = 500 * 1024;

/// Maximum bytes allowed for a binary file read, based on 70% of the conversation's
/// context limit.
///
/// `read_binary_file` sends the file content as a base64-encoded JSON string inside a
/// tool result (plain text — not a native image/document block). The token cost
/// therefore scales with file size regardless of format:
///   base64 expands raw bytes by 4/3; base64 chars tokenise at roughly 2 chars/token,
///   so  raw_bytes → tokens ≈ raw_bytes × (4/3) / 2 = raw_bytes × 2/3
///   → max_raw_bytes = budget_tokens × 3/2
///
/// Context limit lookup order (highest priority first):
///   1. Agent's foundation:maxInputTokensPreference (per-conversation)
///   2. foundation:DefaultMaxInputTokensSetting (global user override)
///   3. foundation:AIModel.foundation:maxInputTokens (model default)
///   4. Hardcoded fallback → 500 KB
fn max_binary_bytes(conn: &Connection, conversation_id: Option<&str>) -> u64 {
    let max_tokens = resolve_context_limit(conn, conversation_id).unwrap_or(0);
    if max_tokens == 0 {
        return FALLBACK_MAX_BINARY_BYTES;
    }
    let budget = (max_tokens as u64) * 70 / 100;
    budget * 3 / 2
}

fn resolve_context_limit(conn: &Connection, conversation_id: Option<&str>) -> Option<usize> {
    use crate::owl::{Individual, Object};

    // 1. Per-agent preference (highest priority — uses load_agent_config which applies the
    //    full agent → model → default hierarchy internally).
    if let Some(conv_iri) = conversation_id {
        if let Ok(config) = crate::commands::chat::settings::load_agent_config(conn, conv_iri) {
            return Some(config.max_tokens);
        }
    }

    // 2. Global user-configured setting.
    if let Ok(Some(setting)) = Individual::get(conn, "foundation:DefaultMaxInputTokensSetting") {
        if let Some((_, Object::Literal { value, .. })) = setting.properties.iter()
            .find(|(k, _)| k == "foundation:settingValue") {
            if let Ok(n) = value.parse::<usize>() {
                return Some(n);
            }
        }
    }

    // 3. Model's maxInputTokens.
    let model_iri = crate::commands::chat::settings::get_ai_model_iri(conn).ok()??;
    let model = Individual::get(conn, &model_iri).ok()??;
    if let Some((_, Object::Integer(n))) = model.properties.iter()
        .find(|(k, _)| k == "foundation:maxInputTokens") {
        return Some(*n as usize);
    }

    None
}

fn resolve_file_path(conn: &Connection, file_iri: &str) -> Result<String, ToolResult> {
    let uri = match crate::owl::get_literal_property(conn, file_iri, "foundation:filePath") {
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
    Ok(uri.strip_prefix("file://").unwrap_or(&uri).to_string())
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

pub fn read_binary_file(conn: &Connection, args: &Value, conversation_id: Option<&str>) -> ToolResult {
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

    let file_size = match std::fs::metadata(&file_path) {
        Ok(m) => m.len(),
        Err(e) => return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Failed to read file metadata '{}': {}", file_path, e)),
            concept: None,
        },
    };

    let limit = max_binary_bytes(conn, conversation_id);
    if file_size > limit {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!(
                "File too large to read: {} bytes (limit is {} bytes, 70% of this conversation's context). Use a text tool or pre-process the file first.",
                file_size, limit
            )),
            concept: None,
        };
    }

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

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_pdf" }), None);

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

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_jpeg" }), None);

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

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_rt" }), None);

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

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_unk" }), None);

        assert!(result.success);
        assert_eq!(result.result.unwrap()["media_type"].as_str().unwrap(), "application/octet-stream");
    }

    #[test]
    fn read_binary_file_returns_error_when_no_file_path_property() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let ind = Individual::new("foundation:File_bin_no_path");
        ind.assert(&mut conn, "foundation:File", "no path", "https://example.com/icon.png", "test").unwrap();

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_no_path" }), None);

        assert!(!result.success);
        assert!(result.error.unwrap().contains("foundation:filePath"));
    }

    #[test]
    fn read_binary_file_returns_error_when_file_exceeds_size_limit() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        // No model config in test DB → falls back to FALLBACK_MAX_BINARY_BYTES (500 KB).
        let oversized = vec![0u8; (FALLBACK_MAX_BINARY_BYTES + 1) as usize];
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(&oversized).expect("write oversized bytes");
        register_file(&mut conn, "foundation:File_bin_oversized", tmp.path().to_str().unwrap());

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_oversized" }), None);

        assert!(!result.success);
        assert!(result.error.unwrap().contains("too large"));
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

        let result = read_binary_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_bin_missing" }), None);

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
