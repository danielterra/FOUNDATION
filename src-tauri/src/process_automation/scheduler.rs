use cron::Schedule;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::task::JoinHandle;

use crate::commands::log_backend;
use crate::owl::{DbExecutor, Individual};

pub struct SchedulerState {
    pub handles: Mutex<Vec<JoinHandle<()>>>,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
        }
    }
}

const STATUS_PAUSED: &str = "foundation:Status_1773016842120";

async fn collect_timer_definitions(
    conn: &crate::owl::Connection,
) -> Result<Vec<(String, String)>, String> {
    let timer_iris =
        crate::owl::find_entities_with_property(conn, "rdf:type", "foundation:bpmn_TimerEventDefinition").await
            .map_err(|e| e.to_string())?;

    let mut timers: Vec<(String, String)> = Vec::new();
    for timer_iri in &timer_iris {
        let ind = match Individual::get(conn, timer_iri.as_str()).await.map_err(|e| e.to_string())? {
            Some(i) => i,
            None => continue,
        };

        let cron_expr = match ind.properties.iter()
            .find(|(p, _)| p == "foundation:timeCycle")
            .and_then(|(_, v)| v.as_literal())
        {
            Some(e) => e.to_string(),
            None => continue,
        };

        let is_paused = ind.properties.iter()
            .find(|(p, _)| p == "foundation:hasStatus")
            .and_then(|(_, v)| v.as_iri())
            .map(|iri| iri == STATUS_PAUSED)
            .unwrap_or(false);
        if is_paused {
            continue;
        }

        let start_event_iri = match ind.properties.iter()
            .find(|(p, _)| p == "foundation:timerEventOf")
            .and_then(|(_, v)| v.as_iri())
        {
            Some(iri) => iri.to_string(),
            None => continue,
        };

        let start_event = match Individual::get(conn, start_event_iri.as_str()).await.map_err(|e| e.to_string())? {
            Some(i) => i,
            None => continue,
        };

        let process_iri = match start_event.properties.iter()
            .find(|(p, _)| p == "foundation:partOfProcess")
            .and_then(|(_, v)| v.as_iri())
        {
            Some(iri) => iri.to_string(),
            None => continue,
        };

        timers.push((process_iri, cron_expr));
    }
    Ok(timers)
}

/// Collects all active TimerEventDefinitions with a timeCycle, spawns one tokio task per schedule.
pub async fn start(app: AppHandle) {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return,
    };

    let timer_defs = executor.read(|conn| async move {
        collect_timer_definitions(&conn).await
    }).await;

    let timer_defs = match timer_defs {
        Ok(v) => v,
        Err(e) => {
            log_backend("error", &format!("[scheduler] Failed to load timer definitions: {}", e));
            return;
        }
    };

    let state = match app.try_state::<SchedulerState>() {
        Some(s) => s,
        None => return,
    };

    let mut handles = match state.handles.lock() {
        Ok(h) => h,
        Err(e) => e.into_inner(),
    };
    for (process_iri, cron_expr) in timer_defs {
        let schedule = match Schedule::from_str(&cron_expr) {
            Ok(s) => s,
            Err(e) => {
                log_backend("warn", &format!("[scheduler] Invalid cron '{}' for {}: {}", cron_expr, process_iri, e));
                continue;
            }
        };

        let app_clone = app.clone();
        let handle = tokio::spawn(async move {
            for next in schedule.upcoming(chrono::Utc) {
                let now = chrono::Utc::now();
                let delay = (next - now).to_std().unwrap_or_default();
                tokio::time::sleep(delay).await;
                if let Err(e) = super::executor::run_process(&app_clone, &process_iri).await {
                    log_backend("error", &format!("[scheduler] Error running process {}: {}", process_iri, e));
                }
            }
        });
        handles.push(handle);
    }
}

