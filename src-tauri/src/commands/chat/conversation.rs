use crate::owl::{Individual, Object, DbExecutor};
use tauri::State;

use super::parse_timestamp;

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

        conv.add_property(conn, "foundation:createdAt", vec![
            Object::DateTime(chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339()),
        ], "ai").map_err(|e| format!("Failed to set createdAt: {}", e))?;

        conv.add_property(conn, "foundation:hasStatus", vec![
            Object::Iri("foundation:InProgress".to_string()),
        ], "ai").map_err(|e| format!("Failed to set conversation status: {}", e))?;

        conv.add_property(conn, "foundation:handledBy", vec![
            Object::Iri("foundation:LocalAIAssistant".to_string()),
        ], "ai").map_err(|e| format!("Failed to set handledBy: {}", e))?;

        Ok(iri_clone)
    }).await?;

    Ok(serde_json::json!({ "iri": conv_iri, "label": conv_label }))
}

#[tauri::command]
pub async fn chat__list_conversations(
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor.read(move |conn| {
        let iris = Individual::find_by_class_with_date_range(
            conn,
            "foundation:AIConversation",
            None,
            None,
            false,
        ).map_err(|e| format!("Failed to query conversations: {}", e))?;

        let last_msg_map: std::collections::HashMap<String, i64> = {
            let mut stmt = conn.prepare(
                "SELECT tp.object, MAX(ts.object_value) \
                 FROM triples tp \
                 JOIN triples ts ON ts.subject = tp.subject \
                   AND ts.predicate = 'foundation:sentAt' \
                   AND ts.retracted = 0 \
                 WHERE tp.predicate = 'foundation:partOfConversation' \
                   AND tp.retracted = 0 \
                 GROUP BY tp.object"
            ).map_err(|e| format!("Failed to prepare last-message query: {}", e))?;
            let rows: Vec<(String, String)> = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("Failed to query last messages: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
            rows.into_iter().filter_map(|(iri, rfc3339)| {
                chrono::DateTime::parse_from_rfc3339(&rfc3339).ok()
                    .map(|dt| (iri, dt.timestamp_millis()))
            }).collect()
        };

        let mut conversations: Vec<(i64, serde_json::Value)> = Vec::new();

        for iri in iris {
            let ind = Individual::get(conn, &iri)
                .ok().flatten()
                .unwrap_or_else(|| Individual::new(&iri));

            let started_at = ind.properties.iter()
                .find(|(k, _)| k == "foundation:createdAt")
                .and_then(|(_, v)| parse_timestamp(v))
                .unwrap_or(0);

            let label = ind.label.clone().filter(|l| !l.is_empty()).unwrap_or_else(|| {
                if started_at > 0 {
                    let secs = started_at / 1000;
                    chrono::DateTime::from_timestamp(secs, 0)
                        .map(|dt| dt.format("Conversation %b %d, %Y %H:%M").to_string())
                        .unwrap_or_else(|| "New Conversation".to_string())
                } else {
                    "New Conversation".to_string()
                }
            });

            let sort_key = last_msg_map.get(&iri).copied().unwrap_or(started_at);
            conversations.push((sort_key, serde_json::json!({
                "iri": iri,
                "label": label,
                "startedAt": started_at,
            })));
        }

        conversations.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(conversations.into_iter().map(|(_, v)| v).collect())
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
