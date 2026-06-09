use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Listener, Manager};
use tokio::sync::{mpsc::unbounded_channel, mpsc::UnboundedSender, oneshot};
use crate::owl::{DbExecutor, query_property};
use crate::eavto::{store, Triple, Object};

const DRAIN_TIMEOUT_SECS: u64 = 10;

#[derive(Debug)]
pub enum WorkerCommand {
    Enqueue { entity_iri: String },
}

pub struct QueryWorker {
    pub sender: UnboundedSender<WorkerCommand>,
    waiters: Mutex<HashMap<String, Vec<oneshot::Sender<()>>>>,
}

impl QueryWorker {
    pub fn spawn(app: AppHandle, executor: DbExecutor) -> Self {
        let (tx, mut rx) = unbounded_channel::<WorkerCommand>();

        let worker = QueryWorker {
            sender: tx,
            waiters: Mutex::new(HashMap::new()),
        };

        tauri::async_runtime::spawn(async move {
            while let Some(WorkerCommand::Enqueue { entity_iri }) = rx.recv().await {
                process_entity_update(&app, &executor, &entity_iri).await;
                // Resolve waiters inline — no channel round-trip that could be
                // silently dropped when the channel is saturated.
                if let Some(w) = app.try_state::<QueryWorker>() {
                    let senders: Vec<oneshot::Sender<()>> = w
                        .waiters
                        .lock()
                        .map(|mut map| map.remove(&entity_iri).unwrap_or_default())
                        .unwrap_or_default();
                    for s in senders {
                        let _ = s.send(());
                    }
                }
            }
        });

        worker
    }

    /// Enqueue `entity_iri` for recompute and wait until the worker has processed
    /// all outstanding jobs for it, or until `DRAIN_TIMEOUT_SECS` elapses.
    ///
    /// Returns `Ok(())` when the worker confirmed completion, `Err(String)` on
    /// timeout (caller should fail the step with a clear message).
    pub async fn await_drained(&self, entity_iri: &str) -> Result<(), String> {
        let (tx, rx) = oneshot::channel::<()>();
        {
            let mut map = self.waiters.lock().map_err(|e| e.to_string())?;
            map.entry(entity_iri.to_string()).or_default().push(tx);
        }
        self.sender
            .send(WorkerCommand::Enqueue {
                entity_iri: entity_iri.to_string(),
            })
            .map_err(|_| format!("query_worker channel closed for {}", entity_iri))?;
        tokio::time::timeout(
            std::time::Duration::from_secs(DRAIN_TIMEOUT_SECS),
            rx,
        )
        .await
        .map_err(|_| format!(
            "timeout waiting for derived materialization of control instance {}",
            entity_iri
        ))?
        .map_err(|_| format!(
            "drain channel closed unexpectedly for {}",
            entity_iri
        ))
    }
}

pub fn register_listener(app: AppHandle) {
    let app_clone = app.clone();
    app.listen("entity-changed-internal", move |event| {
        let Some(entity_iri) = parse_entity_id(event.payload()) else { return };
        let worker = match app_clone.try_state::<QueryWorker>() {
            Some(w) => w,
            None => return,
        };
        let _ = worker.sender.send(WorkerCommand::Enqueue { entity_iri });
    });
}

fn parse_entity_id(payload: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    v["entityId"].as_str().map(String::from)
}

async fn process_entity_update(app: &AppHandle, executor: &DbExecutor, entity_iri: &str) {
    let entity = entity_iri.to_string();

    let jobs: Vec<(String, String, query_property::QueryConfig)> = executor.read(move |conn| {
        Ok(collect_affected_jobs(conn, &entity))
    }).await.unwrap_or_default();

    if jobs.is_empty() {
        return;
    }

    let formula_worker = app.try_state::<crate::owl::formula_worker::FormulaWorker>();

    for (owner_iri, property_iri, config) in jobs {
        let owner = owner_iri.clone();
        let cfg = config.clone();

        let new_results: Vec<String> = match executor.read(move |conn| {
            query_property::evaluate_query(conn, &owner, &cfg).map_err(|e| e.to_string())
        }).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        let owner_c = owner_iri.clone();
        let prop_c = property_iri.clone();
        let results_c = new_results.clone();

        let changed: bool = executor.write(move |conn| {
            update_query_property_triples(conn, &owner_c, &prop_c, &results_c)
                .map(|b| b.to_string())
                .map_err(|e| e.to_string())
        }).await.map(|s| s == "true").unwrap_or(false);

        if changed {
            if let Some(ref worker) = formula_worker {
                let owner_c = owner_iri.clone();
                let prop_c = property_iri.clone();
                let job_ids: Vec<String> = executor.write(move |conn| {
                    let jobs = crate::owl::formula_worker::create_instance_recalc_jobs(
                        conn, &owner_c, &prop_c,
                    );
                    Ok(serde_json::to_string(&jobs).unwrap_or_default())
                }).await
                    .map(|s| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default())
                    .unwrap_or_default();

                for job_id in job_ids {
                    let _ = worker.sender.try_send(
                        crate::owl::formula_worker::WorkerCommand::Enqueue { job_id }
                    );
                }
            }

            crate::realtime::emit_entity_updated(&app, &owner_iri);
        }
    }
}

