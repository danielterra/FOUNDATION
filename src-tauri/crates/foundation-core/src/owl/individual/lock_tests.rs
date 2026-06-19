use super::{is_system_locked, set_system_locked, check_system_locked};
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};

fn seed_entity(conn: &mut rusqlite::Connection, iri: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri("owl:NamedIndividual".to_string())),
    ], "test").unwrap();
}

// ── is_system_locked ─────────────────────────────────────────────────────────

#[test]
fn test_is_system_locked_false_when_absent() {
    let conn = setup_test_db();
    assert!(!is_system_locked(&conn, "test:Entity"), "entidade sem tripla deve retornar false");
}

#[test]
fn test_is_system_locked_true_after_set() {
    let mut conn = setup_test_db();
    seed_entity(&mut conn, "test:LockedEntity");
    set_system_locked(&mut conn, "test:LockedEntity", true).unwrap();
    assert!(is_system_locked(&conn, "test:LockedEntity"));
}

#[test]
fn test_is_system_locked_false_after_unlock() {
    let mut conn = setup_test_db();
    seed_entity(&mut conn, "test:Entity");
    set_system_locked(&mut conn, "test:Entity", true).unwrap();
    set_system_locked(&mut conn, "test:Entity", false).unwrap();
    assert!(!is_system_locked(&conn, "test:Entity"), "após desbloquear deve retornar false");
}

// ── set_system_locked ─────────────────────────────────────────────────────────

#[test]
fn test_set_system_locked_imutabilidade_max_tx_vence() {
    let mut conn = setup_test_db();
    seed_entity(&mut conn, "test:ImmutEntity");

    set_system_locked(&mut conn, "test:ImmutEntity", true).unwrap();
    set_system_locked(&mut conn, "test:ImmutEntity", false).unwrap();

    // MAX(tx) é a tripla com false — must read false
    assert!(!is_system_locked(&conn, "test:ImmutEntity"),
        "MAX(tx) deve vencer — ultimo write vence");

    // confirma que existe AMBAS as triplas no histórico (imutabilidade)
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject = 'test:ImmutEntity' AND predicate = 'foundation:isSystemLocked'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(count >= 2, "deve haver pelo menos 2 linhas no histórico (imutabilidade)");
}

// ── check_system_locked ───────────────────────────────────────────────────────

#[test]
fn test_check_system_locked_ok_when_not_locked() {
    let conn = setup_test_db();
    assert!(check_system_locked(&conn, "test:Free", None).is_ok());
}

#[test]
fn test_check_system_locked_err_when_locked() {
    let mut conn = setup_test_db();
    seed_entity(&mut conn, "test:Locked");
    set_system_locked(&mut conn, "test:Locked", true).unwrap();
    let err = check_system_locked(&conn, "test:Locked", None);
    assert!(err.is_err(), "deve retornar Err quando entidade está bloqueada");
    let msg = err.unwrap_err().to_string();
    assert!(msg.contains("system-locked") || msg.contains("locked"), "mensagem de erro deve mencionar locked");
}

#[test]
fn test_check_system_locked_exempt_property_bypasses_lock() {
    let mut conn = setup_test_db();
    seed_entity(&mut conn, "test:ExemptEntity");
    set_system_locked(&mut conn, "test:ExemptEntity", true).unwrap();

    // A propriedade isSystemLocked em si deve ser escrevível mesmo quando locked
    let result = check_system_locked(&conn, "test:ExemptEntity", Some("foundation:isSystemLocked"));
    assert!(result.is_ok(), "escrita na própria propriedade de lock deve ser permitida");
}
