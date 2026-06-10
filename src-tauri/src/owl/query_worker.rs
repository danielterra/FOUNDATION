use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Listener, Manager};
use tokio::sync::{mpsc::unbounded_channel, mpsc::UnboundedSender, oneshot};
use crate::owl::{DbExecutor, query_property};
use crate::eavto::{store, Triple, Object};

/// Timeout for a single `await_drained` call. With admission control in the
/// executor, materialization always completes — it may just be queued behind
/// long-running readers (e.g. parallel AgentTasks holding permits across LLM
/// tool loops). 60s covers that tail; the timeout exists only to surface a
/// genuinely stuck worker, so a wide window beats failing healthy executions.
const DRAIN_TIMEOUT_SECS: u64 = 60;

#[derive(Debug)]
pub enum WorkerCommand {
    Enqueue { entity_iri: String },
}

pub struct QueryWorker {
    pub sender: UnboundedSender<WorkerCommand>,
    waiters: Mutex<HashMap<String, Vec<oneshot::Sender<()>>>>,
}

/// Parsed query property with its domain classes, loaded once and cached between writes.
struct CachedQueryProperty {
    property_iri: String,
    config: query_property::QueryConfig,
    domains: Vec<String>,
}

impl QueryWorker {
    pub fn spawn(app: AppHandle, executor: DbExecutor) -> Self {
        let (tx, mut rx) = unbounded_channel::<WorkerCommand>();

        let worker = QueryWorker {
            sender: tx,
            waiters: Mutex::new(HashMap::new()),
        };

        tauri::async_runtime::spawn(async move {
            let mut cache: Option<Arc<Vec<CachedQueryProperty>>> = None;

            while let Some(WorkerCommand::Enqueue { entity_iri }) = rx.recv().await {
                process_entity_update(&app, &executor, &entity_iri, &mut cache).await;
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
    /// all outstanding jobs for it. Returns `Ok(())` on success.
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

        match tokio::time::timeout(
            std::time::Duration::from_secs(DRAIN_TIMEOUT_SECS),
            rx,
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(format!(
                "drain channel closed unexpectedly for {}",
                entity_iri
            )),
            Err(_) => Err(format!(
                "timeout waiting for derived materialization of {}",
                entity_iri
            )),
        }
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

async fn process_entity_update(
    app: &AppHandle,
    executor: &DbExecutor,
    entity_iri: &str,
    cache: &mut Option<Arc<Vec<CachedQueryProperty>>>,
) {
    let entity = entity_iri.to_string();
    let snapshot = cache.clone();

    type ReadResult = (Vec<(String, String, query_property::QueryConfig)>, Option<Arc<Vec<CachedQueryProperty>>>);

    let result: ReadResult = executor.read(move |conn| {
        let needs_reload = snapshot.is_none()
            || snapshot.as_ref().map_or(false, |s| s.iter().any(|e| e.property_iri == entity))
            || has_query_config(conn, &entity);

        let entries: Arc<Vec<CachedQueryProperty>> = if needs_reload {
            Arc::new(load_query_property_cache(conn))
        } else {
            snapshot.unwrap()
        };

        let refreshed = if needs_reload { Some(entries.clone()) } else { None };
        let jobs = collect_affected_jobs(conn, &entity, &entries);
        Ok((jobs, refreshed))
    }).await.unwrap_or_default();

    let (jobs, refreshed) = result;

    if let Some(r) = refreshed {
        *cache = Some(r);
    }

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

/// Returns true if `entity_iri` is the subject of any current `foundation:queryConfig` triple.
/// Used to decide cache invalidation without a full reload.
fn has_query_config(conn: &rusqlite::Connection, entity_iri: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM triples_current WHERE subject = ? AND predicate = 'foundation:queryConfig' LIMIT 1",
        rusqlite::params![entity_iri],
        |_| Ok(true),
    ).unwrap_or(false)
}

/// Loads all query properties with their domain classes into a flat cache.
/// Called only on cache miss or invalidation — not on every entity write.
fn load_query_property_cache(conn: &rusqlite::Connection) -> Vec<CachedQueryProperty> {
    let mut stmt = match conn.prepare(
        "SELECT subject, object_value FROM triples_current WHERE predicate = 'foundation:queryConfig'"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let pairs: Vec<(String, String)> = stmt.query_map(rusqlite::params![], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default();

    pairs.into_iter().filter_map(|(iri, json)| {
        let config = query_property::parse_query_config(&json).ok()?;
        let domains = get_property_domains(conn, &iri);
        Some(CachedQueryProperty { property_iri: iri, config, domains })
    }).collect()
}

fn collect_affected_jobs(
    conn: &rusqlite::Connection,
    entity_iri: &str,
    entries: &[CachedQueryProperty],
) -> Vec<(String, String, query_property::QueryConfig)> {
    let entity_types = get_types(conn, entity_iri);
    let mut jobs: Vec<(String, String, query_property::QueryConfig)> = Vec::new();

    for entry in entries {
        let entity_is_target = entity_types.contains(&entry.config.target_class);

        if entity_is_target {
            // Scenario A: entity is of targetClass → re-eval all owner instances
            let owners = find_owners_for_domains(conn, &entry.domains);
            for owner_iri in owners {
                if !jobs.iter().any(|(o, p, _)| o == &owner_iri && p == &entry.property_iri) {
                    jobs.push((owner_iri, entry.property_iri.clone(), entry.config.clone()));
                }
            }
        }

        // Scenario B: entity itself is an owner of this property
        if entity_types.iter().any(|t| entry.domains.contains(t))
            && !jobs.iter().any(|(o, p, _)| o == entity_iri && p == &entry.property_iri)
        {
            jobs.push((entity_iri.to_string(), entry.property_iri.clone(), entry.config.clone()));
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

fn find_owners_for_domains(conn: &rusqlite::Connection, domains: &[String]) -> Vec<String> {
    let mut owners = Vec::new();
    for class_iri in domains {
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
    // is_current marks the single latest TX per (subject, predicate). Combined with
    // retracted=0, this is precisely the set of current values — no correlated
    // MAX(tx) subquery needed. The invariant is maintained eagerly by assert_triples
    // and retract_triples on every write.
    let current: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT t.object FROM triples t \
             WHERE t.subject = ? AND t.predicate = ? \
               AND t.retracted = 0 AND t.object IS NOT NULL \
               AND t.is_current = 1"
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
                retracted INTEGER NOT NULL DEFAULT 0,
                is_current INTEGER NOT NULL DEFAULT 1
             );
             CREATE VIEW IF NOT EXISTS triples_current AS
             SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_number, object_integer, object_boolean, tx, origin_id, object_type, created_at
             FROM triples
             WHERE is_current = 1 AND retracted = 0;",
        )
        .unwrap();
        conn
    }

    fn insert_triple_iri(conn: &Connection, subject: &str, predicate: &str, object: &str, tx: i64) {
        conn.execute(
            "UPDATE triples SET is_current = 0 WHERE subject = ?1 AND predicate = ?2",
            rusqlite::params![subject, predicate],
        ).unwrap();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted, is_current)
             VALUES (?1, ?2, ?3, 'iri', ?4, 1, 0, 0, 1)",
            rusqlite::params![subject, predicate, object, tx],
        ).unwrap();
    }

    fn insert_triple_literal(conn: &Connection, subject: &str, predicate: &str, value: &str, tx: i64) {
        conn.execute(
            "UPDATE triples SET is_current = 0 WHERE subject = ?1 AND predicate = ?2",
            rusqlite::params![subject, predicate],
        ).unwrap();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, tx, origin_id, created_at, retracted, is_current)
             VALUES (?1, ?2, ?3, 'literal', ?4, 1, 0, 0, 1)",
            rusqlite::params![subject, predicate, value, tx],
        ).unwrap();
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

    /// `load_query_property_cache` retorna config + domains a partir de triples inseridas.
    #[test]
    fn test_load_query_property_cache_returns_config_and_domains() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;

        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);
        insert_triple_iri(&conn, prop, "rdfs:domain", "foundation:Project", 1);

        let cache = load_query_property_cache(&conn);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache[0].property_iri, prop);
        assert_eq!(cache[0].config.target_class, "foundation:Task");
        assert_eq!(cache[0].domains, vec!["foundation:Project".to_string()]);
    }

    /// `collect_affected_jobs` cenário A: entity é do targetClass → owner recebe job.
    #[test]
    fn test_collect_affected_jobs_scenario_a_entity_is_target() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        let owner = "foundation:proj1";
        let task = "foundation:task1";

        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);
        insert_triple_iri(&conn, prop, "rdfs:domain", "foundation:Project", 1);
        insert_triple_iri(&conn, owner, "rdf:type", "foundation:Project", 1);
        insert_triple_iri(&conn, task, "rdf:type", "foundation:Task", 1);

        let cache = load_query_property_cache(&conn);
        let jobs = collect_affected_jobs(&conn, task, &cache);

        assert_eq!(jobs.len(), 1, "one job expected for owner");
        assert_eq!(jobs[0].0, owner);
        assert_eq!(jobs[0].1, prop);
    }

