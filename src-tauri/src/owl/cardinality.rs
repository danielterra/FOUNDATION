// ============================================================================
// OWL Cardinality - Cardinality Constraint Validation
// ============================================================================
// Validates owl:minCardinality, owl:maxCardinality, and owl:cardinality
// restrictions on properties for OWL classes
// ============================================================================

use crate::eavto::Connection;
use crate::eavto::query;
use crate::owl::{Result, OwlError, vocabulary::owl};

/// Cardinality restriction for a property on a class
#[derive(Debug, Clone)]
pub struct CardinalityRestriction {
    pub property_iri: String,
    pub min: Option<u32>,
    pub max: Option<u32>,
    pub exact: Option<u32>,
}

impl CardinalityRestriction {
    /// Returns true if this restriction requires at least one value (minCardinality >= 1 or exact >= 1)
    pub fn is_required(&self) -> bool {
        self.exact.map(|e| e >= 1).unwrap_or(false) || self.min.map(|m| m >= 1).unwrap_or(false)
    }

    /// Check if a count violates this cardinality restriction
    pub fn is_violated(&self, count: usize) -> bool {
        let count = count as u32;

        // Check exact cardinality first (overrides min/max)
        if let Some(exact) = self.exact {
            return count != exact;
        }

        // Check min
        if let Some(min) = self.min {
            if count < min {
                return true;
            }
        }

        // Check max
        if let Some(max) = self.max {
            if count > max {
                return true;
            }
        }

        false
    }

    /// Get a human-readable description of the violation
    pub fn violation_message(&self, count: usize, property_label: Option<&str>) -> String {
        let count = count as u32;
        let property_name = property_label.unwrap_or(&self.property_iri);

        if let Some(exact) = self.exact {
            return format!(
                "Property '{}' requires exactly {} value(s), but has {}",
                property_name, exact, count
            );
        }

        if let Some(min) = self.min {
            if count < min {
                return format!(
                    "Property '{}' requires at least {} value(s), but has {}",
                    property_name, min, count
                );
            }
        }

        if let Some(max) = self.max {
            if count > max {
                return format!(
                    "Property '{}' allows at most {} value(s), but has {}",
                    property_name, max, count
                );
            }
        }

        format!("Property '{}' cardinality constraint violated", property_name)
    }
}

/// Get all cardinality restrictions for a class
///
/// This queries for owl:Restriction nodes that are part of the class definition:
/// ```turtle
/// foundation:Person a owl:Class ;
///     rdfs:subClassOf [
///         a owl:Restriction ;
///         owl:onProperty foundation:name ;
///         owl:cardinality "1"^^xsd:nonNegativeInteger
///     ] .
/// ```
pub fn get_class_cardinality_restrictions(
    conn: &Connection,
    class_iri: &str,
) -> Result<Vec<CardinalityRestriction>> {
    let mut restrictions = Vec::new();

    // Query for restriction nodes that this class is a subclass of
    // These are blank nodes with rdf:type owl:Restriction
    let subclass_result =
        query::get_by_entity_predicate(conn, class_iri, "rdfs:subClassOf")?;

    for triple in &subclass_result.triples {
        // Check if the object is a blank node (restriction)
        // as_iri() returns both IRI and Blank nodes
        if let Some(restriction_node) = triple.object.as_iri() {
            // Only process blank nodes (they start with "_:")
            if !restriction_node.starts_with("_:") {
                continue;
            }

            // Check if this blank node is an owl:Restriction
            let type_result =
                query::get_by_entity_predicate(conn, restriction_node, "rdf:type")?;
            let is_restriction = type_result.triples.iter().any(|t| {
                if let Some(type_iri) = t.object.as_iri() {
                    type_iri == owl::RESTRICTION
                } else {
                    false
                }
            });

            if !is_restriction {
                continue;
            }

            // Get the property this restriction applies to
            let prop_result =
                query::get_by_entity_predicate(conn, restriction_node, owl::ON_PROPERTY)?;
            let property_iri = match prop_result.triples.first().and_then(|t| t.object.as_iri()) {
                Some(iri) => iri.to_string(),
                None => continue,
            };

            // Get cardinality constraints
            let mut min = None;
            let mut max = None;
            let mut exact = None;

            // Check for exact cardinality
            let card_result =
                query::get_by_entity_predicate(conn, restriction_node, owl::CARDINALITY)?;
            if let Some(triple) = card_result.triples.first() {
                // Extract integer value
                match &triple.object {
                    crate::eavto::Object::Integer(value) => exact = Some(*value as u32),
                    _ => {}
                }
            }

            // Check for min cardinality
            let min_result =
                query::get_by_entity_predicate(conn, restriction_node, owl::MIN_CARDINALITY)?;
            if let Some(triple) = min_result.triples.first() {
                match &triple.object {
                    crate::eavto::Object::Integer(value) => min = Some(*value as u32),
                    _ => {}
                }
            }

            // Check for max cardinality
            let max_result =
                query::get_by_entity_predicate(conn, restriction_node, owl::MAX_CARDINALITY)?;
            if let Some(triple) = max_result.triples.first() {
                match &triple.object {
                    crate::eavto::Object::Integer(value) => max = Some(*value as u32),
                    _ => {}
                }
            }

            restrictions.push(CardinalityRestriction {
                property_iri,
                min,
                max,
                exact,
            });
        }
    }

    Ok(restrictions)
}

