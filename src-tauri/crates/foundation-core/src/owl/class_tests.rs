use super::*;
use crate::eavto::test_helpers::setup_test_db;

#[test]
fn test_assert_and_get_class() {
    let mut conn = setup_test_db();
    let class = Class::new("foundation:TestClass");

    // Assert class with label and icon (will default to owl:Thing as parent)
    let result = class.assert(
        &mut conn,
        ClassType::OwlClass,
        "Test Class",
        "test-icon",
        None,
        "test",
    );
    assert!(result.is_ok());

    // Verify it exists
    assert!(Class::get(&conn, "foundation:TestClass").unwrap().is_some());

    // Get complete class data
    let class_data = Class::get(&conn, "foundation:TestClass").unwrap().unwrap();
    assert_eq!(class_data.iri, "foundation:TestClass");
    assert_eq!(class_data.label, Some("Test Class".to_string()));
    assert_eq!(class_data.icon, Some("test-icon".to_string()));
    // Should have owl:Thing as super class
    assert_eq!(class_data.super_classes.len(), 1);
    assert_eq!(class_data.super_classes[0].iri, "owl:Thing");
}

#[test]
fn test_get_instances() {
    let mut conn = setup_test_db();
    let class = Class::new("foundation:Person");

    class.assert(
        &mut conn,
        ClassType::OwlClass,
        "Person",
        "person-icon",
        None,
        "test",
    ).unwrap();

    // Create instances
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
    store::assert_triples(&mut conn, &[triple1, triple2], "test").unwrap();

    // Get instances separately
    let instances = Class::get_instances(&conn, "foundation:Person").unwrap();
    assert_eq!(instances.len(), 2);
    assert!(instances.contains(&"foundation:John".to_string()));
    assert!(instances.contains(&"foundation:Jane".to_string()));
}

#[test]
fn test_get_instances_polymorphic() {
    let mut conn = setup_test_db();

    Class::new("foundation:Animal").assert(
        &mut conn, ClassType::OwlClass, "Animal", "animal", None, "test",
    ).unwrap();
    Class::new("foundation:Mammal").assert(
        &mut conn, ClassType::OwlClass, "Mammal", "mammal",
        Some("foundation:Animal"), "test",
    ).unwrap();
    Class::new("foundation:Dog").assert(
        &mut conn, ClassType::OwlClass, "Dog", "dog",
        Some("foundation:Mammal"), "test",
    ).unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Rex", rdf::TYPE, Object::Iri("foundation:Dog".to_string())),
        Triple::new("foundation:Lassie", rdf::TYPE, Object::Iri("foundation:Dog".to_string())),
        Triple::new("foundation:Bat", rdf::TYPE, Object::Iri("foundation:Mammal".to_string())),
        Triple::new("foundation:GenericAnimal", rdf::TYPE, Object::Iri("foundation:Animal".to_string())),
    ], "test").unwrap();

    let instances = Class::get_instances(&conn, "foundation:Animal").unwrap();
    assert_eq!(instances.len(), 4);
    assert!(instances.contains(&"foundation:Rex".to_string()));
    assert!(instances.contains(&"foundation:Lassie".to_string()));
    assert!(instances.contains(&"foundation:Bat".to_string()));
    assert!(instances.contains(&"foundation:GenericAnimal".to_string()));

    let mammal_instances = Class::get_instances(&conn, "foundation:Mammal").unwrap();
    assert_eq!(mammal_instances.len(), 3);
    assert!(mammal_instances.contains(&"foundation:Rex".to_string()));
    assert!(mammal_instances.contains(&"foundation:Lassie".to_string()));
    assert!(mammal_instances.contains(&"foundation:Bat".to_string()));

    let dog_instances = Class::get_instances(&conn, "foundation:Dog").unwrap();
    assert_eq!(dog_instances.len(), 2);
    assert!(dog_instances.contains(&"foundation:Rex".to_string()));
    assert!(dog_instances.contains(&"foundation:Lassie".to_string()));
}

