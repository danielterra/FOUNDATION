
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

        if let Some(exact) = self.exact {
            return count != exact;
        }

        if let Some(min) = self.min {
            if count < min {
                return true;
            }
        }

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
                if max == 1 {
                    return format!(
                        "Property '{}' is single-valued. Use replace_property_values to overwrite the existing value.",
                        property_name
                    );
                }
                return format!(
                    "Property '{}' allows at most {} value(s), but has {}",
                    property_name, max, count
                );
            }
        }

        format!("Property '{}' cardinality constraint violated", property_name)
    }
}

/// A cardinality restriction to set on a class property.
/// A restriction with both `min` and `max` as `None` is a no-op and will be skipped.
pub struct PropertyRestriction<'a> {
    pub property_iri: &'a str,
    pub min: Option<u32>,
    pub max: Option<u32>,
}

/// Get all cardinality restrictions for a class, including those inherited from parent classes.
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
///
/// Restrictions defined directly on the class take precedence over inherited ones.
/// Cycles in the class hierarchy are handled via a visited set.
pub fn get_class_cardinality_restrictions(
    conn: &Connection,
    class_iri: &str,
) -> Result<Vec<CardinalityRestriction>> {
    let mut visited = std::collections::HashSet::new();
    get_class_cardinality_restrictions_inner(conn, class_iri, &mut visited)
}

