use crate::eavto::Connection;

include!(concat!(env!("OUT_DIR"), "/material_symbols.rs"));

pub const MATERIAL_SYMBOLS_LIBRARY_IRI: &str = "foundation:IconLibrary_1772733525675";

/// Converts an icon symbol name to its canonical IRI.
/// e.g. "person" → "foundation:icon-material-symbols-name-person"
pub fn icon_name_to_iri(name: &str) -> String {
    format!("foundation:icon-material-symbols-name-{name}")
}

/// Resolves an icon IRI to its display value (symbol name or URL).
/// Only Material Symbols IRIs are valid — file icons are always stored as literals.
pub fn icon_iri_to_display(_conn: &Connection, iri: &str) -> Option<String> {
    if let Some(key) = iri.strip_prefix("foundation:icon-material-symbols-name-") {
        return Some(key.to_string());
    }
    if iri.starts_with("https://") || iri.starts_with("http://") || iri.starts_with("data:") {
        return Some(iri.to_string());
    }
    None
}

/// Resolve an icon literal value to an absolute path/URL the frontend can render.
/// Portable relative paths (e.g. `attachments/foo.jpg`) are joined onto the
/// current foundation_dir and returned as `file://...` so callers that strip
/// the prefix and pass to `convertFileSrc` keep working unchanged.
pub fn icon_literal_to_display(value: &str) -> String {
    if value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("data:")
        || value.starts_with("file://")
    {
        return value.to_string();
    }
    let absolute = crate::paths::resolve_path(value);
    format!("file://{}", absolute.to_string_lossy())
}

