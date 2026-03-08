use chrono::Utc;
use tauri::{AppHandle, Manager};

use crate::eavto::query;
use crate::owl::{get_iri_property, get_literal_property, DbExecutor, Individual, Object};

use super::executor::ExecutionContext;

type Result<T> = std::result::Result<T, String>;

fn str_lit(v: impl Into<String>) -> Object {
    Object::Literal {
        value: v.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

fn interpolate(template: &str, ctx: &ExecutionContext) -> String {
    let mut result = template.to_string();
    for (key, value) in ctx {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

/// Executes a bpmn_RequestTask node: reads its linked HTTPRequest, performs the HTTP call,
/// persists an HTTPResponse, and returns the response body.
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

    let resolved_url = interpolate(&url, ctx);
    let resolved_body = body.map(|b| interpolate(&b, ctx));

    let client = reqwest::Client::new();
    let mut req = match method.to_uppercase().as_str() {
        "POST" => client.post(&resolved_url),
        "PUT" => client.put(&resolved_url),
        "PATCH" => client.patch(&resolved_url),
        "DELETE" => client.delete(&resolved_url),
        _ => client.get(&resolved_url),
    };

    if let Some(ref headers_str) = headers_json {
        if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(headers_str) {
            for (k, v) in map {
                if let Some(v_str) = v.as_str() {
                    if let (Ok(name), Ok(value)) = (
                        reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                        reqwest::header::HeaderValue::from_str(v_str),
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

    let response = req
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status_code = response.status().as_u16() as i64;
    let resp_body = response.text().await.unwrap_or_default();

    let timestamp = Utc::now().timestamp_millis();
    let response_iri = format!("foundation:HTTPResponse_{}", timestamp);

    executor
        .write({
            let request_iri = request_iri.clone();
            let response_iri = response_iri.clone();
            let resp_body_clone = resp_body.clone();
            move |conn| {
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
        _ => {}
    }

    Ok(req)
}

#[cfg(test)]
#[path = "request_task_tests.rs"]
mod request_task_tests;
