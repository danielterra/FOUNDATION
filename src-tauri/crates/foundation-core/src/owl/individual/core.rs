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
                Object::Literal { value, .. } =>
                    Some(crate::owl::icon_literal_to_display(value)),
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
        let t0 = std::time::Instant::now();

        let all_triples = query::get_by_entity(conn, &iri)?;
        if all_triples.triples.is_empty() {
            return Ok(None);
        }

        let label = all_triples.triples.iter()
            .find(|t| t.predicate == rdfs::LABEL)
            .and_then(|t| t.object.as_literal());

        let icon = query::get_by_entity_predicate(conn, &iri, "foundation:hasIcon")
            .ok()
            .and_then(|r| r.triples.into_iter().next())
            .and_then(|t| match t.object {
                Object::Iri(iri) => crate::owl::icon_iri_to_display(conn, &iri),
                Object::Literal { value, .. } =>
                    Some(crate::owl::icon_literal_to_display(&value)),
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

        let elapsed = t0.elapsed().as_millis();
        if elapsed > 30 {
            crate::diagnostics::log_backend("debug", &format!(
                "[OWL] Individual::get({}) props={} backlinks={} {}ms",
                iri, properties.len(), backlinks.len(), elapsed
            ));
        }

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

    /// Retract all triples for the given entity IRI, including references to it from other entities.
    ///
    /// Cascade rules are configured on the CLASS, not the property:
    /// - `foundation:cascadeDeleteDomain propIRI` — when this class is retracted, also retract all
    ///   subjects that hold `(subject, propIRI, this_iri)` (children reference the parent)
    /// - `foundation:cascadeDeleteRange propIRI` — when this class is retracted, also retract all
    ///   IRIs that this entity references via `(this_iri, propIRI, target)` (parent references children)
    pub fn retract(conn: &mut Connection, iri: &str, origin: &str) -> Result<()> {
        let mut summary = Vec::new();
        let mut retracted = Vec::new();
        Self::retract_inner(conn, iri, origin, &mut summary, &mut retracted)
    }

    /// Retract an individual and return a per-rule cascade summary: `(property_iri, direction, count)`.
    pub fn retract_with_summary(
        conn: &mut Connection,
        iri: &str,
        origin: &str,
    ) -> Result<Vec<(String, String, usize)>> {
        let mut summary: Vec<(String, String, usize)> = Vec::new();
        let mut retracted = Vec::new();
        Self::retract_inner(conn, iri, origin, &mut summary, &mut retracted)?;
        Ok(summary)
    }

    /// Retract an individual and return all IRIs that were retracted (root + cascade children).
    pub fn retract_collecting(
        conn: &mut Connection,
        iri: &str,
        origin: &str,
    ) -> Result<Vec<String>> {
        let mut summary = Vec::new();
        let mut retracted = Vec::new();
        Self::retract_inner(conn, iri, origin, &mut summary, &mut retracted)?;
        Ok(retracted)
    }

    /// Retract an individual and return both all retracted IRIs and the cascade summary.
    pub fn retract_collecting_with_summary(
        conn: &mut Connection,
        iri: &str,
        origin: &str,
    ) -> Result<(Vec<String>, Vec<(String, String, usize)>)> {
        let mut summary = Vec::new();
        let mut retracted = Vec::new();
        Self::retract_inner(conn, iri, origin, &mut summary, &mut retracted)?;
        Ok((retracted, summary))
    }

    fn retract_inner(
        conn: &mut Connection,
        iri: &str,
        origin: &str,
        summary: &mut Vec<(String, String, usize)>,
        retracted: &mut Vec<String>,
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, iri, None)?;
        retracted.push(iri.to_string());

        let type_iris: Vec<String> = query::get_by_entity_predicate(conn, iri, rdf::TYPE)
            .map(|r| r.triples.into_iter()
                .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
                .collect())
            .unwrap_or_default();

        for class_iri in &type_iris {
            let domain_props: Vec<String> =
                query::get_by_entity_predicate(conn, class_iri, "foundation:cascadeDeleteDomain")
                    .map(|r| r.triples.into_iter()
                        .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
                        .collect())
                    .unwrap_or_default();

            for prop in &domain_props {
                let children: Vec<String> =
                    query::get_by_predicate_object(conn, prop, iri)
                        .map(|r| r.triples.into_iter().map(|t| t.subject).collect())
                        .unwrap_or_default();
                let count = children.len();
                for child in children {
                    Self::retract_inner(conn, &child, origin, summary, retracted)?;
                }
                if count > 0 {
                    summary.push((prop.clone(), "domain".to_string(), count));
                }
            }

            let range_props: Vec<String> =
                query::get_by_entity_predicate(conn, class_iri, "foundation:cascadeDeleteRange")
                    .map(|r| r.triples.into_iter()
                        .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
                        .collect())
                    .unwrap_or_default();

            for prop in &range_props {
                let targets: Vec<String> =
                    query::get_by_entity_predicate(conn, iri, prop)
                        .map(|r| r.triples.into_iter()
                            .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
                            .collect())
                        .unwrap_or_default();
                let count = targets.len();
                for target in targets {
                    Self::retract_inner(conn, &target, origin, summary, retracted)?;
                }
                if count > 0 {
                    summary.push((prop.clone(), "range".to_string(), count));
                }
            }
        }

        let mut triples = query::get_by_entity(conn, iri)?.triples;
        triples.extend(query::get_by_object_iri(conn, iri)?.triples);
        if !triples.is_empty() {
            store::retract_triples(conn, &triples, origin)?;
        }
        Ok(())
    }

    /// Compute the cascade delete impact without performing any writes.
    /// Returns `(cascade_items, backlink_count)` where:
    /// - `cascade_items`: list of `(iri, label, type_label)` for each individual cascade-retracted
    /// - `backlink_count`: triples from other entities pointing to this IRI (references cleaned up,
    ///   those entities are NOT deleted)
    pub fn compute_delete_impact(conn: &Connection, iri: &str) -> Result<(Vec<(String, String, String)>, usize)> {
        let mut visited = std::collections::HashSet::new();
        let mut cascade_items: Vec<(String, String, String)> = Vec::new();
        compute_impact_inner(conn, iri, &mut visited, &mut cascade_items)?;
        let backlink_count = query::get_by_object_iri(conn, iri)?.triples.len();
        Ok((cascade_items, backlink_count))
    }

    pub fn restore(conn: &mut Connection, iri: &str, origin: &str) -> Result<()> {
        let retract_tx = query::get_retraction_tx(conn, iri)?
            .ok_or_else(|| OwlError::NotFound(
                format!("Individual '{}' has no retracted triples to restore", iri)
            ))?;

        let last_active = query::get_last_active_by_entity_before_tx(conn, iri, retract_tx)?;
        if last_active.triples.is_empty() {
            return Err(OwlError::NotFound(
                format!("No active triples found for '{}' before retraction tx {}", iri, retract_tx)
            ));
        }

        let triples: Vec<Triple> = last_active.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();

        store::assert_triples(conn, &triples, origin)?;
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

fn entity_label_and_type(conn: &Connection, iri: &str) -> (String, String) {
    let label = query::get_by_entity_predicate(conn, iri, rdfs::LABEL)
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_literal().map(|s| s.to_string()))
        .unwrap_or_else(|| iri.to_string());
    let type_label = query::get_by_entity_predicate(conn, iri, rdf::TYPE)
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_iri().map(|s| s.to_string()))
        .and_then(|class_iri| {
            query::get_by_entity_predicate(conn, &class_iri, rdfs::LABEL)
                .ok()
                .and_then(|r| r.triples.into_iter().next())
                .and_then(|t| t.object.as_literal().map(|s| s.to_string()))
        })
        .unwrap_or_default();
    (label, type_label)
}

