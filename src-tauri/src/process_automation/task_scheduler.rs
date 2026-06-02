use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::task::JoinHandle;

use crate::commands::log_backend;
use crate::owl::{
    DbExecutor, Individual, get_iri_property, get_literal_property, is_instance_of,
    find_entities_with_predicate,
};

pub struct TaskSchedulerState {
    handles: Mutex<Vec<JoinHandle<()>>>,
    reload_lock: tokio::sync::Mutex<()>,
}

impl TaskSchedulerState {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
            reload_lock: tokio::sync::Mutex::new(()),
        }
    }
}

pub fn is_status_descendant_of(conn: &rusqlite::Connection, status_iri: &str, ancestor_iri: &str) -> bool {
    // Walk up the parentStatus chain to check if status_iri is a descendant of ancestor_iri.
    // A status that is its own parent is a root node (stop condition).
    conn.query_row(
        "WITH RECURSIVE ancestors(iri) AS (
            SELECT ?1
            UNION ALL
            SELECT t.object
            FROM triples t
            JOIN ancestors a ON t.subject = a.iri
            WHERE t.predicate = 'foundation:parentStatus'
              AND t.retracted = 0
              AND t.object != t.subject
         )
         SELECT COUNT(*) FROM ancestors WHERE iri = ?2",
        rusqlite::params![status_iri, ancestor_iri],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0
}

fn collect_scheduled_tasks(conn: &rusqlite::Connection) -> Result<Vec<(String, String)>, String> {
    let task_iris = find_entities_with_predicate(conn, "foundation:scheduledAt")
        .map_err(|e| e.to_string())?;

    let mut tasks = Vec::new();
    for task_iri in task_iris {
        if !is_instance_of(conn, &task_iri, "foundation:Task") {
            continue;
        }

        // Skip tasks that have already started or finished.
        if get_literal_property(conn, &task_iri, "foundation:startedAt")
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        if get_literal_property(conn, &task_iri, "foundation:result")
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }

        // Skip tasks in a halted status (Blocked waiting for dependency,
        // or Rejected after a permanent failure).
        let status = get_iri_property(conn, &task_iri, "foundation:hasStatus")
            .map_err(|e| e.to_string())?;
        if let Some(ref s) = status {
            if s == "foundation:Blocked" || s == "foundation:Rejected" {
                continue;
            }
        }

        let scheduled_at = match get_literal_property(conn, &task_iri, "foundation:scheduledAt")
            .map_err(|e| e.to_string())?
        {
            Some(s) => s,
            None => continue,
        };

        let assignee_iri = match get_iri_property(conn, &task_iri, "foundation:assignee")
            .map_err(|e| e.to_string())?
        {
            Some(a) => a,
            None => continue,
        };

        if !is_instance_of(conn, &assignee_iri, "foundation:SoftwareAgent") {
            continue;
        }

        tasks.push((task_iri, scheduled_at));
    }
    Ok(tasks)
}

fn collect_interrupted_tasks(conn: &rusqlite::Connection) -> Result<Vec<String>, String> {
    // Tasks with startedAt but no result were running when the app closed.
    let task_iris = find_entities_with_predicate(conn, "foundation:startedAt")
        .map_err(|e| e.to_string())?;

    let mut interrupted = Vec::new();
    for task_iri in task_iris {
        if !is_instance_of(conn, &task_iri, "foundation:Task") {
            continue;
        }
        let assignee = get_iri_property(conn, &task_iri, "foundation:assignee")
            .map_err(|e| e.to_string())?;
        if !assignee.map(|a| is_instance_of(conn, &a, "foundation:SoftwareAgent")).unwrap_or(false) {
            continue;
        }
        if get_literal_property(conn, &task_iri, "foundation:result")
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        interrupted.push(task_iri);
    }
    Ok(interrupted)
}

