use super::*;

pub(super) async fn is_formula_property(conn: &Connection, property_iri: &str) -> Result<bool> {
    let result = query::get_by_entity_predicate(conn, property_iri, "foundation:formula").await?;
    Ok(!result.triples.is_empty())
}

impl Individual {
    /// Assert individual with required metadata (label and icon)
    /// This is the recommended way to create individuals
    pub async fn assert(
        &self,
        conn: &Connection,
        class_iri: &str,
        label: &str,
        icon: &str,
        origin: &str
    ) -> Result<()> {
        crate::owl::validate_icon(conn, icon).await?;

        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(class_iri.to_string()));
        store::assert_triples(conn, &[triple], origin).await
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

        let label_obj = Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let label_triple = Triple::new(&self.iri, rdfs::LABEL, label_obj);
        store::assert_triples(conn, &[label_triple], origin).await
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

        let (icon_pred, icon_obj) = crate::owl::icon_store_value(icon);
        let icon_triple = Triple::new(&self.iri, icon_pred, icon_obj);
        store::assert_triples(conn, &[icon_triple], origin).await
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

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
    pub async fn add_property(
        &self,
        conn: &Connection,
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
            if let Ok(true) = is_formula_property(conn, property).await {
                return Err(OwlError::ValidationError(format!(
                    "Property '{}' is calculated via a formula and cannot be set directly",
                    property
                )));
            }
        }

