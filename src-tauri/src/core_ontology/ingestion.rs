use crate::owl::{Class, ClassType, Connection, Individual, Object, Property, PropertyType};
use sha2::{Digest, Sha256};

const ORIGIN: &str = "core_ontology_sync";

/// Installs the ontology vocabulary used by the asynchronous content-ingestion queue.
pub fn sync_schema(conn: &mut Connection) -> Result<(), String> {
    for (iri, label, comment) in [
        (
            "foundation:IngestionSession",
            "Ingestion Session",
            "Durable context and cursor for ingesting one source conversation.",
        ),
        (
            "foundation:IngestionItem",
            "Ingestion Item",
            "A source content item queued for ontology-aware processing.",
        ),
        (
            "foundation:IngestionBatch",
            "Ingestion Batch",
            "An immutable batch of items processed together for one ingestion session.",
        ),
        (
            "foundation:IngestionAction",
            "Ingestion Action",
            "Durable result of one ontology-agent write tool within an ingestion batch.",
        ),
    ] {
        if !Class::exists(conn, iri) {
            Class::new(iri)
                .assert(
                    conn,
                    ClassType::OwlClass,
                    label,
                    "smart_toy",
                    Some("foundation:AbstractContinuant"),
                    ORIGIN,
                )
                .map_err(|error| error.to_string())?;
            let _ = comment;
        }
    }

    for (iri, label, comment, domain, range, kind) in [
        ("foundation:ingestionSourceKind", "Ingestion Source Kind", "Transport or subsystem that supplied the content.", "foundation:IngestionSession", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionConversationKey", "Ingestion Conversation Key", "Stable source conversation identifier used to group context.", "foundation:IngestionSession", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionCursor", "Ingestion Cursor", "Highest per-session sequence confirmed as processed.", "foundation:IngestionSession", "xsd:integer", PropertyType::DatatypeProperty),
        ("foundation:ingestionNextSequence", "Ingestion Next Sequence", "Next monotonically increasing item sequence for the session.", "foundation:IngestionSession", "xsd:integer", PropertyType::DatatypeProperty),
        ("foundation:ingestionSessionStatus", "Ingestion Session Status", "Current queue state for this session.", "foundation:IngestionSession", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionProgressStage", "Ingestion Progress Stage", "Last confirmed processing stage for this session; no estimated progress is stored.", "foundation:IngestionSession", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionAgentConversation", "Ingestion Agent Conversation", "Private AI conversation retaining ontology-agent context for this session.", "foundation:IngestionSession", "foundation:AIConversation", PropertyType::ObjectProperty),
        ("foundation:ingestionInternalSession", "Ingestion Internal Session", "Marks an AI conversation used internally by the ingestion worker so its prompts are not recursively queued.", "foundation:AIConversation", "foundation:IngestionSession", PropertyType::ObjectProperty),
        ("foundation:ingestionSession", "Ingestion Session", "Session that owns this content item or processing batch.", "foundation:IngestionItem", "foundation:IngestionSession", PropertyType::ObjectProperty),
        ("foundation:ingestionSourceEntity", "Ingestion Source Entity", "Ontology entity containing the received content.", "foundation:IngestionItem", "owl:Thing", PropertyType::ObjectProperty),
        ("foundation:ingestionStableKey", "Ingestion Stable Key", "Idempotency key for the source content item.", "foundation:IngestionItem", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionSequence", "Ingestion Sequence", "Monotonic sequence within the source conversation.", "foundation:IngestionItem", "xsd:integer", PropertyType::DatatypeProperty),
        ("foundation:ingestionItemStatus", "Ingestion Item Status", "Queue state for the content item.", "foundation:IngestionItem", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionBatch", "Ingestion Batch", "Batch currently or previously processing this item.", "foundation:IngestionItem", "foundation:IngestionBatch", PropertyType::ObjectProperty),
        ("foundation:ingestionBatchUpperBound", "Ingestion Batch Upper Bound", "Frozen last item sequence included in this batch.", "foundation:IngestionBatch", "xsd:integer", PropertyType::DatatypeProperty),
        ("foundation:ingestionBatchStatus", "Ingestion Batch Status", "Lifecycle state of the immutable batch.", "foundation:IngestionBatch", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionFailure", "Ingestion Failure", "Sanitized failure detail for a retryable or terminal queue failure.", "foundation:IngestionBatch", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionReceivedAt", "Ingestion Received At", "When the queue received the content item.", "foundation:IngestionItem", "xsd:dateTime", PropertyType::DatatypeProperty),
        ("foundation:ingestionActionBatch", "Ingestion Action Batch", "Batch that owns this idempotent write action.", "foundation:IngestionAction", "foundation:IngestionBatch", PropertyType::ObjectProperty),
        ("foundation:ingestionActionStatus", "Ingestion Action Status", "Processing state of this durable ontology-agent action.", "foundation:IngestionAction", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionActionTool", "Ingestion Action Tool", "Ontology write tool that produced this action.", "foundation:IngestionAction", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionActionInputHash", "Ingestion Action Input Hash", "Hash of canonical tool arguments used as the idempotency key.", "foundation:IngestionAction", "xsd:string", PropertyType::DatatypeProperty),
        ("foundation:ingestionActionResult", "Ingestion Action Result", "Serialized successful tool result replayed instead of repeating a committed write.", "foundation:IngestionAction", "xsd:string", PropertyType::DatatypeProperty),
    ] {
        if Property::get(conn, iri).map_err(|error| error.to_string())?.is_none() {
            Property::new(iri)
                .assert(
                    conn,
                    kind,
                    label,
                    Some(comment),
                    &[domain],
                    Some(range),
                    (range == "xsd:integer").then_some("unit:Count"),
                    ORIGIN,
                )
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Adds a source entity to the durable queue in the same transaction as its persistence.
/// Replaying a source item with the same stable key is a no-op.
pub fn enqueue_source_item(
    conn: &mut Connection,
    source_kind: &str,
    conversation_key: &str,
    source_entity: &str,
    stable_key: &str,
) -> Result<bool, String> {
    let session_iri = stable_iri("IngestionSession", &[source_kind, conversation_key]);
    let item_iri = stable_iri("IngestionItem", &[source_kind, stable_key]);
    if Individual::get(conn, &item_iri)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(false);
    }

    if Individual::get(conn, &session_iri)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        let session = Individual::new(&session_iri);
        session
            .assert(
                conn,
                "foundation:IngestionSession",
                "Content Ingestion",
                "smart_toy",
                "ingestion",
            )
            .map_err(|error| error.to_string())?;
        session
            .add_property(
                conn,
                "foundation:hasStatus",
                vec![Object::Iri("foundation:Pending".into())],
                "ingestion",
            )
            .map_err(|error| error.to_string())?;
        session
            .add_property(
                conn,
                "foundation:ingestionSourceKind",
                vec![literal(source_kind)],
                "ingestion",
            )
            .map_err(|error| error.to_string())?;
        session
            .add_property(
                conn,
                "foundation:ingestionConversationKey",
                vec![literal(conversation_key)],
                "ingestion",
            )
            .map_err(|error| error.to_string())?;
        session
            .add_property(
                conn,
                "foundation:ingestionCursor",
                vec![integer_value(0)],
                "ingestion",
            )
            .map_err(|error| error.to_string())?;
        session
            .add_property(
                conn,
                "foundation:ingestionNextSequence",
                vec![integer_value(1)],
                "ingestion",
            )
            .map_err(|error| error.to_string())?;
        session
            .add_property(
                conn,
                "foundation:ingestionSessionStatus",
                vec![literal("Pending")],
                "ingestion",
            )
            .map_err(|error| error.to_string())?;
        session
            .add_property(
                conn,
                "foundation:ingestionProgressStage",
                vec![literal("Waiting for a quiet conversation window")],
                "ingestion",
            )
            .map_err(|error| error.to_string())?;
    }

    let session = Individual::get(conn, &session_iri)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Ingestion session {session_iri} disappeared during enqueue"))?;
    let sequence = session
        .properties
        .iter()
        .find(|(predicate, _)| predicate == "foundation:ingestionNextSequence")
        .and_then(|(_, value)| object_string(value))
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(1);
    let ingestion_session = Individual::new(&session_iri);
    ingestion_session
        .add_property(
            conn,
            "foundation:ingestionNextSequence",
            vec![integer_value(sequence + 1)],
            "ingestion",
        )
        .map_err(|error| error.to_string())?;

    let item = Individual::new(&item_iri);
    item.assert(
        conn,
        "foundation:IngestionItem",
        "Queued Content",
        "mail",
        "ingestion",
    )
    .map_err(|error| error.to_string())?;
    item.add_property(
        conn,
        "foundation:hasStatus",
        vec![Object::Iri("foundation:Pending".into())],
        "ingestion",
    )
    .map_err(|error| error.to_string())?;
    item.add_property(
        conn,
        "foundation:ingestionSession",
        vec![Object::Iri(session_iri)],
        "ingestion",
    )
    .map_err(|error| error.to_string())?;
    item.add_property(
        conn,
        "foundation:ingestionSourceEntity",
        vec![Object::Iri(source_entity.to_string())],
        "ingestion",
    )
    .map_err(|error| error.to_string())?;
    item.add_property(
        conn,
        "foundation:ingestionStableKey",
        vec![literal(stable_key)],
        "ingestion",
    )
    .map_err(|error| error.to_string())?;
    item.add_property(
        conn,
        "foundation:ingestionSequence",
        vec![integer_value(sequence)],
        "ingestion",
    )
    .map_err(|error| error.to_string())?;
    item.add_property(
        conn,
        "foundation:ingestionItemStatus",
        vec![literal("Pending")],
        "ingestion",
    )
    .map_err(|error| error.to_string())?;
    item.add_property(
        conn,
        "foundation:ingestionReceivedAt",
        vec![Object::Literal {
            value: chrono::Utc::now().to_rfc3339(),
            datatype: Some("xsd:dateTime".into()),
            language: None,
        }],
        "ingestion",
    )
    .map_err(|error| error.to_string())?;
    Ok(true)
}

/// Lists current instances of an ontology class, including the empty-filter
/// case that `find_by_class_and_properties` intentionally treats as a no-op.
#[cfg(test)]
pub(crate) fn individuals_of_class(
    conn: &Connection,
    class_iri: &str,
) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(
            "SELECT DISTINCT subject FROM triples_current \
             WHERE predicate = 'rdf:type' AND object = ?1 ORDER BY subject",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([class_iri], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn stable_iri(kind: &str, parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    let digest = hash.finalize();
    format!(
        "foundation:{kind}_{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn literal(value: &str) -> Object {
    Object::Literal {
        value: value.to_string(),
        datatype: Some("xsd:string".into()),
        language: None,
    }
}

fn integer_value(value: i64) -> Object {
    Object::Integer(value)
}

fn object_string(value: &Object) -> Option<String> {
    match value {
        Object::Literal { value, .. } => Some(value.clone()),
        Object::Integer(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{enqueue_source_item, individuals_of_class, stable_iri, sync_schema};
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{get_literal_property, Class, ClassType, Individual, Property, PropertyType};

    fn test_connection() -> crate::owl::Connection {
        let mut conn = setup_test_db();
        crate::owl::seed_icon_library(&mut conn);
        sync_schema(&mut conn).unwrap();
        Class::new("foundation:Status")
            .assert(
                &mut conn,
                ClassType::OwlClass,
                "Status",
                "task_alt",
                None,
                "test",
            )
            .unwrap();
        Class::new("foundation:Email")
            .assert(
                &mut conn,
                ClassType::OwlClass,
                "Email",
                "mail",
                None,
                "test",
            )
            .unwrap();
        Property::new("foundation:hasStatus")
            .assert(
                &mut conn,
                PropertyType::ObjectProperty,
                "Has Status",
                None,
                &["foundation:IngestionSession", "foundation:IngestionItem"],
                Some("foundation:Status"),
                None,
                "test",
            )
            .unwrap();
        Individual::new("foundation:Pending")
            .assert(
                &mut conn,
                "foundation:Status",
                "Pending",
                "hourglass_empty",
                "test",
            )
            .unwrap();
        Individual::new("foundation:Email_fixture")
            .assert(&mut conn, "foundation:Email", "Test email", "mail", "test")
            .unwrap();
        conn
    }

    #[test]
    fn enqueue_is_idempotent_and_assigns_monotonic_sequences_per_conversation() {
        let mut conn = test_connection();
        let source = "foundation:Email_fixture";
        let session = stable_iri("IngestionSession", &["imap_email", "account:thread-42"]);
        let first_item = stable_iri("IngestionItem", &["imap_email", "account:message-1"]);
        let second_item = stable_iri("IngestionItem", &["imap_email", "account:message-2"]);

        assert!(enqueue_source_item(
            &mut conn,
            "imap_email",
            "account:thread-42",
            source,
            "account:message-1",
        )
        .unwrap());
        assert!(!enqueue_source_item(
            &mut conn,
            "imap_email",
            "account:thread-42",
            source,
            "account:message-1",
        )
        .unwrap());
        assert!(enqueue_source_item(
            &mut conn,
            "imap_email",
            "account:thread-42",
            source,
            "account:message-2",
        )
        .unwrap());

        let session_record = Individual::get(&conn, &session).unwrap().unwrap();
        assert_eq!(
            get_literal_property(&conn, &session, "foundation:ingestionNextSequence")
                .unwrap()
                .as_deref(),
            Some("3")
        );
        assert_eq!(
            get_literal_property(&conn, &session, "foundation:ingestionCursor")
                .unwrap()
                .as_deref(),
            Some("0")
        );
        assert_eq!(
            get_literal_property(&conn, &first_item, "foundation:ingestionSequence")
                .unwrap()
                .as_deref(),
            Some("1")
        );
        assert_eq!(
            get_literal_property(&conn, &second_item, "foundation:ingestionSequence")
                .unwrap()
                .as_deref(),
            Some("2")
        );
        assert!(session_record
            .properties
            .iter()
            .any(|(property, _)| property == "foundation:ingestionConversationKey"));
        assert_eq!(
            individuals_of_class(&conn, "foundation:IngestionSession").unwrap(),
            vec![session]
        );
        let mut expected_items = vec![first_item, second_item];
        expected_items.sort();
        assert_eq!(
            individuals_of_class(&conn, "foundation:IngestionItem").unwrap(),
            expected_items
        );
    }

    #[test]
    fn repeated_schema_sync_is_safe_for_existing_ontology() {
        let mut conn = test_connection();

        sync_schema(&mut conn).unwrap();
        sync_schema(&mut conn).unwrap();

        assert!(crate::owl::Class::exists(
            &conn,
            "foundation:IngestionSession"
        ));
        assert!(
            crate::owl::Property::get(&conn, "foundation:ingestionCursor")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn different_source_conversations_get_independent_ingestion_sessions() {
        let mut conn = test_connection();
        let first_session = stable_iri("IngestionSession", &["imap_email", "account:thread-100"]);
        let second_session = stable_iri("IngestionSession", &["imap_email", "account:thread-200"]);

        enqueue_source_item(
            &mut conn,
            "imap_email",
            "account:thread-100",
            "foundation:Email_fixture",
            "account:message-100",
        )
        .unwrap();
        enqueue_source_item(
            &mut conn,
            "imap_email",
            "account:thread-200",
            "foundation:Email_fixture",
            "account:message-200",
        )
        .unwrap();

        assert_ne!(first_session, second_session);
        assert!(Individual::get(&conn, &first_session).unwrap().is_some());
        assert!(Individual::get(&conn, &second_session).unwrap().is_some());
    }
}
