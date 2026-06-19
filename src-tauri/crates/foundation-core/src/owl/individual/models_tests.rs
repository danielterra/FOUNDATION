use super::list_ai_models_as_of;
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};

// ── helpers ──────────────────────────────────────────────────────────────────

const TYPE_PRED: &str = "rdf:type";
const OFFERED_BY: &str = "test:offeredBy";
const MODEL_CLASS: &str = "test:AIModel";
const IS_DEFAULT: &str = "test:isDefaultModel";
const LABEL_PRED: &str = "rdfs:label";
const ID_PRED: &str = "test:modelIdentifier";
const VER_PRED: &str = "test:modelVersion";
const DESC_PRED: &str = "test:description";
const SVC_A: &str = "test:ServiceA";

fn seed_model(conn: &mut rusqlite::Connection, iri: &str, label: &str, service: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, TYPE_PRED, Object::Iri(MODEL_CLASS.to_string())),
        Triple::new(iri, OFFERED_BY, Object::Iri(service.to_string())),
        Triple::new(iri, LABEL_PRED, Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new(iri, ID_PRED, Object::Literal {
            value: iri.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();
}

fn snapshot(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT MAX(tx) FROM transactions", [], |r| r.get(0))
        .unwrap_or(1)
}

fn call(
    conn: &rusqlite::Connection,
    service: Option<&str>,
    snap: i64,
    limit: i64,
    offset: i64,
) -> (Vec<super::AiModelRow>, bool) {
    list_ai_models_as_of(
        conn,
        service,
        OFFERED_BY,
        TYPE_PRED,
        MODEL_CLASS,
        IS_DEFAULT,
        LABEL_PRED,
        ID_PRED,
        VER_PRED,
        DESC_PRED,
        snap,
        limit,
        offset,
    )
    .unwrap()
}

// ── Parte A coverage — is_default via object_value='true' ────────────────────

#[test]
fn test_list_ai_models_is_default_object_value_true() {
    let mut conn = setup_test_db();
    seed_model(&mut conn, "test:ModelDefault", "Default", SVC_A);
    seed_model(&mut conn, "test:ModelOther", "Other", SVC_A);

    // Grava is_default via Object::Boolean — preenche object_value='true' E object_boolean=1
    store::assert_triples(&mut conn, &[
        Triple::new("test:ModelDefault", IS_DEFAULT, Object::Boolean(true)),
    ], "test").unwrap();

    let snap = snapshot(&conn);
    let (rows, _) = call(&conn, Some(SVC_A), snap, 10, 0);
    assert!(!rows.is_empty(), "deve retornar modelos");
    assert_eq!(rows[0].subject, "test:ModelDefault", "default deve ser o primeiro (is_default DESC)");
    assert!(rows[0].is_default, "is_default deve ser true");
    assert!(!rows[1].is_default, "outros modelos: is_default=false");
}

// ── Parte A coverage — is_default via object_boolean=1 sem object_value ──────

#[test]
fn test_list_ai_models_is_default_object_boolean_only() {
    let mut conn = setup_test_db();
    seed_model(&mut conn, "test:ModelBool", "BoolDefault", SVC_A);
    seed_model(&mut conn, "test:ModelNoBool", "NoBool", SVC_A);

    // Insere diretamente com object_boolean=1 e object_value=NULL para simular dado legado
    conn.execute(
        "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
        [],
    ).unwrap();
    let tx_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO triples (subject, predicate, object_boolean, object_value, object_type, \
         object_datatype, origin_id, tx, created_at, retracted, is_current) \
         VALUES ('test:ModelBool', 'test:isDefaultModel', 1, NULL, 'literal', 'xsd:boolean', 1, ?, 0, 0, 1)",
        rusqlite::params![tx_id],
    ).unwrap();

    let snap = snapshot(&conn);
    let (rows, _) = call(&conn, Some(SVC_A), snap, 10, 0);
    let default_row = rows.iter().find(|r| r.subject == "test:ModelBool")
        .expect("ModelBool deve estar presente");
    assert!(default_row.is_default, "is_default via object_boolean=1 deve resultar em true");
    assert_eq!(rows[0].subject, "test:ModelBool", "default deve subir ao topo");
}

// ── membership: por service_iri ───────────────────────────────────────────────

#[test]
fn test_list_ai_models_membership_by_service() {
    let mut conn = setup_test_db();
    seed_model(&mut conn, "test:MA", "ModelA", SVC_A);
    seed_model(&mut conn, "test:MB", "ModelB", "test:ServiceB");

    let snap = snapshot(&conn);
    let (rows, _) = call(&conn, Some(SVC_A), snap, 10, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].subject, "test:MA");
}

