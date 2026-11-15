use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

use crate::owl::{self, DbExecutor, Individual, Object};

const OPENAI_API_HOST: &str = "api.openai.com";
const STATUS_PROPERTY: &str = "foundation:modelCatalogStatus";
const LAST_SUCCESS_PROPERTY: &str = "foundation:modelCatalogLastSuccessAt";
const LAST_ATTEMPT_PROPERTY: &str = "foundation:modelCatalogLastAttemptAt";
const SYNC_ERROR_PROPERTY: &str = "foundation:modelCatalogSyncError";
const DAILY_REFRESH: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Default)]
pub struct OpenAIModelCatalogCoordinator {
    lock: Arc<Mutex<()>>,
    scheduler_started: Arc<AtomicBool>,
}

pub fn start_daily_sync(app: AppHandle, executor: DbExecutor) {
    let coordinator = if let Some(state) = app.try_state::<OpenAIModelCatalogCoordinator>() {
        state.inner().clone()
    } else {
        let state = OpenAIModelCatalogCoordinator::default();
        app.manage(state.clone());
        state
    };
    if coordinator.scheduler_started.swap(true, Ordering::AcqRel) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            synchronize_all_eligible_services(&executor, &coordinator).await;
            tokio::time::sleep(DAILY_REFRESH).await;
        }
    });
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct OpenAIModelCatalog {
    object: String,
    data: Vec<OpenAIModel>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct OpenAIModel {
    id: String,
    owned_by: String,
    #[serde(default)]
    shutdown_date: Option<String>,
}

fn parse_model_catalog(body: &str) -> Result<OpenAIModelCatalog, String> {
    let catalog: OpenAIModelCatalog = serde_json::from_str(body)
        .map_err(|_| "OpenAI returned an invalid model catalog response".to_string())?;
    if catalog.object != "list" {
        return Err("OpenAI model catalog response must have object='list'".to_string());
    }
    if catalog.data.is_empty() {
        return Err(
            "OpenAI returned an empty model catalog; the previous catalog was preserved"
                .to_string(),
        );
    }

    let mut identifiers = std::collections::HashSet::with_capacity(catalog.data.len());
    for model in &catalog.data {
        if model.id.trim().is_empty() || model.owned_by.trim().is_empty() {
            return Err(
                "OpenAI model catalog contains an entry without a valid id or owner".to_string(),
            );
        }
        if !identifiers.insert(model.id.as_str()) {
            return Err(format!(
                "OpenAI model catalog contains duplicate model id '{}'",
                model.id
            ));
        }
        if let Some(shutdown_date) = &model.shutdown_date {
            DateTime::parse_from_rfc3339(shutdown_date)
                .map_err(|_| format!("OpenAI model '{}' has an invalid shutdown_date", model.id))?;
        }
    }
    Ok(catalog)
}

fn model_is_unavailable(shutdown_date: Option<&str>, now: DateTime<Utc>) -> Result<bool, String> {
    let Some(shutdown_date) = shutdown_date else {
        return Ok(false);
    };
    let date = DateTime::parse_from_rfc3339(shutdown_date)
        .map_err(|_| "OpenAI model has an invalid shutdown_date".to_string())?
        .with_timezone(&Utc);
    Ok(date <= now)
}

fn stable_model_iri(service_iri: &str, model_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(service_iri.as_bytes());
    digest.update([0]);
    digest.update(model_id.as_bytes());
    let hash = digest.finalize();
    let prefix = hash[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("foundation:AIModel_OpenAI_API_{prefix}")
}

fn is_openai_api_service(conn: &owl::Connection, service: &Individual) -> bool {
    let base_url = service
        .properties
        .iter()
        .find(|(property, _)| property == "foundation:apiBaseUrl")
        .and_then(|(_, value)| value.as_literal());
    owl::is_instance_of(conn, &service.iri, "foundation:AIAPIService")
        && base_url
            .and_then(|value| url::Url::parse(&value).ok())
            .is_some_and(|url| url.host_str() == Some(OPENAI_API_HOST))
}

async fn fetch_openai_catalog(api_key: &str) -> Result<OpenAIModelCatalog, String> {
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Unable to prepare OpenAI model catalog request: {error}"))?
        .get("https://api.openai.com/v1/models")
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|error| format!("Unable to reach OpenAI model catalog: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "OpenAI model catalog request failed with HTTP {status}"
        ));
    }
    let body = response
        .text()
        .await
        .map_err(|error| format!("Unable to read OpenAI model catalog response: {error}"))?;
    parse_model_catalog(&body)
}

fn set_literal(
    conn: &mut owl::Connection,
    iri: &str,
    property: &str,
    value: &str,
    datatype: &str,
    origin: &str,
) -> Result<(), String> {
    Individual::clear_property(conn, iri, property, origin)
        .map_err(|error| format!("Unable to replace {property} on '{iri}': {error}"))?;
    Individual::new(iri)
        .add_property(
            conn,
            property,
            vec![Object::Literal {
                value: value.to_string(),
                datatype: Some(datatype.to_string()),
                language: None,
            }],
            origin,
        )
        .map_err(|error| format!("Unable to set {property} on '{iri}': {error}"))
}

