use super::*;

impl Individual {
    /// Find individuals of a specific class that match property constraints
    ///
    /// This uses an efficient SQL JOIN query to find all individuals matching all criteria.
    /// Can be used with one or multiple properties.
    ///
    /// Example:
    /// ```ignore
    /// // Single property
    /// let releases = Individual::find_by_class_and_properties(
    ///     conn,
    ///     "foundation:SoftwareRelease",
    ///     &[("foundation:versionNumber", "0.1.0")]
    /// )?;
    ///
    /// // Multiple properties
    /// let releases = Individual::find_by_class_and_properties(
    ///     conn,
    ///     "foundation:SoftwareRelease",
    ///     &[
    ///         ("foundation:versionNumber", "0.1.0"),
    ///         ("foundation:releaseOf", "foundation:FoundationProduct"),
    ///     ]
    /// )?;
    /// ```
    pub fn find_by_class_and_properties(
        conn: &Connection,
        class_iri: &str,
        properties: &[(&str, &str)],
    ) -> Result<Vec<String>> {
        query::find_by_class_and_properties(conn, class_iri, properties)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }

    pub fn find_by_class_with_date_range(
        conn: &Connection,
        class_iri: &str,
        from_millis: Option<i64>,
        to_millis: Option<i64>,
        include_retracted: bool,
    ) -> Result<Vec<String>> {
        query::find_entities_by_class_with_date_range(conn, class_iri, from_millis, to_millis, include_retracted)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }

    pub fn find_by_class_and_properties_with_options(
        conn: &Connection,
        class_iri: &str,
        properties: &[(&str, &str, &str)],
        include_retracted: bool,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<String>, usize)> {
        let descendant_iris = Class::get_descendant_iris(conn, class_iri)?;
        let class_iris: Vec<&str> = descendant_iris.iter().map(|s| s.as_str()).collect();
        query::find_by_class_iris_and_properties_with_options(
            conn,
            &class_iris,
            properties,
            include_retracted,
            limit,
            offset,
        ).map_err(|e| OwlError::DatabaseError(e.to_string()))
    }

