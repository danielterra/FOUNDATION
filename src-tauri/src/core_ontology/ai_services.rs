use crate::owl::{self, Connection, Individual, Object, Property, PropertyType};
use uuid::Uuid;

const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";

pub fn sync(conn: &mut Connection) -> Result<usize, String> {
    ensure_ai_service_credential_domain(conn)?;
    ensure_model_catalog_properties(conn)?;
    if Property::get(conn, "foundation:aiAuthenticationMode")
        .map_err(|error| error.to_string())?
        .is_none()
    {
        Property::new("foundation:aiAuthenticationMode")
            .assert(
                conn,
                PropertyType::DatatypeProperty,
                "AI authentication mode",
                None,
                &["foundation:AIAPIService"],
                Some("xsd:string"),
                None,
                "core_ontology_sync",
            )
            .map_err(|error| error.to_string())?;
    }

    let mut created = 0;
    created += ensure_service(
        conn,
        "OpenAI com ChatGPT",
        "chatgpt_plan",
        "Use the user's eligible ChatGPT plan through the public OpenAI Responses API. No API key billing or provider fallback is used.",
        "https://chatgpt.com",
    )? as usize;
    created += ensure_service(
        conn,
        "OpenAI API",
        "api_key",
        "Use an OpenAI API key with usage billed to the user's API account.",
        "https://platform.openai.com/docs/api-reference/responses",
    )? as usize;
    Ok(created)
}