#[test]
fn test_class_hierarchy() {
    let mut conn = setup_test_db();

    // Create super class (with owl:Thing as parent)
    let super_class = Class::new("foundation:Animal");
    super_class.assert(
        &mut conn,
        ClassType::OwlClass,
        "Animal",
        "animal-icon",
        None,
        "test",
    ).unwrap();

    // Create sub class (with Animal as parent)
    let sub_class = Class::new("foundation:Dog");
    sub_class.assert(
        &mut conn,
        ClassType::OwlClass,
        "Dog",
        "dog-icon",
        Some("foundation:Animal"),
        "test",
    ).unwrap();

    // Get super class data and check sub classes
    let animal_data = Class::get(&conn, "foundation:Animal").unwrap().unwrap();
    assert_eq!(animal_data.sub_classes.len(), 1);
    assert_eq!(animal_data.sub_classes[0].iri, "foundation:Dog");

    // Get sub class data and check super classes
    let dog_data = Class::get(&conn, "foundation:Dog").unwrap().unwrap();
    assert_eq!(dog_data.super_classes.len(), 1);
    assert_eq!(dog_data.super_classes[0].iri, "foundation:Animal");
}

#[test]
fn test_single_subclass_of_relationship() {
    let mut conn = setup_test_db();

    // Create class with explicit parent
    let test_class = Class::new("foundation:TestClass");
    test_class.assert(
        &mut conn,
        ClassType::OwlClass,
        "Test Class",
        "test-icon",
        Some("owl:Thing"),
        "test",
    ).unwrap();

    // Get class data
    let class_data = Class::get(&conn, "foundation:TestClass").unwrap().unwrap();

    // Should have exactly 1 super class
    assert_eq!(
        class_data.super_classes.len(),
        1,
        "Expected exactly 1 super class, found {}",
        class_data.super_classes.len()
    );
    assert_eq!(class_data.super_classes[0].iri, "owl:Thing");
}

#[test]
fn test_owl_one_of_enumeration() {
    let mut conn = setup_test_db();

    // Create enumeration class with owl:oneOf
    let priority_class = Class::new("foundation:TaskPriority");
    priority_class.assert(
        &mut conn,
        ClassType::OwlClass,
        "Task Priority",
        "priority-icon",
        None,
        "test",
    ).unwrap();

    // Create enumerated individuals
    let high = Triple::new(
        "foundation:HighPriority",
        rdf::TYPE,
        Object::Iri("foundation:TaskPriority".to_string()),
    );
    let medium = Triple::new(
        "foundation:MediumPriority",
        rdf::TYPE,
        Object::Iri("foundation:TaskPriority".to_string()),
    );
    let low = Triple::new(
        "foundation:LowPriority",
        rdf::TYPE,
        Object::Iri("foundation:TaskPriority".to_string()),
    );
    store::assert_triples(&mut conn, &[high, medium, low], "test").unwrap();

    // Create RDF list: (High Medium Low)
    // List structure: _:list1 -> _:list2 -> _:list3 -> rdf:nil
    let list3 = Triple::new(
        "_:list3",
        rdf::FIRST,
        Object::Iri("foundation:LowPriority".to_string()),
    );
    let list3_rest = Triple::new("_:list3", rdf::REST, Object::Iri(rdf::NIL.to_string()));

    let list2 = Triple::new(
        "_:list2",
        rdf::FIRST,
        Object::Iri("foundation:MediumPriority".to_string()),
    );
    let list2_rest = Triple::new("_:list2", rdf::REST, Object::Iri("_:list3".to_string()));

    let list1 = Triple::new(
        "_:list1",
        rdf::FIRST,
        Object::Iri("foundation:HighPriority".to_string()),
    );
    let list1_rest = Triple::new("_:list1", rdf::REST, Object::Iri("_:list2".to_string()));

    store::assert_triples(
        &mut conn,
        &[list1, list1_rest, list2, list2_rest, list3, list3_rest],
        "test",
    ).unwrap();

    // Add owl:oneOf to the class
    let one_of = Triple::new(
        "foundation:TaskPriority",
        owl::ONE_OF,
        Object::Iri("_:list1".to_string()),
    );
    store::assert_triples(&mut conn, &[one_of], "test").unwrap();

    // Get class and verify owl:oneOf values
    let class_data = Class::get(&conn, "foundation:TaskPriority").unwrap().unwrap();
    assert_eq!(class_data.one_of_values.len(), 3);
    assert!(class_data.one_of_values.contains(&"foundation:HighPriority".to_string()));
    assert!(class_data.one_of_values.contains(&"foundation:MediumPriority".to_string()));
    assert!(class_data.one_of_values.contains(&"foundation:LowPriority".to_string()));
}

