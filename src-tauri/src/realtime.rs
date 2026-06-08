use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

/// A creation-query declares interest in new instances of a class that have a specific
/// property pointing at a specific value (e.g. AIConversationMessage.partOfConversation=convIri).
/// The backend matches new triples against registered queries in memory — no DB query per emit.
#[derive(Clone, Debug)]
pub struct CreationQuery {
    /// IRI of the class the new subject must belong to (matched against rdf:type triples).
    pub class_iri: String,
    /// Predicate the new triple must carry.
    pub predicate: String,
    /// Object IRI or literal value the predicate must equal.
    pub object_value: String,
}

/// A single event stored in the in-memory ring for cursor-based replay.
#[derive(Clone, Debug)]
pub struct ReplayEvent {
    pub tx: i64,
    pub event_name: String,
    pub entity_id: String,
    /// Predicates changed — only meaningful for entity-updated events.
    pub changed_predicates: Vec<String>,
    /// Class, predicate and object value carried by `entity-joined-set` events —
    /// must reach the frontend on replay so consumers can match their creation-query.
    /// `None` for the other event types.
    pub class_iri: Option<String>,
    pub predicate: Option<String>,
    pub object_value: Option<String>,
}

/// Maximum number of events kept in the replay ring.
const RING_CAPACITY: usize = 1024;

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
///
/// Extended with:
/// - creation-queries (Classe+Prop=Valor) matched in memory from the write payload
/// - a ring of recent events keyed by tx for lossless replay during subscription gaps
#[derive(Default)]
pub struct SubscriptionRegistry {
    inner: Mutex<Subscriptions>,
}

#[derive(Default)]
struct Subscriptions {
    iris: HashSet<String>,
    patterns: Vec<String>,
    creation_queries: Vec<CreationQuery>,
    ring: VecDeque<ReplayEvent>,
}

impl SubscriptionRegistry {
    pub fn replace(
        &self,
        iris: Vec<String>,
        patterns: Vec<String>,
        creation_queries: Vec<CreationQuery>,
    ) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.iris = iris.into_iter().collect();
            guard.patterns = patterns;
            guard.creation_queries = creation_queries;
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

    /// Returns all creation-queries currently registered.
    pub fn creation_queries(&self) -> Vec<CreationQuery> {
        self.inner.lock()
            .map(|g| g.creation_queries.clone())
            .unwrap_or_default()
    }

    /// Push an event into the replay ring, dropping the oldest entry when full.
    pub fn push_ring(&self, event: ReplayEvent) {
        if let Ok(mut guard) = self.inner.lock() {
            if guard.ring.len() >= RING_CAPACITY {
                guard.ring.pop_front();
            }
            guard.ring.push_back(event);
        }
    }

