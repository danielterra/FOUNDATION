use super::super::log_backend;
use crate::eavto::{enter_batch_transaction, query};
use crate::owl::LAST_UPDATED_AT;
use crate::owl::{Connection, DbExecutor, Individual};

const RETENTION_DAYS_PROP: &str = "foundation:retentionDays";
const CONVERSATION_CLASS: &str = "foundation:AIConversation";

fn classes_with_retention(conn: &Connection) -> Vec<(String, i64)> {
    let Ok(result) = query::get_by_predicate(conn, RETENTION_DAYS_PROP) else {
        return Vec::new();
    };
    result
        .triples
        .into_iter()
        .filter_map(|t| {
            let days = match t.object {
                crate::eavto::Object::Integer(n) if n > 0 => n,
                _ => return None,
            };
            Some((t.subject, days))
        })
        .collect()
}

pub fn apply_retention_policy(conn: &mut Connection) -> Result<usize, String> {
    let classes = classes_with_retention(conn);
    if classes.is_empty() {
        log_backend(
            "info",
            "[RETENTION] No classes have retentionDays configured — skipping",
        );
        return Ok(0);
    }

    log_backend(
        "info",
        &format!(
            "[RETENTION] Running policy for {} class(es): {}",
            classes.len(),
            classes
                .iter()
                .map(|(c, d)| format!("{} ({}d)", c, d))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut deleted = 0;
    let _batch = enter_batch_transaction();
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| format!("Failed to begin transaction: {}", e))?;

    for (class_iri, retention_days) in classes {
        let cutoff_ms = now_ms - retention_days * 24 * 60 * 60 * 1000;

        let instances =
            Individual::find_by_class_with_date_range(conn, &class_iri, None, None, false)
                .map_err(|e| format!("Failed to query {} instances: {}", class_iri, e))?;

        log_backend(
            "info",
            &format!(
                "[RETENTION] Scanning {} {} instance(s) (cutoff: {}d ago)",
                instances.len(),
                class_iri,
                retention_days
            ),
        );

        let mut stale = 0;
        let mut skipped_no_timestamp = 0;
        let mut preserved_human_conversations = 0;

        for iri in instances {
            if class_iri == CONVERSATION_CLASS
                && crate::core_ontology::conversation::is_human_conversation(conn, &iri)
                    .map_err(|e| format!("Failed to classify conversation {}: {}", iri, e))?
            {
                preserved_human_conversations += 1;
                continue;
            }

            let last_updated = query::get_by_entity_predicate(conn, &iri, LAST_UPDATED_AT)
                .ok()
                .and_then(|r| r.triples.into_iter().next())
                .and_then(|t| match &t.object {
                    crate::eavto::Object::DateTime(s) => chrono::DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|dt| (dt.timestamp_millis(), s.clone())),
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
                        log_backend(
                            "info",
                            &format!("[RETENTION] Retracted {} — last updated: {}", iri, last_str),
                        );
                        deleted += 1;
                    }
                    Err(e) => {
                        log_backend(
                            "warn",
                            &format!("[RETENTION] Failed to retract {}: {}", iri, e),
                        );
                    }
                }
            }
        }

        if skipped_no_timestamp > 0 {
            log_backend(
                "warn",
                &format!(
                    "[RETENTION] Skipped {} {} instance(s) with no lastUpdatedAt",
                    skipped_no_timestamp, class_iri
                ),
            );
        }

        if preserved_human_conversations > 0 {
            log_backend(
                "info",
                &format!(
                    "[RETENTION] Preserved {} human conversation(s)",
                    preserved_human_conversations
                ),
            );
        }

        log_backend(
            "info",
            &format!(
                "[RETENTION] {} — {} stale, {} retracted",
                class_iri, stale, deleted
            ),
        );
    }

    conn.execute_batch("COMMIT")
        .map_err(|e| format!("Failed to commit transaction: {}", e))?;
    log_backend(
        "info",
        &format!("[RETENTION] Done — {} total instance(s) retracted", deleted),
    );

    Ok(deleted)
}

