use chrono::Utc;
use tauri::{AppHandle, Manager};

use crate::eavto::query;
use crate::owl::{
    get_iri_property, get_literal_property, replace_all_property_literals,
    DbExecutor, Individual, Object,
};

use super::executor::{ExecutionContext, interpolate_with_db};

type Result<T> = std::result::Result<T, String>;

fn str_lit(v: impl Into<String>) -> Object {
    Object::Literal {
        value: v.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

/// Serializes the HTTP result (success or network error) as a JSON literal for outputProperty.
fn build_result_json(
    status: Option<i64>,
    headers: Option<&serde_json::Map<String, serde_json::Value>>,
    body: &str,
    error: Option<&str>,
) -> String {
    let mut map = serde_json::Map::new();
    match status {
        Some(s) => { map.insert("status".to_string(), serde_json::Value::Number(s.into())); }
        None    => { map.insert("status".to_string(), serde_json::Value::Null); }
    }
    if let Some(hdrs) = headers {
        map.insert("headers".to_string(), serde_json::Value::Object(hdrs.clone()));
    } else {
        map.insert("headers".to_string(), serde_json::Value::Object(serde_json::Map::new()));
    }
    map.insert("body".to_string(), serde_json::Value::String(body.to_string()));
    if let Some(e) = error {
        map.insert("error".to_string(), serde_json::Value::String(e.to_string()));
    }
    serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default()
}

/// Writes the HTTP result JSON to the control instance's outputProperty, if configured.
/// Non-fatal: logs on failure but does not abort execution.
async fn write_output_to_control_instance(
    executor: &DbExecutor,
    node_iri: &str,
    ctx: &ExecutionContext,
    result_json: &str,
) {
    let output_prop = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                get_iri_property(conn, &node_iri, "foundation:outputProperty")
                    .map_err(|e| e.to_string())
            }
        })
        .await
        .ok()
        .flatten();

    let Some(output_prop_iri) = output_prop else { return };
    let Some(control_iri) = ctx.get("self").cloned() else { return };

    let result_json = result_json.to_string();
    executor
        .write(move |conn| {
            replace_all_property_literals(
                conn,
                &control_iri,
                &output_prop_iri,
                &[result_json.as_str()],
                "process_automation",
            )
            .map_err(|e| e.to_string())?;
            Ok(String::new())
        })
        .await
        .unwrap_or_else(|e| {
            crate::commands::log_backend(
                "error",
                &format!("[request_task] failed to write result to outputProperty: {}", e),
            );
            String::new()
        });
}

