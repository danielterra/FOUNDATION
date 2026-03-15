use super::*;
use crate::eavto::test_helpers::setup_test_db;

async fn query_count(conn: &Connection, sql: &str) -> i64 {
    let mut stmt = conn.prepare(sql).await.expect("prepare failed");
    let row = stmt.query_row(()).await.expect("query failed");
    row.get_value(0).unwrap().as_integer().copied().unwrap_or(0)
}

#[tokio::test]
async fn test_assert_and_get_class() {
    let conn = setup_test_db().await;
    let class = Class::new("foundation:TestClass");

    let result = class.assert(
        &conn,
        ClassType::OwlClass,
        "Test Class",
        "test-icon",
        None,
        "test",
    ).await;
    assert!(result.is_ok());

    assert!(Class::get(&conn, "foundation:TestClass").await.unwrap().is_some());

    let class_data = Class::get(&conn, "foundation:TestClass").await.unwrap().unwrap();
    assert_eq!(class_data.iri, "foundation:TestClass");
    assert_eq!(class_data.label, Some("Test Class".to_string()));
    assert_eq!(class_data.icon, Some("test-icon".to_string()));
    assert_eq!(class_data.super_classes.len(), 1);
    assert_eq!(class_data.super_classes[0].iri, "owl:Thing");
}

#[tokio::test]
async fn test_get_instances() {
    let conn = setup_test_db().await;
    let class = Class::new("foundation:Person");

    class.assert(
        &conn,
        ClassType::OwlClass,
        "Person",
        "person-icon",
        None,
        "test",
    ).await.unwrap();

    let triple1 = Triple::new(
        "foundation:John",
        rdf::TYPE,
        Object::Iri("foundation:Person".to_string()),
    );
    let triple2 = Triple::new(
        "foundation:Jane",
        rdf::TYPE,
        Object::Iri("foundation:Person".to_string()),
    );
    store::assert_triples(&conn, &[triple1, triple2], "test").await.unwrap();

    let instances = Class::get_instances(&conn, "foundation:Person").await.unwrap();
    assert_eq!(instances.len(), 2);
    assert!(instances.contains(&"foundation:John".to_string()));
    assert!(instances.contains(&"foundation:Jane".to_string()));
}

#[tokio::test]
async fn test_get_instances_polymorphic() {
    let conn = setup_test_db().await;

    Class::new("foundation:Animal").assert(
        &conn, ClassType::OwlClass, "Animal", "animal", None, "test",
    ).await.unwrap();
    Class::new("foundation:Mammal").assert(
        &conn, ClassType::OwlClass, "Mammal", "mammal",
        Some("foundation:Animal"), "test",
    ).await.unwrap();
    Class::new("foundation:Dog").assert(
        &conn, ClassType::OwlClass, "Dog", "dog",
        Some("foundation:Mammal"), "test",
    ).await.unwrap();

    store::assert_triples(&conn, &[
        Triple::new("foundation:Rex", rdf::TYPE, Object::Iri("foundation:Dog".to_string())),
        Triple::new("foundation:Lassie", rdf::TYPE, Object::Iri("foundation:Dog".to_string())),
        Triple::new("foundation:Bat", rdf::TYPE, Object::Iri("foundation:Mammal".to_string())),
        Triple::new("foundation:GenericAnimal", rdf::TYPE, Object::Iri("foundation:Animal".to_string())),
    ], "test").await.unwrap();

    let instances = Class::get_instances(&conn, "foundation:Animal").await.unwrap();
    assert_eq!(instances.len(), 4);
    assert!(instances.contains(&"foundation:Rex".to_string()));
    assert!(instances.contains(&"foundation:Lassie".to_string()));
    assert!(instances.contains(&"foundation:Bat".to_string()));
    assert!(instances.contains(&"foundation:GenericAnimal".to_string()));

    let mammal_instances = Class::get_instances(&conn, "foundation:Mammal").await.unwrap();
    assert_eq!(mammal_instances.len(), 3);
    assert!(mammal_instances.contains(&"foundation:Rex".to_string()));
    assert!(mammal_instances.contains(&"foundation:Lassie".to_string()));
    assert!(mammal_instances.contains(&"foundation:Bat".to_string()));

    let dog_instances = Class::get_instances(&conn, "foundation:Dog").await.unwrap();
    assert_eq!(dog_instances.len(), 2);
    assert!(dog_instances.contains(&"foundation:Rex".to_string()));
    assert!(dog_instances.contains(&"foundation:Lassie".to_string()));
}

