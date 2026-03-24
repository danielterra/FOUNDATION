use super::*;
use crate::eavto::{store, Triple, Object};
use crate::eavto::test_helpers::setup_test_db;

// ── step output → inputIRIs (regression: Bug_1774382679925) ──────────────────

#[test]
fn test_step_output_inserts_input_iris() {
    let output = Some("foundation:Task_123".to_string());
    let mut ctx = ExecutionContext::new();

    if let Some(value) = output.filter(|v| !v.is_empty()) {
        ctx.insert("inputIRIs".to_string(), value);
    }

    assert_eq!(ctx.get("inputIRIs").map(|s| s.as_str()), Some("foundation:Task_123"));
}

#[test]
fn test_step_output_empty_not_inserted() {
    let output = Some(String::new());
    let mut ctx = ExecutionContext::new();

    if let Some(value) = output.filter(|v| !v.is_empty()) {
        ctx.insert("inputIRIs".to_string(), value);
    }

    assert!(ctx.get("inputIRIs").is_none());
}

// ── interpolate ──────────────────────────────────────────────────────────────

#[test]
fn test_interpolate_replaces_single_key() {
    let mut ctx = ExecutionContext::new();
    ctx.insert("city".to_string(), "London".to_string());
    assert_eq!(interpolate("Weather in {{city}} today", &ctx), "Weather in London today");
}

#[test]
fn test_interpolate_replaces_multiple_keys() {
    let mut ctx = ExecutionContext::new();
    ctx.insert("name".to_string(), "Alice".to_string());
    ctx.insert("role".to_string(), "admin".to_string());
    let result = interpolate("Hello {{name}}, you are {{role}}", &ctx);
    assert_eq!(result, "Hello Alice, you are admin");
}

#[test]
fn test_interpolate_unknown_key_left_unchanged() {
    let ctx = ExecutionContext::new();
    let template = "Value is {{missing}}";
    assert_eq!(interpolate(template, &ctx), "Value is {{missing}}");
}

#[test]
fn test_interpolate_empty_context_returns_template() {
    let ctx = ExecutionContext::new();
    assert_eq!(interpolate("no placeholders", &ctx), "no placeholders");
}

#[test]
fn test_interpolate_empty_template_returns_empty() {
    let mut ctx = ExecutionContext::new();
    ctx.insert("key".to_string(), "value".to_string());
    assert_eq!(interpolate("", &ctx), "");
}

#[test]
fn test_interpolate_replaces_key_multiple_occurrences() {
    let mut ctx = ExecutionContext::new();
    ctx.insert("x".to_string(), "42".to_string());
    assert_eq!(interpolate("{{x}} plus {{x}}", &ctx), "42 plus 42");
}

#[test]
fn test_interpolate_key_with_url_context() {
    let mut ctx = ExecutionContext::new();
    ctx.insert("token".to_string(), "abc123".to_string());
    ctx.insert("city".to_string(), "Paris".to_string());
    let result = interpolate("https://api.example.com/weather?city={{city}}&token={{token}}", &ctx);
    assert_eq!(result, "https://api.example.com/weather?city=Paris&token=abc123");
}

// ── load_flow_nodes ───────────────────────────────────────────────────────────

fn insert_triples(conn: &mut rusqlite::Connection, triples: &[Triple]) {
    store::assert_triples(conn, triples, "test").expect("Failed to insert triples");
}

#[test]
fn test_load_flow_nodes_empty_process_returns_empty() {
    let conn = setup_test_db();
    let (result, _) = load_flow_nodes(&conn, "foundation:Process_Empty").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_load_flow_nodes_returns_nodes_with_types() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:Start1", "foundation:partOfProcess", Object::Iri("foundation:Proc1".to_string())),
        Triple::new("foundation:End1", "foundation:partOfProcess", Object::Iri("foundation:Proc1".to_string())),
        Triple::new("foundation:Start1", "rdf:type", Object::Iri("foundation:automation_StartEvent".to_string())),
        Triple::new("foundation:End1",   "rdf:type", Object::Iri("foundation:automation_EndEvent".to_string())),
    ]);

    let (nodes, _) = load_flow_nodes(&conn, "foundation:Proc1").unwrap();
    assert_eq!(nodes.len(), 2);

    let types: Vec<&str> = nodes.iter().map(|(_, t)| t.as_str()).collect();
    assert!(types.contains(&"foundation:automation_StartEvent"));
    assert!(types.contains(&"foundation:automation_EndEvent"));
}

