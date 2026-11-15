use crate::owl::{self, DbExecutor, Individual, Object};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::form_urlencoded;
use uuid::Uuid;

const CHATGPT_AUTHENTICATION_MODE: &str = "chatgpt_plan";
const AUTHORIZATION_ENDPOINT: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN_ENDPOINT: &str = "https://auth.openai.com/api/accounts/oauth/token";
const RESOURCE: &str = "https://api.openai.com/v1";
const CHATGPT_SCOPE: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const CALLBACK_PATH: &str = "/auth/callback";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptAuthSession {
    pub auth_session_id: String,
    pub status: String,
    pub stage: String,
    pub email: Option<String>,
    pub error: Option<String>,
}

#[derive(Default, Clone)]
pub struct ChatGptAuthState {
    inner: Arc<Mutex<ChatGptAuthSessions>>,
    start_lock: Arc<tokio::sync::Mutex<()>>,
    finalization_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
struct ChatGptAuthSessions {
    sessions: HashMap<String, ChatGptAuthSession>,
    tasks: HashMap<String, tokio::task::AbortHandle>,
    authorization_urls: HashMap<String, String>,
    active_session_id: Option<String>,
}

impl ChatGptAuthState {
    fn active_session_with_url(&self) -> Option<(ChatGptAuthSession, String)> {
        self.inner.lock().ok().and_then(|inner| {
            let session_id = inner.active_session_id.as_deref()?;
            let session = inner.sessions.get(session_id)?;
            let url = inner.authorization_urls.get(session_id)?;
            (session.status == "authorizing").then(|| (session.clone(), url.clone()))
        })
    }

    fn set_session(&self, session: ChatGptAuthSession) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        if session.status == "authorizing" {
            if inner
                .active_session_id
                .as_deref()
                .is_some_and(|active_id| active_id != session.auth_session_id)
            {
                return false;
            }
            inner.active_session_id = Some(session.auth_session_id.clone());
        } else if inner.active_session_id.as_deref() != Some(&session.auth_session_id) {
            return false;
        } else {
            inner.active_session_id = None;
            inner.tasks.remove(&session.auth_session_id);
            inner.authorization_urls.remove(&session.auth_session_id);
        }
        inner
            .sessions
            .insert(session.auth_session_id.clone(), session);
        true
    }

    fn set_task(&self, session_id: String, task: tokio::task::AbortHandle) {
        let inserted = self.inner.lock().ok().is_some_and(|mut inner| {
            let is_active = inner.active_session_id.as_deref() == Some(&session_id)
                && inner
                    .sessions
                    .get(&session_id)
                    .is_some_and(|session| session.status == "authorizing");
            if is_active {
                inner.tasks.insert(session_id, task.clone());
            }
            is_active
        });
        if !inserted {
            task.abort();
        }
    }

