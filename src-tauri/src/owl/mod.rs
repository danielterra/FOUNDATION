// App-crate OWL façade: re-exports the ontology primitives from foundation-core.
pub use foundation_core::owl::*;

#[derive(Debug, Clone)]
pub struct SnapshotPropertyValue {
    pub iri: Option<String>,
    pub literal: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyValueCursor {
    pub entity_iri: String,
    pub predicate: String,
    pub snapshot_tx: i64,
    pub value_key: String,
    pub value_tx: i64,
    pub datatype: Option<String>,
    pub language: Option<String>,
    pub object_type: String,
}

#[derive(Debug, Clone)]
pub struct PropertyValuePage {
    pub items: Vec<Triple>,
    pub has_more: bool,
    pub snapshot_tx: i64,
    pub next_cursor: Option<PropertyValueCursor>,
}

pub fn get_property_values_page(
    conn: &Connection,
    subject: &str,
    predicate: &str,
    limit: usize,
    snapshot_tx: i64,
    after: Option<&PropertyValueCursor>,
) -> std::result::Result<PropertyValuePage, String> {
    if let Some(cursor) = after {
        if cursor.entity_iri != subject
            || cursor.predicate != predicate
            || cursor.snapshot_tx != snapshot_tx
            || !matches!(cursor.object_type.as_str(), "iri" | "blank" | "literal")
        {
            return Err(
                "Property cursor does not match the requested entity, predicate, and snapshot"
                    .to_string(),
            );
        }
    }

    if limit == 0 {
        return Err("Property page size must be greater than zero".to_string());
    }

    let after_cursor = after.map(|cursor| (cursor.value_tx, cursor.value_key.clone()));
    let mut rows = foundation_core::eavto::query::get_property_values_page(
        conn,
        subject,
        predicate,
        after_cursor,
        limit
            .checked_add(1)
            .ok_or_else(|| "Property page size is too large".to_string())?,
        snapshot_tx,
    )
    .map_err(|error| error.to_string())?;

    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    let next_cursor = if has_more {
        rows.last()
            .map(|row| -> std::result::Result<PropertyValueCursor, String> {
                let value_key = row
                    .object
                    .clone()
                    .or_else(|| row.object_value.clone())
                    .ok_or_else(|| "Property value row is missing its cursor key".to_string())?;
                Ok(PropertyValueCursor {
                    entity_iri: subject.to_string(),
                    predicate: predicate.to_string(),
                    snapshot_tx,
                    value_key,
                    value_tx: row.value_tx,
                    datatype: row.object_datatype.clone(),
                    language: row.object_language.clone(),
                    object_type: row.object_type.clone(),
                })
            })
            .transpose()?
    } else {
        None
    };

    let items =
        rows.into_iter()
            .map(|row| {
                let object = match row.object_type.as_str() {
                    "iri" => Object::Iri(row.object.ok_or_else(|| {
                        "Property value row is missing its IRI object".to_string()
                    })?),
                    "blank" => Object::Blank(row.object.ok_or_else(|| {
                        "Property value row is missing its blank-node object".to_string()
                    })?),
                    "literal" => Object::Literal {
                        value: row.object_value.ok_or_else(|| {
                            "Property value row is missing its literal value".to_string()
                        })?,
                        datatype: row.object_datatype,
                        language: row.object_language,
                    },
                    object_type => {
                        return Err(format!(
                            "Unsupported property value object type: {object_type}"
                        ))
                    }
                };
                Ok(Triple::new(subject, predicate, object))
            })
            .collect::<std::result::Result<Vec<_>, String>>()?;

    Ok(PropertyValuePage {
        items,
        has_more,
        snapshot_tx,
        next_cursor,
    })
}

pub fn get_property_values_as_of(
    conn: &Connection,
    subject: &str,
    predicate: &str,
    snapshot_tx: i64,
) -> std::result::Result<Vec<SnapshotPropertyValue>, String> {
    const PAGE_SIZE: usize = 128;

    let mut values = Vec::new();
    let mut cursor = None;
    loop {
        let page = get_property_values_page(
            conn,
            subject,
            predicate,
            PAGE_SIZE,
            snapshot_tx,
            cursor.as_ref(),
        )?;
        if page.items.is_empty() {
            break;
        }
        cursor = page.next_cursor;
        values.extend(page.items.into_iter().map(|value| SnapshotPropertyValue {
            iri: value.object.as_iri().map(str::to_string),
            literal: value.object.as_literal(),
        }));
        if cursor.is_none() {
            break;
        }
    }

    Ok(values)
}

pub fn get_iri_property_as_of(
    conn: &Connection,
    subject: &str,
    predicate: &str,
    snapshot_tx: i64,
) -> std::result::Result<Option<String>, String> {
    Ok(
        get_property_values_page(conn, subject, predicate, 1, snapshot_tx, None)
            .map_err(|error| error.to_string())?
            .items
            .into_iter()
            .find_map(|value| value.object.as_iri().map(str::to_string)),
    )
}

pub fn get_all_iri_properties_as_of(
    conn: &Connection,
    subject: &str,
    predicate: &str,
    snapshot_tx: i64,
) -> std::result::Result<Vec<String>, String> {
    Ok(
        get_property_values_as_of(conn, subject, predicate, snapshot_tx)?
            .into_iter()
            .filter_map(|value| value.iri)
            .collect(),
    )
}

pub fn get_literal_property_as_of(
    conn: &Connection,
    subject: &str,
    predicate: &str,
    snapshot_tx: i64,
) -> std::result::Result<Option<String>, String> {
    Ok(
        get_property_values_page(conn, subject, predicate, 1, snapshot_tx, None)
            .map_err(|error| error.to_string())?
            .items
            .into_iter()
            .find_map(|value| value.object.as_literal()),
    )
}

// Re-export specific items that are shadowed by glob to ensure they are accessible.
pub use foundation_core::owl::{OwlError, Result};

pub mod materialization;

pub struct QueryResult {
    pub triples: Vec<Triple>,
}

pub fn query_by_entity_predicate(
    conn: &Connection,
    subject: &str,
    predicate: &str,
) -> Result<QueryResult> {
    foundation_core::eavto::query::get_by_entity_predicate(conn, subject, predicate)
        .map(|result| QueryResult {
            triples: result.triples,
        })
        .map_err(|error| OwlError::DatabaseError(error.to_string()))
}

pub fn query_by_entity(conn: &Connection, subject: &str) -> Result<QueryResult> {
    foundation_core::eavto::query::get_by_entity(conn, subject)
        .map(|result| QueryResult {
            triples: result.triples,
        })
        .map_err(|error| OwlError::DatabaseError(error.to_string()))
}

pub fn query_by_predicate(conn: &Connection, predicate: &str) -> Result<QueryResult> {
    foundation_core::eavto::query::get_by_predicate(conn, predicate)
        .map(|result| QueryResult {
            triples: result.triples,
        })
        .map_err(|error| OwlError::DatabaseError(error.to_string()))
}

pub fn query_by_predicate_object(
    conn: &Connection,
    predicate: &str,
    object: &str,
) -> Result<QueryResult> {
    foundation_core::eavto::query::get_by_predicate_object(conn, predicate, object)
        .map(|result| QueryResult {
            triples: result.triples,
        })
        .map_err(|error| OwlError::DatabaseError(error.to_string()))
}

pub fn retract_raw_triples(conn: &mut Connection, triples: &[Triple], origin: &str) -> Result<()> {
    foundation_core::eavto::store::retract_triples(conn, triples, origin)
        .map(|_| ())
        .map_err(|error| OwlError::DatabaseError(error.to_string()))
}

pub fn assert_raw_triples_with_tx(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str,
) -> Result<i64> {
    foundation_core::eavto::store::assert_triples(conn, triples, origin)
        .map_err(|error| OwlError::DatabaseError(error.to_string()))
}

pub fn get_predicates_for_subjects(
    conn: &Connection,
    subjects: &[String],
    predicates: &[&str],
) -> Result<Vec<(String, String, Object)>> {
    foundation_core::eavto::query::get_predicates_for_subjects(conn, subjects, predicates)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))
}

