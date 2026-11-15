use crate::owl::{self, Connection, PropertyValueCursor};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const MAX_PROPERTIES_PER_SNAPSHOT: usize = 100;
const MAX_PROPERTY_PAGE_SIZE: usize = 100;
const MAX_PROJECTED_PROPERTIES_PER_REFERENCE: usize = 25;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPropertySnapshot {
    pub entity_iri: String,
    pub snapshot_tx: i64,
    pub properties: Vec<SnapshotPropertyPage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPropertyPage {
    pub entity_iri: String,
    pub property_iri: String,
    pub property_label: String,
    pub unit_iri: Option<String>,
    pub unit_label: Option<String>,
    pub items: Vec<SnapshotPropertyValue>,
    pub has_more: bool,
    pub next_cursor: Option<PropertyValueCursor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPropertyValue {
    pub value_type: String,
    pub value: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
    pub entity_kind: Option<super::entity_kind::EntityKind>,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub status: Option<SnapshotStatusInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected_properties: Option<Vec<SnapshotPropertyPage>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStatusInfo {
    pub iri: String,
    pub label: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// Reads only the requested forward properties from one immutable-store revision.
/// The returned cursors are bound to this entity, predicate, and snapshot.
pub fn get_entity_property_snapshot(
    conn: &Connection,
    entity_iri: &str,
    property_iris: &[String],
    requested_snapshot_tx: Option<i64>,
    page_size: usize,
    property_projections: Option<&HashMap<String, Vec<String>>>,
) -> Result<EntityPropertySnapshot, String> {
    if property_iris.len() > MAX_PROPERTIES_PER_SNAPSHOT {
        return Err(format!(
            "A semantic widget snapshot supports at most {MAX_PROPERTIES_PER_SNAPSHOT} properties"
        ));
    }
    if page_size == 0 {
        return Err("Property page size must be greater than zero".to_string());
    }
    if page_size > MAX_PROPERTY_PAGE_SIZE {
        return Err(format!(
            "Property page size cannot exceed {MAX_PROPERTY_PAGE_SIZE}"
        ));
    }

    let snapshot_tx = match requested_snapshot_tx {
        Some(snapshot_tx) => snapshot_tx,
        None => owl::current_snapshot_tx(conn)?,
    };
    if snapshot_tx < 0 || snapshot_tx > owl::current_snapshot_tx(conn)? {
        return Err(format!("Invalid entity snapshot revision: {snapshot_tx}"));
    }
    let mut seen = HashSet::with_capacity(property_iris.len());
    let mut properties = Vec::with_capacity(property_iris.len());
    let requested_properties = property_iris
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if let Some(projections) = property_projections {
        for (property_iri, projected_property_iris) in projections {
            if !requested_properties.contains(property_iri.as_str()) {
                return Err(format!(
                    "Projection property '{property_iri}' must also be requested in the snapshot"
                ));
            }
            validate_projection_properties(projected_property_iris)?;
        }
    }
    for property_iri in property_iris {
        if !seen.insert(property_iri.as_str()) {
            return Err(format!("Duplicate property IRI: {property_iri}"));
        }
        let projection_property_iris = property_projections
            .and_then(|projections| projections.get(property_iri))
            .map(Vec::as_slice);
        let page = get_property_page_with_projection(
            conn,
            entity_iri,
            property_iri,
            page_size,
            snapshot_tx,
            None,
            projection_property_iris,
        )?;
        properties.push(page);
    }

    Ok(EntityPropertySnapshot {
        entity_iri: entity_iri.to_string(),
        snapshot_tx,
        properties,
    })
}

pub fn get_property_page(
    conn: &Connection,
    entity_iri: &str,
    property_iri: &str,
    page_size: usize,
    snapshot_tx: i64,
    cursor: Option<&PropertyValueCursor>,
) -> Result<SnapshotPropertyPage, String> {
    get_property_page_with_projection(
        conn,
        entity_iri,
        property_iri,
        page_size,
        snapshot_tx,
        cursor,
        None,
    )
}

pub fn get_property_page_with_projection(
    conn: &Connection,
    entity_iri: &str,
    property_iri: &str,
    page_size: usize,
    snapshot_tx: i64,
    cursor: Option<&PropertyValueCursor>,
    projection_property_iris: Option<&[String]>,
) -> Result<SnapshotPropertyPage, String> {
    if page_size == 0 {
        return Err("Property page size must be greater than zero".to_string());
    }
    if page_size > MAX_PROPERTY_PAGE_SIZE {
        return Err(format!(
            "Property page size cannot exceed {MAX_PROPERTY_PAGE_SIZE}"
        ));
    }
    if snapshot_tx < 0 || snapshot_tx > owl::current_snapshot_tx(conn)? {
        return Err(format!("Invalid entity snapshot revision: {snapshot_tx}"));
    }
    if let Some(property_iris) = projection_property_iris {
        validate_projection_properties(property_iris)?;
    }
    let page = owl::get_property_values_page(
        conn,
        entity_iri,
        property_iri,
        page_size,
        snapshot_tx,
        cursor,
    )?;
    let property_label =
        owl::get_literal_property_as_of(conn, property_iri, "rdfs:label", snapshot_tx)?
            .unwrap_or_else(|| property_iri.to_string());
    let unit_iri = owl::get_iri_property_as_of(conn, property_iri, "qudt:unit", snapshot_tx)?;
    let unit_label = match unit_iri.as_deref() {
        Some(unit_iri) => {
            owl::get_literal_property_as_of(conn, unit_iri, "qudt:currencyCode", snapshot_tx)?.or(
                owl::get_literal_property_as_of(conn, unit_iri, "rdfs:label", snapshot_tx)?,
            )
        }
        None => None,
    };
    let items = page
        .items
        .into_iter()
        .map(|triple| match triple.object {
            owl::Object::Iri(value) => {
                let mut reference = reference_value(conn, value, snapshot_tx)?;
                if let Some(projected_property_iris) = projection_property_iris {
                    reference.projected_properties = Some(
                        projected_property_iris
                            .iter()
                            .map(|projected_property_iri| {
                                get_property_page_with_projection(
                                    conn,
                                    &reference.value,
                                    projected_property_iri,
                                    page_size,
                                    snapshot_tx,
                                    None,
                                    None,
                                )
                            })
                            .collect::<Result<Vec<_>, String>>()?,
                    );
                }
                Ok(reference)
            }
            owl::Object::Blank(value) => Ok(SnapshotPropertyValue {
                value_type: "blank".to_string(),
                value,
                datatype: None,
                language: None,
                entity_kind: None,
                label: None,
                icon: None,
                status: None,
                projected_properties: None,
            }),
            owl::Object::Literal {
                value,
                datatype,
                language,
            } => literal_value(value, datatype, language),
            owl::Object::Integer(value) => {
                literal_value(value.to_string(), Some("xsd:integer".to_string()), None)
            }
            owl::Object::Number(value) => {
                literal_value(value.to_string(), Some("xsd:decimal".to_string()), None)
            }
            owl::Object::Boolean(value) => {
                literal_value(value.to_string(), Some("xsd:boolean".to_string()), None)
            }
            owl::Object::DateTime(value) => {
                literal_value(value, Some("xsd:dateTime".to_string()), None)
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SnapshotPropertyPage {
        entity_iri: entity_iri.to_string(),
        property_iri: property_iri.to_string(),
        property_label,
        unit_iri,
        unit_label,
        items,
        has_more: page.has_more,
        next_cursor: page.next_cursor,
    })
}

fn literal_value(
    value: String,
    datatype: Option<String>,
    language: Option<String>,
) -> Result<SnapshotPropertyValue, String> {
    Ok(SnapshotPropertyValue {
        value_type: "literal".to_string(),
        value,
        datatype,
        language,
        entity_kind: None,
        label: None,
        icon: None,
        status: None,
        projected_properties: None,
    })
}

fn reference_value(
    conn: &Connection,
    value: String,
    snapshot_tx: i64,
) -> Result<SnapshotPropertyValue, String> {
    let label = owl::get_literal_property_as_of(conn, &value, "rdfs:label", snapshot_tx)?
        .unwrap_or_else(|| value.clone());
    let icon = owl::get_property_values_as_of(conn, &value, "foundation:hasIcon", snapshot_tx)?
        .into_iter()
        .next()
        .and_then(|icon| match (icon.iri, icon.literal) {
            (Some(iri), _) => owl::icon_iri_to_display(conn, &iri),
            (_, Some(icon)) => Some(owl::icon_literal_to_display(&icon)),
            (None, None) => None,
        });
    let types = owl::get_all_iri_properties_as_of(conn, &value, "rdf:type", snapshot_tx)?;
    let entity_kind = if types
        .iter()
        .any(|class_iri| class_iri == "owl:Class" || class_iri == "rdfs:Class")
    {
        Some(super::entity_kind::EntityKind::Class)
    } else if types.is_empty() {
        None
    } else {
        Some(super::entity_kind::EntityKind::Individual)
    };
    let status_iri =
        owl::get_iri_property_as_of(conn, &value, "foundation:hasStatus", snapshot_tx)?;
    let status = match status_iri {
        Some(status_iri)
            if owl::get_all_iri_properties_as_of(conn, &status_iri, "rdf:type", snapshot_tx)?
                .iter()
                .any(|class_iri| class_iri == "foundation:Status") =>
        {
            let (label, color, icon) =
                super::status::get_status_info_as_of(conn, &status_iri, snapshot_tx)?;
            Some(SnapshotStatusInfo {
                iri: status_iri,
                label,
                color,
                icon,
            })
        }
        _ => None,
    };
    Ok(SnapshotPropertyValue {
        value_type: "iri".to_string(),
        value,
        datatype: None,
        language: None,
        entity_kind,
        label: Some(label),
        icon,
        status,
        projected_properties: None,
    })
}

fn validate_projection_properties(property_iris: &[String]) -> Result<(), String> {
    if property_iris.len() > MAX_PROJECTED_PROPERTIES_PER_REFERENCE {
        return Err(format!(
            "A semantic widget reference projection supports at most {MAX_PROJECTED_PROPERTIES_PER_REFERENCE} properties"
        ));
    }
    let mut seen = HashSet::with_capacity(property_iris.len());
    for property_iri in property_iris {
        if !seen.insert(property_iri.as_str()) {
            return Err(format!("Duplicate projected property IRI: {property_iri}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Object, Triple};

    #[test]
    fn requested_properties_and_pages_remain_at_the_initial_snapshot() {
        let mut conn = setup_test_db();
        let values = (1..=3)
            .map(|number| {
                Triple::new(
                    "test:entity",
                    "test:property",
                    Object::Literal {
                        value: number.to_string(),
                        datatype: Some("xsd:integer".to_string()),
                        language: None,
                    },
                )
            })
            .collect::<Vec<_>>();
        store::assert_triples(&mut conn, &values, "semantic-widget-test").unwrap();

        let first_snapshot = get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into()],
            None,
            2,
            None,
        )
        .unwrap();
        let first_page = &first_snapshot.properties[0];
        assert_eq!(first_page.items.len(), 2);
        assert!(first_page.has_more);
        let cursor = first_page.next_cursor.clone().unwrap();

        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "test:entity",
                "test:property",
                Object::Literal {
                    value: "4".to_string(),
                    datatype: Some("xsd:integer".to_string()),
                    language: None,
                },
            )],
            "semantic-widget-test",
        )
        .unwrap();

        let second_page = get_property_page(
            &conn,
            "test:entity",
            "test:property",
            2,
            first_snapshot.snapshot_tx,
            Some(&cursor),
        )
        .unwrap();
        assert_eq!(second_page.items.len(), 1);
        assert_eq!(second_page.items[0].value, "3");
        assert!(!second_page.has_more);
    }

    #[test]
    fn snapshot_rejects_duplicate_properties_and_zero_page_size() {
        let conn = setup_test_db();
        assert!(get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into(), "test:property".into()],
            None,
            5,
            None,
        )
        .is_err());
        assert!(get_entity_property_snapshot(&conn, "test:entity", &[], None, 0, None).is_err());
    }

    #[test]
    fn reference_values_include_friendly_metadata_from_the_snapshot() {
        let mut conn = setup_test_db();
        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "test:property",
                    "rdfs:label",
                    Object::Literal {
                        value: "Owner".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:entity",
                    "test:property",
                    Object::Iri("test:owner".into()),
                ),
                Triple::new(
                    "test:owner",
                    "rdf:type",
                    Object::Iri("owl:NamedIndividual".into()),
                ),
                Triple::new(
                    "test:owner",
                    "rdfs:label",
                    Object::Literal {
                        value: "Ada Lovelace".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:owner",
                    "foundation:hasIcon",
                    Object::Iri(owl::icon_name_to_iri("person")),
                ),
                Triple::new(
                    "test:owner",
                    "foundation:hasStatus",
                    Object::Iri("test:active".into()),
                ),
                Triple::new(
                    "test:active",
                    "rdf:type",
                    Object::Iri("foundation:Status".into()),
                ),
                Triple::new(
                    "test:active",
                    "rdfs:label",
                    Object::Literal {
                        value: "Active".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:active",
                    "foundation:color",
                    Object::Literal {
                        value: "green".into(),
                        datatype: None,
                        language: None,
                    },
                ),
            ],
            "semantic-widget-test",
        )
        .unwrap();

        let snapshot = get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into()],
            None,
            5,
            None,
        )
        .unwrap();
        let property = &snapshot.properties[0];
        assert_eq!(property.property_label, "Owner");
        let owner = &property.items[0];
        assert_eq!(owner.label.as_deref(), Some("Ada Lovelace"));
        assert_eq!(owner.icon.as_deref(), Some("person"));
        assert_eq!(
            owner.entity_kind,
            Some(super::super::entity_kind::EntityKind::Individual)
        );
        let status = owner.status.as_ref().unwrap();
        assert_eq!(status.label, "Active");
        assert_eq!(status.color.as_deref(), Some("green"));
    }

    #[test]
    fn reference_projection_exposes_units_and_keeps_all_pages_on_one_snapshot() {
        let mut conn = setup_test_db();
        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "test:entity",
                    "test:payments",
                    Object::Iri("test:payment-1".into()),
                ),
                Triple::new(
                    "test:entity",
                    "test:payments",
                    Object::Iri("test:payment-2".into()),
                ),
                Triple::new(
                    "test:payments",
                    "rdfs:label",
                    Object::Literal {
                        value: "Pagamentos".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:amount",
                    "rdfs:label",
                    Object::Literal {
                        value: "Valor".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:amount",
                    "qudt:unit",
                    Object::Iri("currency:BRL".into()),
                ),
                Triple::new(
                    "currency:BRL",
                    "qudt:currencyCode",
                    Object::Literal {
                        value: "BRL".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new("test:payment-1", "test:amount", Object::Number(10.5)),
                Triple::new("test:payment-2", "test:amount", Object::Number(20.0)),
            ],
            "semantic-widget-test",
        )
        .unwrap();

        let projections =
            HashMap::from([("test:payments".to_string(), vec!["test:amount".to_string()])]);
        let first_snapshot = get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:payments".into()],
            None,
            1,
            Some(&projections),
        )
        .unwrap();
        let first_page = &first_snapshot.properties[0];
        assert_eq!(first_page.entity_iri, "test:entity");
        assert!(first_page.has_more);
        let first_payment = &first_page.items[0];
        assert_eq!(first_payment.value, "test:payment-1");
        let amount = &first_payment.projected_properties.as_ref().unwrap()[0];
        assert_eq!(amount.entity_iri, "test:payment-1");
        assert_eq!(amount.unit_iri.as_deref(), Some("currency:BRL"));
        assert_eq!(amount.unit_label.as_deref(), Some("BRL"));
        assert_eq!(amount.items[0].value, "10.5");

        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "test:payment-2",
                "test:amount",
                Object::Number(99.0),
            )],
            "semantic-widget-test",
        )
        .unwrap();

        let second_page = get_property_page_with_projection(
            &conn,
            "test:entity",
            "test:payments",
            1,
            first_snapshot.snapshot_tx,
            first_page.next_cursor.as_ref(),
            Some(&projections["test:payments"]),
        )
        .unwrap();
        assert_eq!(second_page.items[0].value, "test:payment-2");
        assert_eq!(
            second_page.items[0].projected_properties.as_ref().unwrap()[0].items[0].value,
            "20"
        );
    }

    #[test]
    fn projection_rejects_unrequested_parent_properties_and_duplicates() {
        let conn = setup_test_db();
        let extra_parent = HashMap::from([("test:other".to_string(), vec![])]);
        assert!(get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into()],
            None,
            5,
            Some(&extra_parent),
        )
        .is_err());
        let duplicate_projection = HashMap::from([(
            "test:property".to_string(),
            vec!["test:value".to_string(), "test:value".to_string()],
        )]);
        assert!(get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into()],
            None,
            5,
            Some(&duplicate_projection),
        )
        .is_err());
    }
}
