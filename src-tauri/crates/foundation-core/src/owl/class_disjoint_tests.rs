use super::*;
use crate::eavto::test_helpers::setup_test_db;

fn mk_class(conn: &mut Connection, iri: &str) {
    Class::new(iri).assert(conn, ClassType::OwlClass, iri, "test-icon", None, "test").unwrap();
}

// ── set_disjoint_with ──────────────────────────────────────────────────────────

#[test]
fn test_set_disjoint_with_happy_path() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    Class::set_disjoint_with(&mut conn, "test:Cat", &["test:Dog"], "test").unwrap();

    let fwd = Class::get_direct_disjoint_pair_iris(&conn, "test:Cat").unwrap();
    assert!(fwd.contains(&"test:Dog".to_string()));
    let rev = Class::get_direct_disjoint_pair_iris(&conn, "test:Dog").unwrap();
    assert!(rev.contains(&"test:Cat".to_string()), "symmetric triple must be asserted");
}

#[test]
fn test_set_disjoint_with_self_reference_rejected() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");

    let err = Class::set_disjoint_with(&mut conn, "test:Cat", &["test:Cat"], "test");
    assert!(err.is_err(), "cannot declare class disjoint with itself");
}

#[test]
fn test_set_disjoint_with_nonexistent_class_rejected() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");

    let err = Class::set_disjoint_with(&mut conn, "test:Cat", &["test:Ghost"], "test");
    assert!(err.is_err(), "non-existent class must be rejected");
}

#[test]
fn test_set_disjoint_with_empty_list_clears_existing() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");
    Class::set_disjoint_with(&mut conn, "test:Cat", &["test:Dog"], "test").unwrap();

    Class::set_disjoint_with(&mut conn, "test:Cat", &[], "test").unwrap();

    let fwd = Class::get_direct_disjoint_pair_iris(&conn, "test:Cat").unwrap();
    assert!(fwd.is_empty(), "passing [] must clear all pairwise disjointness");
}

#[test]
fn test_set_disjoint_with_replaces_old_set() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:A");
    mk_class(&mut conn, "test:B");
    mk_class(&mut conn, "test:C");
    Class::set_disjoint_with(&mut conn, "test:A", &["test:B"], "test").unwrap();
    Class::set_disjoint_with(&mut conn, "test:A", &["test:C"], "test").unwrap();

    let fwd = Class::get_direct_disjoint_pair_iris(&conn, "test:A").unwrap();
    assert!(!fwd.contains(&"test:B".to_string()), "old disjoint must be cleared");
    assert!(fwd.contains(&"test:C".to_string()), "new disjoint must be set");
}

// ── add_disjoint_with ─────────────────────────────────────────────────────────

#[test]
fn test_add_disjoint_with_happy_path() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    Class::add_disjoint_with(&mut conn, "test:Cat", "test:Dog", "test").unwrap();

    let fwd = Class::get_direct_disjoint_pair_iris(&conn, "test:Cat").unwrap();
    assert!(fwd.contains(&"test:Dog".to_string()));
}

#[test]
fn test_add_disjoint_with_is_idempotent() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    Class::add_disjoint_with(&mut conn, "test:Cat", "test:Dog", "test").unwrap();
    Class::add_disjoint_with(&mut conn, "test:Cat", "test:Dog", "test").unwrap();

    let fwd = Class::get_direct_disjoint_pair_iris(&conn, "test:Cat").unwrap();
    let dog_count = fwd.iter().filter(|d| d.as_str() == "test:Dog").count();
    assert_eq!(dog_count, 1, "duplicate add must not create a second entry");
}

#[test]
fn test_add_disjoint_with_self_reference_rejected() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");

    let err = Class::add_disjoint_with(&mut conn, "test:Cat", "test:Cat", "test");
    assert!(err.is_err());
}

// ── remove_disjoint_with ──────────────────────────────────────────────────────

#[test]
fn test_remove_disjoint_with_removes_both_directions() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");
    Class::add_disjoint_with(&mut conn, "test:Cat", "test:Dog", "test").unwrap();

    Class::remove_disjoint_with(&mut conn, "test:Cat", "test:Dog", "test").unwrap();

    let fwd = Class::get_direct_disjoint_pair_iris(&conn, "test:Cat").unwrap();
    let rev = Class::get_direct_disjoint_pair_iris(&conn, "test:Dog").unwrap();
    assert!(fwd.is_empty());
    assert!(rev.is_empty());
}

