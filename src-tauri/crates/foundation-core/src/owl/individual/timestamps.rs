use crate::eavto::{store, Triple, Object, Connection};

pub const LAST_UPDATED_AT: &str = "foundation:lastUpdatedAt";

#[cfg(test)]
#[path = "timestamps_tests.rs"]
mod tests;

/// Uses direct SQL + eavto store to bypass class-property validation and avoid recursion.
/// Failures are silently ignored — timestamp writes must never break the primary operation.
pub fn touch(conn: &mut Connection, iri: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let triple = Triple::new(iri, LAST_UPDATED_AT, Object::DateTime(now));
    let _ = store::assert_triples(conn, &[triple], "system");
}