// ── membership: por type (sem service_iri) ───────────────────────────────────

#[test]
fn test_list_ai_models_membership_by_type() {
    let mut conn = setup_test_db();
    seed_model(&mut conn, "test:MC", "ModelC", SVC_A);
    seed_model(&mut conn, "test:MD", "ModelD", SVC_A);

    let snap = snapshot(&conn);
    let (rows, _) = call(&conn, None, snap, 10, 0);
    assert_eq!(rows.len(), 2);
}

// ── ordenação: is_default DESC, label ASC, identifier ASC, subject ASC ───────

#[test]
fn test_list_ai_models_ordering() {
    let mut conn = setup_test_db();

    // Três modelos; "Beta" é default
    for (iri, label) in &[("test:Zeta", "Zeta"), ("test:Alpha", "Alpha"), ("test:Beta", "Beta")] {
        seed_model(&mut conn, iri, label, SVC_A);
    }
    store::assert_triples(&mut conn, &[
        Triple::new("test:Beta", IS_DEFAULT, Object::Boolean(true)),
    ], "test").unwrap();

    let snap = snapshot(&conn);
    let (rows, _) = call(&conn, Some(SVC_A), snap, 10, 0);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].subject, "test:Beta");  // default primeiro
    assert_eq!(rows[1].subject, "test:Alpha"); // depois label ASC
    assert_eq!(rows[2].subject, "test:Zeta");
}

// ── has_more com limit+1 ──────────────────────────────────────────────────────

#[test]
fn test_list_ai_models_has_more_with_limit() {
    let mut conn = setup_test_db();
    for i in 0..5 {
        seed_model(&mut conn, &format!("test:M{}", i), &format!("M{}", i), SVC_A);
    }

    let snap = snapshot(&conn);
    let (rows, has_more) = call(&conn, Some(SVC_A), snap, 3, 0);
    assert_eq!(rows.len(), 3);
    assert!(has_more);

    let (rows2, has_more2) = call(&conn, Some(SVC_A), snap, 3, 3);
    assert_eq!(rows2.len(), 2);
    assert!(!has_more2);
}

// ── offset estável ────────────────────────────────────────────────────────────

#[test]
fn test_list_ai_models_offset_stable() {
    let mut conn = setup_test_db();
    for i in 0..4 {
        seed_model(&mut conn, &format!("test:N{}", i), &format!("N{}", i), SVC_A);
    }
    let snap = snapshot(&conn);

    let (page1, _) = call(&conn, Some(SVC_A), snap, 2, 0);
    let (page2, _) = call(&conn, Some(SVC_A), snap, 2, 2);
    let all: Vec<_> = page1.iter().chain(page2.iter())
        .map(|r| r.subject.clone()).collect();
    assert_eq!(all.len(), 4, "4 resultados no total");
    // sem duplicatas
    let mut sorted = all.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 4);
}

// ── snapshot_tx congela valor: nova tripla com tx maior NÃO aparece na página ─

#[test]
fn test_list_ai_models_snapshot_tx_freezes_values() {
    let mut conn = setup_test_db();
    seed_model(&mut conn, "test:Freeze", "OldLabel", SVC_A);
    let snap = snapshot(&conn);

    // Escreve novo label APÓS o snapshot
    store::assert_triples(&mut conn, &[
        Triple::new("test:Freeze", LABEL_PRED, Object::Literal {
            value: "NewLabel".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
    ], "test").unwrap();

    let (rows, _) = call(&conn, Some(SVC_A), snap, 10, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label.as_deref(), Some("OldLabel"),
        "label deve ser o valor as-of snapshot_tx, não o novo");
}

// ── página vazia ──────────────────────────────────────────────────────────────

#[test]
fn test_list_ai_models_empty_page() {
    let conn = setup_test_db();
    let (rows, has_more) = call(&conn, Some(SVC_A), 9999, 10, 0);
    assert!(rows.is_empty());
    assert!(!has_more);
}