fn set_datetime(
    conn: &mut owl::Connection,
    iri: &str,
    property: &str,
    value: &str,
    origin: &str,
) -> Result<(), String> {
    Individual::clear_property(conn, iri, property, origin)
        .map_err(|error| format!("Unable to replace {property} on '{iri}': {error}"))?;
    Individual::new(iri)
        .add_property(
            conn,
            property,
            vec![Object::DateTime(value.to_string())],
            origin,
        )
        .map_err(|error| format!("Unable to set {property} on '{iri}': {error}"))
}

fn set_boolean(
    conn: &mut owl::Connection,
    iri: &str,
    property: &str,
    value: bool,
    origin: &str,
) -> Result<(), String> {
    Individual::clear_property(conn, iri, property, origin)
        .map_err(|error| format!("Unable to replace {property} on '{iri}': {error}"))?;
    Individual::new(iri)
        .add_property(conn, property, vec![Object::Boolean(value)], origin)
        .map_err(|error| format!("Unable to set {property} on '{iri}': {error}"))
}

fn has_single_literal(
    conn: &owl::Connection,
    individual_iri: &str,
    property: &str,
    value: &str,
) -> Result<bool, String> {
    let current = owl::get_literal_property(conn, individual_iri, property)
        .map_err(|error| format!("Unable to inspect {property} on '{individual_iri}': {error}"))?;
    Ok(current.as_deref() == Some(value))
}

fn has_single_datetime(individual: &Individual, property: &str, value: &str) -> bool {
    let mut values = individual
        .properties
        .iter()
        .filter(|(current_property, _)| current_property == property)
        .map(|(_, current_value)| current_value);
    let Some(current_value) = values.next() else {
        return false;
    };
    let Some(current_datetime) = current_value
        .as_literal()
        .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
    else {
        return false;
    };
    let Some(expected_datetime) = DateTime::parse_from_rfc3339(value).ok() else {
        return false;
    };
    values.next().is_none() && current_datetime == expected_datetime
}

fn set_literal_if_changed(
    conn: &mut owl::Connection,
    individual: &Individual,
    property: &str,
    value: &str,
    datatype: &str,
    origin: &str,
) -> Result<(), String> {
    if has_single_literal(conn, &individual.iri, property, value)? {
        return Ok(());
    }
    set_literal(conn, &individual.iri, property, value, datatype, origin)
}

fn set_datetime_if_changed(
    conn: &mut owl::Connection,
    individual: &Individual,
    property: &str,
    value: &str,
    origin: &str,
) -> Result<(), String> {
    if has_single_datetime(individual, property, value) {
        return Ok(());
    }
    set_datetime(conn, &individual.iri, property, value, origin)
}

fn set_boolean_if_changed(
    conn: &mut owl::Connection,
    individual: &Individual,
    property: &str,
    value: bool,
    origin: &str,
) -> Result<(), String> {
    let mut values = individual
        .properties
        .iter()
        .filter(|(current_property, _)| current_property == property)
        .map(|(_, current_value)| current_value);
    if matches!(values.next(), Some(Object::Boolean(current)) if *current == value)
        && values.next().is_none()
    {
        return Ok(());
    }
    set_boolean(conn, &individual.iri, property, value, origin)
}

fn clear_property_if_present(
    conn: &mut owl::Connection,
    individual: &Individual,
    property: &str,
    origin: &str,
) -> Result<(), String> {
    if !individual
        .properties
        .iter()
        .any(|(current_property, _)| current_property == property)
    {
        return Ok(());
    }
    Individual::clear_property(conn, &individual.iri, property, origin).map_err(|error| {
        format!(
            "Unable to clear {property} on '{}': {error}",
            individual.iri
        )
    })
}

fn mark_catalog_stale(
    conn: &mut owl::Connection,
    service_iri: &str,
    message: &str,
) -> Result<(), String> {
    let safe_message = sanitize_sync_error(message, None);
    let service = Individual::get(conn, service_iri)
        .map_err(|error| format!("Unable to load AI service '{service_iri}': {error}"))?
        .ok_or_else(|| format!("AI service '{service_iri}' does not exist"))?;
    set_literal_if_changed(conn, &service, STATUS_PROPERTY, "stale", "xsd:string", "ai")?;
    set_literal_if_changed(
        conn,
        &service,
        SYNC_ERROR_PROPERTY,
        &safe_message,
        "xsd:string",
        "ai",
    )
}

fn sanitize_sync_error(message: &str, api_key: Option<&str>) -> String {
    let sanitized = api_key
        .filter(|secret| !secret.is_empty())
        .map(|secret| message.replace(secret, "[redacted]"))
        .unwrap_or_else(|| message.to_string());
    sanitized.chars().take(500).collect()
}