    fn set_authorization_url(&self, session_id: &str, url: String) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        if inner.active_session_id.as_deref() != Some(session_id)
            || !inner
                .sessions
                .get(session_id)
                .is_some_and(|session| session.status == "authorizing")
        {
            return false;
        }
        inner.authorization_urls.insert(session_id.to_string(), url);
        true
    }

    fn set_stage(&self, session_id: &str, stage: &str) -> bool {
        let updated = self.inner.lock().ok().is_some_and(|mut inner| {
            if inner.active_session_id.as_deref() != Some(session_id) {
                return false;
            }
            if let Some(session) = inner.sessions.get_mut(session_id) {
                if session.status == "authorizing" {
                    session.stage = stage.to_string();
                    return true;
                }
            }
            false
        });
        if updated {
            auth_log(session_id, stage, "in_progress");
        }
        updated
    }

    fn finish_active_session(
        &self,
        session: ChatGptAuthSession,
        outcome: &str,
    ) -> Option<ChatGptAuthSession> {
        let Ok(mut inner) = self.inner.lock() else {
            return None;
        };
        if inner.active_session_id.as_deref() != Some(&session.auth_session_id)
            || !inner
                .sessions
                .get(&session.auth_session_id)
                .is_some_and(|current| current.status == "authorizing")
        {
            return None;
        }
        inner.active_session_id = None;
        inner.tasks.remove(&session.auth_session_id);
        inner.authorization_urls.remove(&session.auth_session_id);
        inner
            .sessions
            .insert(session.auth_session_id.clone(), session.clone());
        auth_log(&session.auth_session_id, &session.stage, outcome);
        Some(session)
    }

    fn finish_connected_session(&self, session_id: &str, email: Option<String>) -> bool {
        self.finish_active_session(
            ChatGptAuthSession {
                auth_session_id: session_id.to_string(),
                status: "connected".to_string(),
                stage: "connected".to_string(),
                email,
                error: None,
            },
            "success",
        )
        .is_some()
    }

    fn finish_error_session(
        &self,
        session_id: &str,
        stage: &str,
        error: String,
    ) -> Option<ChatGptAuthSession> {
        self.finish_active_session(
            ChatGptAuthSession {
                auth_session_id: session_id.to_string(),
                status: "error".to_string(),
                stage: stage.to_string(),
                email: None,
                error: Some(error),
            },
            "error",
        )
    }

    fn finish_cancelled_session(&self, session_id: &str) -> Option<ChatGptAuthSession> {
        self.finish_active_session(
            ChatGptAuthSession {
                auth_session_id: session_id.to_string(),
                status: "cancelled".to_string(),
                stage: "cancelled".to_string(),
                email: None,
                error: None,
            },
            "cancelled",
        )
    }

    pub async fn fail_session(
        &self,
        session_id: &str,
        stage: &str,
        error: String,
    ) -> Option<ChatGptAuthSession> {
        let _finalization_guard = self.finalization_lock.lock().await;
        self.finish_error_session(session_id, stage, error)
    }

    pub fn get_session(&self, session_id: &str) -> Option<ChatGptAuthSession> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.sessions.get(session_id).cloned())
    }

    pub fn get_active_session(&self) -> Option<ChatGptAuthSession> {
        self.inner.lock().ok().and_then(|inner| {
            inner
                .active_session_id
                .as_deref()
                .and_then(|session_id| inner.sessions.get(session_id))
                .filter(|session| session.status == "authorizing")
                .cloned()
        })
    }

    pub fn get_active_or_connected_session(&self) -> Option<ChatGptAuthSession> {
        self.inner.lock().ok().and_then(|inner| {
            inner
                .active_session_id
                .as_deref()
                .and_then(|session_id| inner.sessions.get(session_id))
                .cloned()
        })
    }

    pub async fn cancel_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ChatGptAuthSession>, String> {
        let _finalization_guard = self.finalization_lock.lock().await;
        let task = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| "ChatGPT auth state is unavailable".to_string())?;
            if inner.active_session_id.as_deref() != Some(session_id)
                || !inner
                    .sessions
                    .get(session_id)
                    .is_some_and(|session| session.status == "authorizing")
            {
                return Ok(None);
            }
            inner.active_session_id = None;
            inner.authorization_urls.remove(session_id);
            let task = inner.tasks.remove(session_id);
            inner.sessions.insert(
                session_id.to_string(),
                ChatGptAuthSession {
                    auth_session_id: session_id.to_string(),
                    status: "cancelled".to_string(),
                    stage: "cancelled".to_string(),
                    email: None,
                    error: None,
                },
            );
            task
        };
        if let Some(task) = task {
            task.abort();
        }
        auth_log(session_id, "cancelled", "cancelled");
        Ok(Some(ChatGptAuthSession {
            auth_session_id: session_id.to_string(),
            status: "cancelled".to_string(),
            stage: "cancelled".to_string(),
            email: None,
            error: None,
        }))
    }

    pub async fn cancel_active_session(&self) {
        let session_id = self
            .get_active_session()
            .map(|session| session.auth_session_id);
        if let Some(session_id) = session_id {
            let _ = self.cancel_session(&session_id).await;
        }
    }
}

