use tauri::{AppHandle, Listener, Manager, Emitter};
use tokio::sync::mpsc;
use crate::owl::{DbExecutor, query_property};
use crate::eavto::{store, Triple, Object};

#[derive(Debug)]
pub enum WorkerCommand {
    Enqueue { entity_iri: String },
}

pub struct QueryWorker {
    pub sender: mpsc::Sender<WorkerCommand>,
}

impl QueryWorker {
    pub fn spawn(app: AppHandle, executor: DbExecutor) -> Self {
        let (tx, mut rx) = mpsc::channel::<WorkerCommand>(1024);

        tauri::async_runtime::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    WorkerCommand::Enqueue { entity_iri } => {
                        process_entity_update(&app, &executor, &entity_iri).await;
                    }
                }
            }
        });

        QueryWorker { sender: tx }
    }
}

pub fn register_listener(app: AppHandle) {
    for event_name in ["entity-created", "entity-updated"] {
        let app_clone = app.clone();
        app.listen(event_name, move |event| {
            let Some(entity_iri) = parse_entity_id(event.payload()) else { return };
            let worker = match app_clone.try_state::<QueryWorker>() {
                Some(w) => w,
                None => return,
            };
            let _ = worker.sender.try_send(WorkerCommand::Enqueue { entity_iri });
        });
    }
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

            app.emit("entity-updated", serde_json::json!({ "entityId": owner_iri })).ok();
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
