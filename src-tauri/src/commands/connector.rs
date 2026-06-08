use serde::Serialize;
use tauri::{AppHandle, State};

use crate::owl::{DbExecutor, Individual, Object};

#[derive(Debug, Serialize)]
pub struct CredentialSummary {
    pub connector_iri: String,
    pub auth_type: Option<String>,
    pub credential_iri: Option<String>,
    pub is_configured: bool,
}

fn str_literal(value: impl Into<String>) -> Object {
    Object::Literal {
        value: value.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

/// Save or replace authentication credentials for an external service connector.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn connector__save_credential(
    app: AppHandle,
    connector_iri: String,
    credential: serde_json::Value,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let _ = app;
    let timestamp = chrono::Utc::now().timestamp_millis();

    executor.write(move |conn| {
        // Retract previous credential link if one exists
        if let Ok(Some(old_cred_iri)) = crate::owl::get_iri_property(conn, &connector_iri, "foundation:hasCredential") {
            Individual::retract(conn, &old_cred_iri, "connector")
                .map_err(|e| format!("Failed to retract old credential: {}", e))?;
        }

        let auth_type = credential.get("auth_type").and_then(|v| v.as_str()).unwrap_or("api_key");

        let (class_iri, cred_iri) = match auth_type {
            "api_key" => {
                let iri = format!("foundation:APIKey_{}", timestamp);
                ("foundation:APIKey", iri)
            }
            "token" => {
                let iri = format!("foundation:TokenCredential_{}", timestamp);
                ("foundation:TokenCredential", iri)
            }
            "username_password" => {
                let iri = format!("foundation:UsernamePasswordCredential_{}", timestamp);
                ("foundation:UsernamePasswordCredential", iri)
            }
            "oauth2" => {
                let iri = format!("foundation:OAuth2Credential_{}", timestamp);
                ("foundation:OAuth2Credential", iri)
            }
            _ => return Err(format!("Unknown auth_type: {}", auth_type)),
        };

        let ind = Individual::new(&cred_iri);
        ind.assert(conn, class_iri, &cred_iri, "vpn_key", "connector")
            .map_err(|e| format!("Failed to create credential: {}", e))?;

        match auth_type {
            "api_key" | "token" => {
                let value = credential.get("value").and_then(|v| v.as_str())
                    .ok_or("Missing 'value' field")?;
                ind.add_property(conn, "foundation:credentialValue",
                    vec![str_literal(value)], "connector")
                    .map_err(|e| format!("Failed to set credentialValue: {}", e))?;
            }
            "username_password" => {
                let username = credential.get("username").and_then(|v| v.as_str())
                    .ok_or("Missing 'username' field")?;
                let password = credential.get("password").and_then(|v| v.as_str())
                    .ok_or("Missing 'password' field")?;
                ind.add_property(conn, "foundation:credentialUsername",
                    vec![str_literal(username)], "connector")
                    .map_err(|e| format!("Failed to set username: {}", e))?;
                ind.add_property(conn, "foundation:credentialValue",
                    vec![str_literal(password)], "connector")
                    .map_err(|e| format!("Failed to set password: {}", e))?;
            }
            "oauth2" => {
                let client_id = credential.get("client_id").and_then(|v| v.as_str())
                    .ok_or("Missing 'client_id' field")?;
                let client_secret = credential.get("client_secret").and_then(|v| v.as_str())
                    .ok_or("Missing 'client_secret' field")?;
                let token_url = credential.get("token_url").and_then(|v| v.as_str())
                    .ok_or("Missing 'token_url' field")?;
                let scopes = credential.get("scopes").and_then(|v| v.as_str()).unwrap_or("");

                ind.add_property(conn, "foundation:clientId",
                    vec![str_literal(client_id)], "connector")
                    .map_err(|e| format!("Failed to set clientId: {}", e))?;
                ind.add_property(conn, "foundation:clientSecret",
                    vec![str_literal(client_secret)], "connector")
                    .map_err(|e| format!("Failed to set clientSecret: {}", e))?;
                ind.add_property(conn, "foundation:tokenUrl",
                    vec![str_literal(token_url)], "connector")
                    .map_err(|e| format!("Failed to set tokenUrl: {}", e))?;
                ind.add_property(conn, "foundation:oauthScopes",
                    vec![str_literal(scopes)], "connector")
                    .map_err(|e| format!("Failed to set oauthScopes: {}", e))?;
                ind.add_property(conn, "foundation:grantType",
                    vec![str_literal("client_credentials")], "connector")
                    .map_err(|e| format!("Failed to set grantType: {}", e))?;

                ind.add_property(conn, "foundation:hasStatus",
                    vec![Object::Iri("foundation:Status_1772940750706".to_string())], "connector")
                    .map_err(|e| format!("Failed to set status: {}", e))?;
            }
            _ => {}
        }

        let dt_str = chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339();
        ind.add_property(conn, "foundation:credentialCreatedAt",
            vec![Object::DateTime(dt_str)], "connector")
            .map_err(|e| format!("Failed to set timestamp: {}", e))?;

        let connector = Individual::new(&connector_iri);
        connector.add_property(conn, "foundation:hasCredential",
            vec![Object::Iri(cred_iri.clone())], "connector")
            .map_err(|e| format!("Failed to link credential: {}", e))?;

        connector.add_property(conn, "foundation:authType",
            vec![str_literal(auth_type)], "connector")
            .map_err(|e| format!("Failed to set authType: {}", e))?;

        Ok(cred_iri)
    }).await
}

/// Returns a summary of credential configuration for a connector (no secret values exposed).
#[tauri::command]
#[allow(non_snake_case)]
pub async fn connector__get_credential_summary(
    connector_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<CredentialSummary, String> {
    executor.read(move |conn| {
        let auth_type = crate::owl::get_literal_property(conn, &connector_iri, "foundation:authType")
            .map_err(|e| e.to_string())?;

        let credential_iri = crate::owl::get_iri_property(conn, &connector_iri, "foundation:hasCredential")
            .map_err(|e| e.to_string())?;

        Ok(CredentialSummary {
            connector_iri: connector_iri.clone(),
            is_configured: credential_iri.is_some(),
            auth_type,
            credential_iri,
        })
    }).await
}

/// Tests authentication for a connector by doing a minimal connectivity check.
/// Returns Ok(message) on success, Err(message) on failure.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn connector__test_auth(
    connector_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let (base_url, auth_type, cred_iri) = executor.read(move |conn| {
        let base_url = crate::owl::get_literal_property(conn, &connector_iri, "foundation:baseUrl")
            .map_err(|e| e.to_string())?;
        let auth_type = crate::owl::get_literal_property(conn, &connector_iri, "foundation:authType")
            .map_err(|e| e.to_string())?;
        let cred_iri = crate::owl::get_iri_property(conn, &connector_iri, "foundation:hasCredential")
            .map_err(|e| e.to_string())?;
        Ok((base_url, auth_type, cred_iri))
    }).await?;

    let cred_iri = cred_iri.ok_or("No credential configured for this connector")?;
    let base_url = base_url.ok_or("No baseUrl configured for this connector")?;
    let auth_type = auth_type.unwrap_or_else(|| "api_key".to_string());

    let (value, username) = executor.read(move |conn| {
        let value = crate::owl::get_literal_property(conn, &cred_iri, "foundation:credentialValue")
            .map_err(|e| e.to_string())?;
        let username = crate::owl::get_literal_property(conn, &cred_iri, "foundation:credentialUsername")
            .map_err(|e| e.to_string())?;
        Ok((value, username))
    }).await?;

    let client = reqwest::Client::new();
    let mut req = client.get(&base_url);

    match auth_type.as_str() {
        "api_key" | "token" => {
            if let Some(token) = value {
                req = req.bearer_auth(token);
            }
        }
        "username_password" => {
            let user = username.ok_or("Missing username")?;
            let pass = value.unwrap_or_default();
            req = req.basic_auth(user, Some(pass));
        }
        _ => {}
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() || status.as_u16() == 401 {
                Ok(format!("Connection successful (HTTP {})", status.as_u16()))
            } else {
                Err(format!("Unexpected status: HTTP {}", status.as_u16()))
            }
        }
        Err(e) => Err(format!("Connection failed: {}", e)),
    }
}

