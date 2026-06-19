use super::{touch, LAST_UPDATED_AT};
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};

fn seed_entity(conn: &mut rusqlite::Connection, iri: &str) {
    store::assert_triples(conn, &[
        Triple::new(iri, "rdf:type", Object::Iri("owl:NamedIndividual".to_string())),
    ], "test").unwrap();
}

fn current_last_updated(conn: &rusqlite::Connection, iri: &str) -> Option<String> {
    conn.query_row(
        "SELECT COALESCE(object_value, object) \
         FROM triples_current \
         WHERE subject = ? AND predicate = ?",
        rusqlite::params![iri, LAST_UPDATED_AT],
        |row| row.get(0),
    ).ok()
}

fn count_active_last_updated(conn: &rusqlite::Connection, iri: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject = ? AND predicate = ?",
        rusqlite::params![iri, LAST_UPDATED_AT],
        |row| row.get(0),
    ).unwrap_or(0)
}

// ── touch ────────────────────────────────────────────────────────────────────

#[test]
fn test_touch_grava_last_updated_at() {
    let mut conn = setup_test_db();
    seed_entity(&mut conn, "test:Touched");
    touch(&mut conn, "test:Touched");
    let val = current_last_updated(&conn, "test:Touched");
    assert!(val.is_some(), "deve gravar foundation:lastUpdatedAt");
}

#[test]
fn test_touch_chamadas_sucessivas_avancam_valor() {
    let mut conn = setup_test_db();
    seed_entity(&mut conn, "test:TouchTwice");

    touch(&mut conn, "test:TouchTwice");
    let first = current_last_updated(&conn, "test:TouchTwice").unwrap();

    // Força um avanço de tempo mínimo aguardando pelo menos 1ms
    std::thread::sleep(std::time::Duration::from_millis(2));

    touch(&mut conn, "test:TouchTwice");
    let second = current_last_updated(&conn, "test:TouchTwice").unwrap();

    assert!(
        second >= first,
        "segunda chamada deve avançar ou manter o timestamp: first={}, second={}",
        first, second
    );
}

#[test]
fn test_touch_nao_duplica_triplas_ativas() {
    let mut conn = setup_test_db();
    seed_entity(&mut conn, "test:NoDup");

    touch(&mut conn, "test:NoDup");
    touch(&mut conn, "test:NoDup");

    let active = count_active_last_updated(&conn, "test:NoDup");
    assert_eq!(active, 1, "deve haver exatamente 1 tripla ativa para lastUpdatedAt (MAX(tx) vence)");
}