/// Executes an automation_RequestTask node: reads its linked HTTPRequest, performs the HTTP call,
/// persists an HTTPResponse, writes the result to the control instance's outputProperty, and
/// returns the response body.
pub async fn execute_request_task(
    app: &AppHandle,
    node_iri: &str,
    ctx: &ExecutionContext,
) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let request_iri = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                let result = query::get_by_entity_predicate(conn, &node_iri, "foundation:requestInputRefs")
                    .map_err(|e| e.to_string())?;
                Ok(result
                    .triples
                    .first()
                    .and_then(|t| t.object.as_iri())
                    .map(|s| s.to_string()))
            }
        })
        .await?
        .ok_or_else(|| format!("RequestTask {} has no requestInputRefs", node_iri))?;

    let (url, method, body, headers_json, cred_iri) = executor
        .read({
            let request_iri = request_iri.clone();
            move |conn| {
                let url = get_literal_property(conn, &request_iri, "foundation:httpUrl")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("HTTPRequest {} has no httpUrl", request_iri))?;
                let method = get_literal_property(conn, &request_iri, "foundation:httpMethod")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| "GET".to_string());
                let body = get_literal_property(conn, &request_iri, "foundation:httpBody")
                    .map_err(|e| e.to_string())?;
                let headers_json = get_literal_property(conn, &request_iri, "foundation:httpHeaders")
                    .map_err(|e| e.to_string())?;
                let cred_iri = get_iri_property(conn, &request_iri, "foundation:usesCredential")
                    .map_err(|e| e.to_string())?;
                Ok((url, method, body, headers_json, cred_iri))
            }
        })
        .await?;

    let resolved_url = interpolate_with_db(&url, ctx, &executor).await;
    let resolved_body = match body {
        Some(b) => Some(interpolate_with_db(&b, ctx, &executor).await),
        None => None,
    };

    let client = reqwest::Client::new();
    let mut req = match method.to_uppercase().as_str() {
        "POST" => client.post(&resolved_url),
        "PUT" => client.put(&resolved_url),
        "PATCH" => client.patch(&resolved_url),
        "DELETE" => client.delete(&resolved_url),
        _ => client.get(&resolved_url),
    };

    // Parse headers JSON once, then interpolate each value individually to avoid
    // corrupting the JSON structure if a placeholder value contains special characters.
    if let Some(ref headers_str) = headers_json {
        if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(headers_str) {
            for (k, v) in map {
                if let Some(v_str) = v.as_str() {
                    let resolved_value = interpolate_with_db(v_str, ctx, &executor).await;
                    if let (Ok(name), Ok(value)) = (
                        reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                        reqwest::header::HeaderValue::from_str(&resolved_value),
                    ) {
                        req = req.header(name, value);
                    }
                }
            }
        }
    }

    if let Some(ref cred_iri) = cred_iri {
        req = apply_credential(app, req, cred_iri).await?;
    }

    if let Some(body_str) = resolved_body {
        req = req.body(body_str);
    }

    let start_ms = Utc::now().timestamp_millis();

    let send_result = req.send().await;

    let response = match send_result {
        Ok(resp) => resp,
        Err(e) => {
            let error_msg = format!("HTTP request failed: {}", e);
            let result_json = build_result_json(None, None, "", Some(&error_msg));
            write_output_to_control_instance(&executor, node_iri, ctx, &result_json).await;
            return Err(error_msg);
        }
    };

    let status_code = response.status().as_u16() as i64;

    // Collect response headers before consuming the response body.
    let response_headers: serde_json::Map<String, serde_json::Value> = response
        .headers()
        .iter()
        .filter_map(|(k, v)| {
            v.to_str().ok().map(|s| (k.to_string(), serde_json::Value::String(s.to_string())))
        })
        .collect();

    let resp_body = response.text().await.unwrap_or_default();

    let end_ms = Utc::now().timestamp_millis();

    // Write structured result to the control instance's outputProperty (canonical chaining path).
    let result_json = build_result_json(
        Some(status_code),
        Some(&response_headers),
        &resp_body,
        None,
    );
    write_output_to_control_instance(&executor, node_iri, ctx, &result_json).await;

    // Persist the HTTPResponse as an auditable record (AC 442).
    let response_iri = format!("foundation:HTTPResponse_{}", end_ms);
    executor
        .write({
            let request_iri = request_iri.clone();
            let response_iri = response_iri.clone();
            let resp_body_clone = resp_body.clone();
            move |conn| {
                let dt = |ms: i64| -> Object {
                    Object::Literal {
                        value: chrono::DateTime::from_timestamp_millis(ms)
                            .unwrap_or_default()
                            .to_rfc3339(),
                        datatype: Some("xsd:dateTime".to_string()),
                        language: None,
                    }
                };
                let ind = Individual::new(&response_iri);
                ind.assert(conn, "foundation:HTTPResponse", &response_iri, "http", "process_automation")
                    .map_err(|e| e.to_string())?;
                ind.add_property(conn, "foundation:httpStatusCode",
                    vec![Object::Integer(status_code)], "process_automation")
                    .map_err(|e| e.to_string())?;
                ind.add_property(conn, "foundation:httpResponseBody",
                    vec![str_lit(resp_body_clone)], "process_automation")
                    .map_err(|e| e.to_string())?;
                ind.add_property(conn, "foundation:respondsTo",
                    vec![Object::Iri(request_iri.clone())], "process_automation")
                    .map_err(|e| e.to_string())?;
                ind.add_property(conn, "foundation:hasStartTime",
                    vec![dt(start_ms)], "process_automation")
                    .map_err(|e| e.to_string())?;
                ind.add_property(conn, "foundation:hasEndTime",
                    vec![dt(end_ms)], "process_automation")
                    .map_err(|e| e.to_string())?;
                Individual::new(&request_iri)
                    .add_property(conn, "foundation:hasResponse",
                        vec![Object::Iri(response_iri.clone())], "process_automation")
                    .map_err(|e| e.to_string())?;
                Ok(response_iri)
            }
        })
        .await?;

    Ok(resp_body)
}

