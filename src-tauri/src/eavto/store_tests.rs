use super::*;
use crate::eavto::test_helpers::{
    setup_test_db, create_test_triples, assert_triple_exists, get_active_triple_count,
};

#[test]
fn test_assert_triples_basic() {
    let mut conn = setup_test_db();
    let triples = create_test_triples();

    let tx_id = assert_triples(&mut conn, &triples, "test_origin")
        .expect("Failed to assert triples");

    assert!(tx_id > 0);
    assert_eq!(get_active_triple_count(&conn), 3);
}

#[test]
fn test_assert_triples_creates_transaction() {
    let mut conn = setup_test_db();
    let triples = create_test_triples();

    let tx_id = assert_triples(&mut conn, &triples, "test_origin").unwrap();

    let tx_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM transactions WHERE tx = ?", [tx_id], |row| row.get(0))
        .unwrap();

    assert_eq!(tx_count, 1);
}

#[test]
fn test_assert_triples_creates_origin() {
    let mut conn = setup_test_db();
    let triples = create_test_triples();

    assert_triples(&mut conn, &triples, "new_origin").unwrap();

    let origin_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM origins WHERE name = 'new_origin'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(origin_exists);
}

#[test]
fn test_assert_triples_with_different_object_types() {
    let mut conn = setup_test_db();

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

    assert_triples(&mut conn, &triples, "test").unwrap();

    assert_triple_exists(&conn, "test:Subject1", "test:hasIri");
    assert_triple_exists(&conn, "test:Subject2", "test:hasInteger");
    assert_triple_exists(&conn, "test:Subject3", "test:hasNumber");
    assert_triple_exists(&conn, "test:Subject4", "test:hasBoolean");
}

#[test]
fn test_retract_triples() {
    let mut conn = setup_test_db();
    let triples = create_test_triples();

    // Assert triples first
    assert_triples(&mut conn, &triples, "test").unwrap();
    assert_eq!(get_active_triple_count(&conn), 3);

    // Retract one triple
    let to_retract = vec![triples[0].clone()];
    let retract_tx_id = retract_triples(&mut conn, &to_retract, "test").unwrap();

    assert!(retract_tx_id > 0);
    assert_eq!(get_active_triple_count(&conn), 2); // One should be retracted
}

#[test]
fn test_retract_triples_multiple() {
    let mut conn = setup_test_db();
    let triples = create_test_triples();

    assert_triples(&mut conn, &triples, "test").unwrap();
    assert_eq!(get_active_triple_count(&conn), 3);

    // Retract all triples
    retract_triples(&mut conn, &triples, "test").unwrap();
    assert_eq!(get_active_triple_count(&conn), 0);
}

#[test]
fn test_retract_nonexistent_triple_does_not_error() {
    let mut conn = setup_test_db();

    let triples = vec![Triple {
        subject: "nonexistent:Subject".to_string(),
        predicate: "nonexistent:predicate".to_string(),
        object: Object::Iri("nonexistent:Object".to_string()),
        tx: 0,
        created_at: 1000,
        origin_id: 1,
        retracted: false,
    }];

    // Should not error even though triple doesn't exist
    let result = retract_triples(&mut conn, &triples, "test");
    assert!(result.is_ok());
}

#[test]
fn test_get_or_create_origin_existing() {
    let mut conn = setup_test_db();
    let tx = conn.transaction().unwrap();

    // Origin "test" should already exist from setup_test_db
    let id1 = get_or_create_origin(&tx, "test").unwrap();
    let id2 = get_or_create_origin(&tx, "test").unwrap();

    assert_eq!(id1, id2); // Should return same ID
}

#[test]
fn test_get_or_create_origin_new() {
    let mut conn = setup_test_db();
    let tx = conn.transaction().unwrap();

    let id = get_or_create_origin(&tx, "brand_new_origin").unwrap();
    assert!(id > 0);
}

#[test]
fn test_now_millis() {
    let ts = now_millis();
    assert!(ts > 0);

    // Should be a reasonable timestamp (after 2020)
    assert!(ts > 1577836800000); // Jan 1, 2020 in milliseconds
}

