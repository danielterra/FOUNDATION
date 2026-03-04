use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, Thing, vocabulary::{rdf, rdfs, owl}};

#[derive(Debug, Clone)]
pub struct Class {
    pub iri: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub comment: Option<String>,
    pub types: Vec<Thing>, // rdf:type (e.g., owl:Class, rdfs:Class)
    pub super_classes: Vec<Thing>,
    pub sub_classes: Vec<Thing>,
    pub properties: Vec<(String, String)>, // (property_iri, source_class_iri)
    pub backlinks: Vec<(String, String, Object)>, // (source_entity, property_iri, value)
    pub one_of_values: Vec<String>, // owl:oneOf enumerated individuals
}

impl Class {
    /// Create a new empty Class reference (only IRI)
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            iri: iri.into(),
            label: None,
            icon: None,
            comment: None,
            types: Vec::new(),
            super_classes: Vec::new(),
            sub_classes: Vec::new(),
            properties: Vec::new(),
            backlinks: Vec::new(),
            one_of_values: Vec::new(),
        }
    }

    /// Parse an RDF list (rdf:first/rdf:rest) into a Vec of IRIs
    pub(crate) fn parse_rdf_list(conn: &Connection, list_head: &str) -> Result<Vec<String>> {
        let mut values = Vec::new();
        let mut current = list_head.to_string();

        loop {
            if current == rdf::NIL {
                break;
            }

            let first_result = query::get_by_entity_predicate(conn, &current, rdf::FIRST)?;
            if let Some(triple) = first_result.triples.first() {
                if let Some(iri) = triple.object.as_iri() {
                    values.push(iri.to_string());
                }
            }

            let rest_result = query::get_by_entity_predicate(conn, &current, rdf::REST)?;
            if let Some(triple) = rest_result.triples.first() {
                if let Some(iri) = triple.object.as_iri() {
                    current = iri.to_string();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(values)
    }

    /// Get complete class data from database
    pub fn get(conn: &Connection, iri: impl Into<String>) -> Result<Option<Self>> {
        let iri = iri.into();

        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE)?;
        let is_class = types_result.triples.iter().any(|t| {
            t.object.as_iri()
                .map(|type_iri| type_iri == rdfs::CLASS || type_iri == owl::CLASS)
                .unwrap_or(false)
        });
        if !is_class {
            return Ok(None);
        }

        let label_result = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL)?;
        let label = label_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let icon_result = query::get_by_entity_predicate(conn, &iri, "foundation:icon")?;
        let icon = icon_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT)?;
        let comment = comment_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let types: Vec<Thing> = types_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|type_iri| Thing::get(conn, type_iri))
            .collect();

        let super_result = query::get_by_entity_predicate(conn, &iri, rdfs::SUB_CLASS_OF)?;
        let super_classes: Vec<Thing> = super_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|super_iri| Thing::get(conn, super_iri))
            .collect();

        let sub_result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, &iri)?;
        let sub_classes: Vec<Thing> = sub_result.triples.iter()
            .map(|t| Thing::get(conn, &t.subject))
            .collect();

        let properties = Self::get_properties(conn, &iri)?;

        let backlinks_result = query::get_by_predicate_object(conn, rdf::TYPE, &iri)?;
        let backlinks: Vec<(String, String, Object)> = backlinks_result.triples.iter()
            .map(|t| {
                (t.subject.clone(), rdf::TYPE.to_string(), Object::Iri(iri.clone()))
            })
            .collect();

        let one_of_result = query::get_by_entity_predicate(conn, &iri, owl::ONE_OF)?;
        let one_of_values = if let Some(triple) = one_of_result.triples.first() {
            if let Some(list_head) = triple.object.as_iri() {
                Self::parse_rdf_list(conn, list_head)?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(Some(Self {
            iri,
            label,
            icon,
            comment,
            types,
            super_classes,
            sub_classes,
            properties,
            backlinks,
            one_of_values,
        }))
    }

    /// Get all properties for this class (declared, used, and inherited)
    /// Returns Vec<(property_iri, source_class_iri)>
    fn get_properties(
        conn: &Connection,
        class_iri: &str
    ) -> Result<Vec<(String, String)>> {
        let mut all_properties: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let declared_result = query::get_by_predicate_object(conn, rdfs::DOMAIN, class_iri)?;
        for triple in declared_result.triples {
            if seen.insert(triple.subject.clone()) {
                all_properties.push((triple.subject.clone(), class_iri.to_string()));
            }
        }

        for universal_class in &["owl:Thing", "rdfs:Resource"] {
            let universal_props_result =
                query::get_by_predicate_object(conn, rdfs::DOMAIN, universal_class)?;
            for triple in universal_props_result.triples {
                if seen.insert(triple.subject.clone()) {
                    all_properties.push((triple.subject.clone(), universal_class.to_string()));
                }
            }
        }

        let super_result = query::get_by_entity_predicate(conn, class_iri, rdfs::SUB_CLASS_OF)?;
        let super_classes: Vec<String> = super_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        for super_class_iri in super_classes {
            if super_class_iri != "owl:Thing" && super_class_iri != "rdfs:Resource" {
                let inherited_props = Self::get_properties(conn, &super_class_iri)?;
                for (prop, source) in inherited_props {
                    if seen.insert(prop.clone()) {
                        all_properties.push((prop, source));
                    }
                }
            }
        }

        Ok(all_properties)
    }

    /// Assert class with required metadata (label and icon)
    /// If super_class is None, automatically assigns owl:Thing as parent
    pub fn assert(
        &self,
        conn: &mut Connection,
        class_type: ClassType,
        label: &str,
        icon: &str,
        super_class: Option<&str>,
        origin: &str
    ) -> Result<()> {
        let type_iri = match class_type {
            ClassType::RdfsClass => rdfs::CLASS,
            ClassType::OwlClass => owl::CLASS,
        };

        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(type_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;

        let label_obj = Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let label_triple = Triple::new(&self.iri, rdfs::LABEL, label_obj);
        store::assert_triples(conn, &[label_triple], origin)?;

        let icon_obj = Object::Literal {
            value: icon.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let icon_triple = Triple::new(&self.iri, "foundation:icon", icon_obj);
        store::assert_triples(conn, &[icon_triple], origin)?;

        let parent = super_class.unwrap_or(owl::THING);
        let subclass_triple =
            Triple::new(&self.iri, rdfs::SUB_CLASS_OF, Object::Iri(parent.to_string()));
        store::assert_triples(conn, &[subclass_triple], origin)?;

        Ok(())
    }


    /// Get all instances of this class (returned as IRIs only)
    /// Call separately when needed - can be thousands of instances
    pub fn get_instances(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let result = query::get_by_predicate_object(conn, rdf::TYPE, class_iri)?;
        Ok(result.triples.iter()
            .map(|t| t.subject.clone())
            .collect())
    }
}

/// Type of class (RDFS or OWL)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassType {
    #[allow(dead_code)]
    RdfsClass,
    OwlClass,
}

#[cfg(test)]
mod tests {
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
}
