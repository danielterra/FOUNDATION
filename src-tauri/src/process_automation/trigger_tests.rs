use super::*;
use crate::eavto::{store, Triple, Object};
use crate::eavto::test_helpers::setup_test_db;

fn insert_triples(conn: &mut rusqlite::Connection, triples: &[Triple]) {
    store::assert_triples(conn, triples, "test").expect("Failed to insert triples");
}

#[test]
fn test_find_processes_for_event_key_empty_db_returns_empty() {
    let conn = setup_test_db();
    let result = find_processes_for_event_key(&conn, "entity-created").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_find_processes_for_event_key_no_matching_key_returns_empty() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-updated".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ]);

    let result = find_processes_for_event_key(&conn, "entity-created").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_find_processes_for_event_key_missing_message_event_def_returns_empty() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        // No MessageEventDefinition linking to EventType1
    ]);

    let result = find_processes_for_event_key(&conn, "entity-created").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_find_processes_for_event_key_missing_start_event_returns_empty() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:MsgDef1", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        // No messageEventOf linking MsgDef1 to a StartEvent
    ]);

    let result = find_processes_for_event_key(&conn, "entity-created").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_find_processes_for_event_key_full_chain_returns_process() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:MsgDef1", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        Triple::new("foundation:MsgDef1", "foundation:messageEventOf", Object::Iri("foundation:Start1".to_string())),
        Triple::new("foundation:Start1", "foundation:partOfProcess", Object::Iri("foundation:Proc1".to_string())),
    ]);

    let result = find_processes_for_event_key(&conn, "entity-created").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "foundation:Proc1");
}

#[test]
fn test_find_processes_for_event_key_multiple_processes_returned() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:MsgDef1", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        Triple::new("foundation:MsgDef1", "foundation:messageEventOf", Object::Iri("foundation:Start1".to_string())),
        Triple::new("foundation:MsgDef2", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        Triple::new("foundation:MsgDef2", "foundation:messageEventOf", Object::Iri("foundation:Start2".to_string())),
        Triple::new("foundation:Start1", "foundation:partOfProcess", Object::Iri("foundation:Proc1".to_string())),
        Triple::new("foundation:Start2", "foundation:partOfProcess", Object::Iri("foundation:Proc2".to_string())),
    ]);

    let result = find_processes_for_event_key(&conn, "entity-created").unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&"foundation:Proc1".to_string()));
    assert!(result.contains(&"foundation:Proc2".to_string()));
}

#[test]
fn test_find_processes_for_event_key_different_key_not_matched() {
    let mut conn = setup_test_db();
    insert_triples(&mut conn, &[
        Triple::new("foundation:EventType1", "foundation:eventKey", Object::Literal {
            value: "entity-created".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:MsgDef1", "foundation:eventType", Object::Iri("foundation:EventType1".to_string())),
        Triple::new("foundation:MsgDef1", "foundation:messageEventOf", Object::Iri("foundation:Start1".to_string())),
        Triple::new("foundation:Start1", "foundation:partOfProcess", Object::Iri("foundation:Proc1".to_string())),
    ]);

    let result = find_processes_for_event_key(&conn, "entity-updated").unwrap();
    assert!(result.is_empty());
}
