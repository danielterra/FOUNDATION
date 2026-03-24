use cron::Schedule;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::task::JoinHandle;

use crate::commands::log_backend;
use crate::owl::{DbExecutor, Individual};

pub struct SchedulerState {
    pub handles: Mutex<Vec<JoinHandle<()>>>,
    reload_lock: tokio::sync::Mutex<()>,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
            reload_lock: tokio::sync::Mutex::new(()),
        }
    }
}

const STATUS_PAUSED: &str = "foundation:Status_1773016842120";

fn collect_timer_definitions(
    conn: &rusqlite::Connection,
) -> Result<Vec<(String, String)>, String> {
    let start_event_iris =
        crate::owl::find_entities_with_property(conn, "rdf:type", "foundation:automation_TimerStartEvent")
            .map_err(|e| e.to_string())?;

    let mut timers: Vec<(String, String)> = Vec::new();
    for start_event_iri in &start_event_iris {
        let start_event = match Individual::get(conn, start_event_iri.as_str()).map_err(|e| e.to_string())? {
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

        let timer_def_iri = match start_event.properties.iter()
            .find(|(p, _)| p == "foundation:eventDefinition")
            .and_then(|(_, v)| v.as_iri())
        {
            Some(iri) => iri.to_string(),
            None => continue,
        };

        let timer_def = match Individual::get(conn, timer_def_iri.as_str()).map_err(|e| e.to_string())? {
            Some(i) => i,
            None => continue,
        };

        let cron_expr = match timer_def.properties.iter()
            .find(|(p, _)| p == "foundation:timeCycle")
            .and_then(|(_, v)| v.as_literal())
        {
            Some(e) => e.to_string(),
            None => continue,
        };

        let is_paused = timer_def.properties.iter()
            .find(|(p, _)| p == "foundation:hasStatus")
            .and_then(|(_, v)| v.as_iri())
            .map(|iri| iri == STATUS_PAUSED)
            .unwrap_or(false);
        if is_paused {
            continue;
        }

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

    let timer_defs = executor.read(collect_timer_definitions).await;

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
                if let Err(e) = super::executor::run_process(&app_clone, &process_iri, None).await {
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
        .read(move |conn| {
            let has_timer_type = |ind: &Individual| {
                ind.properties.iter().any(|(p, v)| {
                    p == "rdf:type"
                        && v.as_iri()
                            .map(|iri| {
                                iri == "foundation:automation_TimerEventDefinition"
                                    || iri == "foundation:automation_TimerStartEvent"
                            })
                            .unwrap_or(false)
                })
            };

            if let Ok(Some(ind)) = Individual::get(conn, &entity_id) {
                if has_timer_type(&ind) {
                    return Ok(true);
                }
            }

            if include_retracted {
                if let Ok(Some(ind)) = Individual::get_from_retracted(conn, &entity_id) {
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
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    fn insert_triples(conn: &mut rusqlite::Connection, triples: &[Triple]) {
        store::assert_triples(conn, triples, "test").expect("Failed to insert triples");
    }

    fn timer_start_event_triples(start_iri: &str, timer_iri: &str, process_iri: &str, cron: Option<&str>, paused: bool) -> Vec<Triple> {
        let mut t = vec![
            Triple::new(start_iri, "rdf:type", Object::Iri("foundation:automation_TimerStartEvent".to_string())),
            Triple::new(start_iri, "foundation:eventDefinition", Object::Iri(timer_iri.to_string())),
            Triple::new(start_iri, "foundation:partOfProcess", Object::Iri(process_iri.to_string())),
            Triple::new(timer_iri, "rdf:type", Object::Iri("foundation:automation_TimerEventDefinition".to_string())),
        ];
        if let Some(c) = cron {
            t.push(Triple::new(timer_iri, "foundation:timeCycle", Object::Literal {
                value: c.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }));
        }
        if paused {
            t.push(Triple::new(timer_iri, "foundation:hasStatus", Object::Iri(super::STATUS_PAUSED.to_string())));
        }
        t
    }

    #[test]
    fn test_collect_timer_definitions_empty_db_returns_empty() {
        let conn = setup_test_db();
        let timers = collect_timer_definitions(&conn).unwrap();
        assert!(timers.is_empty());
    }

    #[test]
    fn test_collect_timer_definitions_returns_process_iri() {
        let mut conn = setup_test_db();
        insert_triples(&mut conn, &timer_start_event_triples(
            "foundation:Start1", "foundation:Timer1", "foundation:Process1",
            Some("0 * * * * *"), false,
        ));

        let timers = collect_timer_definitions(&conn).unwrap();
        assert_eq!(timers.len(), 1);
        let (process_iri, cron_expr) = &timers[0];
        assert_eq!(process_iri, "foundation:Process1");
        assert_eq!(cron_expr, "0 * * * * *");
    }

    #[test]
    fn test_collect_timer_definitions_skips_timer_without_time_cycle() {
        let mut conn = setup_test_db();
        insert_triples(&mut conn, &timer_start_event_triples(
            "foundation:Start2", "foundation:Timer2", "foundation:Process2",
            None, false,
        ));

        let timers = collect_timer_definitions(&conn).unwrap();
        assert!(timers.is_empty());
    }

    #[test]
    fn test_collect_timer_definitions_skips_start_event_without_process_link() {
        let mut conn = setup_test_db();
        insert_triples(&mut conn, &[
            Triple::new("foundation:Start3", "rdf:type", Object::Iri("foundation:automation_TimerStartEvent".to_string())),
            Triple::new("foundation:Start3", "foundation:eventDefinition", Object::Iri("foundation:Timer3".to_string())),
            // No partOfProcess on Start3
            Triple::new("foundation:Timer3", "rdf:type", Object::Iri("foundation:automation_TimerEventDefinition".to_string())),
            Triple::new("foundation:Timer3", "foundation:timeCycle", Object::Literal {
                value: "0 * * * * *".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ]);

        let timers = collect_timer_definitions(&conn).unwrap();
        assert!(timers.is_empty());
    }

    #[test]
    fn test_collect_timer_definitions_skips_paused_timer() {
        let mut conn = setup_test_db();
        insert_triples(&mut conn, &timer_start_event_triples(
            "foundation:StartP", "foundation:TimerP", "foundation:ProcessP",
            Some("0 * * * * *"), true,
        ));

        let timers = collect_timer_definitions(&conn).unwrap();
        assert!(timers.is_empty());
    }

    #[test]
    fn test_collect_timer_definitions_includes_active_timer() {
        let mut conn = setup_test_db();
        let mut triples = timer_start_event_triples(
            "foundation:StartAct", "foundation:TimerAct", "foundation:ProcessAct",
            Some("0 * * * * *"), false,
        );
        triples.push(Triple::new("foundation:TimerAct", "foundation:hasStatus",
            Object::Iri("foundation:Status_1772755611667".to_string())));
        insert_triples(&mut conn, &triples);

        let timers = collect_timer_definitions(&conn).unwrap();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].0, "foundation:ProcessAct");
    }

    #[test]
    fn test_collect_timer_definitions_multiple_timers() {
        let mut conn = setup_test_db();
        let mut triples = timer_start_event_triples(
            "foundation:StartA", "foundation:TimerA", "foundation:ProcessA",
            Some("0 * * * * *"), false,
        );
        triples.extend(timer_start_event_triples(
            "foundation:StartB", "foundation:TimerB", "foundation:ProcessB",
            Some("0 0 * * * *"), false,
        ));
        insert_triples(&mut conn, &triples);

        let timers = collect_timer_definitions(&conn).unwrap();
        assert_eq!(timers.len(), 2);

        let process_iris: Vec<&str> = timers.iter().map(|(p, _)| p.as_str()).collect();
        assert!(process_iris.contains(&"foundation:ProcessA"));
        assert!(process_iris.contains(&"foundation:ProcessB"));
    }
}

/// Aborts all running schedule tasks and restarts them from current DB state.
/// Uses reload_lock to serialize concurrent reload calls, preventing duplicate timer tasks.
pub async fn reload(app: AppHandle) {
    let state = match app.try_state::<SchedulerState>() {
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

    start(app.clone()).await;
}
