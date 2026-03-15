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

    executor.write(move |conn| async move {
        // Retract previous credential link if one exists
        if let Ok(Some(old_cred_iri)) = crate::owl::get_iri_property(&conn, &connector_iri, "foundation:hasCredential").await {
            Individual::retract(&conn, &old_cred_iri, "connector").await
                .map_err(|e| format!("Failed to retract old credential: {}", e))?;
        }

        let auth_type = credential.get("auth_type").and_then(|v| v.as_str()).unwrap_or("api_key");

        let (concept_iri, cred_iri) = match auth_type {
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
            _ => return Err(format!("Unknown auth_type: {}", auth_type)),
        };

        let ind = Individual::new(&cred_iri);
        ind.assert(&conn, concept_iri, &cred_iri, "vpn_key", "connector").await
            .map_err(|e| format!("Failed to create credential: {}", e))?;

        match auth_type {
            "api_key" | "token" => {
                let value = credential.get("value").and_then(|v| v.as_str())
                    .ok_or("Missing 'value' field")?;
                ind.add_property(&conn, "foundation:credentialValue",
                    vec![str_literal(value)], "connector").await
                    .map_err(|e| format!("Failed to set credentialValue: {}", e))?;
            }
            "username_password" => {
                let username = credential.get("username").and_then(|v| v.as_str())
                    .ok_or("Missing 'username' field")?;
                let password = credential.get("password").and_then(|v| v.as_str())
                    .ok_or("Missing 'password' field")?;
                ind.add_property(&conn, "foundation:credentialUsername",
                    vec![str_literal(username)], "connector").await
                    .map_err(|e| format!("Failed to set username: {}", e))?;
                ind.add_property(&conn, "foundation:credentialValue",
                    vec![str_literal(password)], "connector").await
                    .map_err(|e| format!("Failed to set password: {}", e))?;
            }
            _ => {}
        }

        ind.add_property(&conn, "foundation:credentialCreatedAt",
            vec![Object::DateTime(chrono::DateTime::from_timestamp_millis(timestamp).unwrap_or_default().to_rfc3339())], "connector").await
            .map_err(|e| format!("Failed to set timestamp: {}", e))?;

        // Link credential to connector
        let connector = Individual::new(&connector_iri);
        connector.add_property(&conn, "foundation:hasCredential",
            vec![Object::Iri(cred_iri.clone())], "connector").await
            .map_err(|e| format!("Failed to link credential: {}", e))?;

        // Update authType on the connector
        connector.add_property(&conn, "foundation:authType",
            vec![str_literal(auth_type)], "connector").await
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
    executor.read(move |conn| async move {
        let auth_type = crate::owl::get_literal_property(&conn, &connector_iri, "foundation:authType").await
            .map_err(|e| e.to_string())?;

        let credential_iri = crate::owl::get_iri_property(&conn, &connector_iri, "foundation:hasCredential").await
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
    let (base_url, auth_type, cred_iri) = executor.read(move |conn| async move {
        let base_url = crate::owl::get_literal_property(&conn, &connector_iri, "foundation:baseUrl").await
            .map_err(|e| e.to_string())?;
        let auth_type = crate::owl::get_literal_property(&conn, &connector_iri, "foundation:authType").await
            .map_err(|e| e.to_string())?;
        let cred_iri = crate::owl::get_iri_property(&conn, &connector_iri, "foundation:hasCredential").await
            .map_err(|e| e.to_string())?;
        Ok((base_url, auth_type, cred_iri))
    }).await?;

    let cred_iri = cred_iri.ok_or("No credential configured for this connector")?;
    let base_url = base_url.ok_or("No baseUrl configured for this connector")?;
    let auth_type = auth_type.unwrap_or_else(|| "api_key".to_string());

    let (value, username) = executor.read(move |conn| async move {
        let value = crate::owl::get_literal_property(&conn, &cred_iri, "foundation:credentialValue").await
            .map_err(|e| e.to_string())?;
        let username = crate::owl::get_literal_property(&conn, &cred_iri, "foundation:credentialUsername").await
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
