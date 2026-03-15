use std::path::PathBuf;
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
                        cancel_jobs_for_property(&db_path, &property_iri).await;
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

async fn open_conn(db_path: &PathBuf) -> Option<turso::Connection> {
    let path_str = db_path.to_str()?;
    let client = turso::Builder::new_local(path_str).build().await.ok()?;
    client.connect().ok()
}

async fn process_job(app: &tauri::AppHandle, db_path: &PathBuf, job_id: &str) {
    let conn = match open_conn(db_path).await {
        Some(c) => c,
        None => return,
    };

    let _ = conn.execute("PRAGMA busy_timeout=5000", ()).await;

    let job = match load_job(&conn, job_id).await {
        Some(j) => j,
        None => return,
    };

    if conn.execute(
        "UPDATE formula_recalc_jobs SET status = 'running', updated_at = ? WHERE id = ?",
        turso::params![now_millis(), job_id],
    ).await.is_err() {
        return;
    }

    let total: i64 = {
        let mut stmt = match conn.prepare(
            "SELECT COUNT(DISTINCT subject) FROM triples WHERE predicate = 'rdf:type' AND object = ? AND retracted = 0"
        ).await {
            Ok(s) => s,
            Err(_) => return,
        };
        match stmt.query_row(turso::params![job.class_iri.clone()]).await {
            Ok(row) => row.get_value(0).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0),
            Err(_) => 0,
        }
    };

    let _ = conn.execute(
        "UPDATE formula_recalc_jobs SET total = ?, updated_at = ? WHERE id = ?",
        turso::params![total, now_millis(), job_id],
    ).await;

    let instance_iris: Vec<String> = {
        let mut stmt = match conn.prepare(
            "SELECT DISTINCT subject FROM triples
             WHERE predicate = 'rdf:type' AND object = ? AND retracted = 0
             ORDER BY subject
             LIMIT -1 OFFSET ?"
        ).await {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut rows = match stmt.query(turso::params![job.class_iri.clone(), job.last_offset]).await {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut iris = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            if let Ok(v) = row.get_value(0) {
                if let Some(s) = v.as_text() {
                    iris.push(s.clone());
                }
            }
        }
        iris
    };

    let mut processed = job.processed;
    let mut last_offset = job.last_offset;
    let mut last_pct: u32 = (processed * 100 / total.max(1)) as u32;
    let mut cancelled = false;

    for instance_iri in &instance_iris {
        let current_status: String = {
            let mut stmt = match conn.prepare(
                "SELECT status FROM formula_recalc_jobs WHERE id = ?"
            ).await {
                Ok(s) => s,
                Err(_) => break,
            };
            match stmt.query_row(turso::params![job_id]).await {
                Ok(row) => row.get_value(0).ok().and_then(|v| v.as_text().cloned()).unwrap_or_default(),
                Err(_) => break,
            }
        };

        if current_status == "cancelled" {
            cancelled = true;
            break;
        }

        match evaluate_formula_for_instance(&conn, instance_iri, &job.property_iri).await {
            Ok(value) => {
                store_calculated_value(&conn, instance_iri, &job.property_iri, &value, now_millis()).await;
            }
            Err(msg) => {
                let now = now_millis();
                let _ = conn.execute(
                    "INSERT INTO formula_instance_errors (instance_iri, property_iri, error_message, created_at)
                     VALUES (?, ?, ?, ?)
                     ON CONFLICT(instance_iri, property_iri) DO UPDATE SET error_message = excluded.error_message, created_at = excluded.created_at",
                    turso::params![instance_iri.clone(), job.property_iri.clone(), msg, now],
                ).await;
            }
        }

        processed += 1;
        last_offset += 1;

        let _ = conn.execute(
            "UPDATE formula_recalc_jobs SET processed = ?, last_offset = ?, updated_at = ? WHERE id = ?",
            turso::params![processed, last_offset, now_millis(), job_id],
        ).await;

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
        turso::params![final_status, now_millis(), job_id],
    ).await;

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

async fn load_job(conn: &turso::Connection, job_id: &str) -> Option<JobRecord> {
    let mut stmt = conn.prepare(
        "SELECT property_iri, property_label, class_iri, class_label, processed, last_offset, status
         FROM formula_recalc_jobs WHERE id = ?"
    ).await.ok()?;

    let row = stmt.query_row(turso::params![job_id]).await.ok()?;

    let property_iri = row.get_value(0).ok()?.as_text().cloned()?;
    let property_label = row.get_value(1).ok()?.as_text().cloned().unwrap_or_default();
    let class_iri = row.get_value(2).ok()?.as_text().cloned()?;
    let class_label = row.get_value(3).ok()?.as_text().cloned().unwrap_or_default();
    let processed = row.get_value(4).ok()?.as_integer().copied().unwrap_or(0);
    let last_offset = row.get_value(5).ok()?.as_integer().copied().unwrap_or(0);
    let status = row.get_value(6).ok()?.as_text().cloned().unwrap_or_default();

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
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

async fn evaluate_formula_for_instance(
    conn: &turso::Connection,
    instance_iri: &str,
    property_iri: &str,
) -> Result<String, String> {
    crate::owl::formula::evaluate_formula_for_instance_raw(conn, instance_iri, property_iri).await
}

async fn store_calculated_value(
    conn: &turso::Connection,
    instance_iri: &str,
    property_iri: &str,
    value: &str,
    now: i64,
) {
    let _ = conn.execute(
        "UPDATE triples SET retracted = 1 WHERE subject = ? AND predicate = ? AND retracted = 0",
        turso::params![instance_iri, property_iri],
    ).await;
    let _ = conn.execute(
        "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted)
         VALUES (?, ?, ?, 'literal', 'xsd:string', 1, 0, ?, 0)",
        turso::params![instance_iri, property_iri, value, now],
    ).await;
    let _ = conn.execute(
        "DELETE FROM formula_instance_errors WHERE instance_iri = ? AND property_iri = ?",
        turso::params![instance_iri, property_iri],
    ).await;
}

async fn cancel_jobs_for_property(db_path: &PathBuf, property_iri: &str) {
    if let Some(conn) = open_conn(db_path).await {
        let _ = conn.execute(
            "UPDATE formula_recalc_jobs SET status = 'cancelled', updated_at = ?
             WHERE property_iri = ? AND status IN ('pending', 'running')",
            turso::params![now_millis(), property_iri],
        ).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> turso::Connection {
        let client = turso::Builder::new_local(":memory:").build().await.unwrap();
        let conn = client.connect().unwrap();
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
        ").await.unwrap();
        conn
    }

    async fn insert_job(conn: &turso::Connection, id: &str, property_iri: &str, class_iri: &str, status: &str) {
        conn.execute(
            "INSERT INTO formula_recalc_jobs
             (id, property_iri, property_label, class_iri, class_label, status, created_at, updated_at)
             VALUES (?, ?, '', ?, '', ?, 0, 0)",
            turso::params![id, property_iri, class_iri, status],
        ).await.unwrap();
    }

    // ── load_job ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_load_job_returns_none_when_not_found() {
        let conn = setup_test_db().await;
        assert!(load_job(&conn, "nonexistent-job").await.is_none());
    }

    #[tokio::test]
    async fn test_load_job_returns_none_for_non_pending_status() {
        let conn = setup_test_db().await;
        insert_job(&conn, "job1", "foundation:MyProp", "foundation:Task", "running").await;

        let result = load_job(&conn, "job1").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_load_job_returns_record_for_pending_status() {
        let conn = setup_test_db().await;
        insert_job(&conn, "job1", "foundation:MyProp", "foundation:Task", "pending").await;

        let result = load_job(&conn, "job1").await;
        assert!(result.is_some());
        let record = result.unwrap();
        assert_eq!(record.property_iri, "foundation:MyProp");
        assert_eq!(record.class_iri, "foundation:Task");
        assert_eq!(record.processed, 0);
        assert_eq!(record.last_offset, 0);
    }

    #[tokio::test]
    async fn test_load_job_returns_none_for_cancelled_status() {
        let conn = setup_test_db().await;
        insert_job(&conn, "job1", "foundation:MyProp", "foundation:Task", "cancelled").await;

        assert!(load_job(&conn, "job1").await.is_none());
    }

    #[tokio::test]
    async fn test_load_job_returns_none_for_completed_status() {
        let conn = setup_test_db().await;
        insert_job(&conn, "job1", "foundation:MyProp", "foundation:Task", "completed").await;

        assert!(load_job(&conn, "job1").await.is_none());
    }

    // ── store_calculated_value ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_store_calculated_value_inserts_new_triple() {
        let conn = setup_test_db().await;
        store_calculated_value(&conn, "foundation:Instance1", "foundation:score", "42", 1000).await;

        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = ? AND retracted = 0"
        ).await.unwrap();
        let row = stmt.query_row(turso::params!["foundation:Instance1", "foundation:score"]).await.unwrap();
        let count: i64 = row.get_value(0).unwrap().as_integer().copied().unwrap_or(0);
        assert_eq!(count, 1);

        let mut stmt2 = conn.prepare(
            "SELECT object_value FROM triples WHERE subject = ? AND predicate = ? AND retracted = 0"
        ).await.unwrap();
        let row2 = stmt2.query_row(turso::params!["foundation:Instance1", "foundation:score"]).await.unwrap();
        let value = row2.get_value(0).unwrap().as_text().cloned().unwrap_or_default();
        assert_eq!(value, "42");
    }

    #[tokio::test]
    async fn test_store_calculated_value_retracts_existing_before_insert() {
        let conn = setup_test_db().await;
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, origin_id, tx, created_at, retracted)
             VALUES ('foundation:Instance1', 'foundation:score', 'old', 'literal', 1, 0, 0, 0)",
            (),
        ).await.unwrap();

        store_calculated_value(&conn, "foundation:Instance1", "foundation:score", "new", 2000).await;

        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM triples WHERE subject = ? AND predicate = ? AND retracted = 1"
        ).await.unwrap();
        let row = stmt.query_row(turso::params!["foundation:Instance1", "foundation:score"]).await.unwrap();
        let retracted: i64 = row.get_value(0).unwrap().as_integer().copied().unwrap_or(0);
        assert_eq!(retracted, 1);

        let mut stmt2 = conn.prepare(
            "SELECT object_value FROM triples WHERE subject = ? AND predicate = ? AND retracted = 0"
        ).await.unwrap();
        let row2 = stmt2.query_row(turso::params!["foundation:Instance1", "foundation:score"]).await.unwrap();
        let active = row2.get_value(0).unwrap().as_text().cloned().unwrap_or_default();
        assert_eq!(active, "new");
    }

    #[tokio::test]
    async fn test_store_calculated_value_clears_formula_errors() {
        let conn = setup_test_db().await;
        conn.execute(
            "INSERT INTO formula_instance_errors (instance_iri, property_iri, error_message, created_at)
             VALUES ('foundation:Instance1', 'foundation:score', 'previous error', 0)",
            (),
        ).await.unwrap();

        store_calculated_value(&conn, "foundation:Instance1", "foundation:score", "42", 1000).await;

        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM formula_instance_errors WHERE instance_iri = ? AND property_iri = ?"
        ).await.unwrap();
        let row = stmt.query_row(turso::params!["foundation:Instance1", "foundation:score"]).await.unwrap();
        let count: i64 = row.get_value(0).unwrap().as_integer().copied().unwrap_or(0);
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
