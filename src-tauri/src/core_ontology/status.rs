use crate::owl::{
    get_all_iri_properties, get_iri_property, get_literal_property, icon_iri_to_display,
    icon_literal_to_display, is_instance_of, Class, Connection, Individual, OwlError, Result,
    Thing,
};
use std::collections::HashSet;

pub fn resolve_active_status(conn: &Connection) -> std::result::Result<String, String> {
    resolve_status_by_label(conn, "Ativo")
}

fn resolve_status_by_label(
    conn: &Connection,
    expected_label: &str,
) -> std::result::Result<String, String> {
    let candidates =
        Class::get_instances(conn, "foundation:Status").map_err(|error| error.to_string())?;
    let mut matches = Vec::new();
    for iri in candidates {
        if get_literal_property(conn, &iri, "rdfs:label")
            .map_err(|error| error.to_string())?
            .as_deref()
            == Some(expected_label)
        {
            matches.push(iri);
        }
    }

    match matches.as_slice() {
        [status_iri] => Ok(status_iri.clone()),
        [] => Err(format!(
            "Ontology has no foundation:Status individual labeled '{expected_label}'"
        )),
        _ => Err(format!(
            "Ontology has multiple foundation:Status individuals labeled '{expected_label}': {}",
            matches.join(", ")
        )),
    }
}

/// Validates that `status_iri` is in the `foundation:allowedStatus` list of `class_iri`.
pub fn validate_allowed_status(conn: &Connection, class_iri: &str, status_iri: &str) -> Result<()> {
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
        let allowed_labels: Vec<String> = allowed_iris
            .iter()
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
            status_iri,
            class_label,
            allowed_labels.join(", ")
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
            color = get_literal_property(conn, &current, "foundation:color")
                .ok()
                .flatten();
        }
        if icon.is_some() && color.is_some() {
            break;
        }
        match get_iri_property(conn, &current, "foundation:parentStatus")
            .ok()
            .flatten()
        {
            Some(parent) if parent != current => current = parent,
            _ => break,
        }
    }

    let elapsed = t0.elapsed().as_millis();
    if elapsed > 5 || hops > 1 {
        crate::commands::log_backend(
            "debug",
            &format!(
                "[CORE] status::resolve({}) {} hops {}ms",
                status_iri, hops, elapsed
            ),
        );
    }
    (icon, color)
}

/// Resolves a status label, icon, and color from one immutable-store snapshot.
pub fn get_status_info_as_of(
    conn: &Connection,
    status_iri: &str,
    snapshot_tx: i64,
) -> std::result::Result<(String, Option<String>, Option<String>), String> {
    let label =
        crate::owl::get_literal_property_as_of(conn, status_iri, "rdfs:label", snapshot_tx)?
            .unwrap_or_else(|| status_iri.to_string());
    let (icon, color) = resolve_status_appearance_as_of(conn, status_iri, snapshot_tx)?;
    Ok((label, color, icon))
}

