use serde_json::Value;
use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::ai::functions::ToolResult;

pub fn datasync_status(conn: &Connection, args: &Value) -> ToolResult {
    let filter_ds = args["data_source_iri"].as_str();

    if let Some(ds_iri) = filter_ds {
        let raw_iris = crate::owl::find_entities_with_property(conn, "foundation:belongsToDataSource", ds_iri)
            .unwrap_or_default();

        let mut by_status: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for raw_iri in &raw_iris {
            let rdf_type = crate::owl::get_iri_property(conn, raw_iri, "rdf:type")
                .ok().flatten().unwrap_or_default();
            if rdf_type != "foundation:RawDataRecord" {
                continue;
            }
            let status = crate::owl::get_iri_property(conn, raw_iri, "foundation:hasStatus")
                .ok().flatten().unwrap_or_default();
            *by_status.entry(status).or_insert(0) += 1;
        }

        let is_connected = crate::owl::get_literal_property(conn, ds_iri, "foundation:isConnected")
            .ok().flatten().map(|v| v == "true").unwrap_or(false);
        let last_error = crate::owl::get_literal_property(conn, ds_iri, "foundation:lastConnectionError")
            .ok().flatten();
        let status_iri = crate::owl::get_iri_property(conn, ds_iri, "foundation:hasStatus")
            .ok().flatten().unwrap_or_default();

        let by_status_json: serde_json::Map<String, Value> = by_status
            .into_iter()
            .map(|(k, v)| (k, Value::Number(v.into())))
            .collect();

        ToolResult {
            success: true,
            result: Some(serde_json::json!({
                "data_source_iri": ds_iri,
                "status": status_iri,
                "is_connected": is_connected,
                "last_connection_error": last_error,
                "raw_records_by_status": by_status_json,
            })),
            error: None,
            concept: None,
        }
    } else {
        let ds_iris = crate::owl::find_entities_with_property(conn, "rdf:type", "foundation:DataSource")
            .unwrap_or_default();
        let mut summaries = Vec::new();
        for ds_iri in ds_iris {
            let status_iri = crate::owl::get_iri_property(conn, &ds_iri, "foundation:hasStatus")
                .ok().flatten().unwrap_or_default();
            let is_connected = crate::owl::get_literal_property(conn, &ds_iri, "foundation:isConnected")
                .ok().flatten().map(|v| v == "true").unwrap_or(false);
            summaries.push(serde_json::json!({
                "data_source_iri": ds_iri,
                "status": status_iri,
                "is_connected": is_connected,
            }));
        }
        ToolResult {
            success: true,
            result: Some(Value::Array(summaries)),
            error: None,
            concept: None,
        }
    }
}

/// Shared record summary used by both the MCP tool and the Tauri command.
#[derive(serde::Serialize)]
pub struct RawRecordSummary {
    pub iri: String,
    pub external_id: Option<String>,
    pub received_at: Option<String>,
    pub transform_status: String,
    pub retry_count: Option<i64>,
}

/// Shared response shape for list_raw_records, also used by the Tauri command.
pub struct ListRawResult {
    pub records: Vec<RawRecordSummary>,
    pub counts_by_status: std::collections::HashMap<String, i64>,
    pub snapshot_tx: i64,
}

