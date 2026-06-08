use cron::Schedule;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::task::JoinHandle;

use crate::commands::log_backend;
use crate::eavto::{Object, Triple};
use crate::owl::{DbExecutor, Individual};

const DEBOUNCE_MS: u64 = 150;

pub struct SchedulerState {
    pub handles: Mutex<Vec<JoinHandle<()>>>,
    reload_lock: tokio::sync::Mutex<()>,
    debounce_handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
            reload_lock: tokio::sync::Mutex::new(()),
            debounce_handle: tokio::sync::Mutex::new(None),
        }
    }
}

const STATUS_PAUSED: &str = "foundation:Status_1773016842120";

struct TimerDef {
    process_iri: String,
    cron_expr: String,
    timer_def_iri: String,
    start_event_iri: String,
    last_run_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn collect_timer_definitions(
    conn: &rusqlite::Connection,
) -> Result<Vec<TimerDef>, String> {
    let start_event_iris =
        crate::owl::find_entities_with_property(conn, "rdf:type", "foundation:automation_TimerStartEvent")
            .map_err(|e| e.to_string())?;

    let mut timers: Vec<TimerDef> = Vec::new();
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

        let last_run_at = timer_def.properties.iter()
            .filter(|(p, _)| p == "foundation:lastRunAt")
            .filter_map(|(_, v)| {
                let s = v.as_literal()?;
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            })
            .max();

        timers.push(TimerDef { process_iri, cron_expr, timer_def_iri, start_event_iri: start_event_iri.to_string(), last_run_at });
    }
    Ok(timers)
}

async fn record_last_run(app: &AppHandle, timer_def_iri: &str) {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return,
    };
    let timer_def_iri = timer_def_iri.to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    if let Err(e) = executor.write(move |conn| {
        let triple = Triple::new(
            &timer_def_iri,
            "foundation:lastRunAt",
            Object::Literal {
                value: now_str,
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            },
        );
        crate::eavto::store::assert_triples(conn, &[triple], "scheduler")
            .map_err(|e| e.to_string())?;
        Ok(String::new())
    }).await {
        log_backend("warn", &format!("[scheduler] Failed to record lastRunAt: {}", e));
    }
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
    for TimerDef { process_iri, cron_expr, timer_def_iri, start_event_iri, last_run_at } in timer_defs {
        let schedule = match Schedule::from_str(&cron_expr) {
            Ok(s) => s,
            Err(e) => {
                log_backend("warn", &format!("[scheduler] Invalid cron '{}' for {}: {}", cron_expr, process_iri, e));
                continue;
            }
        };

        let app_clone = app.clone();
        let handle = tokio::spawn(async move {
            // Catch-up: if the app was offline during a scheduled time, execute once.
            // Only triggers when a previous run was recorded (last_run_at is set) and
            // the next scheduled time after it already passed (with 60s grace margin).
            if let Some(last) = last_run_at {
                let catch_up_threshold = chrono::Utc::now() - chrono::Duration::seconds(60);
                if let Some(missed) = schedule.after(&last).next() {
                    if missed < catch_up_threshold {
                        log_backend("info", &format!(
                            "[scheduler] Catch-up: running missed execution of {} (was due at {})",
                            process_iri, missed
                        ));
                        if let Err(e) = super::executor::run_process_from_timer(&app_clone, &process_iri, &start_event_iri).await {
                            log_backend("error", &format!("[scheduler] Catch-up error for {}: {}", process_iri, e));
                        }
                        record_last_run(&app_clone, &timer_def_iri).await;
                    }
                }
            }

            for next in schedule.upcoming(chrono::Utc) {
                let now = chrono::Utc::now();
                let delay = (next - now).to_std().unwrap_or_default();
                tokio::time::sleep(delay).await;
                if let Err(e) = super::executor::run_process_from_timer(&app_clone, &process_iri, &start_event_iri).await {
                    log_backend("error", &format!("[scheduler] Error running process {}: {}", process_iri, e));
                }
                record_last_run(&app_clone, &timer_def_iri).await;
            }
        });
        handles.push(handle);
    }
}

