use crate::eavto::Connection;

include!(concat!(env!("OUT_DIR"), "/material_symbols.rs"));

pub const MATERIAL_SYMBOLS_LIBRARY_IRI: &str = "foundation:IconLibrary_1772733525675";

/// Converts an icon symbol name to its canonical IRI.
/// e.g. "person" → "foundation:icon-material-symbols-name-person"
pub fn icon_name_to_iri(name: &str) -> String {
    format!("foundation:icon-material-symbols-name-{name}")
}

/// Resolves an icon IRI to its display value (symbol name or file URL).
/// For library icons, parses directly from the IRI (no DB query needed).
/// For file icons, queries foundation:iconKey from the DB.
pub fn icon_iri_to_display(conn: &Connection, iri: &str) -> Option<String> {
    if let Some(key) = iri.strip_prefix("foundation:icon-material-symbols-name-") {
        return Some(key.to_string());
    }
    if iri.starts_with("foundation:icon-file-") {
        return crate::owl::get_literal_property(conn, iri, "foundation:iconKey").ok().flatten();
    }
    None
}

/// Returns `(predicate, Object)` for storing an icon value.
/// Symbol names use `foundation:hasIcon` with an IRI; URL icons use `foundation:icon` (legacy literal).
pub fn icon_store_value(icon: &str) -> (&'static str, crate::eavto::Object) {
    use crate::eavto::Object;
    if icon.starts_with("http://")
        || icon.starts_with("https://")
        || icon.starts_with("file://")
        || icon.starts_with("data:")
    {
        ("foundation:icon", Object::Literal {
            value: icon.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })
    } else {
        ("foundation:hasIcon", Object::Iri(icon_name_to_iri(icon)))
    }
}

/// Validates that `icon` is a recognised icon: a valid Material Symbols IRI, a raw symbol name
/// that exists in the seeded library, or a URL-based icon (http/https/file/data).
pub fn validate_icon(conn: &Connection, icon: &str) -> crate::owl::Result<()> {
    if icon.starts_with("http://")
        || icon.starts_with("https://")
        || icon.starts_with("file://")
        || icon.starts_with("data:")
    {
        return Ok(());
    }

    // Accept fully-qualified icon IRIs that exist in the DB
    if icon.starts_with("foundation:icon-") {
        use crate::eavto::query;
        let result = query::get_by_entity_predicate(conn, icon, "foundation:iconKey")?;
        if result.triples.is_empty() {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Icon IRI '{}' does not exist in the ontology.",
                icon
            )));
        }
        return Ok(());
    }

    // Accept raw symbol names that map to a known icon IRI
    use crate::eavto::query;
    let target_iri = icon_name_to_iri(icon);
    let result = query::get_by_entity_predicate(conn, &target_iri, "foundation:iconKey")?;
    if result.triples.is_empty() {
        return Err(crate::owl::OwlError::ValidationError(format!(
            "Icon '{}' is not a valid Material Symbols name. \
             Use a valid icon name (e.g., 'person', 'home', 'star') or an image URL.",
            icon
        )));
    }
    Ok(())
}

/// Seeds all Material Symbols icons into the ontology if not already up to date.
/// Uses a single batch transaction for performance. Idempotent — safe to call every startup.
pub fn seed_icon_library(conn: &mut Connection) {
    let current_version = MATERIAL_SYMBOLS_VERSION;

    let seeded_version = crate::owl::get_literal_property(
        conn,
        MATERIAL_SYMBOLS_LIBRARY_IRI,
        "foundation:libraryVersion",
    )
    .ok()
    .flatten();

    if seeded_version.as_deref() == Some(current_version) {
        // Verify icons are actually present — version marker alone isn't enough
        let sample_iri = icon_name_to_iri("home");
        let icons_present = crate::owl::get_literal_property(conn, &sample_iri, "foundation:iconKey")
            .ok()
            .flatten()
            .is_some();
        if icons_present {
            crate::commands::log_backend(
                "info",
                &format!("Icon library already seeded (v{current_version}), skipping."),
            );
            return;
        }
    }

    crate::commands::log_backend(
        "info",
        &format!(
            "Seeding {} Material Symbols icons (v{current_version})…",
            MATERIAL_SYMBOLS.len()
        ),
    );

    seed_icons_batch(conn, current_version);
}