/// Canonical list implementation: filter → sort by received_at desc → limit.
///
/// Applying .take(limit) before the status filter would return an empty page when
/// most records are Transformed and the caller asks for Error — this ordering is
/// the correct semantic.
pub fn list_raw_records(
    conn: &Connection,
    ds_iri: &str,
    status_filter: Option<&str>,
    limit: i64,
) -> Result<ListRawResult, String> {
    let snapshot_tx: i64 = conn
        .query_row("SELECT COALESCE(MAX(tx), 0) FROM triples", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let raw_iris = crate::owl::find_entities_with_property(conn, "foundation:belongsToDataSource", ds_iri)
        .map_err(|e| e.to_string())?;

    let mut counts_by_status: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut filtered: Vec<(String, String, Option<String>, Option<String>, Option<i64>)> = Vec::new();

    for raw_iri in &raw_iris {
        let rdf_type = crate::owl::get_iri_property(conn, raw_iri, "rdf:type")
            .ok().flatten().unwrap_or_default();
        if rdf_type != "foundation:RawDataRecord" {
            continue;
        }

        let status = crate::owl::get_iri_property(conn, raw_iri, "foundation:hasStatus")
            .ok().flatten().unwrap_or_default();

        *counts_by_status.entry(status.clone()).or_insert(0) += 1;

        if let Some(filter) = status_filter {
            if status != filter {
                continue;
            }
        }

        let external_id = crate::owl::get_literal_property(conn, raw_iri, "foundation:externalId")
            .ok().flatten();
        let received_at = crate::owl::get_literal_property(conn, raw_iri, "foundation:receivedAt")
            .ok().flatten();
        let retry_count = crate::owl::get_literal_property(conn, raw_iri, "foundation:retryCount")
            .ok().flatten()
            .and_then(|s| s.parse::<i64>().ok());

        filtered.push((raw_iri.clone(), status, external_id, received_at, retry_count));
    }

    filtered.sort_by(|a, b| b.3.cmp(&a.3));
    filtered.truncate(limit as usize);

    let records = filtered
        .into_iter()
        .map(|(iri, transform_status, external_id, received_at, retry_count)| RawRecordSummary {
            iri,
            external_id,
            received_at,
            transform_status,
            retry_count,
        })
        .collect();

    Ok(ListRawResult { records, counts_by_status, snapshot_tx })
}

pub fn datasync_list_raw(conn: &Connection, args: &Value) -> ToolResult {
    let ds_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult { success: false, result: None, error: Some("data_source_iri is required".to_string()), concept: None },
    };
    let status_filter = args["transform_status"].as_str();
    let limit = args["limit"].as_i64().unwrap_or(50);

    match list_raw_records(conn, ds_iri, status_filter, limit) {
        Ok(result) => {
            let records_json: Vec<Value> = result.records.iter().map(|r| serde_json::json!({
                "iri": r.iri,
                "external_id": r.external_id,
                "received_at": r.received_at,
                "transform_status": r.transform_status,
                "retry_count": r.retry_count,
            })).collect();
            ToolResult {
                success: true,
                result: Some(Value::Array(records_json)),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult { success: false, result: None, error: Some(e), concept: None },
    }
}

/// Shared detail shape used by both the MCP tool and the Tauri command.
#[derive(serde::Serialize)]
pub struct RawRecordDetail {
    pub iri: String,
    pub data_source_iri: Option<String>,
    pub external_id: Option<String>,
    pub transform_status: Option<String>,
    pub received_at: Option<String>,
    pub retry_count: Option<i64>,
    pub raw_source_ref: Option<String>,
    pub raw_payload: Option<String>,
    pub raw_file_path: Option<String>,
    pub transform_error: Option<String>,
}

/// Canonical inspect implementation shared between the MCP tool and the Tauri command.
pub fn inspect_raw_record(conn: &Connection, raw_iri: &str) -> Result<RawRecordDetail, String> {
    let rdf_type = crate::owl::get_iri_property(conn, raw_iri, "rdf:type")
        .ok().flatten();
    if rdf_type.as_deref() != Some("foundation:RawDataRecord") {
        return Err(format!("{} is not a RawDataRecord", raw_iri));
    }

    let transform_status = crate::owl::get_iri_property(conn, raw_iri, "foundation:hasStatus")
        .ok().flatten();
    let external_id = crate::owl::get_literal_property(conn, raw_iri, "foundation:externalId")
        .ok().flatten();
    let raw_payload = crate::owl::get_literal_property(conn, raw_iri, "foundation:rawPayload")
        .ok().flatten();
    let raw_file_iri = crate::owl::get_iri_property(conn, raw_iri, "foundation:rawFile")
        .ok().flatten();
    let transform_error = crate::owl::get_literal_property(conn, raw_iri, "foundation:transformError")
        .ok().flatten();
    let received_at = crate::owl::get_literal_property(conn, raw_iri, "foundation:receivedAt")
        .ok().flatten();
    let retry_count = crate::owl::get_literal_property(conn, raw_iri, "foundation:retryCount")
        .ok().flatten()
        .and_then(|s| s.parse::<i64>().ok());
    let raw_source_ref = crate::owl::get_literal_property(conn, raw_iri, "foundation:rawSourceRef")
        .ok().flatten();
    let data_source_iri = crate::owl::get_iri_property(conn, raw_iri, "foundation:belongsToDataSource")
        .ok().flatten();

    let raw_file_path = raw_file_iri.as_deref().and_then(|f_iri| {
        crate::owl::get_literal_property(conn, f_iri, "foundation:filePath").ok().flatten()
    });

    Ok(RawRecordDetail {
        iri: raw_iri.to_string(),
        data_source_iri,
        external_id,
        transform_status,
        received_at,
        retry_count,
        raw_source_ref,
        raw_payload,
        raw_file_path,
        transform_error,
    })
}

pub fn datasync_inspect_raw(conn: &Connection, args: &Value) -> ToolResult {
    let raw_iri = match args["raw_record_iri"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult { success: false, result: None, error: Some("raw_record_iri is required".to_string()), concept: None },
    };

    match inspect_raw_record(conn, raw_iri) {
        Ok(detail) => ToolResult {
            success: true,
            result: Some(serde_json::json!({
                "iri": detail.iri,
                "data_source_iri": detail.data_source_iri,
                "external_id": detail.external_id,
                "transform_status": detail.transform_status,
                "received_at": detail.received_at,
                "retry_count": detail.retry_count,
                "raw_source_ref": detail.raw_source_ref,
                "raw_payload": detail.raw_payload,
                "raw_file_path": detail.raw_file_path,
                "transform_error": detail.transform_error,
            })),
            error: None,
            concept: None,
        },
        Err(e) => ToolResult { success: false, result: None, error: Some(e), concept: None },
    }
}

pub async fn datasync_create_source_tool(
    args: &Value,
    app: Option<&AppHandle>,
) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => return ToolResult { success: false, result: None, error: Some("AppHandle required for datasync_create_source".to_string()), concept: None },
    };

    let connector_iri = match args["connector_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("connector_iri is required".to_string()), concept: None },
    };
    let sync_namespace = match args["sync_namespace"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("sync_namespace is required".to_string()), concept: None },
    };
    let sync_schedule = match args["sync_schedule"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("sync_schedule is required".to_string()), concept: None },
    };
    let label = match args["label"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("label is required".to_string()), concept: None },
    };
    let target_endpoint = args["target_endpoint"].as_str().unwrap_or("").to_string();
    let item_path = args["item_path"].as_str().unwrap_or("").to_string();
    let transform_script = args["transform_script"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());

    let executor = app.state::<crate::owl::DbExecutor>();

    match crate::data_sync::source::create_data_source(
        &executor,
        crate::data_sync::source::CreateDataSourceParams {
            connector_iri,
            sync_namespace,
            sync_schedule,
            target_endpoint,
            item_path,
            label,
            transform_script,
        },
    ).await {
        Ok(ds_iri) => {
            let status = executor
                .read({
                    let ds_iri_clone = ds_iri.clone();
                    move |conn| {
                        crate::owl::get_iri_property(conn, &ds_iri_clone, "foundation:hasStatus")
                            .map_err(|e| e.to_string())
                    }
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();

            ToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "data_source_iri": ds_iri,
                    "status": status,
                })),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult { success: false, result: None, error: Some(e), concept: None },
    }
}