#[test]
fn test_parse_rdf_list() {
    let mut conn = setup_test_db();

    // Create a simple RDF list: (A B C)
    let list3 = Triple::new("_:n3", rdf::FIRST, Object::Iri("foundation:C".to_string()));
    let list3_rest = Triple::new("_:n3", rdf::REST, Object::Iri(rdf::NIL.to_string()));

    let list2 = Triple::new("_:n2", rdf::FIRST, Object::Iri("foundation:B".to_string()));
    let list2_rest = Triple::new("_:n2", rdf::REST, Object::Iri("_:n3".to_string()));

    let list1 = Triple::new("_:n1", rdf::FIRST, Object::Iri("foundation:A".to_string()));
    let list1_rest = Triple::new("_:n1", rdf::REST, Object::Iri("_:n2".to_string()));

    store::assert_triples(
        &mut conn,
        &[list1, list1_rest, list2, list2_rest, list3, list3_rest],
        "test",
    ).unwrap();

    // Parse the list
    let values = Class::parse_rdf_list(&conn, "_:n1").unwrap();

    assert_eq!(values.len(), 3);
    assert_eq!(values[0], "foundation:A");
    assert_eq!(values[1], "foundation:B");
    assert_eq!(values[2], "foundation:C");
}

#[test]
fn test_set_super_classes_preserves_owl_restrictions() {
    use crate::owl::cardinality;

    let mut conn = setup_test_db();

    let class = Class::new("foundation:Task");
    class.assert(
        &mut conn, ClassType::OwlClass, "Task", "task-icon", None, "test",
    ).unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new(
            "foundation:taskName", "rdf:type",
            Object::Iri("owl:DatatypeProperty".to_string()),
        ),
    ], "test").unwrap();

    cardinality::set_class_required_fields(
        &mut conn, "foundation:Task", &["foundation:taskName"], "test",
    ).unwrap();

    let before =
        cardinality::get_class_cardinality_restrictions(&conn, "foundation:Task").unwrap();
    assert_eq!(before.len(), 1, "Should have 1 restriction before set_super_classes");

    Class::set_super_classes(
        &mut conn, "foundation:Task", &["owl:Thing"], "test",
    ).unwrap();

    let after =
        cardinality::get_class_cardinality_restrictions(&conn, "foundation:Task").unwrap();
    assert_eq!(
        after.len(), 1,
        "OWL restrictions must survive set_super_classes; got: {:?}",
        after,
    );
}

#[test]
fn test_get_descendant_iris() {
    let mut conn = setup_test_db();

    // Build: Animal -> Mammal -> Dog (3-level hierarchy)
    Class::new("foundation:Animal").assert(
        &mut conn, ClassType::OwlClass, "Animal", "animal", None, "test",
    ).unwrap();
    Class::new("foundation:Mammal").assert(
        &mut conn, ClassType::OwlClass, "Mammal", "mammal",
        Some("foundation:Animal"), "test",
    ).unwrap();
    Class::new("foundation:Dog").assert(
        &mut conn, ClassType::OwlClass, "Dog", "dog",
        Some("foundation:Mammal"), "test",
    ).unwrap();

    let descendants = Class::get_descendant_iris(&conn, "foundation:Animal").unwrap();
    assert_eq!(descendants.len(), 3);
    assert!(descendants.contains(&"foundation:Animal".to_string()));
    assert!(descendants.contains(&"foundation:Mammal".to_string()));
    assert!(descendants.contains(&"foundation:Dog".to_string()));

    // Querying a leaf class returns only itself
    let leaf = Class::get_descendant_iris(&conn, "foundation:Dog").unwrap();
    assert_eq!(leaf, vec!["foundation:Dog".to_string()]);
}

