use crate::owl::{get_iri_property, get_literal_property, Individual, Object};

/// Status IRIs for RawDataRecord transform outcomes.
pub const STATUS_TRANSFORMED: &str = "foundation:Status_1781300928585";
pub const STATUS_PENDING: &str = "foundation:Pending";
pub const STATUS_ERROR: &str = "foundation:Status_1781300928559";
pub const STATUS_SKIPPED: &str = "foundation:Status_1781300928612";

/// Status IRIs for DataSource.
pub const DATASOURCE_STATUS_ACTIVE: &str = "foundation:Status_1781300928499";
pub const DATASOURCE_STATUS_PAUSED: &str = "foundation:Status_1781300928524";
pub const DATASOURCE_STATUS_ERROR: &str = "foundation:Status_1781300928559";

/// Status IRIs for SyncRecord.
pub const SYNCRECORD_STATUS_SYNCED: &str = "foundation:Status_1781300928637";
pub const SYNCRECORD_STATUS_PENDING: &str = "foundation:Pending";
pub const SYNCRECORD_STATUS_CONFLICT: &str = "foundation:Status_1781300928665";

fn str_lit(v: impl Into<String>) -> Object {
    Object::Literal {
        value: v.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

fn dt_lit(rfc3339: impl Into<String>) -> Object {
    Object::Literal {
        value: rfc3339.into(),
        datatype: Some("xsd:dateTime".to_string()),
        language: None,
    }
}

/// Builds the deterministic IRI for a synced entity.
///
/// Format: `{namespace}:{class_local_name}_{sanitized_external_id}`
///
/// Sanitization rules (injective — distinct ids never collide):
/// - Colons become `__colon__`
/// - Slashes become `__slash__`
/// - Characters outside `[A-Za-z0-9_-]` are percent-encoded (uppercase hex)
///
/// The namespace and class_local_name are left as-is (callers supply clean values).
pub fn build_deterministic_iri(namespace: &str, class_local_name: &str, external_id: &str) -> String {
    let sanitized = sanitize_external_id(external_id);
    format!("{}:{}_{}", namespace, class_local_name, sanitized)
}

fn sanitize_external_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len() * 2);
    for ch in id.chars() {
        match ch {
            ':' => out.push_str("__colon__"),
            '/' => out.push_str("__slash__"),
            '\\' => out.push_str("__backslash__"),
            '#' => out.push_str("__hash__"),
            '?' => out.push_str("__q__"),
            ' ' => out.push('_'),
            c if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' => out.push(c),
            c => {
                for byte in c.to_string().as_bytes() {
                    out.push_str(&format!("_{:02X}", byte));
                }
            }
        }
    }
    out
}

/// Asserts (upsert) a SyncRecord linking `data_source_iri` + `external_id` to `thing_iri`.
///
/// The SyncRecord IRI is itself deterministic: `{namespace}_sync_{sanitized_external_id}`.
/// If the record already exists the latest-TX write silently updates it.
pub fn upsert_sync_record(
    conn: &mut crate::owl::Connection,
    data_source_iri: &str,
    namespace: &str,
    external_id: &str,
    thing_iri: &str,
) -> Result<String, String> {
    let sanitized = sanitize_external_id(external_id);
    let sync_iri = format!("{}_sync_{}", namespace, sanitized);
    let now = chrono::Utc::now().to_rfc3339();

    let ind = Individual::new(&sync_iri);

    let already_exists = get_iri_property(conn, &sync_iri, "rdf:type")
        .map_err(|e| e.to_string())?
        .is_some();

    if !already_exists {
        ind.assert(conn, "foundation:SyncRecord", &sync_iri, "sync_alt", "data_sync")
            .map_err(|e| format!("assert SyncRecord: {}", e))?;
        ind.add_property(conn, "foundation:hasStatus",
            vec![Object::Iri(SYNCRECORD_STATUS_SYNCED.to_string())], "data_sync")
            .map_err(|e| format!("SyncRecord hasStatus: {}", e))?;
    }

    ind.add_property(conn, "foundation:belongsToDataSource",
        vec![Object::Iri(data_source_iri.to_string())], "data_sync")
        .map_err(|e| format!("SyncRecord belongsToDataSource: {}", e))?;

    ind.add_property(conn, "foundation:externalId",
        vec![str_lit(external_id)], "data_sync")
        .map_err(|e| format!("SyncRecord externalId: {}", e))?;

    ind.add_property(conn, "foundation:mapsToThing",
        vec![Object::Iri(thing_iri.to_string())], "data_sync")
        .map_err(|e| format!("SyncRecord mapsToThing: {}", e))?;

    ind.add_property(conn, "foundation:lastSyncedAt",
        vec![dt_lit(now)], "data_sync")
        .map_err(|e| format!("SyncRecord lastSyncedAt: {}", e))?;

    ind.add_property(conn, "foundation:syncStatus",
        vec![Object::Iri(SYNCRECORD_STATUS_SYNCED.to_string())], "data_sync")
        .map_err(|e| format!("SyncRecord syncStatus: {}", e))?;

    Ok(sync_iri)
}

