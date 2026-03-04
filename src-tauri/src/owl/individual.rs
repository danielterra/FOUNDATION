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
    pub property_tx: Vec<i64>, // transaction IDs parallel to properties
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
            property_tx: Vec::new(),
            backlinks: Vec::new(),
        }
    }

    pub fn get(conn: &Connection, iri: impl Into<String>) -> Result<Option<Self>> {
        let iri = iri.into();

        let all_triples = query::get_by_entity(conn, &iri)?;
        if all_triples.triples.is_empty() {
            return Ok(None);
        }

        let label = all_triples.triples.iter()
            .find(|t| t.predicate == rdfs::LABEL)
            .and_then(|t| t.object.as_literal());

        let icon = all_triples.triples.iter()
            .find(|t| t.predicate == "foundation:icon")
            .and_then(|t| t.object.as_literal());

        let comment = all_triples.triples.iter()
            .find(|t| t.predicate == rdfs::COMMENT)
            .and_then(|t| t.object.as_literal());

        let types: Vec<Thing> = all_triples.triples.iter()
            .filter(|t| t.predicate == rdf::TYPE)
            .filter_map(|t| t.object.as_iri())
            .map(|type_iri| Thing::get(conn, type_iri))
            .collect();

        let prop_triples: Vec<_> = all_triples.triples.into_iter()
            .filter(|t| {
                t.predicate != rdfs::LABEL
                    && t.predicate != rdfs::COMMENT
                    && t.predicate != "foundation:icon"
            })
            .collect();

        let property_tx: Vec<i64> = prop_triples.iter().map(|t| t.tx).collect();
        let properties: Vec<(String, Object)> = prop_triples.into_iter()
            .map(|t| (t.predicate, t.object))
            .collect();

        let backlinks_result = query::get_by_object(conn, &iri)?;
        let backlinks: Vec<(String, String, Object)> = backlinks_result.triples.iter()
            .filter(|t| t.subject != iri && t.predicate != rdf::TYPE)
            .map(|t| (t.subject.clone(), t.predicate.clone(), t.object.clone()))
            .collect();

        Ok(Some(Self {
            iri: iri.clone(),
            label,
            icon,
            comment,
            types,
            properties,
            property_tx,
            backlinks,
        }))
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
        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(class_iri.to_string()));
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

        Ok(())
    }

    /// Set property values for this individual, replacing all existing values.
    /// Validates that:
    /// 1. The property is defined in the individual's class or inherited from parent classes
    /// 2. The new value count won't violate cardinality constraints
    /// 3. If the property range has owl:oneOf, all values must be from the enumerated set
    ///
    /// Always retracts all current values and asserts the full new set atomically.
    /// Pass all desired values — this is always a full replace, never an append.
    pub fn add_property(
        &self,
        conn: &mut Connection,
        property: &str,
        values: Vec<Object>,
        origin: &str,
    ) -> Result<()> {
        if values.is_empty() {
            return Err(OwlError::InvalidOperation(
                format!("No values provided for property {}", property)
            ));
        }

        let is_meta_property = property.starts_with("rdfs:") || property == "foundation:icon";

        let types_result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;

        if types_result.triples.is_empty() {
            return Err(OwlError::NotFound(format!("Individual {} has no rdf:type", self.iri)));
        }

        if !is_meta_property {
            let mut property_is_valid = false;

            for triple in &types_result.triples {
                if let Some(class_iri) = triple.object.as_iri() {
                    if let Ok(Some(class)) = Class::get(conn, class_iri) {
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
        }

        for value in &values {
            Self::validate_one_of_constraint(conn, property, value)?;
        }

        crate::owl::cardinality::validate_property_cardinality(
            conn,
            &self.iri,
            property,
            values.len()
        )?;

        let triples: Vec<Triple> = values.into_iter()
            .map(|v| Triple::new(&self.iri, property, v))
            .collect();
        store::assert_triples(conn, &triples, origin)?;
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

        let range_result = query::get_by_entity_predicate(conn, property, rdfs::RANGE)?;

        if let Some(range_triple) = range_result.triples.first() {
            if let Some(range_class) = range_triple.object.as_iri() {
                let one_of_result = query::get_by_entity_predicate(conn, range_class, owl::ONE_OF)?;

                if let Some(one_of_triple) = one_of_result.triples.first() {
                    if let Some(list_head) = one_of_triple.object.as_iri() {
                        let allowed_values = Class::parse_rdf_list(conn, list_head)?;

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

    pub fn serializable_properties(&self, conn: &Connection) -> Vec<serde_json::Value> {
        use crate::owl::Property;

        self.properties.iter().map(|(prop_iri, value)| {
            let unit = Property::get(conn, prop_iri).ok()
                .flatten()
                .and_then(|p| p.unit);

            let json_value: serde_json::Value = match value {
                Object::Integer(i) => serde_json::json!(i),
                Object::Number(n) => serde_json::json!(n),
                Object::Boolean(b) => serde_json::json!(b),
                Object::Literal { value: v, datatype: Some(dt), .. }
                    if matches!(dt.as_str(), "xsd:decimal" | "xsd:float" | "xsd:double") =>
                {
                    v.parse::<f64>()
                        .map(|n| serde_json::json!(n))
                        .unwrap_or_else(|_| serde_json::json!(v))
                }
                Object::Literal { value: v, datatype: Some(dt), .. }
                    if dt.as_str() == "xsd:integer" =>
                {
                    v.parse::<i64>()
                        .map(|n| serde_json::json!(n))
                        .unwrap_or_else(|_| serde_json::json!(v))
                }
                _ => {
                    let s = value.as_literal()
                        .or_else(|| value.as_iri().map(|s| s.to_string()))
                        .unwrap_or_default();
                    serde_json::json!(s)
                }
            };

            let mut entry = serde_json::json!({
                "property": prop_iri,
                "value": json_value,
            });
            if let Some(unit_iri) = unit {
                entry["unit"] = serde_json::json!(unit_iri);
            }
            entry
        }).collect()
    }


    /// Retract all triples for the given entity IRI
    pub fn retract(conn: &mut Connection, iri: &str, origin: &str) -> Result<()> {
        let all_triples = query::get_by_entity(conn, iri)?;
        if !all_triples.triples.is_empty() {
            store::retract_triples(conn, &all_triples.triples, origin)?;
        }
        Ok(())
    }

    pub fn search(conn: &Connection) -> Result<Vec<String>> {
        let result = query::get_by_predicate(conn, rdf::TYPE)?;
        let mut seen = std::collections::HashSet::new();
        let iris = result.triples.into_iter()
            .filter_map(|t| {
                if let Some(class_iri) = t.object.as_iri() {
                    if !class_iri.starts_with("owl:") &&
                       !class_iri.starts_with("rdfs:") &&
                       !class_iri.starts_with("rdf:") &&
                       class_iri != "owl:Class" &&
                       seen.insert(t.subject.clone()) {
                        Some(t.subject)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        Ok(iris)
    }


    pub fn remove_property_value(
        conn: &mut Connection,
        iri: &str,
        property_iri: &str,
        value_str: &str,
        origin: &str,
    ) -> Result<Option<Object>> {
        let result = query::get_by_entity_predicate(conn, iri, property_iri)?;
        for triple in result.triples {
            let matches = match &triple.object {
                Object::Iri(s) => s.as_str() == value_str,
                Object::Blank(s) => s.as_str() == value_str,
                Object::Literal { value: v, .. } => v.as_str() == value_str,
                Object::Integer(i) => i.to_string() == value_str,
                Object::Number(n) => {
                    if let Ok(input) = value_str.parse::<f64>() {
                        (n - input).abs() < f64::EPSILON
                    } else {
                        n.to_string() == value_str
                    }
                },
                Object::Boolean(b) => b.to_string() == value_str,
                Object::DateTime(dt) => dt.to_string() == value_str,
            };
            if matches {
                let found = triple.object.clone();
                store::retract_triples(
                    conn, &[Triple::new(iri, property_iri, triple.object)], origin,
                )?;
                return Ok(Some(found));
            }
        }
        Ok(None)
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
            vec![Object::Iri("foundation:HighPriority".to_string())],
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
            vec![Object::Iri("foundation:LowPriority".to_string())],
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