/// Migrates existing `foundation:icon` literal triples to `foundation:hasIcon` IRI references.
/// Only migrates symbol names (skips file:// and URL-based icons).
/// Idempotent: if already migrated, does nothing. Safe to call on every startup.
pub fn migrate_icon_to_has_icon(conn: &mut Connection) {
    use crate::eavto::store::assert_triples;
    use crate::eavto::{Triple, Object};

    let rows: Vec<(String, String)> = {
        let sql = "
            SELECT subject, object_value
            FROM triples
            WHERE predicate = 'foundation:icon'
              AND object_type = 'literal'
              AND retracted = 0
              AND object_value NOT LIKE 'http://%'
              AND object_value NOT LIKE 'https://%'
              AND object_value NOT LIKE 'file://%'
              AND object_value NOT LIKE 'data:%'
        ";
        match conn.prepare(sql) {
            Ok(mut stmt) => match stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
                Err(e) => {
                    crate::commands::log_backend("error", &format!("Migration query failed: {e}"));
                    return;
                }
            },
            Err(e) => {
                crate::commands::log_backend("error", &format!("Migration prepare failed: {e}"));
                return;
            }
        }
    };

    if rows.is_empty() {
        return;
    }

    crate::commands::log_backend(
        "info",
        &format!("Migrating {} entities from foundation:icon → foundation:hasIcon…", rows.len()),
    );

    // Insert all new foundation:hasIcon IRI triples in a single transaction
    let new_triples: Vec<Triple> = rows.iter()
        .map(|(subject, icon_name)| Triple::new(
            subject.as_str(),
            "foundation:hasIcon",
            Object::Iri(icon_name_to_iri(icon_name)),
        ))
        .collect();

    if let Err(e) = assert_triples(conn, &new_triples, "system") {
        crate::commands::log_backend("error", &format!("Migration assert failed: {e}"));
        return;
    }

    // Bulk-retract old foundation:icon symbol literals in a single SQL update
    let retract_sql = "
        UPDATE triples SET retracted = 1
        WHERE predicate = 'foundation:icon'
          AND object_type = 'literal'
          AND retracted = 0
          AND object_value NOT LIKE 'http://%'
          AND object_value NOT LIKE 'https://%'
          AND object_value NOT LIKE 'file://%'
          AND object_value NOT LIKE 'data:%'
    ";
    match conn.execute(retract_sql, []) {
        Ok(retracted) => crate::commands::log_backend(
            "info",
            &format!("Migration complete: {retracted} entities migrated to foundation:hasIcon."),
        ),
        Err(e) => crate::commands::log_backend(
            "error",
            &format!("Migration retraction failed: {e}"),
        ),
    }
}

fn seed_icons_batch(conn: &mut Connection, version: &str) {
    use crate::eavto::store::{assert_triples, enter_batch_transaction};
    use crate::eavto::{Triple, Object};

    let _guard = enter_batch_transaction();

    let mut all_triples: Vec<Triple> = Vec::with_capacity(MATERIAL_SYMBOLS.len() * 4 + 1);

    // Update the seeded version on the library instance
    all_triples.push(Triple::new(
        MATERIAL_SYMBOLS_LIBRARY_IRI,
        "foundation:libraryVersion",
        Object::Literal {
            value: version.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        },
    ));

    for name in MATERIAL_SYMBOLS {
        let iri = icon_name_to_iri(name);
        all_triples.push(Triple::new(&iri, "rdf:type", Object::Iri("foundation:Icon".to_string())));
        all_triples.push(Triple::new(
            &iri,
            "rdfs:label",
            Object::Literal {
                value: name.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
        ));
        all_triples.push(Triple::new(
            &iri,
            "foundation:iconKey",
            Object::Literal {
                value: name.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
        ));
        all_triples.push(Triple::new(
            &iri,
            "foundation:fromLibrary",
            Object::Iri(MATERIAL_SYMBOLS_LIBRARY_IRI.to_string()),
        ));
    }

    match assert_triples(conn, &all_triples, "system") {
        Ok(_) => crate::commands::log_backend(
            "info",
            &format!("Seeded {} Material Symbols icons successfully.", MATERIAL_SYMBOLS.len()),
        ),
        Err(e) => crate::commands::log_backend(
            "error",
            &format!("Failed to seed icon library: {e}"),
        ),
    }
}
