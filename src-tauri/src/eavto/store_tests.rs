use super::*;
use crate::eavto::test_helpers::{
    setup_test_db, create_test_triples, assert_triple_exists, get_active_triple_count,
};

async fn query_count(conn: &Connection, sql: &str) -> i64 {
    let mut stmt = conn.prepare(sql).await.expect("prepare failed");
    let row = stmt.query_row(()).await.expect("query failed");
    row.get_value(0).unwrap().as_integer().copied().unwrap_or(0)
}

async fn query_count_param(conn: &Connection, sql: &str, param: &str) -> i64 {
    let mut stmt = conn.prepare(sql).await.expect("prepare failed");
    let row = stmt.query_row(turso::params![param]).await.expect("query failed");
    row.get_value(0).unwrap().as_integer().copied().unwrap_or(0)
}

async fn query_bool(conn: &Connection, sql: &str) -> bool {
    query_count(conn, sql).await > 0
}

async fn query_bool_param(conn: &Connection, sql: &str, param: &str) -> bool {
    query_count_param(conn, sql, param).await > 0
}

async fn query_string(conn: &Connection, sql: &str) -> String {
    let mut stmt = conn.prepare(sql).await.expect("prepare failed");
    let row = stmt.query_row(()).await.expect("query failed");
    row.get_value(0).unwrap().as_text().map_or("", |v| v).to_string()
}

#[tokio::test]
async fn test_assert_triples_basic() {
    let conn = setup_test_db().await;
    let triples = create_test_triples();

    let tx_id = assert_triples(&conn, &triples, "test_origin")
        .await
        .expect("Failed to assert triples");

    assert!(tx_id > 0);
    assert_eq!(get_active_triple_count(&conn).await, 3);
}

#[tokio::test]
async fn test_assert_triples_creates_transaction() {
    let conn = setup_test_db().await;
    let triples = create_test_triples();

    let tx_id = assert_triples(&conn, &triples, "test_origin").await.unwrap();

    let tx_count = query_count_param(
        &conn,
        "SELECT COUNT(*) FROM transactions WHERE tx = ?",
        &tx_id.to_string(),
    ).await;

    assert_eq!(tx_count, 1);
}

#[tokio::test]
async fn test_assert_triples_creates_origin() {
    let conn = setup_test_db().await;
    let triples = create_test_triples();

    assert_triples(&conn, &triples, "new_origin").await.unwrap();

    let origin_exists = query_bool(
        &conn,
        "SELECT COUNT(*) > 0 FROM origins WHERE name = 'new_origin'",
    ).await;

    assert!(origin_exists);
}

#[tokio::test]
async fn test_assert_triples_with_different_object_types() {
    let conn = setup_test_db().await;

    let triples = vec![
        Triple {
            subject: "test:Subject1".to_string(),
            predicate: "test:hasIri".to_string(),
            object: Object::Iri("test:Object1".to_string()),
            tx: 0,
            created_at: 1000,
            origin_id: 1,
            retracted: false,
        },
        Triple {
            subject: "test:Subject2".to_string(),
            predicate: "test:hasInteger".to_string(),
            object: Object::Integer(42),
            tx: 0,
            created_at: 1000,
            origin_id: 1,
            retracted: false,
        },
        Triple {
            subject: "test:Subject3".to_string(),
            predicate: "test:hasNumber".to_string(),
            object: Object::Number(3.14),
            tx: 0,
            created_at: 1000,
            origin_id: 1,
            retracted: false,
        },
        Triple {
            subject: "test:Subject4".to_string(),
            predicate: "test:hasBoolean".to_string(),
            object: Object::Boolean(true),
            tx: 0,
            created_at: 1000,
            origin_id: 1,
            retracted: false,
        },
    ];

    assert_triples(&conn, &triples, "test").await.unwrap();

    assert_triple_exists(&conn, "test:Subject1", "test:hasIri").await;
    assert_triple_exists(&conn, "test:Subject2", "test:hasInteger").await;
    assert_triple_exists(&conn, "test:Subject3", "test:hasNumber").await;
    assert_triple_exists(&conn, "test:Subject4", "test:hasBoolean").await;
}

#[tokio::test]
async fn test_retract_triples() {
    let conn = setup_test_db().await;
    let triples = create_test_triples();

    assert_triples(&conn, &triples, "test").await.unwrap();
    assert_eq!(get_active_triple_count(&conn).await, 3);

    let to_retract = vec![triples[0].clone()];
    let retract_tx_id = retract_triples(&conn, &to_retract, "test").await.unwrap();

    assert!(retract_tx_id > 0);
    assert_eq!(get_active_triple_count(&conn).await, 2);
}

#[tokio::test]
async fn test_retract_triples_multiple() {
    let conn = setup_test_db().await;
    let triples = create_test_triples();

    assert_triples(&conn, &triples, "test").await.unwrap();
    assert_eq!(get_active_triple_count(&conn).await, 3);

    retract_triples(&conn, &triples, "test").await.unwrap();
    assert_eq!(get_active_triple_count(&conn).await, 0);
}