#[test]
fn test_load_flow_nodes_returns_node_iri_and_type() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:Task1", "foundation:partOfProcess", Object::Iri("foundation:Proc2".to_string())),
        Triple::new("foundation:Task1", "rdf:type", Object::Iri("foundation:automation_AgentTask".to_string())),
    ]);

    let (nodes, _) = load_flow_nodes(&conn, "foundation:Proc2").unwrap();
    assert_eq!(nodes.len(), 1);
    let (iri, node_type) = &nodes[0];
    assert_eq!(iri, "foundation:Task1");
    assert_eq!(node_type, "foundation:automation_AgentTask");
}

#[test]
fn test_load_flow_nodes_missing_type_defaults_to_flow_node() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:Node4", "foundation:partOfProcess", Object::Iri("foundation:Proc4".to_string())),
        // No rdf:type for Node4
    ]);

    let (nodes, _) = load_flow_nodes(&conn, "foundation:Proc4").unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].1, "foundation:automation_FlowNode");
}

#[test]
fn test_load_flow_nodes_multiple_nodes_all_returned() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:S5", "foundation:partOfProcess", Object::Iri("foundation:Proc5".to_string())),
        Triple::new("foundation:T5", "foundation:partOfProcess", Object::Iri("foundation:Proc5".to_string())),
        Triple::new("foundation:E5", "foundation:partOfProcess", Object::Iri("foundation:Proc5".to_string())),
        Triple::new("foundation:S5", "rdf:type", Object::Iri("foundation:automation_StartEvent".to_string())),
        Triple::new("foundation:T5", "rdf:type", Object::Iri("foundation:automation_AgentTask".to_string())),
        Triple::new("foundation:E5", "rdf:type", Object::Iri("foundation:automation_EndEvent".to_string())),
    ]);

    let (nodes, _) = load_flow_nodes(&conn, "foundation:Proc5").unwrap();
    assert_eq!(nodes.len(), 3);
}

// ── resolve_error_handler ─────────────────────────────────────────────────────

#[test]
fn test_resolve_error_handler_returns_none_when_no_handler() {
    let conn = setup_test_db();
    let result = resolve_error_handler(&conn, "foundation:TaskNoHandler").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_resolve_error_handler_returns_fallback_when_handler_attached() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:EH1", "rdf:type", Object::Iri("foundation:ErrorHandler".to_string())),
        Triple::new("foundation:EH1", "foundation:appliesTo", Object::Iri("foundation:TaskWithHandler".to_string())),
        Triple::new("foundation:EH1", "foundation:fallbackNode", Object::Iri("foundation:FallbackNode".to_string())),
    ]);

    let result = resolve_error_handler(&conn, "foundation:TaskWithHandler").unwrap();
    assert_eq!(result.as_deref(), Some("foundation:FallbackNode"));
}

#[test]
fn test_resolve_error_handler_ignores_non_error_handler_applies_to() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:SomeOtherThing", "rdf:type", Object::Iri("foundation:SomeClass".to_string())),
        Triple::new("foundation:SomeOtherThing", "foundation:appliesTo", Object::Iri("foundation:TaskY".to_string())),
        Triple::new("foundation:SomeOtherThing", "foundation:fallbackNode", Object::Iri("foundation:FallbackY".to_string())),
    ]);

    let result = resolve_error_handler(&conn, "foundation:TaskY").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_resolve_error_handler_returns_none_when_handler_has_no_fallback() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:EH2", "rdf:type", Object::Iri("foundation:ErrorHandler".to_string())),
        Triple::new("foundation:EH2", "foundation:appliesTo", Object::Iri("foundation:TaskNoFallback".to_string())),
    ]);

    let result = resolve_error_handler(&conn, "foundation:TaskNoFallback").unwrap();
    assert!(result.is_none());
}