    /// Returns IRIs of messages in `conversation_iri` ordered by sentAt descending (newest first).
    /// Pass `limit = usize::MAX` for no limit.
    pub fn find_messages_by_conversation(
        conn: &Connection,
        conversation_iri: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<String>> {
        query::find_message_iris_by_conversation(conn, conversation_iri, limit, offset)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Class, ClassType, Property, PropertyType, vocabulary::rdf};

    #[test]
    fn test_find_by_class_and_properties_empty_properties_returns_empty() {
        let conn = setup_test_db();
        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Task",
            &[],
        ).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_by_class_and_properties_single_filter() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TaskA", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskA", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
            Triple::new("foundation:TaskB", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskB", "foundation:hasStatus", Object::Iri("foundation:Done".to_string())),
        ], "test").unwrap();

        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Task",
            &[("foundation:hasStatus", "foundation:Active")],
        ).unwrap();

        assert_eq!(result, vec!["foundation:TaskA".to_string()]);
    }

    #[test]
    fn test_find_by_class_and_properties_multiple_filters() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TaskA", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskA", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
            Triple::new("foundation:TaskA", "foundation:priority", Object::Literal { value: "high".to_string(), datatype: None, language: None }),
            Triple::new("foundation:TaskB", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskB", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
            Triple::new("foundation:TaskB", "foundation:priority", Object::Literal { value: "low".to_string(), datatype: None, language: None }),
        ], "test").unwrap();

        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Task",
            &[
                ("foundation:hasStatus", "foundation:Active"),
                ("foundation:priority", "high"),
            ],
        ).unwrap();

        assert_eq!(result, vec!["foundation:TaskA".to_string()]);
    }

    #[test]
    fn test_find_by_class_and_properties_no_match_returns_empty() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:TaskA", rdf::TYPE, Object::Iri("foundation:Task".to_string())),
            Triple::new("foundation:TaskA", "foundation:hasStatus", Object::Iri("foundation:Active".to_string())),
        ], "test").unwrap();

        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Task",
            &[("foundation:hasStatus", "foundation:Done")],
        ).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_find_by_class_and_properties_literal_value() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:ReleaseA", rdf::TYPE, Object::Iri("foundation:Release".to_string())),
            Triple::new("foundation:ReleaseA", "foundation:versionNumber", Object::Literal { value: "1.0.0".to_string(), datatype: None, language: None }),
            Triple::new("foundation:ReleaseB", rdf::TYPE, Object::Iri("foundation:Release".to_string())),
            Triple::new("foundation:ReleaseB", "foundation:versionNumber", Object::Literal { value: "2.0.0".to_string(), datatype: None, language: None }),
        ], "test").unwrap();

        let result = Individual::find_by_class_and_properties(
            &conn,
            "foundation:Release",
            &[("foundation:versionNumber", "1.0.0")],
        ).unwrap();

        assert_eq!(result, vec!["foundation:ReleaseA".to_string()]);
    }

    #[test]
    fn test_find_by_class_and_properties_with_options_polymorphic() {
        let mut conn = setup_test_db();

        let animal_class = Class::new("foundation:Animal");
        animal_class.assert(
            &mut conn, ClassType::OwlClass, "Animal", "https://example.com/animal.svg", None, "test",
        ).unwrap();

        let dog_class = Class::new("foundation:Dog");
        dog_class.assert(
            &mut conn, ClassType::OwlClass, "Dog", "https://example.com/dog.svg",
            Some("foundation:Animal"), "test",
        ).unwrap();

        let name_prop = Property::new("foundation:animalName");
        name_prop.assert(
            &mut conn, PropertyType::DatatypeProperty, "animalName",
            None, &["foundation:Animal"], Some("xsd:string"), None, "test",
        ).unwrap();

        store::assert_triples(&mut conn, &[
            Triple { subject: "foundation:Rex".to_string(), predicate: rdf::TYPE.to_string(),
                object: Object::Iri("foundation:Dog".to_string()),
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:Rex".to_string(), predicate: "foundation:animalName".to_string(),
                object: Object::Literal { value: "Rex".to_string(),
                    datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        let (results, total) = Individual::find_by_class_and_properties_with_options(
            &conn,
            "foundation:Animal",
            &[("foundation:animalName", "Rex", "=")],
            false,
            100,
            0,
        ).unwrap();

        assert_eq!(total, 1, "Should find 1 result via polymorphic search");
        assert!(results.contains(&"foundation:Rex".to_string()), "Should include the Dog instance");
    }

    #[test]
    fn test_find_by_class_and_properties_with_options_parent_has_no_direct_instances() {
        let mut conn = setup_test_db();

        let event_class = Class::new("foundation:Event");
        event_class.assert(
            &mut conn, ClassType::OwlClass, "Event", "https://example.com/event.svg", None, "test",
        ).unwrap();

        let vacation_class = Class::new("foundation:Vacation");
        vacation_class.assert(
            &mut conn, ClassType::OwlClass, "Vacation", "https://example.com/vacation.svg",
            Some("foundation:Event"), "test",
        ).unwrap();

        let social_class = Class::new("foundation:SocialEvent");
        social_class.assert(
            &mut conn, ClassType::OwlClass, "Social Event", "https://example.com/social.svg",
            Some("foundation:Event"), "test",
        ).unwrap();

        store::assert_triples(&mut conn, &[
            Triple { subject: "foundation:HolidayVacation".to_string(), predicate: rdf::TYPE.to_string(),
                object: Object::Iri("foundation:Vacation".to_string()),
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:HolidayVacation".to_string(), predicate: "foundation:title".to_string(),
                object: Object::Literal { value: "Holiday".to_string(),
                    datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:BirthdayParty".to_string(), predicate: rdf::TYPE.to_string(),
                object: Object::Iri("foundation:SocialEvent".to_string()),
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:BirthdayParty".to_string(), predicate: "foundation:title".to_string(),
                object: Object::Literal { value: "Birthday".to_string(),
                    datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        let (results, total) = Individual::find_by_class_and_properties_with_options(
            &conn,
            "foundation:Event",
            &[("foundation:title", "Holiday", "=")],
            false,
            100,
            0,
        ).unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"foundation:HolidayVacation".to_string()));
        assert!(!results.contains(&"foundation:BirthdayParty".to_string()));
    }

    fn insert_message(conn: &mut Connection, iri: &str, conversation_iri: &str, sent_at_ms: i64) {
        store::assert_triples(conn, &[
            Triple::new(iri, rdf::TYPE, Object::Iri("foundation:AIConversationMessage".to_string())),
            Triple::new(iri, "foundation:partOfConversation", Object::Iri(conversation_iri.to_string())),
            Triple::new(iri, "foundation:sentAt", Object::DateTime(sent_at_ms)),
        ], "test").unwrap();
    }

    #[test]
    fn test_find_messages_by_conversation_empty_db() {
        let conn = setup_test_db();
        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            usize::MAX,
            0,
        ).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_messages_by_conversation_returns_messages_ordered_newest_first() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);

        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            usize::MAX,
            0,
        ).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "foundation:Msg2");
        assert_eq!(result[1], "foundation:Msg3");
        assert_eq!(result[2], "foundation:Msg1");
    }

    #[test]
    fn test_find_messages_by_conversation_respects_limit() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);

        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            2,
            0,
        ).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "foundation:Msg2");
        assert_eq!(result[1], "foundation:Msg3");
    }

    #[test]
    fn test_find_messages_by_conversation_respects_offset() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvA", 3_000);
        insert_message(&mut conn, "foundation:Msg3", "foundation:ConvA", 2_000);

        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            usize::MAX,
            1,
        ).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "foundation:Msg3");
        assert_eq!(result[1], "foundation:Msg1");
    }

    #[test]
    fn test_find_messages_by_conversation_excludes_other_conversations() {
        let mut conn = setup_test_db();
        insert_message(&mut conn, "foundation:Msg1", "foundation:ConvA", 1_000);
        insert_message(&mut conn, "foundation:Msg2", "foundation:ConvB", 3_000);

        let result = Individual::find_messages_by_conversation(
            &conn,
            "foundation:ConvA",
            usize::MAX,
            0,
        ).unwrap();

        assert_eq!(result, vec!["foundation:Msg1".to_string()]);
    }
}