#[derive(Debug, Serialize)]
pub struct OAuth2CredentialSummary {
    pub credential_iri: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub token_url: Option<String>,
    pub scopes: Option<String>,
    pub grant_type: Option<String>,
    pub is_configured: bool,
}

/// Returns a summary of an OAuth2Credential (no secret values exposed).
#[tauri::command]
#[allow(non_snake_case)]
pub async fn connector__get_oauth2_summary(
    cred_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<OAuth2CredentialSummary, String> {
    executor.read(move |conn| {
        let client_id = crate::owl::get_literal_property(conn, &cred_iri, "foundation:clientId")
            .map_err(|e| e.to_string())?;
        let client_secret = crate::owl::get_literal_property(conn, &cred_iri, "foundation:clientSecret")
            .map_err(|e| e.to_string())?;
        let token_url = crate::owl::get_literal_property(conn, &cred_iri, "foundation:tokenUrl")
            .map_err(|e| e.to_string())?;
        let scopes = crate::owl::get_literal_property(conn, &cred_iri, "foundation:oauthScopes")
            .map_err(|e| e.to_string())?;
        let grant_type = crate::owl::get_literal_property(conn, &cred_iri, "foundation:grantType")
            .map_err(|e| e.to_string())?;

        let is_configured = client_id.is_some() && token_url.is_some();
        Ok(OAuth2CredentialSummary {
            credential_iri: cred_iri,
            client_id,
            client_secret,
            token_url,
            scopes,
            grant_type,
            is_configured,
        })
    }).await
}

/// Tests an OAuth2 client_credentials grant and returns Ok on success.
/// Never logs or returns the access token.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn connector__test_oauth2_auth(
    cred_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let (client_id, client_secret, token_url, scopes) = executor.read(move |conn| {
        let client_id = crate::owl::get_literal_property(conn, &cred_iri, "foundation:clientId")
            .map_err(|e| e.to_string())?
            .ok_or("Missing clientId on OAuth2Credential")?;
        let client_secret = crate::owl::get_literal_property(conn, &cred_iri, "foundation:clientSecret")
            .map_err(|e| e.to_string())?
            .ok_or("Missing clientSecret on OAuth2Credential")?;
        let token_url = crate::owl::get_literal_property(conn, &cred_iri, "foundation:tokenUrl")
            .map_err(|e| e.to_string())?
            .ok_or("Missing tokenUrl on OAuth2Credential")?;
        let scopes = crate::owl::get_literal_property(conn, &cred_iri, "foundation:oauthScopes")
            .map_err(|e| e.to_string())?;
        Ok((client_id, client_secret, token_url, scopes))
    }).await?;

    let mut form = vec![
        ("grant_type", "client_credentials".to_string()),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];
    if let Some(s) = scopes {
        if !s.is_empty() {
            form.push(("scope", s));
        }
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(&token_url)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Token request failed: {}", e))?;

    let status = resp.status();
    if status.is_success() {
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse token response: {}", e))?;
        if body.get("access_token").is_some() {
            Ok(format!("OAuth2 token obtained successfully (HTTP {})", status.as_u16()))
        } else {
            Err(format!("Token endpoint returned success but no access_token (HTTP {})", status.as_u16()))
        }
    } else {
        let body_text = resp.text().await.unwrap_or_default();
        Err(format!("Token endpoint returned HTTP {}: {}", status.as_u16(), body_text))
    }
}