fn collect_affected_jobs(
    conn: &rusqlite::Connection,
    entity_iri: &str,
) -> Vec<(String, String, query_property::QueryConfig)> {
    let entity_types = get_types(conn, entity_iri);
    let all_query_props = load_all_query_properties(conn);
    let mut jobs: Vec<(String, String, query_property::QueryConfig)> = Vec::new();

    for (prop_iri, config) in &all_query_props {
        let entity_is_target = entity_types.contains(&config.target_class);

        if entity_is_target {
            // Scenario A: entity is of targetClass → re-eval all owner instances
            let owners = find_owners_for_property(conn, prop_iri);
            for owner_iri in owners {
                if !jobs.iter().any(|(o, p, _)| o == &owner_iri && p == prop_iri) {
                    jobs.push((owner_iri, prop_iri.clone(), config.clone()));
                }
            }
        }

        // Scenario B: entity itself is an owner of this property
        let owner_classes = get_property_domains(conn, prop_iri);
        if entity_types.iter().any(|t| owner_classes.contains(t))
            && !jobs.iter().any(|(o, p, _)| o == entity_iri && p == prop_iri)
        {
            jobs.push((entity_iri.to_string(), prop_iri.clone(), config.clone()));
        }
    }

    jobs
}

fn get_types(conn: &rusqlite::Connection, iri: &str) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT DISTINCT object FROM triples_current WHERE subject = ? AND predicate = 'rdf:type'"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map(rusqlite::params![iri], |row| row.get::<_, String>(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

