use crate::owl::{
    Connection, Result, OwlError, Thing,
    get_literal_property, get_iri_property, get_all_iri_properties, is_instance_of,
    icon_iri_to_display, icon_literal_to_display,
    Individual,
};

/// Validates that `status_iri` is in the `foundation:allowedStatus` list of `class_iri`.
pub fn validate_allowed_status(
    conn: &Connection,
    class_iri: &str,
    status_iri: &str,
) -> Result<()> {
    let allowed_iris = get_all_iri_properties(conn, class_iri, "foundation:allowedStatus")?;
    if allowed_iris.is_empty() {
        let class_label = get_literal_property(conn, class_iri, "rdfs:label")?
            .unwrap_or_else(|| class_iri.to_string());
        return Err(OwlError::ValidationError(format!(
            "Concept '{}' has no statuses configured. Every concept must have at least one allowed status. Use learn_concepts to add allowedStatuses to '{}'.",
            class_label, class_iri
        )));
    }
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

/// Resolves icon and color for a status IRI, following `foundation:parentStatus` recursively.
pub fn resolve_status_appearance(
    conn: &Connection,
    status_iri: &str,
) -> (Option<String>, Option<String>) {
    let t0 = std::time::Instant::now();
    let mut current = status_iri.to_string();
    let mut icon: Option<String> = None;
    let mut color: Option<String> = None;
    let mut hops = 0usize;

    loop {
        hops += 1;
        if icon.is_none() {
            icon = get_iri_property(conn, &current, "foundation:hasIcon")
                .ok()
                .flatten()
                .and_then(|icon_iri| icon_iri_to_display(conn, &icon_iri))
                .or_else(|| {
                    get_literal_property(conn, &current, "foundation:hasIcon")
                        .ok()
                        .flatten()
                        .map(|v| icon_literal_to_display(&v))
                });
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

    let elapsed = t0.elapsed().as_millis();
    if elapsed > 5 || hops > 1 {
        crate::commands::log_backend("debug", &format!(
            "[CORE] status::resolve({}) {} hops {}ms",
            status_iri, hops, elapsed
        ));
    }
    (icon, color)
}

/// Finds the `foundation:hasStatus` of `entity_iri`.
/// Returns `(iri, label, color, icon)` if a status is found.
pub fn get_entity_status_info(
    conn: &Connection,
    entity_iri: &str,
) -> Option<(String, String, Option<String>, Option<String>)> {
    // Use Individual::get so we go through OWL — never call eavto directly.
    let individual = Individual::get(conn, entity_iri).ok()??;
    for (_, value) in &individual.properties {
        if let Some(iri) = value.as_iri() {
            if is_instance_of(conn, iri, "foundation:Status") {
                let thing = Thing::get(conn, iri);
                let (icon, color) = resolve_status_appearance(conn, iri);
                return Some((iri.to_string(), thing.label, color, icon));
            }
        }
    }
    None
}