#[tokio::test]
async fn test_class_hierarchy() {
    let conn = setup_test_db().await;

    let super_class = Class::new("foundation:Animal");
    super_class.assert(
        &conn, ClassType::OwlClass, "Animal", "animal-icon", None, "test",
    ).await.unwrap();

    let sub_class = Class::new("foundation:Dog");
    sub_class.assert(
        &conn, ClassType::OwlClass, "Dog", "dog-icon", Some("foundation:Animal"), "test",
    ).await.unwrap();

    let animal_data = Class::get(&conn, "foundation:Animal").await.unwrap().unwrap();
    assert_eq!(animal_data.sub_classes.len(), 1);
    assert_eq!(animal_data.sub_classes[0].iri, "foundation:Dog");

    let dog_data = Class::get(&conn, "foundation:Dog").await.unwrap().unwrap();
    assert_eq!(dog_data.super_classes.len(), 1);
    assert_eq!(dog_data.super_classes[0].iri, "foundation:Animal");
}

#[tokio::test]
async fn test_single_subclass_of_relationship() {
    let conn = setup_test_db().await;

    let test_class = Class::new("foundation:TestClass");
    test_class.assert(
        &conn, ClassType::OwlClass, "Test Class", "test-icon", Some("owl:Thing"), "test",
    ).await.unwrap();

    let class_data = Class::get(&conn, "foundation:TestClass").await.unwrap().unwrap();

    assert_eq!(
        class_data.super_classes.len(),
        1,
        "Expected exactly 1 super class, found {}",
        class_data.super_classes.len()
    );
    assert_eq!(class_data.super_classes[0].iri, "owl:Thing");
}

#[tokio::test]
async fn test_owl_one_of_enumeration() {
    let conn = setup_test_db().await;

    let priority_class = Class::new("foundation:TaskPriority");
    priority_class.assert(
        &conn, ClassType::OwlClass, "Task Priority", "priority-icon", None, "test",
    ).await.unwrap();

    let high = Triple::new("foundation:HighPriority", rdf::TYPE, Object::Iri("foundation:TaskPriority".to_string()));
    let medium = Triple::new("foundation:MediumPriority", rdf::TYPE, Object::Iri("foundation:TaskPriority".to_string()));
    let low = Triple::new("foundation:LowPriority", rdf::TYPE, Object::Iri("foundation:TaskPriority".to_string()));
    store::assert_triples(&conn, &[high, medium, low], "test").await.unwrap();

    let list3 = Triple::new("_:list3", rdf::FIRST, Object::Iri("foundation:LowPriority".to_string()));
    let list3_rest = Triple::new("_:list3", rdf::REST, Object::Iri(rdf::NIL.to_string()));
    let list2 = Triple::new("_:list2", rdf::FIRST, Object::Iri("foundation:MediumPriority".to_string()));
    let list2_rest = Triple::new("_:list2", rdf::REST, Object::Iri("_:list3".to_string()));
    let list1 = Triple::new("_:list1", rdf::FIRST, Object::Iri("foundation:HighPriority".to_string()));
    let list1_rest = Triple::new("_:list1", rdf::REST, Object::Iri("_:list2".to_string()));
    store::assert_triples(&conn, &[list1, list1_rest, list2, list2_rest, list3, list3_rest], "test").await.unwrap();

    let one_of = Triple::new("foundation:TaskPriority", owl::ONE_OF, Object::Iri("_:list1".to_string()));
    store::assert_triples(&conn, &[one_of], "test").await.unwrap();

    let class_data = Class::get(&conn, "foundation:TaskPriority").await.unwrap().unwrap();
    assert_eq!(class_data.one_of_values.len(), 3);
    assert!(class_data.one_of_values.contains(&"foundation:HighPriority".to_string()));
    assert!(class_data.one_of_values.contains(&"foundation:MediumPriority".to_string()));
    assert!(class_data.one_of_values.contains(&"foundation:LowPriority".to_string()));
}

