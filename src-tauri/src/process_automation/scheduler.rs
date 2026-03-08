use cron::Schedule;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::task::JoinHandle;

use crate::commands::log_backend;
use crate::eavto::query;
use crate::owl::DbExecutor;

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

/// Collects all active TimerEventDefinitions with a timeCycle, spawns one tokio task per schedule.
pub async fn start(app: AppHandle) {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return,
    };

    let timer_defs = executor
        .read(|conn| {
            let result = query::get_by_predicate_object(conn, "rdf:type", "foundation:bpmn_TimerEventDefinition")
                .map_err(|e| e.to_string())?;

            let mut timers: Vec<(String, String)> = Vec::new();
            for triple in &result.triples {
                let subject = &triple.subject;
                let cycle_result = query::get_by_entity_predicate(conn, subject, "foundation:timeCycle")
                    .map_err(|e| e.to_string())?;
                if let Some(cron_expr) = cycle_result.triples.first().and_then(|t| t.object.as_literal()) {
                    let process_result = query::get_by_entity_predicate(conn, subject, "foundation:timerEventOf")
                        .map_err(|e| e.to_string())?;
                    if let Some(process_iri) = process_result.triples.first().and_then(|t| t.object.as_iri()) {
                        timers.push((process_iri.to_string(), cron_expr.to_string()));
                    }
                }
            }
            Ok(timers)
        })
        .await;

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
