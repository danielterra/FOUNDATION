use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::owl::{DbExecutor, Individual, Object};
use crate::eavto::query;

/// Portable representation of a connector and its capabilities, safe to serialize to JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectorPackageJson {
    pub schema_version: String,
    pub connector_iri: String,
    pub label: String,
    pub comment: Option<String>,
    pub base_url: Option<String>,
    pub auth_type: Option<String>,
    pub api_spec_url: Option<String>,
    pub connector_version: Option<String>,
    pub author: Option<String>,
    pub capabilities: Vec<CapabilityJson>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CapabilityJson {
    pub iri: String,
    pub label: String,
    pub comment: Option<String>,
    pub http_method: Option<String>,
    pub endpoint_path: Option<String>,
}

async fn read_literal(conn: &turso::Connection, iri: &str, predicate: &str) -> Option<String> {
    crate::owl::get_literal_property(conn, iri, predicate).await.ok().flatten()
}

/// Export a connector and its capabilities as a portable JSON package.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn connector__export_package(
    connector_iri: String,
    author: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<ConnectorPackageJson, String> {
    executor.read(move |conn| async move {
        let label = read_literal(&conn, &connector_iri, "rdfs:label").await
            .unwrap_or_else(|| connector_iri.clone());
        let comment = read_literal(&conn, &connector_iri, "rdfs:comment").await;
        let base_url = read_literal(&conn, &connector_iri, "foundation:baseUrl").await;
        let auth_type = read_literal(&conn, &connector_iri, "foundation:authType").await;
        let api_spec_url = read_literal(&conn, &connector_iri, "foundation:apiSpecUrl").await;
        let connector_version = read_literal(&conn, &connector_iri, "foundation:connectorVersion").await;

        // Load all ConnectorCapability instances linked to this connector
        let cap_triples = query::get_by_predicate_object(&conn, "foundation:partOfConnector", &connector_iri).await
            .map_err(|e| e.to_string())?;

        let mut capabilities = Vec::new();
        for triple in &cap_triples.triples {
            let cap_iri = &triple.subject;
            let cap_label = read_literal(&conn, cap_iri, "rdfs:label").await
                .unwrap_or_else(|| cap_iri.clone());
            let cap_comment = read_literal(&conn, cap_iri, "rdfs:comment").await;
            let http_method = read_literal(&conn, cap_iri, "foundation:httpMethod").await;
            let endpoint_path = read_literal(&conn, cap_iri, "foundation:endpointPath").await;

            capabilities.push(CapabilityJson {
                iri: cap_iri.clone(),
                label: cap_label,
                comment: cap_comment,
                http_method,
                endpoint_path,
            });
        }

        Ok(ConnectorPackageJson {
            schema_version: "1.0".to_string(),
            connector_iri: connector_iri.clone(),
            label,
            comment,
            base_url,
            auth_type,
            api_spec_url,
            connector_version,
            author,
            capabilities,
        })
    }).await
}

/// Import a connector package JSON, creating or updating the connector and its capabilities.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn connector__import_package(
    _app: AppHandle,
    package: ConnectorPackageJson,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let timestamp = chrono::Utc::now().timestamp_millis();

    executor.write(move |conn| async move {
        let connector_iri = if package.connector_iri.starts_with("foundation:") {
            package.connector_iri.clone()
        } else {
            format!("foundation:ExternalServiceConnector_{}", timestamp)
        };

        let ind = Individual::new(&connector_iri);
        ind.assert(&conn, "foundation:ExternalServiceConnector", &package.label, "api", "connector").await
            .map_err(|e| format!("Failed to create connector: {}", e))?;

        let str_lit = |v: String| Object::Literal {
            value: v,
            datatype: Some("xsd:string".to_string()),
            language: None,
        };

        if let Some(comment) = package.comment {
            ind.add_property(&conn, "rdfs:comment", vec![str_lit(comment)], "connector").await
                .map_err(|e| format!("Failed to set comment: {}", e))?;
        }
        if let Some(url) = package.base_url {
            ind.add_property(&conn, "foundation:baseUrl", vec![str_lit(url)], "connector").await
                .map_err(|e| format!("Failed to set baseUrl: {}", e))?;
        }
        if let Some(auth) = package.auth_type {
            ind.add_property(&conn, "foundation:authType", vec![str_lit(auth)], "connector").await
                .map_err(|e| format!("Failed to set authType: {}", e))?;
        }
        if let Some(spec) = package.api_spec_url {
            ind.add_property(&conn, "foundation:apiSpecUrl", vec![str_lit(spec)], "connector").await
                .map_err(|e| format!("Failed to set apiSpecUrl: {}", e))?;
        }
        if let Some(ver) = package.connector_version {
            ind.add_property(&conn, "foundation:connectorVersion", vec![str_lit(ver)], "connector").await
                .map_err(|e| format!("Failed to set connectorVersion: {}", e))?;
        }

        for cap in package.capabilities {
            let cap_iri = format!("foundation:ConnectorCapability_{}_{}", timestamp, cap.iri.replace([':', '/'], "_"));
            let cap_ind = Individual::new(&cap_iri);

            cap_ind.assert(&conn, "foundation:ConnectorCapability", &cap.label, "bolt", "connector").await
                .map_err(|e| format!("Failed to create capability: {}", e))?;

            if let Some(comment) = cap.comment {
                cap_ind.add_property(&conn, "rdfs:comment", vec![str_lit(comment)], "connector").await
                    .map_err(|e| format!("Failed to set capability comment: {}", e))?;
            }
            if let Some(method) = cap.http_method {
                cap_ind.add_property(&conn, "foundation:httpMethod", vec![str_lit(method)], "connector").await
                    .map_err(|e| format!("Failed to set httpMethod: {}", e))?;
            }
            if let Some(path) = cap.endpoint_path {
                cap_ind.add_property(&conn, "foundation:endpointPath", vec![str_lit(path)], "connector").await
                    .map_err(|e| format!("Failed to set endpointPath: {}", e))?;
            }
            cap_ind.add_property(&conn, "foundation:partOfConnector",
                vec![Object::Iri(connector_iri.clone())], "connector").await
                .map_err(|e| format!("Failed to link capability: {}", e))?;
        }

        Ok(connector_iri)
    }).await
}
