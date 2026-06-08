use super::*;
use crate::process_automation::executor::interpolate;
use crate::eavto::Object;

// ── str_lit ───────────────────────────────────────────────────────────────────

#[test]
fn test_str_lit_produces_string_literal() {
    let obj = str_lit("hello");
    match obj {
        Object::Literal { value, datatype, language } => {
            assert_eq!(value, "hello");
            assert_eq!(datatype.as_deref(), Some("xsd:string"));
            assert!(language.is_none());
        }
        _ => panic!("Expected a Literal object"),
    }
}

#[test]
fn test_str_lit_empty_string() {
    let obj = str_lit("");
    match obj {
        Object::Literal { value, .. } => assert_eq!(value, ""),
        _ => panic!("Expected a Literal object"),
    }
}

#[test]
fn test_str_lit_accepts_string_type() {
    let s = String::from("owned");
    let obj = str_lit(s);
    match obj {
        Object::Literal { value, .. } => assert_eq!(value, "owned"),
        _ => panic!("Expected a Literal object"),
    }
}

// ── interpolate ───────────────────────────────────────────────────────────────

#[test]
fn test_interpolate_replaces_placeholder() {
    let mut ctx = ExecutionContext::new();
    ctx.insert("host".to_string(), "api.example.com".to_string());
    assert_eq!(
        interpolate("https://{{host}}/v1", &ctx),
        "https://api.example.com/v1"
    );
}

#[test]
fn test_interpolate_body_with_context_value() {
    let mut ctx = ExecutionContext::new();
    ctx.insert("userId".to_string(), "42".to_string());
    let result = interpolate("{\"id\": \"{{userId}}\"}", &ctx);
    assert_eq!(result, "{\"id\": \"42\"}");
}

#[test]
fn test_interpolate_unknown_key_unchanged() {
    let ctx = ExecutionContext::new();
    assert_eq!(interpolate("{{missing}}", &ctx), "{{missing}}");
}

#[test]
fn test_interpolate_no_placeholders_unchanged() {
    let ctx = ExecutionContext::new();
    assert_eq!(interpolate("plain text", &ctx), "plain text");
}