    /// `collect_affected_jobs` cenário B: entity é o owner → recebe job para si.
    #[test]
    fn test_collect_affected_jobs_scenario_b_entity_is_owner() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        let owner = "foundation:proj1";

        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);
        insert_triple_iri(&conn, prop, "rdfs:domain", "foundation:Project", 1);
        insert_triple_iri(&conn, owner, "rdf:type", "foundation:Project", 1);

        let cache = load_query_property_cache(&conn);

        // No task instances → scenario A produces nothing.
        // Owner itself triggers scenario B.
        let jobs = collect_affected_jobs(&conn, owner, &cache);
        assert!(jobs.iter().any(|(o, p, _)| o == owner && p == prop));
    }

    /// Invalidação: entity cujo IRI está no cache como property_iri → needs_reload=true.
    #[test]
    fn test_needs_reload_when_entity_is_cached_property() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);

        let cache = Arc::new(load_query_property_cache(&conn));
        let snapshot = Some(cache.clone());

        let needs_reload = snapshot.is_none()
            || snapshot.as_ref().map_or(false, |s| s.iter().any(|e| e.property_iri == prop))
            || has_query_config(&conn, prop);

        assert!(needs_reload, "property IRI presente no cache deve forçar reload");
    }

    /// Invalidação: entity alheia ao cache sem queryConfig → sem reload.
    #[test]
    fn test_no_reload_for_unrelated_entity() {
        let conn = setup_db();
        let prop = "foundation:myQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;
        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);

        let cache = Arc::new(load_query_property_cache(&conn));
        let snapshot = Some(cache.clone());
        let unrelated = "foundation:someOtherEntity";

        let needs_reload = snapshot.is_none()
            || snapshot.as_ref().map_or(false, |s| s.iter().any(|e| e.property_iri == unrelated))
            || has_query_config(&conn, unrelated);

        assert!(!needs_reload, "entity alheia não deve forçar reload");
    }

    /// Invalidação: entity com triple queryConfig → needs_reload=true mesmo ausente do cache anterior.
    #[test]
    fn test_needs_reload_when_entity_has_query_config_triple() {
        let conn = setup_db();
        let prop = "foundation:existingProp";
        let new_prop = "foundation:newQueryProp";
        let config_json = r#"{"targetClass":"foundation:Task","filters":[]}"#;

        insert_triple_literal(&conn, prop, "foundation:queryConfig", config_json, 1);
        let cache = Arc::new(load_query_property_cache(&conn));

        insert_triple_literal(&conn, new_prop, "foundation:queryConfig", config_json, 2);
        let snapshot = Some(cache.clone());

        let needs_reload = snapshot.is_none()
            || snapshot.as_ref().map_or(false, |s| s.iter().any(|e| e.property_iri == new_prop))
            || has_query_config(&conn, new_prop);

        assert!(needs_reload, "nova queryConfig triple deve forçar reload");
    }
}
