use super::*;
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};
use crate::owl::{Class, ClassType, Individual, Property, PropertyType};

mod crud;
mod search;
mod automation;

fn setup_task_class_with_statuses(conn: &mut Connection) {
    let task_class = Class::new("foundation:Task");
    task_class.assert(conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test").unwrap();

    let triples = vec![
        Triple::new("foundation:ActiveStatus", "rdf:type", Object::Iri("foundation:Status".to_string())),
        Triple::new("foundation:ActiveStatus", "rdfs:label", Object::Literal {
            value: "Active".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:DoneStatus", "rdf:type", Object::Iri("foundation:Status".to_string())),
        Triple::new("foundation:DoneStatus", "rdfs:label", Object::Literal {
            value: "Done".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }),
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:ActiveStatus".to_string())),
        Triple::new("foundation:Task", "foundation:allowedStatus", Object::Iri("foundation:DoneStatus".to_string())),
    ];
    store::assert_triples(conn, &triples, "test").unwrap();

    Property::new("foundation:priority")
        .assert(conn, PropertyType::DatatypeProperty, "priority", None, &["foundation:Task"], Some("xsd:string"), None, "test")
        .unwrap();

    Property::new("foundation:hasStatus")
        .assert(conn, PropertyType::ObjectProperty, "hasStatus", None, &["foundation:Task"], None, None, "test")
        .unwrap();
}

fn create_task(conn: &mut Connection, iri: &str) {
    let individual = Individual::new(iri);
    individual.assert(conn, "foundation:Task", "Test Task", "https://example.com/icon.svg", "test").unwrap();
}

fn setup_event_hierarchy(conn: &mut Connection) {
    let event_class = Class::new("foundation:Event");
    event_class.assert(conn, ClassType::OwlClass, "Event", "https://example.com/event.svg", None, "test").unwrap();

    let vacation_class = Class::new("foundation:Vacation");
    vacation_class.assert(
        conn, ClassType::OwlClass, "Vacation", "https://example.com/vacation.svg",
        Some("foundation:Event"), "test",
    ).unwrap();

    Property::new("foundation:hasStatus")
        .assert(conn, PropertyType::ObjectProperty, "hasStatus", None, &["foundation:Event"], None, None, "test")
        .unwrap();
}

fn create_event(conn: &mut Connection, iri: &str, class_iri: &str) {
    let individual = Individual::new(iri);
    individual.assert(conn, class_iri, "Test Event", "https://example.com/event.svg", "test").unwrap();
}

fn setup_animal_hierarchy(conn: &mut Connection) {
    let animal_class = Class::new("foundation:Animal");
    animal_class.assert(conn, ClassType::OwlClass, "Animal", "https://example.com/animal.svg", None, "test").unwrap();

    let mammal_class = Class::new("foundation:Mammal");
    mammal_class.assert(
        conn, ClassType::OwlClass, "Mammal", "https://example.com/mammal.svg",
        Some("foundation:Animal"), "test",
    ).unwrap();

    let dog_class = Class::new("foundation:Dog");
    dog_class.assert(
        conn, ClassType::OwlClass, "Dog", "https://example.com/dog.svg",
        Some("foundation:Mammal"), "test",
    ).unwrap();
}

const ICON_URL: &str = "https://example.com/icon.svg";

fn setup_automation_hierarchy(conn: &mut Connection) {
    let i = ICON_URL;
    Class::new("foundation:automation_Process")
        .assert(conn, ClassType::OwlClass, "Automation Process", i, None, "test").unwrap();
    Class::new("foundation:automation_FlowNode")
        .assert(conn, ClassType::OwlClass, "Flow Node", i, None, "test").unwrap();
    Property::new("foundation:partOfProcess")
        .assert(conn, PropertyType::ObjectProperty, "Part of Process", None,
            &["foundation:automation_FlowNode"], Some("foundation:automation_Process"), None, "test")
        .unwrap();
    Class::new("foundation:automation_Event")
        .assert(conn, ClassType::OwlClass, "Automation Event", i,
            Some("foundation:automation_FlowNode"), "test").unwrap();
    Class::new("foundation:automation_StartEvent")
        .assert(conn, ClassType::OwlClass, "Start Event", i,
            Some("foundation:automation_Event"), "test").unwrap();
    Class::new("foundation:automation_Task")
        .assert(conn, ClassType::OwlClass, "Automation Task", i,
            Some("foundation:automation_FlowNode"), "test").unwrap();
    Class::new("foundation:automation_AgentTask")
        .assert(conn, ClassType::OwlClass, "Agent Task", i,
            Some("foundation:automation_Task"), "test").unwrap();
    Individual::new("foundation:Process_Reg")
        .assert(conn, "foundation:automation_Process", "Reg Process", i, "test").unwrap();
}

fn set_part_of_process(
    conn: &mut Connection, ind_iri: &str,
) -> Result<(), crate::owl::OwlError> {
    Individual::new(ind_iri).add_property(
        conn, "foundation:partOfProcess",
        vec![Object::Iri("foundation:Process_Reg".to_string())], "test",
    )
}