fn ensure_model_catalog_properties(conn: &mut Connection) -> Result<(), String> {
    for (iri, label, comment, range) in [
        (
            "foundation:modelCatalogStatus",
            "Model Catalog Status",
            "Freshness state of the model catalog for this API service (current or stale)",
            "xsd:string",
        ),
        (
            "foundation:modelCatalogLastSuccessAt",
            "Model Catalog Last Success At",
            "Time when the complete model catalog was last synchronized successfully",
            "xsd:dateTime",
        ),
        (
            "foundation:modelCatalogLastAttemptAt",
            "Model Catalog Last Attempt At",
            "Time when the most recent model catalog synchronization attempt started",
            "xsd:dateTime",
        ),
        (
            "foundation:modelCatalogSyncError",
            "Model Catalog Sync Error",
            "Most recent model catalog synchronization error, without credentials or response payloads",
            "xsd:string",
        ),
    ] {
        if Property::get(conn, iri)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            Property::new(iri)
                .assert(
                    conn,
                    PropertyType::DatatypeProperty,
                    label,
                    Some(comment),
                    &["foundation:AIAPIService"],
                    Some(range),
                    None,
                    "core_ontology_sync",
                )
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn ensure_ai_service_credential_domain(conn: &mut Connection) -> Result<(), String> {
    let property = Property::get(conn, "foundation:hasCredential")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "foundation:hasCredential is missing from the ontology".to_string())?;
    if property
        .domains
        .iter()
        .any(|domain| domain == "foundation:AIAPIService")
    {
        return Ok(());
    }
    owl::assert_raw_triples_with_tx(
        conn,
        &[owl::Triple::new(
            "foundation:hasCredential",
            "rdfs:domain",
            Object::Iri("foundation:AIAPIService".to_string()),
        )],
        "core_ontology_sync",
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub fn resolve_openai_service(
    conn: &Connection,
    authentication_mode: &str,
) -> Result<String, String> {
    find_openai_service(conn, authentication_mode)?.ok_or_else(|| {
        format!(
            "No OpenAI API service is configured for authentication mode '{authentication_mode}'"
        )
    })
}

fn find_openai_service(
    conn: &Connection,
    authentication_mode: &str,
) -> Result<Option<String>, String> {
    let candidates = Individual::find_by_class_and_properties(
        conn,
        "foundation:AIAPIService",
        &[
            ("foundation:aiAuthenticationMode", authentication_mode),
            ("foundation:apiBaseUrl", OPENAI_API_BASE_URL),
        ],
    )
    .map_err(|error| error.to_string())?
    .into_iter()
    .filter_map(|iri| match Individual::get(conn, &iri) {
        Ok(Some(service)) if service_has_openai_endpoint_metadata(&service) => Some(Ok(iri)),
        Ok(_) => None,
        Err(error) => Some(Err(error.to_string())),
    })
    .collect::<Result<Vec<_>, _>>()?;
    match candidates.as_slice() {
        [service] => Ok(Some(service.clone())),
        [] => Ok(None),
        _ => Err(format!(
            "Multiple OpenAI API services match authentication mode '{authentication_mode}'; resolve the duplicate service records before continuing"
        )),
    }
}

fn service_has_openai_endpoint_metadata(service: &Individual) -> bool {
    service.properties.iter().any(|(predicate, object)| {
        matches!(
            predicate.as_str(),
            "foundation:apiBaseUrl" | "foundation:apiDocumentation" | "foundation:apiEndpoint"
        ) && match object {
            Object::Literal { value, .. } => value.contains("openai.com"),
            Object::Iri(value) => value.contains("openai.com"),
            _ => false,
        }
    })
}

fn ensure_service(
    conn: &mut Connection,
    label: &str,
    authentication_mode: &str,
    description: &str,
    documentation_url: &str,
) -> Result<bool, String> {
    if find_openai_service(conn, authentication_mode)?.is_some() {
        return Ok(false);
    }

    let service_iri = format!("foundation:AIAPIService_{}", Uuid::new_v4());

    let service = Individual::new(&service_iri);
    service
        .assert(
            conn,
            "foundation:AIAPIService",
            label,
            "smart_toy",
            "core_ontology_sync",
        )
        .map_err(|error| error.to_string())?;
    service
        .add_property(
            conn,
            "foundation:hasStatus",
            vec![Object::Iri("foundation:Pending".to_string())],
            "core_ontology_sync",
        )
        .map_err(|error| error.to_string())?;
    for (property, value) in [
        ("foundation:aiAuthenticationMode", authentication_mode),
        ("foundation:apiBaseUrl", OPENAI_API_BASE_URL),
        ("foundation:apiDocumentation", documentation_url),
        ("foundation:seeAlsoUrl", "https://openai.com"),
        ("rdfs:comment", description),
    ] {
        service
            .add_property(
                conn,
                property,
                vec![Object::Literal {
                    value: value.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "core_ontology_sync",
            )
            .map_err(|error| error.to_string())?;
    }
    if !owl::find_entities_with_property(conn, "rdf:type", "foundation:AIAPIService")
        .map_err(|error| error.to_string())?
        .iter()
        .any(|iri| iri == &service_iri)
    {
        return Err(format!("AI service {service_iri} was not persisted"));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    fn add_service(conn: &mut Connection, iri: &str, mode: &str) {
        owl::assert_raw_triples_with_tx(
            conn,
            &[
                owl::Triple::new(
                    iri,
                    "rdf:type",
                    Object::Iri("foundation:AIAPIService".to_string()),
                ),
                owl::Triple::new(
                    iri,
                    "foundation:aiAuthenticationMode",
                    Object::Literal {
                        value: mode.to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                owl::Triple::new(
                    iri,
                    "foundation:apiBaseUrl",
                    Object::Literal {
                        value: OPENAI_API_BASE_URL.to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                owl::Triple::new(
                    iri,
                    "foundation:apiDocumentation",
                    Object::Literal {
                        value: "https://developers.openai.com/api/docs".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
            ],
            "test",
        )
        .unwrap();
    }

    #[test]
    fn resolves_one_service_by_authentication_mode_and_endpoint() {
        let mut conn = setup_test_db();
        add_service(&mut conn, "foundation:service_chatgpt", "chatgpt_plan");
        add_service(&mut conn, "foundation:service_api", "api_key");

        assert_eq!(
            resolve_openai_service(&conn, "chatgpt_plan").unwrap(),
            "foundation:service_chatgpt"
        );
        assert_eq!(
            resolve_openai_service(&conn, "api_key").unwrap(),
            "foundation:service_api"
        );
    }

    #[test]
    fn reports_missing_and_ambiguous_service_records() {
        let mut conn = setup_test_db();
        assert!(resolve_openai_service(&conn, "chatgpt_plan")
            .unwrap_err()
            .contains("No OpenAI API service"));

        add_service(&mut conn, "foundation:service_a", "chatgpt_plan");
        add_service(&mut conn, "foundation:service_b", "chatgpt_plan");
        assert!(resolve_openai_service(&conn, "chatgpt_plan")
            .unwrap_err()
            .contains("Multiple OpenAI API services"));
    }
}