#[test]
fn test_get_super_classes_excludes_blank_nodes() {
    use crate::owl::cardinality;

    let mut conn = setup_test_db();

    let parent = Class::new("foundation:BaseItem");
    parent.assert(
        &mut conn, ClassType::OwlClass, "Base Item", "base-icon", None, "test",
    ).unwrap();

    let child = Class::new("foundation:SpecificItem");
    child.assert(
        &mut conn, ClassType::OwlClass, "Specific Item", "item-icon",
        Some("foundation:BaseItem"), "test",
    ).unwrap();

    store::assert_triples(&mut conn, &[
        Triple::new(
            "foundation:itemName", "rdf:type",
            Object::Iri("owl:DatatypeProperty".to_string()),
        ),
    ], "test").unwrap();

    cardinality::set_class_required_fields(
        &mut conn, "foundation:SpecificItem", &["foundation:itemName"], "test",
    ).unwrap();

    let class_data = Class::get(&conn, "foundation:SpecificItem").unwrap().unwrap();
    let super_iris: Vec<&str> =
        class_data.super_classes.iter().map(|t| t.iri.as_str()).collect();

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

#[test]
fn test_find_all_iris_empty_db() {
    let conn = setup_test_db();
    let iris = Class::find_all_iris(&conn).unwrap();
    assert!(iris.is_empty(), "Fresh DB should have no classes");
}

#[test]
fn test_find_all_iris_returns_owl_classes() {
    let mut conn = setup_test_db();

    Class::new("foundation:Person").assert(
        &mut conn, ClassType::OwlClass, "Person", "person", None, "test",
    ).unwrap();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Task", "task", None, "test",
    ).unwrap();

    let iris = Class::find_all_iris(&conn).unwrap();
    assert!(iris.contains(&"foundation:Person".to_string()));
    assert!(iris.contains(&"foundation:Task".to_string()));
}

#[test]
fn test_find_all_iris_returns_rdfs_classes() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:RdfsOnly", rdf::TYPE, Object::Iri(rdfs::CLASS.to_string())),
    ], "test").unwrap();

    let iris = Class::find_all_iris(&conn).unwrap();
    assert!(iris.contains(&"foundation:RdfsOnly".to_string()));
}

#[test]
fn test_find_all_iris_deduplicates_dual_typed_class() {
    let mut conn = setup_test_db();

    store::assert_triples(&mut conn, &[
        Triple::new("foundation:Both", rdf::TYPE, Object::Iri(owl::CLASS.to_string())),
    ], "test").unwrap();
    store::append_triples(&mut conn, &[
        Triple::new("foundation:Both", rdf::TYPE, Object::Iri(rdfs::CLASS.to_string())),
    ], "test").unwrap();

    let iris = Class::find_all_iris(&conn).unwrap();
    let count = iris.iter().filter(|iri| *iri == "foundation:Both").count();
    assert_eq!(count, 1, "Duplicate IRI should appear only once");
}

#[test]
fn test_find_all_iris_is_sorted() {
    let mut conn = setup_test_db();

    Class::new("foundation:Zebra").assert(
        &mut conn, ClassType::OwlClass, "Zebra", "zebra", None, "test",
    ).unwrap();
    Class::new("foundation:Apple").assert(
        &mut conn, ClassType::OwlClass, "Apple", "apple", None, "test",
    ).unwrap();
    Class::new("foundation:Mango").assert(
        &mut conn, ClassType::OwlClass, "Mango", "mango", None, "test",
    ).unwrap();

    let iris = Class::find_all_iris(&conn).unwrap();
    let foundation_iris: Vec<&str> = iris.iter()
        .filter(|iri| iri.starts_with("foundation:"))
        .map(|s| s.as_str())
        .collect();

    let mut sorted = foundation_iris.clone();
    sorted.sort();
    assert_eq!(foundation_iris, sorted, "Result should be sorted alphabetically");
}

// ── retract_all ──────────────────────────────────────────────────────────

#[test]
fn test_retract_all_removes_class() {
    let mut conn = setup_test_db();

    Class::new("foundation:Person").assert(
        &mut conn, ClassType::OwlClass, "Person", "person", None, "test",
    ).unwrap();

    assert!(Class::get(&conn, "foundation:Person").unwrap().is_some());

    Class::retract_all(&mut conn, "foundation:Person", "test").unwrap();

    assert!(Class::get(&conn, "foundation:Person").unwrap().is_none(),
        "Class should be gone after retract_all");
}

#[test]
fn test_retract_all_removes_all_triples() {
    let mut conn = setup_test_db();

    Class::new("foundation:Person").assert(
        &mut conn, ClassType::OwlClass, "Person", "person", Some("foundation:Agent"), "test",
    ).unwrap();

    Class::retract_all(&mut conn, "foundation:Person", "test").unwrap();

    let remaining = crate::eavto::query::get_by_entity(&conn, "foundation:Person").unwrap();
    assert!(remaining.triples.is_empty(), "All triples should be retracted");
}

#[test]
fn test_retract_all_noop_on_nonexistent_class() {
    let mut conn = setup_test_db();

    let result = Class::retract_all(&mut conn, "foundation:Ghost", "test");
    assert!(result.is_ok(), "retract_all on non-existent class should not error");
}