pub async fn datasync_run_tool(
    args: &Value,
    app: Option<&AppHandle>,
) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => return ToolResult { success: false, result: None, error: Some("AppHandle required".to_string()), concept: None },
    };

    let ds_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("data_source_iri is required".to_string()), concept: None },
    };
    let run_transform = args["run_transform"].as_bool().unwrap_or(false);

    match crate::data_sync::request_worker::run_extract_cycle(app, &ds_iri).await {
        Ok(staged) => {
            let transform_result = if run_transform {
                trigger_transform_automation(app, &ds_iri).await
            } else {
                None
            };

            ToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "staged_count": staged.len(),
                    "staged_iris": staged,
                    "transform_triggered": transform_result.is_some(),
                    "transform_result": transform_result,
                })),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult { success: false, result: None, error: Some(e), concept: None },
    }
}

pub async fn datasync_retry_transform_tool(
    args: &Value,
    app: Option<&AppHandle>,
) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => return ToolResult { success: false, result: None, error: Some("AppHandle required".to_string()), concept: None },
    };

    let ds_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("data_source_iri is required".to_string()), concept: None },
    };

    let executor = app.state::<crate::owl::DbExecutor>();
    let ds_iri_clone = ds_iri.clone();

    let error_iris = executor
        .read(move |conn| {
            let all = crate::owl::find_entities_with_property(
                conn, "foundation:belongsToDataSource", &ds_iri_clone,
            ).map_err(|e| e.to_string())?;
            let mut errors = Vec::new();
            for iri in all {
                let status = crate::owl::get_iri_property(conn, &iri, "foundation:hasStatus")
                    .ok().flatten().unwrap_or_default();
                if status == "foundation:Status_1781300928559" {
                    errors.push(iri);
                }
            }
            Ok::<Vec<String>, String>(errors)
        })
        .await
        .unwrap_or_default();

    let count = error_iris.len();
    for raw_iri in error_iris {
        let raw_iri_clone = raw_iri.clone();
        let _ = executor.write(move |conn| {
            crate::core_ontology::data_sync::increment_retry_count(conn, &raw_iri_clone)?;
            Individual::new(&raw_iri_clone)
                .add_property(conn, "foundation:hasStatus",
                    vec![crate::owl::Object::Iri("foundation:Pending".to_string())], "data_sync")
                .map_err(|e| e.to_string())?;
            Ok(String::new())
        }).await;
    }

    let transform_result = trigger_transform_automation(app, &ds_iri).await;

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "reset_count": count,
            "transform_triggered": transform_result.is_some(),
        })),
        error: None,
        concept: None,
    }
}

