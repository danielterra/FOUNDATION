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
                updated_at      INTEGER NOT NULL
            );
            CREATE TABLE formula_instance_errors (
                instance_iri    TEXT NOT NULL,
                property_iri    TEXT NOT NULL,
                error_message   TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                PRIMARY KEY (instance_iri, property_iri)
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

    // ── load_job ─────────────────────────────────────────────────────────────

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

    // ── store_calculated_value ────────────────────────────────────────────────

    #[test]
    fn test_store_calculated_value_inserts_new_triple() {
        let conn = setup_test_db();
        store_calculated_value(&conn, "foundation:Instance1", "foundation:score", "42", 1000);

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = ? AND retracted = 0",
            rusqlite::params!["foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);

        let value: String = conn.query_row(
            "SELECT object_value FROM triples WHERE subject = ? AND predicate = ? AND retracted = 0",
            rusqlite::params!["foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(value, "42");
    }

    #[test]
    fn test_store_calculated_value_retracts_existing_before_insert() {
        let conn = setup_test_db();
        // Insert an existing value
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, origin_id, tx, created_at, retracted)
             VALUES ('foundation:Instance1', 'foundation:score', 'old', 'literal', 1, 0, 0, 0)",
            [],
        ).unwrap();

        store_calculated_value(&conn, "foundation:Instance1", "foundation:score", "new", 2000);

        let retracted: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = ? AND retracted = 1",
            rusqlite::params!["foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(retracted, 1);

        let active: String = conn.query_row(
            "SELECT object_value FROM triples WHERE subject = ? AND predicate = ? AND retracted = 0",
            rusqlite::params!["foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(active, "new");
    }

    #[test]
    fn test_store_calculated_value_clears_formula_errors() {
        let conn = setup_test_db();
        // Insert an existing error
        conn.execute(
            "INSERT INTO formula_instance_errors (instance_iri, property_iri, error_message, created_at)
             VALUES ('foundation:Instance1', 'foundation:score', 'previous error', 0)",
            [],
        ).unwrap();

        store_calculated_value(&conn, "foundation:Instance1", "foundation:score", "42", 1000);

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM formula_instance_errors WHERE instance_iri = ? AND property_iri = ?",
            rusqlite::params!["foundation:Instance1", "foundation:score"],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 0);
    }

    // ── now_millis ────────────────────────────────────────────────────────────

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
}
