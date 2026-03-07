use super::*;

impl Individual {
    /// Validate that a literal value conforms to its declared xsd datatype
    pub(super) fn validate_literal_datatype(property: &str, value: &Object) -> Result<()> {
        let (raw, datatype) = match value {
            Object::Literal { value, datatype: Some(dt), .. } => (value.as_str(), dt.as_str()),
            _ => return Ok(()),
        };

        match datatype {
            "xsd:dateTime" => {
                let valid = raw.parse::<i64>().is_ok()
                    || chrono::DateTime::parse_from_rfc3339(raw).is_ok();
                if !valid {
                    return Err(OwlError::ValidationError(format!(
                        "Property {}: '{}' is not a valid xsd:dateTime \
                         (expected Unix milliseconds i64, e.g. '1772380322157', \
                         or RFC3339, e.g. '2026-03-06T12:00:00-03:00')",
                        property, raw
                    )));
                }
            }
            "xsd:date" => {
                chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
                    OwlError::ValidationError(format!(
                        "Property {}: '{}' is not a valid xsd:date \
                         (expected YYYY-MM-DD, e.g. '2025-01-28')",
                        property, raw
                    ))
                })?;
            }
            "xsd:integer" | "xsd:long" | "xsd:int" | "xsd:short" => {
                raw.parse::<i64>().map_err(|_| {
                    OwlError::ValidationError(format!(
                        "Property {}: '{}' is not a valid {}", property, raw, datatype
                    ))
                })?;
            }
            "xsd:decimal" | "xsd:float" | "xsd:double" => {
                raw.parse::<f64>().map_err(|_| {
                    OwlError::ValidationError(format!(
                        "Property {}: '{}' is not a valid {}", property, raw, datatype
                    ))
                })?;
            }
            "xsd:boolean" => {
                if raw != "true" && raw != "false" {
                    return Err(OwlError::ValidationError(format!(
                        "Property {}: '{}' is not a valid xsd:boolean (expected 'true' or 'false')",
                        property, raw
                    )));
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Validate that the value types match the property's declared type (ObjectProperty vs DatatypeProperty)
    pub(super) fn validate_value_type(conn: &Connection, property: &str, values: &[Object]) -> Result<()> {
        use crate::owl::{Property, PropertyType};

        let prop = match Property::get(conn, property)? {
            Some(p) => p,
            None => return Ok(()),
        };

        match prop.property_type {
            PropertyType::ObjectProperty => {
                for value in values {
                    if value.as_iri().is_none() {
                        let range_hint = if !prop.ranges.is_empty() {
                            format!(" (expected an IRI of type {})", prop.ranges.join(" or "))
                        } else {
                            " (expected an IRI)".to_string()
                        };
                        return Err(OwlError::ValidationError(format!(
                            "Property '{}' is an ObjectProperty{}, but got a literal value: '{}'",
                            property, range_hint,
                            value.as_literal().unwrap_or_default()
                        )));
                    }
                }
            }
            PropertyType::DatatypeProperty => {
                for value in values {
                    if value.as_iri().is_some() {
                        let range_hint = if !prop.ranges.is_empty() {
                            format!(" (expected a {} literal)", prop.ranges.join(" or "))
                        } else {
                            " (expected a literal value)".to_string()
                        };
                        return Err(OwlError::ValidationError(format!(
                            "Property '{}' is a DatatypeProperty{}, but got an IRI: '{}'",
                            property, range_hint,
                            value.as_iri().unwrap()
                        )));
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Validate that an IRI value exists in the graph before referencing it
    pub(super) fn validate_iri_exists(conn: &Connection, property: &str, value: &Object) -> Result<()> {
        let value_iri = match value.as_iri() {
            Some(iri) => iri,
            None => return Ok(()),
        };

        let result = query::get_by_entity(conn, value_iri)?;
        if result.triples.is_empty() {
            return Err(OwlError::ValidationError(format!(
                "IRI '{}' does not exist in the graph. \
                 Cannot set property '{}' to a non-existent resource.",
                value_iri, property
            )));
        }

        Ok(())
    }

    /// Validate that a value conforms to owl:oneOf constraint on the property's range
    pub(super) fn validate_one_of_constraint(conn: &Connection, property: &str, value: &Object) -> Result<()> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Class, ClassType, Property, PropertyType, vocabulary::{rdf, owl}};

    #[test]
    fn test_owl_one_of_validation_success() {
        let mut conn = setup_test_db();

        let task_class = Class::new("foundation:Task");
        task_class.assert(
            &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test",
        ).unwrap();

        let priority_class = Class::new("foundation:TaskPriority");
        priority_class.assert(
            &mut conn,
            ClassType::OwlClass,
            "Task Priority",
            "https://example.com/priority.svg",
            None,
            "test",
        ).unwrap();

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

        let one_of = Triple::new(
            "foundation:TaskPriority",
            owl::ONE_OF,
            Object::Iri("_:list1".to_string()),
        );
        store::assert_triples(&mut conn, &[one_of], "test").unwrap();

        let priority_prop = Property::new("foundation:priority");
        priority_prop.assert(
            &mut conn,
            PropertyType::ObjectProperty,
            "priority",
            None,
            &["foundation:Task"],
            Some("foundation:TaskPriority"),
            None,
            "test",
        ).unwrap();

        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").unwrap();

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

        let task_class = Class::new("foundation:Task");
        task_class.assert(
            &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test",
        ).unwrap();

        let priority_class = Class::new("foundation:TaskPriority");
        priority_class.assert(
            &mut conn,
            ClassType::OwlClass,
            "Task Priority",
            "https://example.com/priority.svg",
            None,
            "test",
        ).unwrap();

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

        let one_of = Triple::new(
            "foundation:TaskPriority",
            owl::ONE_OF,
            Object::Iri("_:list1".to_string()),
        );
        store::assert_triples(&mut conn, &[one_of], "test").unwrap();

        let priority_prop = Property::new("foundation:priority");
        priority_prop.assert(
            &mut conn,
            PropertyType::ObjectProperty,
            "priority",
            None,
            &["foundation:Task"],
            Some("foundation:TaskPriority"),
            None,
            "test",
        ).unwrap();

        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").unwrap();

        let invalid = Triple::new(
            "foundation:LowPriority",
            rdf::TYPE,
            Object::Iri("foundation:TaskPriority".to_string()),
        );
        store::assert_triples(&mut conn, &[invalid], "test").unwrap();

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

    #[test]
    fn test_iri_existence_validation() {
        let mut conn = setup_test_db();

        let task_class = Class::new("foundation:Task");
        task_class.assert(
            &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test",
        ).unwrap();

        let prop = Property::new("foundation:assignedTo");
        prop.assert(
            &mut conn,
            PropertyType::ObjectProperty,
            "assignedTo",
            None,
            &["foundation:Task"],
            None,
            None,
            "test",
        ).unwrap();

        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").unwrap();

        let result = task.add_property(
            &mut conn,
            "foundation:assignedTo",
            vec![Object::Iri("foundation:NonExistentUser".to_string())],
            "test",
        );
        assert!(result.is_err(), "Should reject reference to non-existent IRI");
        if let Err(OwlError::ValidationError(msg)) = result {
            assert!(msg.contains("foundation:NonExistentUser"));
            assert!(msg.contains("does not exist"));
        } else {
            panic!("Expected ValidationError");
        }

        let user = Individual::new("foundation:NonExistentUser");
        user.assert(&mut conn, "foundation:Task", "A User", "https://example.com/person.svg", "test").unwrap();

        let result = task.add_property(
            &mut conn,
            "foundation:assignedTo",
            vec![Object::Iri("foundation:NonExistentUser".to_string())],
            "test",
        );
        assert!(result.is_ok(), "Should accept reference to existing IRI");
    }

    #[test]
    fn test_value_type_mismatch_validation() {
        let mut conn = setup_test_db();

        let task_class = Class::new("foundation:Task");
        task_class.assert(
            &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test",
        ).unwrap();

        let obj_prop = Property::new("foundation:relatedTo");
        obj_prop.assert(
            &mut conn, PropertyType::ObjectProperty, "relatedTo",
            None, &["foundation:Task"], None, None, "test",
        ).unwrap();

        let dt_prop = Property::new("foundation:title");
        dt_prop.assert(
            &mut conn, PropertyType::DatatypeProperty, "title",
            None, &["foundation:Task"], Some("xsd:string"), None, "test",
        ).unwrap();

        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").unwrap();

        let result = task.add_property(
            &mut conn, "foundation:relatedTo",
            vec![Object::Literal { value: "some-string".to_string(), datatype: Some("xsd:string".to_string()), language: None }],
            "test",
        );
        assert!(result.is_err(), "Should reject literal on ObjectProperty");
        if let Err(OwlError::ValidationError(msg)) = result {
            assert!(msg.contains("ObjectProperty"), "Error should mention ObjectProperty");
        } else {
            panic!("Expected ValidationError");
        }

        let result = task.add_property(
            &mut conn, "foundation:title",
            vec![Object::Iri("foundation:MyTask".to_string())],
            "test",
        );
        assert!(result.is_err(), "Should reject IRI on DatatypeProperty");
        if let Err(OwlError::ValidationError(msg)) = result {
            assert!(msg.contains("DatatypeProperty"), "Error should mention DatatypeProperty");
        } else {
            panic!("Expected ValidationError");
        }
    }
}
