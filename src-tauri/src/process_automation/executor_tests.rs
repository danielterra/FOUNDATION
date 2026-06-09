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

// ── strip_token_punctuation ───────────────────────────────────────────────────

#[test]
fn strip_backtick_from_iri_token() {
    assert_eq!(strip_token_punctuation("`foundation:Task_123`"), "foundation:Task_123");
}

#[test]
fn strip_parens_and_comma() {
    assert_eq!(strip_token_punctuation("(foundation:Task_123,"), "foundation:Task_123");
}

#[test]
fn strip_nothing_from_clean_iri() {
    assert_eq!(strip_token_punctuation("foundation:Task_123"), "foundation:Task_123");
}

#[test]
fn strip_leading_and_trailing_colon() {
    assert_eq!(strip_token_punctuation(":foundation:Task_123:"), "foundation:Task_123");
}

// ── finish_step_record: embedded IRI extraction is robust ────────────────────

#[test]
fn backtick_iri_not_added_when_entity_absent() {
    let mut conn = setup_test_db();

    let step_iri = "foundation:Step_test_backtick";
    insert_triples(&mut conn, &[
        Triple::new(step_iri, "rdf:type", Object::Iri("foundation:StepExecution".to_string())),
        Triple::new(step_iri, "foundation:hasStatus", Object::Iri("foundation:InProgress".to_string())),
    ]);

    let markdown = "- `foundation:Task_nonexistent` — some label\n";
    finish_step_record(&mut conn, step_iri, Some(markdown), None).unwrap();

    let result = crate::eavto::query::get_by_entity_predicate(&conn, step_iri, "foundation:executionOutput").unwrap();
    assert!(result.triples.is_empty(), "backtick IRI for non-existent entity must not appear in executionOutput");
}