fn compute_impact_inner(
    conn: &Connection,
    iri: &str,
    visited: &mut std::collections::HashSet<String>,
    cascade_items: &mut Vec<(String, String, String)>,
) -> Result<()> {
    if !visited.insert(iri.to_string()) {
        return Ok(());
    }

    let type_iris: Vec<String> = query::get_by_entity_predicate(conn, iri, rdf::TYPE)
        .map(|r| r.triples.into_iter()
            .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
            .collect())
        .unwrap_or_default();

    for class_iri in &type_iris {
        let domain_props: Vec<String> =
            query::get_by_entity_predicate(conn, class_iri, "foundation:cascadeDeleteDomain")
                .map(|r| r.triples.into_iter()
                    .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
                    .collect())
                .unwrap_or_default();

        for prop in &domain_props {
            let children: Vec<String> =
                query::get_by_predicate_object(conn, prop, iri)
                    .map(|r| r.triples.into_iter().map(|t| t.subject).collect())
                    .unwrap_or_default();
            for child in children {
                let (label, type_label) = entity_label_and_type(conn, &child);
                cascade_items.push((child.clone(), label, type_label));
                compute_impact_inner(conn, &child, visited, cascade_items)?;
            }
        }

        let range_props: Vec<String> =
            query::get_by_entity_predicate(conn, class_iri, "foundation:cascadeDeleteRange")
                .map(|r| r.triples.into_iter()
                    .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
                    .collect())
                .unwrap_or_default();

        for prop in &range_props {
            let targets: Vec<String> =
                query::get_by_entity_predicate(conn, iri, prop)
                    .map(|r| r.triples.into_iter()
                        .filter_map(|t| t.object.as_iri().map(|s| s.to_string()))
                        .collect())
                    .unwrap_or_default();
            for target in targets {
                let (label, type_label) = entity_label_and_type(conn, &target);
                cascade_items.push((target.clone(), label, type_label));
                compute_impact_inner(conn, &target, visited, cascade_items)?;
            }
        }
    }

    Ok(())
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

    #[test]
    fn test_retract_cascades_domain_children() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Conv1", rdf::TYPE, Object::Iri("foundation:AIConversation".to_string())),
            Triple::new("foundation:Msg1", rdf::TYPE, Object::Iri("foundation:AIConversationMessage".to_string())),
            Triple::new("foundation:Msg1", "foundation:partOfConversation", Object::Iri("foundation:Conv1".to_string())),
            Triple::new("foundation:AIConversation", "foundation:cascadeDeleteDomain",
                Object::Iri("foundation:partOfConversation".to_string())),
        ], "test").unwrap();

        Individual::retract(&mut conn, "foundation:Conv1", "test").unwrap();

        let conv = Individual::get(&conn, "foundation:Conv1").unwrap();
        assert!(conv.is_none(), "Conversation must be retracted");

        let msg = Individual::get(&conn, "foundation:Msg1").unwrap();
        assert!(msg.is_none(), "Message must be cascade-retracted via cascadeDeleteDomain");
    }

    #[test]
    fn test_retract_cascades_range_targets() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Parent1", rdf::TYPE, Object::Iri("foundation:Container".to_string())),
            Triple::new("foundation:Child1", rdf::TYPE, Object::Iri("foundation:Item".to_string())),
            Triple::new("foundation:Parent1", "foundation:hasChild", Object::Iri("foundation:Child1".to_string())),
            Triple::new("foundation:Container", "foundation:cascadeDeleteRange",
                Object::Iri("foundation:hasChild".to_string())),
        ], "test").unwrap();

        Individual::retract(&mut conn, "foundation:Parent1", "test").unwrap();

        let parent = Individual::get(&conn, "foundation:Parent1").unwrap();
        assert!(parent.is_none(), "Parent must be retracted");

        let child = Individual::get(&conn, "foundation:Child1").unwrap();
        assert!(child.is_none(), "Child must be cascade-retracted via cascadeDeleteRange");
    }

    #[test]
    fn test_compute_delete_impact_zero_for_isolated_entity() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Orphan", rdf::TYPE, Object::Iri("foundation:Thing".to_string())),
        ], "test").unwrap();

        let (items, backlinks) = Individual::compute_delete_impact(&conn, "foundation:Orphan").unwrap();
        assert_eq!(items.len(), 0, "No cascade rules → cascade count must be 0");
        assert_eq!(backlinks, 0, "No references to this entity → backlink count must be 0");
    }

    #[test]
    fn test_compute_delete_impact_cascade_domain() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Parent", rdf::TYPE, Object::Iri("foundation:Container".to_string())),
            Triple::new("foundation:Child1", rdf::TYPE, Object::Iri("foundation:Item".to_string())),
            Triple::new("foundation:Child2", rdf::TYPE, Object::Iri("foundation:Item".to_string())),
            Triple::new("foundation:Child1", "foundation:partOf", Object::Iri("foundation:Parent".to_string())),
            Triple::new("foundation:Child2", "foundation:partOf", Object::Iri("foundation:Parent".to_string())),
            Triple::new("foundation:Container", "foundation:cascadeDeleteDomain",
                Object::Iri("foundation:partOf".to_string())),
        ], "test").unwrap();

        let (items, _) = Individual::compute_delete_impact(&conn, "foundation:Parent").unwrap();
        assert_eq!(items.len(), 2, "Two children via cascadeDeleteDomain → cascade count must be 2");
    }

    #[test]
    fn test_compute_delete_impact_cascade_range() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Parent", rdf::TYPE, Object::Iri("foundation:Container".to_string())),
            Triple::new("foundation:Child1", rdf::TYPE, Object::Iri("foundation:Item".to_string())),
            Triple::new("foundation:Child2", rdf::TYPE, Object::Iri("foundation:Item".to_string())),
            Triple::new("foundation:Parent", "foundation:hasChild", Object::Iri("foundation:Child1".to_string())),
            Triple::new("foundation:Parent", "foundation:hasChild", Object::Iri("foundation:Child2".to_string())),
            Triple::new("foundation:Container", "foundation:cascadeDeleteRange",
                Object::Iri("foundation:hasChild".to_string())),
        ], "test").unwrap();

        let (items, _) = Individual::compute_delete_impact(&conn, "foundation:Parent").unwrap();
        assert_eq!(items.len(), 2, "Two targets via cascadeDeleteRange → cascade count must be 2");
    }

    #[test]
    fn test_compute_delete_impact_backlinks_no_cascade() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Target", rdf::TYPE, Object::Iri("foundation:TypeA".to_string())),
            Triple::new("foundation:Ref1", "foundation:linksTo", Object::Iri("foundation:Target".to_string())),
            Triple::new("foundation:Ref2", "foundation:linksTo", Object::Iri("foundation:Target".to_string())),
            Triple::new("foundation:Ref3", "foundation:linksTo", Object::Iri("foundation:Target".to_string())),
        ], "test").unwrap();

        let (items, backlinks) = Individual::compute_delete_impact(&conn, "foundation:Target").unwrap();
        assert_eq!(items.len(), 0, "TypeA has no cascade rules → cascade count must be 0");
        assert_eq!(backlinks, 3, "Three entities reference this target → backlink count must be 3");
    }

    #[test]
    fn test_compute_delete_impact_transitive_cascade() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Grandparent", rdf::TYPE, Object::Iri("foundation:Level0".to_string())),
            Triple::new("foundation:Parent", rdf::TYPE, Object::Iri("foundation:Level1".to_string())),
            Triple::new("foundation:Child", rdf::TYPE, Object::Iri("foundation:Level2".to_string())),
            Triple::new("foundation:Parent", "foundation:partOf", Object::Iri("foundation:Grandparent".to_string())),
            Triple::new("foundation:Child", "foundation:partOf", Object::Iri("foundation:Parent".to_string())),
            Triple::new("foundation:Level0", "foundation:cascadeDeleteDomain",
                Object::Iri("foundation:partOf".to_string())),
            Triple::new("foundation:Level1", "foundation:cascadeDeleteDomain",
                Object::Iri("foundation:partOf".to_string())),
        ], "test").unwrap();

        let (items, _) = Individual::compute_delete_impact(&conn, "foundation:Grandparent").unwrap();
        assert_eq!(items.len(), 2, "Parent + Child via transitive cascade → cascade count must be 2");
    }

    #[test]
    fn test_retract_without_cascade_rules_leaves_references_intact() {
        let mut conn = setup_test_db();

        store::assert_triples(&mut conn, &[
            Triple::new("foundation:EntityA", rdf::TYPE, Object::Iri("foundation:TypeA".to_string())),
            Triple::new("foundation:EntityB", rdf::TYPE, Object::Iri("foundation:TypeB".to_string())),
            Triple::new("foundation:EntityB", "foundation:linksTo", Object::Iri("foundation:EntityA".to_string())),
        ], "test").unwrap();

        Individual::retract(&mut conn, "foundation:EntityA", "test").unwrap();

        let entity_b = Individual::get(&conn, "foundation:EntityB").unwrap();
        assert!(entity_b.is_some(), "EntityB must survive when TypeA has no cascade rules");
    }

    // ── Individual::get ──────────────────────────────────────────────────────

    #[test]
    fn test_get_existing_with_label_and_type() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("test:Alice", rdf::TYPE, Object::Iri("test:Person".to_string())),
            Triple::new("test:Alice", "rdfs:label", Object::Literal {
                value: "Alice".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let ind = Individual::get(&conn, "test:Alice").unwrap();
        assert!(ind.is_some(), "existing individual must be returned");
        let ind = ind.unwrap();
        assert_eq!(ind.iri, "test:Alice");
        assert_eq!(ind.label, Some("Alice".to_string()));
        assert!(ind.types.iter().any(|t| t.iri == "test:Person"));
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let conn = setup_test_db();
        let result = Individual::get(&conn, "test:Nonexistent").unwrap();
        assert!(result.is_none(), "non-existent IRI must return None");
    }

    #[test]
    fn test_get_retracted_returns_none() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("test:Bob", rdf::TYPE, Object::Iri("test:Person".to_string())),
        ], "test").unwrap();
        Individual::retract(&mut conn, "test:Bob", "test").unwrap();

        let result = Individual::get(&conn, "test:Bob").unwrap();
        assert!(result.is_none(), "retracted individual must return None from get");
    }

    #[test]
    fn test_get_multi_value_property() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("test:Doc", rdf::TYPE, Object::Iri("test:Document".to_string())),
            Triple::new("test:Doc", "test:tag", Object::Literal {
                value: "alpha".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("test:Doc", "test:tag", Object::Literal {
                value: "beta".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let ind = Individual::get(&conn, "test:Doc").unwrap().unwrap();
        let tag_count = ind.properties.iter()
            .filter(|(pred, _)| pred == "test:tag")
            .count();
        assert_eq!(tag_count, 2, "both tag values must be loaded");
    }

    #[test]
    fn test_get_includes_backlinks() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("test:Project", rdf::TYPE, Object::Iri("test:Project".to_string())),
            Triple::new("test:Task", rdf::TYPE, Object::Iri("test:Task".to_string())),
            Triple::new("test:Task", "test:partOf", Object::Iri("test:Project".to_string())),
        ], "test").unwrap();

        let ind = Individual::get(&conn, "test:Project").unwrap().unwrap();
        assert!(
            ind.backlinks.iter().any(|b| b.subject == "test:Task"),
            "backlinks must include test:Task which references test:Project"
        );
    }

    // ── Individual::restore ───────────────────────────────────────────────────

    #[test]
    fn test_restore_retracted_individual_comes_back() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("test:Ghost", rdf::TYPE, Object::Iri("test:Ghost".to_string())),
            Triple::new("test:Ghost", "rdfs:label", Object::Literal {
                value: "Ghost".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();
        Individual::retract(&mut conn, "test:Ghost", "test").unwrap();
        assert!(Individual::get(&conn, "test:Ghost").unwrap().is_none(), "pre-condition: must be retracted");

        Individual::restore(&mut conn, "test:Ghost", "test").unwrap();

        let restored = Individual::get(&conn, "test:Ghost").unwrap();
        assert!(restored.is_some(), "after restore the individual must be visible again");
        let restored = restored.unwrap();
        assert_eq!(restored.label, Some("Ghost".to_string()));
    }

    // ── Individual::search ────────────────────────────────────────────────────

    #[test]
    fn test_search_returns_domain_individuals() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("test:Alice", rdf::TYPE, Object::Iri("test:Person".to_string())),
            Triple::new("test:Bob", rdf::TYPE, Object::Iri("test:Person".to_string())),
        ], "test").unwrap();

        let results = Individual::search(&conn).unwrap();
        assert!(results.contains(&"test:Alice".to_string()));
        assert!(results.contains(&"test:Bob".to_string()));
    }

    #[test]
    fn test_search_empty_when_no_individuals() {
        let conn = setup_test_db();
        let results = Individual::search(&conn).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_excludes_owl_typed_entities() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("test:MyClass", rdf::TYPE, Object::Iri("owl:Class".to_string())),
            Triple::new("test:MyProp", rdf::TYPE, Object::Iri("owl:ObjectProperty".to_string())),
            Triple::new("test:RealIndividual", rdf::TYPE, Object::Iri("test:SomeDomainClass".to_string())),
        ], "test").unwrap();

        let results = Individual::search(&conn).unwrap();
        assert!(!results.contains(&"test:MyClass".to_string()), "owl:Class-typed entity must be excluded");
        assert!(!results.contains(&"test:MyProp".to_string()), "owl:ObjectProperty-typed entity must be excluded");
        assert!(results.contains(&"test:RealIndividual".to_string()));
    }

    #[test]
    fn test_search_excludes_retracted_individuals() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("test:Active", rdf::TYPE, Object::Iri("test:Item".to_string())),
            Triple::new("test:Deleted", rdf::TYPE, Object::Iri("test:Item".to_string())),
        ], "test").unwrap();
        Individual::retract(&mut conn, "test:Deleted", "test").unwrap();

        let results = Individual::search(&conn).unwrap();
        assert!(results.contains(&"test:Active".to_string()));
        assert!(!results.contains(&"test:Deleted".to_string()), "retracted individual must not appear in search");
    }
}