pub fn get_first_iri_property_batch(
    conn: &Connection,
    subjects: &[String],
    predicate: &str,
) -> Result<std::collections::HashMap<String, String>> {
    foundation_core::eavto::query::get_first_iri_property_batch(conn, subjects, predicate)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))
}

/// Initialize the database with Tauri AppHandle for progress emission.
/// This wrapper lives in the app crate because it depends on tauri::AppHandle.
pub fn initialize_with_progress(
    app: tauri::AppHandle,
) -> std::result::Result<
    (foundation_core::eavto::Connection, std::path::PathBuf),
    foundation_core::eavto::connection::DbError,
> {
    use tauri::Emitter;
    let foundation_dir = crate::config::get_foundation_dir(&app);
    let db_path = foundation_core::eavto::connection::get_db_path_for_dir(&foundation_dir)?;
    let app_clone = app.clone();
    let emit_fn = move |event: &str, payload: serde_json::Value| {
        let _ = app_clone.emit(event, payload);
    };
    let conn =
        foundation_core::eavto::connection::initialize_db_with_emit(&db_path, Some(emit_fn))?;
    Ok((conn, db_path))
}

#[cfg(test)]
mod property_value_page_tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Object};

    #[test]
    fn property_page_cursor_is_bound_to_entity_predicate_and_snapshot() {
        let mut conn = setup_test_db();
        let triples = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"].map(|value| {
            Triple::new(
                "ex:entity",
                "ex:property",
                Object::Literal {
                    value: value.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
            )
        });
        store::assert_triples(&mut conn, &triples, "test").unwrap();
        let snapshot_tx = current_snapshot_tx(&conn).unwrap();

        let first =
            get_property_values_page(&conn, "ex:entity", "ex:property", 3, snapshot_tx, None)
                .unwrap();
        assert!(first.has_more);
        let cursor = first.next_cursor.unwrap();
        assert_eq!(cursor.entity_iri, "ex:entity");
        assert_eq!(cursor.predicate, "ex:property");
        assert_eq!(cursor.snapshot_tx, snapshot_tx);

        let next = get_property_values_page(
            &conn,
            "ex:entity",
            "ex:property",
            3,
            snapshot_tx,
            Some(&cursor),
        )
        .unwrap();
        assert!(!next.has_more);
        assert_eq!(next.items.len(), 3);

        assert!(get_property_values_page(
            &conn,
            "ex:other",
            "ex:property",
            3,
            snapshot_tx,
            Some(&cursor),
        )
        .is_err());
        assert!(get_property_values_page(
            &conn,
            "ex:entity",
            "ex:other-property",
            3,
            snapshot_tx,
            Some(&cursor),
        )
        .is_err());
        assert!(get_property_values_page(
            &conn,
            "ex:entity",
            "ex:property",
            3,
            snapshot_tx + 1,
            Some(&cursor),
        )
        .is_err());
    }
}