#[tokio::test]
async fn test_parse_rdf_list() {
    let conn = setup_test_db().await;

    let list3 = Triple::new("_:n3", rdf::FIRST, Object::Iri("foundation:C".to_string()));
    let list3_rest = Triple::new("_:n3", rdf::REST, Object::Iri(rdf::NIL.to_string()));
    let list2 = Triple::new("_:n2", rdf::FIRST, Object::Iri("foundation:B".to_string()));
    let list2_rest = Triple::new("_:n2", rdf::REST, Object::Iri("_:n3".to_string()));
    let list1 = Triple::new("_:n1", rdf::FIRST, Object::Iri("foundation:A".to_string()));
    let list1_rest = Triple::new("_:n1", rdf::REST, Object::Iri("_:n2".to_string()));

    store::assert_triples(&conn, &[list1, list1_rest, list2, list2_rest, list3, list3_rest], "test").await.unwrap();

    let values = Class::parse_rdf_list(&conn, "_:n1").await.unwrap();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], "foundation:A");
    assert_eq!(values[1], "foundation:B");
    assert_eq!(values[2], "foundation:C");
}

#[tokio::test]
async fn test_set_super_classes_preserves_owl_restrictions() {
    use crate::owl::cardinality;

    let conn = setup_test_db().await;

    let class = Class::new("foundation:Task");
    class.assert(&conn, ClassType::OwlClass, "Task", "task-icon", None, "test").await.unwrap();

    store::assert_triples(&conn, &[
        Triple::new("foundation:taskName", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
    ], "test").await.unwrap();

    cardinality::set_class_required_fields(&conn, "foundation:Task", &["foundation:taskName"], "test").await.unwrap();

    let before = cardinality::get_class_cardinality_restrictions(&conn, "foundation:Task").await.unwrap();
    assert_eq!(before.len(), 1, "Should have 1 restriction before set_super_classes");

    Class::set_super_classes(&conn, "foundation:Task", &["owl:Thing"], "test").await.unwrap();

    let after = cardinality::get_class_cardinality_restrictions(&conn, "foundation:Task").await.unwrap();
    assert_eq!(after.len(), 1, "OWL restrictions must survive set_super_classes; got: {:?}", after);
}

#[tokio::test]
async fn test_get_descendant_iris() {
    let conn = setup_test_db().await;

    Class::new("foundation:Animal").assert(&conn, ClassType::OwlClass, "Animal", "animal", None, "test").await.unwrap();
    Class::new("foundation:Mammal").assert(&conn, ClassType::OwlClass, "Mammal", "mammal", Some("foundation:Animal"), "test").await.unwrap();
    Class::new("foundation:Dog").assert(&conn, ClassType::OwlClass, "Dog", "dog", Some("foundation:Mammal"), "test").await.unwrap();

    let descendants = Class::get_descendant_iris(&conn, "foundation:Animal").await.unwrap();
    assert_eq!(descendants.len(), 3);
    assert!(descendants.contains(&"foundation:Animal".to_string()));
    assert!(descendants.contains(&"foundation:Mammal".to_string()));
    assert!(descendants.contains(&"foundation:Dog".to_string()));

    let leaf = Class::get_descendant_iris(&conn, "foundation:Dog").await.unwrap();
    assert_eq!(leaf, vec!["foundation:Dog".to_string()]);
}

#[tokio::test]
async fn test_get_super_classes_excludes_blank_nodes() {
    use crate::owl::cardinality;

    let conn = setup_test_db().await;

    let parent = Class::new("foundation:BaseItem");
    parent.assert(&conn, ClassType::OwlClass, "Base Item", "base-icon", None, "test").await.unwrap();

    let child = Class::new("foundation:SpecificItem");
    child.assert(&conn, ClassType::OwlClass, "Specific Item", "item-icon", Some("foundation:BaseItem"), "test").await.unwrap();

    store::assert_triples(&conn, &[
        Triple::new("foundation:itemName", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
    ], "test").await.unwrap();

    cardinality::set_class_required_fields(&conn, "foundation:SpecificItem", &["foundation:itemName"], "test").await.unwrap();

    let class_data = Class::get(&conn, "foundation:SpecificItem").await.unwrap().unwrap();
    let super_iris: Vec<&str> = class_data.super_classes.iter().map(|t| t.iri.as_str()).collect();

    assert!(
        !super_iris.iter().any(|iri| iri.starts_with("_:")),
        "superClasses must not contain blank node restriction IRIs; got: {:?}",
        super_iris,
    );
    assert!(
        super_iris.contains(&"foundation:BaseItem"),
        "superClasses must contain the real parent class; got: {:?}",
        super_iris,
    );
}

// ── find_all_iris ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_find_all_iris_empty_db() {
    let conn = setup_test_db().await;
    let iris = Class::find_all_iris(&conn).await.unwrap();
    assert!(iris.is_empty(), "Fresh DB should have no classes");
}

#[tokio::test]
async fn test_find_all_iris_returns_owl_classes() {
    let conn = setup_test_db().await;

    Class::new("foundation:Person").assert(&conn, ClassType::OwlClass, "Person", "person", None, "test").await.unwrap();
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Task", "task", None, "test").await.unwrap();

    let iris = Class::find_all_iris(&conn).await.unwrap();
    assert!(iris.contains(&"foundation:Person".to_string()));
    assert!(iris.contains(&"foundation:Task".to_string()));
}

#[tokio::test]
async fn test_find_all_iris_returns_rdfs_classes() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
        Triple::new("foundation:RdfsOnly", rdf::TYPE, Object::Iri(rdfs::CLASS.to_string())),
    ], "test").await.unwrap();

    let iris = Class::find_all_iris(&conn).await.unwrap();
    assert!(iris.contains(&"foundation:RdfsOnly".to_string()));
}