async fn mark_attempt_started(executor: &DbExecutor, service_iri: &str) -> Result<(), String> {
    let service_iri = service_iri.to_string();
    let attempted_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    executor
        .write(move |conn| {
            let service = Individual::get(conn, &service_iri)
                .map_err(|error| format!("Unable to load AI service '{service_iri}': {error}"))?
                .ok_or_else(|| format!("AI service '{service_iri}' does not exist"))?;
            set_literal_if_changed(
                conn,
                &service,
                STATUS_PROPERTY,
                "syncing",
                "xsd:string",
                "ai",
            )?;
            set_datetime(
                conn,
                &service_iri,
                LAST_ATTEMPT_PROPERTY,
                &attempted_at,
                "ai",
            )?;
            clear_property_if_present(conn, &service, SYNC_ERROR_PROPERTY, "ai")?;
            Ok(String::new())
        })
        .await
        .map(|_| ())
}

fn service_sync_data(
    conn: &owl::Connection,
    service_iri: &str,
) -> Result<
    Option<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
    String,
> {
    let Some(service) = Individual::get(conn, service_iri)
        .map_err(|error| format!("Unable to load AI service '{service_iri}': {error}"))?
    else {
        return Err(format!("AI service '{service_iri}' does not exist"));
    };
    if !is_openai_api_service(conn, &service) {
        return Ok(None);
    }
    let authentication_mode =
        owl::get_literal_property(conn, service_iri, "foundation:aiAuthenticationMode")
            .map_err(|error| format!("Unable to read AI authentication mode: {error}"))?
            .unwrap_or_else(|| "api_key".to_string());
    if authentication_mode != "api_key" {
        return Ok(None);
    }
    let credential_iri = owl::get_iri_property(conn, service_iri, "foundation:apiKey")
        .map_err(|error| format!("Unable to read OpenAI API credential reference: {error}"))?;
    let (api_key, credential_created_at) = match credential_iri {
        Some(credential_iri) => {
            let api_key =
                owl::get_literal_property(conn, &credential_iri, "foundation:credentialValue")
                    .map_err(|error| format!("Unable to read OpenAI API credential: {error}"))?
                    .filter(|value| !value.trim().is_empty());
            let created_at =
                owl::get_literal_property(conn, &credential_iri, "foundation:credentialCreatedAt")
                    .map_err(|error| {
                        format!("Unable to read OpenAI API credential creation time: {error}")
                    })?;
            (api_key, created_at)
        }
        None => (None, None),
    };
    let status = owl::get_literal_property(conn, service_iri, STATUS_PROPERTY)
        .map_err(|error| format!("Unable to read model catalog state: {error}"))?;
    let last_success = owl::get_literal_property(conn, service_iri, LAST_SUCCESS_PROPERTY)
        .map_err(|error| format!("Unable to read model catalog freshness: {error}"))?;
    Ok(Some((
        service_iri.to_string(),
        api_key,
        status,
        last_success,
        credential_created_at,
    )))
}

fn service_model_count(conn: &owl::Connection, service_iri: &str) -> Result<usize, String> {
    let models = owl::find_entities_with_property(conn, "foundation:offeredBy", service_iri)
        .map_err(|error| format!("Unable to inspect existing OpenAI models: {error}"))?;
    Ok(models
        .into_iter()
        .filter(|iri| owl::is_instance_of(conn, iri, "foundation:AIModel"))
        .count())
}

fn last_success_is_fresh(value: Option<&str>, credential_created_at: Option<&str>) -> bool {
    let Some(last_success) = value.and_then(|value| DateTime::parse_from_rfc3339(value).ok())
    else {
        return false;
    };
    let credential_is_newer = credential_created_at
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|created_at| created_at > last_success);
    let age = Utc::now().signed_duration_since(last_success.with_timezone(&Utc));
    !credential_is_newer && age >= chrono::Duration::zero() && age < chrono::Duration::days(1)
}

async fn synchronize_all_eligible_services(
    executor: &DbExecutor,
    coordinator: &OpenAIModelCatalogCoordinator,
) {
    let services = match executor
        .read(|conn| {
            let services =
                owl::find_entities_with_property(conn, "rdf:type", "foundation:AIAPIService")
                    .map_err(|error| format!("Unable to find AI API services: {error}"))?;
            Ok(services)
        })
        .await
    {
        Ok(services) => services,
        Err(error) => {
            crate::commands::log_backend(
                "error",
                &format!("OpenAI model catalog scheduler: {error}"),
            );
            return;
        }
    };
    for service_iri in services {
        let result = synchronize_service(executor, coordinator, &service_iri, false).await;
        if let Err(error) = result {
            crate::commands::log_backend(
                "warn",
                &format!("OpenAI API model catalog refresh for '{service_iri}' failed: {error}"),
            );
        }
    }
}

