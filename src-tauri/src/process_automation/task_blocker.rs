use tauri::{AppHandle, Manager};

use crate::owl::{
    DbExecutor, find_entities_with_property, get_iri_property, get_literal_property,
    replace_all_property_iris,
};
use crate::commands::log_backend;

const STATUS_PENDING: &str = "foundation:Pending";
const STATUS_BLOCKED: &str = "foundation:Blocked";

/// If `task_iri` has any active blockers that are not Completed, sets its status to Blocked.
pub fn check_and_block(conn: &mut rusqlite::Connection, task_iri: &str) {
    use crate::eavto::query;

    let result = match query::get_by_entity_predicate(conn, task_iri, "foundation:blockedBy") {
        Ok(r) => r,
        Err(e) => {
            log_backend("error", &format!("[task_blocker] check_and_block query failed for {}: {}", task_iri, e));
            return;
        }
    };

    let has_active_blocker = result.triples.iter().any(|triple| {
        let blocker_iri = match triple.object.as_iri() {
            Some(s) => s,
            None => return false,
        };
        get_literal_property(conn, blocker_iri, "foundation:result")
            .unwrap_or(None)
            .is_none()
    });

    if has_active_blocker {
        let current_status = get_iri_property(conn, task_iri, "foundation:hasStatus").unwrap_or(None);
        if current_status.as_deref() == Some(STATUS_BLOCKED) {
            return;
        }
        if let Err(e) = replace_all_property_iris(conn, task_iri, "foundation:hasStatus", &[STATUS_BLOCKED], "task_blocker") {
            log_backend("error", &format!("[task_blocker] block {} failed: {}", task_iri, e));
        } else {
            crate::owl::touch(conn, task_iri);
            log_backend("info", &format!("[task_blocker] blocked {}", task_iri));
        }
    }
}

/// Finds all tasks blocked by `completed_task_iri` and unblocks those
/// whose remaining blockers are all Completed.
pub fn check_and_unblock(conn: &mut rusqlite::Connection, completed_task_iri: &str) {
    let blocked_tasks = match find_entities_with_property(conn, "foundation:blockedBy", completed_task_iri) {
        Ok(v) => v,
        Err(e) => {
            log_backend("error", &format!("[task_blocker] find blocked tasks failed: {}", e));
            return;
        }
    };

    for task_iri in blocked_tasks {
        let all_blockers_done = match are_all_blockers_completed(conn, &task_iri) {
            Ok(v) => v,
            Err(e) => {
                log_backend("error", &format!("[task_blocker] check blockers for {} failed: {}", task_iri, e));
                continue;
            }
        };

        if all_blockers_done {
            if let Err(e) = replace_all_property_iris(conn, &task_iri, "foundation:hasStatus", &[STATUS_PENDING], "task_blocker") {
                log_backend("error", &format!("[task_blocker] unblock {} failed: {}", task_iri, e));
            } else {
                crate::owl::touch(conn, &task_iri);
                log_backend("info", &format!("[task_blocker] unblocked {}", task_iri));
            }
        }
    }
}

fn are_all_blockers_completed(conn: &rusqlite::Connection, task_iri: &str) -> Result<bool, String> {
    use crate::eavto::query;

    let result = query::get_by_entity_predicate(conn, task_iri, "foundation:blockedBy")
        .map_err(|e| e.to_string())?;

    for triple in &result.triples {
        let blocker_iri = match triple.object.as_iri() {
            Some(s) => s,
            None => continue,
        };
        let is_done = get_literal_property(conn, blocker_iri, "foundation:result")
            .map_err(|e| e.to_string())?
            .is_some();
        if !is_done {
            return Ok(false);
        }
    }

    Ok(true)
}

pub fn listen_for_blocking(app: AppHandle) {
    use tauri::Listener;

    app.clone().listen("entity-updated", move |event| {
        let entity_id = match parse_entity_id(event.payload()) {
            Some(id) => id,
            None => return,
        };
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let executor = match app2.try_state::<DbExecutor>() {
                Some(e) => e,
                None => return,
            };
            let entity = entity_id.clone();
            let (just_completed, is_task) = executor
                .read(move |conn| {
                    let is_task = crate::owl::is_instance_of(conn, &entity, "foundation:Task");
                    if !is_task {
                        return Ok((false, false));
                    }
                    let completed = get_literal_property(conn, &entity, "foundation:result")
                        .map_err(|e| e.to_string())?
                        .is_some();
                    Ok((completed, true))
                })
                .await
                .unwrap_or((false, false));

            if just_completed {
                let entity2 = entity_id.clone();
                let _ = executor
                    .write(move |conn| {
                        check_and_unblock(conn, &entity2);
                        Ok(String::new())
                    })
                    .await;
            } else if is_task {
                // Task may have received a new blockedBy — auto-block if needed
                let entity2 = entity_id.clone();
                let _ = executor
                    .write(move |conn| {
                        check_and_block(conn, &entity2);
                        Ok(String::new())
                    })
                    .await;
            }
        });
    });
}

