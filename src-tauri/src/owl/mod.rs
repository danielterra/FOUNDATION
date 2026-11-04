// App-crate OWL façade: re-exports the ontology primitives from foundation-core.
pub use foundation_core::owl::formula_job_store;
pub use foundation_core::owl::*;

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