fn compute_delay(scheduled_at_iso: &str) -> std::time::Duration {
    match chrono::DateTime::parse_from_rfc3339(scheduled_at_iso) {
        Ok(scheduled_at) => {
            let now = chrono::Utc::now();
            let scheduled_utc = scheduled_at.with_timezone(&chrono::Utc);
            if scheduled_utc > now {
                (scheduled_utc - now).to_std().unwrap_or_default()
            } else {
                std::time::Duration::ZERO
            }
        }
        Err(_) => std::time::Duration::ZERO,
    }
}

pub async fn start(app: AppHandle) {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return,
    };

    // Reset any tasks that were InProgress when the app was last closed.
    // Only runs on initial startup — reload does NOT call this.
    let interrupted = match executor.read(collect_interrupted_tasks).await {
        Ok(v) => v,
        Err(e) => {
            log_backend("error", &format!(
                "[task_scheduler] Failed to collect interrupted tasks: {}", e
            ));
            vec![]
        }
    };
    if !interrupted.is_empty() {
        let reset_result = executor.write(move |conn| {
            for task_iri in &interrupted {
                if let Err(e) = Individual::clear_property(conn, task_iri, "foundation:startedAt", "task_scheduler") {
                    log_backend("error", &format!(
                        "[task_scheduler] Failed to clear startedAt for {}: {}", task_iri, e
                    ));
                    continue;
                }
                if let Err(e) = crate::owl::replace_all_property_iris(conn, task_iri, "foundation:hasStatus", &["foundation:Pending"], "task_scheduler") {
                    log_backend("error", &format!(
                        "[task_scheduler] Failed to reset status for {}: {}", task_iri, e
                    ));
                } else {
                    log_backend("warn", &format!(
                        "[task_scheduler] Recovered interrupted task: {}", task_iri
                    ));
                }
            }
            Ok(String::new())
        }).await;
        if let Err(e) = reset_result {
            log_backend("error", &format!(
                "[task_scheduler] Failed to reset interrupted tasks: {}", e
            ));
        }
    }

    schedule(app).await;
}

async fn schedule(app: AppHandle) {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return,
    };

    let tasks = match executor.read(collect_scheduled_tasks).await {
        Ok(v) => v,
        Err(e) => {
            log_backend("error", &format!(
                "[task_scheduler] Failed to load scheduled tasks: {}", e
            ));
            return;
        }
    };

    let state = match app.try_state::<TaskSchedulerState>() {
        Some(s) => s,
        None => return,
    };

    let mut handles = match state.handles.lock() {
        Ok(h) => h,
        Err(e) => e.into_inner(),
    };

    for (task_iri, scheduled_at_iso) in tasks {
        let delay = compute_delay(&scheduled_at_iso);
        let app_clone = app.clone();
        // Only the timer (sleep) is tracked in handles so reload can cancel pending tasks.
        // The actual execution is spawned independently and never aborted by reload.
        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            super::task_manager::maybe_execute_task_for_entity(app_clone, task_iri);
        });
        handles.push(handle);
    }
}

pub fn listen_for_scheduled_tasks(app: AppHandle) {
    use tauri::Listener;

    let app_for_created = app.clone();
    app.listen("entity-created", move |event| {
        if let Some(entity_id) = parse_entity_id(event.payload()) {
            let app2 = app_for_created.clone();
            tauri::async_runtime::spawn(async move {
                if is_task_entity(&app2, &entity_id).await {
                    reload(app2).await;
                }
            });
        }
    });

    let app_for_updated = app.clone();
    app.listen("entity-updated", move |event| {
        if let Some(entity_id) = parse_entity_id(event.payload()) {
            let app2 = app_for_updated.clone();
            tauri::async_runtime::spawn(async move {
                if is_task_entity(&app2, &entity_id).await {
                    reload(app2).await;
                }
            });
        }
    });
}

fn parse_entity_id(payload: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v["entityId"].as_str().map(|s| s.to_string()))
}