pub async fn ensure_fresh_for_service(
    app: AppHandle,
    executor: &DbExecutor,
    service_iri: &str,
) -> Result<(), String> {
    let coordinator = if let Some(state) = app.try_state::<OpenAIModelCatalogCoordinator>() {
        state.inner().clone()
    } else {
        let state = OpenAIModelCatalogCoordinator::default();
        app.manage(state.clone());
        state
    };
    let _guard = coordinator.lock.lock().await;
    let service_iri_owned = service_iri.to_string();
    let sync_data = executor
        .read(move |conn| service_sync_data(conn, &service_iri_owned))
        .await?;
    let Some((service_iri, api_key, status, last_success, credential_created_at)) = sync_data
    else {
        return Ok(());
    };
    let Some(api_key) = api_key else {
        mark_missing_credential_stale(executor, &service_iri).await?;
        return Err(format!(
            "No OpenAI API key is configured for '{service_iri}'"
        ));
    };
    if status.as_deref() == Some("current")
        && last_success_is_fresh(last_success.as_deref(), credential_created_at.as_deref())
    {
        return Ok(());
    }
    synchronize_locked(executor, &service_iri, &api_key)
        .await
        .map(|_| ())
}

async fn mark_missing_credential_stale(
    executor: &DbExecutor,
    service_iri: &str,
) -> Result<(), String> {
    let service_iri = service_iri.to_string();
    executor
        .write(move |conn| {
            mark_catalog_stale(
                conn,
                &service_iri,
                "No OpenAI API key is configured; reconnect the API account to refresh its model catalog.",
            )?;
            Ok(String::new())
        })
        .await
        .map(|_| ())
}

pub async fn ensure_fresh_for_conversation(
    app: AppHandle,
    executor: &DbExecutor,
    conversation_iri: &str,
) -> Result<(), String> {
    let conversation_iri = conversation_iri.to_string();
    let service_iri = executor
        .read(move |conn| {
            let agent_iri = owl::get_iri_property(conn, &conversation_iri, "foundation:handledBy")
                .map_err(|error| format!("Unable to resolve conversation agent: {error}"))?
                .ok_or_else(|| {
                    format!("Conversation '{}' has no assigned agent", conversation_iri)
                })?;
            let service_iri = owl::get_iri_property(conn, &agent_iri, "foundation:usesService")
                .map_err(|error| format!("Unable to resolve agent AI service: {error}"))?
                .or(crate::commands::chat::settings::get_ai_service_iri(conn)?)
                .ok_or_else(|| format!("Agent '{}' has no configured AI service", agent_iri))?;
            let authentication_mode =
                owl::get_literal_property(conn, &service_iri, "foundation:aiAuthenticationMode")
                    .map_err(|error| {
                        format!("Unable to inspect AI service authentication: {error}")
                    })?
                    .unwrap_or_else(|| "api_key".to_string());
            Ok((service_iri, authentication_mode))
        })
        .await?;
    if service_iri.1 != "api_key" {
        return Ok(());
    }
    ensure_fresh_for_service(app, executor, &service_iri.0).await
}

async fn synchronize_service(
    executor: &DbExecutor,
    coordinator: &OpenAIModelCatalogCoordinator,
    service_iri: &str,
    force: bool,
) -> Result<(), String> {
    let _guard = coordinator.lock.lock().await;
    let service_iri_owned = service_iri.to_string();
    let sync_data = executor
        .read(move |conn| service_sync_data(conn, &service_iri_owned))
        .await?;
    let Some((service_iri, api_key, status, last_success, credential_created_at)) = sync_data
    else {
        return Ok(());
    };
    if !force
        && status.as_deref() == Some("current")
        && last_success_is_fresh(last_success.as_deref(), credential_created_at.as_deref())
    {
        return Ok(());
    }
    let Some(api_key) = api_key else {
        mark_missing_credential_stale(executor, &service_iri).await?;
        return Ok(());
    };
    synchronize_locked(executor, &service_iri, &api_key)
        .await
        .map(|_| ())
}

