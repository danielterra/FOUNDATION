use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tokio::sync::{mpsc, oneshot};

/// Ceiling on a single `await_drained`. Mirrors the query worker's window: the
/// timeout exists to surface a stuck worker, not to cut short a healthy queue
/// waiting behind long readers.
const DRAIN_TIMEOUT_SECS: u64 = 60;

#[derive(Debug)]
pub enum WorkerCommand {
    Enqueue { job_id: String },
    Cancel { property_iri: String },
}

pub struct FormulaWorker {
    pub sender: mpsc::Sender<WorkerCommand>,
    /// One-shot channels waiting for an instance's calculated values to settle,
    /// keyed by instance IRI. Populated by `await_drained`, drained by the worker
    /// when the last job for that instance finishes.
    waiters: Mutex<HashMap<String, Vec<oneshot::Sender<()>>>>,
    /// Jobs still queued or running per instance. A waiter is only released at
    /// zero — releasing per job would let the caller read a half-settled instance.
    inflight: Mutex<HashMap<String, usize>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FormulaProgressEvent {
    pub job_id: String,
    pub property_iri: String,
    pub property_label: String,
    pub class_iri: String,
    pub class_label: String,
    pub percent: u32,
    pub status: String,
}

impl FormulaWorker {
    pub fn spawn(app: tauri::AppHandle, executor: crate::owl::DbExecutor) -> Self {
        let (tx, mut rx) = mpsc::channel::<WorkerCommand>(1024);
        let enqueue_tx = tx.clone();

        tauri::async_runtime::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    WorkerCommand::Enqueue { job_id } => {
                        process_job(&app, &executor, &job_id, &enqueue_tx).await;
                    }
                    WorkerCommand::Cancel { property_iri } => {
                        cancel_jobs_for_property(&executor, &property_iri).await;
                    }
                }
            }
        });

        FormulaWorker {
            sender: tx,
            waiters: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
        }
    }

    /// Queue every calculated property of `instance_iri`'s class and wait until
    /// they have all been recomputed.
    ///
    /// This is what lets a caller read a derived value and know it reflects the
    /// writes that just happened. Without it, a total computed from a value written
    /// a moment ago is read stale, and nothing about the read says so.
    pub async fn await_drained(
        app: &tauri::AppHandle,
        executor: &crate::owl::DbExecutor,
        instance_iri: &str,
    ) -> Result<(), String> {
        let Some(worker) = app.try_state::<FormulaWorker>() else {
            return Ok(());
        };

        let iri = instance_iri.to_string();
        let job_ids: Vec<String> = executor
            .write(move |conn| Ok(create_class_calculated_recalc_jobs(conn, &iri).join(",")))
            .await
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Nothing derived on this class: already settled, by definition.
        if job_ids.is_empty() {
            return Ok(());
        }

        let (tx, rx) = oneshot::channel::<()>();
        {
            let mut inflight = worker.inflight.lock().map_err(|e| e.to_string())?;
            *inflight.entry(instance_iri.to_string()).or_insert(0) += job_ids.len();
        }
        {
            let mut waiters = worker.waiters.lock().map_err(|e| e.to_string())?;
            waiters
                .entry(instance_iri.to_string())
                .or_default()
                .push(tx);
        }

        for job_id in job_ids {
            if worker
                .sender
                .send(WorkerCommand::Enqueue { job_id })
                .await
                .is_err()
            {
                release_waiters(app, instance_iri);
                return Err(format!(
                    "formula_worker channel closed for {}",
                    instance_iri
                ));
            }
        }

        match tokio::time::timeout(std::time::Duration::from_secs(DRAIN_TIMEOUT_SECS), rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(format!(
                "drain channel closed unexpectedly for {}",
                instance_iri
            )),
            Err(_) => Err(format!(
                "timeout waiting for calculated properties of {} to settle",
                instance_iri
            )),
        }
    }
}

/// Marks one job done for `instance_iri` and releases the waiters once none remain.
fn job_settled(app: &tauri::AppHandle, instance_iri: &str) {
    let Some(worker) = app.try_state::<FormulaWorker>() else {
        return;
    };
    let now_zero = match worker.inflight.lock() {
        Ok(mut map) => match map.get_mut(instance_iri) {
            Some(count) => {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    map.remove(instance_iri);
                    true
                } else {
                    false
                }
            }
            None => false,
        },
        Err(_) => false,
    };
    if now_zero {
        release_waiters(app, instance_iri);
    }
}

fn release_waiters(app: &tauri::AppHandle, instance_iri: &str) {
    let Some(worker) = app.try_state::<FormulaWorker>() else {
        return;
    };
    let senders: Vec<oneshot::Sender<()>> = worker
        .waiters
        .lock()
        .map(|mut map| map.remove(instance_iri).unwrap_or_default())
        .unwrap_or_default();
    for s in senders {
        let _ = s.send(());
    }
}

struct JobRecord {
    property_iri: String,
    property_label: String,
    class_iri: String,
    class_label: String,
    processed: i64,
    last_offset: i64,
    instance_iri: Option<String>,
}

