use serde_json::json;
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationCorrelation {
    pub execution_iri: String,
    pub step_iri: String,
    pub node_iri: String,
    pub source_tx: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializationTerminal {
    Failed(String),
    Cancelled(String),
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