fn auth_log(session_id: &str, stage: &str, outcome: &str) {
    crate::commands::log_backend(
        "info",
        &format!("[CHATGPT_AUTH] authSessionId={session_id} stage={stage} outcome={outcome}"),
    );
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptCredential {
    pub client_id: String,
    pub ext_agent_host_id: String,
    pub subject: String,
    pub email: Option<String>,
    pub issuer: String,
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub scopes: Vec<String>,
    pub expires_at_unix: i64,
    #[serde(default)]
    pub plan_usage_acknowledged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptConnectionStatus {
    pub status: String,
    pub email: Option<String>,
    pub plan_usage_enabled: bool,
    pub expires_at: Option<String>,
    pub usage: Option<serde_json::Value>,
    pub show_welcome_confirmation: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenIdConfiguration {
    issuer: String,
    jwks_uri: String,
    revocation_endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdTokenClaims {
    iss: String,
    sub: String,
    aud: Audience,
    exp: usize,
    nonce: String,
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Audience {
    Single(String),
    Multiple(Vec<String>),
}

impl Audience {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Self::Single(value) => value == expected,
            Self::Multiple(values) => values.iter().any(|value| value == expected),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptModel {
    pub id: String,
    pub name: String,
    pub owned_by: String,
}

#[derive(Debug, Deserialize)]
struct ChatGptModelCatalog {
    models: Vec<ChatGptModelCatalogEntry>,
}

#[derive(Debug, Deserialize)]
struct ChatGptModelCatalogEntry {
    slug: String,
    display_name: String,
    visibility: String,
}

fn parse_model_catalog(body: &[u8]) -> Result<Vec<ChatGptModel>, String> {
    let catalog: ChatGptModelCatalog = serde_json::from_slice(body)
        .map_err(|error| format!("OpenAI returned an invalid model list: {error}"))?;

    Ok(catalog
        .models
        .into_iter()
        .filter(|model| model.visibility == "list" && !model.slug.trim().is_empty())
        .map(|model| ChatGptModel {
            id: model.slug,
            name: model.display_name,
            owned_by: "OpenAI".to_string(),
        })
        .collect())
}

fn read_credential_from_connection(
    conn: &crate::owl::Connection,
) -> Result<Option<ChatGptCredential>, String> {
    let service_iri = crate::core_ontology::ai_services::resolve_openai_service(
        conn,
        CHATGPT_AUTHENTICATION_MODE,
    )?;
    let Some(credential_iri) = linked_credential_iri(conn, &service_iri)? else {
        return Ok(None);
    };
    read_credential_for_iri_from_connection(conn, &credential_iri)
}

fn linked_credential_iri(
    conn: &crate::owl::Connection,
    service_iri: &str,
) -> Result<Option<String>, String> {
    let service = Individual::get(conn, service_iri)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("OpenAI service {service_iri} no longer exists"))?;
    let credentials = service
        .properties
        .iter()
        .filter(|(predicate, _)| predicate == "foundation:hasCredential")
        .filter_map(|(_, value)| match value {
            Object::Iri(iri) => Some(iri.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    match credentials.as_slice() {
        [] => Ok(None),
        [credential] => Ok(Some(credential.clone())),
        _ => Err(format!(
            "OpenAI service {service_iri} has multiple linked credentials; resolve the duplicate credential records before continuing"
        )),
    }
}

fn read_credential_for_iri_from_connection(
    conn: &crate::owl::Connection,
    credential_iri: &str,
) -> Result<Option<ChatGptCredential>, String> {
    let Some(serialized) =
        owl::get_literal_property(conn, credential_iri, "foundation:credentialValue")
            .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    serde_json::from_str(&serialized)
        .map(Some)
        .map_err(|_| "Stored ChatGPT account data is invalid".to_string())
}

async fn read_credential(executor: &DbExecutor) -> Result<Option<ChatGptCredential>, String> {
    executor.read(read_credential_from_connection).await
}

pub fn has_plan_access_from_connection(conn: &crate::owl::Connection) -> Result<bool, String> {
    Ok(
        read_credential_from_connection(conn)?.is_some_and(|credential| {
            credential
                .scopes
                .iter()
                .any(|scope| scope == "chatgpt.tokens.use.direct")
        }),
    )
}

pub fn service_has_plan_access_from_connection(
    conn: &crate::owl::Connection,
    service_iri: &str,
) -> Result<bool, String> {
    let Some(credential_iri) = owl::get_iri_property(conn, service_iri, "foundation:hasCredential")
        .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };

    Ok(
        read_credential_for_iri_from_connection(conn, &credential_iri)?.is_some_and(|credential| {
            credential
                .scopes
                .iter()
                .any(|scope| scope == "chatgpt.tokens.use.direct")
        }),
    )
}

async fn write_credential(
    executor: &DbExecutor,
    credential: ChatGptCredential,
) -> Result<(), String> {
    let serialized = serde_json::to_string(&credential)
        .map_err(|_| "Unable to encode ChatGPT account data".to_string())?;
    let client_id = credential.client_id;
    let scopes = credential.scopes.join(" ");
    let expires_at = chrono::DateTime::from_timestamp(credential.expires_at_unix, 0)
        .unwrap_or_default()
        .to_rfc3339();
    executor
        .write(move |conn| {
            let service_iri = crate::core_ontology::ai_services::resolve_openai_service(
                conn,
                CHATGPT_AUTHENTICATION_MODE,
            )?;
            let credential_iri = match linked_credential_iri(conn, &service_iri)? {
                Some(credential_iri) => credential_iri,
                None => format!("foundation:OAuth2Credential_{}", Uuid::new_v4()),
            };
            let is_new = Individual::get(conn, &credential_iri)
                .map_err(|error| error.to_string())?
                .is_none();
            if is_new {
                let active_status_iri = crate::core_ontology::status::resolve_active_status(conn)?;
                let credential = Individual::new(&credential_iri);
                credential
                    .assert(
                        conn,
                        "foundation:OAuth2Credential",
                        "ChatGPT account",
                        "vpn_key",
                        "chatgpt_auth",
                    )
                    .map_err(|error| error.to_string())?;
                credential
                    .add_property(
                        conn,
                        "foundation:hasStatus",
                        vec![Object::Iri(active_status_iri)],
                        "chatgpt_auth",
                    )
                    .map_err(|error| error.to_string())?;
            }
            for (predicate, value) in [
                ("foundation:credentialValue", serialized),
                ("foundation:clientId", client_id),
                ("foundation:oauthScopes", scopes),
                ("foundation:grantType", "authorization_code".to_string()),
                (
                    "foundation:authorizationUrl",
                    AUTHORIZATION_ENDPOINT.to_string(),
                ),
                ("foundation:tokenUrl", TOKEN_ENDPOINT.to_string()),
            ] {
                owl::replace_all_property_literals(
                    conn,
                    &credential_iri,
                    predicate,
                    &[&value],
                    "chatgpt_auth",
                )
                .map_err(|error| error.to_string())?;
            }
            let existing_expiration = Individual::get(conn, &credential_iri)
                .map_err(|error| error.to_string())?
                .into_iter()
                .flat_map(|individual| individual.properties)
                .filter(|(predicate, _)| predicate == "foundation:tokenExpiresAt")
                .map(|(_, value)| {
                    owl::Triple::new(&credential_iri, "foundation:tokenExpiresAt", value)
                })
                .collect::<Vec<_>>();
            if !existing_expiration.is_empty() {
                owl::retract_raw_triples(conn, &existing_expiration, "chatgpt_auth")
                    .map_err(|error| error.to_string())?;
            }
            owl::assert_raw_triples_with_tx(
                conn,
                &[owl::Triple::new(
                    &credential_iri,
                    "foundation:tokenExpiresAt",
                    Object::DateTime(expires_at),
                )],
                "chatgpt_auth",
            )
            .map_err(|error| error.to_string())?;
            owl::replace_all_property_iris(
                conn,
                &service_iri,
                "foundation:hasCredential",
                &[&credential_iri],
                "chatgpt_auth",
            )
            .map_err(|error| error.to_string())?;
            Ok(String::new())
        })
        .await
        .map(|_| ())
}

fn get_or_create_host_id() -> String {
    format!("urn:uuid:{}", Uuid::new_v4())
}

fn random_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub async fn start_auth(
    app: AppHandle,
    state: ChatGptAuthState,
) -> Result<serde_json::Value, String> {
    let _start_guard = state.start_lock.lock().await;
    if let Some((session, authorization_url)) = state.active_session_with_url() {
        return Ok(serde_json::json!({
            "authSessionId": session.auth_session_id,
            "status": session.status,
            "authorizationUrl": authorization_url,
            "reused": true,
        }));
    }

    let session_id = Uuid::new_v4().to_string();
    auth_log(&session_id, "session_start", "started");
    let listener = match TcpListener::bind(("127.0.0.1", 0)).await {
        Ok(listener) => listener,
        Err(_) => {
            auth_log(&session_id, "session_start", "callback_bind_error");
            return Err("Unable to start the local ChatGPT sign-in callback".to_string());
        }
    };
    let port = listener
        .local_addr()
        .map_err(|error| format!("Unable to read the local callback address: {error}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
    let host_id = get_or_create_host_id();

    let state_token = random_secret();
    let nonce = random_secret();
    let verifier = random_secret();
    let client_id = "dynamic_agent_client".to_string();
    let mut params = vec![
        ("client_id", client_id.clone()),
        ("response_type", "code".to_string()),
        ("redirect_uri", redirect_uri.clone()),
        ("scope", CHATGPT_SCOPE.to_string()),
        ("resource", RESOURCE.to_string()),
        ("state", state_token.clone()),
        ("nonce", nonce.clone()),
        ("code_challenge_method", "S256".to_string()),
        ("code_challenge", pkce_challenge(&verifier)),
        ("ext_agent_host_id", host_id.clone()),
    ];
    params.push(("agent_name_hint", "Foundation".to_string()));
    let authorization_url = format!(
        "{AUTHORIZATION_ENDPOINT}?{}",
        form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params)
            .finish()
    );

    let session = ChatGptAuthSession {
        auth_session_id: session_id.clone(),
        status: "authorizing".to_string(),
        stage: "waiting_for_callback".to_string(),
        email: None,
        error: None,
    };
    if !state.set_session(session.clone()) {
        return Err("Unable to reserve the ChatGPT sign-in session".to_string());
    }
    if !state.set_authorization_url(&session_id, authorization_url.clone()) {
        return Err("Unable to prepare the ChatGPT sign-in session".to_string());
    }
    auth_log(&session_id, "waiting_for_callback", "in_progress");

    let callback_state = state.clone();
    let callback_app = app.clone();
    let callback_session_id = session_id.clone();
    let callback_state_for_auth = state.clone();
    let handle = tokio::spawn(async move {
        match complete_auth(
            listener,
            &redirect_uri,
            &client_id,
            true,
            &state_token,
            &nonce,
            &verifier,
            &host_id,
            &callback_state_for_auth,
            &callback_session_id,
        )
        .await
        {
            Ok(credential) => {
                let _finalization_guard = callback_state.finalization_lock.lock().await;
                if callback_state
                    .get_active_session()
                    .as_ref()
                    .map(|session| session.auth_session_id.as_str())
                    != Some(callback_session_id.as_str())
                {
                    return;
                }
                if write_credential(
                    callback_app.state::<DbExecutor>().inner(),
                    credential.clone(),
                )
                .await
                .is_err()
                {
                    if let Some(session) = callback_state.finish_error_session(
                        &callback_session_id,
                        "saving_credentials",
                        "Unable to store ChatGPT account credentials in the local ontology"
                            .to_string(),
                    ) {
                        let _ = callback_app.emit("chatgpt-auth-status", session);
                    }
                    return;
                }
                if callback_state.finish_connected_session(&callback_session_id, credential.email) {
                    if let Some(session) = callback_state.get_session(&callback_session_id) {
                        let _ = callback_app.emit("chatgpt-auth-status", session);
                    }
                }
            }
            Err(error) => {
                let _finalization_guard = callback_state.finalization_lock.lock().await;
                if error == "__CHATGPT_AUTH_CANCELLED__" {
                    if let Some(session) =
                        callback_state.finish_cancelled_session(&callback_session_id)
                    {
                        let _ = callback_app.emit("chatgpt-auth-status", session);
                    }
                    return;
                }
                let stage = callback_state
                    .get_session(&callback_session_id)
                    .map(|session| session.stage)
                    .unwrap_or_else(|| "unknown".to_string());
                if let Some(session) =
                    callback_state.finish_error_session(&callback_session_id, &stage, error)
                {
                    let _ = callback_app.emit("chatgpt-auth-status", session);
                }
            }
        }
    });
    state.set_task(session_id, handle.abort_handle());

    Ok(serde_json::json!({
        "authSessionId": session.auth_session_id,
        "status": session.status,
        "authorizationUrl": authorization_url,
        "reused": false,
    }))
}

async fn complete_auth(
    listener: TcpListener,
    redirect_uri: &str,
    previous_client_id: &str,
    first_registration: bool,
    expected_state: &str,
    nonce: &str,
    verifier: &str,
    host_id: &str,
    state: &ChatGptAuthState,
    session_id: &str,
) -> Result<ChatGptCredential, String> {
    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|error| format!("The ChatGPT sign-in callback failed: {error}"))?;
    if !state.set_stage(session_id, "callback_received") {
        return Err("__CHATGPT_AUTH_CANCELLED__".to_string());
    }
    let mut request = vec![0_u8; 16 * 1024];
    let read = stream
        .read(&mut request)
        .await
        .map_err(|error| format!("Unable to read the ChatGPT sign-in response: {error}"))?;
    let request = std::str::from_utf8(&request[..read])
        .map_err(|_| "The ChatGPT sign-in callback was not valid UTF-8".to_string())?;
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| "The ChatGPT sign-in callback did not include a URL".to_string())?;
    let callback = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|error| format!("The ChatGPT sign-in callback URL is invalid: {error}"))?;
    if callback.path() != CALLBACK_PATH {
        write_callback_response(&mut stream, 404, "Not found")
            .await
            .ok();
        return Err("The ChatGPT sign-in callback path did not match".to_string());
    }
    let query: HashMap<String, String> = callback.query_pairs().into_owned().collect();
    if query.get("state").map(String::as_str) != Some(expected_state) {
        write_callback_response(&mut stream, 400, "Sign-in could not be verified")
            .await
            .ok();
        return Err("The ChatGPT sign-in could not be verified. Start again.".to_string());
    }
    if let Some(error) = query.get("error") {
        write_callback_response(&mut stream, 200, "Sign-in was cancelled")
            .await
            .ok();
        if error == "access_denied" {
            return Err("__CHATGPT_AUTH_CANCELLED__".to_string());
        }
        return Err("ChatGPT sign-in was rejected by the authorization provider".to_string());
    }
    let code = query.get("code").ok_or_else(|| {
        "The ChatGPT sign-in callback did not include an authorization code".to_string()
    })?;
    let issued_client_id = query.get("client_id").map(String::as_str);
    let client_id = if first_registration {
        issued_client_id
            .filter(|value| !value.is_empty() && *value != "dynamic_agent_client")
            .ok_or_else(|| "OpenAI did not complete the local app registration".to_string())?
    } else {
        if issued_client_id.is_some_and(|value| value != previous_client_id) {
            return Err("OpenAI returned a different ChatGPT account registration".to_string());
        }
        previous_client_id
    };
    write_callback_response(
        &mut stream,
        200,
        "ChatGPT sign-in completed. You can return to Foundation.",
    )
    .await?;

    if !state.set_stage(session_id, "exchanging_code") {
        return Err("__CHATGPT_AUTH_CANCELLED__".to_string());
    }
    let client = reqwest::Client::new();
    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
            ("resource", RESOURCE),
        ])
        .send()
        .await
        .map_err(|error| format!("Unable to exchange the ChatGPT sign-in code: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "ChatGPT sign-in token exchange failed (HTTP {status}): {}",
            safe_provider_error(&body)
        ));
    }
    let token: TokenResponse = response
        .json()
        .await
        .map_err(|error| format!("OpenAI returned an invalid token response: {error}"))?;
    if !token.token_type.eq_ignore_ascii_case("bearer") {
        return Err("OpenAI returned an unsupported token type".to_string());
    }
    let id_token = token
        .id_token
        .as_deref()
        .ok_or_else(|| "OpenAI did not return an ID token".to_string())?;
    let scopes = token
        .scope
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !scopes
        .iter()
        .any(|scope| scope == "chatgpt.tokens.use.direct")
    {
        return Err(
            "The account did not grant permission to use the eligible ChatGPT plan allowance"
                .to_string(),
        );
    }
    let configuration = load_openid_configuration(&client).await?;
    if !state.set_stage(session_id, "validating_identity") {
        return Err("__CHATGPT_AUTH_CANCELLED__".to_string());
    }
    let claims = validate_id_token(&client, &configuration, id_token, client_id, nonce).await?;
    let refresh_token = token.refresh_token.ok_or_else(|| {
        "OpenAI did not return a refresh token for the ChatGPT account".to_string()
    })?;
    let credential = ChatGptCredential {
        client_id: client_id.to_string(),
        ext_agent_host_id: host_id.to_string(),
        subject: claims.sub,
        email: claims.email,
        issuer: configuration.issuer,
        id_token: id_token.to_string(),
        access_token: token.access_token,
        refresh_token,
        scopes,
        expires_at_unix: chrono::Utc::now().timestamp() + token.expires_in as i64,
        plan_usage_acknowledged: false,
    };
    if !state.set_stage(session_id, "saving_credentials") {
        return Err("__CHATGPT_AUTH_CANCELLED__".to_string());
    }
    Ok(credential)
}

async fn write_callback_response(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    message: &str,
) -> Result<(), String> {
    let body =
        format!("<!doctype html><meta charset=\"utf-8\"><title>Foundation</title><p>{message}</p>");
    let response = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n{}",
        body.len(), body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| format!("Unable to complete the ChatGPT sign-in callback: {error}"))
}

async fn load_openid_configuration(
    client: &reqwest::Client,
) -> Result<OpenIdConfiguration, String> {
    let response = client
        .get("https://auth.openai.com/.well-known/openid-configuration")
        .send()
        .await
        .map_err(|error| format!("Unable to load OpenAI sign-in verification metadata: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "OpenAI sign-in metadata request failed (HTTP {})",
            response.status()
        ));
    }
    response
        .json::<OpenIdConfiguration>()
        .await
        .map_err(|error| format!("OpenAI returned invalid sign-in verification metadata: {error}"))
}