#[tokio::test]
async fn test_find_all_iris_deduplicates_dual_typed_class() {
    let conn = setup_test_db().await;

    store::assert_triples(&conn, &[
        Triple::new("foundation:Both", rdf::TYPE, Object::Iri(owl::CLASS.to_string())),
    ], "test").await.unwrap();
    store::append_triples(&conn, &[
        Triple::new("foundation:Both", rdf::TYPE, Object::Iri(rdfs::CLASS.to_string())),
    ], "test").await.unwrap();

    let iris = Class::find_all_iris(&conn).await.unwrap();
    let count = iris.iter().filter(|iri| *iri == "foundation:Both").count();
    assert_eq!(count, 1, "Duplicate IRI should appear only once");
}

#[tokio::test]
async fn test_find_all_iris_is_sorted() {
    let conn = setup_test_db().await;

    Class::new("foundation:Zebra").assert(&conn, ClassType::OwlClass, "Zebra", "zebra", None, "test").await.unwrap();
    Class::new("foundation:Apple").assert(&conn, ClassType::OwlClass, "Apple", "apple", None, "test").await.unwrap();
    Class::new("foundation:Mango").assert(&conn, ClassType::OwlClass, "Mango", "mango", None, "test").await.unwrap();

    let iris = Class::find_all_iris(&conn).await.unwrap();
    let foundation_iris: Vec<&str> = iris.iter()
        .filter(|iri| iri.starts_with("foundation:"))
        .map(|s| s.as_str())
        .collect();

    let mut sorted = foundation_iris.clone();
    sorted.sort();
    assert_eq!(foundation_iris, sorted, "Result should be sorted alphabetically");
}

// ── retract_all ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_retract_all_removes_class() {
    let conn = setup_test_db().await;

    Class::new("foundation:Person").assert(&conn, ClassType::OwlClass, "Person", "person", None, "test").await.unwrap();
    assert!(Class::get(&conn, "foundation:Person").await.unwrap().is_some());

    Class::retract_all(&conn, "foundation:Person", "test").await.unwrap();

    assert!(Class::get(&conn, "foundation:Person").await.unwrap().is_none(),
        "Class should be gone after retract_all");
}

#[tokio::test]
async fn test_retract_all_removes_all_triples() {
    let conn = setup_test_db().await;

    Class::new("foundation:Person").assert(&conn, ClassType::OwlClass, "Person", "person", Some("foundation:Agent"), "test").await.unwrap();

    Class::retract_all(&conn, "foundation:Person", "test").await.unwrap();

    let remaining = crate::eavto::query::get_by_entity(&conn, "foundation:Person").await.unwrap();
    assert!(remaining.triples.is_empty(), "All triples should be retracted");
}

#[tokio::test]
async fn test_retract_all_noop_on_nonexistent_class() {
    let conn = setup_test_db().await;

    let result = Class::retract_all(&conn, "foundation:Ghost", "test").await;
    assert!(result.is_ok(), "retract_all on non-existent class should not error");
}

#[tokio::test]
async fn test_retract_all_does_not_affect_other_classes() {
    let conn = setup_test_db().await;

    Class::new("foundation:Person").assert(&conn, ClassType::OwlClass, "Person", "person", None, "test").await.unwrap();
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Task", "task", None, "test").await.unwrap();

    Class::retract_all(&conn, "foundation:Person", "test").await.unwrap();

    assert!(Class::get(&conn, "foundation:Person").await.unwrap().is_none());
    assert!(Class::get(&conn, "foundation:Task").await.unwrap().is_some(),
        "Other classes should be unaffected");
}