pub fn icon_store_value(icon: &str) -> (&'static str, crate::eavto::Object) {
    use crate::eavto::Object;
    if icon.starts_with("http://")
        || icon.starts_with("https://")
        || icon.starts_with("data:")
    {
        return ("foundation:hasIcon", Object::Literal {
            value: icon.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
    }
    if let Some(path) = icon.strip_prefix("file://") {
        // Convert paths inside foundation_dir to portable form so the same DB
        // can roam between machines (matches the foundation:filePath convention).
        // External `file://` paths are kept absolute.
        let stored = crate::paths::to_portable_path(std::path::Path::new(path));
        return ("foundation:hasIcon", Object::Literal {
            value: stored,
            datatype: Some("xsd:string".to_string()),
            language: None,
        });
    }
    ("foundation:hasIcon", Object::Iri(icon_name_to_iri(icon)))
}

/// Validates that `icon` is a recognised icon: a valid Material Symbols IRI, a raw symbol name
/// that exists in the seeded library, or a URL-based icon (http/https/file/data).
pub fn validate_icon(conn: &Connection, icon: &str) -> crate::owl::Result<()> {
    if icon.starts_with("http://") || icon.starts_with("https://") || icon.starts_with("data:") {
        return Ok(());
    }
    if let Some(path) = icon.strip_prefix("file://") {
        if !std::path::Path::new(path).exists() {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Icon file not found: '{}'. The file must exist on disk.",
                path
            )));
        }
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
            crate::diagnostics::log_backend(
                "info",
                &format!("Icon library already seeded (v{current_version}), skipping."),
            );
            return;
        }
    }

    crate::diagnostics::log_backend(
        "info",
        &format!(
            "Seeding {} Material Symbols icons (v{current_version})…",
            MATERIAL_SYMBOLS.len()
        ),
    );

    seed_icons_batch(conn, current_version);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::eavto::Object;

    // ── icon_name_to_iri ─────────────────────────────────────────────────────────

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

    // ── icon_iri_to_display ──────────────────────────────────────────────────────

    #[test]
    fn test_icon_iri_to_display_symbol() {
        let conn = setup_test_db();
        let iri = icon_name_to_iri("star");
        assert_eq!(icon_iri_to_display(&conn, &iri), Some("star".to_string()));
    }

    #[test]
    fn test_icon_iri_to_display_unknown_returns_none() {
        let conn = setup_test_db();
        assert_eq!(icon_iri_to_display(&conn, "foundation:icon-unknown-xyz"), None);
    }

    #[test]
    fn test_icon_iri_to_display_non_icon_iri_returns_none() {
        let conn = setup_test_db();
        assert_eq!(icon_iri_to_display(&conn, "foundation:SomethingElse"), None);
    }

    #[test]
    fn test_icon_iri_to_display_url_schemes_passthrough() {
        let conn = setup_test_db();
        assert_eq!(
            icon_iri_to_display(&conn, "https://example.com/icon.png"),
            Some("https://example.com/icon.png".to_string())
        );
        assert_eq!(
            icon_iri_to_display(&conn, "http://example.com/icon.png"),
            Some("http://example.com/icon.png".to_string())
        );
        assert_eq!(
            icon_iri_to_display(&conn, "data:image/png;base64,abc"),
            Some("data:image/png;base64,abc".to_string())
        );
    }

    // ── icon_store_value ─────────────────────────────────────────────────────────

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
        assert!(matches!(obj, Object::Literal { .. }));
        if let Object::Literal { value, .. } = obj {
            assert!(!value.starts_with("file://"), "stored value must not carry file:// prefix");
            assert!(value.contains("path") && value.contains("to") && value.contains("icon.png"),
                "stored value must contain the path components: got {value}");
        }
    }

    #[test]
    fn test_icon_store_value_data_url_uses_has_icon_literal() {
        let (pred, obj) = icon_store_value("data:image/png;base64,abc");
        assert_eq!(pred, "foundation:hasIcon");
        assert!(matches!(obj, Object::Literal { ref value, .. } if value == "data:image/png;base64,abc"));
    }

    // ── seed_icon_library ────────────────────────────────────────────────────────

    #[test]
    fn test_seed_icon_library_seeds_known_icons() {
        let mut conn = setup_test_db();
        seed_icon_library(&mut conn);

        let home_iri = icon_name_to_iri("home");
        let key = crate::owl::get_literal_property(&conn, &home_iri, "foundation:iconKey")
            .unwrap()
            .unwrap();
        assert_eq!(key, "home");
    }

    #[test]
    fn test_seed_icon_library_sets_version() {
        let mut conn = setup_test_db();
        seed_icon_library(&mut conn);

        let version = crate::owl::get_literal_property(
            &conn,
            MATERIAL_SYMBOLS_LIBRARY_IRI,
            "foundation:libraryVersion",
        )
        .unwrap()
        .unwrap();
        assert_eq!(version, MATERIAL_SYMBOLS_VERSION);
    }

    #[test]
    fn test_seed_icon_library_is_idempotent() {
        let mut conn = setup_test_db();
        seed_icon_library(&mut conn);
        seed_icon_library(&mut conn);

        let home_iri = icon_name_to_iri("home");
        let key = crate::owl::get_literal_property(&conn, &home_iri, "foundation:iconKey")
            .unwrap()
            .unwrap();
        assert_eq!(key, "home");
    }

    #[test]
    fn test_seed_icon_library_sets_self_referential_has_icon() {
        let mut conn = setup_test_db();
        seed_icon_library(&mut conn);

        let home_iri = icon_name_to_iri("home");
        let result = crate::eavto::query::get_by_entity_predicate(
            &conn, &home_iri, "foundation:hasIcon"
        ).unwrap();
        assert_eq!(result.triples.len(), 1);
        assert!(matches!(&result.triples[0].object, Object::Iri(iri) if iri == &home_iri));
    }

    // ── validate_icon ────────────────────────────────────────────────────────────

    #[test]
    fn test_validate_icon_valid_symbol_name() {
        let mut conn = setup_test_db();
        seed_icon_library(&mut conn);
        assert!(validate_icon(&conn, "home").is_ok());
        assert!(validate_icon(&conn, "person").is_ok());
    }

    #[test]
    fn test_validate_icon_invalid_symbol_name() {
        let mut conn = setup_test_db();
        seed_icon_library(&mut conn);
        let err = validate_icon(&conn, "not_a_real_icon_xyz_abc");
        assert!(err.is_err());
    }

    #[test]
    fn test_validate_icon_http_https_data_always_valid() {
        let conn = setup_test_db();
        assert!(validate_icon(&conn, "https://example.com/icon.png").is_ok());
        assert!(validate_icon(&conn, "http://example.com/icon.png").is_ok());
        assert!(validate_icon(&conn, "data:image/png;base64,abc").is_ok());
    }

    #[test]
    fn test_validate_icon_file_url_nonexistent_rejected() {
        let conn = setup_test_db();
        assert!(validate_icon(&conn, "file:///nonexistent/path/icon.png").is_err());
    }

    #[test]
    fn test_validate_icon_file_url_existing_accepted() {
        let conn = setup_test_db();
        let tmp = std::env::temp_dir().join("test_icon.png");
        std::fs::write(&tmp, b"fake").unwrap();
        let url = format!("file://{}", tmp.display());
        assert!(validate_icon(&conn, &url).is_ok());
        std::fs::remove_file(tmp).ok();
    }

    // ── icon_literal_to_display ───────────────────────────────────────────────

    #[test]
    fn test_icon_literal_to_display_http_passthrough() {
        let url = "http://example.com/icon.png";
        assert_eq!(icon_literal_to_display(url), url);
    }

    #[test]
    fn test_icon_literal_to_display_https_passthrough() {
        let url = "https://example.com/icon.svg";
        assert_eq!(icon_literal_to_display(url), url);
    }

    #[test]
    fn test_icon_literal_to_display_data_uri_passthrough() {
        let data = "data:image/png;base64,abc123";
        assert_eq!(icon_literal_to_display(data), data);
    }

    #[test]
    fn test_icon_literal_to_display_file_url_passthrough() {
        let path = "file:///home/user/pics/icon.png";
        assert_eq!(icon_literal_to_display(path), path);
    }

    #[test]
    fn test_icon_literal_to_display_relative_path_gets_file_prefix() {
        let result = icon_literal_to_display("attachments/photo.jpg");
        assert!(result.starts_with("file://"),
            "relative path must be prefixed with file://; got: {}", result);
        assert!(result.ends_with("photo.jpg"),
            "relative path must preserve filename; got: {}", result);
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
        all_triples.push(Triple::new(
            &iri,
            "foundation:hasIcon",
            Object::Iri(iri.clone()),
        ));
    }

    match assert_triples(conn, &all_triples, "system") {
        Ok(_) => crate::diagnostics::log_backend(
            "info",
            &format!("Seeded {} Material Symbols icons successfully.", MATERIAL_SYMBOLS.len()),
        ),
        Err(e) => crate::diagnostics::log_backend(
            "error",
            &format!("Failed to seed icon library: {e}"),
        ),
    }
}