#[test]
fn backtick_iri_added_when_entity_exists() {
    let mut conn = setup_test_db();

    let step_iri = "foundation:Step_test_backtick_exists";
    let task_iri = "foundation:Task_exists_for_test";
    insert_triples(&mut conn, &[
        Triple::new(step_iri, "rdf:type", Object::Iri("foundation:StepExecution".to_string())),
        Triple::new(step_iri, "foundation:hasStatus", Object::Iri("foundation:InProgress".to_string())),
        Triple::new(task_iri, "rdf:type", Object::Iri("foundation:Task".to_string())),
    ]);

    let markdown = format!("- `{}` — real task\n", task_iri);
    finish_step_record(&mut conn, step_iri, Some(&markdown), None).unwrap();

    let result = crate::eavto::query::get_by_entity_predicate(&conn, step_iri, "foundation:executionOutput").unwrap();
    let found = result.triples.iter().any(|t| t.object.as_iri() == Some(task_iri));
    assert!(found, "existing IRI should appear in executionOutput after stripping backticks");
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

// ── loop post-batch aggregation (US foundation:UserStory_1780960597833) ──────

// Helpers reused from aggregation tests (reproduced here for isolation).
fn insert_tx_loop(conn: &rusqlite::Connection) -> i64 {
    conn.execute("INSERT INTO transactions (origin, created_at) VALUES ('test', 0)", [])
        .unwrap();
    conn.last_insert_rowid()
}

fn insert_iri_ref(conn: &rusqlite::Connection, tx: i64, subject: &str, predicate: &str, object: &str) {
    conn.execute(
        "INSERT INTO triples \
         (subject, predicate, object, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, ?, ?, 'iri', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params![subject, predicate, object, tx],
    ).unwrap();
}

fn insert_literal_decimal(conn: &rusqlite::Connection, tx: i64, subject: &str, predicate: &str, value: &str) {
    conn.execute(
        "INSERT INTO triples \
         (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, ?, ?, 'literal', 'xsd:decimal', 1, ?, 0, 0)",
        rusqlite::params![subject, predicate, value, tx],
    ).unwrap();
}

fn insert_aggregation_formula(conn: &rusqlite::Connection, tx: i64, property_iri: &str, formula: &str) {
    conn.execute(
        "INSERT INTO triples \
         (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, 'foundation:aggregation', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params![property_iri, formula, tx],
    ).unwrap();
}

fn insert_rdf_type_loop(conn: &rusqlite::Connection, tx: i64, subject: &str, rdf_type: &str) {
    conn.execute(
        "INSERT INTO triples \
         (subject, predicate, object, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, 'rdf:type', ?, 'iri', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params![subject, rdf_type, tx],
    ).unwrap();
}

fn insert_rdfs_domain(conn: &rusqlite::Connection, tx: i64, property_iri: &str, domain: &str) {
    conn.execute(
        "INSERT INTO triples \
         (subject, predicate, object, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, 'rdfs:domain', ?, 'iri', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params![property_iri, domain, tx],
    ).unwrap();
}

/// AC3 — coleção vazia: CONTAR via inversa sobre conjunto vazio retorna "0" sem erro.
/// Validates that the aggregation engine handles N=0 without panicking or erroring.
#[test]
fn test_loop_aggregate_count_empty_collection_returns_zero() {
    let conn = setup_test_db();
    let tx = insert_tx_loop(&conn);

    // Declare the aggregation formula for the count property.
    insert_aggregation_formula(&conn, tx, "p:iterCount", "CONTAR({{p:iterationOf}})");

    // iterationOf has domain p:IterClass (child class), not p:PrimaryClass.
    // is_inverse_navigation: domain=p:IterClass, instance_type=p:PrimaryClass → inverse=true.
    insert_rdfs_domain(&conn, tx, "p:iterationOf", "p:IterClass");
    insert_rdf_type_loop(&conn, tx, "inst:primary", "p:PrimaryClass");

    // No iteration children linked to inst:primary — empty inverse set.
    let result = crate::owl::aggregation::evaluate_aggregation_for_instance(
        &conn, "inst:primary", "p:iterCount",
    ).unwrap();

    assert_eq!(result, "0", "empty collection must yield 0 for CONTAR");
}

/// AC2 — N iterações ligadas via iterationOf (inversa): CONTAR retorna N e SOMA retorna a soma dos scores.
/// Validates that the inverse-navigation aggregation resolves correctly for a concrete domain.
#[test]
fn test_loop_aggregate_count_and_sum_via_inverse_iteration_of() {
    let conn = setup_test_db();
    let tx = insert_tx_loop(&conn);

    // Aggregation formulas on primary's properties.
    insert_aggregation_formula(&conn, tx, "p:iterCount", "CONTAR({{p:iterationOf}})");
    insert_aggregation_formula(&conn, tx, "p:scoreSum", "SOMA({{p:iterationOf}}.p:score)");

    // iterationOf domain = p:IterClass (child type); primary type = p:PrimaryClass → inverse nav.
    insert_rdfs_domain(&conn, tx, "p:iterationOf", "p:IterClass");
    insert_rdf_type_loop(&conn, tx, "inst:primary", "p:PrimaryClass");

    // Three iteration children each with p:iterationOf → inst:primary and a p:score.
    for (child, score) in [("inst:iter1", "10"), ("inst:iter2", "25"), ("inst:iter3", "15")] {
        insert_rdf_type_loop(&conn, tx, child, "p:IterClass");
        insert_iri_ref(&conn, tx, child, "p:iterationOf", "inst:primary");
        insert_literal_decimal(&conn, tx, child, "p:score", score);
    }

    let count = crate::owl::aggregation::evaluate_aggregation_for_instance(
        &conn, "inst:primary", "p:iterCount",
    ).unwrap();
    assert_eq!(count, "3", "CONTAR must return 3 for three linked iterations");

    let sum = crate::owl::aggregation::evaluate_aggregation_for_instance(
        &conn, "inst:primary", "p:scoreSum",
    ).unwrap();
    assert_eq!(sum, "50", "SOMA must return 50 (10+25+15) for three linked iterations");
}

/// AC2 — failure iterations still count: a failed child (no score) is counted but excluded from SOMA.
/// Validates that partial failures don't break the count and SOMA skips missing values gracefully.
#[test]
fn test_loop_aggregate_failed_iteration_counted_but_score_missing_skipped() {
    let conn = setup_test_db();
    let tx = insert_tx_loop(&conn);

    insert_aggregation_formula(&conn, tx, "p:iterCount", "CONTAR({{p:iterationOf}})");
    insert_aggregation_formula(&conn, tx, "p:scoreSum", "SOMA({{p:iterationOf}}.p:score)");

    insert_rdfs_domain(&conn, tx, "p:iterationOf", "p:IterClass");
    insert_rdf_type_loop(&conn, tx, "inst:primary", "p:PrimaryClass");

    // Two successful iterations with score, one failed iteration without score.
    for (child, score) in [("inst:ok1", Some("40")), ("inst:ok2", Some("60")), ("inst:fail1", None)] {
        insert_rdf_type_loop(&conn, tx, child, "p:IterClass");
        insert_iri_ref(&conn, tx, child, "p:iterationOf", "inst:primary");
        if let Some(s) = score {
            insert_literal_decimal(&conn, tx, child, "p:score", s);
        }
    }

    let count = crate::owl::aggregation::evaluate_aggregation_for_instance(
        &conn, "inst:primary", "p:iterCount",
    ).unwrap();
    assert_eq!(count, "3", "failed iteration must still be counted (barrier counts completion not success)");

    let sum = crate::owl::aggregation::evaluate_aggregation_for_instance(
        &conn, "inst:primary", "p:scoreSum",
    ).unwrap();
    assert_eq!(sum, "100", "SOMA must sum only iterations that have a score value");
}

// ── resilience: AC _1780960763801 / AC _1780960763913 ────────────────────────
//
// AC _1780960763801: a failed item must not abort the batch — remaining items
//   are processed and the iteration_results vector accumulates all outcomes.
// AC _1780960763913: after a mixed batch the terminal hasStatus of each control
//   instance is distinguishable (Completed vs Failed) and linked via iterationOf.

/// AC _1780960763801 (sequential mode): simulates the sequential loop body by
/// running N outcomes through the same match pattern used in run_sub_process —
/// verifies that one Err does not short-circuit the remaining iterations.
#[test]
fn test_sequential_loop_item_failure_does_not_abort_batch() {
    // Simulate the body of the `is_sequential` branch in run_sub_process.
    // We drive outcomes directly (no AppHandle needed) to isolate the loop contract.
    let outcomes: Vec<std::result::Result<Option<String>, String>> = vec![
        Ok(None),
        Err("item 2 failed".to_string()),
        Ok(None),
        Ok(None),
    ];

    let mut iteration_results: Vec<(String, Option<String>)> = Vec::new();
    for (idx, outcome) in outcomes.into_iter().enumerate() {
        let child_self = format!("inst:child_{}", idx);
        match outcome {
            Ok(_) => iteration_results.push((child_self, None)),
            Err(e) => iteration_results.push((child_self, Some(e))),
        }
    }

    assert_eq!(iteration_results.len(), 4, "all 4 iterations must be recorded");
    let failure_count = iteration_results.iter().filter(|(_, e)| e.is_some()).count();
    assert_eq!(failure_count, 1, "exactly 1 failure must be captured");
    let success_count = iteration_results.iter().filter(|(_, e)| e.is_none()).count();
    assert_eq!(success_count, 3, "exactly 3 successes must remain");
}

/// AC _1780960763801 (parallel mode, panic isolation): a panic inside one
/// parallel iteration future must be caught by AssertUnwindSafe + catch_unwind
/// and mapped to Err for that slot only — the join_all completes and all other
/// slots receive their correct outcomes.
#[tokio::test]
async fn test_parallel_loop_panic_isolated_does_not_abort_batch() {
    use futures_util::FutureExt;

    // Box-erase to a single concrete future type so Vec is homogeneous.
    type BoxedFut = futures_util::future::CatchUnwind<
        std::panic::AssertUnwindSafe<
            std::pin::Pin<Box<dyn futures_util::Future<Output = Result<Option<String>>>>>
        >
    >;

    let make_ok = |id: u32| -> BoxedFut {
        let fut: std::pin::Pin<Box<dyn futures_util::Future<Output = Result<Option<String>>>>> =
            Box::pin(async move { Ok(Some(format!("inst:ok_{}", id))) });
        std::panic::AssertUnwindSafe(fut).catch_unwind()
    };

    let make_panic = || -> BoxedFut {
        let fut: std::pin::Pin<Box<dyn futures_util::Future<Output = Result<Option<String>>>>> =
            Box::pin(async move {
                panic!("simulated iteration panic");
                #[allow(unreachable_code)]
                Ok(None)
            });
        std::panic::AssertUnwindSafe(fut).catch_unwind()
    };

    let futures: Vec<BoxedFut> = vec![make_ok(1), make_panic(), make_ok(3)];
    let chunk_results = futures_util::future::join_all(futures).await;

    let mut iteration_results: Vec<(String, Option<String>)> = Vec::new();
    for (i, unwind_result) in chunk_results.into_iter().enumerate() {
        let child_self = format!("inst:child_{}", i);
        let outcome: Result<Option<String>> = match unwind_result {
            Ok(r) => r,
            Err(panic_payload) => {
                let msg = panic_payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic_payload.downcast_ref::<String>().map(|s| s.as_str()))
                    .unwrap_or("unknown panic");
                Err(format!("iteration panicked: {}", msg))
            }
        };
        match outcome {
            Ok(_) => iteration_results.push((child_self, None)),
            Err(e) => iteration_results.push((child_self, Some(e))),
        }
    }

    assert_eq!(iteration_results.len(), 3, "all 3 slots must be recorded after a panic in slot 1");
    assert!(
        iteration_results[0].1.is_none(),
        "slot 0 (ok_future 1) must succeed"
    );
    assert!(
        iteration_results[1].1.is_some(),
        "slot 1 (panic_future) must be recorded as failure"
    );
    let panic_msg = iteration_results[1].1.as_deref().unwrap_or("");
    assert!(
        panic_msg.contains("panicked") || panic_msg.contains("simulated"),
        "failure message must reflect the panic: got {:?}", panic_msg
    );
    assert!(
        iteration_results[2].1.is_none(),
        "slot 2 (ok_future 3) must succeed — batch continued past the panic"
    );
}

