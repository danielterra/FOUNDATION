use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError};

#[cfg(test)]
#[path = "lock_tests.rs"]
mod tests;

/// Returns `true` if the entity has `foundation:isSystemLocked = true`.
pub fn is_system_locked(conn: &Connection, iri: &str) -> bool {
    query::get_by_entity_predicate(conn, iri, "foundation:isSystemLocked")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| if let Object::Boolean(b) = t.object { Some(b) } else { None })
        .unwrap_or(false)
}

/// Sets `foundation:isSystemLocked` on any entity, bypassing the lock guard.
/// This is the only write operation intentionally exempt from lock enforcement.
pub fn set_system_locked(conn: &mut Connection, iri: &str, locked: bool) -> Result<()> {
    let triple = Triple::new(iri, "foundation:isSystemLocked", Object::Boolean(locked));
    store::assert_triples(conn, &[triple], "user")
        .map_err(|e| OwlError::InvalidOperation(e.to_string()))?;
    Ok(())
}

/// Returns `Err` if the entity at `iri` has `foundation:isSystemLocked = true`.
/// Pass `Some("foundation:isSystemLocked")` as `exempt_property` to allow writing
/// the lock flag itself (prevents deadlock when bootstrapping locked entities).
pub fn check_system_locked(
    conn: &Connection,
    iri: &str,
    exempt_property: Option<&str>,
) -> Result<()> {
    if exempt_property == Some("foundation:isSystemLocked") {
        return Ok(());
    }
    let result = query::get_by_entity_predicate(conn, iri, "foundation:isSystemLocked")?;
    let is_locked = result.triples.first()
        .and_then(|t| if let Object::Boolean(b) = &t.object { Some(*b) } else { None })
        .unwrap_or(false);
    if is_locked {
        return Err(OwlError::InvalidOperation(format!(
            "Entity '{}' is system-locked and cannot be modified",
            iri
        )));
    }
    Ok(())
}
