use serde_json::json;
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationCorrelation {
    pub execution_iri: String,
    pub step_iri: String,
    pub node_iri: String,
    pub source_entity_iri: Option<String>,
    pub source_update_id: Option<String>,
    pub source_tx: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializationTerminal {
    Failed(String),
    Cancelled(String),
}

pub fn source_update_id(entity_iri: &str, source_tx: i64) -> String {
    format!("{}@{}", entity_iri, source_tx)
}

impl From<String> for MaterializationTerminal {
    fn from(reason: String) -> Self {
        Self::Failed(reason)
    }
}

impl std::fmt::Display for MaterializationTerminal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(reason) | Self::Cancelled(reason) => formatter.write_str(reason),
        }
    }
}

pub fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
    correlation: &MaterializationCorrelation,
    phase: &str,
    status: &str,
    entity_iri: &str,
    property_iri: Option<&str>,
    completed: Option<usize>,
    total: Option<usize>,
    reason: Option<&str>,
) {
    let mut payload = json!({
        "executionIri": correlation.execution_iri,
        "stepIri": correlation.step_iri,
        "nodeIri": correlation.node_iri,
        "phase": phase,
        "status": status,
        "entityIri": entity_iri,
    });
    if let Some(source_entity) = correlation.source_entity_iri.as_deref() {
        payload["sourceEntityIri"] = json!(source_entity);
    }
    if let Some(update_id) = correlation.source_update_id.as_deref() {
        payload["sourceUpdateId"] = json!(update_id);
    }
    if let Some(tx) = correlation.source_tx {
        payload["sourceTx"] = json!(tx);
    }
    if let Some(property) = property_iri {
        payload["propertyIri"] = json!(property);
    }
    if let Some(count) = completed {
        payload["completedCount"] = json!(count);
    }
    if let Some(count) = total {
        payload["totalCount"] = json!(count);
    }
    if let Some(error) = reason {
        payload["reason"] = json!(error);
    }
    let _ = app.emit("automation-step-progress", payload);
}

#[cfg(test)]
mod tests {
    use super::source_update_id;

    #[test]
    fn source_update_identity_distinguishes_entity_and_revision() {
        let concurrent_execution_one = source_update_id("foundation:Conversation_1", 42);
        let concurrent_execution_two = source_update_id("foundation:Conversation_1", 42);
        assert_eq!(concurrent_execution_one, concurrent_execution_two);
        assert_ne!(
            concurrent_execution_one,
            source_update_id("foundation:Conversation_1", 43)
        );
        assert_ne!(
            concurrent_execution_one,
            source_update_id("foundation:Conversation_2", 42)
        );
        assert_eq!(concurrent_execution_one, "foundation:Conversation_1@42");
    }
}