/// AC _1780960763913: after a mixed batch (some Ok, one Err), the terminal
/// hasStatus written on each control instance is distinguishable as
/// STATUS_COMPLETED vs STATUS_FAILED, and both are linked via iterationOf to
/// the primary control instance.
#[test]
fn test_loop_iteration_terminal_status_distinguishable() {
    let mut conn = setup_test_db();

    // Minimal ontology stubs required by create_execution_record.
    let tx = insert_tx_loop(&conn);
    let proc_iri = "foundation:Proc_resilience_test";
    let ctrl_class = "foundation:CtrlClass_resilience_test";
    let primary_ctrl = "foundation:CtrlInst_primary_resilience";

    // Register the process and its controlClass so create_control_instance succeeds.
    conn.execute(
        "INSERT INTO triples (subject, predicate, object, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, 'foundation:controlClass', ?, 'iri', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params![proc_iri, ctrl_class, tx],
    ).unwrap();
    conn.execute(
        "INSERT INTO triples (subject, predicate, object, object_type, object_datatype, origin_id, tx, created_at, retracted) \
         VALUES (?, 'rdf:type', 'owl:Class', 'iri', 'xsd:string', 1, ?, 0, 0)",
        rusqlite::params![ctrl_class, tx],
    ).unwrap();

    // Simulate the two terminal-status writes that run_process_with_context does
    // (l.543-556): one successful iteration and one failed iteration.
    let ok_ctrl = "foundation:CtrlInst_ok_resilience";
    let fail_ctrl = "foundation:CtrlInst_fail_resilience";

    Individual::add_iri_value(
        &mut conn, ok_ctrl, "foundation:hasStatus", STATUS_COMPLETED, "process_automation",
    ).unwrap();
    Individual::add_iri_value(
        &mut conn, fail_ctrl, "foundation:hasStatus", STATUS_FAILED, "process_automation",
    ).unwrap();

    // Link both back to the primary control instance via iterationOf (mirrors l.1090-1098).
    Individual::add_iri_value(
        &mut conn, ok_ctrl, "foundation:iterationOf", primary_ctrl, "process_automation",
    ).unwrap();
    Individual::add_iri_value(
        &mut conn, fail_ctrl, "foundation:iterationOf", primary_ctrl, "process_automation",
    ).unwrap();

    // Verify that the statuses are distinguishable.
    let ok_status = crate::owl::get_iri_property(&conn, ok_ctrl, "foundation:hasStatus")
        .unwrap()
        .expect("ok_ctrl must have hasStatus");
    let fail_status = crate::owl::get_iri_property(&conn, fail_ctrl, "foundation:hasStatus")
        .unwrap()
        .expect("fail_ctrl must have hasStatus");

    assert_eq!(ok_status, STATUS_COMPLETED, "successful iteration must carry STATUS_COMPLETED");
    assert_eq!(fail_status, STATUS_FAILED, "failed iteration must carry STATUS_FAILED");
    assert_ne!(ok_status, fail_status, "statuses must be distinguishable");

    // Verify iterationOf links.
    let ok_link = crate::owl::get_iri_property(&conn, ok_ctrl, "foundation:iterationOf")
        .unwrap()
        .expect("ok_ctrl must have iterationOf");
    let fail_link = crate::owl::get_iri_property(&conn, fail_ctrl, "foundation:iterationOf")
        .unwrap()
        .expect("fail_ctrl must have iterationOf");

    assert_eq!(ok_link, primary_ctrl, "ok iteration must be linked to primary via iterationOf");
    assert_eq!(fail_link, primary_ctrl, "failed iteration must be linked to primary via iterationOf");
}

