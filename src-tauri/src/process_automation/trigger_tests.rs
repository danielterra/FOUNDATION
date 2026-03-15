use super::*;
use crate::eavto::{store, Triple, Object};
use crate::eavto::test_helpers::setup_test_db;

async fn insert_triples(conn: &crate::owl::Connection, triples: &[Triple]) {
    store::assert_triples(conn, triples, "test").await.expect("Failed to insert triples");
}

#[tokio::test]
async fn test_find_processes_for_event_key_empty_db_returns_empty() {
    let conn = setup_test_db().await;
    let result = find_processes_for_event_key(&conn, "entity-created").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_find_processes_for_event_key_no_matching_key_returns_empty() {
    let conn = setup_test_db().await;
    insert_triples(&conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-updated".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ]).await;

    let result = find_processes_for_event_key(&conn, "entity-created").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_find_processes_for_event_key_missing_message_event_def_returns_empty() {
    let conn = setup_test_db().await;
    insert_triples(&conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ]).await;

    let result = find_processes_for_event_key(&conn, "entity-created").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_find_processes_for_event_key_missing_start_event_returns_empty() {
    let conn = setup_test_db().await;
    insert_triples(&conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:MsgDef1", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
    ]).await;

    let result = find_processes_for_event_key(&conn, "entity-created").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_find_processes_for_event_key_full_chain_returns_process() {
    let conn = setup_test_db().await;
    insert_triples(&conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:MsgDef1", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        Triple::new("foundation:MsgDef1", "foundation:messageEventOf", Object::Iri("foundation:Start1".to_string())),
        Triple::new("foundation:Proc1", "foundation:hasFlowNode", Object::Iri("foundation:Start1".to_string())),
    ]).await;

    let result = find_processes_for_event_key(&conn, "entity-created").await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "foundation:Proc1");
}

#[tokio::test]
async fn test_find_processes_for_event_key_multiple_processes_returned() {
    let conn = setup_test_db().await;
    insert_triples(&conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:MsgDef1", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        Triple::new("foundation:MsgDef1", "foundation:messageEventOf", Object::Iri("foundation:Start1".to_string())),
        Triple::new("foundation:MsgDef2", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        Triple::new("foundation:MsgDef2", "foundation:messageEventOf", Object::Iri("foundation:Start2".to_string())),
        Triple::new("foundation:Proc1", "foundation:hasFlowNode", Object::Iri("foundation:Start1".to_string())),
        Triple::new("foundation:Proc2", "foundation:hasFlowNode", Object::Iri("foundation:Start2".to_string())),
    ]).await;

    let result = find_processes_for_event_key(&conn, "entity-created").await.unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&"foundation:Proc1".to_string()));
    assert!(result.contains(&"foundation:Proc2".to_string()));
}

#[tokio::test]
async fn test_find_processes_for_event_key_different_key_not_matched() {
    let conn = setup_test_db().await;
    insert_triples(&conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:MsgDef1", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        Triple::new("foundation:MsgDef1", "foundation:messageEventOf", Object::Iri("foundation:Start1".to_string())),
        Triple::new("foundation:Proc1", "foundation:hasFlowNode", Object::Iri("foundation:Start1".to_string())),
    ]).await;

    let result = find_processes_for_event_key(&conn, "entity-updated").await.unwrap();
    assert!(result.is_empty());
}