async fn validate_id_token(
    client: &reqwest::Client,
    configuration: &OpenIdConfiguration,
    id_token: &str,
    client_id: &str,
    expected_nonce: &str,
) -> Result<IdTokenClaims, String> {
    if configuration.issuer != "https://auth.openai.com" {
        return Err("OpenAI sign-in metadata contains an unexpected issuer".to_string());
    }
    let header = decode_header(id_token)
        .map_err(|error| format!("OpenAI returned an invalid ID token header: {error}"))?;
    let kid = header
        .kid
        .as_deref()
        .ok_or_else(|| "OpenAI ID token did not identify its signing key".to_string())?;
    let jwks_response = client
        .get(&configuration.jwks_uri)
        .send()
        .await
        .map_err(|error| format!("Unable to load OpenAI signing keys: {error}"))?;
    if !jwks_response.status().is_success() {
        return Err(format!(
            "OpenAI signing-key request failed (HTTP {})",
            jwks_response.status()
        ));
    }
    let jwks: jsonwebtoken::jwk::JwkSet = jwks_response
        .json()
        .await
        .map_err(|error| format!("OpenAI returned invalid signing keys: {error}"))?;
    let jwk = jwks
        .find(kid)
        .ok_or_else(|| "OpenAI ID token used an unknown signing key".to_string())?;
    let key = DecodingKey::from_jwk(jwk)
        .map_err(|error| format!("Unable to prepare OpenAI ID token verification: {error}"))?;
    let algorithm = header.alg;
    let mut validation = Validation::new(algorithm);
    validation.set_issuer(&[configuration.issuer.as_str()]);
    validation.set_audience(&[client_id]);
    validation.validate_exp = true;
    let token = decode::<IdTokenClaims>(id_token, &key, &validation)
        .map_err(|_| "OpenAI ID token signature or claims could not be validated".to_string())?;
    if token.claims.iss != configuration.issuer
        || !token.claims.aud.contains(client_id)
        || token.claims.nonce != expected_nonce
        || token.claims.exp <= chrono::Utc::now().timestamp() as usize
    {
        return Err("OpenAI ID token claims did not match this sign-in attempt".to_string());
    }
    Ok(token.claims)
}