/// Registers event listeners that reload the scheduler when TimerEventDefinitions are created,
/// modified, or deleted.
pub fn listen_for_new_timers(app: AppHandle) {
    use tauri::Listener;

    let app_for_created = app.clone();
    app.listen("entity-created", move |event| {
        if let Some(entity_id) = parse_entity_id(event.payload()) {
            let app2 = app_for_created.clone();
            tauri::async_runtime::spawn(async move {
                if is_timer_event_definition(&app2, &entity_id, false).await {
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
                if is_timer_event_definition(&app2, &entity_id, true).await {
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

async fn is_timer_event_definition(app: &AppHandle, entity_id: &str, include_retracted: bool) -> bool {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return false,
    };
    let entity_id = entity_id.to_string();
    executor
        .read(move |conn| async move {
            let has_timer_type = |ind: &Individual| {
                ind.properties.iter().any(|(p, v)| {
                    p == "rdf:type"
                        && v.as_iri()
                            .map(|iri| iri == "foundation:bpmn_TimerEventDefinition")
                            .unwrap_or(false)
                })
            };

            if let Ok(Some(ind)) = Individual::get(&conn, &entity_id).await {
                if has_timer_type(&ind) {
                    return Ok(true);
                }
            }

            if include_retracted {
                if let Ok(Some(ind)) = Individual::get_from_retracted(&conn, &entity_id).await {
                    return Ok(has_timer_type(&ind));
                }
            }

            Ok(false)
        })
        .await
        .unwrap_or(false)
}

#[cfg(test)]
mod scheduler_tests {
    use super::collect_timer_definitions;
    use super::STATUS_PAUSED;
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    async fn insert_triples(conn: &crate::owl::Connection, triples: &[Triple]) {
        store::assert_triples(conn, triples, "test").await.expect("Failed to insert triples");
    }

    #[tokio::test]
    async fn test_collect_timer_definitions_empty_db_returns_empty() {
        let conn = setup_test_db().await;
        let timers = collect_timer_definitions(&conn).await.unwrap();
        assert!(timers.is_empty());
    }

    #[tokio::test]
    async fn test_collect_timer_definitions_returns_process_iri_not_start_event() {
        let conn = setup_test_db().await;
        insert_triples(&conn, &[
            Triple::new("foundation:Timer1", "rdf:type", Object::Iri("foundation:bpmn_TimerEventDefinition".to_string())),
            Triple::new("foundation:Timer1", "foundation:timeCycle", Object::Literal {
                value: "0 * * * * *".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:Timer1", "foundation:timerEventOf", Object::Iri("foundation:Start1".to_string())),
            Triple::new("foundation:Start1", "foundation:partOfProcess", Object::Iri("foundation:Process1".to_string())),
        ]).await;

        let timers = collect_timer_definitions(&conn).await.unwrap();
        assert_eq!(timers.len(), 1);
        let (process_iri, cron_expr) = &timers[0];
        assert_eq!(process_iri, "foundation:Process1");
        assert_eq!(cron_expr, "0 * * * * *");
    }

    #[tokio::test]
    async fn test_collect_timer_definitions_skips_timer_without_time_cycle() {
        let conn = setup_test_db().await;
        insert_triples(&conn, &[
            Triple::new("foundation:Timer2", "rdf:type", Object::Iri("foundation:bpmn_TimerEventDefinition".to_string())),
            Triple::new("foundation:Timer2", "foundation:timerEventOf", Object::Iri("foundation:Start2".to_string())),
            Triple::new("foundation:Start2", "foundation:partOfProcess", Object::Iri("foundation:Process2".to_string())),
        ]).await;

        let timers = collect_timer_definitions(&conn).await.unwrap();
        assert!(timers.is_empty());
    }

    #[tokio::test]
    async fn test_collect_timer_definitions_skips_timer_without_process_link() {
        let conn = setup_test_db().await;
        insert_triples(&conn, &[
            Triple::new("foundation:Timer3", "rdf:type", Object::Iri("foundation:bpmn_TimerEventDefinition".to_string())),
            Triple::new("foundation:Timer3", "foundation:timeCycle", Object::Literal {
                value: "0 * * * * *".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:Timer3", "foundation:timerEventOf", Object::Iri("foundation:Start3".to_string())),
        ]).await;

        let timers = collect_timer_definitions(&conn).await.unwrap();
        assert!(timers.is_empty());
    }

    #[tokio::test]
    async fn test_collect_timer_definitions_skips_paused_timer() {
        let conn = setup_test_db().await;
        insert_triples(&conn, &[
            Triple::new("foundation:TimerP", "rdf:type", Object::Iri("foundation:bpmn_TimerEventDefinition".to_string())),
            Triple::new("foundation:TimerP", "foundation:timeCycle", Object::Literal {
                value: "0 * * * * *".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:TimerP", "foundation:hasStatus", Object::Iri(STATUS_PAUSED.to_string())),
            Triple::new("foundation:TimerP", "foundation:timerEventOf", Object::Iri("foundation:StartP".to_string())),
            Triple::new("foundation:StartP", "foundation:partOfProcess", Object::Iri("foundation:ProcessP".to_string())),
        ]).await;

        let timers = collect_timer_definitions(&conn).await.unwrap();
        assert!(timers.is_empty());
    }

    #[tokio::test]
    async fn test_collect_timer_definitions_includes_active_timer() {
        let conn = setup_test_db().await;
        insert_triples(&conn, &[
            Triple::new("foundation:TimerAct", "rdf:type", Object::Iri("foundation:bpmn_TimerEventDefinition".to_string())),
            Triple::new("foundation:TimerAct", "foundation:timeCycle", Object::Literal {
                value: "0 * * * * *".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:TimerAct", "foundation:hasStatus", Object::Iri("foundation:Status_1772755611667".to_string())),
            Triple::new("foundation:TimerAct", "foundation:timerEventOf", Object::Iri("foundation:StartAct".to_string())),
            Triple::new("foundation:StartAct", "foundation:partOfProcess", Object::Iri("foundation:ProcessAct".to_string())),
        ]).await;

        let timers = collect_timer_definitions(&conn).await.unwrap();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].0, "foundation:ProcessAct");
    }

    #[tokio::test]
    async fn test_collect_timer_definitions_multiple_timers() {
        let conn = setup_test_db().await;
        insert_triples(&conn, &[
            Triple::new("foundation:TimerA", "rdf:type", Object::Iri("foundation:bpmn_TimerEventDefinition".to_string())),
            Triple::new("foundation:TimerA", "foundation:timeCycle", Object::Literal {
                value: "0 * * * * *".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:TimerA", "foundation:timerEventOf", Object::Iri("foundation:StartA".to_string())),
            Triple::new("foundation:StartA", "foundation:partOfProcess", Object::Iri("foundation:ProcessA".to_string())),

            Triple::new("foundation:TimerB", "rdf:type", Object::Iri("foundation:bpmn_TimerEventDefinition".to_string())),
            Triple::new("foundation:TimerB", "foundation:timeCycle", Object::Literal {
                value: "0 0 * * * *".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:TimerB", "foundation:timerEventOf", Object::Iri("foundation:StartB".to_string())),
            Triple::new("foundation:StartB", "foundation:partOfProcess", Object::Iri("foundation:ProcessB".to_string())),
        ]).await;

        let timers = collect_timer_definitions(&conn).await.unwrap();
        assert_eq!(timers.len(), 2);

        let process_iris: Vec<&str> = timers.iter().map(|(p, _)| p.as_str()).collect();
        assert!(process_iris.contains(&"foundation:ProcessA"));
        assert!(process_iris.contains(&"foundation:ProcessB"));
    }
}

/// Aborts all running schedule tasks and restarts them from current DB state.
pub async fn reload(app: AppHandle) {
    let state = match app.try_state::<SchedulerState>() {
        Some(s) => s,
        None => return,
    };

    {
        let mut handles = match state.handles.lock() {
            Ok(h) => h,
            Err(e) => e.into_inner(),
        };
        for handle in handles.drain(..) {
            handle.abort();
        }
    }

    start(app).await;
}