#[test]
fn test_remove_disjoint_with_noop_when_not_set() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    let result = Class::remove_disjoint_with(&mut conn, "test:Cat", "test:Dog", "test");
    assert!(result.is_ok(), "removing non-existent disjointness must not error");
}

// ── assert_all_disjoint_classes ───────────────────────────────────────────────

#[test]
fn test_assert_all_disjoint_classes_happy_path() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");
    mk_class(&mut conn, "test:Fish");

    let adc_iri = Class::assert_all_disjoint_classes(
        &mut conn, &["test:Cat", "test:Dog", "test:Fish"], "test",
    ).unwrap();

    assert!(adc_iri.starts_with("_:adc_"), "adc iri must be a blank node");
    let members = Class::get_all_disjoint_classes_members(&conn, &adc_iri).unwrap();
    assert_eq!(members.len(), 3);
    assert!(members.contains(&"test:Cat".to_string()));
    assert!(members.contains(&"test:Dog".to_string()));
    assert!(members.contains(&"test:Fish".to_string()));
}

#[test]
fn test_assert_all_disjoint_classes_requires_at_least_two() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");

    let err = Class::assert_all_disjoint_classes(&mut conn, &["test:Cat"], "test");
    assert!(err.is_err(), "must require at least 2 members");
}

#[test]
fn test_assert_all_disjoint_classes_rejects_duplicate_members() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    let err = Class::assert_all_disjoint_classes(
        &mut conn, &["test:Cat", "test:Dog", "test:Cat"], "test",
    );
    assert!(err.is_err(), "duplicate members must be rejected");
}

#[test]
fn test_assert_all_disjoint_classes_is_idempotent() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    let iri1 = Class::assert_all_disjoint_classes(
        &mut conn, &["test:Cat", "test:Dog"], "test",
    ).unwrap();
    let iri2 = Class::assert_all_disjoint_classes(
        &mut conn, &["test:Cat", "test:Dog"], "test",
    ).unwrap();
    assert_eq!(iri1, iri2, "same member set must produce same blank IRI");
}

#[test]
fn test_assert_all_disjoint_classes_rejects_nonexistent_member() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");

    let err = Class::assert_all_disjoint_classes(
        &mut conn, &["test:Cat", "test:Ghost"], "test",
    );
    assert!(err.is_err(), "non-existent class member must be rejected");
}

// ── retract_all_disjoint_classes ──────────────────────────────────────────────

#[test]
fn test_retract_all_disjoint_classes_removes_node() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    let adc_iri = Class::assert_all_disjoint_classes(
        &mut conn, &["test:Cat", "test:Dog"], "test",
    ).unwrap();

    Class::retract_all_disjoint_classes(&mut conn, &adc_iri, "test").unwrap();

    let members = Class::get_all_disjoint_classes_members(&conn, &adc_iri).unwrap();
    assert!(members.is_empty(), "after retraction ADC node must have no members");
}

#[test]
fn test_retract_then_reassert_all_disjoint_classes() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    let adc_iri = Class::assert_all_disjoint_classes(
        &mut conn, &["test:Cat", "test:Dog"], "test",
    ).unwrap();
    Class::retract_all_disjoint_classes(&mut conn, &adc_iri, "test").unwrap();

    let adc2 = Class::assert_all_disjoint_classes(
        &mut conn, &["test:Cat", "test:Dog"], "test",
    ).unwrap();
    assert_eq!(adc_iri, adc2, "deterministic IRI must be stable across retract+reassert");
    let members = Class::get_all_disjoint_classes_members(&conn, &adc2).unwrap();
    assert_eq!(members.len(), 2, "after re-assert members must be restored");
}

// ── get_effective_disjoint_iris ───────────────────────────────────────────────

#[test]
fn test_get_effective_disjoint_iris_includes_direct() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");
    Class::add_disjoint_with(&mut conn, "test:Cat", "test:Dog", "test").unwrap();

    let effective = Class::get_effective_disjoint_iris(&conn, "test:Cat").unwrap();
    assert!(effective.contains("test:Dog"));
}