fn safe_provider_error(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| "The authorization was not accepted".to_string())
}

pub async fn connection_status(executor: &DbExecutor) -> Result<ChatGptConnectionStatus, String> {
    let credential = read_credential(executor).await?;
    let Some(credential) = credential else {
        return Ok(ChatGptConnectionStatus {
            status: "disconnected".to_string(),
            email: None,
            plan_usage_enabled: false,
            expires_at: None,
            usage: None,
            show_welcome_confirmation: false,
            error: None,
        });
    };
    let usage_enabled = credential
        .scopes
        .iter()
        .any(|scope| scope == "chatgpt.tokens.use.direct");
    let connected = credential.expires_at_unix > chrono::Utc::now().timestamp();
    Ok(ChatGptConnectionStatus {
        status: if !connected {
            "expired".to_string()
        } else {
            "connected".to_string()
        },
        email: credential.email,
        plan_usage_enabled: usage_enabled,
        expires_at: chrono::DateTime::from_timestamp(credential.expires_at_unix, 0)
            .map(|value| value.to_rfc3339()),
        usage: None,
        show_welcome_confirmation: connected
            && usage_enabled
            && !credential.plan_usage_acknowledged,
        error: None,
    })
}

pub async fn acknowledge_plan_usage(executor: &DbExecutor) -> Result<(), String> {
    let mut credential = read_credential(executor)
        .await?
        .ok_or_else(|| "No ChatGPT account is connected".to_string())?;
    if !credential
        .scopes
        .iter()
        .any(|scope| scope == "chatgpt.tokens.use.direct")
    {
        return Err("The connected ChatGPT account has not authorized plan usage".to_string());
    }
    credential.plan_usage_acknowledged = true;
    write_credential(executor, credential).await?;
    Ok(())
}