pub async fn trigger_transform(app: &AppHandle, ds_iri: &str) -> bool {
    trigger_transform_automation(app, ds_iri).await.is_some()
}

async fn trigger_transform_automation(app: &AppHandle, ds_iri: &str) -> Option<String> {
    let executor = app.state::<crate::owl::DbExecutor>();
    let ds_iri_clone = ds_iri.to_string();
    let auto_iri = executor
        .read(move |conn| {
            crate::owl::get_iri_property(conn, &ds_iri_clone, "foundation:transformAutomation")
                .map_err(|e| e.to_string())
        })
        .await
        .ok()
        .flatten()?;

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let call = crate::ai::functions::ToolCall {
            name: "run_automation".to_string(),
            arguments: serde_json::json!({ "automation_iri": auto_iri, "dry_run": false }),
        };
        let executor_inner = app_clone.state::<crate::owl::DbExecutor>();
        let app_for_closure = app_clone.clone();
        let _ = executor_inner
            .write(move |conn| {
                let r = crate::ai::functions::execute_tool(conn, &call, Some(&app_for_closure), None);
                Ok(r.result.map(|v| v.to_string()).unwrap_or_default())
            })
            .await;
    });

    Some("triggered".to_string())
}

use crate::owl::{Individual, Object};

/// Upsert a domain entity from a RawDataRecord.
///
/// Without `target_iri`, builds a deterministic IRI via `build_deterministic_iri`;
/// with `target_iri`, adopts the existing individual in-place (preserves relationships
/// like `usesModel`). Runs `upsert_sync_record` and `mark_raw_transformed` in the same
/// write — all inside DbExecutor::write so the notify path fires normally.
pub async fn datasync_upsert_item_tool(
    args: &Value,
    app: Option<&AppHandle>,
) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => return ToolResult { success: false, result: None, error: Some("AppHandle required for datasync_upsert_item".to_string()), concept: None },
    };

    let data_source_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("data_source_iri is required".to_string()), concept: None },
    };
    let raw_record_iri = match args["raw_record_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("raw_record_iri is required".to_string()), concept: None },
    };
    let class_iri = match args["class_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("class_iri is required".to_string()), concept: None },
    };
    let external_id = match args["external_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult { success: false, result: None, error: Some("external_id is required".to_string()), concept: None },
    };
    let properties = args["properties"].clone();
    let target_iri_opt = args["target_iri"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());

    let executor = app.state::<crate::owl::DbExecutor>();

    let result = executor.write(move |conn| {
        // Resolve the class local name for IRI construction.
        let class_local = class_iri
            .rsplit(':')
            .next()
            .unwrap_or(&class_iri)
            .to_string();

        // Load data source namespace.
        let sync_namespace = crate::owl::get_literal_property(conn, &data_source_iri, "foundation:syncNamespace")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "foundation".to_string());

        // Determine entity IRI: adopt existing or build deterministic.
        let entity_iri = match target_iri_opt {
            Some(ref t) => t.clone(),
            None => crate::core_ontology::data_sync::build_deterministic_iri(
                &sync_namespace, &class_local, &external_id,
            ),
        };

        // Build label from external_id if not in properties.
        let label = properties.get("label")
            .and_then(|v| v.as_str())
            .unwrap_or(&external_id)
            .to_string();

        // Check if entity exists already.
        let already_exists = crate::owl::get_iri_property(conn, &entity_iri, "rdf:type")
            .map_err(|e| e.to_string())?
            .is_some();

        if !already_exists {
            Individual::new(&entity_iri)
                .assert(conn, &class_iri, &label, "sync_alt", "data_sync")
                .map_err(|e| format!("assert entity: {}", e))?;

            Individual::new(&entity_iri)
                .add_property(conn, "foundation:hasStatus",
                    vec![crate::owl::Object::Iri("foundation:Pending".to_string())], "data_sync")
                .map_err(|e| format!("entity hasStatus: {}", e))?;
        } else {
            // Update label on existing entity so it reflects current data.
            Individual::new(&entity_iri)
                .add_property(conn, "rdfs:label",
                    vec![crate::owl::Object::Literal {
                        value: label.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }], "data_sync")
                .map_err(|e| format!("update label: {}", e))?;
        }

        // Apply properties from the JSON object.
        if let Some(obj) = properties.as_object() {
            for (prop_iri, val) in obj {
                if prop_iri == "label" {
                    continue;
                }
                const IRI_PREFIXES: &[&str] = &[
                    "foundation:", "anthropic:", "owl:", "rdf:", "rdfs:", "xsd:", "qudt:",
                ];
                let object = if let Some(s) = val.as_str() {
                    if !s.contains(' ') && IRI_PREFIXES.iter().any(|p| s.starts_with(p)) {
                        crate::owl::Object::Iri(s.to_string())
                    } else {
                        crate::owl::Object::Literal {
                            value: s.to_string(),
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        }
                    }
                } else if let Some(b) = val.as_bool() {
                    crate::owl::Object::Boolean(b)
                } else if let Some(n) = val.as_i64() {
                    crate::owl::Object::Literal {
                        value: n.to_string(),
                        datatype: Some("xsd:integer".to_string()),
                        language: None,
                    }
                } else if let Some(f) = val.as_f64() {
                    crate::owl::Object::Literal {
                        value: format!("{:.10}", f).trim_end_matches('0').trim_end_matches('.').to_string(),
                        datatype: Some("xsd:decimal".to_string()),
                        language: None,
                    }
                } else {
                    continue;
                };

                Individual::new(&entity_iri)
                    .add_property(conn, prop_iri, vec![object], "data_sync")
                    .map_err(|e| format!("set property {}: {}", prop_iri, e))?;
            }
        }

        // Upsert the SyncRecord.
        crate::core_ontology::data_sync::upsert_sync_record(
            conn, &data_source_iri, &sync_namespace, &external_id, &entity_iri,
        ).map_err(|e| format!("upsert_sync_record: {}", e))?;

        // Mark raw as Transformed.
        crate::core_ontology::data_sync::mark_raw_transformed(
            conn, &raw_record_iri, &[entity_iri.as_str()],
        ).map_err(|e| format!("mark_raw_transformed: {}", e))?;

        Ok(entity_iri)
    }).await;

    match result {
        Ok(entity_iri) => ToolResult {
            success: true,
            result: Some(serde_json::json!({ "entity_iri": entity_iri })),
            error: None,
            concept: None,
        },
        Err(e) => ToolResult { success: false, result: None, error: Some(e), concept: None },
    }
}

