use crate::owl::{get_all_iri_properties, Connection, Result};

pub fn is_setup_complete(conn: &Connection) -> Result<bool> {
    let user_types = get_all_iri_properties(conn, "foundation:ThisUser", "rdf:type")?;
    if !user_types.iter().any(|iri| iri == "foundation:Person") {
        return Ok(false);
    }

    let app_types = get_all_iri_properties(conn, "foundation:ThisFoundationInstance", "rdf:type")?;
    if !app_types.iter().any(|iri| iri == "foundation:Application") {
        return Ok(false);
    }

    let computer_users =
        get_all_iri_properties(conn, "foundation:ThisComputer", "foundation:hasUser")?;
    Ok(computer_users
        .iter()
        .any(|iri| iri == "foundation:ThisUser"))
}
