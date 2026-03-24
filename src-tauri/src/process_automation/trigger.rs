use tauri::{AppHandle, Listener, Manager};

use crate::commands::log_backend;
use crate::eavto::query;
use crate::owl::DbExecutor;

/// Registers listeners for internal FOUNDATION events that can trigger reactive processes.
/// For each event key, queries MessageEventDefinitions whose eventType matches, then runs the
/// linked process via the executor.
pub fn register_listeners(app: AppHandle) {
    register_event(&app, "entity-created");
    register_event(&app, "entity-updated");
}

fn register_event(app: &AppHandle, event_key: &'static str) {
    let app_clone = app.clone();
    app.listen(event_key, move |_event| {
        let app2 = app_clone.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = fire_processes_for_event(&app2, event_key).await {
                log_backend("error", &format!("[trigger] Error handling '{}': {}", event_key, e));
            }
        });
    });
}

async fn fire_processes_for_event(app: &AppHandle, event_key: &str) -> Result<(), String> {
    let executor = app.state::<DbExecutor>();
    let event_key = event_key.to_string();

    let process_iris = executor
        .read(move |conn| {
            find_processes_for_event_key(conn, &event_key)
        })
        .await?;

    for process_iri in process_iris {
        if let Err(e) = super::executor::run_process(app, &process_iri, None).await {
            log_backend("error", &format!("[trigger] Error running process {}: {}", process_iri, e));
        }
    }

    Ok(())
}

/// Queries all Automation process IRIs whose StartEvent has a MessageEventDefinition
/// with eventType pointing to an InternalEventType whose eventKey matches.
fn find_processes_for_event_key(
    conn: &rusqlite::Connection,
    event_key: &str,
) -> Result<Vec<String>, String> {
    // Find InternalEventType IRIs with matching eventKey
    let event_type_iris: Vec<String> = {
        let result = query::get_by_predicate(conn, "foundation:eventKey")
            .map_err(|e| e.to_string())?;
        result
            .triples
            .iter()
            .filter(|t| {
                t.object
                    .as_literal()
                    .map(|v| v == event_key)
                    .unwrap_or(false)
            })
            .map(|t| t.subject.clone())
            .collect()
    };

    if event_type_iris.is_empty() {
        return Ok(Vec::new());
    }

    // Find MessageEventDefinitions whose eventType matches one of those IRIs
    let msg_event_def_iris: Vec<String> = {
        let result = query::get_by_predicate(conn, "foundation:eventType")
            .map_err(|e| e.to_string())?;
        result
            .triples
            .iter()
            .filter(|t| {
                t.object
                    .as_iri()
                    .map(|iri| event_type_iris.iter().any(|e| e == iri))
                    .unwrap_or(false)
            })
            .map(|t| t.subject.clone())
            .collect()
    };

    if msg_event_def_iris.is_empty() {
        return Ok(Vec::new());
    }

    // Find StartEvents linked via messageEventOf
    let start_event_iris: Vec<String> = {
        let result = query::get_by_predicate(conn, "foundation:messageEventOf")
            .map_err(|e| e.to_string())?;
        result
            .triples
            .iter()
            .filter(|t| msg_event_def_iris.contains(&t.subject))
            .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
            .collect()
    };

    if start_event_iris.is_empty() {
        return Ok(Vec::new());
    }

    // Find processes via partOfProcess on the StartEvents
    let mut process_iris: Vec<String> = Vec::new();
    for start_iri in &start_event_iris {
        let result = query::get_by_entity_predicate(conn, start_iri, "foundation:partOfProcess")
            .map_err(|e| e.to_string())?;
        if let Some(process_iri) = result.triples.first().and_then(|t| t.object.as_iri()) {
            process_iris.push(process_iri.to_string());
        }
    }

    Ok(process_iris)
}

#[cfg(test)]
#[path = "trigger_tests.rs"]
mod trigger_tests;