async fn is_task_entity(app: &AppHandle, entity_id: &str) -> bool {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return false,
    };
    let entity_id = entity_id.to_string();
    executor
        .read(move |conn| Ok(is_instance_of(conn, &entity_id, "foundation:Task")))
        .await
        .unwrap_or(false)
}

pub async fn reload(app: AppHandle) {
    let state = match app.try_state::<TaskSchedulerState>() {
        Some(s) => s,
        None => return,
    };

    let _guard = state.reload_lock.lock().await;

    {
        let mut handles = match state.handles.lock() {
            Ok(h) => h,
            Err(e) => e.into_inner(),
        };
        for handle in handles.drain(..) {
            handle.abort();
        }
    }

    schedule(app.clone()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    fn insert(conn: &mut rusqlite::Connection, triples: &[Triple]) {
        store::assert_triples(conn, triples, "test").expect("Failed to insert triples");
    }

    fn agent_task_triples(task_iri: &str, agent_iri: &str, scheduled_at: Option<&str>) -> Vec<Triple> {
        let mut t = vec![
            Triple::new(task_iri, "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new(task_iri, "foundation:assignee", Object::Iri(agent_iri.to_string())),
            Triple::new(agent_iri, "rdf:type", Object::Iri("foundation:SoftwareAgent".to_string())),
        ];
        if let Some(dt) = scheduled_at {
            t.push(Triple::new(task_iri, "foundation:scheduledAt", Object::Literal {
                value: dt.to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }));
        }
        t
    }

    #[test]
    fn test_collect_scheduled_tasks_returns_agent_tasks() {
        let mut conn = setup_test_db();
        insert(&mut conn, &agent_task_triples(
            "foundation:Task_sched1", "foundation:Agent_sched1",
            Some("2099-01-01T10:00:00Z"),
        ));

        let tasks = collect_scheduled_tasks(&conn).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].0, "foundation:Task_sched1");
        assert!(tasks[0].1.starts_with("2099-01-01T10:00:00"));
    }

    #[test]
    fn test_collect_scheduled_tasks_ignores_tasks_without_scheduled_at() {
        let mut conn = setup_test_db();
        insert(&mut conn, &agent_task_triples(
            "foundation:Task_sched2", "foundation:Agent_sched2",
            None,
        ));

        let tasks = collect_scheduled_tasks(&conn).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_collect_scheduled_tasks_ignores_tasks_with_result() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Task_sched_done", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_sched_done", "foundation:assignee", Object::Iri("foundation:Agent_sched_done".to_string())),
            Triple::new("foundation:Task_sched_done", "foundation:result", Object::Literal {
                value: "concluído".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Task_sched_done", "foundation:scheduledAt", Object::Literal {
                value: "2099-01-01T10:00:00Z".to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }),
            Triple::new("foundation:Agent_sched_done", "rdf:type", Object::Iri("foundation:SoftwareAgent".to_string())),
        ]);

        let tasks = collect_scheduled_tasks(&conn).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_collect_scheduled_tasks_ignores_tasks_with_started_at() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Task_sched_running", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_sched_running", "foundation:assignee", Object::Iri("foundation:Agent_sched_run".to_string())),
            Triple::new("foundation:Task_sched_running", "foundation:startedAt", Object::Literal {
                value: "2025-01-01T10:00:00Z".to_string(), datatype: Some("xsd:dateTime".to_string()), language: None,
            }),
            Triple::new("foundation:Task_sched_running", "foundation:scheduledAt", Object::Literal {
                value: "2099-01-01T10:00:00Z".to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }),
            Triple::new("foundation:Agent_sched_run", "rdf:type", Object::Iri("foundation:SoftwareAgent".to_string())),
        ]);

        let tasks = collect_scheduled_tasks(&conn).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_collect_scheduled_tasks_ignores_blocked_tasks() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Task_sched_blocked", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_sched_blocked", "foundation:hasStatus", Object::Iri("foundation:Blocked".to_string())),
            Triple::new("foundation:Task_sched_blocked", "foundation:assignee", Object::Iri("foundation:Agent_sched_bl".to_string())),
            Triple::new("foundation:Task_sched_blocked", "foundation:scheduledAt", Object::Literal {
                value: "2099-01-01T10:00:00Z".to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }),
            Triple::new("foundation:Agent_sched_bl", "rdf:type", Object::Iri("foundation:SoftwareAgent".to_string())),
        ]);

        let tasks = collect_scheduled_tasks(&conn).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_collect_scheduled_tasks_ignores_human_assignees() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Task_sched3", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_sched3", "foundation:assignee", Object::Iri("foundation:Person_sched3".to_string())),
            Triple::new("foundation:Task_sched3", "foundation:scheduledAt", Object::Literal {
                value: "2099-01-01T10:00:00Z".to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }),
            Triple::new("foundation:Person_sched3", "rdf:type", Object::Iri("foundation:Person".to_string())),
        ]);

        let tasks = collect_scheduled_tasks(&conn).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_collect_scheduled_tasks_ignores_non_tasks() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Other_sched4", "rdf:type", Object::Iri("foundation:SoftwareFeature".to_string())),
            Triple::new("foundation:Other_sched4", "foundation:scheduledAt", Object::Literal {
                value: "2099-01-01T10:00:00Z".to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }),
        ]);

        let tasks = collect_scheduled_tasks(&conn).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_collect_interrupted_tasks_returns_tasks_with_started_at_without_result() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Task_interrupted", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_interrupted", "foundation:assignee", Object::Iri("foundation:Agent_interrupted".to_string())),
            Triple::new("foundation:Task_interrupted", "foundation:startedAt", Object::Literal {
                value: "2025-01-01T10:00:00Z".to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }),
            Triple::new("foundation:Agent_interrupted", "rdf:type", Object::Iri("foundation:SoftwareAgent".to_string())),
        ]);

        let interrupted = collect_interrupted_tasks(&conn).unwrap();
        assert_eq!(interrupted.len(), 1);
        assert_eq!(interrupted[0], "foundation:Task_interrupted");
    }

    #[test]
    fn test_collect_interrupted_tasks_ignores_tasks_without_started_at() {
        let mut conn = setup_test_db();
        insert(&mut conn, &agent_task_triples(
            "foundation:Task_pending", "foundation:Agent_pending",
            Some("2099-01-01T10:00:00Z"),
        ));

        let interrupted = collect_interrupted_tasks(&conn).unwrap();
        assert!(interrupted.is_empty());
    }

    #[test]
    fn test_collect_interrupted_tasks_ignores_completed_tasks() {
        let mut conn = setup_test_db();
        insert(&mut conn, &[
            Triple::new("foundation:Task_done", "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:Task_done", "foundation:assignee", Object::Iri("foundation:Agent_done".to_string())),
            Triple::new("foundation:Task_done", "foundation:startedAt", Object::Literal {
                value: "2025-01-01T10:00:00Z".to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            }),
            Triple::new("foundation:Task_done", "foundation:result", Object::Literal {
                value: "resultado".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Agent_done", "rdf:type", Object::Iri("foundation:SoftwareAgent".to_string())),
        ]);

        let interrupted = collect_interrupted_tasks(&conn).unwrap();
        assert!(interrupted.is_empty());
    }

    #[test]
    fn test_compute_delay_future_datetime_returns_positive() {
        let future = "2099-01-01T10:00:00Z";
        let delay = compute_delay(future);
        assert!(delay > std::time::Duration::ZERO);
    }

    #[test]
    fn test_compute_delay_past_datetime_returns_zero() {
        let past = "2000-01-01T10:00:00Z";
        let delay = compute_delay(past);
        assert_eq!(delay, std::time::Duration::ZERO);
    }

    #[test]
    fn test_compute_delay_invalid_string_returns_zero() {
        let delay = compute_delay("not-a-datetime");
        assert_eq!(delay, std::time::Duration::ZERO);
    }
}