async fn process_job(
    app: &tauri::AppHandle,
    executor: &crate::owl::DbExecutor,
    job_id: &str,
    sender: &mpsc::Sender<WorkerCommand>,
) {
    let job_id_owned = job_id.to_string();
    let job = match executor
        .read(move |conn| Ok(load_job(conn, &job_id_owned)))
        .await
    {
        Ok(Some(j)) => j,
        // The job row is gone. Whoever is waiting on this instance must still be
        // released, or it blocks for the full drain timeout on work that will
        // never arrive.
        _ => {
            let jid = job_id.to_string();
            if let Ok(Some(inst)) = executor
                .read(move |conn| {
                    Ok(conn
                        .query_row(
                            "SELECT instance_iri FROM formula_recalc_jobs WHERE id = ?",
                            rusqlite::params![jid],
                            |row| row.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten())
                })
                .await
            {
                job_settled(app, &inst);
            }
            return;
        }
    };

    let job_id_owned = job_id.to_string();
    if executor
        .write(move |conn| {
            conn.execute(
                "UPDATE formula_recalc_jobs SET status = 'running', updated_at = ? WHERE id = ?",
                rusqlite::params![now_millis(), job_id_owned],
            )
            .map(|_| String::new())
            .map_err(|e| e.to_string())
        })
        .await
        .is_err()
    {
        if let Some(ref inst) = job.instance_iri {
            job_settled(app, inst);
        }
        return;
    }

    let prop_iri = job.property_iri.clone();
    let class_iri = if job.class_iri.is_empty() {
        let prop = prop_iri.clone();
        executor
            .read(move |conn| Ok(fetch_domain(conn, &prop)))
            .await
            .unwrap_or_default()
    } else {
        job.class_iri.clone()
    };

    let instance_iris: Vec<String> = if let Some(ref inst) = job.instance_iri {
        vec![inst.clone()]
    } else {
        let ci = class_iri.clone();
        let offset = job.last_offset;
        executor
            .read(move |conn| Ok(query_class_instances(conn, &ci, offset)))
            .await
            .unwrap_or_default()
    };

    let total: i64 = if job.instance_iri.is_some() {
        1
    } else {
        let ci = class_iri.clone();
        executor
            .read(move |conn| {
                conn.query_row(
                    "SELECT COUNT(DISTINCT subject) FROM triples \
                 WHERE predicate = 'rdf:type' AND object = ? AND retracted = 0",
                    rusqlite::params![ci],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())
            })
            .await
            .unwrap_or(0)
    };

    let job_id_owned = job_id.to_string();
    let _ = executor
        .write(move |conn| {
            conn.execute(
                "UPDATE formula_recalc_jobs SET total = ?, updated_at = ? WHERE id = ?",
                rusqlite::params![total, now_millis(), job_id_owned],
            )
            .map(|_| String::new())
            .map_err(|e| e.to_string())
        })
        .await;

    let mut processed = job.processed;
    let mut last_offset = job.last_offset;
    let mut last_pct: u32 = (processed * 100 / total.max(1)) as u32;
    let mut cancelled = false;

    for instance_iri in &instance_iris {
        let instance = instance_iri.clone();
        let prop = job.property_iri.clone();
        let job_id_owned = job_id.to_string();

        let (current_status, eval_result) = match executor
            .read(move |conn| {
                let status: String = conn
                    .query_row(
                        "SELECT status FROM formula_recalc_jobs WHERE id = ?",
                        rusqlite::params![job_id_owned],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let eval = evaluate_formula_for_instance(conn, &instance, &prop);
                Ok((status, eval))
            })
            .await
        {
            Ok(pair) => pair,
            Err(_) => break,
        };

        if current_status == "cancelled" {
            cancelled = true;
            break;
        }

        match eval_result {
            Ok(value) => {
                let inst = instance_iri.clone();
                let prop = job.property_iri.clone();
                let dependent_jobs = executor
                    .write(move |conn| {
                        store_calculated_value(conn, &inst, &prop, &value);
                        let jobs = create_instance_recalc_jobs(conn, &inst, &prop);
                        Ok(jobs.join(","))
                    })
                    .await
                    .unwrap_or_default();
                for jid in dependent_jobs.split(',').filter(|s| !s.is_empty()) {
                    let _ = sender.try_send(WorkerCommand::Enqueue {
                        job_id: jid.to_string(),
                    });
                }
                if job.instance_iri.is_some() {
                    crate::realtime::emit_entity_updated(&app, &instance_iri);
                }
            }
            Err(msg) => {
                let inst = instance_iri.clone();
                let prop = job.property_iri.clone();
                let now = now_millis();
                let _ = executor.write(move |conn| {
                    conn.execute(
                        "INSERT INTO formula_instance_errors (instance_iri, property_iri, error_message, created_at)
                         VALUES (?, ?, ?, ?)
                         ON CONFLICT(instance_iri, property_iri) DO UPDATE SET error_message = excluded.error_message, created_at = excluded.created_at",
                        rusqlite::params![inst, prop, msg, now],
                    ).map(|_| String::new()).map_err(|e| e.to_string())
                }).await;
            }
        }

        processed += 1;
        last_offset += 1;

        let job_id_owned = job_id.to_string();
        let _ = executor.write(move |conn| {
            conn.execute(
                "UPDATE formula_recalc_jobs SET processed = ?, last_offset = ?, updated_at = ? WHERE id = ?",
                rusqlite::params![processed, last_offset, now_millis(), job_id_owned],
            ).map(|_| String::new()).map_err(|e| e.to_string())
        }).await;

        let pct = (processed * 100 / total.max(1)) as u32;
        if pct != last_pct {
            last_pct = pct;
            let event = FormulaProgressEvent {
                job_id: job_id.to_string(),
                property_iri: job.property_iri.clone(),
                property_label: job.property_label.clone(),
                class_iri: class_iri.clone(),
                class_label: job.class_label.clone(),
                percent: pct,
                status: "running".to_string(),
            };
            app.emit("formula-recalc-progress", event).ok();
        }
    }

    if let Some(ref inst) = job.instance_iri {
        job_settled(app, inst);
    }

    let final_status = if cancelled { "cancelled" } else { "completed" };
    let job_id_owned = job_id.to_string();
    let fs = final_status.to_string();
    let _ = executor
        .write(move |conn| {
            conn.execute(
                "UPDATE formula_recalc_jobs SET status = ?, updated_at = ? WHERE id = ?",
                rusqlite::params![fs, now_millis(), job_id_owned],
            )
            .map(|_| String::new())
            .map_err(|e| e.to_string())
        })
        .await;

    let event = FormulaProgressEvent {
        job_id: job_id.to_string(),
        property_iri: job.property_iri.clone(),
        property_label: job.property_label.clone(),
        class_iri: class_iri.clone(),
        class_label: job.class_label.clone(),
        percent: last_pct,
        status: final_status.to_string(),
    };
    app.emit("formula-recalc-progress", event).ok();
}

fn load_job(conn: &Connection, job_id: &str) -> Option<JobRecord> {
    conn.query_row(
        "SELECT property_iri, property_label, class_iri, class_label, \
                processed, last_offset, status, instance_iri \
         FROM formula_recalc_jobs WHERE id = ?",
        rusqlite::params![job_id],
        |row| {
            let status: String = row.get(6)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                status,
                row.get::<_, Option<String>>(7)?,
            ))
        },
    )
    .ok()
    .and_then(
        |(
            property_iri,
            property_label,
            class_iri,
            class_label,
            processed,
            last_offset,
            status,
            instance_iri,
        )| {
            if status != "pending" {
                return None;
            }
            Some(JobRecord {
                property_iri,
                property_label,
                class_iri,
                class_label,
                processed,
                last_offset,
                instance_iri,
            })
        },
    )
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn evaluate_formula_for_instance(
    conn: &Connection,
    instance_iri: &str,
    property_iri: &str,
) -> Result<String, String> {
    // Respect the immutable TX model: the predicate with the highest tx is the current intent.
    // A property may have historical aggregation triples with retracted=0 that predate a formula
    // update — checking COUNT(*) would incorrectly prefer the stale aggregation.
    let mode: Option<String> = conn
        .query_row(
            "SELECT predicate FROM triples \
         WHERE subject = ? AND predicate IN ('foundation:formula', 'foundation:aggregation') \
         AND retracted = 0 \
         ORDER BY tx DESC LIMIT 1",
            rusqlite::params![property_iri],
            |row| row.get(0),
        )
        .ok();

    match mode.as_deref() {
        Some("foundation:aggregation") => {
            crate::owl::aggregation::evaluate_aggregation_for_instance(
                conn,
                instance_iri,
                property_iri,
            )
        }
        Some("foundation:formula") => {
            crate::owl::formula::evaluate_formula_for_instance_raw(conn, instance_iri, property_iri)
        }
        _ => Err(format!(
            "Property '{}' has no formula or aggregation",
            property_iri
        )),
    }
}

fn store_calculated_value(
    conn: &mut Connection,
    instance_iri: &str,
    property_iri: &str,
    value: &str,
) {
    use crate::eavto::{store, Object, Triple};
    let datatype = resolve_calculated_datatype(conn, property_iri);
    let triple = Triple::new(
        instance_iri,
        property_iri,
        Object::Literal {
            value: value.to_string(),
            datatype: Some(datatype),
            language: None,
        },
    );
    let _ = store::assert_triples(conn, &[triple], "formula");
    let _ = conn.execute(
        "DELETE FROM formula_instance_errors WHERE instance_iri = ? AND property_iri = ?",
        rusqlite::params![instance_iri, property_iri],
    );
}

/// Decides the `object_datatype` to store for a calculated value, from the
/// property's declared `rdfs:range`. Numbers stay numbers (integer ranges become
/// `xsd:integer`, the rest `xsd:decimal`) so downstream formats them correctly;
/// text, boolean and date ranges are stored as themselves.
fn resolve_calculated_datatype(conn: &Connection, property_iri: &str) -> String {
    const INTEGER_RANGES: &[&str] = &[
        "xsd:integer",
        "xsd:int",
        "xsd:long",
        "xsd:short",
        "xsd:byte",
        "xsd:nonNegativeInteger",
        "xsd:positiveInteger",
    ];
    let range: Option<String> =
        crate::eavto::query::get_by_entity_predicate(conn, property_iri, "rdfs:range")
            .ok()
            .and_then(|r| r.triples.into_iter().next())
            .and_then(|t| t.object.as_iri().map(|s| s.to_string()));
    match range.as_deref() {
        Some(r) if INTEGER_RANGES.contains(&r) => "xsd:integer".to_string(),
        // A calculated property is no longer always a number: a formula can build
        // text or answer a yes/no. Storing "Overdue" as xsd:decimal would make every
        // reader downstream try to parse it as one.
        Some("xsd:string") => "xsd:string".to_string(),
        Some("xsd:boolean") => "xsd:boolean".to_string(),
        Some("xsd:date") => "xsd:date".to_string(),
        Some("xsd:dateTime") => "xsd:dateTime".to_string(),
        _ => "xsd:decimal".to_string(),
    }
}

fn fetch_label(conn: &Connection, iri: &str) -> String {
    crate::eavto::query::get_by_entity_predicate(conn, iri, "rdfs:label")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_literal().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn fetch_domain(conn: &Connection, iri: &str) -> String {
    crate::eavto::query::get_by_entity_predicate(conn, iri, "rdfs:domain")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_iri().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn query_class_instances(conn: &Connection, class_iri: &str, offset: i64) -> Vec<String> {
    conn.prepare(
        "SELECT DISTINCT subject FROM triples_current
         WHERE predicate = 'rdf:type' AND object = ?
         ORDER BY subject
         LIMIT -1 OFFSET ?",
    )
    .map(|mut stmt| {
        stmt.query_map(rusqlite::params![class_iri, offset], |row| {
            row.get::<_, String>(0)
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
        .unwrap_or_default()
    })
    .unwrap_or_default()
}

fn query_formula_properties_by_reference(
    conn: &Connection,
    placeholder: &str,
) -> Vec<(String, String, String)> {
    conn.prepare(
        "SELECT subject, object_value, predicate FROM triples_current \
         WHERE predicate IN ('foundation:formula', 'foundation:aggregation')",
    )
    .map(|mut stmt| {
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map(|rows| {
            rows.filter_map(|r| r.ok())
                .filter(|(_, formula, _)| formula.contains(placeholder))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    })
    .unwrap_or_default()
}

async fn cancel_jobs_for_property(executor: &crate::owl::DbExecutor, property_iri: &str) {
    let prop = property_iri.to_string();
    let _ = executor.write(move |conn| {
        conn.execute(
            "UPDATE formula_recalc_jobs SET status = 'cancelled', updated_at = ? WHERE property_iri = ? AND status IN ('pending', 'running')",
            rusqlite::params![now_millis(), prop],
        ).map(|_| String::new()).map_err(|e| e.to_string())
    }).await;
}

pub fn cancel_and_create_property_job(conn: &Connection, property_iri: &str) -> Option<String> {
    let _ = conn.execute(
        "UPDATE formula_recalc_jobs SET status = 'cancelled', updated_at = ?
         WHERE property_iri = ? AND instance_iri IS NULL AND status IN ('pending', 'running')",
        rusqlite::params![now_millis(), property_iri],
    );

    let property_label = fetch_label(conn, property_iri);
    let class_iri = fetch_domain(conn, property_iri);
    let class_label = if class_iri.is_empty() {
        String::new()
    } else {
        fetch_label(conn, &class_iri)
    };

    let now = now_millis();
    let job_id = format!("job_{}_{}", now, property_iri.replace(':', "_"));
    let result = conn.execute(
        "INSERT INTO formula_recalc_jobs
         (id, property_iri, property_label, class_iri, class_label, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 'pending', ?, ?)",
        rusqlite::params![
            job_id,
            property_iri,
            property_label,
            class_iri,
            class_label,
            now,
            now,
        ],
    );

    result.ok().map(|_| job_id)
}

/// Find all (aggregation_property_iri, source_prop_iri) pairs where the aggregation formula
/// references `subprop_iri` as its sub-property. Used to trigger reverse cache invalidation
/// when a sub-property value changes on a related instance.
fn find_aggregation_props_by_subprop(
    conn: &Connection,
    subprop_iri: &str,
) -> Vec<(String, String)> {
    conn.prepare(
        "SELECT subject, object_value FROM triples \
         WHERE predicate = 'foundation:aggregation' AND retracted = 0",
    )
    .map(|mut stmt| {
        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map(|rows| {
            rows.filter_map(|r| r.ok())
                .filter_map(|(prop_iri, formula)| {
                    crate::owl::aggregation::parse_aggregation_call(formula.trim())
                        .ok()
                        .filter(|call| call.sub_prop.as_deref() == Some(subprop_iri))
                        .map(|call| (prop_iri, call.source_prop))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    })
    .unwrap_or_default()
}

/// Create recalc jobs for all parent instances that aggregate a sub-property that just changed.
///
/// When `changed_subprop_iri` changes on `changed_instance_iri`, this function finds all
/// aggregation properties whose sub-property is `changed_subprop_iri`, then for each one
/// locates parent instances that link to `changed_instance_iri` via the source ObjectProperty,
/// and creates recalc jobs for those parents.
pub fn create_reverse_aggregation_recalc_jobs(
    conn: &Connection,
    changed_instance_iri: &str,
    changed_subprop_iri: &str,
) -> Vec<String> {
    let agg_props = find_aggregation_props_by_subprop(conn, changed_subprop_iri);
    let now = now_millis();
    let mut job_ids = Vec::new();

    for (agg_prop_iri, source_prop_iri) in agg_props {
        // Determine traversal direction from the formula's OWNER class, not the changed
        // instance type. When source_prop has the same class on both sides (e.g.
        // previousMonthBudget: MonthlyBudget→MonthlyBudget), comparing domain to the
        // instance type is ambiguous and picks the wrong direction.
        //
        // - If the aggregation owner sits at source_prop.domain, the formula walks
        //   Owner --source_prop--> Target (forward). The changed instance is the Target,
        //   so we find Owners by reverse lookup (WHERE object = changed_instance).
        // - Otherwise the formula uses inverse navigation (Target --source_prop--> Owner,
        //   e.g. Obligation aggregating SOMA({{settles}}.amount) where Payment.settles
        //   points TO Obligation). The changed instance is the source, so we find Owners
        //   by forward lookup (WHERE subject = changed_instance).
        let source_domain = fetch_domain(conn, &source_prop_iri);
        let agg_owner_class = fetch_domain(conn, &agg_prop_iri);
        let owner_at_source_domain = !source_domain.is_empty()
            && !agg_owner_class.is_empty()
            && agg_owner_class == source_domain;
        let prop_from_instance = !owner_at_source_domain;

        // The direction above is only trustworthy when BOTH domains are declared.
        // With either missing, the comparison degrades to "not equal" and always
        // picks the same branch — so an undeclared ontology silently produces no
        // job at all, and the aggregate never updates.
        let direction_is_grounded = !source_domain.is_empty() && !agg_owner_class.is_empty();

        let lookup_forward = |conn: &Connection| -> Vec<String> {
            conn.prepare(
                "SELECT DISTINCT object FROM triples \
                 WHERE subject = ? AND predicate = ? AND retracted = 0 AND object IS NOT NULL",
            )
            .map(|mut stmt| {
                stmt.query_map(
                    rusqlite::params![changed_instance_iri, source_prop_iri],
                    |row| row.get::<_, String>(0),
                )
                .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                .unwrap_or_default()
            })
            .unwrap_or_default()
        };
        let lookup_reverse = |conn: &Connection| -> Vec<String> {
            conn.prepare(
                "SELECT DISTINCT subject FROM triples \
                 WHERE predicate = ? AND object = ? AND retracted = 0",
            )
            .map(|mut stmt| {
                stmt.query_map(
                    rusqlite::params![source_prop_iri, changed_instance_iri],
                    |row| row.get::<_, String>(0),
                )
                .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                .unwrap_or_default()
            })
            .unwrap_or_default()
        };

        let mut parent_iris: Vec<String> = if prop_from_instance {
            lookup_forward(conn)
        } else {
            lookup_reverse(conn)
        };

        // Ungrounded guess that found nothing: the link exists in the other
        // direction. Try it rather than dropping the recalculation on the floor.
        if parent_iris.is_empty() && !direction_is_grounded {
            parent_iris = if prop_from_instance {
                lookup_reverse(conn)
            } else {
                lookup_forward(conn)
            };
        }

        let property_label = fetch_label(conn, &agg_prop_iri);
        let class_iri = fetch_domain(conn, &agg_prop_iri);
        let class_label = if class_iri.is_empty() {
            String::new()
        } else {
            fetch_label(conn, &class_iri)
        };

        for parent_iri in &parent_iris {
            let job_id = format!(
                "job_{}_{}_{}",
                now,
                agg_prop_iri.replace(':', "_"),
                parent_iri.replace(':', "_")
            );
            let ok = conn
                .execute(
                    "INSERT INTO formula_recalc_jobs
                 (id, property_iri, property_label, class_iri, class_label,
                  instance_iri, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
                    rusqlite::params![
                        job_id,
                        agg_prop_iri,
                        property_label,
                        class_iri,
                        class_label,
                        parent_iri,
                        now,
                        now,
                    ],
                )
                .is_ok();

            if ok {
                job_ids.push(job_id);
            }
        }
    }

    job_ids
}

fn entity_exists(conn: &Connection, iri: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM triples_current WHERE subject = ? AND predicate = 'rdf:type' LIMIT 1",
        rusqlite::params![iri],
        |_| Ok(()),
    )
    .is_ok()
}

/// Queues a whole-class recompute for every property whose formula reads the clock.
///
/// These values go stale on their own: nothing writes to the instance, but "days
/// overdue" is wrong the moment the date rolls over. Every other calculated value
/// is refreshed by the write that invalidated it — these are the only ones with no
/// write to react to, so they need a clock of their own.
pub fn create_clock_dependent_recalc_jobs(conn: &Connection) -> Vec<String> {
    let mut job_ids = Vec::new();

    let calculated: Vec<(String, String)> = match conn.prepare(
        "SELECT subject, object_value FROM triples_current \
         WHERE predicate = 'foundation:formula' AND object_value IS NOT NULL",
    ) {
        Ok(mut stmt) => stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
        Err(_) => return job_ids,
    };

    let now = now_millis();
    for (prop_iri, formula) in calculated {
        if !crate::owl::expr::depends_on_clock(&formula) {
            continue;
        }
        let class_iri = fetch_domain(conn, &prop_iri);
        if class_iri.is_empty() {
            continue;
        }
        // instance_iri stays NULL: this is the whole-class recompute path, which
        // pages through instances instead of loading them all at once.
        let job_id = format!("job_clock_{}_{}", now, prop_iri.replace(':', "_"));
        let ok = conn.execute(
            "INSERT INTO formula_recalc_jobs
             (id, property_iri, property_label, class_iri, class_label, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'pending', ?, ?)",
            rusqlite::params![
                job_id, prop_iri, fetch_label(conn, &prop_iri),
                class_iri, fetch_label(conn, &class_iri), now, now,
            ],
        ).is_ok();
        if ok {
            job_ids.push(job_id);
        }
    }

    job_ids
}

/// Refreshes clock-dependent values once at startup and once a day after that.
pub fn spawn_daily_clock_refresh(app: tauri::AppHandle, executor: crate::owl::DbExecutor) {
    tauri::async_runtime::spawn(async move {
        loop {
            let job_ids: Vec<String> = executor
                .write(|conn| Ok(create_clock_dependent_recalc_jobs(conn).join(",")))
                .await
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();

            if !job_ids.is_empty() {
                if let Some(worker) = app.try_state::<FormulaWorker>() {
                    for job_id in job_ids {
                        let _ = worker.sender.send(WorkerCommand::Enqueue { job_id }).await;
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}

/// Every calculated property declared on an instance's own class, queued for
/// recompute against that instance.
///
/// The reference-driven path only fires when someone names the property that
/// changed, which misses the case the automation engine needs: a node writes to
/// the control instance and the NEXT node has to read totals derived from that
/// write. Here the trigger is the class, not the write — so the engine can settle
/// every derived value it owns before advancing, without knowing what changed.
pub fn create_class_calculated_recalc_jobs(conn: &Connection, instance_iri: &str) -> Vec<String> {
    let mut job_ids = Vec::new();

    let types: Vec<String> = match conn
        .prepare("SELECT object FROM triples_current WHERE subject = ? AND predicate = 'rdf:type'")
    {
        Ok(mut stmt) => stmt
            .query_map(rusqlite::params![instance_iri], |row| {
                row.get::<_, String>(0)
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
        Err(_) => return job_ids,
    };
    if types.is_empty() {
        return job_ids;
    }

    let now = now_millis();
    for class_iri in types {
        let props: Vec<(String, String)> = match conn.prepare(
            "SELECT DISTINCT calc.subject, calc.predicate FROM triples_current calc \
             JOIN triples_current dom \
               ON dom.subject = calc.subject \
              AND dom.predicate = 'rdfs:domain' \
              AND dom.object = ? \
             WHERE calc.predicate IN ('foundation:formula', 'foundation:aggregation')",
        ) {
            Ok(mut stmt) => stmt
                .query_map(rusqlite::params![class_iri], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => continue,
        };

        for (prop_iri, _) in props {
            let property_label = fetch_label(conn, &prop_iri);
            let class_label = fetch_label(conn, &class_iri);
            let job_id = format!(
                "job_{}_{}_{}",
                now,
                prop_iri.replace(':', "_"),
                instance_iri.replace(':', "_"),
            );
            let ok = conn
                .execute(
                    "INSERT INTO formula_recalc_jobs
                 (id, property_iri, property_label, class_iri, class_label,
                  instance_iri, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
                    rusqlite::params![
                        job_id,
                        prop_iri,
                        property_label,
                        class_iri,
                        class_label,
                        instance_iri,
                        now,
                        now,
                    ],
                )
                .is_ok();
            if ok {
                job_ids.push(job_id);
            }
        }
    }

    job_ids
}

pub fn create_instance_recalc_jobs(
    conn: &Connection,
    instance_iri: &str,
    changed_property_iri: &str,
) -> Vec<String> {
    let placeholder = format!("{{{{{}}}}}", changed_property_iri);
    let formula_rows = query_formula_properties_by_reference(conn, &placeholder);

    let now = now_millis();
    let mut job_ids = Vec::new();

    let instance_type: Option<String> = conn
        .query_row(
            "SELECT object FROM triples \
         WHERE subject = ? AND predicate = 'rdf:type' AND retracted = 0 \
         ORDER BY tx DESC LIMIT 1",
            rusqlite::params![instance_iri],
            |row| row.get(0),
        )
        .ok();

    for (formula_prop_iri, _, pred) in formula_rows {
        let target_iri: String = if pred == "foundation:aggregation" {
            // An aggregate lives on ONE side of a link and the write happens on the
            // other. Getting the side wrong recalculates an instance nobody asked
            // about and leaves the real total stale.
            let formula_domain = fetch_domain(conn, &formula_prop_iri);
            let changed_domain = fetch_domain(conn, changed_property_iri);
            let is_inverse = match (&instance_type, formula_domain.as_str()) {
                // The aggregate is declared on a different class than the instance
                // that changed: the instance sits on the far side of the link.
                (Some(t), d) if !d.is_empty() => t.as_str() != d,
                // The aggregate's own domain is undeclared, so it can't name the
                // side. Fall back to the link being aggregated: when the changed
                // property is declared on the changed instance's class, this
                // instance is the SOURCE and the aggregate lives on its target.
                (Some(t), _) => !changed_domain.is_empty() && changed_domain == *t,
                _ => false,
            };
            if is_inverse {
                match conn
                    .query_row(
                        "SELECT object FROM triples \
                     WHERE subject = ? AND predicate = ? AND retracted = 0 \
                     ORDER BY tx DESC LIMIT 1",
                        rusqlite::params![instance_iri, changed_property_iri],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
                {
                    // Only hop when the link actually landed on an entity. An
                    // aggregation over a literal-valued property has no far side,
                    // and a literal treated as an IRI would target a job at nothing.
                    Some(parent) if entity_exists(conn, &parent) => parent,
                    _ => continue,
                }
            } else {
                instance_iri.to_string()
            }
        } else {
            instance_iri.to_string()
        };

        let property_label = fetch_label(conn, &formula_prop_iri);
        let class_iri = fetch_domain(conn, &formula_prop_iri);
        let class_label = if class_iri.is_empty() {
            String::new()
        } else {
            fetch_label(conn, &class_iri)
        };

        let job_id = format!(
            "job_{}_{}_{}",
            now,
            formula_prop_iri.replace(':', "_"),
            target_iri.replace(':', "_"),
        );
        let ok = conn
            .execute(
                "INSERT INTO formula_recalc_jobs
             (id, property_iri, property_label, class_iri, class_label,
              instance_iri, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
                rusqlite::params![
                    job_id,
                    formula_prop_iri,
                    property_label,
                    class_iri,
                    class_label,
                    target_iri,
                    now,
                    now,
                ],
            )
            .is_ok();

        if ok {
            job_ids.push(job_id);
        }
    }

    job_ids.extend(create_reverse_aggregation_recalc_jobs(
        conn,
        instance_iri,
        changed_property_iri,
    ));

    job_ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE formula_recalc_jobs (
                id              TEXT    PRIMARY KEY,
                property_iri    TEXT    NOT NULL,
                property_label  TEXT,
                class_iri       TEXT    NOT NULL,
                class_label     TEXT,
                status          TEXT    NOT NULL DEFAULT 'pending',
                total           INTEGER NOT NULL DEFAULT 0,
                processed       INTEGER NOT NULL DEFAULT 0,
                last_offset     INTEGER NOT NULL DEFAULT 0,
                error_message   TEXT,
                created_at      INTEGER NOT NULL,
                updated_at      INTEGER NOT NULL,
                instance_iri    TEXT
            );
            CREATE TABLE formula_instance_errors (
                instance_iri    TEXT NOT NULL,
                property_iri    TEXT NOT NULL,
                error_message   TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                PRIMARY KEY (instance_iri, property_iri)
            );
            CREATE TABLE transactions (
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
            WHERE is_current = 1 AND retracted = 0;
        ").unwrap();
        conn
    }

    fn insert_job(conn: &Connection, id: &str, property_iri: &str, class_iri: &str, status: &str) {
        conn.execute(
            "INSERT INTO formula_recalc_jobs
             (id, property_iri, property_label, class_iri, class_label, status, created_at, updated_at)
             VALUES (?, ?, '', ?, '', ?, 0, 0)",
            rusqlite::params![id, property_iri, class_iri, status],
        ).unwrap();
    }

    #[test]
    fn test_load_job_returns_none_when_not_found() {
        let conn = setup_test_db();
        assert!(load_job(&conn, "nonexistent-job").is_none());
    }

    #[test]
    fn test_load_job_returns_none_for_non_pending_status() {
        let conn = setup_test_db();
        insert_job(
            &conn,
            "job1",
            "foundation:MyProp",
            "foundation:Task",
            "running",
        );

        let result = load_job(&conn, "job1");
        assert!(result.is_none());
    }

    #[test]
    fn test_load_job_returns_record_for_pending_status() {
        let conn = setup_test_db();
        insert_job(
            &conn,
            "job1",
            "foundation:MyProp",
            "foundation:Task",
            "pending",
        );

        let result = load_job(&conn, "job1");
        assert!(result.is_some());
        let record = result.unwrap();
        assert_eq!(record.property_iri, "foundation:MyProp");
        assert_eq!(record.class_iri, "foundation:Task");
        assert_eq!(record.processed, 0);
        assert_eq!(record.last_offset, 0);
    }

    #[test]
    fn test_load_job_returns_none_for_cancelled_status() {
        let conn = setup_test_db();
        insert_job(
            &conn,
            "job1",
            "foundation:MyProp",
            "foundation:Task",
            "cancelled",
        );

        assert!(load_job(&conn, "job1").is_none());
    }

    #[test]
    fn test_load_job_returns_none_for_completed_status() {
        let conn = setup_test_db();
        insert_job(
            &conn,
            "job1",
            "foundation:MyProp",
            "foundation:Task",
            "completed",
        );

        assert!(load_job(&conn, "job1").is_none());
    }

    #[test]
    fn test_store_calculated_value_inserts_new_triple() {
        let mut conn = setup_test_db();
        store_calculated_value(&mut conn, "foundation:Instance1", "foundation:score", "42");

        let value: String = conn.query_row(
            "SELECT object_value FROM triples WHERE subject = ? AND predicate = ?
             AND retracted = 0 AND tx = (SELECT MAX(tx) FROM triples WHERE subject = ? AND predicate = ?)",
            rusqlite::params!["foundation:Instance1", "foundation:score", "foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(value, "42");
    }

    #[test]
    fn test_store_calculated_value_replaces_existing() {
        let mut conn = setup_test_db();
        // Insert a prior transaction so the transactions autoincrement starts at 1,
        // and the old triple uses tx=1; the next assert_triples will create tx=2 and win.
        // Values are numeric because store_calculated_value writes as xsd:decimal,
        // which requires a value parseable as f64.
        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, origin_id, tx, created_at, retracted)
             VALUES ('foundation:Instance1', 'foundation:score', '10', 'literal', 1, 1, 0, 0)",
            [],
        ).unwrap();

        store_calculated_value(&mut conn, "foundation:Instance1", "foundation:score", "20");

        let active: String = conn.query_row(
            "SELECT object_value FROM triples WHERE subject = ? AND predicate = ?
             AND retracted = 0 AND tx = (SELECT MAX(tx) FROM triples WHERE subject = ? AND predicate = ?)",
            rusqlite::params!["foundation:Instance1", "foundation:score", "foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(active, "20");
    }

    #[test]
    fn test_store_calculated_value_uses_xsd_decimal_by_default() {
        // Regression: Bug_1777120091054 — previously the datatype was hardcoded as xsd:string
        // even for properties with a numeric range in the schema.
        let mut conn = setup_test_db();
        store_calculated_value(
            &mut conn,
            "foundation:Instance1",
            "foundation:score",
            "42.5",
        );

        let datatype: String = conn.query_row(
            "SELECT object_datatype FROM triples WHERE subject = ? AND predicate = ?
             AND retracted = 0 AND tx = (SELECT MAX(tx) FROM triples WHERE subject = ? AND predicate = ?)",
            rusqlite::params!["foundation:Instance1", "foundation:score", "foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(datatype, "xsd:decimal");
    }

    #[test]
    fn test_store_calculated_value_uses_xsd_integer_when_range_is_integer() {
        // Regression: Bug_1777120091054 — properties with range xsd:integer must
        // be stored as xsd:integer, not as xsd:string.
        let mut conn = setup_test_db();
        // Install the range of foundation:itemCount via assert_triples so the schema
        // query sees the triple (going through the triples_current view correctly).
        use crate::eavto::{store, Object, Triple};
        let range_triple = Triple::new(
            "foundation:itemCount",
            "rdfs:range",
            Object::Iri("xsd:integer".to_string()),
        );
        store::assert_triples(&mut conn, &[range_triple], "test").unwrap();

        store_calculated_value(
            &mut conn,
            "foundation:Instance1",
            "foundation:itemCount",
            "7",
        );

        let datatype: String = conn.query_row(
            "SELECT object_datatype FROM triples WHERE subject = ? AND predicate = ?
             AND retracted = 0 AND tx = (SELECT MAX(tx) FROM triples WHERE subject = ? AND predicate = ?)",
            rusqlite::params!["foundation:Instance1", "foundation:itemCount", "foundation:Instance1", "foundation:itemCount"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(datatype, "xsd:integer");
    }

    #[test]
    fn test_store_calculated_value_clears_formula_errors() {
        let mut conn = setup_test_db();
        conn.execute(
            "INSERT INTO formula_instance_errors (instance_iri, property_iri, error_message, created_at)
             VALUES ('foundation:Instance1', 'foundation:score', 'previous error', 0)",
            [],
        ).unwrap();

        store_calculated_value(&mut conn, "foundation:Instance1", "foundation:score", "42");

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM formula_instance_errors WHERE instance_iri = ? AND property_iri = ?",
            rusqlite::params!["foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_now_millis_is_positive() {
        assert!(now_millis() > 0);
    }

    #[test]
    fn test_now_millis_increases_over_time() {
        let t1 = now_millis();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let t2 = now_millis();
        assert!(t2 >= t1);
    }

    #[test]
    fn test_create_reverse_aggregation_recalc_jobs_creates_job_for_parent() {
        let conn = setup_test_db();

        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, origin_id, tx, created_at, retracted) \
             VALUES ('p:total', 'foundation:aggregation', 'SOMA({{p:hasItem}}.p:subVal)', 'literal', 1, 1, 0, 0)",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, origin_id, tx, created_at, retracted) \
             VALUES ('inst:parent', 'p:hasItem', 'inst:child1', 'iri', 1, 1, 0, 0)",
            [],
        ).unwrap();

        let job_ids = create_reverse_aggregation_recalc_jobs(&conn, "inst:child1", "p:subVal");
        assert!(
            !job_ids.is_empty(),
            "expected at least one recalc job for parent"
        );

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM formula_recalc_jobs WHERE instance_iri = 'inst:parent' AND property_iri = 'p:total'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1, "expected one job for inst:parent / p:total");
    }

    fn triple(
        conn: &Connection,
        subject: &str,
        predicate: &str,
        object: Option<&str>,
        value: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_value, object_type) \
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![
                subject,
                predicate,
                object,
                value,
                if object.is_some() { "iri" } else { "literal" },
            ],
        )
        .unwrap();
    }

    #[test]
    fn class_discovery_queues_every_calculated_property_of_the_instance_class() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:Run_1",
            "rdf:type",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:total",
            "rdfs:domain",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:total",
            "foundation:formula",
            None,
            Some("{{foundation:a}} + {{foundation:b}}"),
        );
        triple(
            &conn,
            "foundation:seen",
            "rdfs:domain",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:seen",
            "foundation:aggregation",
            None,
            Some("CONTAR({{foundation:items}})"),
        );

        let jobs = create_class_calculated_recalc_jobs(&conn, "foundation:Run_1");

        assert_eq!(
            jobs.len(),
            2,
            "both the formula and the aggregation must be queued"
        );
        let targeted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM formula_recalc_jobs WHERE instance_iri = ?",
                rusqlite::params!["foundation:Run_1"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            targeted, 2,
            "jobs must target the instance that was passed in"
        );
    }

    #[test]
    fn class_discovery_ignores_calculated_properties_of_other_classes() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:Run_1",
            "rdf:type",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:otherTotal",
            "rdfs:domain",
            Some("foundation:SomethingElse"),
            None,
        );
        triple(
            &conn,
            "foundation:otherTotal",
            "foundation:formula",
            None,
            Some("{{foundation:x}} * 2"),
        );

        assert!(create_class_calculated_recalc_jobs(&conn, "foundation:Run_1").is_empty());
    }

    #[test]
    fn class_discovery_is_empty_for_an_untyped_instance() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:total",
            "rdfs:domain",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:total",
            "foundation:formula",
            None,
            Some("{{foundation:a}}"),
        );

        assert!(create_class_calculated_recalc_jobs(&conn, "foundation:Nothing").is_empty());
    }

    #[test]
    fn class_discovery_ignores_plain_properties() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:Run_1",
            "rdf:type",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:label",
            "rdfs:domain",
            Some("foundation:Run"),
            None,
        );
        triple(
            &conn,
            "foundation:label",
            "rdfs:range",
            Some("xsd:string"),
            None,
        );

        assert!(
            create_class_calculated_recalc_jobs(&conn, "foundation:Run_1").is_empty(),
            "a property with no formula or aggregation is not calculated"
        );
    }

    #[test]
    fn clock_dependent_formulas_get_a_whole_class_refresh_job() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:daysOverdue",
            "rdfs:domain",
            Some("foundation:Task"),
            None,
        );
        triple(
            &conn,
            "foundation:daysOverdue",
            "foundation:formula",
            None,
            Some("DAYS_SINCE({{foundation:dueDate}})"),
        );

        let jobs = create_clock_dependent_recalc_jobs(&conn);

        assert_eq!(jobs.len(), 1);
        let instance: Option<String> = conn
            .query_row(
                "SELECT instance_iri FROM formula_recalc_jobs WHERE property_iri = ?",
                rusqlite::params!["foundation:daysOverdue"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            instance.is_none(),
            "a clock refresh covers the whole class, not one instance"
        );
    }

    #[test]
    fn a_formula_that_does_not_read_the_clock_is_left_alone() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:total",
            "rdfs:domain",
            Some("foundation:Task"),
            None,
        );
        triple(
            &conn,
            "foundation:total",
            "foundation:formula",
            None,
            Some("{{foundation:a}} + {{foundation:b}}"),
        );

        assert!(
            create_clock_dependent_recalc_jobs(&conn).is_empty(),
            "refreshing a value nothing changed is wasted work on every instance of the class"
        );
    }

    #[test]
    fn a_clock_formula_with_no_declared_domain_is_skipped_rather_than_guessed() {
        let conn = setup_test_db();
        triple(
            &conn,
            "foundation:orphan",
            "foundation:formula",
            None,
            Some("TODAY()"),
        );

        assert!(create_clock_dependent_recalc_jobs(&conn).is_empty());
    }
}