    /// Returns events with tx >= since_tx in ascending tx order (ring path).
    /// The inclusive lower-bound covers the edge case where the frontend snapshot
    /// captured exactly the TX of a message that was not yet delivered — using `>`
    /// would permanently drop that event from replay.
    /// Callers fall back to the triples table when the ring does not cover the cursor.
    pub fn replay_since(&self, since_tx: i64) -> Vec<ReplayEvent> {
        self.inner.lock()
            .map(|guard| {
                guard.ring.iter()
                    .filter(|e| e.tx >= since_tx)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns the minimum tx stored in the ring, or i64::MAX when empty.
    /// Used by callers to decide whether the ring covers the requested since_tx.
    pub fn ring_min_tx(&self) -> i64 {
        self.inner.lock()
            .map(|guard| guard.ring.front().map(|e| e.tx).unwrap_or(i64::MAX))
            .unwrap_or(i64::MAX)
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
        let payload = serde_json::json!({ "entityId": entity_id });
        if let Some(registry) = app.try_state::<SubscriptionRegistry>() {
            registry.push_ring(ReplayEvent {
                tx: 0,
                event_name: "entity-updated".into(),
                entity_id: entity_id.to_owned(),
                changed_predicates: vec![],
                class_iri: None,
                predicate: None,
                object_value: None,
            });
        }
        app.emit("entity-updated", payload).ok();
    }
}

pub fn emit_entity_updated_with(app: &AppHandle, entity_id: &str, changed_predicates: &[String]) {
    emit_entity_updated_with_tx(app, entity_id, changed_predicates, 0);
}

pub fn emit_entity_updated_with_tx(app: &AppHandle, entity_id: &str, changed_predicates: &[String], tx: i64) {
    if subscribed(app, entity_id) {
        let payload = serde_json::json!({
            "entityId": entity_id,
            "changedPredicates": changed_predicates,
            "tx": tx,
        });
        if let Some(registry) = app.try_state::<SubscriptionRegistry>() {
            registry.push_ring(ReplayEvent {
                tx,
                event_name: "entity-updated".into(),
                entity_id: entity_id.to_owned(),
                changed_predicates: changed_predicates.to_vec(),
                class_iri: None,
                predicate: None,
                object_value: None,
            });
        }
        app.emit("entity-updated", payload).ok();
    }
}

pub fn emit_entity_referenced(app: &AppHandle, entity_id: &str) {
    emit_entity_referenced_with_tx(app, entity_id, 0);
}

pub fn emit_entity_referenced_with_tx(app: &AppHandle, entity_id: &str, tx: i64) {
    if subscribed(app, entity_id) {
        let payload = serde_json::json!({ "entityId": entity_id, "tx": tx });
        if let Some(registry) = app.try_state::<SubscriptionRegistry>() {
            registry.push_ring(ReplayEvent {
                tx,
                event_name: "entity-referenced".into(),
                entity_id: entity_id.to_owned(),
                changed_predicates: vec![],
                class_iri: None,
                predicate: None,
                object_value: None,
            });
        }
        app.emit("entity-referenced", payload).ok();
    }
}

pub fn emit_entity_deleted(app: &AppHandle, entity_id: &str) {
    if subscribed(app, entity_id) {
        app.emit("entity-deleted", serde_json::json!({ "entityId": entity_id })).ok();
    }
}

/// Non-gated internal signal for backend reactors (task execution, recurrence,
/// scheduler). Unlike `entity-updated`/`entity-created` — which the frontend
/// subscription registry filters — this fires for EVERY written subject so backend
/// logic runs whether or not the UI is currently showing the entity. The frontend
/// must never listen to this; it exists purely to drive server-side automation.
pub fn emit_entity_changed_internal(app: &AppHandle, entity_id: &str) {
    app.emit("entity-changed-internal", serde_json::json!({ "entityId": entity_id })).ok();
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

/// Emit an `entity-joined-set` event announcing a new entity that matches a creation-query.
/// The frontend adds the new entity to the displayed set and begins watching its updates.
pub fn emit_entity_joined_set(
    app: &AppHandle,
    entity_id: &str,
    class_iri: &str,
    predicate: &str,
    object_value: &str,
    tx: i64,
) {
    if let Some(registry) = app.try_state::<SubscriptionRegistry>() {
        registry.push_ring(ReplayEvent {
            tx,
            event_name: "entity-joined-set".into(),
            entity_id: entity_id.to_owned(),
            changed_predicates: vec![],
            class_iri: Some(class_iri.to_owned()),
            predicate: Some(predicate.to_owned()),
            object_value: Some(object_value.to_owned()),
        });
    }
    app.emit("entity-joined-set", serde_json::json!({
        "entityId": entity_id,
        "classIri": class_iri,
        "predicate": predicate,
        "objectValue": object_value,
        "tx": tx,
    })).ok();
}

/// Frontend declares the full set of entity IRIs (and IRI substring patterns) it is
/// currently displaying, plus creation-queries for discovering new entities.
/// Replaces the previous set; called on every mount/unmount.
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
    registry.replace(iris, patterns, vec![]);
    Ok(())
}

/// Frontend declares the full set including creation-queries for reactive entity discovery.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn events__set_subscriptions_v2(
    iris: Vec<String>,
    patterns: Vec<String>,
    creation_queries: Vec<serde_json::Value>,
    registry: State<'_, SubscriptionRegistry>,
) -> Result<(), String> {
    let queries: Vec<CreationQuery> = creation_queries
        .into_iter()
        .filter_map(|v| {
            let class_iri = v.get("classIri")?.as_str()?.to_owned();
            let predicate = v.get("predicate")?.as_str()?.to_owned();
            let object_value = v.get("objectValue")?.as_str()?.to_owned();
            Some(CreationQuery { class_iri, predicate, object_value })
        })
        .collect();
    crate::diagnostics::log_backend(
        "debug",
        &format!(
            "[REALTIME] subscriptions_v2 set: {} iris, {} patterns, {} creation_queries",
            iris.len(), patterns.len(), queries.len()
        ),
    );
    registry.replace(iris, patterns, queries);
    Ok(())
}

/// Returns events from the ring with tx >= since_tx. Falls back to the triples table
/// when the ring does not cover the cursor (ring_min_tx > since_tx).
#[tauri::command]
#[allow(non_snake_case)]
pub async fn events__replay_since(
    since_tx: i64,
    executor: State<'_, crate::owl::DbExecutor>,
    registry: State<'_, SubscriptionRegistry>,
) -> Result<Vec<serde_json::Value>, String> {
    let ring_min = registry.ring_min_tx();
    if ring_min <= since_tx {
        // Ring covers the cursor — fast path, no DB access.
        let events = registry.replay_since(since_tx);

        // Dedup within the replay window: the subscription gap may have already
        // delivered some of these events before the frontend called replay.
        // Emitting the same (entityId, eventName) pair twice causes the bubble
        // to re-render with stale data and can produce visible duplicates.
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let deduplicated: Vec<serde_json::Value> = events
            .into_iter()
            .filter(|e| seen.insert((e.entity_id.clone(), e.event_name.clone())))
            .map(|e| serde_json::json!({
                "tx": e.tx,
                "event": e.event_name,
                "entityId": e.entity_id,
                "changedPredicates": e.changed_predicates,
                "classIri": e.class_iri,
                "predicate": e.predicate,
                "objectValue": e.object_value,
            }))
            .collect();

        return Ok(deduplicated);
    }

    // Ring does not cover the cursor — fall back to triples table.
    // Uses >= since_tx (inclusive) so that a triple whose TX equals the frontend
    // snapshot is never silently dropped from replay (same rationale as the ring path).
    executor.read(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT t.subject, t.predicate, t.tx
             FROM triples t
             WHERE t.tx >= ?1 AND t.retracted = 0
             ORDER BY t.tx ASC
             LIMIT 500"
        ).map_err(|e| e.to_string())?;

        let rows: Vec<(String, String, i64)> = stmt.query_map(
            rusqlite::params![since_tx],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        // Dedup by entityId on the DB path as well — the same subject can appear in
        // multiple triples within the window; one entity-updated per entity is enough.
        let mut seen: HashSet<String> = HashSet::new();
        let events: Vec<serde_json::Value> = rows
            .into_iter()
            .filter(|(subject, _, _)| seen.insert(subject.clone()))
            .map(|(subject, predicate, tx)| {
                serde_json::json!({
                    "tx": tx,
                    "event": "entity-updated",
                    "entityId": subject,
                    "changedPredicates": [predicate],
                })
            })
            .collect();

        Ok(events)
    }).await
}
