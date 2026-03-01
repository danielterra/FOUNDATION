// ============================================================================
// OWL Cardinality - Cardinality Constraint Validation
// ============================================================================
// Validates owl:minCardinality, owl:maxCardinality, and owl:cardinality
// restrictions on properties for OWL classes
// ============================================================================

use rusqlite::Connection;
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
    let subclass_result = query::get_by_entity_predicate(conn, class_iri, "rdfs:subClassOf")?;

    for triple in &subclass_result.triples {
        // Check if the object is a blank node (restriction)
        // as_iri() returns both IRI and Blank nodes
        if let Some(restriction_node) = triple.object.as_iri() {
            // Only process blank nodes (they start with "_:")
            if !restriction_node.starts_with("_:") {
                continue;
            }

            // Check if this blank node is an owl:Restriction
            let type_result = query::get_by_entity_predicate(conn, restriction_node, "rdf:type")?;
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
            let prop_result = query::get_by_entity_predicate(conn, restriction_node, owl::ON_PROPERTY)?;
            let property_iri = match prop_result.triples.first().and_then(|t| t.object.as_iri()) {
                Some(iri) => iri.to_string(),
                None => continue,
            };

            // Get cardinality constraints
            let mut min = None;
            let mut max = None;
            let mut exact = None;

            // Check for exact cardinality
            let card_result = query::get_by_entity_predicate(conn, restriction_node, owl::CARDINALITY)?;
            if let Some(triple) = card_result.triples.first() {
                // Extract integer value
                match &triple.object {
                    crate::eavto::Object::Integer(value) => exact = Some(*value as u32),
                    _ => {}
                }
            }

            // Check for min cardinality
            let min_result = query::get_by_entity_predicate(conn, restriction_node, owl::MIN_CARDINALITY)?;
            if let Some(triple) = min_result.triples.first() {
                match &triple.object {
                    crate::eavto::Object::Integer(value) => min = Some(*value as u32),
                    _ => {}
                }
            }

            // Check for max cardinality
            let max_result = query::get_by_entity_predicate(conn, restriction_node, owl::MAX_CARDINALITY)?;
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
                        let prop_label_result = query::get_by_entity_predicate(conn, property_iri, "rdfs:label")?;
                        let prop_label = prop_label_result.triples.first()
                            .and_then(|t| t.object.as_literal());

                        return Err(OwlError::CardinalityViolation(
                            restriction.violation_message(new_value_count, prop_label.as_deref())
                        ));
                    }
                }
            }
        }
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
}