async fn synchronize_locked(
    executor: &DbExecutor,
    service_iri: &str,
    api_key: &str,
) -> Result<usize, String> {
    mark_attempt_started(executor, service_iri).await?;
    let catalog = match fetch_openai_catalog(api_key).await {
        Ok(catalog) => catalog,
        Err(error) => {
            let stale_error = sanitize_sync_error(&error, Some(api_key));
            let stale_error_for_write = stale_error.clone();
            executor
                .write({
                    let service_iri = service_iri.to_string();
                    move |conn| {
                        mark_catalog_stale(conn, &service_iri, &stale_error_for_write)?;
                        Ok(String::new())
                    }
                })
                .await
                .map_err(|write_error| {
                    format!("{stale_error}; additionally failed to mark the catalog stale: {write_error}")
                })?;
            return Err(stale_error);
        }
    };
    let count = catalog.data.len();
    let service_iri = service_iri.to_string();
    let service_iri_for_write = service_iri.clone();
    let persisted = executor
        .write(move |conn| {
            let service = Individual::get(conn, &service_iri_for_write)
                .map_err(|error| format!("Unable to reload OpenAI API service: {error}"))?
                .ok_or_else(|| format!("AI service '{service_iri_for_write}' no longer exists"))?;
            if !is_openai_api_service(conn, &service) {
                return Err(format!(
                    "'{service_iri_for_write}' is no longer an OpenAI API service"
                ));
            }
            persist_catalog(conn, &service_iri_for_write, catalog)?;
            Ok(count.to_string())
        })
        .await;
    match persisted {
        Ok(count) => count.parse::<usize>().map_err(|error| error.to_string()),
        Err(error) => {
            let stale_error = sanitize_sync_error(
                &format!("Catalog persistence failed: {error}"),
                Some(api_key),
            );
            let stale_write_error = stale_error.clone();
            executor
                .write({
                    let service_iri = service_iri.clone();
                    move |conn| {
                        mark_catalog_stale(conn, &service_iri, &stale_write_error)?;
                        Ok(String::new())
                    }
                })
                .await
                .map_err(|mark_error| format!("{stale_error}; additionally failed to mark the catalog stale: {mark_error}"))?;
            Err(stale_error)
        }
    }
}

