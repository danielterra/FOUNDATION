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
                return format!(
                    "Property '{}' allows at most {} value(s), but has {}",
                    property_name, max, count
                );
            }
        }

        format!("Property '{}' cardinality constraint violated", property_name)
    }
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
pub async fn get_class_cardinality_restrictions(
    conn: &Connection,
    class_iri: &str,
) -> Result<Vec<CardinalityRestriction>> {
    let mut visited = std::collections::HashSet::new();
    Box::pin(get_class_cardinality_restrictions_inner(conn, class_iri, &mut visited)).await
}

async fn get_class_cardinality_restrictions_inner(
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
        query::get_by_entity_predicate(conn, class_iri, "rdfs:subClassOf").await?;

    let mut parent_iris = Vec::new();

    for triple in &subclass_result.triples {
        if let Some(node) = triple.object.as_iri() {
            if node.starts_with("_:") {
                let type_result =
                    query::get_by_entity_predicate(conn, node, "rdf:type").await?;
                let is_restriction = type_result.triples.iter().any(|t| {
                    t.object.as_iri().map(|iri| iri == owl::RESTRICTION).unwrap_or(false)
                });

                if !is_restriction {
                    continue;
                }

                let prop_result =
                    query::get_by_entity_predicate(conn, node, owl::ON_PROPERTY).await?;
                let property_iri = match prop_result.triples.first().and_then(|t| t.object.as_iri()) {
                    Some(iri) => iri.to_string(),
                    None => continue,
                };

                let mut min = None;
                let mut max = None;
                let mut exact = None;

                let card_result =
                    query::get_by_entity_predicate(conn, node, owl::CARDINALITY).await?;
                if let Some(t) = card_result.triples.first() {
                    if let crate::eavto::Object::Integer(v) = &t.object { exact = Some(*v as u32); }
                }

                let min_result =
                    query::get_by_entity_predicate(conn, node, owl::MIN_CARDINALITY).await?;
                if let Some(t) = min_result.triples.first() {
                    if let crate::eavto::Object::Integer(v) = &t.object { min = Some(*v as u32); }
                }

                let max_result =
                    query::get_by_entity_predicate(conn, node, owl::MAX_CARDINALITY).await?;
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

    if parent_iris.is_empty() && class_iri != "owl:Thing" {
        parent_iris.push("owl:Thing".to_string());
    }

    for parent_iri in parent_iris {
        let inherited = Box::pin(get_class_cardinality_restrictions_inner(conn, &parent_iri, visited)).await?;
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
pub async fn validate_property_cardinality(
    conn: &Connection,
    individual_iri: &str,
    property_iri: &str,
    new_value_count: usize,
) -> Result<()> {
    let types_result = query::get_by_entity_predicate(conn, individual_iri, "rdf:type").await?;

    if types_result.triples.is_empty() {
        return Ok(());
    }

    for type_triple in &types_result.triples {
        if let Some(class_iri) = type_triple.object.as_iri() {
            if !class_iri.starts_with("foundation:") {
                continue;
            }

            let restrictions = get_class_cardinality_restrictions(conn, class_iri).await?;

            for restriction in restrictions {
                if restriction.property_iri == property_iri {
                    if restriction.is_violated(new_value_count) {
                        let prop_label_result = query::get_by_entity_predicate(
                            conn,
                            property_iri,
                            "rdfs:label",
                        ).await?;
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
pub async fn set_class_required_fields(
    conn: &Connection,
    class_iri: &str,
    required_properties: &[&str],
    origin: &str,
) -> Result<()> {
    use crate::eavto::{store, query, Triple, Object};
    use sha2::{Sha256, Digest};

    let subclass_result = query::get_by_entity_predicate(conn, class_iri, "rdfs:subClassOf").await?;
    for triple in &subclass_result.triples {
        if let Some(node) = triple.object.as_iri() {
            if !node.starts_with("_:") {
                continue;
            }
            let type_result = query::get_by_entity_predicate(conn, node, "rdf:type").await?;
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
                let result = query::get_by_entity_predicate(conn, node, predicate).await?;
                for rt in result.triples {
                    to_retract.push(Triple::new(node, predicate, rt.object));
                }
            }
            store::retract_triples(conn, &to_retract, origin).await
                .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
        }
    }

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
        store::assert_triples(conn, &blank_internal_triples, origin).await
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
    }
    if !subclass_link_triples.is_empty() {
        store::append_triples(conn, &subclass_link_triples, origin).await
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
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

        assert!(!restriction.is_violated(1));
        assert!(restriction.is_violated(0));
        assert!(restriction.is_violated(2));
    }

    #[test]
    fn test_min_cardinality() {
        let restriction = CardinalityRestriction {
            property_iri: "foundation:email".to_string(),
            min: Some(1),
            max: None,
            exact: None,
        };

        assert!(restriction.is_violated(0));
        assert!(!restriction.is_violated(1));
        assert!(!restriction.is_violated(5));
    }

    #[test]
    fn test_max_cardinality() {
        let restriction = CardinalityRestriction {
            property_iri: "foundation:phone".to_string(),
            min: None,
            max: Some(3),
            exact: None,
        };

        assert!(!restriction.is_violated(0));
        assert!(!restriction.is_violated(3));
        assert!(restriction.is_violated(4));
    }

    #[test]
    fn test_min_max_cardinality() {
        let restriction = CardinalityRestriction {
            property_iri: "foundation:hasPhoneNumber".to_string(),
            min: Some(0),
            max: Some(2),
            exact: None,
        };

        assert!(!restriction.is_violated(0));
        assert!(!restriction.is_violated(1));
        assert!(!restriction.is_violated(2));
        assert!(restriction.is_violated(3));
    }

    #[tokio::test]
    async fn test_set_class_required_fields() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:TestClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        ], "test").await.unwrap();

        set_class_required_fields(
            &conn,
            "foundation:TestClass",
            &["foundation:name", "foundation:email"],
            "test",
        ).await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:TestClass").await.unwrap();
        assert_eq!(restrictions.len(), 2);
        let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();
        assert!(props.contains(&"foundation:name"));
        assert!(props.contains(&"foundation:email"));
        for r in &restrictions {
            assert_eq!(r.min, Some(1));
        }

        set_class_required_fields(
            &conn,
            "foundation:TestClass",
            &["foundation:name"],
            "test",
        ).await.unwrap();

        let restrictions2 = get_class_cardinality_restrictions(&conn, "foundation:TestClass").await.unwrap();
        assert_eq!(restrictions2.len(), 1);
        assert_eq!(restrictions2[0].property_iri, "foundation:name");

        set_class_required_fields(&conn, "foundation:TestClass", &[], "test").await.unwrap();
        let restrictions3 = get_class_cardinality_restrictions(&conn, "foundation:TestClass").await.unwrap();
        assert_eq!(restrictions3.len(), 0);
    }

    #[tokio::test]
    async fn test_set_required_fields_preserves_parent_class_subclass_link() {
        use crate::eavto::{store, query, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:ParentClass", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:ChildClass", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:ChildClass", "rdfs:subClassOf", Object::Iri("foundation:ParentClass".to_string())),
            Triple::new("foundation:childProp", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
            Triple::new("foundation:childProp", "rdfs:domain", Object::Iri("foundation:ChildClass".to_string())),
        ], "test").await.unwrap();

        set_class_required_fields(&conn, "foundation:ChildClass", &["foundation:childProp"], "test").await.unwrap();

        let subclass_result = query::get_by_entity_predicate(
            &conn, "foundation:ChildClass", "rdfs:subClassOf",
        ).await.unwrap();
        let real_parent_links: Vec<&str> = subclass_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .filter(|iri| !iri.starts_with("_:"))
            .collect();

        assert!(
            real_parent_links.contains(&"foundation:ParentClass"),
            "rdfs:subClassOf foundation:ParentClass must survive set_class_required_fields; got: {:?}",
            real_parent_links
        );
    }

    #[tokio::test]
    async fn test_inherited_properties_accessible_after_set_required_fields() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;
        use crate::owl::Class;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:FinancialTransaction", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:transactionDate", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
            Triple::new("foundation:transactionDate", "rdfs:domain", Object::Iri("foundation:FinancialTransaction".to_string())),
            Triple::new("foundation:InstallmentPayment", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:InstallmentPayment", "rdfs:subClassOf", Object::Iri("foundation:FinancialTransaction".to_string())),
            Triple::new("foundation:dueDate", "rdf:type", Object::Iri("owl:DatatypeProperty".to_string())),
            Triple::new("foundation:dueDate", "rdfs:domain", Object::Iri("foundation:InstallmentPayment".to_string())),
        ], "test").await.unwrap();

        set_class_required_fields(
            &conn, "foundation:InstallmentPayment", &["foundation:dueDate"], "test",
        ).await.unwrap();

        let class = Class::get(&conn, "foundation:InstallmentPayment").await
            .unwrap()
            .expect("InstallmentPayment class must exist after set_class_required_fields");

        let prop_iris: Vec<&str> = class.properties.iter().map(|(iri, _)| iri.as_str()).collect();
        assert!(
            prop_iris.contains(&"foundation:transactionDate"),
            "Inherited property foundation:transactionDate must remain accessible \
             after set_class_required_fields; got: {:?}",
            prop_iris
        );
    }

    // ── is_required ─────────────────────────────────────────────────────────

    #[test]
    fn test_is_required_exact_one() {
        let r = CardinalityRestriction { property_iri: "p".to_string(), min: None, max: None, exact: Some(1) };
        assert!(r.is_required());
    }

    #[test]
    fn test_is_required_exact_zero_is_not_required() {
        let r = CardinalityRestriction { property_iri: "p".to_string(), min: None, max: None, exact: Some(0) };
        assert!(!r.is_required());
    }

    #[test]
    fn test_is_required_min_one() {
        let r = CardinalityRestriction { property_iri: "p".to_string(), min: Some(1), max: None, exact: None };
        assert!(r.is_required());
    }

    #[test]
    fn test_is_required_min_zero_is_not_required() {
        let r = CardinalityRestriction { property_iri: "p".to_string(), min: Some(0), max: None, exact: None };
        assert!(!r.is_required());
    }

    #[test]
    fn test_is_required_no_constraints_is_not_required() {
        let r = CardinalityRestriction { property_iri: "p".to_string(), min: None, max: None, exact: None };
        assert!(!r.is_required());
    }

    #[test]
    fn test_is_required_max_only_is_not_required() {
        let r = CardinalityRestriction { property_iri: "p".to_string(), min: None, max: Some(5), exact: None };
        assert!(!r.is_required());
    }

    // ── violation_message ───────────────────────────────────────────────────

    #[test]
    fn test_violation_message_exact_uses_property_label() {
        let r = CardinalityRestriction { property_iri: "foundation:name".to_string(), min: None, max: None, exact: Some(1) };
        let msg = r.violation_message(0, Some("Name"));
        assert_eq!(msg, "Property 'Name' requires exactly 1 value(s), but has 0");
    }

    #[test]
    fn test_violation_message_exact_falls_back_to_iri() {
        let r = CardinalityRestriction { property_iri: "foundation:name".to_string(), min: None, max: None, exact: Some(2) };
        let msg = r.violation_message(3, None);
        assert_eq!(msg, "Property 'foundation:name' requires exactly 2 value(s), but has 3");
    }

    #[test]
    fn test_violation_message_min_below_threshold() {
        let r = CardinalityRestriction { property_iri: "foundation:email".to_string(), min: Some(1), max: None, exact: None };
        let msg = r.violation_message(0, Some("Email"));
        assert_eq!(msg, "Property 'Email' requires at least 1 value(s), but has 0");
    }

    #[test]
    fn test_violation_message_max_exceeded() {
        let r = CardinalityRestriction { property_iri: "foundation:phone".to_string(), min: None, max: Some(3), exact: None };
        let msg = r.violation_message(5, Some("Phone"));
        assert_eq!(msg, "Property 'Phone' allows at most 3 value(s), but has 5");
    }

    #[test]
    fn test_violation_message_fallback_when_no_branch_matches() {
        let r = CardinalityRestriction { property_iri: "foundation:tag".to_string(), min: Some(1), max: Some(5), exact: None };
        let msg = r.violation_message(3, Some("Tag"));
        assert_eq!(msg, "Property 'Tag' cardinality constraint violated");
    }

    // ── get_class_cardinality_restrictions ──────────────────────────────────

    #[tokio::test]
    async fn test_get_class_cardinality_restrictions_empty_for_no_restrictions() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Task", "rdf:type", Object::Iri("owl:Class".to_string())),
        ], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Task").await.unwrap();
        assert!(restrictions.is_empty());
    }

    #[tokio::test]
    async fn test_get_class_cardinality_restrictions_reads_min_cardinality() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Project", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Project", "rdfs:subClassOf", Object::Blank("_:r1".to_string())),
            Triple::new("_:r1", "rdf:type", Object::Iri("owl:Restriction".to_string())),
            Triple::new("_:r1", "owl:onProperty", Object::Iri("foundation:title".to_string())),
            Triple::new("_:r1", "owl:minCardinality", Object::Integer(1)),
        ], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Project").await.unwrap();
        assert_eq!(restrictions.len(), 1);
        assert_eq!(restrictions[0].property_iri, "foundation:title");
        assert_eq!(restrictions[0].min, Some(1));
        assert_eq!(restrictions[0].max, None);
        assert_eq!(restrictions[0].exact, None);
    }

    #[tokio::test]
    async fn test_get_class_cardinality_restrictions_reads_exact_cardinality() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Invoice", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Invoice", "rdfs:subClassOf", Object::Blank("_:r2".to_string())),
            Triple::new("_:r2", "rdf:type", Object::Iri("owl:Restriction".to_string())),
            Triple::new("_:r2", "owl:onProperty", Object::Iri("foundation:invoiceNumber".to_string())),
            Triple::new("_:r2", "owl:cardinality", Object::Integer(1)),
        ], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Invoice").await.unwrap();
        assert_eq!(restrictions.len(), 1);
        assert_eq!(restrictions[0].exact, Some(1));
    }

    #[tokio::test]
    async fn test_get_class_cardinality_restrictions_reads_max_cardinality() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Task", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Task", "rdfs:subClassOf", Object::Blank("_:r3".to_string())),
            Triple::new("_:r3", "rdf:type", Object::Iri("owl:Restriction".to_string())),
            Triple::new("_:r3", "owl:onProperty", Object::Iri("foundation:assignedTo".to_string())),
            Triple::new("_:r3", "owl:maxCardinality", Object::Integer(5)),
        ], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Task").await.unwrap();
        assert_eq!(restrictions.len(), 1);
        assert_eq!(restrictions[0].max, Some(5));
    }

    #[tokio::test]
    async fn test_get_class_cardinality_restrictions_ignores_non_restriction_subclasses() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Child", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Child", "rdfs:subClassOf", Object::Iri("foundation:Parent".to_string())),
        ], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Child").await.unwrap();
        assert!(restrictions.is_empty(), "plain IRI subClassOf must not be treated as restriction");
    }

    // ── validate_property_cardinality ───────────────────────────────────────

    #[tokio::test]
    async fn test_validate_property_cardinality() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Person", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Person", "rdfs:subClassOf", Object::Blank("_:r1".to_string())),
            Triple::new("_:r1", "rdf:type", Object::Iri("owl:Restriction".to_string())),
            Triple::new("_:r1", "owl:onProperty", Object::Iri("foundation:name".to_string())),
            Triple::new("_:r1", "owl:minCardinality", Object::Integer(1)),
        ], "test").await.unwrap();

        store::assert_triples(&conn, &[
            Triple::new("foundation:john", "rdf:type", Object::Iri("foundation:Person".to_string())),
        ], "test").await.unwrap();

        validate_property_cardinality(&conn, "foundation:john", "foundation:name", 1).await.unwrap();

        let result = validate_property_cardinality(&conn, "foundation:john", "foundation:name", 0).await;
        assert!(result.is_err(), "Should fail with 0 values for a required field");
    }

    #[tokio::test]
    async fn test_validate_property_cardinality_exact_violation() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Invoice", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Invoice", "rdfs:subClassOf", Object::Blank("_:r1".to_string())),
            Triple::new("_:r1", "rdf:type", Object::Iri("owl:Restriction".to_string())),
            Triple::new("_:r1", "owl:onProperty", Object::Iri("foundation:invoiceNumber".to_string())),
            Triple::new("_:r1", "owl:cardinality", Object::Integer(1)),
            Triple::new("foundation:inv1", "rdf:type", Object::Iri("foundation:Invoice".to_string())),
        ], "test").await.unwrap();

        assert!(validate_property_cardinality(&conn, "foundation:inv1", "foundation:invoiceNumber", 1).await.is_ok());
        assert!(validate_property_cardinality(&conn, "foundation:inv1", "foundation:invoiceNumber", 0).await.is_err());
        assert!(validate_property_cardinality(&conn, "foundation:inv1", "foundation:invoiceNumber", 2).await.is_err());
    }

    #[tokio::test]
    async fn test_validate_property_cardinality_max_violation() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Task", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Task", "rdfs:subClassOf", Object::Blank("_:r2".to_string())),
            Triple::new("_:r2", "rdf:type", Object::Iri("owl:Restriction".to_string())),
            Triple::new("_:r2", "owl:onProperty", Object::Iri("foundation:assignedTo".to_string())),
            Triple::new("_:r2", "owl:maxCardinality", Object::Integer(3)),
            Triple::new("foundation:task1", "rdf:type", Object::Iri("foundation:Task".to_string())),
        ], "test").await.unwrap();

        assert!(validate_property_cardinality(&conn, "foundation:task1", "foundation:assignedTo", 3).await.is_ok());
        assert!(validate_property_cardinality(&conn, "foundation:task1", "foundation:assignedTo", 4).await.is_err());
    }

    #[tokio::test]
    async fn test_validate_property_cardinality_error_message_contains_property_label() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Person", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Person", "rdfs:subClassOf", Object::Blank("_:r3".to_string())),
            Triple::new("_:r3", "rdf:type", Object::Iri("owl:Restriction".to_string())),
            Triple::new("_:r3", "owl:onProperty", Object::Iri("foundation:fullName".to_string())),
            Triple::new("_:r3", "owl:minCardinality", Object::Integer(1)),
            Triple::new("foundation:fullName", "rdfs:label", Object::Literal {
                value: "Full Name".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:alice", "rdf:type", Object::Iri("foundation:Person".to_string())),
        ], "test").await.unwrap();

        let err = validate_property_cardinality(&conn, "foundation:alice", "foundation:fullName", 0)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Full Name"), "error message should use property label, got: {msg}");
        assert!(msg.contains("0"), "error message should mention the count, got: {msg}");
    }

    #[tokio::test]
    async fn test_validate_property_cardinality_no_type_skips_validation() {
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;
        assert!(validate_property_cardinality(&conn, "foundation:orphan", "foundation:name", 0).await.is_ok());
    }

    // ── inherited required fields ────────────────────────────────────────────

    #[tokio::test]
    async fn test_child_inherits_required_fields_from_parent() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:ParentClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        ], "test").await.unwrap();
        set_class_required_fields(&conn, "foundation:ParentClass", &["foundation:title"], "test").await.unwrap();

        store::assert_triples(&conn, &[
            Triple::new("foundation:ChildClass", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:ChildClass", "rdfs:subClassOf", Object::Iri("foundation:ParentClass".to_string())),
        ], "test").await.unwrap();
        set_class_required_fields(&conn, "foundation:ChildClass", &["foundation:dueDate"], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:ChildClass").await.unwrap();
        let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();

        assert!(props.contains(&"foundation:dueDate"), "own required field must be present");
        assert!(props.contains(&"foundation:title"), "inherited required field from parent must be present");
        assert_eq!(restrictions.len(), 2);
    }

    #[tokio::test]
    async fn test_grandparent_required_fields_are_inherited() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:GrandParent", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Parent", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Parent", "rdfs:subClassOf", Object::Iri("foundation:GrandParent".to_string())),
            Triple::new("foundation:Child", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Child", "rdfs:subClassOf", Object::Iri("foundation:Parent".to_string())),
        ], "test").await.unwrap();

        set_class_required_fields(&conn, "foundation:GrandParent", &["foundation:name"], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Child").await.unwrap();
        let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();

        assert!(props.contains(&"foundation:name"), "grandparent required field must be inherited by grandchild");
    }

    #[tokio::test]
    async fn test_child_own_required_field_takes_precedence_over_inherited() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Base", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Derived", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Derived", "rdfs:subClassOf", Object::Iri("foundation:Base".to_string())),
        ], "test").await.unwrap();

        set_class_required_fields(&conn, "foundation:Base", &["foundation:name"], "test").await.unwrap();
        set_class_required_fields(&conn, "foundation:Derived", &["foundation:name"], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Derived").await.unwrap();
        let name_restrictions: Vec<_> = restrictions.iter()
            .filter(|r| r.property_iri == "foundation:name")
            .collect();

        assert_eq!(name_restrictions.len(), 1, "same property must not appear twice; got {:?}", name_restrictions);
    }

    #[tokio::test]
    async fn test_class_without_explicit_parent_inherits_from_owl_thing() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("owl:Thing", "rdf:type", Object::Iri("owl:Class".to_string())),
        ], "test").await.unwrap();
        set_class_required_fields(&conn, "owl:Thing", &["foundation:hasStatus"], "test").await.unwrap();

        store::assert_triples(&conn, &[
            Triple::new("foundation:MyClass", "rdf:type", Object::Iri("owl:Class".to_string())),
        ], "test").await.unwrap();
        set_class_required_fields(&conn, "foundation:MyClass", &["foundation:label"], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:MyClass").await.unwrap();
        let props: Vec<&str> = restrictions.iter().map(|r| r.property_iri.as_str()).collect();

        assert!(props.contains(&"foundation:label"), "own required field must be present");
        assert!(
            props.contains(&"foundation:hasStatus"),
            "owl:Thing required field must be inherited by classes with no explicit parent; got: {:?}",
            props,
        );
    }

    #[tokio::test]
    async fn test_no_required_fields_on_parent_returns_only_child_fields() {
        use crate::eavto::{store, Triple, Object};
        use crate::eavto::test_helpers::setup_test_db;

        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Base", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Derived", "rdf:type", Object::Iri("owl:Class".to_string())),
            Triple::new("foundation:Derived", "rdfs:subClassOf", Object::Iri("foundation:Base".to_string())),
        ], "test").await.unwrap();

        set_class_required_fields(&conn, "foundation:Derived", &["foundation:email"], "test").await.unwrap();

        let restrictions = get_class_cardinality_restrictions(&conn, "foundation:Derived").await.unwrap();
        assert_eq!(restrictions.len(), 1);
        assert_eq!(restrictions[0].property_iri, "foundation:email");
    }
}
