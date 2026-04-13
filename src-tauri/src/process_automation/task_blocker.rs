use tauri::{AppHandle, Manager};

use crate::owl::{
    DbExecutor, find_entities_with_property, get_iri_property, replace_all_property_iris,
};
use crate::process_automation::task_scheduler::is_status_descendant_of;
use crate::commands::log_backend;

const STATUS_PENDING: &str = "foundation:Pending";
const STATUS_BLOCKED: &str = "foundation:Blocked";
const STATUS_COMPLETED: &str = "foundation:Completed";

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
        let status = get_iri_property(conn, blocker_iri, "foundation:hasStatus").unwrap_or(None);
        !status
            .as_deref()
            .map(|s| is_status_descendant_of(conn, s, STATUS_COMPLETED))
            .unwrap_or(false)
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
        let status = get_iri_property(conn, blocker_iri, "foundation:hasStatus")
            .map_err(|e| e.to_string())?;
        let is_done = status
            .as_deref()
            .map(|s| is_status_descendant_of(conn, s, STATUS_COMPLETED))
            .unwrap_or(false);
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
                    let status = get_iri_property(conn, &entity, "foundation:hasStatus")
                        .map_err(|e| e.to_string())?;
                    let completed = status
                        .as_deref()
                        .map(|s| is_status_descendant_of(conn, s, STATUS_COMPLETED))
                        .unwrap_or(false);
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

    fn task(iri: &str, status: &str) -> Vec<Triple> {
        vec![
            Triple::new(iri, "rdf:type", Object::Iri("foundation:Task".to_string())),
            Triple::new(iri, "foundation:hasStatus", Object::Iri(status.to_string())),
        ]
    }

    fn blocker_tree(conn: &mut rusqlite::Connection) {
        // foundation:Completed is its own parent (root)
        insert(conn, &[Triple::new(
            "foundation:Completed", "foundation:parentStatus",
            Object::Iri("foundation:Completed".to_string()),
        )]);
        insert(conn, &[Triple::new(
            "foundation:Pending", "foundation:parentStatus",
            Object::Iri("foundation:Pending".to_string()),
        )]);
    }

    #[test]
    fn tarefa_e_desbloqueada_quando_bloqueadora_e_concluida() {
        let mut conn = setup_test_db();
        blocker_tree(&mut conn);

        insert(&mut conn, &task("foundation:TaskA", "foundation:Completed"));
        insert(&mut conn, &task("foundation:TaskB", "foundation:Blocked"));
        insert(&mut conn, &[Triple::new(
            "foundation:TaskB", "foundation:blockedBy",
            Object::Iri("foundation:TaskA".to_string()),
        )]);

        check_and_unblock(&mut conn, "foundation:TaskA");

        let status = get_iri_property(&conn, "foundation:TaskB", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some(STATUS_PENDING.to_string()));
    }

    #[test]
    fn tarefa_com_multiplos_bloqueadores_so_e_desbloqueada_quando_todos_concluidos() {
        let mut conn = setup_test_db();
        blocker_tree(&mut conn);

        insert(&mut conn, &task("foundation:TaskA", "foundation:Completed"));
        insert(&mut conn, &task("foundation:TaskB", "foundation:Pending"));
        insert(&mut conn, &task("foundation:TaskC", "foundation:Blocked"));
        insert(&mut conn, &[
            Triple::new("foundation:TaskC", "foundation:blockedBy", Object::Iri("foundation:TaskA".to_string())),
            Triple::new("foundation:TaskC", "foundation:blockedBy", Object::Iri("foundation:TaskB".to_string())),
        ]);

        // Conclui A — C não deve ser desbloqueada pois B ainda está Pending
        check_and_unblock(&mut conn, "foundation:TaskA");
        let status = get_iri_property(&conn, "foundation:TaskC", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some("foundation:Blocked".to_string()), "C deve permanecer Bloqueada");

        // Conclui B — agora todos os blockers estão Concluídos
        insert(&mut conn, &[Triple::new(
            "foundation:TaskB", "foundation:hasStatus",
            Object::Iri("foundation:Completed".to_string()),
        )]);
        check_and_unblock(&mut conn, "foundation:TaskB");
        let status = get_iri_property(&conn, "foundation:TaskC", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some(STATUS_PENDING.to_string()), "C deve ser desbloqueada");
    }

    #[test]
    fn tarefa_sem_bloqueadores_nao_e_afetada_por_check_and_unblock() {
        let mut conn = setup_test_db();
        blocker_tree(&mut conn);

        insert(&mut conn, &task("foundation:TaskA", "foundation:Completed"));
        insert(&mut conn, &task("foundation:TaskB", "foundation:Pending"));
        // TaskB não tem blockedBy

        check_and_unblock(&mut conn, "foundation:TaskA");

        let status = get_iri_property(&conn, "foundation:TaskB", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some("foundation:Pending".to_string()), "B não deve ser alterada");
    }

    #[test]
    fn tarefa_e_bloqueada_automaticamente_ao_receber_blocked_by() {
        let mut conn = setup_test_db();
        blocker_tree(&mut conn);

        insert(&mut conn, &task("foundation:TaskA", "foundation:Pending"));
        insert(&mut conn, &task("foundation:TaskB", "foundation:Pending"));
        // TaskB recebe blockedBy apontando para TaskA (que não está Concluída)
        insert(&mut conn, &[Triple::new(
            "foundation:TaskB", "foundation:blockedBy",
            Object::Iri("foundation:TaskA".to_string()),
        )]);

        check_and_block(&mut conn, "foundation:TaskB");

        let status = get_iri_property(&conn, "foundation:TaskB", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some(STATUS_BLOCKED.to_string()), "B deve ser bloqueada");
    }

    #[test]
    fn tarefa_nao_e_bloqueada_se_blocker_ja_esta_concluido() {
        let mut conn = setup_test_db();
        blocker_tree(&mut conn);

        insert(&mut conn, &task("foundation:TaskA", "foundation:Completed"));
        insert(&mut conn, &task("foundation:TaskB", "foundation:Pending"));
        insert(&mut conn, &[Triple::new(
            "foundation:TaskB", "foundation:blockedBy",
            Object::Iri("foundation:TaskA".to_string()),
        )]);

        check_and_block(&mut conn, "foundation:TaskB");

        let status = get_iri_property(&conn, "foundation:TaskB", "foundation:hasStatus").unwrap();
        assert_eq!(status, Some("foundation:Pending".to_string()), "B não deve ser bloqueada pois A já está Concluída");
    }
}
