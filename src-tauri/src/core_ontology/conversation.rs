use crate::owl::{
    get_all_iri_properties, get_literal_property, is_instance_of, Connection, Individual, Result,
};

const CONVERSATION_CLASS: &str = "foundation:AIConversation";
const PARTICIPANT_PROPERTY: &str = "foundation:hasParticipant";
const THIS_USER: &str = "foundation:ThisUser";

pub fn is_active_conversation(conn: &Connection, conversation_iri: &str) -> bool {
    is_instance_of(conn, conversation_iri, CONVERSATION_CLASS)
}

pub fn is_human_conversation(conn: &Connection, conversation_iri: &str) -> Result<bool> {
    Ok(
        get_all_iri_properties(conn, conversation_iri, PARTICIPANT_PROPERTY)?
            .iter()
            .any(|participant| participant == THIS_USER),
    )
}

/// Returns the active, agent-backed conversation containing the newest current message.
pub fn find_most_recent_active_conversation(conn: &Connection) -> Result<Option<String>> {
    let candidates = ["user", "assistant"]
        .into_iter()
        .map(|role| {
            Individual::find_class_instance_ordered_by_child_timestamp(
                conn,
                CONVERSATION_CLASS,
                "foundation:handledBy",
                "foundation:partOfConversation",
                "foundation:sentAt",
                "foundation:role",
                role,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    let mut most_recent: Option<(i64, String)> = None;
    for conversation_iri in candidates.into_iter().flatten() {
        let Some(message_iri) =
            super::chat::find_messages_by_conversation(conn, &conversation_iri, 1, 0)?
                .into_iter()
                .next()
        else {
            continue;
        };
        let Some(sent_at) = get_literal_property(conn, &message_iri, "foundation:sentAt")? else {
            continue;
        };
        let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(&sent_at) else {
            continue;
        };
        let timestamp_ms = timestamp.timestamp_millis();

        if most_recent
            .as_ref()
            .is_none_or(|(current_ms, _)| timestamp_ms > *current_ms)
        {
            most_recent = Some((timestamp_ms, conversation_iri));
        }
    }

    Ok(most_recent.map(|(_, iri)| iri))
}

/// Returns the IRI of the conversation with the most recent user message
/// among conversations that have a `foundation:handledBy` triple.
pub fn find_conversation_by_last_user_message(conn: &Connection) -> Result<Option<String>> {
    Individual::find_class_instance_ordered_by_child_timestamp(
        conn,
        "foundation:AIConversation",
        "foundation:handledBy",
        "foundation:partOfConversation",
        "foundation:sentAt",
        "foundation:role",
        "user",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::eavto::{store, Object, Triple};

    fn insert_conversation(conn: &mut Connection, iri: &str) {
        store::assert_triples(
            conn,
            &[
                Triple::new(iri, "rdf:type", Object::Iri(CONVERSATION_CLASS.to_string())),
                Triple::new(
                    iri,
                    "foundation:handledBy",
                    Object::Iri("foundation:Agent".to_string()),
                ),
            ],
            "test",
        )
        .unwrap();
    }

    fn insert_message(conn: &mut Connection, iri: &str, conversation_iri: &str, sent_at_ms: i64) {
        let sent_at = chrono::DateTime::from_timestamp_millis(sent_at_ms)
            .unwrap_or_default()
            .to_rfc3339();
        store::assert_triples(
            conn,
            &[
                Triple::new(
                    iri,
                    "rdf:type",
                    Object::Iri("foundation:AIConversationMessage".to_string()),
                ),
                Triple::new(
                    iri,
                    "foundation:partOfConversation",
                    Object::Iri(conversation_iri.to_string()),
                ),
                Triple::new(
                    iri,
                    "foundation:role",
                    Object::Literal {
                        value: "user".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(iri, "foundation:sentAt", Object::DateTime(sent_at)),
            ],
            "test",
        )
        .unwrap();
    }

    #[test]
    fn most_recent_active_conversation_ignores_retracted_parent() {
        let mut conn = setup_test_db();
        insert_conversation(&mut conn, "foundation:ActiveConversation");
        insert_message(
            &mut conn,
            "foundation:ActiveMessage",
            "foundation:ActiveConversation",
            1_000,
        );
        insert_conversation(&mut conn, "foundation:RetractedConversation");
        insert_message(
            &mut conn,
            "foundation:RetractedMessage",
            "foundation:RetractedConversation",
            2_000,
        );

        store::retract_triples(
            &mut conn,
            &[Triple::new(
                "foundation:RetractedConversation",
                "rdf:type",
                Object::Iri(CONVERSATION_CLASS.to_string()),
            )],
            "test",
        )
        .unwrap();

        assert_eq!(
            find_most_recent_active_conversation(&conn).unwrap(),
            Some("foundation:ActiveConversation".to_string()),
        );
        assert!(!is_active_conversation(
            &conn,
            "foundation:RetractedConversation"
        ));
    }

    #[test]
    fn human_conversation_requires_this_user_participant() {
        let mut conn = setup_test_db();
        insert_conversation(&mut conn, "foundation:HumanConversation");
        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "foundation:HumanConversation",
                PARTICIPANT_PROPERTY,
                Object::Iri(THIS_USER.to_string()),
            )],
            "test",
        )
        .unwrap();
        insert_conversation(&mut conn, "foundation:InternalConversation");
        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "foundation:InternalConversation",
                PARTICIPANT_PROPERTY,
                Object::Iri("foundation:LocalAIAssistant".to_string()),
            )],
            "test",
        )
        .unwrap();

        assert!(is_human_conversation(&conn, "foundation:HumanConversation").unwrap());
        assert!(!is_human_conversation(&conn, "foundation:InternalConversation").unwrap());
    }

    #[test]
    fn most_recent_active_conversation_returns_none_without_survivor() {
        let mut conn = setup_test_db();
        insert_conversation(&mut conn, "foundation:RetractedConversation");
        insert_message(
            &mut conn,
            "foundation:Message",
            "foundation:RetractedConversation",
            2_000,
        );
        store::retract_triples(
            &mut conn,
            &[Triple::new(
                "foundation:RetractedConversation",
                "rdf:type",
                Object::Iri(CONVERSATION_CLASS.to_string()),
            )],
            "test",
        )
        .unwrap();

        assert_eq!(find_most_recent_active_conversation(&conn).unwrap(), None);
    }

    #[test]
    fn most_recent_active_conversation_selects_newest_current_message() {
        let mut conn = setup_test_db();
        insert_conversation(&mut conn, "foundation:OlderConversation");
        insert_message(
            &mut conn,
            "foundation:OlderMessage",
            "foundation:OlderConversation",
            1_000,
        );
        insert_conversation(&mut conn, "foundation:NewerConversation");
        insert_message(
            &mut conn,
            "foundation:NewerMessage",
            "foundation:NewerConversation",
            2_000,
        );

        assert_eq!(
            find_most_recent_active_conversation(&conn).unwrap(),
            Some("foundation:NewerConversation".to_string()),
        );
    }
}