        let types_result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE).await?;

        if types_result.triples.is_empty() {
            return Err(OwlError::NotFound(format!("Individual {} has no rdf:type", self.iri)));
        }

        if !is_meta_property {
            let mut property_is_valid = false;

            for triple in &types_result.triples {
                if let Some(class_iri) = triple.object.as_iri() {
                    if let Ok(Some(class)) = Class::get(conn, class_iri).await {
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

        if !is_meta_property {
            Self::validate_value_type(conn, property, &values).await?;
            for value in &values {
                Self::validate_iri_exists(conn, property, value).await?;
                Self::validate_range_type(conn, property, value).await?;
                Self::validate_one_of_constraint(conn, property, value).await?;
                Self::validate_literal_datatype(property, value)?;
            }
        }

        crate::owl::cardinality::validate_property_cardinality(
            conn,
            &self.iri,
            property,
            values.len()
        ).await?;

        let triples: Vec<Triple> = values.into_iter()
            .map(|v| Triple::new(&self.iri, property, v))
            .collect();
        store::assert_triples(conn, &triples, origin).await
            .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    pub async fn serializable_properties(&self, conn: &Connection) -> Vec<serde_json::Value> {
        use crate::owl::Property;

        let mut result = Vec::new();
        for (prop_iri, value) in &self.properties {
            let unit = Property::get(conn, prop_iri).await.ok()
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
            result.push(entry);
        }
        result
    }

    pub async fn remove_property_value(
        conn: &Connection,
        iri: &str,
        property_iri: &str,
        value_str: &str,
        origin: &str,
    ) -> Result<Option<Object>> {
        let result = query::get_by_entity_predicate(conn, iri, property_iri).await?;
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
                ).await.map_err(|e| OwlError::DatabaseError(e.to_string()))?;
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// Returns the number of current (non-retracted) values for a property on an individual.
    pub async fn get_property_count(conn: &Connection, iri: &str, property_iri: &str) -> Result<usize> {
        let result = query::get_by_entity_predicate(conn, iri, property_iri).await?;
        Ok(result.triples.len())
    }

    /// Retracts all current values of `property_iri` on individual `iri`.
    pub async fn clear_property(
        conn: &Connection,
        iri: &str,
        property_iri: &str,
        origin: &str,
    ) -> Result<()> {
        let result = query::get_by_entity_predicate(conn, iri, property_iri).await?;
        if !result.triples.is_empty() {
            store::retract_triples(conn, &result.triples, origin).await
                .map_err(|e| OwlError::DatabaseError(e.to_string()))?;
        }
        Ok(())
    }

    /// Returns retracted triples for an individual, excluding metadata predicates.
    pub async fn get_retracted_properties(conn: &Connection, iri: &str) -> Result<Vec<Triple>> {
        query::get_retracted_by_entity(conn, iri).await
            .map(|r| r.triples.into_iter().filter(|t| {
                t.predicate != "rdfs:label"
                    && t.predicate != "rdfs:comment"
                    && t.predicate != "foundation:icon"
                    && t.predicate != "foundation:hasIcon"
            }).collect())
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Class, ClassType, Property, PropertyType, vocabulary::rdf};

    async fn insert_formula_triple(conn: &Connection, property_iri: &str, formula: &str) {
        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            turso::params![],
        ).await.unwrap();

        let mut stmt = conn.prepare("SELECT last_insert_rowid()").await.unwrap();
        let mut rows = stmt.query(()).await.unwrap();
        let tx_id: i64 = *rows.next().await.unwrap().unwrap().get_value(0).unwrap().as_integer().unwrap();

        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            turso::params![property_iri, formula, tx_id],
        ).await.unwrap();
    }

    #[tokio::test]
    async fn test_write_to_calculated_property_is_rejected() {
        let conn = setup_test_db().await;

        let c = Class::new("foundation:Rectangle");
        c.assert(&conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").await.unwrap();

        let width_prop = Property::new("foundation:hasWidth");
        width_prop.assert(&conn, PropertyType::DatatypeProperty, "has width", None,
            &["foundation:Rectangle"], Some("xsd:integer"), Some("unit:Meter"), "test").await.unwrap();

        let area_prop = Property::new("foundation:hasArea");
        area_prop.assert(&conn, PropertyType::DatatypeProperty, "has area", None,
            &["foundation:Rectangle"], Some("xsd:decimal"), Some("unit:SquareMeter"), "test").await.unwrap();

        let ind = Individual::new("foundation:MyRect");
        ind.assert(&conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").await.unwrap();

        insert_formula_triple(&conn, "foundation:hasArea", "{{foundation:hasWidth}} * 2").await;

        let result = ind.add_property(
            &conn,
            "foundation:hasArea",
            vec![Object::Literal { value: "100".to_string(), datatype: Some("xsd:decimal".to_string()), language: None }],
            "test",
        ).await;

        assert!(result.is_err(), "Should reject write to calculated property");
        if let Err(OwlError::ValidationError(msg)) = result {
            assert!(msg.contains("calculated via a formula"));
        } else {
            panic!("Expected ValidationError");
        }
    }

    #[tokio::test]
    async fn test_write_to_non_calculated_property_succeeds() {
        let conn = setup_test_db().await;

        let c = Class::new("foundation:Rectangle");
        c.assert(&conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").await.unwrap();

        let width_prop = Property::new("foundation:hasWidth");
        width_prop.assert(&conn, PropertyType::DatatypeProperty, "has width", None,
            &["foundation:Rectangle"], Some("xsd:integer"), Some("unit:Meter"), "test").await.unwrap();

        let ind = Individual::new("foundation:MyRect");
        ind.assert(&conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").await.unwrap();

        let result = ind.add_property(
            &conn,
            "foundation:hasWidth",
            vec![Object::Literal { value: "5".to_string(), datatype: Some("xsd:integer".to_string()), language: None }],
            "test",
        ).await;

        assert!(result.is_ok(), "Should accept write to non-calculated property");
    }

    // Regression: Bug_1773352703259 — foundation:hasIcon is an ObjectProperty but must accept
    // literal values when set to a URL (file://, https://, etc.).
    // The meta-property bypass must cover the full validation pipeline, not just formula checks.
    #[tokio::test]
    async fn test_add_property_has_icon_file_url_literal_is_accepted() {
        let conn = setup_test_db().await;

        let c = Class::new("foundation:Item");
        c.assert(&conn, ClassType::OwlClass, "Item", "https://example.com/item.svg", None, "test").await.unwrap();

        Property::new("foundation:hasIcon")
            .assert(&conn, PropertyType::ObjectProperty, "has icon", None,
                &[], Some("foundation:Icon"), None, "test")
            .await
            .unwrap();

        let ind = Individual::new("foundation:MyItem");
        ind.assert(&conn, "foundation:Item", "My Item", "https://example.com/item.svg", "test").await.unwrap();

        let result = ind.add_property(
            &conn,
            "foundation:hasIcon",
            vec![Object::Literal {
                value: "file:///path/to/icon.png".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }],
            "test",
        ).await;
        assert!(result.is_ok(), "foundation:hasIcon must accept file:// literal values: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_meta_property_bypasses_formula_protection() {
        let conn = setup_test_db().await;

        let c = Class::new("foundation:Rectangle");
        c.assert(&conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").await.unwrap();

        let ind = Individual::new("foundation:MyRect");
        ind.assert(&conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").await.unwrap();

        insert_formula_triple(&conn, "rdfs:label", "some formula").await;

        let result = ind.add_property(
            &conn,
            "rdfs:label",
            vec![Object::Literal { value: "Updated Label".to_string(), datatype: Some("xsd:string".to_string()), language: None }],
            "test",
        ).await;

        assert!(result.is_ok(), "Meta properties should bypass formula protection");
    }

    #[tokio::test]
    async fn test_calculated_property_error_message_is_descriptive() {
        let conn = setup_test_db().await;

        let c = Class::new("foundation:Rectangle");
        c.assert(&conn, ClassType::OwlClass, "Rectangle", "https://example.com/rect.svg", None, "test").await.unwrap();

        let area_prop = Property::new("foundation:hasArea");
        area_prop.assert(&conn, PropertyType::DatatypeProperty, "has area", None,
            &["foundation:Rectangle"], Some("xsd:decimal"), Some("unit:SquareMeter"), "test").await.unwrap();

        let ind = Individual::new("foundation:MyRect");
        ind.assert(&conn, "foundation:Rectangle", "My Rect", "https://example.com/rect.svg", "test").await.unwrap();

        insert_formula_triple(&conn, "foundation:hasArea", "{{foundation:hasWidth}} * 2").await;

        let result = ind.add_property(
            &conn,
            "foundation:hasArea",
            vec![Object::Literal { value: "100".to_string(), datatype: Some("xsd:decimal".to_string()), language: None }],
            "test",
        ).await;

        if let Err(OwlError::ValidationError(msg)) = result {
            assert!(msg.contains("foundation:hasArea"), "Error should contain the property IRI");
            assert!(msg.contains("calculated via a formula"), "Error should mention formula");
            assert!(msg.contains("cannot be set directly"), "Error should say cannot be set directly");
        } else {
            panic!("Expected ValidationError");
        }
    }

    #[tokio::test]
    async fn test_serializable_properties_integer() {
        let conn = setup_test_db().await;

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

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props.len(), 1);
        assert_eq!(props[0]["property"], "foundation:age");
        assert_eq!(props[0]["value"], 30);
    }

    #[tokio::test]
    async fn test_serializable_properties_number() {
        let conn = setup_test_db().await;

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:score".to_string(), Object::Number(9.5))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props[0]["value"], 9.5);
    }

    #[tokio::test]
    async fn test_serializable_properties_boolean() {
        let conn = setup_test_db().await;

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:active".to_string(), Object::Boolean(true))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props[0]["value"], true);
    }

    #[tokio::test]
    async fn test_serializable_properties_string_literal() {
        let conn = setup_test_db().await;

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

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props[0]["value"], "Alice");
    }

    #[tokio::test]
    async fn test_serializable_properties_decimal_literal_parsed_as_number() {
        let conn = setup_test_db().await;

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

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props[0]["value"], 3.14);
    }

    #[tokio::test]
    async fn test_serializable_properties_integer_literal_parsed_as_number() {
        let conn = setup_test_db().await;

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

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props[0]["value"], 99);
    }

    #[tokio::test]
    async fn test_serializable_properties_iri_value() {
        let conn = setup_test_db().await;

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:knows".to_string(), Object::Iri("foundation:Bob".to_string()))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props[0]["value"], "foundation:Bob");
    }

    #[tokio::test]
    async fn test_serializable_properties_includes_unit_when_property_has_one() {
        let conn = setup_test_db().await;

        Property::new("foundation:height").assert(
            &conn,
            PropertyType::DatatypeProperty,
            "height",
            None,
            &[],
            Some("xsd:decimal"),
            Some("unit:Meter"),
            "test",
        ).await.unwrap();

        let ind = Individual {
            iri: "foundation:Alice".to_string(),
            label: None, icon: None, comment: None, types: vec![],
            properties: vec![("foundation:height".to_string(), Object::Number(1.75))],
            property_tx: vec![0],
            backlinks: vec![],
        };

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props[0]["unit"], "unit:Meter");
    }

    #[tokio::test]
    async fn test_serializable_properties_no_unit_key_when_property_has_none() {
        let conn = setup_test_db().await;

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

        let props = ind.serializable_properties(&conn).await;
        assert!(props[0].get("unit").is_none(), "No unit key when property has no unit");
    }

    #[tokio::test]
    async fn test_serializable_properties_multiple() {
        let conn = setup_test_db().await;

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

        let props = ind.serializable_properties(&conn).await;
        assert_eq!(props.len(), 3);
    }

    #[tokio::test]
    async fn test_remove_property_value_iri_happy_path() {
        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", rdf::TYPE, Object::Iri("foundation:Person".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        ], "test").await.unwrap();

        let result = Individual::remove_property_value(
            &conn,
            "foundation:Alice",
            "foundation:knows",
            "foundation:Bob",
            "test",
        ).await.unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), Object::Iri("foundation:Bob".to_string()));

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert!(after.triples.is_empty(), "Triple should have been retracted");
    }

    #[tokio::test]
    async fn test_remove_property_value_integer() {
        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:age", Object::Integer(30)),
        ], "test").await.unwrap();

        let result = Individual::remove_property_value(
            &conn,
            "foundation:Alice",
            "foundation:age",
            "30",
            "test",
        ).await.unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), Object::Integer(30));

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:age").await.unwrap();
        assert!(after.triples.is_empty(), "Integer triple should have been retracted");
    }

    #[tokio::test]
    async fn test_remove_property_value_string_literal() {
        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:nickname", Object::Literal {
                value: "Ally".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").await.unwrap();

        let result = Individual::remove_property_value(
            &conn,
            "foundation:Alice",
            "foundation:nickname",
            "Ally",
            "test",
        ).await.unwrap();

        assert!(result.is_some());

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:nickname").await.unwrap();
        assert!(after.triples.is_empty(), "String literal triple should have been retracted");
    }

    #[tokio::test]
    async fn test_remove_property_value_nonexistent_value_returns_none() {
        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        ], "test").await.unwrap();

        let result = Individual::remove_property_value(
            &conn,
            "foundation:Alice",
            "foundation:knows",
            "foundation:Charlie",
            "test",
        ).await.unwrap();

        assert!(result.is_none(), "Should return None when value does not match");

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert_eq!(after.triples.len(), 1, "Existing triple should be untouched");
    }

    #[tokio::test]
    async fn test_remove_property_value_no_triples_returns_none() {
        let conn = setup_test_db().await;

        let result = Individual::remove_property_value(
            &conn,
            "foundation:Alice",
            "foundation:knows",
            "foundation:Bob",
            "test",
        ).await.unwrap();

        assert!(result.is_none(), "Should return None when property has no triples");
    }

    #[tokio::test]
    async fn test_remove_property_value_boolean() {
        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:active", Object::Boolean(true)),
        ], "test").await.unwrap();

        let result = Individual::remove_property_value(
            &conn,
            "foundation:Alice",
            "foundation:active",
            "true",
            "test",
        ).await.unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), Object::Boolean(true));
    }

    #[tokio::test]
    async fn test_remove_property_value_number() {
        let conn = setup_test_db().await;

        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:score", Object::Number(9.5)),
        ], "test").await.unwrap();

        let result = Individual::remove_property_value(
            &conn,
            "foundation:Alice",
            "foundation:score",
            "9.5",
            "test",
        ).await.unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), Object::Number(9.5));
    }

    #[tokio::test]
    async fn test_remove_property_value_only_removes_matching_multivalue() {
        let conn = setup_test_db().await;

        store::append_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
        ], "test").await.unwrap();

        let result = Individual::remove_property_value(
            &conn,
            "foundation:Alice",
            "foundation:knows",
            "foundation:Bob",
            "test",
        ).await.unwrap();

        assert!(result.is_some());

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert_eq!(after.triples.len(), 1, "Only the matching value should be removed");
        assert_eq!(
            after.triples[0].object,
            Object::Iri("foundation:Carol".to_string()),
        );
    }

    #[tokio::test]
    async fn test_get_property_count_returns_zero_when_no_values() {
        let conn = setup_test_db().await;
        let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_get_property_count_returns_one_for_single_value() {
        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        ], "test").await.unwrap();

        let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_get_property_count_returns_correct_count_for_multiple_values() {
        let conn = setup_test_db().await;
        store::append_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Dave".to_string())),
        ], "test").await.unwrap();

        let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_get_property_count_excludes_retracted_values() {
        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
        ], "test").await.unwrap();
        Individual::remove_property_value(&conn, "foundation:Alice", "foundation:knows", "foundation:Bob", "test").await.unwrap();

        let count = Individual::get_property_count(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_clear_property_removes_all_values() {
        let conn = setup_test_db().await;
        store::append_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Carol".to_string())),
        ], "test").await.unwrap();

        Individual::clear_property(&conn, "foundation:Alice", "foundation:knows", "test").await.unwrap();

        let after = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert!(after.triples.is_empty(), "All values should have been retracted");
    }

    #[tokio::test]
    async fn test_clear_property_is_noop_when_no_values() {
        let conn = setup_test_db().await;
        let result = Individual::clear_property(&conn, "foundation:Alice", "foundation:knows", "test").await;
        assert!(result.is_ok(), "clear_property on empty property should not error");
    }

    #[tokio::test]
    async fn test_clear_property_preserves_other_properties() {
        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:knows", Object::Iri("foundation:Bob".to_string())),
            Triple::new("foundation:Alice", "foundation:name", Object::Literal {
                value: "Alice".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").await.unwrap();

        Individual::clear_property(&conn, "foundation:Alice", "foundation:knows", "test").await.unwrap();

        let knows = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:knows").await.unwrap();
        assert!(knows.triples.is_empty(), "foundation:knows should be cleared");

        let name = query::get_by_entity_predicate(&conn, "foundation:Alice", "foundation:name").await.unwrap();
        assert_eq!(name.triples.len(), 1, "foundation:name must not be affected");
    }

    #[tokio::test]
    async fn test_get_retracted_properties_empty_when_nothing_retracted() {
        let conn = setup_test_db().await;
        let result = Individual::get_retracted_properties(&conn, "foundation:Alice").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_get_retracted_properties_returns_retracted_triples() {
        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
            Triple::new("foundation:Alice", "foundation:score", Object::Integer(42)),
        ], "test").await.unwrap();
        Individual::clear_property(&conn, "foundation:Alice", "foundation:score", "test").await.unwrap();

        let result = Individual::get_retracted_properties(&conn, "foundation:Alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].predicate, "foundation:score");
    }

    #[tokio::test]
    async fn test_get_retracted_properties_filters_metadata_predicates() {
        let conn = setup_test_db().await;
        store::assert_triples(&conn, &[
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
        ], "test").await.unwrap();

        Individual::clear_property(&conn, "foundation:Alice", "rdfs:label", "test").await.unwrap();
        Individual::clear_property(&conn, "foundation:Alice", "rdfs:comment", "test").await.unwrap();
        Individual::clear_property(&conn, "foundation:Alice", "foundation:icon", "test").await.unwrap();
        Individual::clear_property(&conn, "foundation:Alice", "foundation:score", "test").await.unwrap();

        let result = Individual::get_retracted_properties(&conn, "foundation:Alice").await.unwrap();
        let predicates: Vec<&str> = result.iter().map(|t| t.predicate.as_str()).collect();
        assert!(!predicates.contains(&"rdfs:label"), "rdfs:label must be filtered");
        assert!(!predicates.contains(&"rdfs:comment"), "rdfs:comment must be filtered");
        assert!(!predicates.contains(&"foundation:icon"), "foundation:icon must be filtered");
        assert!(predicates.contains(&"foundation:score"), "foundation:score must be included");
    }
}