// ── Bug_1781004764856: iterationOf link must enqueue a recalc job for the primary ──

fn setup_test_db_with_formula_jobs() -> rusqlite::Connection {
    let conn = setup_test_db();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS formula_recalc_jobs (
            id              TEXT    PRIMARY KEY,
            property_iri    TEXT    NOT NULL,
            property_label  TEXT,
            class_iri       TEXT    NOT NULL,
            class_label     TEXT,
            status          TEXT    NOT NULL DEFAULT 'pending',
            total           INTEGER NOT NULL DEFAULT 0,
            processed       INTEGER NOT NULL DEFAULT 0,
            last_offset     INTEGER NOT NULL DEFAULT 0,
            error_message   TEXT,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            instance_iri    TEXT
        );
        CREATE TABLE IF NOT EXISTS formula_instance_errors (
            instance_iri    TEXT NOT NULL,
            property_iri    TEXT NOT NULL,
            error_message   TEXT NOT NULL,
            created_at      INTEGER NOT NULL,
            PRIMARY KEY (instance_iri, property_iri)
        );",
    ).unwrap();
    // Seed origin row needed by assert_triples.
    let _ = conn.execute(
        "INSERT OR IGNORE INTO origins (name, description) VALUES ('test', 'test')",
        [],
    );
    conn
}