fn get_class_cardinality_restrictions_inner(
    conn: &Connection,
    class_iri: &str,
    visited: &mut std::collections::HashSet<String>,
) -> Result<Vec<CardinalityRestriction>> {
    if !visited.insert(class_iri.to_string()) {
        return Ok(Vec::new());
    }

    let mut restrictions = Vec::new();
    let mut seen_properties = std::collections::HashSet::new();

    let subclass_result =
        query::get_by_entity_predicate(conn, class_iri, "rdfs:subClassOf")?;

    let mut parent_iris = Vec::new();

    for triple in &subclass_result.triples {
        if let Some(node) = triple.object.as_iri() {
            if node.starts_with("_:") {
                let type_result =
                    query::get_by_entity_predicate(conn, node, "rdf:type")?;
                let is_restriction = type_result.triples.iter().any(|t| {
                    t.object.as_iri().map(|iri| iri == owl::RESTRICTION).unwrap_or(false)
                });

                if !is_restriction {
                    continue;
                }

                let prop_result =
                    query::get_by_entity_predicate(conn, node, owl::ON_PROPERTY)?;
                let property_iri = match prop_result.triples.first().and_then(|t| t.object.as_iri()) {
                    Some(iri) => iri.to_string(),
                    None => continue,
                };

                let mut min = None;
                let mut max = None;
                let mut exact = None;

                let card_result =
                    query::get_by_entity_predicate(conn, node, owl::CARDINALITY)?;
                if let Some(t) = card_result.triples.first() {
                    if let crate::eavto::Object::Integer(v) = &t.object { exact = Some(*v as u32); }
                }

                let min_result =
                    query::get_by_entity_predicate(conn, node, owl::MIN_CARDINALITY)?;
                if let Some(t) = min_result.triples.first() {
                    if let crate::eavto::Object::Integer(v) = &t.object { min = Some(*v as u32); }
                }

                let max_result =
                    query::get_by_entity_predicate(conn, node, owl::MAX_CARDINALITY)?;
                if let Some(t) = max_result.triples.first() {
                    if let crate::eavto::Object::Integer(v) = &t.object { max = Some(*v as u32); }
                }

                seen_properties.insert(property_iri.clone());
                restrictions.push(CardinalityRestriction { property_iri, min, max, exact });
            } else {
                parent_iris.push(node.to_string());
            }
        }
    }

    // Classes with no explicit IRI parent implicitly inherit from owl:Thing (OWL semantics).
    // This ensures restrictions on owl:Thing (e.g. foundation:hasStatus) are always inherited.
    if parent_iris.is_empty() && class_iri != "owl:Thing" {
        parent_iris.push("owl:Thing".to_string());
    }

    for parent_iri in parent_iris {
        let inherited = get_class_cardinality_restrictions_inner(conn, &parent_iri, visited)?;
        for r in inherited {
            if !seen_properties.contains(&r.property_iri) {
                seen_properties.insert(r.property_iri.clone());
                restrictions.push(r);
            }
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
    let types_result = query::get_by_entity_predicate(conn, individual_iri, "rdf:type")?;

    if types_result.triples.is_empty() {
        return Ok(());
    }

    for type_triple in &types_result.triples {
        if let Some(class_iri) = type_triple.object.as_iri() {
            if !class_iri.starts_with("foundation:") {
                continue;
            }

            let restrictions = get_class_cardinality_restrictions(conn, class_iri)?;

            for restriction in restrictions {
                if restriction.property_iri == property_iri {
                    if restriction.is_violated(new_value_count) {
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

/// Set cardinality restrictions for a class by creating OWL minCardinality/maxCardinality restrictions.
///
/// Retracts all existing owl:Restriction blank nodes linked via rdfs:subClassOf,
/// then asserts new ones for each restriction in `restrictions`.
/// Restrictions with both min and max as None are skipped.
/// Pass an empty slice to remove all cardinality restrictions.
pub fn set_class_cardinality_restrictions(
    conn: &mut Connection,
    class_iri: &str,
    restrictions: &[PropertyRestriction<'_>],
    origin: &str,
) -> Result<()> {
    use crate::eavto::{store, query, Triple, Object};
    use sha2::{Sha256, Digest};

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

    let mut blank_internal_triples: Vec<Triple> = Vec::new();
    let mut subclass_link_triples: Vec<Triple> = Vec::new();

    for r in restrictions {
        let min_val = r.min.filter(|&m| m > 0);
        let has_max = r.max.is_some();
        if min_val.is_none() && !has_max {
            continue;
        }

        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}:restriction", class_iri, r.property_iri).as_bytes());
        let hash = hasher.finalize();
        let blank_id = format!(
            "_:restriction_{}",
            hash[..8].iter().map(|b| format!("{:02x}", b)).collect::<String>()
        );

        subclass_link_triples.push(Triple::new(
            class_iri,
            "rdfs:subClassOf",
            Object::Blank(blank_id.clone()),
        ));
        blank_internal_triples.push(Triple::new(
            &blank_id,
            "rdf:type",
            Object::Iri(owl::RESTRICTION.to_string()),
        ));
        blank_internal_triples.push(Triple::new(
            &blank_id,
            owl::ON_PROPERTY,
            Object::Iri(r.property_iri.to_string()),
        ));

        if let Some(min) = min_val {
            blank_internal_triples.push(Triple::new(
                &blank_id,
                owl::MIN_CARDINALITY,
                Object::Integer(min as i64),
            ));
        }
        if let Some(max) = r.max {
            blank_internal_triples.push(Triple::new(
                &blank_id,
                owl::MAX_CARDINALITY,
                Object::Integer(max as i64),
            ));
        }
    }

    if !blank_internal_triples.is_empty() {
        store::assert_triples(conn, &blank_internal_triples, origin)?;
    }
    if !subclass_link_triples.is_empty() {
        store::append_triples(conn, &subclass_link_triples, origin)?;
    }

    Ok(())
}

/// Set the required fields for a class by creating OWL minCardinality restrictions.
///
/// This is a convenience wrapper over `set_class_cardinality_restrictions` that sets
/// minCardinality=1 for each property. Pass an empty slice to remove all restrictions.
pub fn set_class_required_fields(
    conn: &mut Connection,
    class_iri: &str,
    required_properties: &[&str],
    origin: &str,
) -> Result<()> {
    let restrictions: Vec<PropertyRestriction<'_>> = required_properties
        .iter()
        .map(|iri| PropertyRestriction { property_iri: iri, min: Some(1), max: None })
        .collect();
    set_class_cardinality_restrictions(conn, class_iri, &restrictions, origin)
}

#[cfg(test)]
#[path = "cardinality_tests.rs"]
mod tests;