#[test]
fn test_retract_all_does_not_affect_other_classes() {
    let mut conn = setup_test_db();

    Class::new("foundation:Person").assert(
        &mut conn, ClassType::OwlClass, "Person", "person", None, "test",
    ).unwrap();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Task", "task", None, "test",
    ).unwrap();

    Class::retract_all(&mut conn, "foundation:Person", "test").unwrap();

    assert!(Class::get(&conn, "foundation:Person").unwrap().is_none());
    assert!(Class::get(&conn, "foundation:Task").unwrap().is_some(),
        "Other classes should be unaffected");
}

#[test]
fn test_retract_all_class_no_longer_in_find_all_iris() {
    let mut conn = setup_test_db();

    Class::new("foundation:Person").assert(
        &mut conn, ClassType::OwlClass, "Person", "person", None, "test",
    ).unwrap();

    let before = Class::find_all_iris(&conn).unwrap();
    assert!(before.contains(&"foundation:Person".to_string()));

    Class::retract_all(&mut conn, "foundation:Person", "test").unwrap();

    let after = Class::find_all_iris(&conn).unwrap();
    assert!(!after.contains(&"foundation:Person".to_string()),
        "Retracted class should not appear in find_all_iris");
}

// ── set_label ─────────────────────────────────────────────────────────────

#[test]
fn test_set_label_updates_label() {
    let mut conn = setup_test_db();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Old Label", "https://example.com/icon.svg", None, "test",
    ).unwrap();

    Class::set_label(&mut conn, "foundation:Task", "New Label", "test").unwrap();

    let class = Class::get(&conn, "foundation:Task").unwrap().unwrap();
    assert_eq!(class.label, Some("New Label".to_string()));
}

#[test]
fn test_set_label_retracts_old_label() {
    let mut conn = setup_test_db();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Old Label", "https://example.com/icon.svg", None, "test",
    ).unwrap();

    Class::set_label(&mut conn, "foundation:Task", "New Label", "test").unwrap();

    let retracted: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'rdfs:label' AND retracted = 1",
        [],
        |row| row.get(0),
    ).unwrap();
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject = 'foundation:Task' AND predicate = 'rdfs:label'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(retracted, 1, "Old label should be retracted");
    assert_eq!(active, 1, "Only the new label should be active");
}

// ── set_comment ───────────────────────────────────────────────────────────

#[test]
fn test_set_comment_adds_comment() {
    let mut conn = setup_test_db();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Task", "https://example.com/icon.svg", None, "test",
    ).unwrap();

    Class::set_comment(&mut conn, "foundation:Task", "A task entity", "test").unwrap();

    let class = Class::get(&conn, "foundation:Task").unwrap().unwrap();
    assert_eq!(class.comment, Some("A task entity".to_string()));
}

#[test]
fn test_set_comment_replaces_existing_comment() {
    let mut conn = setup_test_db();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Task", "https://example.com/icon.svg", None, "test",
    ).unwrap();
    Class::set_comment(&mut conn, "foundation:Task", "First comment", "test").unwrap();
    Class::set_comment(&mut conn, "foundation:Task", "Updated comment", "test").unwrap();

    let class = Class::get(&conn, "foundation:Task").unwrap().unwrap();
    assert_eq!(class.comment, Some("Updated comment".to_string()));

    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject = 'foundation:Task' AND predicate = 'rdfs:comment'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(active, 1, "Only one active comment should exist");
}

// ── set_icon ──────────────────────────────────────────────────────────────

#[test]
fn test_set_icon_url_icon_stores_as_has_icon_literal() {
    let mut conn = setup_test_db();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Task", "https://example.com/original.svg", None, "test",
    ).unwrap();

    Class::set_icon(&mut conn, "foundation:Task", "https://example.com/new.svg", "test").unwrap();

    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject = 'foundation:Task' AND predicate = 'foundation:hasIcon' AND object_type = 'literal'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(active, 1);
}

// ── set_super_class ───────────────────────────────────────────────────────

#[test]
fn test_set_super_class_updates_parent() {
    let mut conn = setup_test_db();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg",
        Some("foundation:Work"), "test",
    ).unwrap();

    Class::set_super_class(&mut conn, "foundation:Task", "foundation:Activity", "test").unwrap();

    let class = Class::get(&conn, "foundation:Task").unwrap().unwrap();
    let super_iris: Vec<&str> = class.super_classes.iter().map(|t| t.iri.as_str()).collect();
    assert!(super_iris.contains(&"foundation:Activity"),
        "New super class should be set, got: {:?}", super_iris);
    assert!(!super_iris.contains(&"foundation:Work"),
        "Old super class should be removed, got: {:?}", super_iris);
}

