use super::*;
use super::timestamps::touch;
use crate::owl::Property;

/// Generic: returns true if `property_iri` has a triple with `metadata_predicate`.
/// Used by write hooks to decide whether to trigger recalculation.
pub(super) fn property_has_metadata(conn: &Connection, property_iri: &str, metadata_predicate: &str) -> Result<bool> {
    let result = query::get_by_entity_predicate(conn, property_iri, metadata_predicate)?;
    Ok(!result.triples.is_empty())
}

fn ensure_class_exists(conn: &Connection, class_iri: &str) -> Result<()> {
    if crate::owl::Class::exists(conn, class_iri) {
        Ok(())
    } else {
        Err(OwlError::ValidationError(format!(
            "Class '{}' does not exist in the ontology. \
             Create it first with define_class (check class_graph and search for an existing \
             equivalent before creating), or use an existing class IRI.",
            class_iri,
        )))
    }
}

impl Individual {
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
        crate::owl::check_system_locked(conn, &self.iri, None)?;
        ensure_class_exists(conn, class_iri)?;
        crate::owl::validate_icon(conn, icon)?;
        Self::validate_disjointness(conn, &self.iri, class_iri)?;

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

        touch(conn, &self.iri.clone());

        Ok(())
    }

    /// Assert individual with only label — no icon stored on the instance.
    /// Use when all instances of a class share the class icon and don't need individual icons.
    pub fn assert_without_icon(
        &self,
        conn: &mut Connection,
        class_iri: &str,
        label: &str,
        origin: &str,
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, &self.iri, None)?;
        ensure_class_exists(conn, class_iri)?;
        Self::validate_disjointness(conn, &self.iri, class_iri)?;

        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(class_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;

        let label_triple = Triple::new(&self.iri, rdfs::LABEL, Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
        store::assert_triples(conn, &[label_triple], origin)?;

        touch(conn, &self.iri.clone());

        Ok(())
    }

    pub fn add_property(
        &self,
        conn: &mut Connection,
        property: &str,
        values: Vec<Object>,
        origin: &str,
    ) -> Result<()> {
        let t0 = std::time::Instant::now();

        crate::owl::check_system_locked(conn, &self.iri, Some(property))?;
        let t_lock = t0.elapsed().as_millis();

        if values.is_empty() {
            return Err(OwlError::InvalidOperation(
                format!("No values provided for property {}", property)
            ));
        }

        let is_meta_property = property.starts_with("rdfs:")
            || property == "foundation:hasIcon";

        if !is_meta_property {
            if let Ok(true) = property_has_metadata(conn, property, "foundation:formula") {
                return Err(OwlError::ValidationError(format!(
                    "Property '{}' is calculated via a formula and cannot be set directly",
                    property
                )));
            }
        }
        let t_formula = t0.elapsed().as_millis();

        let types_result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;
        let t_types = t0.elapsed().as_millis();

        if types_result.triples.is_empty() {
            return Err(OwlError::NotFound(format!("Individual {} has no rdf:type", self.iri)));
        }

        let subject_is_property_def = types_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .any(|iri| iri == "owl:DatatypeProperty" || iri == "owl:ObjectProperty");

        if !is_meta_property && !subject_is_property_def {
            let property_is_valid = types_result.triples.iter()
                .filter_map(|t| t.object.as_iri())
                .any(|class_iri| Class::has_property(conn, class_iri, property));

            if !property_is_valid {
                let individual_class = types_result.triples.first()
                    .and_then(|t| t.object.as_iri())
                    .unwrap_or("unknown");

                let property_exists = Property::get(conn, property)
                    .ok()
                    .flatten()
                    .is_some();

                if !property_exists {
                    let local_name = property.split(':').last().unwrap_or(property);
                    let hint = if local_name == "comment" {
                        " To annotate with a comment, use rdfs:comment (annotation property, domain rdfs:Resource).".to_string()
                    } else {
                        String::new()
                    };
                    return Err(OwlError::NotFound(
                        format!("Property not found: {}.{}", property, hint)
                    ));
                }

                let domains: Vec<String> = query::get_by_entity_predicate(conn, property, "rdfs:domain")
                    .map(|r| r.triples.iter().filter_map(|t| t.object.as_iri()).map(String::from).collect())
                    .unwrap_or_default();
                let domain_hint = if domains.is_empty() {
                    " (no domain defined)".to_string()
                } else {
                    format!(" (domain: {})", domains.join(", "))
                };
                return Err(OwlError::InvalidOperation(
                    format!(
                        "Property {}{} is not defined for {}. Use define_property to add {} to the domain, then retry.",
                        property, domain_hint, individual_class, individual_class
                    )
                ));
            }
        }
        let t_has_prop = t0.elapsed().as_millis();

        if !is_meta_property {
            Self::validate_value_type(conn, property, &values)?;
            for value in &values {
                Self::validate_iri_exists(conn, property, value)?;
                Self::validate_range_type(conn, property, value)?;
                Self::validate_one_of_constraint(conn, property, value)?;
                Self::validate_literal_datatype(property, value)?;
            }
        }
        if property == rdf::TYPE {
            for value in &values {
                if let Some(class_iri) = value.as_iri() {
                    Self::validate_disjointness(conn, &self.iri, class_iri)?;
                }
            }
        }
        let t_validate = t0.elapsed().as_millis();

        crate::owl::cardinality::validate_property_cardinality(
            conn,
            &self.iri,
            property,
            values.len()
        )?;
        let t_cardinality = t0.elapsed().as_millis();

        let triples: Vec<Triple> = values.into_iter()
            .map(|v| Triple::new(&self.iri, property, v))
            .collect();
        store::assert_triples(conn, &triples, origin)?;
        let t_assert = t0.elapsed().as_millis();

        if property != super::timestamps::LAST_UPDATED_AT {
            touch(conn, &self.iri.clone());
        }
        let t_touch = t0.elapsed().as_millis();

        if t_touch > 20 {
            crate::diagnostics::log_backend("debug", &format!(
                "[OWL] add_property({}) total={}ms [lock={}ms formula={}ms types={}ms has_prop={}ms validate={}ms cardinality={}ms assert={}ms touch={}ms]",
                property, t_touch, t_lock, t_formula - t_lock, t_types - t_formula,
                t_has_prop - t_types, t_validate - t_has_prop,
                t_cardinality - t_validate, t_assert - t_cardinality, t_touch - t_assert
            ));
        }

        Ok(())
    }

    pub fn append_property(
        &self,
        conn: &mut Connection,
        property: &str,
        values: Vec<Object>,
        origin: &str,
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, &self.iri, Some(property))?;
        if values.is_empty() {
            return Err(OwlError::InvalidOperation(
                format!("No values provided for property {}", property)
            ));
        }

        let is_meta_property = property.starts_with("rdfs:")
            || property == "foundation:hasIcon";

        if !is_meta_property {
            if let Ok(true) = property_has_metadata(conn, property, "foundation:formula") {
                return Err(OwlError::ValidationError(format!(
                    "Property '{}' is calculated via a formula and cannot be set directly",
                    property
                )));
            }
            if let Ok(true) = property_has_metadata(conn, property, "foundation:queryConfig") {
                return Err(OwlError::ValidationError(format!(
                    "Property '{}' is a query property and cannot be set directly",
                    property
                )));
            }
        }

        let types_result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;

        if types_result.triples.is_empty() {
            return Err(OwlError::NotFound(format!("Individual {} has no rdf:type", self.iri)));
        }

        if !is_meta_property {
            let property_is_valid = types_result.triples.iter()
                .filter_map(|t| t.object.as_iri())
                .any(|class_iri| Class::has_property(conn, class_iri, property));

            if !property_is_valid {
                let individual_class = types_result.triples.first()
                    .and_then(|t| t.object.as_iri())
                    .unwrap_or("unknown");

                let property_exists = Property::get(conn, property)
                    .ok()
                    .flatten()
                    .is_some();

                if !property_exists {
                    let local_name = property.split(':').last().unwrap_or(property);
                    let hint = if local_name == "comment" {
                        " To annotate with a comment, use rdfs:comment (annotation property, domain rdfs:Resource).".to_string()
                    } else {
                        String::new()
                    };
                    return Err(OwlError::NotFound(
                        format!("Property not found: {}.{}", property, hint)
                    ));
                }

                let domains: Vec<String> = query::get_by_entity_predicate(conn, property, "rdfs:domain")
                    .map(|r| r.triples.iter().filter_map(|t| t.object.as_iri()).map(String::from).collect())
                    .unwrap_or_default();
                let domain_hint = if domains.is_empty() {
                    " (no domain defined)".to_string()
                } else {
                    format!(" (domain: {})", domains.join(", "))
                };
                return Err(OwlError::InvalidOperation(
                    format!(
                        "Property {}{} is not defined for {}. Use define_property to add {} to the domain, then retry.",
                        property, domain_hint, individual_class, individual_class
                    )
                ));
            }
        }

        if !is_meta_property {
            Self::validate_value_type(conn, property, &values)?;
            for value in &values {
                Self::validate_iri_exists(conn, property, value)?;
                Self::validate_range_type(conn, property, value)?;
                Self::validate_one_of_constraint(conn, property, value)?;
                Self::validate_literal_datatype(property, value)?;
            }
        }
        if property == rdf::TYPE {
            for value in &values {
                if let Some(class_iri) = value.as_iri() {
                    Self::validate_disjointness(conn, &self.iri, class_iri)?;
                }
            }
        }

        let current_count = Self::get_property_count(conn, &self.iri, property)?;
        crate::owl::cardinality::validate_property_cardinality(
            conn,
            &self.iri,
            property,
            current_count + values.len(),
        )?;

        let triples: Vec<Triple> = values.into_iter()
            .map(|v| Triple::new(&self.iri, property, v))
            .collect();
        store::append_triples(conn, &triples, origin)?;

        if property != super::timestamps::LAST_UPDATED_AT {
            touch(conn, &self.iri.clone());
        }

        Ok(())
    }

    pub fn serializable_properties(&self, conn: &Connection) -> Vec<serde_json::Value> {
        use crate::owl::{Property, PropertyClassification};

        self.properties.iter().map(|(prop_iri, value)| {
            let prop = Property::get(conn, prop_iri).ok().flatten();
            let unit = prop.as_ref().and_then(|p| p.unit.clone());

            let property_type_str: &str = match prop.as_ref() {
                Some(p) => p.classification().as_str(),
                None => match value {
                    Object::Iri(_) | Object::Blank(_) => PropertyClassification::Reference.as_str(),
                    _ => PropertyClassification::Value.as_str(),
                },
            };

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
                "propertyType": property_type_str,
            });
            if let Some(unit_iri) = unit {
                entry["unit"] = serde_json::json!(unit_iri);
            }
            entry
        }).collect()
    }

    pub fn remove_property_value(
        conn: &mut Connection,
        iri: &str,
        property_iri: &str,
        value_str: &str,
        origin: &str,
    ) -> Result<Option<Object>> {
        crate::owl::check_system_locked(conn, iri, Some(property_iri))?;
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
                Object::DateTime(rfc3339) => rfc3339.as_str() == value_str,
            };
            if matches {
                let found = triple.object.clone();
                store::retract_triples(
                    conn, &[Triple::new(iri, property_iri, triple.object)], origin,
                )?;
                if property_iri != super::timestamps::LAST_UPDATED_AT {
                    touch(conn, iri);
                }
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    pub fn get_property_count(conn: &Connection, iri: &str, property_iri: &str) -> Result<usize> {
        let result = query::get_by_entity_predicate(conn, iri, property_iri)?;
        Ok(result.triples.len())
    }

    pub fn clear_property(
        conn: &mut Connection,
        iri: &str,
        property_iri: &str,
        origin: &str,
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, iri, Some(property_iri))?;
        let result = query::get_by_entity_predicate(conn, iri, property_iri)?;
        if !result.triples.is_empty() {
            store::retract_triples(conn, &result.triples, origin)?;
            if property_iri != super::timestamps::LAST_UPDATED_AT {
                touch(conn, iri);
            }
        }
        Ok(())
    }

    /// Add a single IRI value to a property without domain validation.
    /// Use when the caller has already verified semantic correctness and domain
    /// validation would incorrectly reject a valid subclass relationship.
    pub fn add_iri_value(
        conn: &mut Connection,
        iri: &str,
        property_iri: &str,
        value_iri: &str,
        origin: &str,
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, iri, Some(property_iri))?;
        let existing = query::get_by_entity_predicate(conn, iri, property_iri)?;
        let already_exists = existing.triples.iter()
            .any(|t| t.object.as_iri() == Some(value_iri));
        if already_exists {
            return Ok(());
        }
        let triple = Triple::new(iri, property_iri, Object::Iri(value_iri.to_string()));
        store::append_triples(conn, &[triple], origin)?;
        touch(conn, iri);
        Ok(())
    }

    /// Remove a specific IRI value from a property.
    /// Uses TX-based retraction via store::retract_triples.
    pub fn remove_iri_value(
        conn: &mut Connection,
        iri: &str,
        property_iri: &str,
        value_iri: &str,
        origin: &str,
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, iri, Some(property_iri))?;
        let result = query::get_by_entity_predicate(conn, iri, property_iri)?;
        let to_retract: Vec<Triple> = result.triples.into_iter()
            .filter(|t| t.object.as_iri() == Some(value_iri))
            .map(|t| Triple::new(t.subject.as_str(), t.predicate.as_str(), t.object.clone()))
            .collect();
        if !to_retract.is_empty() {
            store::retract_triples(conn, &to_retract, origin)?;
            touch(conn, iri);
        }
        Ok(())
    }

    pub fn get_retracted_properties(conn: &Connection, iri: &str) -> Result<Vec<Triple>> {
        query::get_retracted_by_entity(conn, iri)
            .map(|r| r.triples.into_iter().filter(|t| {
                t.predicate != "rdfs:label"
                    && t.predicate != "rdfs:comment"
                    && t.predicate != "foundation:hasIcon"
            }).collect())
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