/// Apply retention policy via the async executor.
pub async fn run_retention_policy(executor: &DbExecutor) {
    match executor
        .write(|conn| apply_retention_policy(conn).map(|n| n.to_string()))
        .await
    {
        Ok(ref s) if s == "0" => {}
        Ok(n) => log_backend(
            "info",
            &format!("[RETENTION] Policy run complete: {} instance(s) deleted", n),
        ),
        Err(e) => log_backend("warn", &format!("[RETENTION] Policy run failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::eavto::{store, Object, Triple};

    const OLD_TIMESTAMP: &str = "2000-01-01T00:00:00+00:00";
    const RECENT_TIMESTAMP: &str = "2999-01-01T00:00:00+00:00";

    fn setup_retention_class(conn: &mut Connection) {
        store::assert_triples(
            conn,
            &[Triple::new(
                CONVERSATION_CLASS,
                RETENTION_DAYS_PROP,
                Object::Integer(7),
            )],
            "test",
        )
        .unwrap();
    }

    fn insert_conversation(conn: &mut Connection, iri: &str, last_updated: &str, human: bool) {
        let mut triples = vec![
            Triple::new(iri, "rdf:type", Object::Iri(CONVERSATION_CLASS.to_string())),
            Triple::new(
                iri,
                "rdfs:label",
                Object::Literal {
                    value: iri.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            ),
            Triple::new(
                iri,
                LAST_UPDATED_AT,
                Object::DateTime(last_updated.to_string()),
            ),
            Triple::new(
                iri,
                "foundation:hasParticipant",
                Object::Iri("foundation:LocalAIAssistant".to_string()),
            ),
        ];
        if human {
            triples.push(Triple::new(
                iri,
                "foundation:hasParticipant",
                Object::Iri("foundation:ThisUser".to_string()),
            ));
        }
        store::assert_triples(conn, &triples, "test").unwrap();
    }

    fn is_active(conn: &Connection, iri: &str) -> bool {
        Individual::get(conn, iri).unwrap().is_some()
    }

    #[test]
    fn old_human_conversation_is_preserved() {
        let mut conn = setup_test_db();
        setup_retention_class(&mut conn);
        insert_conversation(&mut conn, "test:HumanOld", OLD_TIMESTAMP, true);

        assert_eq!(apply_retention_policy(&mut conn).unwrap(), 0);
        assert!(is_active(&conn, "test:HumanOld"));
    }

    #[test]
    fn old_internal_conversation_remains_eligible() {
        let mut conn = setup_test_db();
        setup_retention_class(&mut conn);
        insert_conversation(&mut conn, "test:InternalOld", OLD_TIMESTAMP, false);

        assert_eq!(apply_retention_policy(&mut conn).unwrap(), 1);
        assert!(!is_active(&conn, "test:InternalOld"));
    }

    #[test]
    fn recent_internal_conversation_is_preserved() {
        let mut conn = setup_test_db();
        setup_retention_class(&mut conn);
        insert_conversation(&mut conn, "test:InternalRecent", RECENT_TIMESTAMP, false);

        assert_eq!(apply_retention_policy(&mut conn).unwrap(), 0);
        assert!(is_active(&conn, "test:InternalRecent"));
    }

    #[test]
    fn mixed_batch_only_retracts_old_internal_conversation() {
        let mut conn = setup_test_db();
        setup_retention_class(&mut conn);
        insert_conversation(&mut conn, "test:HumanOld", OLD_TIMESTAMP, true);
        insert_conversation(&mut conn, "test:InternalOld", OLD_TIMESTAMP, false);
        insert_conversation(&mut conn, "test:InternalRecent", RECENT_TIMESTAMP, false);

        assert_eq!(apply_retention_policy(&mut conn).unwrap(), 1);
        assert!(is_active(&conn, "test:HumanOld"));
        assert!(!is_active(&conn, "test:InternalOld"));
        assert!(is_active(&conn, "test:InternalRecent"));
    }

    #[test]
    fn restoring_conversation_preserves_identity_properties_and_relationships() {
        let mut conn = setup_test_db();
        insert_conversation(&mut conn, "test:HumanOld", OLD_TIMESTAMP, true);
        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "test:HumanOld",
                "foundation:handledBy",
                Object::Iri("foundation:LocalAIAssistant".to_string()),
            )],
            "test",
        )
        .unwrap();

        Individual::retract(&mut conn, "test:HumanOld", "test").unwrap();
        Individual::restore(&mut conn, "test:HumanOld", "test").unwrap();

        let restored = Individual::get(&conn, "test:HumanOld")
            .unwrap()
            .expect("restored conversation must retain its IRI");
        assert_eq!(restored.iri, "test:HumanOld");
        assert!(
            crate::core_ontology::conversation::is_human_conversation(&conn, "test:HumanOld")
                .unwrap()
        );
        assert_eq!(
            crate::owl::get_iri_property(&conn, "test:HumanOld", "foundation:handledBy")
                .unwrap()
                .as_deref(),
            Some("foundation:LocalAIAssistant")
        );
    }
}