/// Validate cardinality constraints for an individual
///
/// Returns Ok(()) if all constraints are satisfied, or an error describing the violation
pub fn validate_property_cardinality(
    conn: &Connection,
    individual_iri: &str,
    property_iri: &str,
    new_value_count: usize, // How many values will exist after this operation
) -> Result<()> {
    // Get the individual's type(s)
    let types_result = query::get_by_entity_predicate(conn, individual_iri, "rdf:type")?;

    if types_result.triples.is_empty() {
        // No type means no restrictions
        return Ok(());
    }

    // Check cardinality for each type
    for type_triple in &types_result.triples {
        if let Some(class_iri) = type_triple.object.as_iri() {
            // Skip non-foundation classes
            if !class_iri.starts_with("foundation:") {
                continue;
            }

            // Get restrictions for this class
            let restrictions = get_class_cardinality_restrictions(conn, class_iri)?;

            // Find restriction for this property
            for restriction in restrictions {
                if restriction.property_iri == property_iri {
                    if restriction.is_violated(new_value_count) {
                        // Get property label for better error message
                        let prop_label_result = query::get_by_entity_predicate(
                            conn,
                            property_iri,
                            "rdfs:label",
                        )?;
                        let prop_label = prop_label_result.triples.first()
                            .and_then(|t| t.object.as_literal());

                        return Err(OwlError::CardinalityViolation(
                            restriction.violation_message(
                                new_value_count,
                                prop_label.as_deref(),
                            )
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Set the required fields for a class by creating OWL minCardinality restrictions.
///
/// Retracts all existing owl:Restriction blank nodes linked via rdfs:subClassOf,
/// then asserts new ones for each property in `required_properties`.
/// Pass an empty slice to remove all required field restrictions.
pub fn set_class_required_fields(
    conn: &mut Connection,
    class_iri: &str,
    required_properties: &[&str],
    origin: &str,
) -> Result<()> {
    use crate::eavto::{store, query, Triple, Object};
    use sha2::{Sha256, Digest};

    // 1. Find and retract existing OWL restriction blank nodes
    let subclass_result = query::get_by_entity_predicate(conn, class_iri, "rdfs:subClassOf")?;
    for triple in &subclass_result.triples {
        if let Some(node) = triple.object.as_iri() {
            if !node.starts_with("_:") {
                continue;
            }
            let type_result = query::get_by_entity_predicate(conn, node, "rdf:type")?;
            let is_restriction = type_result.triples.iter()
                .any(|t| t.object.as_iri().map(|iri| iri == owl::RESTRICTION).unwrap_or(false));
            if !is_restriction {
                continue;
            }

            let mut to_retract: Vec<Triple> = Vec::new();
            to_retract.push(Triple::new(class_iri, "rdfs:subClassOf", triple.object.clone()));
            for rt in &type_result.triples {
                to_retract.push(Triple::new(node, "rdf:type", rt.object.clone()));
            }
            for predicate in [owl::ON_PROPERTY, owl::MIN_CARDINALITY, owl::CARDINALITY, owl::MAX_CARDINALITY] {
                let result = query::get_by_entity_predicate(conn, node, predicate)?;
                for rt in result.triples {
                    to_retract.push(Triple::new(node, predicate, rt.object));
                }
            }
            store::retract_triples(conn, &to_retract, origin)?;
        }
    }

    // 2. Assert new minCardinality = 1 restrictions for each required property.
    // The blank node internal triples use assert_triples (safe: fresh blank node subjects).
    // The (class_iri, rdfs:subClassOf, blank_id) links use append_triples to avoid
    // retracting the real parent class rdfs:subClassOf link.
    let mut blank_ids: Vec<String> = Vec::new();

    for prop_iri in required_properties {
        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}:restriction", class_iri, prop_iri).as_bytes());
        let hash = hasher.finalize();
        let blank_id = format!("_:restriction_{}", hash[..8].iter().map(|b| format!("{:02x}", b)).collect::<String>());
        blank_ids.push(blank_id);
    }

    let mut blank_internal_triples: Vec<Triple> = Vec::new();
    let mut subclass_link_triples: Vec<Triple> = Vec::new();

    for (prop_iri, blank_id) in required_properties.iter().zip(blank_ids.iter()) {
        subclass_link_triples.push(Triple::new(class_iri, "rdfs:subClassOf", Object::Blank(blank_id.clone())));
        blank_internal_triples.push(Triple::new(blank_id.as_str(), "rdf:type", Object::Iri(owl::RESTRICTION.to_string())));
        blank_internal_triples.push(Triple::new(blank_id.as_str(), owl::ON_PROPERTY, Object::Iri(prop_iri.to_string())));
        blank_internal_triples.push(Triple::new(blank_id.as_str(), owl::MIN_CARDINALITY, Object::Integer(1)));
    }

    if !blank_internal_triples.is_empty() {
        store::assert_triples(conn, &blank_internal_triples, origin)?;
    }
    if !subclass_link_triples.is_empty() {
        store::append_triples(conn, &subclass_link_triples, origin)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_cardinality() {
        let restriction = CardinalityRestriction {
            property_iri: "foundation:name".to_string(),
            min: None,
            max: None,
            exact: Some(1),
        };

        assert!(!restriction.is_violated(1)); // OK
        assert!(restriction.is_violated(0));  // Too few
        assert!(restriction.is_violated(2));  // Too many
    }

    #[test]
    fn test_min_cardinality() {
        let restriction = CardinalityRestriction {
            property_iri: "foundation:email".to_string(),
            min: Some(1),
            max: None,
            exact: None,
        };

        assert!(restriction.is_violated(0));  // Too few
        assert!(!restriction.is_violated(1)); // OK
        assert!(!restriction.is_violated(5)); // OK (no max)
    }

    #[test]
    fn test_max_cardinality() {
        let restriction = CardinalityRestriction {
            property_iri: "foundation:phone".to_string(),
            min: None,
            max: Some(3),
            exact: None,
        };

        assert!(!restriction.is_violated(0)); // OK (no min)
        assert!(!restriction.is_violated(3)); // OK
        assert!(restriction.is_violated(4));  // Too many
    }

    #[test]
    fn test_min_max_cardinality() {
        let restriction = CardinalityRestriction {
            property_iri: "foundation:hasPhoneNumber".to_string(),
            min: Some(0),
            max: Some(2),
            exact: None,
        };

        assert!(!restriction.is_violated(0)); // OK
        assert!(!restriction.is_violated(1)); // OK
        assert!(!restriction.is_violated(2)); // OK
        assert!(restriction.is_violated(3));  // Too many
    }

    #[test]
    fn test_set_class_required_fields() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let mut conn = setup_test_db();

        // Set up class
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        ], "test").unwrap();

        // Mark two properties as required
        set_class_required_fields(
            &mut conn,
            "foundation:TestClass",
            &["foundation:name", "foundation:email"],
            "test",
        ).unwrap();

        // Verify restrictions exist
        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
        assert_eq!(restrictions.len(), 2);
        let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();
        assert!(props.contains(&"foundation:name"));
        assert!(props.contains(&"foundation:email"));
        for r in &restrictions {
            assert_eq!(r.min, Some(1));
        }

        // Replace with just one required field
        set_class_required_fields(
            &mut conn,
            "foundation:TestClass",
            &["foundation:name"],
            "test",
        ).unwrap();

        let restrictions2 = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
        assert_eq!(restrictions2.len(), 1);
        assert_eq!(restrictions2[0].property_iri, "foundation:name");

        // Clear all required fields
        set_class_required_fields(&mut conn, "foundation:TestClass", &[], "test").unwrap();
        let restrictions3 = get_class_cardinality_restrictions(&conn, "foundation:TestClass").unwrap();
        assert_eq!(restrictions3.len(), 0);
    }

    #[test]
    fn test_validate_property_cardinality() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Person", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Person", "rdfs:subClassOf", Object::Blank("_:r1".to_string())),
            Triple::new("_:r1", "rdf:type", Object::Iri("owl:Restriction".to_string())),
            Triple::new("_:r1", "owl:onProperty", Object::Iri("foundation:name".to_string())),
            Triple::new("_:r1", "owl:minCardinality", Object::Integer(1)),
        ], "test").unwrap();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:john", "rdf:type", Object::Iri("foundation:Person".to_string())),
        ], "test").unwrap();

        validate_property_cardinality(&conn, "foundation:john", "foundation:name", 1).unwrap();

        let result = validate_property_cardinality(&conn, "foundation:john", "foundation:name", 0);
        assert!(result.is_err(), "Should fail with 0 values for a required field");
    }
}