/// Set (and optionally dry-run) the transform script for a DataSource's CodeTask.
///
/// Flow:
///   1. Resolve DataSource → `foundation:transformAutomation` → CodeTask node.
///   2. Compile the candidate script with the same stubs as `script_validator`.
///      Invalid → return compile error, prior script is untouched.
///   3. `dry_run=true` → execute against real data with writes mocked; return
///      script output + intercepted write calls.  Nothing is persisted.
///   4. No `dry_run` → persist the new script via `DbExecutor::write` so the
///      notify path fires `foundation:script` and `script_validator` revalidates.
pub async fn datasync_set_transform_tool(
    args: &Value,
    app: Option<&AppHandle>,
) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("AppHandle required for datasync_set_transform".to_string()),
            concept: None,
        },
    };

    let ds_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("data_source_iri is required".to_string()),
            concept: None,
        },
    };

    let script = match args["script"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("script is required".to_string()),
            concept: None,
        },
    };

    let dry_run = args["dry_run"].as_bool().unwrap_or(false);

    // Resolve DataSource → transformAutomation → CodeTask
    let executor = app.state::<crate::owl::DbExecutor>();
    let ds_iri_clone = ds_iri.clone();

    let resolve_result = executor
        .read(move |conn| {
            let auto_iri = crate::owl::get_iri_property(conn, &ds_iri_clone, "foundation:transformAutomation")
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("DataSource {} has no transformAutomation", ds_iri_clone))?;

            let node_iris = crate::owl::find_entities_with_property(
                conn, "foundation:partOfProcess", &auto_iri,
            ).map_err(|e| e.to_string())?;

            let code_task_iri = node_iris.into_iter().find(|node_iri| {
                crate::owl::get_iri_property(conn, node_iri, "rdf:type")
                    .ok()
                    .flatten()
                    .as_deref() == Some("foundation:automation_CodeTask")
            });

            code_task_iri.ok_or_else(|| format!(
                "transformAutomation {} has no CodeTask node", auto_iri
            ))
        })
        .await;

    let code_task_iri = match resolve_result {
        Ok(iri) => iri,
        Err(e) => return ToolResult { success: false, result: None, error: Some(e), concept: None },
    };

    // Validate the candidate script with the same stubs as script_validator — guard is synchronous.
    if let Err(compile_err) = crate::process_automation::script_validator::compile_script(&script) {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Script compilation error: {}", compile_err)),
            concept: None,
        };
    }

    if dry_run {
        // Execute the candidate script against real data with writes mocked.
        // We do NOT use run_automation (which persists control instances).
        let script_clone = script.clone();
        let app_clone = app.clone();
        let handle = tokio::runtime::Handle::current();

        let exec_result = tokio::task::spawn_blocking(move || {
            let mut ctx = crate::process_automation::executor::ExecutionContext::new();
            ctx.insert("dryRun".to_string(), "true".to_string());
            crate::process_automation::code_task::run_script_dry_run_collect(
                &script_clone, &ctx, &app_clone, &handle,
            )
        })
        .await
        .map_err(|e| e.to_string());

        return match exec_result {
            Ok(Ok((output, intercepted))) => {
                if intercepted.is_empty() && output.is_empty() {
                    ToolResult {
                        success: true,
                        result: Some(serde_json::json!({
                            "dry_run": true,
                            "script_output": null,
                            "intercepted_writes": [],
                            "note": "Script is valid but no pending records found to exercise",
                        })),
                        error: None,
                        concept: None,
                    }
                } else {
                    ToolResult {
                        success: true,
                        result: Some(serde_json::json!({
                            "dry_run": true,
                            "script_output": output,
                            "intercepted_writes": intercepted,
                        })),
                        error: None,
                        concept: None,
                    }
                }
            }
            Ok(Err(e)) => ToolResult {
                success: false,
                result: None,
                error: Some(format!("dry_run execution error: {}", e)),
                concept: None,
            },
            Err(e) => ToolResult {
                success: false,
                result: None,
                error: Some(format!("dry_run task error: {}", e)),
                concept: None,
            },
        };
    }

    // Persist the validated script on the CodeTask via DbExecutor::write so the
    // notify path fires foundation:script and script_validator revalidates.
    let code_task_iri_clone = code_task_iri.clone();
    let script_clone = script.clone();
    let write_result = executor
        .write(move |conn| {
            Individual::new(&code_task_iri_clone)
                .add_property(
                    conn,
                    "foundation:script",
                    vec![Object::Literal {
                        value: script_clone,
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "datasync_set_transform",
                )
                .map_err(|e| e.to_string())?;
            Ok(code_task_iri_clone)
        })
        .await;

    match write_result {
        Ok(iri) => {
            crate::commands::log_backend(
                "info",
                &format!("[datasync] set_transform: persisted new script on {}", iri),
            );
            ToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "code_task_iri": iri,
                    "persisted": true,
                })),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult { success: false, result: None, error: Some(e), concept: None },
    }
}