#[test]
fn test_assert_replaces_old_values() {
    let mut conn = setup_test_db();

    // Add first email
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
    assert_triples(&mut conn, &email1, "test").unwrap();

    // Add second email
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
    assert_triples(&mut conn, &email2, "test").unwrap();

    // Should have only 1 active email (the latest one)
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current \
         WHERE subject = 'test:Person1' AND predicate = 'test:email'",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(active, 1);

    // Should have 2 total rows in history: original group + new group (no retraction row under group semantics)
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples \
         WHERE subject = 'test:Person1' AND predicate = 'test:email'",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(total, 2, "expected 2 total rows: original group + new group (no retraction row under group semantics)");

    // Verify the active one is the latest
    let active_value: String = conn.query_row(
        "SELECT object_value FROM triples_current \
         WHERE subject = 'test:Person1' AND predicate = 'test:email'",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(active_value, "john@work.com");
}

#[test]
fn test_assert_same_value_twice_is_noop() {
    let mut conn = setup_test_db();

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

    assert_triples(&mut conn, &triple, "test").unwrap();
    let total_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM triples", [], |r| r.get(0))
        .unwrap();

    let tx_id = assert_triples(&mut conn, &triple, "test").unwrap();
    assert_eq!(tx_id, 0, "second assert with same value must be a no-op");

    let total_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM triples", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total_before, total_after, "no new rows should be written on no-op");

    let tx_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tx_count, 1, "no new transaction record should be created on no-op");
}