async fn apply_credential(
    app: &AppHandle,
    mut req: reqwest::RequestBuilder,
    cred_iri: &str,
) -> Result<reqwest::RequestBuilder> {
    let executor = app.state::<DbExecutor>();

    let (cred_type, cred_value, cred_username) = executor
        .read({
            let cred_iri = cred_iri.to_string();
            move |conn| {
                let type_result = query::get_by_entity_predicate(conn, &cred_iri, "rdf:type")
                    .map_err(|e| e.to_string())?;
                let cred_type = type_result
                    .triples
                    .first()
                    .and_then(|t| t.object.as_iri())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let cred_value = get_literal_property(conn, &cred_iri, "foundation:credentialValue")
                    .map_err(|e| e.to_string())?;
                let cred_username = get_literal_property(conn, &cred_iri, "foundation:credentialUsername")
                    .map_err(|e| e.to_string())?;

                Ok((cred_type, cred_value, cred_username))
            }
        })
        .await?;

    match cred_type.as_str() {
        "foundation:APIKey" | "foundation:TokenCredential" => {
            if let Some(token) = cred_value {
                req = req.bearer_auth(token);
            }
        }
        "foundation:UsernamePasswordCredential" => {
            let user = cred_username.ok_or_else(|| "Missing username on credential".to_string())?;
            let pass = cred_value.unwrap_or_default();
            req = req.basic_auth(user, Some(pass));
        }
        "foundation:OAuth2Credential" => {
            req = apply_oauth2_credential(app, req, cred_iri).await?;
        }
        _ => {}
    }

    Ok(req)
}

async fn apply_oauth2_credential(
    app: &AppHandle,
    req: reqwest::RequestBuilder,
    cred_iri: &str,
) -> Result<reqwest::RequestBuilder> {
    let executor = app.state::<DbExecutor>();

    let (client_id, client_secret, token_url, scopes) = executor
        .read({
            let cred_iri = cred_iri.to_string();
            move |conn| {
                let client_id = get_literal_property(conn, &cred_iri, "foundation:clientId")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("OAuth2Credential {} missing clientId", cred_iri))?;
                let client_secret = get_literal_property(conn, &cred_iri, "foundation:clientSecret")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("OAuth2Credential {} missing clientSecret", cred_iri))?;
                let token_url = get_literal_property(conn, &cred_iri, "foundation:tokenUrl")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("OAuth2Credential {} missing tokenUrl", cred_iri))?;
                let scopes = get_literal_property(conn, &cred_iri, "foundation:oauthScopes")
                    .map_err(|e| e.to_string())?;
                Ok((client_id, client_secret, token_url, scopes))
            }
        })
        .await?;

    let mut form = vec![
        ("grant_type", "client_credentials".to_string()),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];
    if let Some(s) = &scopes {
        if !s.is_empty() {
            form.push(("scope", s.clone()));
        }
    }

    let token_client = reqwest::Client::new();
    let token_resp = token_client
        .post(&token_url)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("OAuth2 token request failed: {}", e))?;

    if !token_resp.status().is_success() {
        let status = token_resp.status().as_u16();
        let body = token_resp.text().await.unwrap_or_default();
        return Err(format!("OAuth2 token endpoint returned HTTP {}: {}", status, body));
    }

    let body: serde_json::Value = token_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse OAuth2 token response: {}", e))?;

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("OAuth2 token response missing access_token")?;

    Ok(req.bearer_auth(access_token))
}

#[cfg(test)]
#[path = "request_task_tests.rs"]
mod request_task_tests;
