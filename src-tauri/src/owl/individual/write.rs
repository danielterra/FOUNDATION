use super::*;
use super::timestamps::touch;

pub(super) fn is_formula_property(conn: &Connection, property_iri: &str) -> Result<bool> {
    let result = query::get_by_entity_predicate(conn, property_iri, "foundation:formula")?;
    Ok(!result.triples.is_empty())
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
        crate::owl::check_system_locked(conn, &self.iri, Some(property))?;
        if values.is_empty() {
            return Err(OwlError::InvalidOperation(
                format!("No values provided for property {}", property)
            ));
        }

        let is_meta_property = property.starts_with("rdfs:")
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
            let property_is_valid = types_result.triples.iter()
                .filter_map(|t| t.object.as_iri())
                .any(|class_iri| Class::has_property(conn, class_iri, property));

            if !property_is_valid {
                return Err(OwlError::InvalidOperation(
                    format!(
                        "Property {} is not defined in any class of individual {}",
                        property, self.iri
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

        if property != super::timestamps::LAST_UPDATED_AT {
            touch(conn, &self.iri.clone());
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
            let property_is_valid = types_result.triples.iter()
                .filter_map(|t| t.object.as_iri())
                .any(|class_iri| Class::has_property(conn, class_iri, property));

            if !property_is_valid {
                return Err(OwlError::InvalidOperation(
                    format!(
                        "Property {} is not defined in any class of individual {}",
                        property, self.iri
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
