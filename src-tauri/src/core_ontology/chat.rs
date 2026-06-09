use crate::owl::{Connection, Individual, OwlError};

/// Returns IRIs of messages in `conversation_iri` with sentAt > `since_rfc3339`, ordered
/// newest-first. The compaction message (if any) is included because its sentAt is always
/// after the summarized messages.
pub fn find_messages_by_conversation_since(
    conn: &Connection,
    conversation_iri: &str,
    since_rfc3339: &str,
) -> Result<Vec<String>, OwlError> {
    let sql = "
        SELECT subject FROM (
            SELECT t_conv.subject, MAX(t_sent.object_value) AS ts
            FROM (
                SELECT subject, retracted,
                       MAX(tx) OVER (PARTITION BY subject, predicate) AS max_tx, tx
                FROM triples
                WHERE predicate = 'foundation:partOfConversation'
                  AND (object = ?1 OR object_value = ?1)
            ) t_conv
            INNER JOIN triples t_sent
                ON t_conv.subject = t_sent.subject
               AND t_sent.predicate = 'foundation:sentAt'
               AND t_sent.retracted = 0
            WHERE t_conv.tx = t_conv.max_tx AND t_conv.retracted = 0
            GROUP BY t_conv.subject
            HAVING MAX(t_sent.object_value) > ?2
        )
        ORDER BY ts DESC
    ";
    let mut stmt = conn.prepare(sql).map_err(|e| OwlError::DatabaseError(e.to_string()))?;
    let iris = stmt
        .query_map(rusqlite::params![conversation_iri, since_rfc3339], |row| row.get(0))
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
    Ok(iris)
}

/// Returns IRIs of messages in `conversation_iri` ordered by sentAt descending (newest first).
/// Pass `limit = usize::MAX` for no limit.
pub fn find_messages_by_conversation(
    conn: &Connection,
    conversation_iri: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<String>, OwlError> {
    Individual::find_subjects_linked_to_ordered_by(
        conn,
        conversation_iri,
        "foundation:partOfConversation",
        "foundation:sentAt",
        limit,
        offset,
    )
}

/// Returns the IRI of the first message in `conversation_iri` whose tool result
/// corresponds to `tool_use_id`. Used to detect duplicate tool-result storage on recovery.
pub fn find_tool_result_message_iri(
    conn: &Connection,
    tool_use_id: &str,
    conversation_iri: &str,
) -> Option<String> {
    Individual::find_parent_by_linked_id_and_scope(
        conn,
        tool_use_id,
        "anthropic:toolUseId",
        "anthropic:resultOf",
        "foundation:hasContentBlock",
        "foundation:partOfConversation",
        conversation_iri,
    )
}

/// Returns true if `message_iri` has at least one content block that is NOT a ToolResultBlock.
/// Used to distinguish real user messages from system-generated tool-result messages.
pub fn message_has_non_tool_result_block(conn: &Connection, message_iri: &str) -> bool {
    Individual::has_linked_object_without_type(
        conn,
        message_iri,
        "foundation:hasContentBlock",
        "anthropic:ToolResultBlock",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, Triple, Object};
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::vocabulary::rdf;

    fn insert_message(conn: &mut Connection, iri: &str, conversation_iri: &str, sent_at_ms: i64) {
        let rfc3339 = chrono::DateTime::from_timestamp_millis(sent_at_ms)
            .unwrap_or_default()
            .to_rfc3339();
        store::assert_triples(conn, &[
            Triple::new(iri, rdf::TYPE, Object::Iri("foundation:AIConversationMessage".to_string())),
            Triple::new(iri, "foundation:partOfConversation", Object::Iri(conversation_iri.to_string())),
            Triple::new(iri, "foundation:sentAt", Object::DateTime(rfc3339)),
        ], "test").unwrap();
    }

    #[test]
    fn find_messages_empty_db() {
        let conn = setup_test_db();
        let result = find_messages_by_conversation(&conn, "foundation:ConvA", usize::MAX, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn find_messages_ordered_newest_first() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);
        let result = find_messages_by_conversation(&conn, "foundation:ConvA", usize::MAX, 0).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "foundation:Msg2");
        assert_eq!(result[1], "foundation:Msg3");
        assert_eq!(result[2], "foundation:Msg1");
    }

    #[test]
    fn find_messages_respects_limit() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);
        let result = find_messages_by_conversation(&conn, "foundation:ConvA", 2, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "foundation:Msg2");
        assert_eq!(result[1], "foundation:Msg3");
    }

    #[test]
    fn find_messages_respects_offset() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);
        let result = find_messages_by_conversation(&conn, "foundation:ConvA", usize::MAX, 1).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "foundation:Msg3");
        assert_eq!(result[1], "foundation:Msg1");
    }

    #[test]
    fn find_messages_excludes_other_conversations() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvB", 3_000);
        let result = find_messages_by_conversation(&conn, "foundation:ConvA", usize::MAX, 0).unwrap();
        assert_eq!(result, vec!["foundation:Msg1".to_string()]);
    }
}