#[test]
fn test_assert_different_value_does_retract_and_insert() {
    let mut conn = setup_test_db();

    assert_triples(&mut conn, &[Triple {
        subject: "test:Thing".to_string(),
        predicate: "rdfs:label".to_string(),
        object: Object::Literal { value: "Old".to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    }], "test").unwrap();

    let tx_id = assert_triples(&mut conn, &[Triple {
        subject: "test:Thing".to_string(),
        predicate: "rdfs:label".to_string(),
        object: Object::Literal { value: "New".to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    }], "test").unwrap();

    assert!(tx_id > 0, "changing a value must create a real transaction");
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject='test:Thing'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(active, 1);
    let active_value: String = conn.query_row(
        "SELECT object_value FROM triples_current WHERE subject='test:Thing'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(active_value, "New");
}

#[test]
fn test_append_same_value_twice_is_noop() {
    let mut conn = setup_test_db();

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

    append_triples(&mut conn, &triple, "test").unwrap();
    let total_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM triples", [], |r| r.get(0))
        .unwrap();

    let tx_id = append_triples(&mut conn, &triple, "test").unwrap();
    assert_eq!(tx_id, 0, "second append with same value must be a no-op");

    let total_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM triples", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total_before, total_after, "no new rows should be written on no-op");
}

#[test]
fn test_assert_iri_same_value_is_noop() {
    let mut conn = setup_test_db();

    let triple = vec![Triple {
        subject: "test:Thing".to_string(),
        predicate: "rdf:type".to_string(),
        object: Object::Iri("foundation:Task".to_string()),
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    }];

    assert_triples(&mut conn, &triple, "test").unwrap();
    let tx_id = assert_triples(&mut conn, &triple, "test").unwrap();
    assert_eq!(tx_id, 0);
}

#[test]
fn test_assert_multivalue_partial_overlap_only_changes_diff() {
    let mut conn = setup_test_db();

    let mk = |v: &str| Triple {
        subject: "test:Thing".to_string(),
        predicate: "foundation:tag".to_string(),
        object: Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    };

    // Start with [A, B]
    append_triples(&mut conn, &[mk("A"), mk("B")], "test").unwrap();

    // Assert [A, C] — B should be retracted, C inserted, A unchanged
    assert_triples(&mut conn, &[mk("A"), mk("C")], "test").unwrap();

    let active: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT object_value FROM triples_current WHERE subject='test:Thing' AND predicate='foundation:tag' ORDER BY object_value"
        ).unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    };
    assert_eq!(active, vec!["A", "C"]);

    // A appears in both the original group (tx=1) and the new group (tx=2) under group semantics
    let a_rows: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='test:Thing' AND predicate='foundation:tag' AND object_value='A'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(a_rows, 2, "A appears in original group (tx=1) and new group (tx=2) under group semantics");
}

// ── is_current invariant tests ────────────────────────────────────────────────

#[test]
fn test_assert_supersedes_sets_is_current() {
    let mut conn = setup_test_db();

    let mk = |v: &str| Triple {
        subject: "ex:S".to_string(),
        predicate: "ex:p".to_string(),
        object: Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    };

    assert_triples(&mut conn, &[mk("old")], "test").unwrap();
    assert_triples(&mut conn, &[mk("new")], "test").unwrap();

    let old_current: i64 = conn.query_row(
        "SELECT is_current FROM triples WHERE subject='ex:S' AND predicate='ex:p' AND object_value='old'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(old_current, 0, "superseded row must have is_current=0");

    let new_current: i64 = conn.query_row(
        "SELECT is_current FROM triples WHERE subject='ex:S' AND predicate='ex:p' AND object_value='new'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(new_current, 1, "new row must have is_current=1");

    let view_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject='ex:S' AND predicate='ex:p'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(view_count, 1, "view must show only the current row");

    let view_val: String = conn.query_row(
        "SELECT object_value FROM triples_current WHERE subject='ex:S' AND predicate='ex:p'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(view_val, "new");
}

#[test]
fn test_multivalue_group_all_is_current() {
    let mut conn = setup_test_db();

    let mk = |v: &str| Triple {
        subject: "ex:S".to_string(),
        predicate: "ex:tags".to_string(),
        object: Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    };

    append_triples(&mut conn, &[mk("A"), mk("B")], "test").unwrap();
    assert_triples(&mut conn, &[mk("A"), mk("C")], "test").unwrap();

    let current_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='ex:S' AND predicate='ex:tags' AND is_current=1",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(current_count, 2, "new group (A+C) must have both rows as is_current=1");

    let old_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='ex:S' AND predicate='ex:tags' AND is_current=0",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(old_count, 2, "old group (A+B from first append) must be demoted");
}

#[test]
fn test_retract_tombstone_is_current_view_empty() {
    let mut conn = setup_test_db();

    let t = Triple {
        subject: "ex:S".to_string(),
        predicate: "ex:p".to_string(),
        object: Object::Literal { value: "v".to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    };
    assert_triples(&mut conn, &[t.clone()], "test").unwrap();
    retract_triples(&mut conn, &[t], "test").unwrap();

    let tombstone: (i64, i64) = conn.query_row(
        "SELECT is_current, retracted FROM triples ORDER BY tx DESC LIMIT 1",
        [], |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    assert_eq!(tombstone, (1, 1), "tombstone must be is_current=1 AND retracted=1");

    let view_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject='ex:S' AND predicate='ex:p'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(view_count, 0, "view must be empty after retraction");

    let old_rows: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='ex:S' AND predicate='ex:p' AND is_current=0",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(old_rows, 1, "original active row must be demoted");
}

#[test]
fn test_reassert_after_retract_is_current() {
    let mut conn = setup_test_db();

    let t = Triple {
        subject: "ex:S".to_string(),
        predicate: "ex:p".to_string(),
        object: Object::Literal { value: "v".to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    };
    assert_triples(&mut conn, &[t.clone()], "test").unwrap();
    retract_triples(&mut conn, &[t.clone()], "test").unwrap();
    assert_triples(&mut conn, &[t], "test").unwrap();

    let view_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject='ex:S' AND predicate='ex:p'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(view_count, 1, "view must show the re-asserted value");

    let current_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='ex:S' AND predicate='ex:p' AND is_current=1",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(current_count, 1, "exactly one row must be is_current=1 after re-assert");
}

#[test]
fn test_append_extends_group_is_current() {
    let mut conn = setup_test_db();

    let mk = |v: &str| Triple {
        subject: "ex:S".to_string(),
        predicate: "ex:tags".to_string(),
        object: Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None },
        tx: 0, created_at: 0, origin_id: 1, retracted: false,
    };

    append_triples(&mut conn, &[mk("A"), mk("B")], "test").unwrap();
    append_triples(&mut conn, &[mk("C")], "test").unwrap();

    let current: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT object_value FROM triples_current WHERE subject='ex:S' AND predicate='ex:tags' ORDER BY object_value"
        ).unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    };
    assert_eq!(current, vec!["A", "B", "C"], "append must include prior values copied forward");

    let old_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject='ex:S' AND predicate='ex:tags' AND is_current=0",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(old_count, 2, "original A+B rows must be demoted after second append");
}

#[test]
fn test_migrate_is_current_backfill_and_idempotent() {
    use crate::eavto::connection::migrate_is_current;

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("
        CREATE TABLE origins (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE);
        CREATE TABLE transactions (tx INTEGER PRIMARY KEY AUTOINCREMENT, origin TEXT NOT NULL, created_at INTEGER NOT NULL);
        CREATE TABLE triples (
            subject TEXT NOT NULL, predicate TEXT NOT NULL,
            object TEXT, object_value TEXT, object_type TEXT NOT NULL,
            object_datatype TEXT, object_language TEXT,
            object_number REAL, object_integer INTEGER, object_boolean INTEGER,
            tx INTEGER NOT NULL, origin_id INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL DEFAULT 0, retracted INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO origins (name) VALUES ('test');
        INSERT INTO transactions (origin, created_at) VALUES ('test', 0);
        INSERT INTO transactions (origin, created_at) VALUES ('test', 1);
        INSERT INTO triples (subject, predicate, object_value, object_type, tx, retracted)
            VALUES ('ex:S', 'ex:p', 'old', 'literal', 1, 0);
        INSERT INTO triples (subject, predicate, object_value, object_type, tx, retracted)
            VALUES ('ex:S', 'ex:p', 'new', 'literal', 2, 0);
    ").unwrap();

    migrate_is_current(&conn).unwrap();

    let old_ic: i64 = conn.query_row(
        "SELECT is_current FROM triples WHERE object_value='old'", [], |r| r.get(0),
    ).unwrap();
    assert_eq!(old_ic, 0, "old row must be demoted");

    let new_ic: i64 = conn.query_row(
        "SELECT is_current FROM triples WHERE object_value='new'", [], |r| r.get(0),
    ).unwrap();
    assert_eq!(new_ic, 1, "new row must be current");

    migrate_is_current(&conn).unwrap();

    let count_after: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE is_current=0", [], |r| r.get(0),
    ).unwrap();
    assert_eq!(count_after, 1, "idempotent: second call must not change state");
}

#[test]
fn test_assert_triples_uses_savepoint_in_batch() {
    let mut conn = setup_test_db();

    // Set the batch flag and start a raw outer transaction (as batch_operations does)
    let _guard = enter_batch_transaction();
    conn.execute_batch("BEGIN").unwrap();

    let triples = vec![Triple {
        subject: "test:ThingA".to_string(),
        predicate: "test:name".to_string(),
        object: Object::Literal {
            value: "Thing A".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        },
        tx: 0,
        created_at: 1000,
        origin_id: 1,
        retracted: false,
    }];

    // assert_triples should use SAVEPOINT (not BEGIN) because IN_BATCH_TX is true
    assert_triples(&mut conn, &triples, "test").unwrap();

    // Triple is visible within the outer transaction
    let count_in_tx: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject = 'test:ThingA' AND retracted = 0",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(count_in_tx, 1);

    // Rollback the outer transaction — all changes including the savepoint disappear
    conn.execute_batch("ROLLBACK").unwrap();
    drop(_guard);

    let count_after_rollback: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject = 'test:ThingA' AND retracted = 0",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(count_after_rollback, 0, "Triple must be rolled back atomically");
}