fn parse_entity_id(payload: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v["entityId"].as_str().map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;

    fn insert(conn: &mut rusqlite::Connection, triples: &[Triple]) {
        store::assert_triples(conn, triples, "test").expect("insert failed");
    }

    fn task_blocked(iri: &str) -> Vec<Triple> {
        vec![
            Triple::new(iri, "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new(iri, "foundation:hasStatus", Object::Iri("foundation:Blocked".to_string())),
        ]
    }

    fn task_pending(iri: &str) -> Vec<Triple> {
        vec![Triple::new(iri, "rdf:type", Object::Iri("foundation:Task".to_string()))]
    }

    fn task_done(iri: &str) -> Vec<Triple> {
        vec![
            Triple::new(iri, "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new(iri, "foundation:result", Object::Literal {
                value: "completed".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ]
    }

    #[test]
    fn task_is_unblocked_when_blocker_has_result() {
        let mut conn = setup_test_db();

        insert(&mut conn, &task_done("foundation:TaskA"));
        insert(&mut conn, &task_blocked("foundation:TaskB"));
        insert(&mut conn, &[Triple::new(
            "foundation:TaskB", "foundation:blockedBy",
            Object::Iri("foundation:TaskA".to_string()),
        )]);

        check_and_unblock(&mut conn, "foundation:TaskA");

        let status = get_iri_property(&conn, "foundation:TaskB", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some(STATUS_PENDING.to_string()));
    }

    #[test]
    fn task_with_multiple_blockers_is_unblocked_only_when_all_have_result() {
        let mut conn = setup_test_db();

        insert(&mut conn, &task_done("foundation:TaskA"));
        insert(&mut conn, &task_pending("foundation:TaskB"));
        insert(&mut conn, &task_blocked("foundation:TaskC"));
        insert(&mut conn, &[
            Triple::new("foundation:TaskC", "foundation:blockedBy", Object::Iri("foundation:TaskA".to_string())),
            Triple::new("foundation:TaskC", "foundation:blockedBy", Object::Iri("foundation:TaskB".to_string())),
        ]);

        // A has a result — C must not be unblocked because B still has none
        check_and_unblock(&mut conn, "foundation:TaskA");
        let status = get_iri_property(&conn, "foundation:TaskC", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some("foundation:Blocked".to_string()), "C must remain Blocked");

        // B receives a result — now all blockers are completed
        insert(&mut conn, &[Triple::new(
            "foundation:TaskB", "foundation:result",
            Object::Literal { value: "done".to_string(), datatype: Some("xsd:string".to_string()), language: None },
        )]);
        check_and_unblock(&mut conn, "foundation:TaskB");
        let status = get_iri_property(&conn, "foundation:TaskC", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some(STATUS_PENDING.to_string()), "C must be unblocked");
    }

    #[test]
    fn task_without_blockers_is_not_affected_by_check_and_unblock() {
        let mut conn = setup_test_db();

        insert(&mut conn, &task_done("foundation:TaskA"));
        insert(&mut conn, &task_pending("foundation:TaskB"));

        check_and_unblock(&mut conn, "foundation:TaskA");

        let status = get_iri_property(&conn, "foundation:TaskB", "foundation:hasStatus").unwrap();
        assert!(status.is_none(), "B must not be changed because it has no blockedBy");
    }

    #[test]
    fn task_is_blocked_automatically_when_receiving_blocked_by() {
        let mut conn = setup_test_db();

        insert(&mut conn, &task_pending("foundation:TaskA"));
        insert(&mut conn, &task_pending("foundation:TaskB"));
        insert(&mut conn, &[Triple::new(
            "foundation:TaskB", "foundation:blockedBy",
            Object::Iri("foundation:TaskA".to_string()),
        )]);

        check_and_block(&mut conn, "foundation:TaskB");

        let status = get_iri_property(&conn, "foundation:TaskB", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some(STATUS_BLOCKED.to_string()), "B must be blocked");
    }

    #[test]
    fn task_is_not_blocked_if_blocker_already_has_result() {
        let mut conn = setup_test_db();

        insert(&mut conn, &task_done("foundation:TaskA"));
        insert(&mut conn, &task_pending("foundation:TaskB"));
        insert(&mut conn, &[Triple::new(
            "foundation:TaskB", "foundation:blockedBy",
            Object::Iri("foundation:TaskA".to_string()),
        )]);

        check_and_block(&mut conn, "foundation:TaskB");

        let status = get_iri_property(&conn, "foundation:TaskB", "foundation:hasStatus").unwrap();
        assert!(status.is_none(), "B must not be blocked because A already has a result");
    }
}