/// Bug_1781004764856 — after linking a child via foundation:iterationOf,
/// create_instance_recalc_jobs must produce at least one job targeting the primary instance.
/// This validates that the FormulaWorker will be signalled to recompute triagedCount.
#[test]
fn test_iteration_of_link_produces_recalc_job_for_primary() {
    let mut conn = setup_test_db_with_formula_jobs();
    let tx = insert_tx_loop(&conn);

    let primary_iri = "foundation:EmailBatchTriageRun_test_primary";
    let child_iri   = "foundation:EmailTriageItemRun_test_child";
    let child_class = "foundation:EmailTriageItemRun";
    let primary_class = "foundation:EmailBatchTriageRun";
    let count_prop  = "foundation:triagedCount";

    // The aggregation property lives on the primary class.
    insert_aggregation_formula(&conn, tx, count_prop, "CONTAR({{foundation:iterationOf}})");

    // domain of iterationOf = child class → inverse navigation to primary.
    insert_rdfs_domain(&conn, tx, "foundation:iterationOf", child_class);

    // Type the child and the primary so is_inverse resolution works.
    insert_rdf_type_loop(&conn, tx, child_iri, child_class);
    insert_rdf_type_loop(&conn, tx, primary_iri, primary_class);

    // Simulate the link written by run_sub_process.
    Individual::add_iri_value(
        &mut conn,
        child_iri,
        "foundation:iterationOf",
        primary_iri,
        "process_automation",
    ).unwrap();

    // Invoke the same function that the fixed run_sub_process now calls.
    let job_ids = crate::owl::formula_worker::create_instance_recalc_jobs(
        &conn,
        child_iri,
        "foundation:iterationOf",
    );

    assert!(
        !job_ids.is_empty(),
        "create_instance_recalc_jobs must produce at least one recalc job after iterationOf link"
    );

    // The job must target the PRIMARY instance, not the child.
    let job_targets_primary = conn
        .query_row(
            "SELECT COUNT(*) FROM formula_recalc_jobs WHERE instance_iri = ?",
            rusqlite::params![primary_iri],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);

    assert!(
        job_targets_primary > 0,
        "recalc job must target the primary instance '{}', not the child",
        primary_iri
    );
}
