use super::{deterministic_record_iri, raw_record_exists};
use crate::eavto::{store, test_helpers::setup_test_db, Object, Triple};

#[test]
fn deterministic_record_iri_is_stable_and_scoped_to_source() {
    let first = deterministic_record_iri("foundation:DataSource_A", "item-42");
    let repeated = deterministic_record_iri("foundation:DataSource_A", "item-42");
    let other_source = deterministic_record_iri("foundation:DataSource_B", "item-42");

    assert_eq!(first, repeated);
    assert_ne!(first, other_source);
}

#[test]
fn raw_record_exists_checks_for_the_expected_type() {
    let mut conn = setup_test_db();
    store::assert_triples(
        &mut conn,
        &[
            Triple::new(
                "foundation:RawDataRecord_test",
                "rdf:type",
                Object::Iri("foundation:RawDataRecord".to_string()),
            ),
            Triple::new(
                "foundation:OtherRecord_test",
                "rdf:type",
                Object::Iri("foundation:Email".to_string()),
            ),
        ],
        "test",
    )
    .expect("fixture triples should persist");

    assert!(raw_record_exists(&conn, "foundation:RawDataRecord_test").unwrap());
    assert!(!raw_record_exists(&conn, "foundation:OtherRecord_test").unwrap());
    assert!(!raw_record_exists(&conn, "foundation:MissingRecord").unwrap());
}