fn persist_catalog(
    conn: &mut owl::Connection,
    service_iri: &str,
    catalog: OpenAIModelCatalog,
) -> Result<usize, String> {
    let now = Utc::now();
    let now_rfc3339 = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let all_existing = owl::find_entities_with_property(conn, "foundation:offeredBy", service_iri)
        .map_err(|error| format!("Unable to inspect existing OpenAI models: {error}"))?;
    let existing_models = all_existing
        .into_iter()
        .filter(|iri| owl::is_instance_of(conn, iri, "foundation:AIModel"))
        .collect::<Vec<_>>();
    let mut current_iris = std::collections::HashSet::with_capacity(catalog.data.len());

    for model in catalog.data {
        let model_iri = stable_model_iri(service_iri, &model.id);
        current_iris.insert(model_iri.clone());
        if Individual::get(conn, &model_iri)
            .map_err(|error| format!("Unable to inspect model '{}': {error}", model.id))?
            .is_none()
        {
            let individual = Individual::new(&model_iri);
            individual
                .assert(
                    conn,
                    "foundation:AIModel",
                    &model.id,
                    "psychology_alt",
                    "ai",
                )
                .map_err(|error| format!("Unable to create model '{}': {error}", model.id))?;
            individual
                .add_property(
                    conn,
                    "foundation:hasStatus",
                    vec![Object::Iri("foundation:Completed".to_string())],
                    "ai",
                )
                .map_err(|error| format!("Unable to set model status '{}': {error}", model.id))?;
            individual
                .add_property(
                    conn,
                    "foundation:offeredBy",
                    vec![Object::Iri(service_iri.to_string())],
                    "ai",
                )
                .map_err(|error| format!("Unable to link model '{}': {error}", model.id))?;
            individual
                .add_property(
                    conn,
                    "foundation:modelIdentifier",
                    vec![Object::Literal {
                        value: model.id.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "ai",
                )
                .map_err(|error| {
                    format!("Unable to set model identifier '{}': {error}", model.id)
                })?;
        }

        let model_individual = Individual::get(conn, &model_iri)
            .map_err(|error| format!("Unable to reload model '{}': {error}", model.id))?
            .ok_or_else(|| format!("Model '{}' was not created", model.id))?;

        let shutdown_date = model
            .shutdown_date
            .as_deref()
            .map(DateTime::parse_from_rfc3339)
            .transpose()
            .map_err(|error| format!("Invalid shutdown date for '{}': {error}", model.id))?;
        let unavailable = model_is_unavailable(model.shutdown_date.as_deref(), now)?;
        set_boolean_if_changed(
            conn,
            &model_individual,
            "foundation:isDeprecated",
            unavailable,
            "ai",
        )?;
        set_literal_if_changed(
            conn,
            &model_individual,
            "rdfs:comment",
            &format!("OpenAI API model. Publisher: {}", model.owned_by),
            "xsd:string",
            "ai",
        )?;
        if let Some(shutdown_date) = shutdown_date {
            set_datetime_if_changed(
                conn,
                &model_individual,
                "foundation:existsUntil",
                &shutdown_date.to_rfc3339(),
                "ai",
            )?;
        } else {
            clear_property_if_present(conn, &model_individual, "foundation:existsUntil", "ai")?;
        }
    }

    for model_iri in existing_models {
        if !current_iris.contains(&model_iri) {
            let model = Individual::get(conn, &model_iri)
                .map_err(|error| format!("Unable to inspect removed model '{model_iri}': {error}"))?
                .ok_or_else(|| format!("Removed model '{model_iri}' no longer exists"))?;
            set_boolean_if_changed(conn, &model, "foundation:isDeprecated", true, "ai")?;
        }
    }

    let service = Individual::get(conn, service_iri)
        .map_err(|error| format!("Unable to reload OpenAI API service '{service_iri}': {error}"))?
        .ok_or_else(|| format!("AI service '{service_iri}' no longer exists"))?;
    set_literal_if_changed(
        conn,
        &service,
        STATUS_PROPERTY,
        "current",
        "xsd:string",
        "ai",
    )?;
    set_datetime(conn, service_iri, LAST_SUCCESS_PROPERTY, &now_rfc3339, "ai")?;
    clear_property_if_present(conn, &service, SYNC_ERROR_PROPERTY, "ai")?;
    Ok(current_iris.len())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__sync_openai_model_catalog(
    app: AppHandle,
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    let coordinator = if let Some(state) = app.try_state::<OpenAIModelCatalogCoordinator>() {
        state.inner().clone()
    } else {
        let state = OpenAIModelCatalogCoordinator::default();
        app.manage(state.clone());
        state
    };
    let _guard = coordinator.lock.lock().await;
    let service_iri_for_read = service_iri.clone();
    let sync_data = executor
        .read(move |conn| service_sync_data(conn, &service_iri_for_read))
        .await?;
    let Some((service_iri, api_key, status, last_success, credential_created_at)) = sync_data
    else {
        return Err(format!(
            "'{}' is not an OpenAI API key service",
            service_iri
        ));
    };
    let Some(api_key) = api_key else {
        mark_missing_credential_stale(executor.inner(), &service_iri).await?;
        return Err("Connect an OpenAI API key before synchronizing its model catalog".to_string());
    };
    if status.as_deref() == Some("current")
        && last_success_is_fresh(last_success.as_deref(), credential_created_at.as_deref())
    {
        let service_iri_for_read = service_iri.clone();
        let model_count = executor
            .read(move |conn| service_model_count(conn, &service_iri_for_read))
            .await?;
        return Ok(serde_json::json!({
            "status": "current",
            "serviceIri": service_iri,
            "modelCount": model_count,
        }));
    }
    let count = synchronize_locked(executor.inner(), &service_iri, &api_key).await?;
    Ok(serde_json::json!({
        "status": "current",
        "serviceIri": service_iri,
        "modelCount": count,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        model_is_unavailable, parse_model_catalog, persist_catalog, sanitize_sync_error,
        stable_model_iri, OpenAIModel, OpenAIModelCatalog, OpenAIModelCatalogCoordinator,
    };
    use crate::owl::{Class, ClassType, Individual, Property, PropertyType};
    use chrono::{DateTime, Utc};

    fn seed_class(conn: &mut crate::owl::Connection, iri: &str, label: &str, parent: Option<&str>) {
        Class::new(iri)
            .assert(conn, ClassType::OwlClass, label, "category", parent, "test")
            .unwrap();
    }

    fn seed_property(
        conn: &mut crate::owl::Connection,
        iri: &str,
        property_type: PropertyType,
        label: &str,
        domain: &str,
        range: Option<&str>,
    ) {
        Property::new(iri)
            .assert(
                conn,
                property_type,
                label,
                None,
                &[domain],
                range,
                None,
                "test",
            )
            .unwrap();
    }

    fn catalog_test_db() -> crate::owl::Connection {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        crate::owl::seed_icon_library(&mut conn);
        seed_class(&mut conn, "foundation:Continuant", "Continuant", None);
        seed_class(
            &mut conn,
            "foundation:AbstractContinuant",
            "Abstract Continuant",
            Some("foundation:Continuant"),
        );
        seed_class(
            &mut conn,
            "foundation:AIModel",
            "AI Model",
            Some("foundation:AbstractContinuant"),
        );
        seed_class(
            &mut conn,
            "foundation:AIAPIService",
            "AI API Service",
            Some("foundation:AbstractContinuant"),
        );
        seed_class(&mut conn, "foundation:Status", "Status", None);

        seed_property(
            &mut conn,
            "foundation:hasStatus",
            PropertyType::ObjectProperty,
            "Has Status",
            "foundation:Continuant",
            Some("foundation:Status"),
        );
        seed_property(
            &mut conn,
            "foundation:offeredBy",
            PropertyType::ObjectProperty,
            "Offered By",
            "foundation:AIModel",
            Some("foundation:AIAPIService"),
        );
        seed_property(
            &mut conn,
            "foundation:modelIdentifier",
            PropertyType::DatatypeProperty,
            "Model Identifier",
            "foundation:AIModel",
            Some("xsd:string"),
        );
        seed_property(
            &mut conn,
            "foundation:isDeprecated",
            PropertyType::DatatypeProperty,
            "Is Deprecated",
            "foundation:AIModel",
            Some("xsd:boolean"),
        );
        seed_property(
            &mut conn,
            "foundation:existsUntil",
            PropertyType::DatatypeProperty,
            "Exists Until",
            "foundation:AIModel",
            Some("xsd:dateTime"),
        );
        seed_property(
            &mut conn,
            "rdfs:comment",
            PropertyType::AnnotationProperty,
            "Comment",
            "foundation:AIModel",
            Some("xsd:string"),
        );
        for (property, label) in [
            (super::STATUS_PROPERTY, "Model Catalog Status"),
            (super::SYNC_ERROR_PROPERTY, "Model Catalog Sync Error"),
        ] {
            seed_property(
                &mut conn,
                property,
                PropertyType::DatatypeProperty,
                label,
                "foundation:AIAPIService",
                Some("xsd:string"),
            );
        }
        for property in [super::LAST_SUCCESS_PROPERTY, super::LAST_ATTEMPT_PROPERTY] {
            seed_property(
                &mut conn,
                property,
                PropertyType::DatatypeProperty,
                "Catalog Timestamp",
                "foundation:AIAPIService",
                Some("xsd:dateTime"),
            );
        }
        Individual::new("foundation:Completed")
            .assert_without_icon(&mut conn, "foundation:Status", "Completed", "test")
            .unwrap();
        Individual::new("foundation:TestOpenAIAPI")
            .assert_without_icon(
                &mut conn,
                "foundation:AIAPIService",
                "Test OpenAI API",
                "test",
            )
            .unwrap();
        conn
    }

    fn model(id: &str, owner: &str, shutdown_date: Option<&str>) -> OpenAIModel {
        OpenAIModel {
            id: id.to_string(),
            owned_by: owner.to_string(),
            shutdown_date: shutdown_date.map(str::to_string),
        }
    }

    fn catalog(data: Vec<OpenAIModel>) -> OpenAIModelCatalog {
        OpenAIModelCatalog {
            object: "list".to_string(),
            data,
        }
    }

    fn current_fact_txs(conn: &crate::owl::Connection, iri: &str) -> Vec<(String, i64)> {
        let mut statement = conn
            .prepare(
                "SELECT predicate, MAX(tx) FROM triples \
                 WHERE subject = ?1 AND is_current = 1 AND retracted = 0 \
                 GROUP BY predicate ORDER BY predicate",
            )
            .unwrap();
        statement
            .query_map([iri], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn literal(conn: &crate::owl::Connection, iri: &str, property: &str) -> Option<String> {
        crate::owl::get_literal_property(conn, iri, property).unwrap()
    }

    #[test]
    fn identical_catalog_sync_does_not_write_model_facts_again() {
        let mut conn = catalog_test_db();
        let first = catalog(vec![model("gpt-test", "openai", None)]);
        persist_catalog(&mut conn, "foundation:TestOpenAIAPI", first.clone()).unwrap();

        let model_iri = stable_model_iri("foundation:TestOpenAIAPI", "gpt-test");
        let before = current_fact_txs(&conn, &model_iri);
        persist_catalog(&mut conn, "foundation:TestOpenAIAPI", first).unwrap();
        let after = current_fact_txs(&conn, &model_iri);

        assert_eq!(before, after);
    }

    #[test]
    fn changed_metadata_only_rewrites_the_changed_model_fact() {
        let mut conn = catalog_test_db();
        persist_catalog(
            &mut conn,
            "foundation:TestOpenAIAPI",
            catalog(vec![model("gpt-test", "publisher-a", None)]),
        )
        .unwrap();
        let model_iri = stable_model_iri("foundation:TestOpenAIAPI", "gpt-test");
        let before = current_fact_txs(&conn, &model_iri);

        persist_catalog(
            &mut conn,
            "foundation:TestOpenAIAPI",
            catalog(vec![model("gpt-test", "publisher-b", None)]),
        )
        .unwrap();
        let after = current_fact_txs(&conn, &model_iri);

        for (predicate, tx) in &before {
            if predicate != "rdfs:comment" && predicate != "foundation:lastUpdatedAt" {
                assert_eq!(
                    after
                        .iter()
                        .find(|(next, _)| next == predicate)
                        .map(|(_, tx)| tx),
                    Some(tx),
                    "unchanged property {predicate} was rewritten"
                );
            }
        }
        assert_ne!(
            before.iter().find(|(p, _)| p == "rdfs:comment"),
            after.iter().find(|(p, _)| p == "rdfs:comment")
        );
    }

    #[test]
    fn removed_model_is_deprecated_and_reintroduction_reuses_its_identity() {
        let mut conn = catalog_test_db();
        persist_catalog(
            &mut conn,
            "foundation:TestOpenAIAPI",
            catalog(vec![model("gpt-test", "openai", None)]),
        )
        .unwrap();
        let model_iri = stable_model_iri("foundation:TestOpenAIAPI", "gpt-test");

        persist_catalog(
            &mut conn,
            "foundation:TestOpenAIAPI",
            catalog(vec![model("another-model", "openai", None)]),
        )
        .unwrap();
        assert_eq!(
            literal(&conn, &model_iri, "foundation:isDeprecated").as_deref(),
            Some("true")
        );

        persist_catalog(
            &mut conn,
            "foundation:TestOpenAIAPI",
            catalog(vec![model("gpt-test", "openai", None)]),
        )
        .unwrap();
        assert_eq!(
            literal(&conn, &model_iri, "foundation:isDeprecated").as_deref(),
            Some("false")
        );
        assert!(Individual::get(&conn, &model_iri).unwrap().is_some());
    }

    #[test]
    fn invalid_catalog_response_preserves_the_last_persisted_catalog() {
        let mut conn = catalog_test_db();
        persist_catalog(
            &mut conn,
            "foundation:TestOpenAIAPI",
            catalog(vec![model("gpt-test", "openai", None)]),
        )
        .unwrap();
        let model_iri = stable_model_iri("foundation:TestOpenAIAPI", "gpt-test");
        let before = current_fact_txs(&conn, &model_iri);
        let result = parse_model_catalog(r#"{"object":"list","data":[]}"#);

        assert!(result.is_err());
        assert_eq!(current_fact_txs(&conn, &model_iri), before);
    }

    #[test]
    fn sync_failure_marks_catalog_stale_without_changing_its_models() {
        let mut conn = catalog_test_db();
        persist_catalog(
            &mut conn,
            "foundation:TestOpenAIAPI",
            catalog(vec![model("gpt-test", "openai", None)]),
        )
        .unwrap();
        let model_iri = stable_model_iri("foundation:TestOpenAIAPI", "gpt-test");
        let before = current_fact_txs(&conn, &model_iri);

        super::mark_catalog_stale(
            &mut conn,
            "foundation:TestOpenAIAPI",
            "temporary network failure",
        )
        .unwrap();

        assert_eq!(
            literal(&conn, &model_iri, "foundation:isDeprecated").as_deref(),
            Some("false")
        );
        assert_eq!(current_fact_txs(&conn, &model_iri), before);
        assert_eq!(
            literal(&conn, "foundation:TestOpenAIAPI", super::STATUS_PROPERTY).as_deref(),
            Some("stale")
        );
        assert_eq!(
            literal(
                &conn,
                "foundation:TestOpenAIAPI",
                super::SYNC_ERROR_PROPERTY
            )
            .as_deref(),
            Some("temporary network failure")
        );
    }

    #[tokio::test]
    async fn coordinator_serializes_concurrent_sync_attempts() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        let coordinator = OpenAIModelCatalogCoordinator::default();
        let active = std::sync::Arc::new(AtomicUsize::new(0));
        let max_active = std::sync::Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let coordinator = coordinator.clone();
            let active = active.clone();
            let max_active = max_active.clone();
            tasks.push(tokio::spawn(async move {
                let _guard = coordinator.lock.lock().await;
                let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(now_active, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(10)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn parses_a_complete_openai_models_response() {
        let catalog = parse_model_catalog(
            r#"{"object":"list","data":[{"id":"gpt-test","object":"model","created":1,"owned_by":"openai"}]}"#,
        )
        .unwrap();
        assert_eq!(catalog.data[0].id, "gpt-test");
        assert_eq!(catalog.data[0].owned_by, "openai");
    }

    #[test]
    fn rejects_incomplete_or_duplicate_catalogs() {
        assert!(parse_model_catalog(r#"{"object":"list","data":[]}"#).is_err());
        assert!(parse_model_catalog(
            r#"{"object":"list","data":[{"id":"gpt-test","owned_by":"openai"},{"id":"gpt-test","owned_by":"openai"}]}"#
        )
        .is_err());
        assert!(parse_model_catalog(r#"{"object":"page","data":[]}"#).is_err());
    }

    #[test]
    fn model_iris_are_stable_and_scoped_to_the_service() {
        let a = stable_model_iri("foundation:OpenAI_API", "gpt-4.1");
        assert_eq!(a, stable_model_iri("foundation:OpenAI_API", "gpt-4.1"));
        assert_ne!(a, stable_model_iri("foundation:ChatGPT", "gpt-4.1"));
    }

    #[test]
    fn shutdown_date_controls_model_availability() {
        let now = DateTime::parse_from_rfc3339("2026-11-11T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(model_is_unavailable(Some("2026-11-10T12:00:00Z"), now).unwrap());
        assert!(!model_is_unavailable(Some("2026-11-12T12:00:00Z"), now).unwrap());
        assert!(!model_is_unavailable(None, now).unwrap());
    }

    #[test]
    fn sync_errors_never_expose_the_api_key_and_are_bounded() {
        assert_eq!(
            sanitize_sync_error("request failed with sk-secret", Some("sk-secret")),
            "request failed with [redacted]"
        );
        assert_eq!(sanitize_sync_error(&"x".repeat(600), None).len(), 500);
    }
}
