use std::io::{BufRead, BufReader};
use serde_json::Value;
use rusqlite::Connection;
use super::ToolResult;

const DEFAULT_MAX_LINE_CHARS: usize = 4096;

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

fn open_file(path: &str) -> Result<std::fs::File, ToolResult> {
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

pub fn head_file(conn: &Connection, args: &Value) -> ToolResult {
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
    let file = match open_file(&file_path) {
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

pub fn read_lines(conn: &Connection, args: &Value) -> ToolResult {
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
    let file = match open_file(&file_path) {
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

    // ── head_file ────────────────────────────────────────────────────────────

    #[test]
    fn head_file_returns_first_n_lines_with_1_based_numbers() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["alpha", "beta", "gamma", "delta", "epsilon"]);
        register_file(&mut conn, "foundation:File_h1", tmp.path().to_str().unwrap());

        let result = head_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_h1", "n": 3 }));

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
    fn head_file_returns_all_lines_when_file_shorter_than_n() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["one", "two", "three", "four"]);
        register_file(&mut conn, "foundation:File_h2", tmp.path().to_str().unwrap());

        let result = head_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_h2", "n": 20 }));

        assert!(result.success);
        let data = result.result.unwrap();
        assert_eq!(data["lines"].as_array().unwrap().len(), 4);
        assert_eq!(data["total_lines"].as_u64().unwrap(), 4);
    }

    #[test]
    fn head_file_uses_default_n_10_when_omitted() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let content: Vec<String> = (1..=15).map(|i| format!("line{}", i)).collect();
        let refs: Vec<&str> = content.iter().map(|s| s.as_str()).collect();
        let tmp = write_temp_file(&refs);
        register_file(&mut conn, "foundation:File_h3", tmp.path().to_str().unwrap());

        let result = head_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_h3" }));

        assert!(result.success);
        let data = result.result.unwrap();
        assert_eq!(data["lines"].as_array().unwrap().len(), 10);
        assert_eq!(data["total_lines"].as_u64().unwrap(), 15);
    }

    #[test]
    fn head_file_returns_error_when_no_file_path_property() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let ind = Individual::new("foundation:File_no_path");
        ind.assert(&mut conn, "foundation:File", "no path", "https://example.com/icon.png", "test").unwrap();

        let result = head_file(&conn, &serde_json::json!({ "file_iri": "foundation:File_no_path" }));

        assert!(!result.success);
        assert!(result.error.unwrap().contains("foundation:filePath"));
    }

    // ── read_lines ───────────────────────────────────────────────────────────

    #[test]
    fn read_lines_returns_requested_range_with_correct_line_numbers() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["a", "b", "c", "d", "e", "f"]);
        register_file(&mut conn, "foundation:File_r1", tmp.path().to_str().unwrap());

        let result = read_lines(&conn, &serde_json::json!({
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
    fn read_lines_clips_to_end_of_file_when_end_line_exceeds_length() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["a", "b", "c", "d", "e"]);
        register_file(&mut conn, "foundation:File_r2", tmp.path().to_str().unwrap());

        let result = read_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r2", "start_line": 3, "end_line": 100
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        let lines = data["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 3); // lines 3, 4, 5
        assert_eq!(data["total_lines"].as_u64().unwrap(), 5);
    }

    #[test]
    fn read_lines_always_returns_total_lines_for_pagination_planning() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let content: Vec<String> = (1..=10).map(|i| format!("row{}", i)).collect();
        let refs: Vec<&str> = content.iter().map(|s| s.as_str()).collect();
        let tmp = write_temp_file(&refs);
        register_file(&mut conn, "foundation:File_r3", tmp.path().to_str().unwrap());

        let result = read_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r3", "start_line": 1, "end_line": 2
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        assert_eq!(data["lines"].as_array().unwrap().len(), 2);
        assert_eq!(data["total_lines"].as_u64().unwrap(), 10);
    }

    #[test]
    fn read_lines_returns_empty_lines_when_start_exceeds_file_length() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["a", "b", "c", "d", "e"]);
        register_file(&mut conn, "foundation:File_r4", tmp.path().to_str().unwrap());

        let result = read_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r4", "start_line": 50, "end_line": 60
        }));

        assert!(result.success);
        let data = result.result.unwrap();
        assert!(data["lines"].as_array().unwrap().is_empty());
        assert_eq!(data["total_lines"].as_u64().unwrap(), 5);
    }

    #[test]
    fn read_lines_returns_error_when_start_line_greater_than_end_line() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["a", "b", "c"]);
        register_file(&mut conn, "foundation:File_r5", tmp.path().to_str().unwrap());

        let result = read_lines(&conn, &serde_json::json!({
            "file_iri": "foundation:File_r5", "start_line": 5, "end_line": 2
        }));

        assert!(!result.success);
        assert!(result.error.unwrap().contains("end_line"));
    }

    #[test]
    fn read_lines_truncates_long_lines_and_reports_total_chars() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let long_line = "x".repeat(8000);
        let tmp = write_temp_file(&[&long_line]);
        register_file(&mut conn, "foundation:File_r6", tmp.path().to_str().unwrap());

        let result = read_lines(&conn, &serde_json::json!({
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
    fn read_lines_returns_char_slice_when_start_char_and_end_char_are_provided() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let tmp = write_temp_file(&["abcdefghij"]);
        register_file(&mut conn, "foundation:File_r7", tmp.path().to_str().unwrap());

        let result = read_lines(&conn, &serde_json::json!({
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
    fn read_lines_supports_iterative_reading_of_long_line_via_start_char() {
        let mut conn = setup_test_db();
        setup_file_ontology(&mut conn);
        let long_line = (0..10000u32).map(|i| char::from_digit(i % 10, 10).unwrap()).collect::<String>();
        let tmp = write_temp_file(&[&long_line]);
        register_file(&mut conn, "foundation:File_r8", tmp.path().to_str().unwrap());

        let result = read_lines(&conn, &serde_json::json!({
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
