use crate::owl::{get_all_iri_properties, Connection, Object, Result, Triple};

pub fn ensure_setup_complete(conn: &mut Connection) -> Result<bool> {
    if is_setup_complete(conn)? {
        return Ok(true);
    }

    let user_types = get_all_iri_properties(conn, "foundation:ThisUser", "rdf:type")?;
    if !user_types.contains(&"foundation:Person".to_string()) {
        return Ok(false);
    }

    let computer_users =
        get_all_iri_properties(conn, "foundation:ThisComputer", "foundation:hasUser")?;
    if !computer_users.contains(&"foundation:ThisUser".to_string()) {
        return Ok(false);
    }

    let app_types = get_all_iri_properties(conn, "foundation:ThisFoundationInstance", "rdf:type")?;
    if !app_types.is_empty() {
        return Ok(false);
    }

    crate::owl::assert_raw_triples_with_tx(
        conn,
        &[
            Triple::new(
                "foundation:ThisFoundationInstance",
                "rdf:type",
                Object::Iri("foundation:Application".to_string()),
            ),
            Triple::new(
                "foundation:ThisFoundationInstance",
                "rdfs:label",
                Object::Literal {
                    value: "FOUNDATION".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            ),
        ],
        "setup_recovery",
    )?;

    is_setup_complete(conn)
}

pub fn is_setup_complete(conn: &Connection) -> Result<bool> {
    let user_types = get_all_iri_properties(conn, "foundation:ThisUser", "rdf:type")?;
    if !user_types.contains(&"foundation:Person".to_string()) {
        return Ok(false);
    }

    let app_types = get_all_iri_properties(conn, "foundation:ThisFoundationInstance", "rdf:type")?;
    if !app_types.contains(&"foundation:Application".to_string()) {
        return Ok(false);
    }

    let computer_users =
        get_all_iri_properties(conn, "foundation:ThisComputer", "foundation:hasUser")?;
    Ok(computer_users.contains(&"foundation:ThisUser".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> (tempfile::TempDir, Connection) {
        let directory = tempfile::TempDir::new().unwrap();
        let database_path = directory.path().join("setup.sqlite");
        let conn = crate::owl::initialize_db(&database_path).unwrap();
        (directory, conn)
    }

    fn setup_test_db_without_application_marker() -> (tempfile::TempDir, Connection) {
        let (directory, mut conn) = setup_test_db();
        crate::owl::retract_raw_triples(
            &mut conn,
            &[Triple::new(
                "foundation:ThisFoundationInstance",
                "rdf:type",
                Object::Iri("foundation:Application".to_string()),
            )],
            "test",
        )
        .unwrap();
        (directory, conn)
    }

    fn assert_profile_triples(conn: &mut Connection, triples: &[Triple]) {
        crate::owl::assert_raw_triples_with_tx(conn, triples, "test").unwrap();
    }

    fn initialized_profile_without_application() -> (tempfile::TempDir, Connection) {
        let (directory, mut conn) = setup_test_db_without_application_marker();
        assert_profile_triples(
            &mut conn,
            &[
                Triple::new(
                    "foundation:ThisUser",
                    "rdf:type",
                    Object::Iri("foundation:Person".to_string()),
                ),
                Triple::new(
                    "foundation:ThisComputer",
                    "foundation:hasUser",
                    Object::Iri("foundation:ThisUser".to_string()),
                ),
            ],
        );
        (directory, conn)
    }

    #[test]
    fn restores_only_the_missing_application_marker_for_an_existing_profile() {
        let (_directory, mut conn) = initialized_profile_without_application();

        assert!(ensure_setup_complete(&mut conn).unwrap());
        assert!(get_all_iri_properties(
            &conn,
            "foundation:ThisFoundationInstance",
            "rdf:type"
        )
        .unwrap()
        .contains(&"foundation:Application".to_string()));
    }

    #[test]
    fn does_not_restore_the_marker_without_a_person_profile() {
        let (_directory, mut conn) = setup_test_db_without_application_marker();

        assert!(!ensure_setup_complete(&mut conn).unwrap());
        assert!(get_all_iri_properties(
            &conn,
            "foundation:ThisFoundationInstance",
            "rdf:type"
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn does_not_restore_the_marker_when_the_computer_is_not_linked_to_the_person() {
        let (_directory, mut conn) = setup_test_db_without_application_marker();
        assert_profile_triples(
            &mut conn,
            &[Triple::new(
                "foundation:ThisUser",
                "rdf:type",
                Object::Iri("foundation:Person".to_string()),
            )],
        );

        assert!(!ensure_setup_complete(&mut conn).unwrap());
        assert!(get_all_iri_properties(
            &conn,
            "foundation:ThisFoundationInstance",
            "rdf:type"
        )
        .unwrap()
        .is_empty());
    }
}
