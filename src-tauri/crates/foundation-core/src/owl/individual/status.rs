use crate::eavto::Connection;
use crate::eavto::query;
use crate::eavto::Object;
use crate::owl::{Result, OwlError, Thing};
use crate::owl::icons::icon_iri_to_display;
use super::properties::{get_literal_property, get_iri_property, is_instance_of};

/// Validates that `status_iri` is in the `foundation:allowedStatus` list of `class_iri`.
/// Returns an error if the class has no configured statuses, or if the status is not allowed.
pub fn validate_allowed_status(
    conn: &Connection,
    class_iri: &str,
    status_iri: &str,
) -> Result<()> {
    let result = query::get_by_entity_predicate(conn, class_iri, "foundation:allowedStatus")?;
    if result.triples.is_empty() {
        let class_label = get_literal_property(conn, class_iri, "rdfs:label")?
            .unwrap_or_else(|| class_iri.to_string());
        return Err(OwlError::ValidationError(format!(
            "Concept '{}' has no statuses configured. Every concept must have at least one allowed status. Use learn_concepts to add allowedStatuses to '{}'.",
            class_label, class_iri
        )));
    }
    let allowed_iris: Vec<String> = result.triples.iter()
        .filter_map(|t| t.object.as_iri())
        .map(|s| s.to_string())
        .collect();
    if !allowed_iris.iter().any(|s| s == status_iri) {
        let allowed_labels: Vec<String> = allowed_iris.iter()
            .map(|iri| {
                get_literal_property(conn, iri, "rdfs:label")
                    .ok()
                    .flatten()
                    .map(|label| format!("{} ({})", label, iri))
                    .unwrap_or_else(|| iri.clone())
            })
            .collect();
        let class_label = get_literal_property(conn, class_iri, "rdfs:label")?
            .unwrap_or_else(|| class_iri.to_string());
        return Err(OwlError::ValidationError(format!(
            "Status '{}' is not allowed for concept '{}'. Accepted statuses: {}",
            status_iri, class_label, allowed_labels.join(", ")
        )));
    }
    Ok(())
}

/// Resolves icon and color for a status IRI, following `foundation:parentStatus` recursively
/// when either is absent on the status itself.
pub fn resolve_status_appearance(
    conn: &Connection,
    status_iri: &str,
) -> (Option<String>, Option<String>) {
    let mut current = status_iri.to_string();
    let mut icon: Option<String> = None;
    let mut color: Option<String> = None;

    loop {
        if icon.is_none() {
            icon = {
                query::get_by_entity_predicate(conn, &current, "foundation:hasIcon")
                    .ok()
                    .and_then(|r| {
                        r.triples.first().and_then(|t| match &t.object {
                            Object::Iri(icon_iri) => icon_iri_to_display(conn, icon_iri),
                            Object::Literal { value, .. } =>
                                Some(crate::owl::icon_literal_to_display(value)),
                            _ => None,
                        })
                    })
            };
        }
        if color.is_none() {
            color = get_literal_property(conn, &current, "foundation:color").ok().flatten();
        }

        if icon.is_some() && color.is_some() {
            break;
        }

        match get_iri_property(conn, &current, "foundation:parentStatus").ok().flatten() {
            Some(parent) if parent != current => current = parent,
            _ => break,
        }
    }

    (icon, color)
}

/// Finds the first property value of the entity that is an instance of `foundation:Status`.
/// Returns `(iri, label, color, icon)` if a status is found.
/// Color and icon are resolved recursively via `foundation:parentStatus` if absent.
pub fn get_entity_status_info(
    conn: &Connection,
    entity_iri: &str,
) -> Option<(String, String, Option<String>, Option<String>)> {
    let result = query::get_by_entity(conn, entity_iri).ok()?;
    for triple in &result.triples {
        if let Some(iri) = triple.object.as_iri() {
            if is_instance_of(conn, iri, "foundation:Status") {
                let thing = Thing::get(conn, iri);
                let (icon, color) = resolve_status_appearance(conn, iri);
                return Some((iri.to_string(), thing.label, color, icon));
            }
        }
    }
    None
}
