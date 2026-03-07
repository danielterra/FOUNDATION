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

        let icon_result = query::get_by_entity_predicate(conn, &iri, "foundation:hasIcon")?;
        let icon = icon_result.triples.first()
            .and_then(|t| t.object.as_iri())
            .and_then(|iri| crate::owl::icon_iri_to_display(conn, iri))
            .or_else(|| {
                query::get_by_entity_predicate(conn, &iri, "foundation:icon").ok()
                    .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
            });

        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT)?;
        let comment = comment_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let types: Vec<Thing> = types_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|type_iri| Thing::get(conn, type_iri))
            .collect();

        let super_result = query::get_by_entity_predicate(conn, &iri, rdfs::SUB_CLASS_OF)?;
        let super_classes: Vec<Thing> = super_result.triples.iter()
            .filter_map(|t| match &t.object {
                Object::Iri(iri) => Some(iri.as_str()),
                _ => None,
            })
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
            .filter_map(|t| match &t.object {
                Object::Iri(iri) => Some(iri.clone()),
                _ => None,
            })
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

        let (icon_pred, icon_obj) = crate::owl::icon_store_value(icon);
        let icon_triple = Triple::new(&self.iri, icon_pred, icon_obj);
        store::assert_triples(conn, &[icon_triple], origin)?;

        let parent = super_class.unwrap_or(owl::THING);
        let subclass_triple =
            Triple::new(&self.iri, rdfs::SUB_CLASS_OF, Object::Iri(parent.to_string()));
        store::assert_triples(conn, &[subclass_triple], origin)?;

        Ok(())
    }


    /// Get all instances of this class and all its subclasses (polymorphic, returned as IRIs only)
    pub fn get_instances(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let descendant_iris = Self::get_descendant_iris(conn, class_iri)?;
        let mut seen = std::collections::HashSet::new();
        let mut instances = Vec::new();
        for iri in &descendant_iris {
            let result = query::get_by_predicate_object(conn, rdf::TYPE, iri)?;
            for t in result.triples {
                if seen.insert(t.subject.clone()) {
                    instances.push(t.subject);
                }
            }
        }
        Ok(instances)
    }

    /// Get all class IRIs (owl:Class and rdfs:Class)
    pub fn find_all_iris(conn: &Connection) -> Result<Vec<String>> {
        let owl_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::CLASS)?;
        let rdfs_result = query::get_by_predicate_object(conn, rdf::TYPE, rdfs::CLASS)?;
        let mut iris: Vec<String> = owl_result.triples.into_iter()
            .chain(rdfs_result.triples)
            .map(|t| t.subject)
            .collect();
        iris.sort();
        iris.dedup();
        Ok(iris)
    }

    /// Get IRIs of all direct subclasses
    pub fn get_subclass_iris(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, class_iri)?;
        Ok(result.triples.into_iter().map(|t| t.subject).collect())
    }

    /// Get the given class IRI plus all descendant class IRIs (BFS traversal of rdfs:subClassOf)
    pub fn get_descendant_iris(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        queue.push_back(class_iri.to_string());

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            result.push(current.clone());
            for child in Self::get_subclass_iris(conn, &current)? {
                if !visited.contains(&child) {
                    queue.push_back(child);
                }
            }
        }

        Ok(result)
    }

    /// Replace the label of an existing class
    pub fn set_label(conn: &mut Connection, iri: &str, label: &str, origin: &str) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::LABEL)?;
        for triple in old.triples {
            store::retract_triples(conn, &[Triple::new(iri, rdfs::LABEL, triple.object)], origin)?;
        }
        store::assert_triples(conn, &[Triple::new(iri, rdfs::LABEL, Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })], origin)?;
        Ok(())
    }

    /// Replace the comment of an existing class (or add one if not present)
    pub fn set_comment(conn: &mut Connection, iri: &str, comment: &str, origin: &str) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::COMMENT)?;
        for triple in old.triples {
            store::retract_triples(conn, &[Triple::new(iri, rdfs::COMMENT, triple.object)], origin)?;
        }
        store::assert_triples(conn, &[Triple::new(iri, rdfs::COMMENT, Object::Literal {
            value: comment.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })], origin)?;
        Ok(())
    }

    /// Replace the icon of an existing class (validates icon name)
    pub fn set_icon(conn: &mut Connection, iri: &str, icon: &str, origin: &str) -> Result<()> {
        crate::owl::validate_icon(conn, icon)?;
        let (icon_pred, icon_obj) = crate::owl::icon_store_value(icon);
        store::assert_triples(conn, &[Triple::new(iri, icon_pred, icon_obj)], origin)?;
        Ok(())
    }

    /// Replace all rdfs:subClassOf relationships with the given list.
    ///
    /// Only IRI-type subClassOf triples are replaced. Blank node triples
    /// (OWL restriction nodes added by set_class_required_fields) are preserved.
    pub fn set_super_classes(
        conn: &mut Connection,
        iri: &str,
        super_classes: &[&str],
        origin: &str,
    ) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::SUB_CLASS_OF)?;
        for triple in old.triples {
            if matches!(triple.object, Object::Iri(_)) {
                store::retract_triples(
                    conn,
                    &[Triple::new(iri, rdfs::SUB_CLASS_OF, triple.object)],
                    origin,
                )?;
            }
        }
        let new_triples: Vec<Triple> = super_classes
            .iter()
            .map(|sc| Triple::new(iri, rdfs::SUB_CLASS_OF, Object::Iri(sc.to_string())))
            .collect();
        store::append_triples(conn, &new_triples, origin)?;
        Ok(())
    }

    /// Replace the rdfs:subClassOf relationship of an existing class with a single superclass
    pub fn set_super_class(
        conn: &mut Connection,
        iri: &str,
        super_class: &str,
        origin: &str,
    ) -> Result<()> {
        Self::set_super_classes(conn, iri, &[super_class], origin)
    }

    /// Retract all triples about this class IRI
    pub fn retract_all(conn: &mut Connection, iri: &str, origin: &str) -> Result<()> {
        let result = query::get_by_entity(conn, iri)?;
        let triples: Vec<Triple> = result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        store::retract_triples(conn, &triples, origin)?;
        Ok(())
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
            "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'rdfs:label' AND retracted = 0",
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
            "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'rdfs:comment' AND retracted = 0",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(active, 1, "Only one active comment should exist");
    }

    // ── set_icon ──────────────────────────────────────────────────────────────

    #[test]
    fn test_set_icon_url_icon_stores_as_literal() {
        let mut conn = setup_test_db();
        Class::new("foundation:Task").assert(
            &mut conn, ClassType::OwlClass, "Task", "https://example.com/original.svg", None, "test",
        ).unwrap();

        Class::set_icon(&mut conn, "foundation:Task", "https://example.com/new.svg", "test").unwrap();

        let active: i64 = conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'foundation:icon' AND retracted = 0",
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
            "SELECT COUNT(*) FROM triples WHERE subject = 'foundation:Task' AND predicate = 'rdfs:subClassOf' AND retracted = 0 AND object IS NOT NULL",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(active, 1, "Only one active subClassOf should exist");
    }
}
