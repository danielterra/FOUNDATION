use super::*;

impl Individual {
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
            .and_then(|t| match &t.object {
                Object::Iri(iri) => crate::owl::icon_iri_to_display(conn, iri),
                Object::Literal { value, .. } => Some(value.clone()),
                _ => None,
            });

        let comment = retracted.triples.iter()
            .find(|t| t.predicate == rdfs::COMMENT)
            .and_then(|t| t.object.as_literal());

        let prop_triples: Vec<_> = retracted.triples.into_iter()
            .filter(|t| {
                t.predicate != rdfs::LABEL
                    && t.predicate != rdfs::COMMENT
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
            .and_then(|t| match &t.object {
                Object::Iri(iri) => crate::owl::icon_iri_to_display(conn, iri),
                Object::Literal { value, .. } => Some(value.clone()),
                _ => None,
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

    /// Retract all triples for the given entity IRI, including references to it from other entities
    pub fn retract(conn: &mut Connection, iri: &str, origin: &str) -> Result<()> {
        crate::owl::check_system_locked(conn, iri, None)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::vocabulary::rdf;

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
