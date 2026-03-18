use super::*;
use crate::eavto::{store, Triple, Object};
use crate::eavto::test_helpers::setup_test_db;

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
    let result = load_flow_nodes(&conn, "foundation:Process_Empty").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_load_flow_nodes_returns_nodes_with_types() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:Proc1", "foundation:hasFlowNode", Object::Iri("foundation:Start1".to_string())),
        Triple::new("foundation:Proc1", "foundation:hasFlowNode", Object::Iri("foundation:End1".to_string())),
        Triple::new("foundation:Start1", "rdf:type", Object::Iri("foundation:automation_StartEvent".to_string())),
        Triple::new("foundation:End1",   "rdf:type", Object::Iri("foundation:automation_EndEvent".to_string())),
    ]);

    let nodes = load_flow_nodes(&conn, "foundation:Proc1").unwrap();
    assert_eq!(nodes.len(), 2);

    let types: Vec<&str> = nodes.iter().map(|(_, t, _)| t.as_str()).collect();
    assert!(types.contains(&"foundation:automation_StartEvent"));
    assert!(types.contains(&"foundation:automation_EndEvent"));
}

#[test]
fn test_load_flow_nodes_includes_output_key() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:Proc2", "foundation:hasFlowNode", Object::Iri("foundation:Task1".to_string())),
        Triple::new("foundation:Task1", "rdf:type", Object::Iri("foundation:automation_AgentTask".to_string())),
        Triple::new("foundation:Task1", "foundation:outputKey", Object::Literal {
            value: "taskResult".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ]);

    let nodes = load_flow_nodes(&conn, "foundation:Proc2").unwrap();
    assert_eq!(nodes.len(), 1);
    let (iri, node_type, output_key) = &nodes[0];
    assert_eq!(iri, "foundation:Task1");
    assert_eq!(node_type, "foundation:automation_AgentTask");
    assert_eq!(output_key.as_deref(), Some("taskResult"));
}

#[test]
fn test_load_flow_nodes_missing_output_key_is_none() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:Proc3", "foundation:hasFlowNode", Object::Iri("foundation:Start3".to_string())),
        Triple::new("foundation:Start3", "rdf:type", Object::Iri("foundation:automation_StartEvent".to_string())),
    ]);

    let nodes = load_flow_nodes(&conn, "foundation:Proc3").unwrap();
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].2.is_none());
}

#[test]
fn test_load_flow_nodes_missing_type_defaults_to_flow_node() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:Proc4", "foundation:hasFlowNode", Object::Iri("foundation:Node4".to_string())),
        // No rdf:type for Node4
    ]);

    let nodes = load_flow_nodes(&conn, "foundation:Proc4").unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].1, "foundation:automation_FlowNode");
}

#[test]
fn test_load_flow_nodes_multiple_nodes_all_returned() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:Proc5", "foundation:hasFlowNode", Object::Iri("foundation:S5".to_string())),
        Triple::new("foundation:Proc5", "foundation:hasFlowNode", Object::Iri("foundation:T5".to_string())),
        Triple::new("foundation:Proc5", "foundation:hasFlowNode", Object::Iri("foundation:E5".to_string())),
        Triple::new("foundation:S5", "rdf:type", Object::Iri("foundation:automation_StartEvent".to_string())),
        Triple::new("foundation:T5", "rdf:type", Object::Iri("foundation:automation_AgentTask".to_string())),
        Triple::new("foundation:E5", "rdf:type", Object::Iri("foundation:automation_EndEvent".to_string())),
    ]);

    let nodes = load_flow_nodes(&conn, "foundation:Proc5").unwrap();
    assert_eq!(nodes.len(), 3);
}
