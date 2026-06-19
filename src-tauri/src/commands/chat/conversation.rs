use crate::owl::{Individual, Object, DbExecutor};
use tauri::{Emitter, State};


#[tauri::command]
pub async fn chat__create_conversation(
    label: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let conv_iri = format!("foundation:Conversation_{}", timestamp);
    let conv_label = label.unwrap_or_else(|| {
        chrono::DateTime::from_timestamp(timestamp / 1000, 0)
            .map(|dt| dt.format("Conversation %b %d, %Y %H:%M").to_string())
            .unwrap_or_else(|| "New Conversation".to_string())
    });

    let iri_clone = conv_iri.clone();
    let label_clone = conv_label.clone();

    executor.write(move |conn| {
        let conv = Individual::new(&iri_clone);

        conv.assert(conn, "foundation:AIConversation", &label_clone, "chat", "ai")
            .map_err(|e| format!("Failed to create conversation: {}", e))?;

        let created_at_dt = chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339();
        conv.add_property(conn, "foundation:createdAt", vec![
            Object::DateTime(created_at_dt.clone()),
        ], "ai").map_err(|e| format!("Failed to set createdAt: {}", e))?;
        conv.add_property(conn, "foundation:hasStartTime", vec![
            Object::DateTime(created_at_dt),
        ], "ai").map_err(|e| format!("Failed to set hasStartTime: {}", e))?;

        conv.add_property(conn, "foundation:hasStatus", vec![
            Object::Iri("foundation:InProgress".to_string()),
        ], "ai").map_err(|e| format!("Failed to set conversation status: {}", e))?;

        conv.add_property(conn, "foundation:handledBy", vec![
            Object::Iri("foundation:LocalAIAssistant".to_string()),
        ], "ai").map_err(|e| format!("Failed to set handledBy: {}", e))?;

        conv.add_property(conn, "foundation:hasParticipant", vec![
            Object::Iri("foundation:ThisUser".to_string()),
            Object::Iri("foundation:LocalAIAssistant".to_string()),
        ], "ai").map_err(|e| format!("Failed to set participants: {}", e))?;

        Ok(iri_clone)
    }).await?;

    Ok(serde_json::json!({ "iri": conv_iri, "label": conv_label }))
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__list_conversations(
    limit: Option<i64>,
    after_cursor_tx: Option<i64>,
    after_cursor_subject: Option<String>,
    snapshot_tx: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    executor.read(move |conn| {
        let page_size = limit.unwrap_or(200).max(1).min(200);

        let pinned_tx: i64 = match snapshot_tx {
            Some(tx) => tx,
            None => conn.query_row(
                "SELECT COALESCE(MAX(tx), 0) FROM triples",
                [],
                |row| row.get(0),
            ).map_err(|e| e.to_string())?,
        };

        // Build composite cursor from the two optional fields.
        // Both must be Some to form a valid cursor; if only one is present the caller
        // passed a malformed cursor and we treat it as no cursor (first page).
        let after: Option<(Option<i64>, String)> = match (after_cursor_tx, after_cursor_subject.clone()) {
            (Some(tx), Some(subj)) => Some((Some(tx), subj)),
            (None, Some(subj)) => Some((None, subj)),
            _ => None,
        };

        // Fetch limit+1 to detect has_more without a COUNT query.
        let rows = crate::eavto::query::page_as_of(
            conn,
            "rdf:type",
            "foundation:AIConversation",
            pinned_tx,
            "foundation:lastUpdatedAt",
            page_size + 1,
            after.as_ref().map(|(tx, subj)| (*tx, subj.as_str())),
            Some(("foundation:hasParticipant", "foundation:ThisUser")),
        ).map_err(|e| e.to_string())?;

        let has_more = rows.len() as i64 > page_size;
        let rows: Vec<_> = rows.into_iter().take(page_size as usize).collect();

        if rows.is_empty() {
            return Ok(serde_json::json!({
                "items": [],
                "next_cursor": serde_json::Value::Null,
                "has_more": false,
                "snapshot_tx": pinned_tx,
            }));
        }

        let iris: Vec<String> = rows.iter().map(|r| r.subject.clone()).collect();
        let placeholders = iris.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let ph_params: Vec<rusqlite::types::Value> = iris.iter()
            .map(|s| rusqlite::types::Value::Text(s.clone()))
            .collect();

        // Batch-fetch labels as-of snapshot_tx
        let label_sql = format!(
            "SELECT t.subject, t.object_value \
             FROM triples t \
             WHERE t.predicate = 'rdfs:label' \
               AND t.retracted = 0 \
               AND t.tx <= ?1 \
               AND t.subject IN ({placeholders}) \
               AND t.tx = (\
                   SELECT MAX(t2.tx) FROM triples t2 \
                   WHERE t2.subject = t.subject AND t2.predicate = 'rdfs:label' \
                     AND t2.retracted = 0 AND t2.tx <= ?1\
               )"
        );
        let mut label_params: Vec<rusqlite::types::Value> =
            vec![rusqlite::types::Value::Integer(pinned_tx)];
        label_params.extend(ph_params.iter().cloned());
        let mut label_stmt = conn.prepare(&label_sql).map_err(|e| e.to_string())?;
        let label_map: std::collections::HashMap<String, String> = label_stmt
            .query_map(rusqlite::params_from_iter(label_params.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        // Batch-fetch createdAt as-of snapshot_tx for label fallback
        let created_sql = format!(
            "SELECT t.subject, t.object_value \
             FROM triples t \
             WHERE t.predicate = 'foundation:createdAt' \
               AND t.retracted = 0 \
               AND t.tx <= ?1 \
               AND t.subject IN ({placeholders}) \
               AND t.tx = (\
                   SELECT MAX(t2.tx) FROM triples t2 \
                   WHERE t2.subject = t.subject AND t2.predicate = 'foundation:createdAt' \
                     AND t2.retracted = 0 AND t2.tx <= ?1\
               )"
        );
        let mut created_params: Vec<rusqlite::types::Value> =
            vec![rusqlite::types::Value::Integer(pinned_tx)];
        created_params.extend(ph_params.iter().cloned());
        let mut created_stmt = conn.prepare(&created_sql).map_err(|e| e.to_string())?;
        let created_map: std::collections::HashMap<String, i64> = created_stmt
            .query_map(rusqlite::params_from_iter(created_params.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .map(|(iri, dt)| {
                let ms = chrono::DateTime::parse_from_rfc3339(&dt)
                    .map(|d| d.timestamp_millis())
                    .unwrap_or(0);
                (iri, ms)
            })
            .collect();

        // Batch-fetch creation tx (tx of the rdf:type triple) as-of snapshot_tx
        let creation_tx_sql = format!(
            "SELECT t.subject, MIN(t.tx) \
             FROM triples t \
             WHERE t.predicate = 'rdf:type' \
               AND t.object = 'foundation:AIConversation' \
               AND t.retracted = 0 \
               AND t.tx <= ?1 \
               AND t.subject IN ({placeholders}) \
             GROUP BY t.subject"
        );
        let mut ct_params: Vec<rusqlite::types::Value> =
            vec![rusqlite::types::Value::Integer(pinned_tx)];
        ct_params.extend(ph_params.iter().cloned());
        let mut ct_stmt = conn.prepare(&creation_tx_sql).map_err(|e| e.to_string())?;
        let creation_tx_map: std::collections::HashMap<String, i64> = ct_stmt
            .query_map(rusqlite::params_from_iter(ct_params.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        // page_as_of already delivers rows in DESC lastUpdatedAt order — no re-sort needed.
        let items: Vec<serde_json::Value> = iris.iter().map(|iri| {
            let created_at = created_map.get(iri).copied().unwrap_or(0);
            let creation_tx = creation_tx_map.get(iri).copied().unwrap_or(0);
            let label = label_map.get(iri).cloned()
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| {
                    if created_at > 0 {
                        let secs = created_at / 1000;
                        chrono::DateTime::from_timestamp(secs, 0)
                            .map(|dt| dt.format("Conversation %b %d, %Y %H:%M").to_string())
                            .unwrap_or_else(|| "New Conversation".to_string())
                    } else {
                        "New Conversation".to_string()
                    }
                });
            serde_json::json!({
                "iri": iri,
                "label": label,
                "startedAt": created_at,
                "creationTx": creation_tx,
            })
        }).collect();

        let next_cursor = if has_more {
            rows.last().map(|r| serde_json::json!({
                "order_tx": r.order_tx,
                "after_subject": r.subject,
            })).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };

        Ok(serde_json::json!({
            "items": items,
            "next_cursor": next_cursor,
            "has_more": has_more,
            "snapshot_tx": pinned_tx,
        }))
    }).await
}

#[tauri::command]
pub async fn chat__delete_conversation(
    conversation_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        let message_iris = Individual::find_by_class_and_properties(
            conn,
            "foundation:AIConversationMessage",
            &[("foundation:partOfConversation", conversation_id.as_str())],
        ).map_err(|e| format!("Failed to query messages: {}", e))?;

        for iri in message_iris {
            Individual::retract(conn, &iri, "user")
                .map_err(|e| format!("Failed to retract message {}: {}", iri, e))?;
        }

        Individual::retract(conn, &conversation_id, "user")
            .map_err(|e| format!("Failed to retract conversation: {}", e))?;

        Ok("".to_string())
    }).await.map(|_| ())
}

#[tauri::command]
pub async fn chat__get_conversation_agent(
    conversation_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    executor.read(move |conn| {
        let agent_iri = crate::owl::get_iri_property(conn, &conversation_id, "foundation:handledBy")
            .map_err(|e| format!("Failed to get agent: {}", e))?
            .ok_or_else(|| format!("Conversation {} has no handledBy agent", conversation_id))?;

        let label = crate::owl::get_literal_property(conn, &agent_iri, "rdfs:label")
            .ok()
            .flatten();
        let icon = crate::owl::Thing::get(conn, &agent_iri).icon;

        Ok::<_, String>(serde_json::json!({ "iri": agent_iri, "label": label, "icon": icon }))
    }).await
}

#[tauri::command]
pub async fn chat__rename_conversation(
    conversation_id: String,
    label: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        let ind = Individual::new(&conversation_id);
        ind.add_property(conn, "rdfs:label", vec![Object::Literal {
            value: label,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "user")
            .map_err(|e| format!("Failed to rename conversation: {}", e))?;
        Ok("".to_string())
    }).await.map(|_| ())
}

#[tauri::command]
pub async fn chat__list_agents(
    limit: Option<i64>,
    offset: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor.read(move |conn| {
        // bounded-por-natureza; keyset-tx não agrega (não é append-no-topo)
        let page_size = limit.unwrap_or(200).max(1).min(500);
        let page_offset = offset.unwrap_or(0).max(0);
        let iris = crate::owl::find_entities_with_property_bounded(
            conn, "rdf:type", "foundation:SoftwareAgent", page_size, page_offset, Some("rdfs:label"),
        ).map_err(|e| format!("Failed to list agents: {}", e))?;

        let default_service_available = crate::commands::chat::settings::get_ai_service_iri(conn)
            .ok()
            .flatten()
            .is_some();

        let agents = iris.iter().map(|iri| {
            let thing = crate::owl::Thing::get(conn, iri);
            let has_service = crate::owl::get_iri_property(conn, iri, "foundation:usesService")
                .ok()
                .flatten()
                .is_some()
                || default_service_available;
            let is_active = crate::owl::get_iri_property(conn, iri, "foundation:hasStatus")
                .ok()
                .flatten()
                .map(|s| s == "foundation:Status_1773183515321")
                .unwrap_or(false);
            serde_json::json!({
                "iri": iri,
                "label": thing.label,
                "icon": thing.icon,
                "available": is_active && has_service,
            })
        }).collect();

        Ok(agents)
    }).await
}

#[tauri::command]
pub async fn chat__set_conversation_agent(
    conversation_id: String,
    agent_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let agent_iri_clone = agent_iri.clone();
    let conv_id_clone = conversation_id.clone();
    executor.write(move |conn| {
        let exists = Individual::get(conn, &agent_iri_clone)
            .map_err(|e| format!("Failed to look up agent: {}", e))?
            .is_some();
        if !exists {
            return Err(format!("Agent '{}' not found", agent_iri_clone));
        }

        crate::owl::replace_all_property_iris(
            conn,
            &conv_id_clone,
            "foundation:handledBy",
            &[agent_iri_clone.as_str()],
            "user",
        ).map_err(|e| format!("Failed to update handledBy: {}", e))?;

        Ok(String::new())
    }).await?;

    app.emit("agent-changed", serde_json::json!({ "agentIri": agent_iri })).ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::eavto::test_helpers::setup_test_db;
    use crate::eavto::{store, Triple, Object};
    use crate::owl::{Individual, Class, ClassType};

    const ICON: &str = "https://example.com/icon.svg";

    fn setup_agent(conn: &mut crate::eavto::Connection, iri: &str, label: &str, active: bool, has_service: bool) {
        Class::new("foundation:SoftwareAgent")
            .assert(conn, ClassType::OwlClass, "Software Agent", ICON, None, "test")
            .ok();

        Individual::new(iri)
            .assert(conn, "foundation:SoftwareAgent", label, ICON, "test")
            .unwrap();

        if active {
            store::assert_triples(conn, &[
                Triple::new(iri, "foundation:hasStatus",
                    Object::Iri("foundation:Status_1772755611667".to_string())),
            ], "test").unwrap();
        }
        if has_service {
            store::assert_triples(conn, &[
                Triple::new(iri, "foundation:usesService",
                    Object::Iri("foundation:SomeService".to_string())),
            ], "test").unwrap();
        }
    }

    fn setup_conversation(conn: &mut crate::eavto::Connection, conv_iri: &str, agent_iri: &str) {
        Class::new("foundation:AIConversation")
            .assert(conn, ClassType::OwlClass, "AI Conversation", ICON, None, "test")
            .ok();
        Individual::new(conv_iri)
            .assert(conn, "foundation:AIConversation", "Test Conv", ICON, "test")
            .unwrap();
        store::assert_triples(conn, &[
            Triple::new(conv_iri, "foundation:handledBy", Object::Iri(agent_iri.to_string())),
        ], "test").unwrap();
    }

    #[test]
    fn test_list_agents_returns_available_true_for_active_with_service() {
        let mut conn = setup_test_db();
        setup_agent(&mut conn, "test:AgentA", "Agent A", true, true);

        let has_service = crate::owl::get_iri_property(&conn, "test:AgentA", "foundation:usesService")
            .ok().flatten().is_some();
        let is_active = crate::owl::get_iri_property(&conn, "test:AgentA", "foundation:hasStatus")
            .ok().flatten()
            .map(|s| s == "foundation:Status_1772755611667")
            .unwrap_or(false);

        assert!(has_service, "Agent with usesService must have has_service=true");
        assert!(is_active, "Agent with Active status must have is_active=true");
        assert!(has_service && is_active, "available must be true for active agent with service");
    }

    #[test]
    fn test_list_agents_flags_unavailable_when_missing_service() {
        let mut conn = setup_test_db();
        setup_agent(&mut conn, "test:AgentNoService", "No Service Agent", true, false);

        let has_service = crate::owl::get_iri_property(&conn, "test:AgentNoService", "foundation:usesService")
            .ok().flatten().is_some();
        let is_active = crate::owl::get_iri_property(&conn, "test:AgentNoService", "foundation:hasStatus")
            .ok().flatten()
            .map(|s| s == "foundation:Status_1772755611667")
            .unwrap_or(false);

        assert!(!has_service, "Agent without usesService must not be available");
        assert!(is_active, "Agent status is Active");
        assert!(!(is_active && has_service), "available must be false");
    }

    #[test]
    fn test_set_conversation_agent_updates_handled_by() {
        let mut conn = setup_test_db();
        setup_agent(&mut conn, "test:AgentA", "Agent A", true, true);
        setup_agent(&mut conn, "test:AgentB", "Agent B", true, true);
        setup_conversation(&mut conn, "test:Conv1", "test:AgentA");

        crate::owl::replace_all_property_iris(
            &mut conn, "test:Conv1", "foundation:handledBy", &["test:AgentB"], "test"
        ).unwrap();

        let agent = crate::owl::get_iri_property(&conn, "test:Conv1", "foundation:handledBy")
            .unwrap().unwrap();
        assert_eq!(agent, "test:AgentB", "handledBy must be updated to new agent");
    }

    #[test]
    fn test_set_conversation_agent_nonexistent_agent_rejected() {
        let mut conn = setup_test_db();
        setup_conversation(&mut conn, "test:Conv2", "test:AgentA");

        let exists = Individual::get(&conn, "test:NonExistent").unwrap().is_some();
        assert!(!exists, "Non-existent agent must not be found");
    }
}