pub async fn list_models(executor: &DbExecutor) -> Result<Vec<ChatGptModel>, String> {
    let token = access_token(executor).await?;
    let response = reqwest::Client::new()
        .get("https://api.openai.com/v1/models")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| {
            format!("Unable to fetch models available to this ChatGPT account: {error}")
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Unable to list ChatGPT account models (HTTP {status}): {}",
            safe_provider_error(&body)
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|error| format!("Unable to read ChatGPT account model list: {error}"))?;
    parse_model_catalog(&body)
}

pub async fn access_token(executor: &DbExecutor) -> Result<String, String> {
    let credential = read_credential(executor).await?.ok_or_else(|| {
        "Connect a ChatGPT account in Settings before using this model".to_string()
    })?;
    if !credential
        .scopes
        .iter()
        .any(|scope| scope == "chatgpt.tokens.use.direct")
    {
        return Err("The connected ChatGPT account has not authorized plan usage".to_string());
    }
    if credential.expires_at_unix > chrono::Utc::now().timestamp() + 60 {
        return Ok(credential.access_token);
    }
    refresh_credential(executor, credential).await
}

async fn refresh_credential(
    executor: &DbExecutor,
    mut credential: ChatGptCredential,
) -> Result<String, String> {
    let response = reqwest::Client::new()
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", credential.client_id.as_str()),
            ("refresh_token", credential.refresh_token.as_str()),
            ("resource", RESOURCE),
        ])
        .send()
        .await
        .map_err(|error| format!("Unable to refresh the ChatGPT account authorization: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "ChatGPT account authorization expired (HTTP {status}): {}",
            safe_provider_error(&body)
        ));
    }
    let token: TokenResponse = response
        .json()
        .await
        .map_err(|error| format!("OpenAI returned an invalid refreshed token response: {error}"))?;
    if let Some(scope) = token.scope {
        credential.scopes = scope.split_whitespace().map(str::to_string).collect();
    }
    if !credential
        .scopes
        .iter()
        .any(|scope| scope == "chatgpt.tokens.use.direct")
    {
        return Err("The refreshed ChatGPT authorization no longer permits plan usage".to_string());
    }
    credential.access_token = token.access_token;
    if let Some(refresh_token) = token.refresh_token {
        credential.refresh_token = refresh_token;
    }
    if let Some(id_token) = token.id_token {
        credential.id_token = id_token;
    }
    credential.expires_at_unix = chrono::Utc::now().timestamp() + token.expires_in as i64;
    let access_token = credential.access_token.clone();
    write_credential(executor, credential).await?;
    Ok(access_token)
}