/// Resolves inherited appearance using only values visible at `snapshot_tx`.
pub fn resolve_status_appearance_as_of(
    conn: &Connection,
    status_iri: &str,
    snapshot_tx: i64,
) -> std::result::Result<(Option<String>, Option<String>), String> {
    let mut current = status_iri.to_string();
    let mut visited = HashSet::new();
    let mut icon = None;
    let mut color = None;

    while visited.insert(current.clone()) {
        if icon.is_none() {
            icon = crate::owl::get_property_values_as_of(
                conn,
                &current,
                "foundation:hasIcon",
                snapshot_tx,
            )?
            .into_iter()
            .next()
            .and_then(|value| match (value.iri, value.literal) {
                (Some(iri), _) => icon_iri_to_display(conn, &iri),
                (_, Some(value)) => Some(icon_literal_to_display(&value)),
                (None, None) => None,
            });
        }
        if color.is_none() {
            color = crate::owl::get_literal_property_as_of(
                conn,
                &current,
                "foundation:color",
                snapshot_tx,
            )?;
        }
        if icon.is_some() && color.is_some() {
            break;
        }
        let Some(parent) = crate::owl::get_iri_property_as_of(
            conn,
            &current,
            "foundation:parentStatus",
            snapshot_tx,
        )?
        else {
            break;
        };
        current = parent;
    }

    Ok((icon, color))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Object, Triple};
    use crate::owl::ClassType;

    fn seed_status_class(conn: &mut Connection) {
        if !Class::exists(conn, "foundation:Status") {
            Class::new("foundation:Status")
                .assert(
                    conn,
                    ClassType::OwlClass,
                    "Status",
                    "category",
                    None,
                    "test",
                )
                .unwrap();
        }
    }

    fn seed_status(conn: &mut Connection, iri: &str, label: &str) {
        Individual::new(iri)
            .assert_without_icon(conn, "foundation:Status", label, "test")
            .unwrap();
    }

    #[test]
    fn resolves_status_by_unique_label() {
        let mut conn = setup_test_db();
        seed_status_class(&mut conn);
        seed_status(&mut conn, "test:ActiveStatus", "Ativo");

        assert_eq!(resolve_active_status(&conn).unwrap(), "test:ActiveStatus");
    }

    #[test]
    fn reports_missing_status_label() {
        let mut conn = setup_test_db();
        seed_status_class(&mut conn);

        let error = resolve_status_by_label(&conn, "Missing test status").unwrap_err();
        assert!(error.contains("no foundation:Status individual"));
    }

    #[test]
    fn rejects_ambiguous_status_label() {
        let mut conn = setup_test_db();
        seed_status_class(&mut conn);
        seed_status(&mut conn, "test:ActiveStatusOne", "Ativo");
        seed_status(&mut conn, "test:ActiveStatusTwo", "Ativo");

        let error = resolve_active_status(&conn).unwrap_err();
        assert!(error.contains("multiple foundation:Status individuals"));
        assert!(error.contains("test:ActiveStatusOne"));
        assert!(error.contains("test:ActiveStatusTwo"));
    }

    #[test]
    fn status_info_keeps_execution_and_step_values_at_the_requested_snapshot() {
        let mut conn = setup_test_db();
        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "test:ExecutionStatus",
                    "rdfs:label",
                    Object::Literal {
                        value: "Old execution label".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(
                    "test:ExecutionStatus",
                    "foundation:parentStatus",
                    Object::Iri("test:ParentStatus".to_string()),
                ),
                Triple::new(
                    "test:StepStatus",
                    "rdfs:label",
                    Object::Literal {
                        value: "Old step label".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(
                    "test:ParentStatus",
                    "foundation:color",
                    Object::Literal {
                        value: "cyan-old".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(
                    "test:ParentStatus",
                    "foundation:hasIcon",
                    Object::Iri("foundation:icon-material-symbols-name-check".to_string()),
                ),
            ],
            "status-as-of-test",
        )
        .unwrap();
        let snapshot_tx = crate::owl::current_snapshot_tx(&conn).unwrap();

        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "test:ExecutionStatus",
                    "rdfs:label",
                    Object::Literal {
                        value: "New execution label".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(
                    "test:StepStatus",
                    "rdfs:label",
                    Object::Literal {
                        value: "New step label".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(
                    "test:ParentStatus",
                    "foundation:color",
                    Object::Literal {
                        value: "orange-new".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(
                    "test:ParentStatus",
                    "foundation:hasIcon",
                    Object::Iri("foundation:icon-material-symbols-name-close".to_string()),
                ),
            ],
            "status-as-of-test",
        )
        .unwrap();

        assert_eq!(
            get_status_info_as_of(&conn, "test:ExecutionStatus", snapshot_tx).unwrap(),
            (
                "Old execution label".to_string(),
                Some("cyan-old".to_string()),
                Some("check".to_string())
            )
        );
        assert_eq!(
            get_status_info_as_of(&conn, "test:StepStatus", snapshot_tx)
                .unwrap()
                .0,
            "Old step label"
        );
        assert_eq!(
            get_status_info_as_of(
                &conn,
                "test:ExecutionStatus",
                crate::owl::current_snapshot_tx(&conn).unwrap()
            )
            .unwrap(),
            (
                "New execution label".to_string(),
                Some("orange-new".to_string()),
                Some("close".to_string())
            )
        );
    }
}
