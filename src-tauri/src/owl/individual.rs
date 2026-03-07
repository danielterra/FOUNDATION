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
    pub backlinks: Vec<crate::eavto::query::BacklinkRow>,
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

    pub fn get_from_retracted(conn: &Connection, iri: impl Into<String>) -> Result<Option<Self>> {
        let iri = iri.into();
        let retracted = query::get_retracted_by_entity(conn, &iri)?;
        if retracted.triples.is_empty() {
            return Ok(None);
        }

        let label = retracted.triples.iter()
            .find(|t| t.predicate == rdfs::LABEL)
            .and_then(|t| t.object.as_literal());

        let icon = retracted.triples.iter()
            .find(|t| t.predicate == "foundation:hasIcon")
            .and_then(|t| t.object.as_iri())
            .and_then(|iri| crate::owl::icon_iri_to_display(conn, iri))
            .or_else(|| {
                retracted.triples.iter()
                    .find(|t| t.predicate == "foundation:icon")
                    .and_then(|t| t.object.as_literal())
            });

        let comment = retracted.triples.iter()
            .find(|t| t.predicate == rdfs::COMMENT)
            .and_then(|t| t.object.as_literal());

        let prop_triples: Vec<_> = retracted.triples.into_iter()
            .filter(|t| {
                t.predicate != rdfs::LABEL
                    && t.predicate != rdfs::COMMENT
                    && t.predicate != "foundation:icon"
                    && t.predicate != "foundation:hasIcon"
            })
            .collect();

        let property_tx: Vec<i64> = prop_triples.iter().map(|t| t.tx).collect();
        let properties: Vec<(String, Object)> = prop_triples.into_iter()
            .map(|t| (t.predicate, t.object))
            .collect();

        Ok(Some(Self {
            iri,
            label,
            icon,
            comment,
            types: Vec::new(),
            properties,
            property_tx,
            backlinks: Vec::new(),
        }))
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
            .find(|t| t.predicate == "foundation:hasIcon")
            .and_then(|t| t.object.as_iri())
            .and_then(|iri| crate::owl::icon_iri_to_display(conn, iri))
            .or_else(|| {
                all_triples.triples.iter()
                    .find(|t| t.predicate == "foundation:icon")
                    .and_then(|t| t.object.as_literal())
            });

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
                    && t.predicate != "foundation:hasIcon"
            })
            .collect();

        let property_tx: Vec<i64> = prop_triples.iter().map(|t| t.tx).collect();
        let properties: Vec<(String, Object)> = prop_triples.into_iter()
            .map(|t| (t.predicate, t.object))
            .collect();

        const BACKLINK_LIMIT_PER_GROUP: usize = 15;
        let backlinks = query::get_backlinks_grouped_limited(conn, &iri, BACKLINK_LIMIT_PER_GROUP)?;

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
        crate::owl::validate_icon(conn, icon)?;

        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(class_iri.to_string()));
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

        let is_meta_property = property.starts_with("rdfs:")
            || property == "foundation:icon"
            || property == "foundation:hasIcon";

        if !is_meta_property {
            if let Ok(true) = is_formula_property(conn, property) {
                return Err(OwlError::ValidationError(format!(
                    "Property '{}' is calculated via a formula and cannot be set directly",
                    property
                )));
            }
        }

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

        Self::validate_value_type(conn, property, &values)?;
        for value in &values {
            Self::validate_iri_exists(conn, property, value)?;
            Self::validate_one_of_constraint(conn, property, value)?;
            Self::validate_literal_datatype(property, value)?;
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

    /// Validate that a literal value conforms to its declared xsd datatype
    fn validate_literal_datatype(property: &str, value: &Object) -> Result<()> {
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
    fn validate_value_type(conn: &Connection, property: &str, values: &[Object]) -> Result<()> {
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
    fn validate_iri_exists(conn: &Connection, property: &str, value: &Object) -> Result<()> {
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


    /// Retract all triples for the given entity IRI, including references to it from other entities
    pub fn retract(conn: &mut Connection, iri: &str, origin: &str) -> Result<()> {
        let mut triples = query::get_by_entity(conn, iri)?.triples;
        triples.extend(query::get_by_object_iri(conn, iri)?.triples);
        if !triples.is_empty() {
            store::retract_triples(conn, &triples, origin)?;
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

    pub fn find_by_class_with_date_range(
        conn: &Connection,
        class_iri: &str,
        from_millis: Option<i64>,
        to_millis: Option<i64>,
        include_retracted: bool,
    ) -> Result<Vec<String>> {
        query::find_entities_by_class_with_date_range(conn, class_iri, from_millis, to_millis, include_retracted)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }

    pub fn find_by_class_and_properties_with_options(
        conn: &Connection,
        class_iri: &str,
        properties: &[(&str, &str, &str)],
        include_retracted: bool,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<String>, usize)> {
        let descendant_iris = Class::get_descendant_iris(conn, class_iri)?;
        let class_iris: Vec<&str> = descendant_iris.iter().map(|s| s.as_str()).collect();
        query::find_by_class_iris_and_properties_with_options(
            conn,
            &class_iris,
            properties,
            include_retracted,
            limit,
            offset,
        ).map_err(|e| OwlError::DatabaseError(e.to_string()))
    }

    /// Returns IRIs of messages in `conversation_iri` ordered by sentAt descending (newest first).
    /// Pass `limit = usize::MAX` for no limit.
    pub fn find_messages_by_conversation(
        conn: &Connection,
        conversation_iri: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<String>> {
        query::find_message_iris_by_conversation(conn, conversation_iri, limit, offset)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }

    /// Returns the number of current (non-retracted) values for a property on an individual.
    pub fn get_property_count(conn: &Connection, iri: &str, property_iri: &str) -> Result<usize> {
        let result = query::get_by_entity_predicate(conn, iri, property_iri)?;
        Ok(result.triples.len())
    }

    /// Retracts all current values of `property_iri` on individual `iri`.
    pub fn clear_property(
        conn: &mut Connection,
        iri: &str,
        property_iri: &str,
        origin: &str,
    ) -> Result<()> {
        let result = query::get_by_entity_predicate(conn, iri, property_iri)?;
        if !result.triples.is_empty() {
            store::retract_triples(conn, &result.triples, origin)?;
        }
        Ok(())
    }

    /// Returns retracted triples for an individual, excluding metadata predicates.
    pub fn get_retracted_properties(conn: &Connection, iri: &str) -> Result<Vec<Triple>> {
        query::get_retracted_by_entity(conn, iri)
            .map(|r| r.triples.into_iter().filter(|t| {
                t.predicate != "rdfs:label"
                    && t.predicate != "rdfs:comment"
                    && t.predicate != "foundation:icon"
                    && t.predicate != "foundation:hasIcon"
            }).collect())
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }

    /// Batch-loads active triples for a list of individual IRIs in a single query.
    pub fn batch_load_triples(
        conn: &Connection,
        iris: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Triple>>> {
        query::batch_load_triples_for_subjects(conn, iris)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }

    /// Batch-loads retracted triples for a list of individual IRIs in a single query.
    pub fn batch_load_retracted_triples(
        conn: &Connection,
        iris: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Triple>>> {
        query::batch_load_retracted_triples_for_subjects(conn, iris)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }
}

fn is_formula_property(conn: &Connection, property_iri: &str) -> Result<bool> {
    let result = query::get_by_entity_predicate(conn, property_iri, "foundation:formula")?;
    Ok(!result.triples.is_empty())
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
            &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test",
        ).unwrap();

        // Create TaskPriority enumeration class
        let priority_class = Class::new("foundation:TaskPriority");
        priority_class.assert(
            &mut conn,
            ClassType::OwlClass,
            "Task Priority",
            "https://example.com/priority.svg",
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
            &["foundation:Task"],
            Some("foundation:TaskPriority"),
            None,
            "test",
        ).unwrap();

        // Create task individual
        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").unwrap();

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
            &mut conn, ClassType::OwlClass, "Task", "https://example.com/task.svg", None, "test",
        ).unwrap();

        // Create TaskPriority enumeration class
        let priority_class = Class::new("foundation:TaskPriority");
        priority_class.assert(
            &mut conn,
            ClassType::OwlClass,
            "Task Priority",
            "https://example.com/priority.svg",
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
            &["foundation:Task"],
            Some("foundation:TaskPriority"),
            None,
            "test",
        ).unwrap();

        // Create task individual
        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").unwrap();

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

    #[test]
    fn test_iri_existence_validation() {
        let mut conn = setup_test_db();

        // Create a class and a property
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

        // Referencing a non-existent IRI should fail
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

        // Create the user and retry — should succeed
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

        // ObjectProperty
        let obj_prop = Property::new("foundation:relatedTo");
        obj_prop.assert(
            &mut conn, PropertyType::ObjectProperty, "relatedTo",
            None, &["foundation:Task"], None, None, "test",
        ).unwrap();

        // DatatypeProperty
        let dt_prop = Property::new("foundation:title");
        dt_prop.assert(
            &mut conn, PropertyType::DatatypeProperty, "title",
            None, &["foundation:Task"], Some("xsd:string"), None, "test",
        ).unwrap();

        let task = Individual::new("foundation:MyTask");
        task.assert(&mut conn, "foundation:Task", "My Task", "https://example.com/task.svg", "test").unwrap();

        // Literal on ObjectProperty → should fail
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

        // IRI on DatatypeProperty → should fail
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

    #[test]
    fn test_find_by_class_and_properties_with_options_polymorphic() {

        let mut conn = setup_test_db();

        // Create parent class Animal and subclass Dog
        let animal_class = Class::new("foundation:Animal");
        animal_class.assert(
            &mut conn, ClassType::OwlClass, "Animal", "https://example.com/animal.svg", None, "test",
        ).unwrap();

        let dog_class = Class::new("foundation:Dog");
        dog_class.assert(
            &mut conn, ClassType::OwlClass, "Dog", "https://example.com/dog.svg",
            Some("foundation:Animal"), "test",
        ).unwrap();

        // Create a name property on Animal
        let name_prop = Property::new("foundation:animalName");
        name_prop.assert(
            &mut conn, PropertyType::DatatypeProperty, "animalName",
            None, &["foundation:Animal"], Some("xsd:string"), None, "test",
        ).unwrap();

        // Create an instance of Dog (subclass of Animal) with a name
        store::assert_triples(&mut conn, &[
            Triple { subject: "foundation:Rex".to_string(), predicate: rdf::TYPE.to_string(),
                object: Object::Iri("foundation:Dog".to_string()),
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:Rex".to_string(), predicate: "foundation:animalName".to_string(),
                object: Object::Literal { value: "Rex".to_string(),
                    datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        // Querying Animal should find the Dog instance via polymorphic expansion
        let (results, total) = Individual::find_by_class_and_properties_with_options(
            &conn,
            "foundation:Animal",
            &[("foundation:animalName", "Rex", "=")],
            false,
            100,
            0,
        ).unwrap();

        assert_eq!(total, 1, "Should find 1 result via polymorphic search");
        assert!(results.contains(&"foundation:Rex".to_string()), "Should include the Dog instance");
    }

    #[test]
    fn test_write_to_calculated_property_is_rejected() {
        let mut conn = setup_test_db();

        let c = Class::new("foundation:Rectangle");
        c.assert(&mut conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").unwrap();

        let width_prop = Property::new("foundation:hasWidth");
        width_prop.assert(&mut conn, PropertyType::DatatypeProperty, "has width", None,
            &["foundation:Rectangle"], Some("xsd:integer"), Some("unit:Meter"), "test").unwrap();

        let area_prop = Property::new("foundation:hasArea");
        area_prop.assert(&mut conn, PropertyType::DatatypeProperty, "has area", None,
            &["foundation:Rectangle"], Some("xsd:decimal"), Some("unit:SquareMeter"), "test").unwrap();

        let ind = Individual::new("foundation:MyRect");
        ind.assert(&mut conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").unwrap();

        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            [],
        ).unwrap();
        let tx_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params!["foundation:hasArea", "{{foundation:hasWidth}} * 2", tx_id],
        ).unwrap();

        let result = ind.add_property(
            &mut conn,
            "foundation:hasArea",
            vec![Object::Literal { value: "100".to_string(), datatype: Some("xsd:decimal".to_string()), language: None }],
            "test",
        );

        assert!(result.is_err(), "Should reject write to calculated property");
        if let Err(OwlError::ValidationError(msg)) = result {
            assert!(msg.contains("calculated via a formula"));
        } else {
            panic!("Expected ValidationError");
        }
    }

    #[test]
    fn test_write_to_non_calculated_property_succeeds() {
        let mut conn = setup_test_db();

        let c = Class::new("foundation:Rectangle");
        c.assert(&mut conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").unwrap();

        let width_prop = Property::new("foundation:hasWidth");
        width_prop.assert(&mut conn, PropertyType::DatatypeProperty, "has width", None,
            &["foundation:Rectangle"], Some("xsd:integer"), Some("unit:Meter"), "test").unwrap();

        let ind = Individual::new("foundation:MyRect");
        ind.assert(&mut conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").unwrap();

        let result = ind.add_property(
            &mut conn,
            "foundation:hasWidth",
            vec![Object::Literal { value: "5".to_string(), datatype: Some("xsd:integer".to_string()), language: None }],
            "test",
        );

        assert!(result.is_ok(), "Should accept write to non-calculated property");
    }

    #[test]
    fn test_meta_property_bypasses_formula_protection() {
        let mut conn = setup_test_db();

        let c = Class::new("foundation:Rectangle");
        c.assert(&mut conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").unwrap();

        let ind = Individual::new("foundation:MyRect");
        ind.assert(&mut conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").unwrap();

        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            [],
        ).unwrap();
        let tx_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params!["rdfs:label", "some formula", tx_id],
        ).unwrap();

        let result = ind.add_property(
            &mut conn,
            "rdfs:label",
            vec![Object::Literal { value: "Updated Label".to_string(), datatype: Some("xsd:string".to_string()), language: None }],
            "test",
        );

        assert!(result.is_ok(), "Meta properties should bypass formula protection");
    }

    #[test]
    fn test_calculated_property_error_message_is_descriptive() {
        let mut conn = setup_test_db();

        let c = Class::new("foundation:Rectangle");
        c.assert(&mut conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").unwrap();

        let area_prop = Property::new("foundation:hasArea");
        area_prop.assert(&mut conn, PropertyType::DatatypeProperty, "has area", None,
            &["foundation:Rectangle"], Some("xsd:decimal"), Some("unit:SquareMeter"), "test").unwrap();

        let ind = Individual::new("foundation:MyRect");
        ind.assert(&mut conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").unwrap();

        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            [],
        ).unwrap();
        let tx_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params!["foundation:hasArea", "{{foundation:hasWidth}} * 2", tx_id],
        ).unwrap();

        let result = ind.add_property(
            &mut conn,
            "foundation:hasArea",
            vec![Object::Literal { value: "100".to_string(), datatype: Some("xsd:decimal".to_string()), language: None }],
            "test",
        );

        if let Err(OwlError::ValidationError(msg)) = result {
            assert!(msg.contains("foundation:hasArea"), "Error should contain the property IRI");
            assert!(msg.contains("calculated via a formula"), "Error should mention formula");
            assert!(msg.contains("cannot be set directly"), "Error should say cannot be set directly");
        } else {
            panic!("Expected ValidationError");
        }
    }

    #[test]
    fn test_find_by_class_and_properties_with_options_parent_has_no_direct_instances() {

        let mut conn = setup_test_db();

        // Create parent class Event with two subclasses
        let event_class = Class::new("foundation:Event");
        event_class.assert(
            &mut conn, ClassType::OwlClass, "Event", "https://example.com/event.svg", None, "test",
        ).unwrap();

        let vacation_class = Class::new("foundation:Vacation");
        vacation_class.assert(
            &mut conn, ClassType::OwlClass, "Vacation", "https://example.com/vacation.svg",
            Some("foundation:Event"), "test",
        ).unwrap();

        let social_class = Class::new("foundation:SocialEvent");
        social_class.assert(
            &mut conn, ClassType::OwlClass, "Social Event", "https://example.com/social.svg",
            Some("foundation:Event"), "test",
        ).unwrap();

        // No direct Event instances — only subclass instances
        store::assert_triples(&mut conn, &[
            Triple { subject: "foundation:HolidayVacation".to_string(), predicate: rdf::TYPE.to_string(),
                object: Object::Iri("foundation:Vacation".to_string()),
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:HolidayVacation".to_string(), predicate: "foundation:title".to_string(),
                object: Object::Literal { value: "Holiday".to_string(),
                    datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:BirthdayParty".to_string(), predicate: rdf::TYPE.to_string(),
                object: Object::Iri("foundation:SocialEvent".to_string()),
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:BirthdayParty".to_string(), predicate: "foundation:title".to_string(),
                object: Object::Literal { value: "Birthday".to_string(),
                    datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        // Querying Event with "Holiday" should find the Vacation instance
        let (results, total) = Individual::find_by_class_and_properties_with_options(
            &conn,
            "foundation:Event",
            &[("foundation:title", "Holiday", "=")],
            false,
            100,
            0,
        ).unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"foundation:HolidayVacation".to_string()));
        assert!(!results.contains(&"foundation:BirthdayParty".to_string()));
    }

    // ── get_from_retracted ──────────────────────────────────────────────────

    #[test]
    fn test_get_from_retracted_returns_none_when_nothing_retracted() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", rdf::TYPE, Object::Iri("foundation:Person".to_string())),
        ], "test").unwrap();

        let result = Individual::get_from_retracted(&conn, "foundation:Alice").unwrap();
        assert!(result.is_none(), "No retracted triples → should return None");
    }

    #[test]
    fn test_get_from_retracted_returns_none_for_unknown_iri() {
        let conn = setup_test_db();

        let result = Individual::get_from_retracted(&conn, "foundation:Unknown").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_from_retracted_finds_deleted_individual() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", rdf::TYPE, Object::Iri("foundation:Person".to_string())),
            Triple::new("foundation:Alice", rdfs::LABEL, Object::Literal {
                value: "Alice".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:Alice", "foundation:age", Object::Integer(30)),
        ], "test").unwrap();

        Individual::retract(&mut conn, "foundation:Alice", "test").unwrap();

        let result = Individual::get_from_retracted(&conn, "foundation:Alice").unwrap();
        assert!(result.is_some(), "Should find retracted individual");

        let ind = result.unwrap();
        assert_eq!(ind.iri, "foundation:Alice");
        assert_eq!(ind.label, Some("Alice".to_string()));
        assert!(ind.properties.iter().any(|(p, _)| p == "foundation:age"));
    }

    #[test]
    fn test_get_from_retracted_extracts_label_and_comment() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Bob", rdf::TYPE, Object::Iri("foundation:Person".to_string())),
            Triple::new("foundation:Bob", rdfs::LABEL, Object::Literal {
                value: "Bob Smith".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:Bob", rdfs::COMMENT, Object::Literal {
                value: "A test person".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        Individual::retract(&mut conn, "foundation:Bob", "test").unwrap();

        let ind = Individual::get_from_retracted(&conn, "foundation:Bob").unwrap().unwrap();
        assert_eq!(ind.label, Some("Bob Smith".to_string()));
        assert_eq!(ind.comment, Some("A test person".to_string()));
    }

    #[test]
    fn test_get_from_retracted_excludes_label_and_comment_from_properties() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Bob", rdf::TYPE, Object::Iri("foundation:Person".to_string())),
            Triple::new("foundation:Bob", rdfs::LABEL, Object::Literal {
                value: "Bob".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:Bob", rdfs::COMMENT, Object::Literal {
                value: "A comment".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:Bob", "foundation:score", Object::Integer(42)),
        ], "test").unwrap();

        Individual::retract(&mut conn, "foundation:Bob", "test").unwrap();

        let ind = Individual::get_from_retracted(&conn, "foundation:Bob").unwrap().unwrap();
        assert!(!ind.properties.iter().any(|(p, _)| p == rdfs::LABEL));
        assert!(!ind.properties.iter().any(|(p, _)| p == rdfs::COMMENT));
        assert!(ind.properties.iter().any(|(p, _)| p == "foundation:score"));
    }

    // ── serializable_properties ─────────────────────────────────────────────

    #[test]
    fn test_serializable_properties_integer() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None,
            icon: None,
            comment: None,
            types: vec![],
            properties: vec![("foundation:age".to_string(), Object::Integer(30))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props.len(), 1);
        assert_eq!(props[0]["property"], "foundation:age");
        assert_eq!(props[0]["value"], 30);
    }

    #[test]
    fn test_serializable_properties_number() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:score".to_string(), Object::Number(9.5))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props[0]["value"], 9.5);
    }

    #[test]
    fn test_serializable_properties_boolean() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:active".to_string(), Object::Boolean(true))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props[0]["value"], true);
    }

    #[test]
    fn test_serializable_properties_string_literal() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:name".to_string(), Object::Literal {
                value: "Alice".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            })],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props[0]["value"], "Alice");
    }

    #[test]
    fn test_serializable_properties_decimal_literal_parsed_as_number() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:ratio".to_string(), Object::Literal {
                value: "3.14".to_string(),
                datatype: Some("xsd:decimal".to_string()),
                language: None,
            })],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props[0]["value"], 3.14);
    }

    #[test]
    fn test_serializable_properties_integer_literal_parsed_as_number() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:count".to_string(), Object::Literal {
                value: "99".to_string(),
                datatype: Some("xsd:integer".to_string()),
                language: None,
            })],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props[0]["value"], 99);
    }

    #[test]
    fn test_serializable_properties_iri_value() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:knows".to_string(), Object::Iri("foundation:Bob".to_string()))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props[0]["value"], "foundation:Bob");
    }

    #[test]
    fn test_serializable_properties_includes_unit_when_property_has_one() {
        let mut conn = setup_test_db();

        Property::new("foundation:height").assert(
            &mut conn,
            PropertyType::DatatypeProperty,
            "height",
            None,
            &[],
            Some("xsd:decimal"),
            Some("unit:Meter"),
            "test",
        ).unwrap();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:height".to_string(), Object::Number(1.75))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props[0]["unit"], "unit:Meter");
    }

    #[test]
    fn test_serializable_properties_no_unit_key_when_property_has_none() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:nickname".to_string(), Object::Literal {
                value: "Ally".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            })],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert!(props[0].get("unit").is_none(), "No unit key when property has no unit");
    }

    #[test]
    fn test_serializable_properties_multiple() {
        let conn = setup_test_db();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![
                ("foundation:age".to_string(), Object::Integer(30)),
                ("foundation:name".to_string(), Object::Literal {
                    value: "Alice".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }),
                ("foundation:active".to_string(), Object::Boolean(false)),
            ],
            property_tx: vec![0, 0, 0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn);
        assert_eq!(props.len(), 3);
    }

    // ── remove_property_value ───────────────────────────────────────────────

    #[test]
    fn test_remove_property_value_iri_happy_path() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", rdf::TYPE, Object::Iri("foundation:Person".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        ], "test").unwrap();

        let result = Individual::remove_property_value(
            &mut conn,
            "foundation:Alice",
            "foundation:knows",
            "foundation:Bob",
            "test",
        ).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), Object::Iri("foundation:Bob".to_string()));

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert!(after.triples.is_empty(), "Triple should have been retracted");
    }

    #[test]
    fn test_remove_property_value_integer() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:age", Object::Integer(30)),
        ], "test").unwrap();

        let result = Individual::remove_property_value(
            &mut conn,
            "foundation:Alice",
            "foundation:age",
            "30",
            "test",
        ).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), Object::Integer(30));

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:age").unwrap();
        assert!(after.triples.is_empty(), "Integer triple should have been retracted");
    }

    #[test]
    fn test_remove_property_value_string_literal() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:nickname", Object::Literal {
                value: "Ally".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let result = Individual::remove_property_value(
            &mut conn,
            "foundation:Alice",
            "foundation:nickname",
            "Ally",
            "test",
        ).unwrap();

        assert!(result.is_some());

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:nickname").unwrap();
        assert!(after.triples.is_empty(), "String literal triple should have been retracted");
    }

    #[test]
    fn test_remove_property_value_nonexistent_value_returns_none() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        ], "test").unwrap();

        let result = Individual::remove_property_value(
            &mut conn,
            "foundation:Alice",
            "foundation:knows",
            "foundation:Charlie",
            "test",
        ).unwrap();

        assert!(result.is_none(), "Should return None when value does not match");

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert_eq!(after.triples.len(), 1, "Existing triple should be untouched");
    }

    #[test]
    fn test_remove_property_value_no_triples_returns_none() {
        let mut conn = setup_test_db();

        let result = Individual::remove_property_value(
            &mut conn,
            "foundation:Alice",
            "foundation:knows",
            "foundation:Bob",
            "test",
        ).unwrap();

        assert!(result.is_none(), "Should return None when property has no triples");
    }

    #[test]
    fn test_remove_property_value_boolean() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:active", Object::Boolean(true)),
        ], "test").unwrap();

        let result = Individual::remove_property_value(
            &mut conn,
            "foundation:Alice",
            "foundation:active",
            "true",
            "test",
        ).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), Object::Boolean(true));
    }

    #[test]
    fn test_remove_property_value_number() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:score", Object::Number(9.5)),
        ], "test").unwrap();

        let result = Individual::remove_property_value(
            &mut conn,
            "foundation:Alice",
            "foundation:score",
            "9.5",
            "test",
        ).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), Object::Number(9.5));
    }

    #[test]
    fn test_remove_property_value_only_removes_matching_multivalue() {
        let mut conn = setup_test_db();

        store::append_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
        ], "test").unwrap();

        let result = Individual::remove_property_value(
            &mut conn,
            "foundation:Alice",
            "foundation:knows",
            "foundation:Bob",
            "test",
        ).unwrap();

        assert!(result.is_some());

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert_eq!(after.triples.len(), 1, "Only the matching value should be removed");
        assert_eq!(
            after.triples[0].object,
            Object::Iri("foundation:Carol".to_string()),
        );
    }

    // ── find_by_class_and_properties ─────────────────────────────────────────

    #[test]
    fn test_find_by_class_and_properties_empty_properties_returns_empty() {
        let conn = setup_test_db();
        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Task",
            &[],
        ).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_by_class_and_properties_single_filter() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TaskA", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskA", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
            Triple::new("foundation:TaskB", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskB", "foundation:hasStatus", Object::Iri("foundation:Done".to_string())),
        ], "test").unwrap();

        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Task",
            &[("foundation:hasStatus", "foundation:Active")],
        ).unwrap();

        assert_eq!(result, vec!["foundation:TaskA".to_string()]);
    }

    #[test]
    fn test_find_by_class_and_properties_multiple_filters() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TaskA", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskA", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
            Triple::new("foundation:TaskA", "foundation:priority", Object::Literal { value: "high".to_string(), datatype: None, language: None }),
            Triple::new("foundation:TaskB", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskB", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
            Triple::new("foundation:TaskB", "foundation:priority", Object::Literal { value: "low".to_string(), datatype: None, language: None }),
        ], "test").unwrap();

        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Task",
            &[
                ("foundation:hasStatus", "foundation:Active"),
                ("foundation:priority", "high"),
            ],
        ).unwrap();

        assert_eq!(result, vec!["foundation:TaskA".to_string()]);
    }

    #[test]
    fn test_find_by_class_and_properties_no_match_returns_empty() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TaskA", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskA", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
        ], "test").unwrap();

        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Task",
            &[("foundation:hasStatus", "foundation:Done")],
        ).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_find_by_class_and_properties_literal_value() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:ReleaseA", rdf::TYPE, Object::Iri("foundation:Release".to_string())),
            Triple::new("foundation:ReleaseA", "foundation:versionNumber", Object::Literal { value: "1.0.0".to_string(), datatype: None, language: None }),
            Triple::new("foundation:ReleaseB", rdf::TYPE, Object::Iri("foundation:Release".to_string())),
            Triple::new("foundation:ReleaseB", "foundation:versionNumber", Object::Literal { value: "2.0.0".to_string(), datatype: None, language: None }),
        ], "test").unwrap();

        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Release",
            &[("foundation:versionNumber", "1.0.0")],
        ).unwrap();

        assert_eq!(result, vec!["foundation:ReleaseA".to_string()]);
    }

    // ── find_messages_by_conversation ────────────────────────────────────────

    fn insert_message(conn: &mut Connection, iri: &str, conversation_iri: &str, sent_at_ms: i64) {
        store::assert_triples(conn, &[
            Triple::new(iri, rdf::TYPE, Object::Iri("foundation:AIConversationMessage".to_string())),
            Triple::new(iri, "foundation:partOfConversation", Object::Iri(conversation_iri.to_string())),
            Triple::new(iri, "foundation:sentAt", Object::DateTime(sent_at_ms)),
        ], "test").unwrap();
    }

    #[test]
    fn test_find_messages_by_conversation_empty_db() {
        let conn = setup_test_db();
        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            usize::MAX,
            0,
        ).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_messages_by_conversation_returns_messages_ordered_newest_first() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);

        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            usize::MAX,
            0,
        ).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "foundation:Msg2");
        assert_eq!(result[1], "foundation:Msg3");
        assert_eq!(result[2], "foundation:Msg1");
    }

    #[test]
    fn test_find_messages_by_conversation_respects_limit() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);

        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            2,
            0,
        ).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "foundation:Msg2");
        assert_eq!(result[1], "foundation:Msg3");
    }

    #[test]
    fn test_find_messages_by_conversation_respects_offset() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);

        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            usize::MAX,
            1,
        ).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "foundation:Msg3");
        assert_eq!(result[1], "foundation:Msg1");
    }

    #[test]
    fn test_find_messages_by_conversation_excludes_other_conversations() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvB", 3_000);

        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            usize::MAX,
            0,
        ).unwrap();

        assert_eq!(result, vec!["foundation:Msg1".to_string()]);
    }

    // ── get_property_count ───────────────────────────────────────────────────

    #[test]
    fn test_get_property_count_returns_zero_when_no_values() {
        let conn = setup_test_db();
        let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_get_property_count_returns_one_for_single_value() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        ], "test").unwrap();

        let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_get_property_count_returns_correct_count_for_multiple_values() {
        let mut conn = setup_test_db();
        store::append_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Dave".to_string())),
        ], "test").unwrap();

        let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_get_property_count_excludes_retracted_values() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        ], "test").unwrap();
        Individual::remove_property_value(&mut conn, "foundation:Alice", "foundation:knows", "foundation:Bob", "test").unwrap();

        let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert_eq!(count, 0);
    }

    // ── clear_property ───────────────────────────────────────────────────────

    #[test]
    fn test_clear_property_removes_all_values() {
        let mut conn = setup_test_db();
        store::append_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
        ], "test").unwrap();

        Individual::clear_property(&mut conn, "foundation:Alice", "foundation:knows", "test").unwrap();

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert!(after.triples.is_empty(), "All values should have been retracted");
    }

    #[test]
    fn test_clear_property_is_noop_when_no_values() {
        let mut conn = setup_test_db();
        let result = Individual::clear_property(&mut conn, "foundation:Alice", "foundation:knows", "test");
        assert!(result.is_ok(), "clear_property on empty property should not error");
    }

    #[test]
    fn test_clear_property_preserves_other_properties() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
            Triple::new("foundation:Alice", "foundation:name", Object::Literal {
                value: "Alice".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        Individual::clear_property(&mut conn, "foundation:Alice", "foundation:knows", "test").unwrap();

        let knows = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").unwrap();
        assert!(knows.triples.is_empty(), "foundation:knows should be cleared");

        let name = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:name").unwrap();
        assert_eq!(name.triples.len(), 1, "foundation:name must not be affected");
    }

    // ── get_retracted_properties ─────────────────────────────────────────────

    #[test]
    fn test_get_retracted_properties_empty_when_nothing_retracted() {
        let conn = setup_test_db();
        let result = Individual::get_retracted_properties(&conn, "foundation:Alice").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_retracted_properties_returns_retracted_triples() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:score", Object::Integer(42)),
        ], "test").unwrap();
        Individual::clear_property(&mut conn, "foundation:Alice", "foundation:score", "test").unwrap();

        let result = Individual::get_retracted_properties(&conn, "foundation:Alice").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].predicate, "foundation:score");
    }

    #[test]
    fn test_get_retracted_properties_filters_metadata_predicates() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "rdfs:label", Object::Literal {
                value: "Alice".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Alice", "rdfs:comment", Object::Literal {
                value: "A person".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Alice", "foundation:icon", Object::Literal {
                value: "person".to_string(), datatype: Some("xsd:string".to_string()), language: None,
            }),
            Triple::new("foundation:Alice", "foundation:score", Object::Integer(10)),
        ], "test").unwrap();

        Individual::clear_property(&mut conn, "foundation:Alice", "rdfs:label", "test").unwrap();
        Individual::clear_property(&mut conn, "foundation:Alice", "rdfs:comment", "test").unwrap();
        Individual::clear_property(&mut conn, "foundation:Alice", "foundation:icon", "test").unwrap();
        Individual::clear_property(&mut conn, "foundation:Alice", "foundation:score", "test").unwrap();

        let result = Individual::get_retracted_properties(&conn, "foundation:Alice").unwrap();
        let predicates: Vec<&str> = result.iter().map(|t| t.predicate.as_str()).collect();
        assert!(!predicates.contains(&"rdfs:label"), "rdfs:label must be filtered");
        assert!(!predicates.contains(&"rdfs:comment"), "rdfs:comment must be filtered");
        assert!(!predicates.contains(&"foundation:icon"), "foundation:icon must be filtered");
        assert!(predicates.contains(&"foundation:score"), "foundation:score must be included");
    }

    // ── batch_load_triples ───────────────────────────────────────────────────

    #[test]
    fn test_batch_load_triples_returns_empty_for_empty_input() {
        let conn = setup_test_db();
        let result = Individual::batch_load_triples(&conn, &[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_batch_load_triples_returns_triples_for_known_iris() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:score", Object::Integer(1)),
            Triple::new("foundation:Bob", "foundation:score", Object::Integer(2)),
        ], "test").unwrap();

        let iris = vec!["foundation:Alice".to_string(), "foundation:Bob".to_string()];
        let result = Individual::batch_load_triples(&conn, &iris).unwrap();

        assert!(result.contains_key("foundation:Alice"), "Alice should be in batch result");
        assert!(result.contains_key("foundation:Bob"), "Bob should be in batch result");
    }

    #[test]
    fn test_batch_load_triples_omits_unknown_iris() {
        let conn = setup_test_db();
        let iris = vec!["foundation:Ghost".to_string()];
        let result = Individual::batch_load_triples(&conn, &iris).unwrap();
        assert!(!result.contains_key("foundation:Ghost"), "Unknown IRI must not appear in result");
    }

    // ── batch_load_retracted_triples ─────────────────────────────────────────

    #[test]
    fn test_batch_load_retracted_triples_empty_for_active_individuals() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", "foundation:score", Object::Integer(1)),
        ], "test").unwrap();

        let iris = vec!["foundation:Alice".to_string()];
        let result = Individual::batch_load_retracted_triples(&conn, &iris).unwrap();
        assert!(!result.contains_key("foundation:Alice"), "Active individual must not appear in retracted batch");
    }

    #[test]
    fn test_batch_load_retracted_triples_returns_retracted_individuals() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Alice", rdf::TYPE, Object::Iri("foundation:Person".to_string())),
        ], "test").unwrap();
        Individual::retract(&mut conn, "foundation:Alice", "test").unwrap();

        let iris = vec!["foundation:Alice".to_string()];
        let result = Individual::batch_load_retracted_triples(&conn, &iris).unwrap();
        assert!(result.contains_key("foundation:Alice"), "Retracted individual should appear in retracted batch");
    }
}