use crate::eavto::{query, enter_batch_transaction};
use crate::owl::{Individual, Connection, DbExecutor};
use crate::owl::LAST_UPDATED_AT;
use super::super::log_backend;

const RETENTION_DAYS_PROP: &str = "foundation:retentionDays";

fn classes_with_retention(conn: &Connection) -> Vec<(String, i64)> {
    let Ok(result) = query::get_by_predicate(conn, RETENTION_DAYS_PROP) else {
        return Vec::new();
    };
    result.triples.into_iter().filter_map(|t| {
        let days = match t.object {
            crate::eavto::Object::Integer(n) if n > 0 => n,
            _ => return None,
        };
        Some((t.subject, days))
    }).collect()
}


pub fn apply_retention_policy(conn: &mut Connection) -> Result<usize, String> {
    let classes = classes_with_retention(conn);
    if classes.is_empty() {
        log_backend("info", "[RETENTION] No classes have retentionDays configured — skipping");
        return Ok(0);
    }

    log_backend("info", &format!(
        "[RETENTION] Running policy for {} class(es): {}",
        classes.len(),
        classes.iter().map(|(c, d)| format!("{} ({}d)", c, d)).collect::<Vec<_>>().join(", ")
    ));

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut deleted = 0;
    let _batch = enter_batch_transaction();
    conn.execute_batch("BEGIN IMMEDIATE").map_err(|e| format!("Failed to begin transaction: {}", e))?;

    for (class_iri, retention_days) in classes {
        let cutoff_ms = now_ms - retention_days * 24 * 60 * 60 * 1000;

        let instances = Individual::find_by_class_with_date_range(conn, &class_iri, None, None, false)
            .map_err(|e| format!("Failed to query {} instances: {}", class_iri, e))?;

        log_backend("info", &format!(
            "[RETENTION] Scanning {} {} instance(s) (cutoff: {}d ago)",
            instances.len(), class_iri, retention_days
        ));

        let mut stale = 0;
        let mut skipped_no_timestamp = 0;

        for iri in instances {
            let last_updated = query::get_by_entity_predicate(conn, &iri, LAST_UPDATED_AT)
                .ok()
                .and_then(|r| r.triples.into_iter().next())
                .and_then(|t| match &t.object {
                    crate::eavto::Object::DateTime(s) => {
                        chrono::DateTime::parse_from_rfc3339(s)
                            .ok()
                            .map(|dt| (dt.timestamp_millis(), s.clone()))
                    }
                    _ => None,
                });

            let Some((last_ms, last_str)) = last_updated else {
                skipped_no_timestamp += 1;
                continue;
            };

            if last_ms < cutoff_ms {
                stale += 1;
                match Individual::retract(conn, &iri, "retention") {
                    Ok(()) => {
                        log_backend("info", &format!(
                            "[RETENTION] Retracted {} — last updated: {}",
                            iri, last_str
                        ));
                        deleted += 1;
                    }
                    Err(e) => {
                        log_backend("warn", &format!("[RETENTION] Failed to retract {}: {}", iri, e));
                    }
                }
            }
        }

        if skipped_no_timestamp > 0 {
            log_backend("warn", &format!(
                "[RETENTION] Skipped {} {} instance(s) with no lastUpdatedAt",
                skipped_no_timestamp, class_iri
            ));
        }

        log_backend("info", &format!(
            "[RETENTION] {} — {} stale, {} retracted",
            class_iri, stale, deleted
        ));
    }

    conn.execute_batch("COMMIT").map_err(|e| format!("Failed to commit transaction: {}", e))?;
    log_backend("info", &format!("[RETENTION] Done — {} total instance(s) retracted", deleted));

    Ok(deleted)
}

/// Apply retention policy via the async executor.
pub async fn run_retention_policy(executor: &DbExecutor) {
    match executor.write(|conn| {
        apply_retention_policy(conn).map(|n| n.to_string())
    }).await {
        Ok(ref s) if s == "0" => {}
        Ok(n) => log_backend("info", &format!("[RETENTION] Policy run complete: {} instance(s) deleted", n)),
        Err(e) => log_backend("warn", &format!("[RETENTION] Policy run failed: {}", e)),
    }
}
