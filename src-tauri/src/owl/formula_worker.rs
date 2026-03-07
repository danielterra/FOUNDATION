use std::path::PathBuf;
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
    pub fn spawn(app: tauri::AppHandle, db_path: PathBuf) -> Self {
        let (tx, mut rx) = mpsc::channel::<WorkerCommand>(64);

        tauri::async_runtime::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    WorkerCommand::Enqueue { job_id } => {
                        process_job(&app, &db_path, &job_id).await;
                    }
                    WorkerCommand::Cancel { property_iri } => {
                        cancel_jobs_for_property(&db_path, &property_iri);
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
}

async fn process_job(app: &tauri::AppHandle, db_path: &PathBuf, job_id: &str) {
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = conn.execute_batch("PRAGMA busy_timeout=5000;");

    let job = match load_job(&conn, job_id) {
        Some(j) => j,
        None => return,
    };

    if conn.execute(
        "UPDATE formula_recalc_jobs SET status = 'running', updated_at = ? WHERE id = ?",
        rusqlite::params![now_millis(), job_id],
    ).is_err() {
        return;
    }

    let total: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT subject) FROM triples WHERE predicate = 'rdf:type' AND object = ? AND retracted = 0",
        rusqlite::params![job.class_iri],
        |row| row.get(0),
    ).unwrap_or(0);

    let _ = conn.execute(
        "UPDATE formula_recalc_jobs SET total = ?, updated_at = ? WHERE id = ?",
        rusqlite::params![total, now_millis(), job_id],
    );

    let mut stmt = match conn.prepare(
        "SELECT DISTINCT subject FROM triples
         WHERE predicate = 'rdf:type' AND object = ? AND retracted = 0
         ORDER BY subject
         LIMIT -1 OFFSET ?"
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let instance_iris: Vec<String> = match stmt.query_map(
        rusqlite::params![job.class_iri, job.last_offset],
        |row| row.get(0),
    ) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => return,
    };

    let mut processed = job.processed;
    let mut last_offset = job.last_offset;
    let mut last_pct: u32 = (processed * 100 / total.max(1)) as u32;
    let mut cancelled = false;

    for instance_iri in &instance_iris {
        let current_status: String = match conn.query_row(
            "SELECT status FROM formula_recalc_jobs WHERE id = ?",
            rusqlite::params![job_id],
            |row| row.get(0),
        ) {
            Ok(s) => s,
            Err(_) => break,
        };

        if current_status == "cancelled" {
            cancelled = true;
            break;
        }

        match evaluate_formula_for_instance(&conn, instance_iri, &job.property_iri) {
            Ok(value) => {
                store_calculated_value(&conn, instance_iri, &job.property_iri, &value, now_millis());
            }
            Err(msg) => {
                let now = now_millis();
                let _ = conn.execute(
                    "INSERT INTO formula_instance_errors (instance_iri, property_iri, error_message, created_at)
                     VALUES (?, ?, ?, ?)
                     ON CONFLICT(instance_iri, property_iri) DO UPDATE SET error_message = excluded.error_message, created_at = excluded.created_at",
                    rusqlite::params![instance_iri, job.property_iri, msg, now],
                );
            }
        }

        processed += 1;
        last_offset += 1;

        let _ = conn.execute(
            "UPDATE formula_recalc_jobs SET processed = ?, last_offset = ?, updated_at = ? WHERE id = ?",
            rusqlite::params![processed, last_offset, now_millis(), job_id],
        );

        let pct = (processed * 100 / total.max(1)) as u32;
        if pct != last_pct {
            last_pct = pct;
            let event = FormulaProgressEvent {
                job_id: job_id.to_string(),
                property_iri: job.property_iri.clone(),
                property_label: job.property_label.clone(),
                class_iri: job.class_iri.clone(),
                class_label: job.class_label.clone(),
                percent: pct,
                status: "running".to_string(),
            };
            app.emit("formula-recalc-progress", event).ok();
        }
    }

    let final_status = if cancelled { "cancelled" } else { "completed" };
    let _ = conn.execute(
        "UPDATE formula_recalc_jobs SET status = ?, updated_at = ? WHERE id = ?",
        rusqlite::params![final_status, now_millis(), job_id],
    );

    let event = FormulaProgressEvent {
        job_id: job_id.to_string(),
        property_iri: job.property_iri.clone(),
        property_label: job.property_label.clone(),
        class_iri: job.class_iri.clone(),
        class_label: job.class_label.clone(),
        percent: last_pct,
        status: final_status.to_string(),
    };
    app.emit("formula-recalc-progress", event).ok();
}

fn load_job(conn: &Connection, job_id: &str) -> Option<JobRecord> {
    conn.query_row(
        "SELECT property_iri, property_label, class_iri, class_label, processed, last_offset, status
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
            ))
        },
    )
    .ok()
    .and_then(|(property_iri, property_label, class_iri, class_label, processed, last_offset, status)| {
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
    crate::owl::formula::evaluate_formula_for_instance_raw(conn, instance_iri, property_iri)
}

fn store_calculated_value(
    conn: &Connection,
    instance_iri: &str,
    property_iri: &str,
    value: &str,
    now: i64,
) {
    let _ = conn.execute(
        "UPDATE triples SET retracted = 1 WHERE subject = ? AND predicate = ? AND retracted = 0",
        rusqlite::params![instance_iri, property_iri],
    );
    let _ = conn.execute(
        "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted)
         VALUES (?, ?, ?, 'literal', 'xsd:string', 1, 0, ?, 0)",
        rusqlite::params![instance_iri, property_iri, value, now],
    );
    let _ = conn.execute(
        "DELETE FROM formula_instance_errors WHERE instance_iri = ? AND property_iri = ?",
        rusqlite::params![instance_iri, property_iri],
    );
}

fn cancel_jobs_for_property(db_path: &PathBuf, property_iri: &str) {
    if let Ok(conn) = Connection::open(db_path) {
        let _ = conn.execute(
            "UPDATE formula_recalc_jobs SET status = 'cancelled', updated_at = ?
             WHERE property_iri = ? AND status IN ('pending', 'running')",
            rusqlite::params![now_millis(), property_iri],
        );
    }
}
