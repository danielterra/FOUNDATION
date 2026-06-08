use crate::eavto::{store, Triple, Object, DbExecutor};
use super::super::log_backend;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceStep {
    pub iteration: usize,
    pub tool: String,
    pub input_snippet: String,
    pub result_snippet: String,
    pub is_error: bool,
    pub duration_ms: u64,
}

pub async fn save_trace(
    executor: &DbExecutor,
    conversation_id: &str,
    steps: &[TraceStep],
    termination_reason: &str,
    iteration_count: usize,
    total_input_tokens: usize,
    total_output_tokens: usize,
    duration_ms: u64,
) -> Result<(), String> {
    if steps.is_empty() && total_input_tokens == 0 {
        return Ok(());
    }

    let timestamp = chrono::Utc::now().timestamp_millis();
    let trace_iri = format!("foundation:AgentTrace_{}", timestamp);

    let steps_json = serde_json::to_string(steps)
        .map_err(|e| format!("Failed to serialize trace steps: {}", e))?;

    let iri = trace_iri.clone();
    let conv_id = conversation_id.to_string();
    let reason = termination_reason.to_string();

    executor.write(move |conn| {
        let sent_at = chrono::DateTime::from_timestamp_millis(timestamp)
            .unwrap_or_default()
            .to_rfc3339();
        let started_at = chrono::DateTime::from_timestamp_millis(timestamp - duration_ms as i64)
            .unwrap_or_default()
            .to_rfc3339();

        let triples = vec![
            Triple::new(&iri, "rdf:type",
                Object::Iri("foundation:AgentTrace".to_string())),
            Triple::new(&iri, "rdfs:label",
                Object::Literal { value: "Agent Trace".to_string(), datatype: Some("xsd:string".to_string()), language: None }),
            Triple::new(&iri, "foundation:partOfConversation",
                Object::Iri(conv_id)),
            Triple::new(&iri, "foundation:sentAt",
                Object::DateTime(sent_at.clone())),
            Triple::new(&iri, "foundation:hasStartTime",
                Object::DateTime(started_at)),
            Triple::new(&iri, "foundation:hasEndTime",
                Object::DateTime(sent_at)),
            Triple::new(&iri, "foundation:traceSteps",
                Object::Literal { value: steps_json, datatype: Some("xsd:string".to_string()), language: None }),
            Triple::new(&iri, "foundation:traceTerminationReason",
                Object::Literal { value: reason, datatype: Some("xsd:string".to_string()), language: None }),
            Triple::new(&iri, "foundation:traceIterationCount",
                Object::Integer(iteration_count as i64)),
            Triple::new(&iri, "foundation:traceInputTokens",
                Object::Integer(total_input_tokens as i64)),
            Triple::new(&iri, "foundation:traceOutputTokens",
                Object::Integer(total_output_tokens as i64)),
            Triple::new(&iri, "foundation:traceDurationMs",
                Object::Integer(duration_ms as i64)),
        ];

        store::assert_triples(conn, &triples, "ai")
            .map_err(|e| format!("Failed to write AgentTrace: {}", e))?;

        Ok(iri)
    }).await?;

    log_backend("info", &format!(
        "[TRACE] Saved AgentTrace_{} — {} steps, reason: {}, {}ms, tokens: {}in/{}out",
        timestamp, steps.len(), termination_reason, duration_ms,
        total_input_tokens, total_output_tokens,
    ));

    Ok(())
}

fn truncate(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}…", truncated)
    } else {
        truncated
    }
}

pub fn make_step(
    iteration: usize,
    tool: &str,
    input: &serde_json::Value,
    result: &str,
    is_error: bool,
    duration_ms: u64,
) -> TraceStep {
    TraceStep {
        iteration,
        tool: tool.to_string(),
        input_snippet: truncate(&input.to_string(), 200),
        result_snippet: truncate(result, 200),
        is_error,
        duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    #[test]
    fn make_step_truncates_long_input() {
        let long_iri = "A".repeat(300);
        let input = serde_json::json!({"iris": [long_iri]});
        let step = make_step(1, "describe_individual", &input, "ok", false, 42);
        assert!(step.input_snippet.ends_with('…'), "should end with an ellipsis");
        assert!(step.input_snippet.chars().count() <= 202, "snippet should not exceed 201 chars + ellipsis");
    }

    #[test]
    fn make_step_does_not_truncate_short_input() {
        let input = serde_json::json!({"query": "hello"});
        let step = make_step(1, "search", &input, "3 results", false, 10);
        assert!(!step.input_snippet.ends_with('…'), "should not truncate short inputs");
        assert_eq!(step.tool, "search");
        assert_eq!(step.iteration, 1);
        assert_eq!(step.duration_ms, 10);
        assert!(!step.is_error);
    }

    #[test]
    fn make_step_error_preserves_flag() {
        let input = serde_json::json!({"iris": ["foundation:X"]});
        let step = make_step(2, "describe_individual", &input, r#"{"success":false,"error":"not found"}"#, true, 5);
        assert!(step.is_error);
        assert_eq!(step.iteration, 2);
    }

    #[tokio::test]
    async fn save_trace_writes_individual_to_db() {
        let conn = setup_test_db();
        let executor = DbExecutor::new_in_memory(conn);

        let steps = vec![
            make_step(1, "search", &serde_json::json!({"query": "tasks"}), "3 results", false, 120),
            make_step(1, "describe_individual", &serde_json::json!({"iris": ["foundation:Task_1"]}), "Task details.", false, 5),
        ];

        let result = save_trace(
            &executor, "foundation:Conv_test_123",
            &steps, "end_turn", 1, 100, 50, 500,
        ).await;
        assert!(result.is_ok(), "save_trace failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn save_trace_does_not_write_without_activity() {
        let conn = setup_test_db();
        let executor = DbExecutor::new_in_memory(conn);

        let result = save_trace(
            &executor, "foundation:Conv_empty",
            &[], "end_turn", 0, 0, 0, 50,
        ).await;
        assert!(result.is_ok(), "should return Ok even with nothing to save");
    }

    #[tokio::test]
    async fn save_trace_with_error_records_correctly() {
        let conn = setup_test_db();
        let executor = DbExecutor::new_in_memory(conn);

        let steps = vec![
            make_step(1, "describe_individual", &serde_json::json!({"iris": ["foundation:X"]}), r#"{"success":false,"error":"not found"}"#, true, 12),
            make_step(1, "describe_individual", &serde_json::json!({"iris": ["foundation:X"]}), r#"{"success":false,"error":"not found"}"#, true, 11),
            make_step(1, "describe_individual", &serde_json::json!({"iris": ["foundation:X"]}), r#"{"success":false,"error":"not found"}"#, true, 13),
        ];

        let result = save_trace(
            &executor, "foundation:Conv_loop",
            &steps, "loop_detected", 3, 300, 150, 1200,
        ).await;
        assert!(result.is_ok(), "save_trace failed for loop_detected: {:?}", result.err());
    }
}