#[tokio::test]
async fn test_retract_all_class_no_longer_in_find_all_iris() {
    let conn = setup_test_db().await;

    Class::new("foundation:Person").assert(&conn, ClassType::OwlClass, "Person", "person", None, "test").await.unwrap();

    let before = Class::find_all_iris(&conn).await.unwrap();
    assert!(before.contains(&"foundation:Person".to_string()));

    Class::retract_all(&conn, "foundation:Person", "test").await.unwrap();

    let after = Class::find_all_iris(&conn).await.unwrap();
    assert!(!after.contains(&"foundation:Person".to_string()),
        "Retracted class should not appear in find_all_iris");
}

// ── set_label ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_set_label_updates_label() {
    let conn = setup_test_db().await;
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Old Label", "https://example.com/icon.svg", None, "test").await.unwrap();

    Class::set_label(&conn, "foundation:Task", "New Label", "test").await.unwrap();

    let class = Class::get(&conn, "foundation:Task").await.unwrap().unwrap();
    assert_eq!(class.label, Some("New Label".to_string()));
}

#[tokio::test]
async fn test_set_label_retracts_old_label() {
    let conn = setup_test_db().await;
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Old Label", "https://example.com/icon.svg", None, "test").await.unwrap();

    Class::set_label(&conn, "foundation:Task", "New Label", "test").await.unwrap();

    let retracted = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'rdfs:label' AND retracted = 1",
    ).await;
    let active = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'rdfs:label' AND retracted = 0",
    ).await;
    assert_eq!(retracted, 1, "Old label should be retracted");
    assert_eq!(active, 1, "Only the new label should be active");
}

// ── set_comment ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_set_comment_adds_comment() {
    let conn = setup_test_db().await;
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Task", "https://example.com/icon.svg", None, "test").await.unwrap();

    Class::set_comment(&conn, "foundation:Task", "A task entity", "test").await.unwrap();

    let class = Class::get(&conn, "foundation:Task").await.unwrap().unwrap();
    assert_eq!(class.comment, Some("A task entity".to_string()));
}

#[tokio::test]
async fn test_set_comment_replaces_existing_comment() {
    let conn = setup_test_db().await;
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Task", "https://example.com/icon.svg", None, "test").await.unwrap();
    Class::set_comment(&conn, "foundation:Task", "First comment", "test").await.unwrap();
    Class::set_comment(&conn, "foundation:Task", "Updated comment", "test").await.unwrap();

    let class = Class::get(&conn, "foundation:Task").await.unwrap().unwrap();
    assert_eq!(class.comment, Some("Updated comment".to_string()));

    let active = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'rdfs:comment' AND retracted = 0",
    ).await;
    assert_eq!(active, 1, "Only one active comment should exist");
}

// ── set_icon ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_set_icon_url_icon_stores_as_has_icon_literal() {
    let conn = setup_test_db().await;
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Task", "https://example.com/original.svg", None, "test").await.unwrap();

    Class::set_icon(&conn, "foundation:Task", "https://example.com/new.svg", "test").await.unwrap();

    let active = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'foundation:hasIcon' AND object_type = 'literal' AND retracted = 0",
    ).await;
    assert_eq!(active, 1);
}

// ── set_super_class ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_set_super_class_updates_parent() {
    let conn = setup_test_db().await;
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", Some("foundation:Work"), "test").await.unwrap();

    Class::set_super_class(&conn, "foundation:Task", "foundation:Activity", "test").await.unwrap();

    let class = Class::get(&conn, "foundation:Task").await.unwrap().unwrap();
    let super_iris: Vec<&str> = class.super_classes.iter().map(|t| t.iri.as_str()).collect();
    assert!(super_iris.contains(&"foundation:Activity"),
        "New super class should be set, got: {:?}", super_iris);
    assert!(!super_iris.contains(&"foundation:Work"),
        "Old super class should be removed, got: {:?}", super_iris);
}

#[tokio::test]
async fn test_set_super_class_replaces_old() {
    let conn = setup_test_db().await;
    Class::new("foundation:Task").assert(&conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", Some("foundation:Work"), "test").await.unwrap();

    Class::set_super_class(&conn, "foundation:Task", "foundation:NewParent", "test").await.unwrap();

    let active = query_count(
        &conn,
        "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'rdfs:subClassOf' AND retracted = 0 AND object IS NOT NULL",
    ).await;
    assert_eq!(active, 1, "Only one active subClassOf should exist");
}
