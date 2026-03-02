// ============================================================================
// OWL Individual - Individual/Instance Operations
// ============================================================================
// High-level operations for managing individuals (instances of classes)
//
// IMPORTANT: Individuals are instances, NOT classes
// - Individuals use rdf:type to declare their class
// - Individuals NEVER use rdfs:subClassOf (that's for classes)
// - Example: foundation:John rdf:type foundation:Person (NOT subClassOf)
// ============================================================================

use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, Thing, Class, vocabulary::{rdf, rdfs}};

/// Represents an OWL Individual (instance of a class)
///
/// An Individual is an instance of a Class, not a Class itself.
/// It uses rdf:type to declare its class membership.
///
/// Example:
/// ```text
/// foundation:John rdf:type foundation:Person .  // John is an instance
/// foundation:Person rdf:type owl:Class .         // Person is a class
/// ```
#[derive(Debug, Clone)]
pub struct Individual {
    pub iri: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub comment: Option<String>,
    pub types: Vec<Thing>,
    pub properties: Vec<(String, Object)>, // (property_iri, value)
    pub backlinks: Vec<(String, String, Object)>, // (source_entity, property_iri, value)
}

impl Individual {
    /// Create a new empty Individual reference (only IRI)
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            iri: iri.into(),
            label: None,
            icon: None,
            comment: None,
            types: Vec::new(),
            properties: Vec::new(),
            backlinks: Vec::new(),
        }
    }

    /// Get complete individual data from database
    pub fn get(conn: &Connection, iri: impl Into<String>) -> Result<Self> {
        let iri = iri.into();

        // Get label
        let label_result = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL)?;
        let label = label_result.triples.first()
            .and_then(|t| t.object.as_literal());

        // Get icon
        let icon_result = query::get_by_entity_predicate(conn, &iri, "foundation:icon")?;
        let icon = icon_result.triples.first()
            .and_then(|t| t.object.as_literal());

        // Get comment
        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT)?;
        let comment = comment_result.triples.first()
            .and_then(|t| t.object.as_literal());

        // Get types (classes)
        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE)?;
        let types: Vec<Thing> = types_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|type_iri| Thing::get(conn, type_iri))
            .collect();

        // Get all properties (excluding metadata like label, icon, comment)
        let all_triples = query::get_by_entity(conn, &iri)?;
        let properties: Vec<(String, Object)> = all_triples.triples.into_iter()
            .filter(|t| {
                t.predicate != rdfs::LABEL
                    && t.predicate != rdfs::COMMENT
                    && t.predicate != "foundation:icon"
            })
            .map(|t| (t.predicate, t.object))
            .collect();

        // Get backlinks - entities that reference this individual
        let backlinks_result = query::get_by_object(conn, &iri)?;
        let backlinks: Vec<(String, String, Object)> = backlinks_result.triples.iter()
            .filter(|t| t.subject != iri && t.predicate != rdf::TYPE)
            .map(|t| (t.subject.clone(), t.predicate.clone(), t.object.clone()))
            .collect();

        Ok(Self {
            iri: iri.clone(),
            label,
            icon,
            comment,
            types,
            properties,
            backlinks,
        })
    }

    /// Assert individual with required metadata (label and icon)
    /// This is the recommended way to create individuals
    pub fn assert(
        &self,
        conn: &mut Connection,
        class_iri: &str,
        label: &str,
        icon: &str,
        origin: &str
    ) -> Result<()> {
        // Create individual type
        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(class_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;

        // Add required label
        let label_obj = Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let label_triple = Triple::new(&self.iri, rdfs::LABEL, label_obj);
        store::assert_triples(conn, &[label_triple], origin)?;

        // Add required icon
        let icon_obj = Object::Literal {
            value: icon.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let icon_triple = Triple::new(&self.iri, "foundation:icon", icon_obj);
        store::assert_triples(conn, &[icon_triple], origin)?;

        Ok(())
    }

    /// Add a property to this individual
    /// Validates that:
    /// 1. The property is defined in the individual's class or inherited from parent classes
    /// 2. Adding this value won't violate cardinality constraints
    /// 3. If the property range has owl:oneOf, the value must be one of the enumerated values
    pub fn add_property(
        &self,
        conn: &mut Connection,
        property: &str,
        value: Object,
        origin: &str,
    ) -> Result<()> {
        // Get individual's types (classes)
        let types_result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;

        if types_result.triples.is_empty() {
            return Err(OwlError::NotFound(format!("Individual {} has no rdf:type", self.iri)));
        }

        // Check if property is valid for any of the individual's classes
        let mut property_is_valid = false;

        for triple in &types_result.triples {
            if let Some(class_iri) = triple.object.as_iri() {
                // Get class with all its properties (including inherited)
                if let Ok(class) = Class::get(conn, class_iri) {
                    // Check if property exists in this class or its parents
                    if class.properties.iter().any(|(prop_iri, _)| prop_iri == property) {
                        property_is_valid = true;
                        break;
                    }
                }
            }
        }

        if !property_is_valid {
            return Err(OwlError::InvalidOperation(
                format!(
                    "Property {} is not defined in any class of individual {}",
                    property, self.iri
                )
            ));
        }

        // Validate owl:oneOf constraint on property range
        Self::validate_one_of_constraint(conn, property, &value)?;

        // After automatic retraction at EAVTO layer, we'll have exactly 1 value
        // Validate cardinality constraints for that single new value
        let new_count = 1;
        crate::owl::cardinality::validate_property_cardinality(
            conn,
            &self.iri,
            property,
            new_count
        )?;

        // Property is valid and cardinality is satisfied
        // EAVTO layer will automatically retract old values and assert the new triple
        let triple = Triple::new(&self.iri, property, value);
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Validate that a value conforms to owl:oneOf constraint on the property's range
    fn validate_one_of_constraint(conn: &Connection, property: &str, value: &Object) -> Result<()> {
        use crate::owl::vocabulary::{rdfs, owl};

        // Only validate for IRI values (owl:oneOf only applies to object properties)
        let value_iri = match value.as_iri() {
            Some(iri) => iri,
            None => return Ok(()), // Literals are not constrained by owl:oneOf
        };

        // Get property's range
        let range_result = query::get_by_entity_predicate(conn, property, rdfs::RANGE)?;

        if let Some(range_triple) = range_result.triples.first() {
            if let Some(range_class) = range_triple.object.as_iri() {
                // Check if the range class has owl:oneOf constraint
                let one_of_result = query::get_by_entity_predicate(conn, range_class, owl::ONE_OF)?;

                if let Some(one_of_triple) = one_of_result.triples.first() {
                    if let Some(list_head) = one_of_triple.object.as_iri() {
                        // Parse the owl:oneOf list
                        let allowed_values = Class::parse_rdf_list(conn, list_head)?;

                        // Check if the value is in the allowed list
                        if !allowed_values.contains(&value_iri.to_string()) {
                            let allowed = allowed_values.join(", ");
                            let msg = format!(
                                "Value '{}' is not allowed for property '{}'.",
                                value_iri, property,
                            );
                            return Err(OwlError::ValidationError(
                                format!("{} Allowed values: {}", msg, allowed)
                            ));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if this individual exists (has at least one triple)
    pub fn exists(&self, conn: &Connection) -> Result<bool> {
        let result = query::get_by_entity(conn, &self.iri)?;
        Ok(!result.triples.is_empty())
    }

    /// Retract all triples for the given entity IRI
    pub fn retract(conn: &mut Connection, iri: &str, origin: &str) -> Result<()> {
        let all_triples = query::get_by_entity(conn, iri)?;
        if !all_triples.triples.is_empty() {
            store::retract_triples(conn, &all_triples.triples, origin)?;
        }
        Ok(())
    }

    /// Find individuals of a specific class that match property constraints
    ///
    /// This uses an efficient SQL JOIN query to find all individuals matching all criteria.
    /// Can be used with one or multiple properties.
    ///
    /// Example:
    /// ```ignore
    /// // Single property
    /// let releases = Individual::find_by_class_and_properties(
    ///     conn,
    ///     "foundation:SoftwareRelease",
    ///     &[("foundation:versionNumber", "0.1.0")]
    /// )?;
    ///
    /// // Multiple properties
    /// let releases = Individual::find_by_class_and_properties(
    ///     conn,
    ///     "foundation:SoftwareRelease",
    ///     &[
    ///         ("foundation:versionNumber", "0.1.0"),
    ///         ("foundation:releaseOf", "foundation:FoundationProduct"),
    ///     ]
    /// )?;
    /// ```
    pub fn find_by_class_and_properties(
        conn: &Connection,
        class_iri: &str,
        properties: &[(&str, &str)],
    ) -> Result<Vec<String>> {
        query::find_by_class_and_properties(conn, class_iri, properties)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Class, ClassType, Property, PropertyType, vocabulary::{rdf, owl}};

    #[test]
    fn test_owl_one_of_validation_success() {
        let mut conn = setup_test_db();

        // Create Task class
        let task_class = Class::new("foundation:Task");
        task_class.assert(
            &mut conn, ClassType::OwlClass, "Task", "task-icon", None, "test",
        ).unwrap();

        // Create TaskPriority enumeration class
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

        // Create RDF list for owl:oneOf
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

        // Add owl:oneOf constraint
        let one_of = Triple::new(
            "foundation:TaskPriority",
            owl::ONE_OF,
            Object::Iri("_:list1".to_string()),
        );
        store::assert_triples(&mut conn, &[one_of], "test").unwrap();

        // Create priority property
        let priority_prop = Property::new("foundation:priority");
        priority_prop.assert(
            &mut conn,
            PropertyType::ObjectProperty,
            "priority",
            None,
            Some("foundation:Task"),
            Some("foundation:TaskPriority"),
            None,
            "test",
        ).unwrap();

        // Create task individual
        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "task-icon", "test").unwrap();

        // Adding valid priority should succeed
        let result = task.add_property(
            &mut conn,
            "foundation:priority",
            Object::Iri("foundation:HighPriority".to_string()),
            "test",
        );
        assert!(result.is_ok(), "Should accept valid enumerated value");
    }

    #[test]
    fn test_owl_one_of_validation_failure() {
        let mut conn = setup_test_db();

        // Create Task class
        let task_class = Class::new("foundation:Task");
        task_class.assert(
            &mut conn, ClassType::OwlClass, "Task", "task-icon", None, "test",
        ).unwrap();

        // Create TaskPriority enumeration class
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
        store::assert_triples(&mut conn, &[high, medium], "test").unwrap();

        // Create RDF list with only High and Medium
        let list2 = Triple::new(
            "_:list2",
            rdf::FIRST,
            Object::Iri("foundation:MediumPriority".to_string()),
        );
        let list2_rest = Triple::new("_:list2", rdf::REST, Object::Iri(rdf::NIL.to_string()));
        let list1 = Triple::new(
            "_:list1",
            rdf::FIRST,
            Object::Iri("foundation:HighPriority".to_string()),
        );
        let list1_rest = Triple::new("_:list1", rdf::REST, Object::Iri("_:list2".to_string()));
        store::assert_triples(&mut conn, &[list1, list1_rest, list2, list2_rest], "test").unwrap();

        // Add owl:oneOf constraint
        let one_of = Triple::new(
            "foundation:TaskPriority",
            owl::ONE_OF,
            Object::Iri("_:list1".to_string()),
        );
        store::assert_triples(&mut conn, &[one_of], "test").unwrap();

        // Create priority property
        let priority_prop = Property::new("foundation:priority");
        priority_prop.assert(
            &mut conn,
            PropertyType::ObjectProperty,
            "priority",
            None,
            Some("foundation:Task"),
            Some("foundation:TaskPriority"),
            None,
            "test",
        ).unwrap();

        // Create task individual
        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "task-icon", "test").unwrap();

        // Create an invalid value that's not in the owl:oneOf list
        let invalid = Triple::new(
            "foundation:LowPriority",
            rdf::TYPE,
            Object::Iri("foundation:TaskPriority".to_string()),
        );
        store::assert_triples(&mut conn, &[invalid], "test").unwrap();

        // Adding invalid priority should fail
        let result = task.add_property(
            &mut conn,
            "foundation:priority",
            Object::Iri("foundation:LowPriority".to_string()),
            "test",
        );
        assert!(result.is_err(), "Should reject invalid enumerated value");

        if let Err(OwlError::ValidationError(msg)) = result {
            assert!(msg.contains("not allowed"));
            assert!(msg.contains("foundation:LowPriority"));
        } else {
            panic!("Expected ValidationError");
        }
    }
}