pub async fn disconnect(executor: &DbExecutor) -> Result<(), String> {
    let record = executor
        .read(|conn| {
            let service_iri = crate::core_ontology::ai_services::resolve_openai_service(
                conn,
                CHATGPT_AUTHENTICATION_MODE,
            )?;
            let Some(credential_iri) = linked_credential_iri(conn, &service_iri)? else {
                return Ok(None);
            };
            Ok(
                read_credential_for_iri_from_connection(conn, &credential_iri)?
                    .map(|credential| (service_iri, credential_iri, credential)),
            )
        })
        .await?;
    let Some((service_iri, credential_iri, credential)) = record else {
        return Ok(());
    };
    let configuration = load_openid_configuration(&reqwest::Client::new()).await?;
    if let Some(endpoint) = configuration.revocation_endpoint {
        let response = reqwest::Client::new()
            .post(endpoint)
            .form(&[
                ("token", credential.refresh_token.as_str()),
                ("token_type_hint", "refresh_token"),
                ("client_id", credential.client_id.as_str()),
            ])
            .send()
            .await
            .map_err(|error| {
                format!("Unable to revoke the ChatGPT account authorization: {error}")
            })?;
        if !response.status().is_success() {
            return Err(format!(
                "OpenAI could not revoke the ChatGPT account authorization (HTTP {})",
                response.status()
            ));
        }
    }
    executor
        .write(move |conn| {
            Individual::retract(conn, &credential_iri, "chatgpt_auth")
                .map_err(|error| error.to_string())?;
            owl::replace_all_property_iris(
                conn,
                &service_iri,
                "foundation:hasCredential",
                &[],
                "chatgpt_auth",
            )
            .map_err(|error| error.to_string())?;
            Ok(String::new())
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        linked_credential_iri, parse_model_catalog, pkce_challenge, Audience, ChatGptAuthSession,
        ChatGptAuthState,
    };
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{self, Object};
    use std::sync::{Arc, Barrier};

    #[test]
    fn model_catalog_maps_visible_siwc_models_in_server_order() {
        let models = parse_model_catalog(
            br#"{"models":[{"slug":"gpt-alpha","display_name":"GPT Alpha","visibility":"list"},{"slug":"gpt-beta","display_name":"GPT Beta","visibility":"list"}]}"#,
        )
        .expect("valid SIWC model catalog");

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-alpha");
        assert_eq!(models[0].name, "GPT Alpha");
        assert_eq!(models[0].owned_by, "OpenAI");
        assert_eq!(models[1].id, "gpt-beta");
    }

    #[test]
    fn model_catalog_excludes_models_not_visible_in_picker() {
        let models = parse_model_catalog(
            br#"{"models":[{"slug":"gpt-visible","display_name":"Visible","visibility":"list"},{"slug":"gpt-hidden","display_name":"Hidden","visibility":"internal"}]}"#,
        )
        .expect("valid SIWC model catalog");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-visible");
    }

    #[test]
    fn model_catalog_accepts_an_empty_list() {
        let models = parse_model_catalog(br#"{"models":[]}"#).expect("valid empty catalog");

        assert!(models.is_empty());
    }

    #[test]
    fn credential_identity_is_resolved_from_service_link() {
        let mut conn = setup_test_db();
        owl::assert_raw_triples_with_tx(
            &mut conn,
            &[owl::Triple::new(
                "foundation:service",
                "rdf:type",
                Object::Iri("foundation:AIAPIService".to_string()),
            )],
            "test",
        )
        .unwrap();

        assert_eq!(
            linked_credential_iri(&conn, "foundation:service").unwrap(),
            None
        );

        owl::assert_raw_triples_with_tx(
            &mut conn,
            &[owl::Triple::new(
                "foundation:service",
                "foundation:hasCredential",
                Object::Iri("foundation:credential_existing".to_string()),
            )],
            "test",
        )
        .unwrap();
        assert_eq!(
            linked_credential_iri(&conn, "foundation:service").unwrap(),
            Some("foundation:credential_existing".to_string())
        );
    }

    #[test]
    fn credential_identity_rejects_ambiguous_service_links() {
        let mut conn = setup_test_db();
        owl::assert_raw_triples_with_tx(
            &mut conn,
            &[
                owl::Triple::new(
                    "foundation:service",
                    "rdf:type",
                    Object::Iri("foundation:AIAPIService".to_string()),
                ),
                owl::Triple::new(
                    "foundation:service",
                    "foundation:hasCredential",
                    Object::Iri("foundation:credential_a".to_string()),
                ),
                owl::Triple::new(
                    "foundation:service",
                    "foundation:hasCredential",
                    Object::Iri("foundation:credential_b".to_string()),
                ),
            ],
            "test",
        )
        .unwrap();

        assert!(linked_credential_iri(&conn, "foundation:service")
            .unwrap_err()
            .contains("multiple linked credentials"));
    }

    #[test]
    fn active_session_can_be_found_without_the_session_id() {
        let state = ChatGptAuthState::default();
        state.set_session(ChatGptAuthSession {
            auth_session_id: "session-1".to_string(),
            status: "authorizing".to_string(),
            stage: "waiting_for_callback".to_string(),
            email: None,
            error: None,
        });

        let active = state.get_active_session().expect("session is active");
        assert_eq!(active.auth_session_id, "session-1");
        assert_eq!(active.status, "authorizing");
    }

    #[test]
    fn concurrent_session_reservations_keep_one_canonical_authorization() {
        let state = ChatGptAuthState::default();
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for session_id in ["session-a", "session-b"] {
            let state = state.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                state.set_session(ChatGptAuthSession {
                    auth_session_id: session_id.to_string(),
                    status: "authorizing".to_string(),
                    stage: "waiting_for_callback".to_string(),
                    email: None,
                    error: None,
                })
            }));
        }
        barrier.wait();

        let accepted = workers
            .into_iter()
            .map(|worker| worker.join().expect("reservation thread completes"))
            .filter(|accepted| *accepted)
            .count();
        assert_eq!(accepted, 1);
        assert!(state.get_active_session().is_some());
    }

    #[test]
    fn active_session_with_url_is_reused_for_repeated_start_requests() {
        let state = ChatGptAuthState::default();
        state.set_session(ChatGptAuthSession {
            auth_session_id: "canonical-session".to_string(),
            status: "authorizing".to_string(),
            stage: "waiting_for_callback".to_string(),
            email: None,
            error: None,
        });
        assert!(state.set_authorization_url(
            "canonical-session",
            "https://auth.example.test/authorize?state=one-use".to_string()
        ));

        let (session, url) = state
            .active_session_with_url()
            .expect("active session is reusable");
        assert_eq!(session.auth_session_id, "canonical-session");
        assert_eq!(url, "https://auth.example.test/authorize?state=one-use");
    }

    #[test]
    fn only_first_terminal_transition_succeeds_and_late_callback_is_ignored() {
        let state = ChatGptAuthState::default();
        state.set_session(ChatGptAuthSession {
            auth_session_id: "session-a".to_string(),
            status: "authorizing".to_string(),
            stage: "exchanging_code".to_string(),
            email: None,
            error: None,
        });

        assert!(
            state.finish_connected_session("session-a", Some("person@example.test".to_string()))
        );
        assert!(
            !state.finish_connected_session("session-a", Some("person@example.test".to_string()))
        );
        assert!(!state.set_stage("session-a", "validating_identity"));
        assert_eq!(state.get_session("session-a").unwrap().status, "connected");
    }

    #[tokio::test]
    async fn cancelled_session_rejects_a_delayed_callback_transition() {
        let state = ChatGptAuthState::default();
        state.set_session(ChatGptAuthSession {
            auth_session_id: "session-a".to_string(),
            status: "authorizing".to_string(),
            stage: "waiting_for_callback".to_string(),
            email: None,
            error: None,
        });

        let cancelled = state
            .cancel_session("session-a")
            .await
            .expect("cancellation succeeds")
            .expect("active session was cancelled");
        assert_eq!(cancelled.status, "cancelled");
        assert!(!state.set_stage("session-a", "callback_received"));
        assert_eq!(state.get_session("session-a").unwrap().status, "cancelled");
    }

    #[test]
    fn pkce_challenge_uses_base64url_sha256_without_padding() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn audience_accepts_single_or_multiple_values() {
        assert!(Audience::Single("client".to_string()).contains("client"));
        assert!(
            Audience::Multiple(vec!["other".to_string(), "client".to_string()]).contains("client")
        );
    }
}
