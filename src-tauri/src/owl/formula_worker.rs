use rusqlite::Connection;
use serde::Serialize;
use tauri::Emitter;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum WorkerCommand {
    Enqueue { job_id: String },
    Cancel  { property_iri: String },
}

pub struct FormulaWorker {
    pub sender: mpsc::Sender<WorkerCommand>,
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

        FormulaWorker { sender: tx }
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

async fn process_job(app: &tauri::AppHandle, executor: &crate::owl::DbExecutor, job_id: &str, sender: &mpsc::Sender<WorkerCommand>) {
    let job_id_owned = job_id.to_string();
    let job = match executor.read(move |conn| {
        Ok(load_job(conn, &job_id_owned))
    }).await {
        Ok(Some(j)) => j,
        _ => return,
    };

    let job_id_owned = job_id.to_string();
    if executor.write(move |conn| {
        conn.execute(
            "UPDATE formula_recalc_jobs SET status = 'running', updated_at = ? WHERE id = ?",
            rusqlite::params![now_millis(), job_id_owned],
        ).map(|_| String::new()).map_err(|e| e.to_string())
    }).await.is_err() {
        return;
    }

    let prop_iri = job.property_iri.clone();
    let class_iri = if job.class_iri.is_empty() {
        let prop = prop_iri.clone();
        executor.read(move |conn| Ok(fetch_domain(conn, &prop))).await.unwrap_or_default()
    } else {
        job.class_iri.clone()
    };

    let instance_iris: Vec<String> = if let Some(ref inst) = job.instance_iri {
        vec![inst.clone()]
    } else {
        let ci = class_iri.clone();
        let offset = job.last_offset;
        executor.read(move |conn| Ok(query_class_instances(conn, &ci, offset))).await.unwrap_or_default()
    };

    let total: i64 = if job.instance_iri.is_some() {
        1
    } else {
        let ci = class_iri.clone();
        executor.read(move |conn| {
            conn.query_row(
                "SELECT COUNT(DISTINCT subject) FROM triples \
                 WHERE predicate = 'rdf:type' AND object = ? AND retracted = 0",
                rusqlite::params![ci],
                |row| row.get(0),
            ).map_err(|e| e.to_string())
        }).await.unwrap_or(0)
    };

    let job_id_owned = job_id.to_string();
    let _ = executor.write(move |conn| {
        conn.execute(
            "UPDATE formula_recalc_jobs SET total = ?, updated_at = ? WHERE id = ?",
            rusqlite::params![total, now_millis(), job_id_owned],
        ).map(|_| String::new()).map_err(|e| e.to_string())
    }).await;

    let mut processed = job.processed;
    let mut last_offset = job.last_offset;
    let mut last_pct: u32 = (processed * 100 / total.max(1)) as u32;
    let mut cancelled = false;

    for instance_iri in &instance_iris {
        let instance = instance_iri.clone();
        let prop = job.property_iri.clone();
        let job_id_owned = job_id.to_string();

        let (current_status, eval_result) = match executor.read(move |conn| {
            let status: String = conn.query_row(
                "SELECT status FROM formula_recalc_jobs WHERE id = ?",
                rusqlite::params![job_id_owned],
                |row| row.get(0),
            ).map_err(|e| e.to_string())?;
            let eval = evaluate_formula_for_instance(conn, &instance, &prop);
            Ok((status, eval))
        }).await {
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
                let dependent_jobs = executor.write(move |conn| {
                    store_calculated_value(conn, &inst, &prop, &value);
                    let jobs = create_instance_recalc_jobs(conn, &inst, &prop);
                    Ok(jobs.join(","))
                }).await.unwrap_or_default();
                for jid in dependent_jobs.split(',').filter(|s| !s.is_empty()) {
                    let _ = sender.try_send(WorkerCommand::Enqueue { job_id: jid.to_string() });
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

    let final_status = if cancelled { "cancelled" } else { "completed" };
    let job_id_owned = job_id.to_string();
    let fs = final_status.to_string();
    let _ = executor.write(move |conn| {
        conn.execute(
            "UPDATE formula_recalc_jobs SET status = ?, updated_at = ? WHERE id = ?",
            rusqlite::params![fs, now_millis(), job_id_owned],
        ).map(|_| String::new()).map_err(|e| e.to_string())
    }).await;

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
    .and_then(|(property_iri, property_label, class_iri, class_label,
                processed, last_offset, status, instance_iri)| {
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
    })
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
    let mode: Option<String> = conn.query_row(
        "SELECT predicate FROM triples \
         WHERE subject = ? AND predicate IN ('foundation:formula', 'foundation:aggregation') \
         AND retracted = 0 \
         ORDER BY tx DESC LIMIT 1",
        rusqlite::params![property_iri],
        |row| row.get(0),
    ).ok();

    match mode.as_deref() {
        Some("foundation:aggregation") =>
            crate::owl::aggregation::evaluate_aggregation_for_instance(conn, instance_iri, property_iri),
        Some("foundation:formula") =>
            crate::owl::formula::evaluate_formula_for_instance_raw(conn, instance_iri, property_iri),
        _ => Err(format!("Property '{}' has no formula or aggregation", property_iri)),
    }
}

fn store_calculated_value(
    conn: &mut Connection,
    instance_iri: &str,
    property_iri: &str,
    value: &str,
) {
    use crate::eavto::{store, Triple, Object};
    let datatype = resolve_calculated_datatype(conn, property_iri);
    let triple = Triple::new(
        instance_iri,
        property_iri,
        Object::Literal { value: value.to_string(), datatype: Some(datatype), language: None },
    );
    let _ = store::assert_triples(conn, &[triple], "formula");
    let _ = conn.execute(
        "DELETE FROM formula_instance_errors WHERE instance_iri = ? AND property_iri = ?",
        rusqlite::params![instance_iri, property_iri],
    );
}

/// Decides the `object_datatype` to store for a calculated value, consulting the
/// property's `rdfs:range`. Formula/aggregation results are always numeric:
/// integer ranges become `xsd:integer`, all other ranges (xsd:decimal,
/// xsd:float, xsd:double, no range, etc.) become `xsd:decimal` — so downstream
/// treats the literal as a number and formats it correctly.
fn resolve_calculated_datatype(conn: &Connection, property_iri: &str) -> String {
    const INTEGER_RANGES: &[&str] = &[
        "xsd:integer", "xsd:int", "xsd:long", "xsd:short", "xsd:byte",
        "xsd:nonNegativeInteger", "xsd:positiveInteger",
    ];
    let range: Option<String> = crate::eavto::query::get_by_entity_predicate(conn, property_iri, "rdfs:range")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_iri().map(|s| s.to_string()));
    match range.as_deref() {
        Some(r) if INTEGER_RANGES.contains(&r) => "xsd:integer".to_string(),
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
        stmt.query_map(rusqlite::params![class_iri, offset], |row| row.get::<_, String>(0))
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
        stmt.query_map([], |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        )))
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

pub fn cancel_and_create_property_job(
    conn: &Connection,
    property_iri: &str,
) -> Option<String> {
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
            job_id, property_iri, property_label, class_iri, class_label, now, now,
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
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
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

        let parent_iris: Vec<String> = if prop_from_instance {
            conn.prepare(
                "SELECT DISTINCT object FROM triples \
                 WHERE subject = ? AND predicate = ? AND retracted = 0 AND object IS NOT NULL",
            )
            .map(|mut stmt| {
                stmt.query_map(rusqlite::params![changed_instance_iri, source_prop_iri], |row| {
                    row.get::<_, String>(0)
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                .unwrap_or_default()
            })
            .unwrap_or_default()
        } else {
            conn.prepare(
                "SELECT DISTINCT subject FROM triples \
                 WHERE predicate = ? AND object = ? AND retracted = 0",
            )
            .map(|mut stmt| {
                stmt.query_map(rusqlite::params![source_prop_iri, changed_instance_iri], |row| {
                    row.get::<_, String>(0)
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                .unwrap_or_default()
            })
            .unwrap_or_default()
        };

        let property_label = fetch_label(conn, &agg_prop_iri);
        let class_iri = fetch_domain(conn, &agg_prop_iri);
        let class_label = if class_iri.is_empty() {
            String::new()
        } else {
            fetch_label(conn, &class_iri)
        };

        for parent_iri in &parent_iris {
            let job_id = format!(
                "job_{}_{}_{}", now,
                agg_prop_iri.replace(':', "_"),
                parent_iri.replace(':', "_")
            );
            let ok = conn.execute(
                "INSERT INTO formula_recalc_jobs
                 (id, property_iri, property_label, class_iri, class_label,
                  instance_iri, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
                rusqlite::params![
                    job_id, agg_prop_iri, property_label, class_iri, class_label,
                    parent_iri, now, now,
                ],
            ).is_ok();

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

    let instance_type: Option<String> = conn.query_row(
        "SELECT object FROM triples \
         WHERE subject = ? AND predicate = 'rdf:type' AND retracted = 0 \
         ORDER BY tx DESC LIMIT 1",
        rusqlite::params![instance_iri],
        |row| row.get(0),
    ).ok();

    for (formula_prop_iri, _, pred) in formula_rows {
        let target_iri: String = if pred == "foundation:aggregation" {
            let domain = fetch_domain(conn, &formula_prop_iri);
            let is_inverse = match (&instance_type, domain.as_str()) {
                (Some(t), d) if !d.is_empty() => t.as_str() != d,
                _ => false,
            };
            if is_inverse {
                match conn.query_row(
                    "SELECT object FROM triples \
                     WHERE subject = ? AND predicate = ? AND retracted = 0 \
                     ORDER BY tx DESC LIMIT 1",
                    rusqlite::params![instance_iri, changed_property_iri],
                    |row| row.get::<_, String>(0),
                ).ok() {
                    Some(parent) => parent,
                    None => continue,
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
            "job_{}_{}_{}", now,
            formula_prop_iri.replace(':', "_"),
            target_iri.replace(':', "_"),
        );
        let ok = conn.execute(
            "INSERT INTO formula_recalc_jobs
             (id, property_iri, property_label, class_iri, class_label,
              instance_iri, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
            rusqlite::params![
                job_id, formula_prop_iri, property_label, class_iri, class_label,
                target_iri, now, now,
            ],
        ).is_ok();

        if ok {
            job_ids.push(job_id);
        }
    }

    job_ids.extend(create_reverse_aggregation_recalc_jobs(
        conn, instance_iri, changed_property_iri,
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
                retracted INTEGER NOT NULL DEFAULT 0
            );
            CREATE VIEW IF NOT EXISTS triples_current AS
            SELECT subject, predicate, object, object_value, object_datatype, object_language,
                   object_number, object_integer, object_boolean, tx, origin_id, object_type, created_at
            FROM triples t
            WHERE t.retracted = 0
              AND t.tx = (
                  SELECT MAX(tx) FROM triples
                  WHERE subject = t.subject AND predicate = t.predicate
              );
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
        insert_job(&conn, "job1", "foundation:MyProp", "foundation:Task", "running");

        let result = load_job(&conn, "job1");
        assert!(result.is_none());
    }

    #[test]
    fn test_load_job_returns_record_for_pending_status() {
        let conn = setup_test_db();
        insert_job(&conn, "job1", "foundation:MyProp", "foundation:Task", "pending");

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
        insert_job(&conn, "job1", "foundation:MyProp", "foundation:Task", "cancelled");

        assert!(load_job(&conn, "job1").is_none());
    }

    #[test]
    fn test_load_job_returns_none_for_completed_status() {
        let conn = setup_test_db();
        insert_job(&conn, "job1", "foundation:MyProp", "foundation:Task", "completed");

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
        conn.execute("INSERT INTO transactions (origin, created_at) VALUES ('test', 0)", []).unwrap();
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
        store_calculated_value(&mut conn, "foundation:Instance1", "foundation:score", "42.5");

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
        use crate::eavto::{store, Triple, Object};
        let range_triple = Triple::new(
            "foundation:itemCount",
            "rdfs:range",
            Object::Iri("xsd:integer".to_string()),
        );
        store::assert_triples(&mut conn, &[range_triple], "test").unwrap();

        store_calculated_value(&mut conn, "foundation:Instance1", "foundation:itemCount", "7");

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
        assert!(!job_ids.is_empty(), "expected at least one recalc job for parent");

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM formula_recalc_jobs WHERE instance_iri = 'inst:parent' AND property_iri = 'p:total'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1, "expected one job for inst:parent / p:total");
    }
}
