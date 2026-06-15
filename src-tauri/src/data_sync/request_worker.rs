use tauri::{AppHandle, Manager};

use crate::commands::log_backend;
use crate::data_sync::source::{load_data_source_config, DataSourceConfig};
use crate::data_sync::staging::{stage_raw_record, StagingInput};
use crate::owl::DbExecutor;

/// Boots the data sync request worker. Called during app startup (setup.rs).
/// Currently a no-op listener placeholder — extract cycles are triggered
/// explicitly via `datasync_run` MCP tool or the scheduled Automation.
pub fn start_request_worker(_app: AppHandle) {}

/// Triggered by `datasync_run` or scheduled extract: performs an HTTP GET
/// against the DataSource endpoint, persists the HTTPResponse via the motor
/// (handled by request_task.rs), and then slices the body here.
pub async fn run_extract_cycle(
    app: &AppHandle,
    data_source_iri: &str,
) -> Result<Vec<String>, String> {
    let executor = app.state::<DbExecutor>();

    let config = load_data_source_config(&executor, data_source_iri).await?;

    let body = perform_extract_request(app, &config).await.map_err(|e| {
        let ds_iri = data_source_iri.to_string();
        let e_clone = e.clone();
        let executor_clone = executor.inner().clone();
        tauri::async_runtime::spawn(async move {
            let _ = executor_clone.write(move |conn| {
                crate::core_ontology::data_sync::update_datasource_connection(
                    conn, &ds_iri, false, Some(&e_clone),
                )?;
                Ok(String::new())
            }).await;
        });
        e
    })?;

    let items = extract_items(&body, &config.item_path);

    let mut staged_iris = Vec::new();
    for (i, item_json) in items.iter().enumerate() {
        let external_id = extract_external_id(item_json);
        let source_ref = format!("extract:{}:{}", data_source_iri, i);

        let result = stage_raw_record(
            &executor,
            StagingInput {
                data_source_iri,
                payload: serde_json::to_vec(item_json).unwrap_or_default(),
                external_id,
                source_ref,
            },
        ).await;

        match result {
            Ok(iri) => staged_iris.push(iri),
            Err(e) => log_backend("error", &format!(
                "[request_worker] staging item {} failed: {}", i, e
            )),
        }
    }

    let ds_iri = data_source_iri.to_string();
    executor.write(move |conn| {
        crate::core_ontology::data_sync::update_datasource_connection(
            conn, &ds_iri, true, None,
        )?;
        Ok(String::new())
    }).await?;

    Ok(staged_iris)
}

async fn perform_extract_request(
    app: &AppHandle,
    config: &DataSourceConfig,
) -> Result<String, String> {
    let url = if config.target_endpoint.starts_with("http") {
        config.target_endpoint.clone()
    } else {
        format!("{}{}", config.base_url.trim_end_matches('/'), config.target_endpoint)
    };

    let client = reqwest::Client::new();
    let mut req = client.get(&url);

    if let Some(cred_iri) = &config.cred_iri {
        req = apply_credential_to_request(app, req, cred_iri).await?;
    }

    let resp = req.send().await.map_err(|e| format!("HTTP request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Extract returned HTTP {}", status.as_u16()));
    }

    resp.text().await.map_err(|e| format!("Failed to read response body: {}", e))
}

async fn apply_credential_to_request(
    app: &AppHandle,
    mut req: reqwest::RequestBuilder,
    cred_iri: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let executor = app.state::<DbExecutor>();
    let cred_iri_clone = cred_iri.to_string();

    let (cred_type, cred_value, cred_username) = executor
        .read(move |conn| {
            let cred_type = crate::owl::get_iri_property(conn, &cred_iri_clone, "rdf:type")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let cred_value = crate::owl::get_literal_property(conn, &cred_iri_clone, "foundation:credentialValue")
                .map_err(|e| e.to_string())?;
            let cred_username = crate::owl::get_literal_property(conn, &cred_iri_clone, "foundation:credentialUsername")
                .map_err(|e| e.to_string())?;
            Ok((cred_type, cred_value, cred_username))
        })
        .await?;

    match cred_type.as_str() {
        "foundation:APIKey" | "foundation:TokenCredential" => {
            if let Some(token) = cred_value {
                req = req.bearer_auth(token);
            }
        }
        "foundation:UsernamePasswordCredential" => {
            let user = cred_username.ok_or("Missing username on credential")?;
            let pass = cred_value.unwrap_or_default();
            req = req.basic_auth(user, Some(pass));
        }
        _ => {}
    }

    Ok(req)
}

/// Extracts the array of items from the response body using the `item_path` JSON pointer.
///
/// - Empty path: the body itself must be a JSON array.
/// - `/some/path`: uses JSON pointer to locate the array.
///
/// Non-array values are wrapped in a single-item vec.
fn extract_items(body: &str, item_path: &str) -> Vec<serde_json::Value> {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![serde_json::Value::String(body.to_string())],
    };

    let target = if item_path.is_empty() {
        &parsed
    } else {
        parsed.pointer(item_path).unwrap_or(&parsed)
    };

    match target {
        serde_json::Value::Array(arr) => arr.clone(),
        other => vec![other.clone()],
    }
}

/// Attempts to extract a stable external identifier from a JSON item.
/// Tries common id field names in order.
fn extract_external_id(item: &serde_json::Value) -> Option<String> {
    for key in &["id", "ID", "uuid", "key", "externalId", "external_id", "uid"] {
        if let Some(v) = item.get(key) {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => continue,
            };
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}