fn load_all_query_properties(conn: &rusqlite::Connection) -> Vec<(String, query_property::QueryConfig)> {
    let mut stmt = match conn.prepare(
        "SELECT subject, object_value FROM triples_current WHERE predicate = 'foundation:queryConfig'"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map(rusqlite::params![], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .map(|rows| {
        rows.filter_map(|r| r.ok())
            .filter_map(|(iri, json)| {
                query_property::parse_query_config(&json).ok().map(|cfg| (iri, cfg))
            })
            .collect()
    })
    .unwrap_or_default()
}

fn get_property_domains(conn: &rusqlite::Connection, property_iri: &str) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT object FROM triples_current WHERE subject = ? AND predicate = 'rdfs:domain'"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map(rusqlite::params![property_iri], |row| row.get::<_, String>(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

fn find_owners_for_property(conn: &rusqlite::Connection, property_iri: &str) -> Vec<String> {
    let domain_classes = get_property_domains(conn, property_iri);
    let mut owners = Vec::new();
    for class_iri in domain_classes {
        let mut stmt = match conn.prepare(
            "SELECT DISTINCT subject FROM triples_current \
             WHERE predicate = 'rdf:type' AND object = ?"
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let class_owners: Vec<String> = stmt
            .query_map(rusqlite::params![class_iri], |row| row.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        owners.extend(class_owners);
    }
    owners
}

fn update_query_property_triples(
    conn: &mut rusqlite::Connection,
    owner_iri: &str,
    property_iri: &str,
    new_results: &[String],
) -> Result<bool, crate::owl::OwlError> {
    // Read current values at MAX(tx) only. Per the immutability model, the latest TX
    // is the source of truth; reading from triples_current (which only filters by
    // retracted=0) would return historical values too, causing spurious diffs.
    let current: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT t.object FROM triples t \
             WHERE t.subject = ? AND t.predicate = ? \
               AND t.retracted = 0 AND t.object IS NOT NULL \
               AND t.tx = (SELECT MAX(tx) FROM triples \
                           WHERE subject = t.subject AND predicate = t.predicate)"
        ).map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        let rows: Vec<String> = stmt
            .query_map(rusqlite::params![owner_iri, property_iri], |row| row.get::<_, String>(0))
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    let current_set: std::collections::HashSet<&str> =
        current.iter().map(|s| s.as_str()).collect();
    let new_set: std::collections::HashSet<&str> =
        new_results.iter().map(|s| s.as_str()).collect();

    if current_set == new_set {
        return Ok(false);
    }

    if new_results.is_empty() {
        // Clearing the property: retract is the documented "fact no longer true"
        // operation. Without a non-empty new TX, MAX(tx) would still point to old data.
        let old_triples: Vec<Triple> = current.iter()
            .map(|obj| Triple::new(owner_iri, property_iri, Object::Iri(obj.clone())))
            .collect();
        if !old_triples.is_empty() {
            store::retract_triples(conn, &old_triples, "query_worker")?;
        }
    } else {
        // Just assert a new TX with the new values. The latest TX defines the truth —
        // retracting old values is unnecessary and contradicts the immutability model.
        let new_triples: Vec<Triple> = new_results.iter()
            .map(|obj| Triple::new(owner_iri, property_iri, Object::Iri(obj.clone())))
            .collect();
        store::assert_triples(conn, &new_triples, "query_worker")?;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                origin TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE origins (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE
             );
             CREATE TABLE triples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                subject TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object TEXT,
                object_value TEXT,
                object_type TEXT NOT NULL,
                object_datatype TEXT,
                object_language TEXT,
                object_number REAL,
                object_integer INTEGER,
                object_datetime INTEGER,
                object_boolean INTEGER,
                tx INTEGER NOT NULL DEFAULT 0,
                origin_id INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL DEFAULT 0,
                retracted INTEGER NOT NULL DEFAULT 0
             );",
        )
        .unwrap();
        conn
    }

    fn count_triples(conn: &Connection, subject: &str, predicate: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = ? AND retracted = 0",
            rusqlite::params![subject, predicate],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    fn max_tx(conn: &Connection, subject: &str, predicate: &str) -> i64 {
        conn.query_row(
            "SELECT COALESCE(MAX(tx), 0) FROM triples WHERE subject = ? AND predicate = ?",
            rusqlite::params![subject, predicate],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    /// Calling `update_query_property_triples` twice with the same set must not
    /// write a new TX on the second call (idempotent — prevents oscillation loops).
    #[test]
    fn test_no_write_when_set_unchanged() {
        let mut conn = setup_db();
        let owner = "foundation:Owner1";
        let prop = "foundation:queryProp";
        let values = vec!["foundation:A".to_string(), "foundation:B".to_string()];

        let changed_first = update_query_property_triples(&mut conn, owner, prop, &values).unwrap();
        assert!(changed_first, "first write must report changed=true");

        let tx_after_first = max_tx(&conn, owner, prop);
        let changed_second = update_query_property_triples(&mut conn, owner, prop, &values).unwrap();
        assert!(!changed_second, "second write with identical set must not write (changed=false)");

        let tx_after_second = max_tx(&conn, owner, prop);
        assert_eq!(
            tx_after_first, tx_after_second,
            "TX must not advance when the set is already current"
        );
    }

    /// When the result set changes (A,B → A,B,C), a new TX is written and `changed=true`.
    #[test]
    fn test_write_when_set_grows() {
        let mut conn = setup_db();
        let owner = "foundation:Owner2";
        let prop = "foundation:queryProp";

        let first = vec!["foundation:A".to_string(), "foundation:B".to_string()];
        let _ = update_query_property_triples(&mut conn, owner, prop, &first).unwrap();
        let tx_v1 = max_tx(&conn, owner, prop);

        let second = vec![
            "foundation:A".to_string(),
            "foundation:B".to_string(),
            "foundation:C".to_string(),
        ];
        let changed = update_query_property_triples(&mut conn, owner, prop, &second).unwrap();
        assert!(changed);
        assert!(
            max_tx(&conn, owner, prop) > tx_v1,
            "TX must advance when the result set grows"
        );
    }

    /// Clearing the set (new_results = []) must retract and report changed=true,
    /// without generating a new IRI triple.
    #[test]
    fn test_clear_retracts_and_reports_changed() {
        let mut conn = setup_db();
        let owner = "foundation:Owner3";
        let prop = "foundation:queryProp";

        let initial = vec!["foundation:A".to_string()];
        let _ = update_query_property_triples(&mut conn, owner, prop, &initial).unwrap();

        let changed = update_query_property_triples(&mut conn, owner, prop, &[]).unwrap();
        assert!(changed, "clearing must report changed=true");
        assert_eq!(
            count_triples(&conn, owner, prop),
            0,
            "all triples must be retracted after clearing"
        );
    }

    /// Calling update once with A then again with A must converge: second call
    /// is a no-op, ensuring the query→formula→query chain terminates in ≤ 2 cycles.
    #[test]
    fn test_convergence_idempotent_in_two_cycles() {
        let mut conn = setup_db();
        let owner = "foundation:Owner4";
        let prop = "foundation:queryPropCycle";
        let values = vec!["foundation:X".to_string()];

        let c1 = update_query_property_triples(&mut conn, owner, prop, &values).unwrap();
        assert!(c1);
        let c2 = update_query_property_triples(&mut conn, owner, prop, &values).unwrap();
        assert!(!c2, "second cycle with same result must not trigger further writes");
    }
}