#[tokio::test]
async fn test_retract_nonexistent_triple_does_not_error() {
    let conn = setup_test_db().await;

    let triples = vec![Triple {
        subject: "nonexistent:Subject".to_string(),
        predicate: "nonexistent:predicate".to_string(),
        object: Object::Iri("nonexistent:Object".to_string()),
        tx: 0,
        created_at: 1000,
        origin_id: 1,
        retracted: false,
    }];

    let result = retract_triples(&conn, &triples, "test").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_now_millis() {
    let ts = now_millis();
    assert!(ts > 0);
    assert!(ts > 1577836800000);
}

#[tokio::test]
async fn test_assert_replaces_old_values() {
    let conn = setup_test_db().await;

    let email1 = vec![Triple {
        subject: "test:Person1".to_string(),
        predicate: "test:email".to_string(),
        object: Object::Literal {
            value: "john@example.com".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        },
        tx: 0,
        created_at: 1000,
        origin_id: 1,
        retracted: false,
    }];
    assert_triples(&conn, &email1, "test").await.unwrap();

    let email2 = vec![Triple {
        subject: "test:Person1".to_string(),
        predicate: "test:email".to_string(),
        object: Object::Literal {
            value: "john@work.com".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        },
        tx: 0,
        created_at: 2000,
        origin_id: 1,
        retracted: false,
    }];
    assert_triples(&conn, &email2, "test").await.unwrap();

    let active = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples \
         WHERE subject = 'test:Person1' AND predicate = 'test:email' AND retracted = 0",
    ).await;
    assert_eq!(active, 1);

    let total = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples \
         WHERE subject = 'test:Person1' AND predicate = 'test:email'",
    ).await;
    assert_eq!(total, 2);

    let active_value = query_string(
        &conn,
        "SELECT object_value FROM triples \
         WHERE subject = 'test:Person1' AND predicate = 'test:email' AND retracted = 0",
    ).await;
    assert_eq!(active_value, "john@work.com");
}

#[tokio::test]
async fn test_assert_same_value_twice_is_noop() {
    let conn = setup_test_db().await;

    let triple = vec![Triple {
        subject: "test:Thing".to_string(),
        predicate: "rdfs:label".to_string(),
        object: Object::Literal {
            value: "Hello".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        },
        tx: 0,
        created_at: 0,
        origin_id: 1,
        retracted: false,
    }];

    assert_triples(&conn, &triple, "test").await.unwrap();
    let total_before = query_count(&conn, "SELECT COUNT(*) FROM triples").await;

    let tx_id = assert_triples(&conn, &triple, "test").await.unwrap();
    assert_eq!(tx_id, 0, "second assert with same value must be a no-op");

    let total_after = query_count(&conn, "SELECT COUNT(*) FROM triples").await;
    assert_eq!(total_before, total_after, "no new rows should be written on no-op");

    let tx_count = query_count(&conn, "SELECT COUNT(*) FROM transactions").await;
    assert_eq!(tx_count, 1, "no new transaction record should be created on no-op");
}

#[tokio::test]
async fn test_assert_different_value_does_retract_and_insert() {
    let conn = setup_test_db().await;

    assert_triples(&conn, &[Triple {
        subject: "test:Thing".to_string(),
        predicate: "rdfs:label".to_string(),
        object: Object::Literal { value: "Old".to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    }], "test").await.unwrap();

    let tx_id = assert_triples(&conn, &[Triple {
        subject: "test:Thing".to_string(),
        predicate: "rdfs:label".to_string(),
        object: Object::Literal { value: "New".to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    }], "test").await.unwrap();

    assert!(tx_id > 0, "changing a value must create a real transaction");
    let active = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples WHERE subject='test:Thing' AND retracted=0",
    ).await;
    assert_eq!(active, 1);
    let active_value = query_string(
        &conn,
        "SELECT object_value FROM triples WHERE subject='test:Thing' AND retracted=0",
    ).await;
    assert_eq!(active_value, "New");
}

#[tokio::test]
async fn test_append_same_value_twice_is_noop() {
    let conn = setup_test_db().await;

    let triple = vec![Triple {
        subject: "test:Thing".to_string(),
        predicate: "foundation:tag".to_string(),
        object: Object::Literal {
            value: "rust".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    }];

    append_triples(&conn, &triple, "test").await.unwrap();
    let total_before = query_count(&conn, "SELECT COUNT(*) FROM triples").await;

    let tx_id = append_triples(&conn, &triple, "test").await.unwrap();
    assert_eq!(tx_id, 0, "second append with same value must be a no-op");

    let total_after = query_count(&conn, "SELECT COUNT(*) FROM triples").await;
    assert_eq!(total_before, total_after, "no new rows should be written on no-op");
}

#[tokio::test]
async fn test_assert_iri_same_value_is_noop() {
    let conn = setup_test_db().await;

    let triple = vec![Triple {
        subject: "test:Thing".to_string(),
        predicate: "rdf:type".to_string(),
        object: Object::Iri("foundation:Task".to_string()),
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    }];

    assert_triples(&conn, &triple, "test").await.unwrap();
    let tx_id = assert_triples(&conn, &triple, "test").await.unwrap();
    assert_eq!(tx_id, 0);
}

#[tokio::test]
async fn test_assert_multivalue_partial_overlap_only_changes_diff() {
    let conn = setup_test_db().await;

    let mk = |v: &str| Triple {
        subject: "test:Thing".to_string(),
        predicate: "foundation:tag".to_string(),
        object: Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    };

    append_triples(&conn, &[mk("A"), mk("B")], "test").await.unwrap();
    assert_triples(&conn, &[mk("A"), mk("C")], "test").await.unwrap();

    let mut stmt = conn.prepare(
        "SELECT object_value FROM triples WHERE subject='test:Thing' AND predicate='foundation:tag' AND retracted=0 ORDER BY object_value"
    ).await.unwrap();
    let mut rows = stmt.query(()).await.unwrap();
    let mut active = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        let v = row.get_value(0).unwrap().as_text().map_or("", |v| v).to_string();
        active.push(v);
    }
    assert_eq!(active, vec!["A", "C"]);

    let a_rows = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples WHERE subject='test:Thing' AND predicate='foundation:tag' AND object_value='A'",
    ).await;
    assert_eq!(a_rows, 1, "unchanged value A must not be duplicated");
}