/// Marks a RawDataRecord as Transformed and links `transformed_into_iris`.
pub fn mark_raw_transformed(
    conn: &mut crate::owl::Connection,
    raw_iri: &str,
    transformed_into_iris: &[&str],
) -> Result<(), String> {
    let ind = Individual::new(raw_iri);

    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(STATUS_TRANSFORMED.to_string())], "data_sync")
        .map_err(|e| format!("mark_raw_transformed hasStatus: {}", e))?;

    if !transformed_into_iris.is_empty() {
        let objects: Vec<Object> = transformed_into_iris.iter()
            .map(|iri| Object::Iri(iri.to_string()))
            .collect();
        ind.add_property(conn, "foundation:transformedInto", objects, "data_sync")
            .map_err(|e| format!("mark_raw_transformed transformedInto: {}", e))?;
    }

    Ok(())
}

/// Marks a RawDataRecord as Error and records the error message.
/// The rawPayload/rawFile remains intact.
pub fn mark_raw_error(
    conn: &mut crate::owl::Connection,
    raw_iri: &str,
    error_msg: &str,
) -> Result<(), String> {
    let ind = Individual::new(raw_iri);

    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(STATUS_ERROR.to_string())], "data_sync")
        .map_err(|e| format!("mark_raw_error hasStatus: {}", e))?;

    ind.add_property(conn, "foundation:transformError",
        vec![str_lit(error_msg)], "data_sync")
        .map_err(|e| format!("mark_raw_error transformError: {}", e))?;

    Ok(())
}

/// Reads the current retry count for a RawDataRecord and increments it.
pub fn increment_retry_count(
    conn: &mut crate::owl::Connection,
    raw_iri: &str,
) -> Result<u32, String> {
    let current: u32 = get_literal_property(conn, raw_iri, "foundation:retryCount")
        .map_err(|e| e.to_string())?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let next = current + 1;
    Individual::new(raw_iri)
        .add_property(conn, "foundation:retryCount",
            vec![Object::Literal {
                value: next.to_string(),
                datatype: Some("xsd:integer".to_string()),
                language: None,
            }], "data_sync")
        .map_err(|e| format!("retryCount: {}", e))?;
    Ok(next)
}

/// Updates the DataSource status and connection fields atomically.
pub fn update_datasource_connection(
    conn: &mut crate::owl::Connection,
    data_source_iri: &str,
    connected: bool,
    error_msg: Option<&str>,
) -> Result<(), String> {
    let status_iri = if connected {
        DATASOURCE_STATUS_ACTIVE
    } else {
        DATASOURCE_STATUS_ERROR
    };

    let ind = Individual::new(data_source_iri);

    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(status_iri.to_string())], "data_sync")
        .map_err(|e| format!("datasource hasStatus: {}", e))?;

    ind.add_property(conn, "foundation:isConnected",
        vec![Object::Boolean(connected)], "data_sync")
        .map_err(|e| format!("isConnected: {}", e))?;

    if let Some(msg) = error_msg {
        ind.add_property(conn, "foundation:lastConnectionError",
            vec![str_lit(msg)], "data_sync")
            .map_err(|e| format!("lastConnectionError: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_iri_stable_for_same_input() {
        let iri1 = build_deterministic_iri("github", "Issue", "123");
        let iri2 = build_deterministic_iri("github", "Issue", "123");
        assert_eq!(iri1, iri2);
        assert_eq!(iri1, "github:Issue_123");
    }

    #[test]
    fn sanitize_colon_is_injective_from_underscore() {
        let with_colon = build_deterministic_iri("ns", "C", "a:b");
        let with_encoded = build_deterministic_iri("ns", "C", "a__colon__b");
        assert_ne!(with_colon, with_encoded, "collision: 'a:b' and 'a__colon__b' must differ");
    }

    #[test]
    fn sanitize_slash_does_not_collide() {
        let with_slash = build_deterministic_iri("ns", "C", "a/b");
        let with_text = build_deterministic_iri("ns", "C", "a__slash__b");
        assert_ne!(with_slash, with_text);
    }

    #[test]
    fn sanitize_space_becomes_underscore() {
        let iri = build_deterministic_iri("ns", "C", "hello world");
        assert_eq!(iri, "ns:C_hello_world");
    }

    #[test]
    fn sanitize_unicode_is_percent_encoded() {
        let iri = build_deterministic_iri("ns", "C", "café");
        assert!(iri.contains("_C3_A9") || iri.contains("caf"), "got: {}", iri);
    }
}