/// Registers an event listener that reloads the scheduler when TimerEventDefinitions or
/// TimerStartEvents are created, modified, or deleted via any path (MCP, UI, automation).
///
/// Uses entity-changed-internal (non-gated) so writes that bypass the UI subscription
/// registry — e.g. MCP tool calls — are correctly observed.
///
/// A 150 ms debounce coalesces the burst of triples emitted during a single MCP creation
/// into one reload instead of N, without introducing external dependencies.
pub fn listen_for_new_timers(app: AppHandle) {
    use tauri::Listener;

    app.clone().listen("entity-changed-internal", move |event| {
        if let Some(entity_id) = parse_entity_id(event.payload()) {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                if !is_timer_event_definition(&app2, &entity_id, true).await {
                    return;
                }

                let state = match app2.try_state::<SchedulerState>() {
                    Some(s) => s,
                    None => return,
                };

                let mut pending = state.debounce_handle.lock().await;
                if let Some(prev) = pending.take() {
                    prev.abort();
                }

                let app3 = app2.clone();
                *pending = Some(tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(DEBOUNCE_MS)).await;
                    reload(app3).await;
                }));
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
            let is_timer_iri = |iri: &str| {
                iri == "foundation:automation_TimerEventDefinition"
                    || iri == "foundation:automation_TimerStartEvent"
            };

            // Fast type check before loading the full individual with backlinks —
            // entity-changed-internal fires for every write so we must not pay Individual::get
            // (which loads backlinks) for unrelated high-cardinality entities.
            let is_candidate = crate::owl::is_instance_of(conn, &entity_id, "foundation:automation_TimerEventDefinition")
                || crate::owl::is_instance_of(conn, &entity_id, "foundation:automation_TimerStartEvent");
            if !is_candidate {
                return Ok(false);
            }

            if let Ok(Some(ind)) = Individual::get(conn, &entity_id) {
                if ind.properties.iter().any(|(p, v)| p == "rdf:type" && v.as_iri().map(is_timer_iri).unwrap_or(false)) {
                    return Ok(true);
                }
            }

            if include_retracted {
                if let Ok(Some(ind)) = Individual::get_from_retracted(conn, &entity_id) {
                    return Ok(ind.properties.iter().any(|(p, v)| p == "rdf:type" && v.as_iri().map(is_timer_iri).unwrap_or(false)));
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
        assert_eq!(timers[0].process_iri, "foundation:Process1");
        assert_eq!(timers[0].cron_expr, "0 * * * * *");
        assert_eq!(timers[0].start_event_iri, "foundation:Start1");
        assert!(timers[0].last_run_at.is_none());
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
        assert_eq!(timers[0].process_iri, "foundation:ProcessAct");
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

        let process_iris: Vec<&str> = timers.iter().map(|t| t.process_iri.as_str()).collect();
        assert!(process_iris.contains(&"foundation:ProcessA"));
        assert!(process_iris.contains(&"foundation:ProcessB"));
    }

    #[test]
    fn test_collect_timer_definitions_reads_last_run_at() {
        let mut conn = setup_test_db();
        let triples = timer_start_event_triples(
            "foundation:StartLR", "foundation:TimerLR", "foundation:ProcessLR",
            Some("0 0 9 * * *"), false,
        );
        insert_triples(&mut conn, &triples);
        insert_triples(&mut conn, &[Triple::new(
            "foundation:TimerLR",
            "foundation:lastRunAt",
            Object::Literal {
                value: "2024-01-15T09:00:00Z".to_string(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            },
        )]);

        let timers = collect_timer_definitions(&conn).unwrap();
        assert_eq!(timers.len(), 1);
        let last = timers[0].last_run_at.expect("should have last_run_at");
        assert_eq!(last.to_rfc3339(), "2024-01-15T09:00:00+00:00");
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