#[test]
fn test_set_super_class_replaces_old() {
    let mut conn = setup_test_db();
    Class::new("foundation:Task").assert(
        &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg",
        Some("foundation:Work"), "test",
    ).unwrap();

    Class::set_super_class(&mut conn, "foundation:Task", "foundation:NewParent", "test").unwrap();

    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM triples_current WHERE subject = 'foundation:Task' AND predicate = 'rdfs:subClassOf' AND object IS NOT NULL",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(active, 1, "Only one active subClassOf should exist");
}

// ── get_properties_for_domain_classes_bounded ─────────────────────────────────

fn seed_prop(conn: &mut rusqlite::Connection, prop_iri: &str, label: &str, domain: &str) {
    use crate::eavto::store;
    store::assert_triples(conn, &[
        Triple::new(prop_iri, rdf::TYPE, Object::Iri(owl::DATATYPE_PROPERTY.to_string())),
        Triple::new(prop_iri, rdfs::LABEL, Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new(prop_iri, rdfs::DOMAIN, Object::Iri(domain.to_string())),
    ], "test").unwrap();
}

#[test]
fn test_get_properties_bounded_bound_in_sql() {
    let mut conn = setup_test_db();
    for i in 0..5 {
        seed_prop(&mut conn, &format!("test:prop{}", i), &format!("Prop {}", i), "test:Widget");
    }
    let rows = Class::get_properties_for_domain_classes_bounded(
        &conn,
        &["test:Widget".to_string()],
        &[],
        2,
        0,
    ).unwrap();
    assert_eq!(rows.len(), 2, "limit=2 deve retornar exatamente 2 propriedades");
}

#[test]
fn test_get_properties_bounded_inheritance() {
    let mut conn = setup_test_db();

    Class::new("test:Animal").assert(&mut conn, ClassType::OwlClass, "Animal", "a", None, "test").unwrap();
    Class::new("test:Dog").assert(&mut conn, ClassType::OwlClass, "Dog", "d", Some("test:Animal"), "test").unwrap();

    seed_prop(&mut conn, "test:hasName", "has name", "test:Animal");
    seed_prop(&mut conn, "test:hasBone", "has bone", "test:Dog");

    let rows = Class::get_properties_for_domain_classes_bounded(
        &conn,
        &["test:Dog".to_string(), "test:Animal".to_string()],
        &[],
        500,
        0,
    ).unwrap();
    let prop_iris: Vec<&str> = rows.iter().map(|(iri, ..)| iri.as_str()).collect();
    assert!(prop_iris.contains(&"test:hasName"), "propriedade da classe-pai deve aparecer");
    assert!(prop_iris.contains(&"test:hasBone"), "propriedade da classe-filha deve aparecer");
}

#[test]
fn test_get_properties_bounded_has_more_via_offset() {
    let mut conn = setup_test_db();
    for i in 0..4 {
        seed_prop(&mut conn, &format!("test:q{}", i), &format!("Q {}", i), "test:Box");
    }
    let page1 = Class::get_properties_for_domain_classes_bounded(
        &conn, &["test:Box".to_string()], &[], 2, 0,
    ).unwrap();
    let page2 = Class::get_properties_for_domain_classes_bounded(
        &conn, &["test:Box".to_string()], &[], 2, 2,
    ).unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 2);
    let p1_iris: Vec<_> = page1.iter().map(|(iri, ..)| iri.clone()).collect();
    for (iri, ..) in &page2 {
        assert!(!p1_iris.contains(iri), "sem sobreposição entre páginas");
    }
}

#[test]
fn test_get_properties_bounded_limit_zero_means_no_cap() {
    let mut conn = setup_test_db();
    for i in 0..6 {
        seed_prop(&mut conn, &format!("test:r{}", i), &format!("R {}", i), "test:Gadget");
    }
    let rows = Class::get_properties_for_domain_classes_bounded(
        &conn, &["test:Gadget".to_string()], &[], 0, 0,
    ).unwrap();
    assert_eq!(rows.len(), 6, "limit=0 deve retornar todas as propriedades sem cap");
}
