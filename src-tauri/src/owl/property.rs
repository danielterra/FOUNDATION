use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, vocabulary::{rdf, rdfs, owl}};
use rusqlite::types::Value as SqlValue;

#[derive(Debug, Clone)]
pub struct DomainLabel {
    pub domain: String,
    pub forward_label: String,
    pub inverse_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Property {
    pub iri: String,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub property_type: PropertyType,
    pub domains: Vec<String>,
    pub ranges: Vec<String>,
    pub super_properties: Vec<String>,
    pub is_functional: bool,
    pub is_transitive: bool,
    pub is_symmetric: bool,
    pub inverse_of: Option<String>,
    pub unit: Option<String>,
    pub formula: Option<String>,
    pub aggregation: Option<String>,
    pub query_config: Option<String>,
    pub domain_labels: Vec<DomainLabel>,
    pub ai_behavior_rules: Option<String>,
}

impl Property {
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            iri: iri.into(),
            label: None,
            comment: None,
            property_type: PropertyType::RdfProperty,
            domains: vec![],
            ranges: vec![],
            super_properties: vec![],
            is_functional: false,
            is_transitive: false,
            is_symmetric: false,
            inverse_of: None,
            unit: None,
            formula: None,
            aggregation: None,
            query_config: None,
            domain_labels: vec![],
            ai_behavior_rules: None,
        }
    }

    fn get_domain_labels(conn: &Connection, property_iri: &str) -> Result<Vec<DomainLabel>> {
        let dl_result = query::get_by_predicate_object(
            conn, "foundation:onProperty", property_iri,
        )?;
        if dl_result.triples.is_empty() {
            return Ok(vec![]);
        }
        let dl_iris: Vec<String> = dl_result.triples.iter().map(|t| t.subject.clone()).collect();
        let dl_triples_map = query::batch_load_triples_for_subjects(conn, &dl_iris)?;
        let mut domain_labels = Vec::new();
        for dl_iri in &dl_iris {
            let triples = match dl_triples_map.get(dl_iri) {
                Some(t) => t,
                None => continue,
            };
            let domain = triples.iter()
                .find(|t| t.predicate == "foundation:forDomain")
                .and_then(|t| t.object.as_iri()).map(|s| s.to_string());
            let forward_label = triples.iter()
                .find(|t| t.predicate == "foundation:forwardLabel")
                .and_then(|t| t.object.as_literal());
            let inverse_label = triples.iter()
                .find(|t| t.predicate == "foundation:inverseLabel")
                .and_then(|t| t.object.as_literal());
            if let (Some(domain), Some(forward_label)) = (domain, forward_label) {
                domain_labels.push(DomainLabel { domain, forward_label, inverse_label });
            }
        }
        Ok(domain_labels)
    }

    fn get_domain_labels_batch(
        conn: &Connection,
        property_iris: &[&str],
    ) -> Result<std::collections::HashMap<String, Vec<DomainLabel>>> {
        if property_iris.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let placeholders = property_iris.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        // DomainLabel.onProperty is written once at creation and never updated.
        // Querying triples_current triggered the correlated MAX(tx) subquery for each of the
        // ~60 matching rows, causing ~100ms overhead per Property::get_batch call.
        // Using the raw triples table with retracted=0 is safe for this immutable relationship.
        let sql = format!(
            "SELECT subject, object FROM (
                SELECT subject, object, MAX(tx) OVER (PARTITION BY subject) AS max_tx, tx
                FROM triples
                WHERE predicate = 'foundation:onProperty' AND object IN ({}) AND retracted = 0
             ) WHERE tx = max_tx",
            placeholders
        );
        let params: Vec<SqlValue> = property_iris.iter()
            .map(|s| SqlValue::Text(s.to_string()))
            .collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| {
            crate::owl::OwlError::DatabaseError(e.to_string())
        })?;
        let dl_refs: Vec<(String, String)> = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let dl_iri: String = row.get(0)?;
                let prop_iri: Option<String> = row.get(1)?;
                Ok((dl_iri, prop_iri))
            })
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|(dl, p)| p.map(|pi| (dl, pi)))
            .collect();

        if dl_refs.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let dl_iris: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            dl_refs.iter().map(|(dl, _)| dl.clone()).filter(|s| seen.insert(s.clone())).collect()
        };
        let dl_triples_map = query::batch_load_triples_for_subjects(conn, &dl_iris)?;

        let mut result: std::collections::HashMap<String, Vec<DomainLabel>> =
            std::collections::HashMap::new();
        for (dl_iri, prop_iri) in dl_refs {
            let triples = match dl_triples_map.get(&dl_iri) {
                Some(t) => t,
                None => continue,
            };
            let domain = triples.iter()
                .find(|t| t.predicate == "foundation:forDomain")
                .and_then(|t| t.object.as_iri()).map(|s| s.to_string());
            let forward_label = triples.iter()
                .find(|t| t.predicate == "foundation:forwardLabel")
                .and_then(|t| t.object.as_literal());
            let inverse_label = triples.iter()
                .find(|t| t.predicate == "foundation:inverseLabel")
                .and_then(|t| t.object.as_literal());
            if let (Some(domain), Some(forward_label)) = (domain, forward_label) {
                result.entry(prop_iri).or_default()
                    .push(DomainLabel { domain, forward_label, inverse_label });
            }
        }
        Ok(result)
    }

    fn build_from_triples(iri: &str, triples: &[Triple]) -> Option<Self> {
        let has_type = triples.iter().any(|t| t.predicate == rdf::TYPE);
        if !has_type {
            return None;
        }

        let label = triples.iter()
            .find(|t| t.predicate == rdfs::LABEL)
            .and_then(|t| t.object.as_literal());
        let comment = triples.iter()
            .find(|t| t.predicate == rdfs::COMMENT)
            .and_then(|t| t.object.as_literal());

        let mut property_type = PropertyType::RdfProperty;
        let mut is_functional = false;
        let mut is_transitive = false;
        let mut is_symmetric = false;
        for triple in triples.iter().filter(|t| t.predicate == rdf::TYPE) {
            if let Some(type_iri) = triple.object.as_iri() {
                match type_iri {
                    t if t == owl::OBJECT_PROPERTY => property_type = PropertyType::ObjectProperty,
                    t if t == owl::DATATYPE_PROPERTY => {
                        property_type = PropertyType::DatatypeProperty
                    }
                    t if t == owl::ANNOTATION_PROPERTY => {
                        property_type = PropertyType::AnnotationProperty
                    }
                    t if t == owl::FUNCTIONAL_PROPERTY => is_functional = true,
                    t if t == owl::TRANSITIVE_PROPERTY => is_transitive = true,
                    t if t == owl::SYMMETRIC_PROPERTY => is_symmetric = true,
                    _ => {}
                }
            }
        }

        let domains: Vec<String> = triples.iter()
            .filter(|t| t.predicate == rdfs::DOMAIN)
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();
        let ranges: Vec<String> = triples.iter()
            .filter(|t| t.predicate == rdfs::RANGE)
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();

        if property_type == PropertyType::RdfProperty {
            let has_class_range = ranges.iter().any(|r| {
                !r.starts_with("xsd:") && r != "rdfs:Literal" && r != "rdf:langString"
            });
            if has_class_range {
                property_type = PropertyType::ObjectProperty;
            }
        }

        let super_properties: Vec<String> = triples.iter()
            .filter(|t| t.predicate == rdfs::SUB_PROPERTY_OF)
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect();
        let inverse_of = triples.iter()
            .find(|t| t.predicate == owl::INVERSE_OF)
            .and_then(|t| t.object.as_iri())
            .map(|s| s.to_string());
        let unit = triples.iter()
            .find(|t| t.predicate == "qudt:hasUnit")
            .and_then(|t| t.object.as_iri())
            .map(|s| s.to_string());
        let formula = triples.iter()
            .find(|t| t.predicate == "foundation:formula")
            .and_then(|t| t.object.as_literal());
        let aggregation = triples.iter()
            .find(|t| t.predicate == "foundation:aggregation")
            .and_then(|t| t.object.as_literal());
        let query_config = triples.iter()
            .find(|t| t.predicate == "foundation:queryConfig")
            .and_then(|t| t.object.as_literal());
        let ai_behavior_rules = triples.iter()
            .find(|t| t.predicate == "foundation:aiBehaviorRules")
            .and_then(|t| t.object.as_literal());

        Some(Self {
            iri: iri.to_string(),
            label,
            comment,
            property_type,
            domains,
            ranges,
            super_properties,
            is_functional,
            is_transitive,
            is_symmetric,
            inverse_of,
            unit,
            formula,
            aggregation,
            query_config,
            domain_labels: vec![],
            ai_behavior_rules,
        })
    }

    pub fn get(conn: &Connection, iri: impl Into<String>) -> Result<Option<Self>> {
        let iri = iri.into();
        let all_triples = query::get_by_entity(conn, &iri)?;
        let Some(mut prop) = Self::build_from_triples(&iri, &all_triples.triples) else {
            return Ok(None);
        };
        prop.domain_labels = Self::get_domain_labels(conn, &iri)?;
        Ok(Some(prop))
    }

    pub fn get_batch(
        conn: &Connection,
        iris: &[&str],
    ) -> Result<std::collections::HashMap<String, Self>> {
        if iris.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let iris_strings: Vec<String> = iris.iter().map(|s| s.to_string()).collect();
        let triples_map = query::batch_load_triples_for_subjects(conn, &iris_strings)?;
        let mut domain_labels_map = Self::get_domain_labels_batch(conn, iris)?;

        let mut result = std::collections::HashMap::new();
        for iri in iris {
            let triples = triples_map.get(*iri).map(|v| v.as_slice()).unwrap_or(&[]);
            let Some(mut prop) = Self::build_from_triples(iri, triples) else { continue };
            prop.domain_labels = domain_labels_map.remove(*iri).unwrap_or_default();
            result.insert(iri.to_string(), prop);
        }
        Ok(result)
    }

    pub fn assert(
        &self,
        conn: &mut Connection,
        property_type: PropertyType,
        label: &str,
        comment: Option<&str>,
        domains: &[&str],
        range: Option<&str>,
        unit: Option<&str>,
        origin: &str
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, &self.iri, None)?;
        if let Some(range_value) = range {
            let is_numeric = matches!(
                range_value,
                "xsd:decimal" | "xsd:integer" | "xsd:float" | "xsd:double"
            );

            if is_numeric && unit.is_none() {
                return Err(crate::owl::OwlError::ValidationError(
                    format!(
                        "Property '{}' has numeric range '{}' but no qudt:unit specified. \
                         Numeric properties MUST have a unit \
                         (e.g., unit:GigaBYTE, unit:Second, unit:Meter)",
                        self.iri, range_value
                    )
                ));
            }

            if !is_numeric && unit.is_some() {
                return Err(crate::owl::OwlError::ValidationError(
                    format!(
                        "Property '{}' has non-numeric range '{}' but qudt:unit was specified. \
                         Only numeric properties can have units.",
                        self.iri, range_value
                    )
                ));
            }
        }

        let type_iri = match property_type {
            PropertyType::RdfProperty => rdf::PROPERTY,
            PropertyType::ObjectProperty => owl::OBJECT_PROPERTY,
            PropertyType::DatatypeProperty => owl::DATATYPE_PROPERTY,
            PropertyType::AnnotationProperty => owl::ANNOTATION_PROPERTY,
        };

        let mut triples = vec![
            Triple::new(&self.iri, rdf::TYPE, Object::Iri(type_iri.to_string())),
            Triple::new(&self.iri, rdfs::LABEL, Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ];

        if let Some(comment_text) = comment {
            triples.push(Triple::new(&self.iri, rdfs::COMMENT, Object::Literal {
                value: comment_text.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }));
        }

        for domain_class in domains {
            triples.push(Triple::new(
                &self.iri,
                rdfs::DOMAIN,
                Object::Iri(domain_class.to_string()),
            ));
        }

        if let Some(range_class) = range {
            triples.push(Triple::new(&self.iri, rdfs::RANGE, Object::Iri(range_class.to_string())));
        }

        if let Some(unit_iri) = unit {
            triples.push(Triple::new(&self.iri, "qudt:hasUnit", Object::Iri(unit_iri.to_string())));
        }

        store::assert_triples(conn, &triples, origin)?;

        if let Some(formula_str) = &self.formula {
            let formula_triple = Triple::new(&self.iri, "foundation:formula", Object::Literal {
                value: formula_str.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[formula_triple], origin)?;
        }

        Ok(())
    }

    pub fn restore(conn: &mut Connection, iri: &str, origin: &str) -> Result<usize> {
        let retract_tx = query::get_retraction_tx(conn, iri)?
            .ok_or_else(|| OwlError::NotFound(
                format!("Property '{}' has no retracted triples to restore", iri)
            ))?;

        let def = query::get_last_active_by_entity_before_tx(conn, iri, retract_tx)?;
        let def_triples: Vec<Triple> = def.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        if !def_triples.is_empty() {
            store::assert_triples(conn, &def_triples, origin)?;
        }

        let facts = query::get_last_active_by_predicate_before_tx(conn, iri, retract_tx)?;
        let count = facts.triples.len();
        let fact_triples: Vec<Triple> = facts.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        if !fact_triples.is_empty() {
            store::assert_triples(conn, &fact_triples, origin)?;
        }

        Ok(count)
    }

    pub fn retract(conn: &mut Connection, iri: &str, origin: &str) -> Result<Vec<String>> {
        crate::owl::check_system_locked(conn, iri, None)?;
        let facts = query::get_by_predicate(conn, iri)?;
        let mut affected: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut all_triples: Vec<Triple> = facts.triples.into_iter()
            .map(|t| {
                affected.insert(t.subject.clone());
                Triple::new(t.subject, t.predicate, t.object)
            })
            .collect();
        let definition = query::get_by_entity(conn, iri)?;
        all_triples.extend(
            definition.triples.into_iter()
                .map(|t| Triple::new(t.subject, t.predicate, t.object))
        );
        if !all_triples.is_empty() {
            store::retract_triples(conn, &all_triples, origin)?;
        }
        Ok(affected.into_iter().collect())
    }

    pub fn find_all_iris(conn: &Connection) -> Result<Vec<String>> {
        let obj_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::OBJECT_PROPERTY)?;
        let dat_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::DATATYPE_PROPERTY)?;
        let mut iris: Vec<String> = obj_result.triples.into_iter()
            .chain(dat_result.triples)
            .map(|t| t.subject)
            .collect();
        iris.sort();
        iris.dedup();
        Ok(iris)
    }

    pub fn is_functional(conn: &Connection, property_iri: &str) -> Result<bool> {
        let types_result = crate::eavto::query::get_by_entity_predicate_internal(
            conn,
            property_iri,
            rdf::TYPE,
            false
        )?;

        for triple in &types_result.triples {
            if let Some(type_iri) = triple.object.as_iri() {
                if type_iri == owl::FUNCTIONAL_PROPERTY {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

#[allow(dead_code)]
pub type ObjectProperty = Property;

#[allow(dead_code)]
pub type DatatypeProperty = Property;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    RdfProperty,
    ObjectProperty,
    DatatypeProperty,
    AnnotationProperty,
}

#[cfg(test)]
#[path = "property_tests.rs"]
mod tests;
