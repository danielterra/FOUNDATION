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
pub async fn icon_iri_to_display(conn: &Connection, iri: &str) -> Option<String> {
    if let Some(key) = iri.strip_prefix("foundation:icon-material-symbols-name-") {
        return Some(key.to_string());
    }
    if iri.starts_with("foundation:icon-file-") {
        return crate::owl::get_literal_property(conn, iri, "foundation:iconKey").await.ok().flatten();
    }
    None
}

pub fn icon_store_value(icon: &str) -> (&'static str, crate::eavto::Object) {
    use crate::eavto::Object;
    if icon.starts_with("http://")
        || icon.starts_with("https://")
        || icon.starts_with("file://")
        || icon.starts_with("data:")
    {
        ("foundation:hasIcon", Object::Literal {
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
pub async fn validate_icon(conn: &Connection, icon: &str) -> crate::owl::Result<()> {
    if icon.starts_with("http://")
        || icon.starts_with("https://")
        || icon.starts_with("file://")
        || icon.starts_with("data:")
    {
        return Ok(());
    }

    if icon.starts_with("foundation:icon-") {
        use crate::eavto::query;
        let result = query::get_by_entity_predicate(conn, icon, "foundation:iconKey").await?;
        if result.triples.is_empty() {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Icon IRI '{}' does not exist in the ontology.",
                icon
            )));
        }
        return Ok(());
    }

    use crate::eavto::query;
    let target_iri = icon_name_to_iri(icon);
    let result = query::get_by_entity_predicate(conn, &target_iri, "foundation:iconKey").await?;
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
pub async fn seed_icon_library(conn: &Connection) {
    let current_version = MATERIAL_SYMBOLS_VERSION;

    let seeded_version = crate::owl::get_literal_property(
        conn,
        MATERIAL_SYMBOLS_LIBRARY_IRI,
        "foundation:libraryVersion",
    )
    .await
    .ok()
    .flatten();

    if seeded_version.as_deref() == Some(current_version) {
        let sample_iri = icon_name_to_iri("home");
        let icons_present = crate::owl::get_literal_property(conn, &sample_iri, "foundation:iconKey")
            .await
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

    seed_icons_batch(conn, current_version).await;
}

/// Idempotent on every startup — safe to call repeatedly.
pub async fn migrate_icon_to_has_icon(conn: &Connection) {
    use crate::eavto::store::assert_triples;
    use crate::eavto::{Triple, Object};

    let rows: Vec<(String, String)> = {
        let sql = "
            SELECT subject, object_value
            FROM triples
            WHERE predicate = 'foundation:icon'
              AND object_type = 'literal'
              AND retracted = 0
        ";
        let mut stmt = match conn.prepare(sql).await {
            Ok(s) => s,
            Err(e) => {
                crate::commands::log_backend("error", &format!("Migration prepare failed: {e}"));
                return;
            }
        };
        let mut rows_raw = match stmt.query(()).await {
            Ok(r) => r,
            Err(e) => {
                crate::commands::log_backend("error", &format!("Migration query failed: {e}"));
                return;
            }
        };
        let mut result = Vec::new();
        while let Ok(Some(row)) = rows_raw.next().await {
            let subject = row.get_value(0).ok().and_then(|v| v.as_text().cloned());
            let value = row.get_value(1).ok().and_then(|v| v.as_text().cloned());
            if let (Some(s), Some(v)) = (subject, value) {
                result.push((s, v));
            }
        }
        result
    };

    if rows.is_empty() {
        return;
    }

    crate::commands::log_backend(
        "info",
        &format!("Migrating {} foundation:icon triples → foundation:hasIcon…", rows.len()),
    );

    let new_triples: Vec<Triple> = rows.iter()
        .map(|(subject, value)| {
            let obj = if value.starts_with("http://")
                || value.starts_with("https://")
                || value.starts_with("file://")
                || value.starts_with("data:")
            {
                Object::Literal {
                    value: value.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }
            } else {
                Object::Iri(icon_name_to_iri(value))
            };
            Triple::new(subject.as_str(), "foundation:hasIcon", obj)
        })
        .collect();

    if let Err(e) = assert_triples(conn, &new_triples, "system").await {
        crate::commands::log_backend("error", &format!("Migration assert failed: {e}"));
        return;
    }

    let retract_sql = "
        UPDATE triples SET retracted = 1
        WHERE predicate = 'foundation:icon'
          AND object_type = 'literal'
          AND retracted = 0
    ";
    match conn.execute(retract_sql, ()).await {
        Ok(_) => crate::commands::log_backend(
            "info",
            "Migration complete: triples migrated to foundation:hasIcon.",
        ),
        Err(e) => crate::commands::log_backend(
            "error",
            &format!("Migration retraction failed: {e}"),
        ),
    }
}

async fn seed_icons_batch(conn: &Connection, version: &str) {
    use crate::eavto::store::assert_triples;
    use crate::eavto::{Triple, Object};

    let mut all_triples: Vec<Triple> = Vec::with_capacity(MATERIAL_SYMBOLS.len() * 4 + 1);

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
        all_triples.push(Triple::new(
            &iri,
            "foundation:hasIcon",
            Object::Iri(iri.clone()),
        ));
    }

    conn.execute("BEGIN", ()).await.ok();
    let result: Result<(), String> = async {
        assert_triples(conn, &all_triples, "system").await.map_err(|e| e.to_string())?;
        Ok(())
    }.await;
    match result {
        Ok(()) => {
            conn.execute("COMMIT", ()).await.ok();
            crate::commands::log_backend(
                "info",
                &format!("Seeded {} Material Symbols icons successfully.", MATERIAL_SYMBOLS.len()),
            );
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            crate::commands::log_backend(
                "error",
                &format!("Failed to seed icon library: {e}"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::eavto::{Triple, Object};
    use crate::eavto::store;

    // ── icon_name_to_iri ────────────────────────────────────────────────────

    #[test]
    fn test_icon_name_to_iri() {
        assert_eq!(
            icon_name_to_iri("person"),
            "foundation:icon-material-symbols-name-person"
        );
        assert_eq!(
            icon_name_to_iri("home"),
            "foundation:icon-material-symbols-name-home"
        );
    }

    // ── icon_iri_to_display ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_icon_iri_to_display_symbol() {
        let conn = setup_test_db().await;
        let iri = icon_name_to_iri("star");
        assert_eq!(icon_iri_to_display(&conn, &iri).await, Some("star".to_string()));
    }

    #[tokio::test]
    async fn test_icon_iri_to_display_unknown_returns_none() {
        let conn = setup_test_db().await;
        assert_eq!(icon_iri_to_display(&conn, "foundation:icon-unknown-xyz").await, None);
    }

    #[tokio::test]
    async fn test_icon_iri_to_display_non_icon_iri_returns_none() {
        let conn = setup_test_db().await;
        assert_eq!(icon_iri_to_display(&conn, "foundation:SomethingElse").await, None);
    }

    // ── icon_store_value ────────────────────────────────────────────────────

    #[test]
    fn test_icon_store_value_symbol_name_uses_has_icon_iri() {
        let (pred, obj) = icon_store_value("home");
        assert_eq!(pred, "foundation:hasIcon");
        assert!(matches!(obj, Object::Iri(iri) if iri == "foundation:icon-material-symbols-name-home"));
    }

    #[test]
    fn test_icon_store_value_https_url_uses_has_icon_literal() {
        let (pred, obj) = icon_store_value("https://example.com/icon.png");
        assert_eq!(pred, "foundation:hasIcon");
        assert!(matches!(obj, Object::Literal { ref value, .. } if value == "https://example.com/icon.png"));
    }

    #[test]
    fn test_icon_store_value_http_url_uses_has_icon_literal() {
        let (pred, obj) = icon_store_value("http://example.com/icon.png");
        assert_eq!(pred, "foundation:hasIcon");
        assert!(matches!(obj, Object::Literal { ref value, .. } if value == "http://example.com/icon.png"));
    }

    #[test]
    fn test_icon_store_value_file_url_uses_has_icon_literal() {
        let (pred, obj) = icon_store_value("file:///path/to/icon.png");
        assert_eq!(pred, "foundation:hasIcon");
        assert!(matches!(obj, Object::Literal { ref value, .. } if value == "file:///path/to/icon.png"));
    }

    #[test]
    fn test_icon_store_value_data_url_uses_has_icon_literal() {
        let (pred, obj) = icon_store_value("data:image/png;base64,abc");
        assert_eq!(pred, "foundation:hasIcon");
        assert!(matches!(obj, Object::Literal { ref value, .. } if value == "data:image/png;base64,abc"));
    }

    // ── seed_icon_library ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_seed_icon_library_seeds_known_icons() {
        let conn = setup_test_db().await;
        seed_icon_library(&conn).await;

        let home_iri = icon_name_to_iri("home");
        let key = crate::owl::get_literal_property(&conn, &home_iri, "foundation:iconKey")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(key, "home");
    }

    #[tokio::test]
    async fn test_seed_icon_library_sets_version() {
        let conn = setup_test_db().await;
        seed_icon_library(&conn).await;

        let version = crate::owl::get_literal_property(
            &conn,
            MATERIAL_SYMBOLS_LIBRARY_IRI,
            "foundation:libraryVersion",
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(version, MATERIAL_SYMBOLS_VERSION);
    }

    #[tokio::test]
    async fn test_seed_icon_library_is_idempotent() {
        let conn = setup_test_db().await;
        seed_icon_library(&conn).await;
        seed_icon_library(&conn).await;

        let home_iri = icon_name_to_iri("home");
        let key = crate::owl::get_literal_property(&conn, &home_iri, "foundation:iconKey")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(key, "home");
    }

    #[tokio::test]
    async fn test_seed_icon_library_sets_self_referential_has_icon() {
        let conn = setup_test_db().await;
        seed_icon_library(&conn).await;

        let home_iri = icon_name_to_iri("home");
        let result = crate::eavto::query::get_by_entity_predicate(
            &conn, &home_iri, "foundation:hasIcon"
        ).await.unwrap();
        assert_eq!(result.triples.len(), 1);
        assert!(matches!(&result.triples[0].object, Object::Iri(iri) if iri == &home_iri));
    }

    // ── validate_icon ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_validate_icon_valid_symbol_name() {
        let conn = setup_test_db().await;
        seed_icon_library(&conn).await;
        assert!(validate_icon(&conn, "home").await.is_ok());
        assert!(validate_icon(&conn, "person").await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_icon_invalid_symbol_name() {
        let conn = setup_test_db().await;
        seed_icon_library(&conn).await;
        let err = validate_icon(&conn, "not_a_real_icon_xyz_abc").await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_validate_icon_url_formats_always_valid() {
        let conn = setup_test_db().await;
        assert!(validate_icon(&conn, "https://example.com/icon.png").await.is_ok());
        assert!(validate_icon(&conn, "http://example.com/icon.png").await.is_ok());
        assert!(validate_icon(&conn, "file:///path/to/icon.png").await.is_ok());
        assert!(validate_icon(&conn, "data:image/png;base64,abc").await.is_ok());
    }

    // ── migrate_icon_to_has_icon ────────────────────────────────────────────

    #[tokio::test]
    async fn test_migrate_converts_symbol_literal_to_has_icon_iri() {
        let conn = setup_test_db().await;
        store::assert_triples(
            &conn,
            &[Triple::new(
                "foundation:TestThing",
                "foundation:icon",
                Object::Literal {
                    value: "person".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            )],
            "test",
        )
        .await
        .unwrap();

        migrate_icon_to_has_icon(&conn).await;

        let old = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:TestThing", "foundation:icon",
        )
        .await
        .unwrap();
        assert!(old.triples.is_empty(), "old foundation:icon literal should be retracted");

        let new = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:TestThing", "foundation:hasIcon",
        )
        .await
        .unwrap();
        assert_eq!(new.triples.len(), 1);
        assert!(matches!(
            &new.triples[0].object,
            Object::Iri(iri) if iri == "foundation:icon-material-symbols-name-person"
        ));
    }

    #[tokio::test]
    async fn test_migrate_converts_url_literal_to_has_icon_literal() {
        let conn = setup_test_db().await;
        store::assert_triples(
            &conn,
            &[Triple::new(
                "foundation:TestThing",
                "foundation:icon",
                Object::Literal {
                    value: "https://example.com/icon.png".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            )],
            "test",
        )
        .await
        .unwrap();

        migrate_icon_to_has_icon(&conn).await;

        let old = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:TestThing", "foundation:icon",
        )
        .await
        .unwrap();
        assert!(old.triples.is_empty(), "old foundation:icon literal should be retracted");

        let new = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:TestThing", "foundation:hasIcon",
        )
        .await
        .unwrap();
        assert_eq!(new.triples.len(), 1);
        assert!(matches!(
            &new.triples[0].object,
            Object::Literal { value, .. } if value == "https://example.com/icon.png"
        ));
    }

    #[tokio::test]
    async fn test_migrate_is_idempotent() {
        let conn = setup_test_db().await;
        store::assert_triples(
            &conn,
            &[Triple::new(
                "foundation:TestThing",
                "foundation:icon",
                Object::Literal {
                    value: "star".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            )],
            "test",
        )
        .await
        .unwrap();

        migrate_icon_to_has_icon(&conn).await;
        migrate_icon_to_has_icon(&conn).await;

        let new = crate::eavto::query::get_by_entity_predicate(
            &conn, "foundation:TestThing", "foundation:hasIcon",
        )
        .await
        .unwrap();
        assert_eq!(new.triples.len(), 1, "second migration should not duplicate the triple");
    }

    #[tokio::test]
    async fn test_migrate_no_op_when_nothing_to_migrate() {
        let conn = setup_test_db().await;
        migrate_icon_to_has_icon(&conn).await;
    }
}