#[test]
fn test_get_effective_disjoint_iris_inherits_from_ancestor() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Animal");
    mk_class(&mut conn, "test:Cat");
    mk_class(&mut conn, "test:Dog");

    Class::set_super_class(&mut conn, "test:Cat", "test:Animal", "test").unwrap();
    Class::add_disjoint_with(&mut conn, "test:Animal", "test:Dog", "test").unwrap();

    let effective = Class::get_effective_disjoint_iris(&conn, "test:Cat").unwrap();
    assert!(
        effective.contains("test:Dog"),
        "disjointness declared on ancestor must propagate to descendant"
    );
}

#[test]
fn test_get_effective_disjoint_iris_empty_when_none_declared() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Isolated");

    let effective = Class::get_effective_disjoint_iris(&conn, "test:Isolated").unwrap();
    assert!(effective.is_empty());
}

// ── validate_super_classes_not_disjoint ───────────────────────────────────────

#[test]
fn test_validate_super_classes_not_disjoint_passes_when_no_conflict() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:A");
    mk_class(&mut conn, "test:B");

    let result = Class::validate_super_classes_not_disjoint(&conn, &["test:A", "test:B"]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_super_classes_not_disjoint_rejects_disjoint_parents() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:A");
    mk_class(&mut conn, "test:B");
    Class::add_disjoint_with(&mut conn, "test:A", "test:B", "test").unwrap();

    let err = Class::validate_super_classes_not_disjoint(&conn, &["test:A", "test:B"]);
    assert!(err.is_err(), "disjoint parents must be rejected");
}

#[test]
fn test_validate_super_classes_not_disjoint_single_parent_passes() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:A");

    let result = Class::validate_super_classes_not_disjoint(&conn, &["test:A"]);
    assert!(result.is_ok());
}

// ── has_property ─────────────────────────────────────────────────────────────

#[test]
fn test_has_property_true_when_declared() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Task");
    store::assert_triples(&mut conn, &[
        Triple::new("test:title", rdf::TYPE, Object::Iri(owl::DATATYPE_PROPERTY.to_string())),
        Triple::new("test:title", "rdfs:domain", Object::Iri("test:Task".to_string())),
    ], "test").unwrap();

    assert!(Class::has_property(&conn, "test:Task", "test:title"));
}

#[test]
fn test_has_property_false_when_not_declared() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Task");

    assert!(!Class::has_property(&conn, "test:Task", "test:nonexistent"));
}

#[test]
fn test_has_property_true_when_inherited() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Base");
    mk_class(&mut conn, "test:Child");
    Class::set_super_class(&mut conn, "test:Child", "test:Base", "test").unwrap();
    store::assert_triples(&mut conn, &[
        Triple::new("test:baseProp", rdf::TYPE, Object::Iri(owl::DATATYPE_PROPERTY.to_string())),
        Triple::new("test:baseProp", "rdfs:domain", Object::Iri("test:Base".to_string())),
    ], "test").unwrap();

    assert!(
        Class::has_property(&conn, "test:Child", "test:baseProp"),
        "child class must inherit property declared on parent"
    );
}

// ── get_ancestor_iris ─────────────────────────────────────────────────────────

#[test]
fn test_get_ancestor_iris_includes_self() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Leaf");

    let ancestors = Class::get_ancestor_iris(&conn, "test:Leaf").unwrap();
    assert!(ancestors.contains(&"test:Leaf".to_string()), "must include the class itself");
}

#[test]
fn test_get_ancestor_iris_multi_level_chain() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Animal");
    mk_class(&mut conn, "test:Mammal");
    mk_class(&mut conn, "test:Dog");
    Class::set_super_class(&mut conn, "test:Mammal", "test:Animal", "test").unwrap();
    Class::set_super_class(&mut conn, "test:Dog", "test:Mammal", "test").unwrap();

    let ancestors = Class::get_ancestor_iris(&conn, "test:Dog").unwrap();
    assert!(ancestors.contains(&"test:Dog".to_string()));
    assert!(ancestors.contains(&"test:Mammal".to_string()));
    assert!(ancestors.contains(&"test:Animal".to_string()));
}

#[test]
fn test_get_ancestor_iris_class_without_explicit_parent() {
    let mut conn = setup_test_db();
    mk_class(&mut conn, "test:Root");

    let ancestors = Class::get_ancestor_iris(&conn, "test:Root").unwrap();
    assert!(ancestors.contains(&"test:Root".to_string()));
    assert!(!ancestors.is_empty());
}
