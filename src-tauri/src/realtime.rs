use std::collections::HashSet;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

/// App-level pub/sub registry for entity events.
///
/// The frontend declares which entity IRIs it currently shows (exact match) plus
/// optional IRI substring patterns for collection views (e.g. a Notification Center
/// watching every `foundation:AINotification_*`). The backend emits `entity-updated`,
/// `entity-referenced` and `entity-deleted` ONLY for entities matching the current set.
///
/// The set is replaced wholesale on every change, so a webview reload self-heals:
/// the old subscriptions are dropped the moment the freshly mounted UI re-declares
/// what it shows. No reference counting on the backend — the frontend owns the truth.
#[derive(Default)]
pub struct SubscriptionRegistry {
    inner: Mutex<Subscriptions>,
}

#[derive(Default)]
struct Subscriptions {
    iris: HashSet<String>,
    patterns: Vec<String>,
}

impl SubscriptionRegistry {
    pub fn replace(&self, iris: Vec<String>, patterns: Vec<String>) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.iris = iris.into_iter().collect();
            guard.patterns = patterns;
        }
    }

    pub fn is_subscribed(&self, entity_id: &str) -> bool {
        match self.inner.lock() {
            Ok(guard) => {
                guard.iris.contains(entity_id)
                    || guard.patterns.iter().any(|p| entity_id.contains(p.as_str()))
            }
            Err(_) => false,
        }
    }
}

/// True when the entity is currently shown by the frontend. When no registry is
/// managed (CI / in-memory mode) there is no UI to filter for, so emit unconditionally.
fn subscribed(app: &AppHandle, entity_id: &str) -> bool {
    app.try_state::<SubscriptionRegistry>()
        .map(|r| r.is_subscribed(entity_id))
        .unwrap_or(true)
}

pub fn emit_entity_updated(app: &AppHandle, entity_id: &str) {
    if subscribed(app, entity_id) {
        app.emit("entity-updated", serde_json::json!({ "entityId": entity_id })).ok();
    }
}

pub fn emit_entity_updated_with(app: &AppHandle, entity_id: &str, changed_predicates: &[String]) {
    if subscribed(app, entity_id) {
        app.emit("entity-updated", serde_json::json!({
            "entityId": entity_id,
            "changedPredicates": changed_predicates,
        })).ok();
    }
}

pub fn emit_entity_referenced(app: &AppHandle, entity_id: &str) {
    if subscribed(app, entity_id) {
        app.emit("entity-referenced", serde_json::json!({ "entityId": entity_id })).ok();
    }
}

pub fn emit_entity_deleted(app: &AppHandle, entity_id: &str) {
    if subscribed(app, entity_id) {
        app.emit("entity-deleted", serde_json::json!({ "entityId": entity_id })).ok();
    }
}

/// Emit a pre-built event queued during a batch, honoring the registry for entity-*
/// events while passing every other event through untouched.
pub fn emit_queued(app: &AppHandle, name: &str, payload: serde_json::Value) {
    let gated = matches!(name, "entity-updated" | "entity-referenced" | "entity-deleted");
    if gated {
        if let Some(entity_id) = payload.get("entityId").and_then(|v| v.as_str()) {
            if !subscribed(app, entity_id) {
                return;
            }
        }
    }
    app.emit(name, payload).ok();
}

/// Frontend declares the full set of entity IRIs (and IRI substring patterns) it is
/// currently displaying. Replaces the previous set; called on every mount/unmount.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn events__set_subscriptions(
    iris: Vec<String>,
    patterns: Vec<String>,
    registry: State<'_, SubscriptionRegistry>,
) -> Result<(), String> {
    crate::diagnostics::log_backend(
        "debug",
        &format!("[REALTIME] subscriptions set: {} iris, {} patterns", iris.len(), patterns.len()),
    );
    registry.replace(iris, patterns);
    Ok(())
}
