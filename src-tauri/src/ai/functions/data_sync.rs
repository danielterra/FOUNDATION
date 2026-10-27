use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::ai::functions::ToolResult;
use crate::owl::Connection;

fn tool_result(result: Result<Value, String>) -> ToolResult {
    match result {
        Ok(value) => ToolResult {
            success: true,
            result: Some(value),
            error: None,
            concept: None,
        },
        Err(error) => ToolResult {
            success: false,
            result: None,
            error: Some(error),
            concept: None,
        },
    }
}

pub fn datasync_status(conn: &Connection, args: &Value) -> ToolResult {
    let result = if let Some(data_source_iri) = args["data_source_iri"].as_str() {
        crate::core_ontology::data_sync::get_data_source_status(conn, data_source_iri)
            .and_then(|summary| serde_json::to_value(summary).map_err(|e| e.to_string()))
    } else {
        let limit = args["limit"].as_i64().unwrap_or(50).max(1);
        let after_tx = args["after_tx"].as_i64();
        crate::core_ontology::data_sync::list_data_source_statuses(conn, limit, after_tx).map(
            |page| {
                serde_json::json!({
                    "items": page.items,
                    "next_cursor": page.next_cursor,
                    "has_more": page.has_more,
                })
            },
        )
    };

    tool_result(result)
}

pub fn datasync_list_raw(conn: &Connection, args: &Value) -> ToolResult {
    let ds_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("data_source_iri is required".to_string()),
                concept: None,
            }
        }
    };
    let status_filter = args["transform_status"].as_str();
    let limit = args["limit"].as_i64().unwrap_or(50);
    let snapshot_tx = args["snapshot_tx"].as_i64();
    let after_tx = args["after_tx"].as_i64();

    match crate::core_ontology::data_sync::list_raw_records(
        conn,
        ds_iri,
        status_filter,
        limit,
        snapshot_tx,
        after_tx,
    ) {
        Ok(result) => {
            let records_json: Vec<Value> = result
                .records
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "iri": r.iri,
                        "external_id": r.external_id,
                        "received_at": r.received_at,
                        "transform_status": r.transform_status,
                        "retry_count": r.retry_count,
                    })
                })
                .collect();
            ToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "items": records_json,
                    "next_cursor": result.next_cursor,
                    "has_more": result.has_more,
                    "counts_by_status": result.counts_by_status,
                    "snapshot_tx": result.snapshot_tx,
                })),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
    }
}

pub fn datasync_inspect_raw(conn: &Connection, args: &Value) -> ToolResult {
    let raw_iri = match args["raw_record_iri"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("raw_record_iri is required".to_string()),
                concept: None,
            }
        }
    };

    match crate::core_ontology::data_sync::inspect_raw_record(conn, raw_iri) {
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
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
    }
}

pub async fn datasync_create_source_tool(args: &Value, app: Option<&AppHandle>) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("AppHandle required for datasync_create_source".to_string()),
                concept: None,
            }
        }
    };

    let connector_iri = match args["connector_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("connector_iri is required".to_string()),
                concept: None,
            }
        }
    };
    let sync_namespace = match args["sync_namespace"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("sync_namespace is required".to_string()),
                concept: None,
            }
        }
    };
    let sync_schedule = match args["sync_schedule"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("sync_schedule is required".to_string()),
                concept: None,
            }
        }
    };
    let label = match args["label"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("label is required".to_string()),
                concept: None,
            }
        }
    };
    let target_endpoint = args["target_endpoint"].as_str().unwrap_or("").to_string();
    let item_path = args["item_path"].as_str().unwrap_or("").to_string();
    let transform_script = args["transform_script"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

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
    )
    .await
    {
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
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
    }
}

pub async fn datasync_run_tool(args: &Value, app: Option<&AppHandle>) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("AppHandle required".to_string()),
                concept: None,
            }
        }
    };

    let ds_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("data_source_iri is required".to_string()),
                concept: None,
            }
        }
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
                    "staged_iris_sample": staged.iter().take(50).collect::<Vec<_>>(),
                    "transform_triggered": transform_result.is_some(),
                    "transform_result": transform_result,
                })),
                error: None,
                concept: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
    }
}

/// Lists SyncRuns, newest first, optionally narrowed to one source and one outcome.
///
/// This is the "did yesterday's sync work?" question. Answering it from the raw
/// records alone is impossible: they carry the state of each item, never the shape
/// of the pass that produced them, so a cycle that fetched nothing and a cycle that
/// never ran look identical.
pub fn datasync_list_runs(conn: &Connection, args: &Value) -> ToolResult {
    let ds_filter = args["data_source_iri"].as_str().filter(|s| !s.is_empty());
    let status_filter = args["status"].as_str().filter(|s| !s.is_empty());
    let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 200) as usize;
    tool_result(
        crate::core_ontology::data_sync::list_sync_runs(conn, ds_filter, status_filter, limit)
            .map(|runs| serde_json::json!({ "count": runs.len(), "runs": runs })),
    )
}

/// Decides whether a single RawDataRecord may be retried, returning the
/// DataSource it belongs to. Shared by the MCP tool and the Tauri command so both
/// enforce the same two guards: it must be a RawDataRecord, and it must be in Error.
pub async fn datasync_retry_transform_tool(args: &Value, app: Option<&AppHandle>) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("AppHandle required".to_string()),
                concept: None,
            }
        }
    };

    let ds_arg = args["data_source_iri"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let explicit: Vec<String> = args["raw_record_iris"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let executor = app.state::<crate::owl::DbExecutor>();

    // Two modes: named records (validated one by one, so a bad IRI is reported
    // instead of silently doing nothing) or every Error record of one source.
    let resolved = if !explicit.is_empty() {
        executor
            .read(move |conn| {
                let mut retryable = Vec::new();
                let mut skipped = Vec::new();
                let mut sources: Vec<String> = Vec::new();
                for iri in explicit {
                    match crate::core_ontology::data_sync::validate_retryable(conn, &iri) {
                        Ok(ds) => {
                            if !sources.contains(&ds) {
                                sources.push(ds);
                            }
                            retryable.push(iri);
                        }
                        Err(reason) => {
                            skipped.push(serde_json::json!({ "iri": iri, "reason": reason }))
                        }
                    }
                }
                Ok::<_, String>((retryable, skipped, sources))
            })
            .await
    } else {
        let Some(ds_iri) = ds_arg.clone() else {
            return ToolResult {
                success: false,
                result: None,
                error: Some("pass raw_record_iris to retry specific records, or data_source_iri to retry every Error record of a source".to_string()),
                concept: None,
            };
        };
        executor
            .read(move |conn| {
                let retryable =
                    crate::core_ontology::data_sync::error_records_of_source(conn, &ds_iri)?;
                Ok::<_, String>((retryable, Vec::new(), vec![ds_iri]))
            })
            .await
    };

    let (retryable, skipped, sources) = match resolved {
        Ok(v) => v,
        Err(e) => {
            return ToolResult {
                success: false,
                result: None,
                error: Some(e),
                concept: None,
            }
        }
    };

    let count = retryable.len();
    for raw_iri in retryable {
        let raw_iri_clone = raw_iri.clone();
        let _ = executor
            .write(move |conn| {
                crate::core_ontology::data_sync::increment_retry_count(conn, &raw_iri_clone)?;
                // A record heading back to Pending carries no verdict yet — leaving the
                // old error behind makes a queued record read as still-failed.
                Individual::clear_property(
                    conn,
                    &raw_iri_clone,
                    "foundation:transformError",
                    "data_sync",
                )
                .map_err(|e| e.to_string())?;
                Individual::new(&raw_iri_clone)
                    .add_property(
                        conn,
                        "foundation:hasStatus",
                        vec![crate::owl::Object::Iri("foundation:Pending".to_string())],
                        "data_sync",
                    )
                    .map_err(|e| e.to_string())?;
                Ok(String::new())
            })
            .await;
    }

    let mut triggered = 0usize;
    for ds_iri in &sources {
        if trigger_transform_automation(app, ds_iri).await.is_some() {
            triggered += 1;
        }
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "reset_count": count,
            "skipped": skipped,
            "data_sources": sources,
            "transform_triggered": triggered > 0,
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
                let r =
                    crate::ai::functions::execute_tool(conn, &call, Some(&app_for_closure), None);
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
pub async fn datasync_upsert_item_tool(args: &Value, app: Option<&AppHandle>) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("AppHandle required for datasync_upsert_item".to_string()),
                concept: None,
            }
        }
    };

    let data_source_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("data_source_iri is required".to_string()),
                concept: None,
            }
        }
    };
    let raw_record_iri = match args["raw_record_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("raw_record_iri is required".to_string()),
                concept: None,
            }
        }
    };
    let class_iri = match args["class_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("class_iri is required".to_string()),
                concept: None,
            }
        }
    };
    let external_id = match args["external_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("external_id is required".to_string()),
                concept: None,
            }
        }
    };
    let properties = args["properties"].clone();
    let target_iri_opt = args["target_iri"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let executor = app.state::<crate::owl::DbExecutor>();

    let result = executor
        .write(move |conn| {
            // Resolve the class local name for IRI construction.
            let class_local = class_iri
                .rsplit(':')
                .next()
                .unwrap_or(&class_iri)
                .to_string();

            // Load data source namespace.
            let sync_namespace = crate::owl::get_literal_property(
                conn,
                &data_source_iri,
                "foundation:syncNamespace",
            )
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "foundation".to_string());

            // Determine entity IRI: adopt existing or build deterministic.
            let entity_iri = match target_iri_opt {
                Some(ref t) => t.clone(),
                None => crate::core_ontology::data_sync::build_deterministic_iri(
                    &sync_namespace,
                    &class_local,
                    &external_id,
                ),
            };

            // Build label from external_id if not in properties.
            let label = properties
                .get("label")
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
                    .add_property(
                        conn,
                        "foundation:hasStatus",
                        vec![crate::owl::Object::Iri("foundation:Pending".to_string())],
                        "data_sync",
                    )
                    .map_err(|e| format!("entity hasStatus: {}", e))?;
            } else {
                // Update label on existing entity so it reflects current data.
                Individual::new(&entity_iri)
                    .add_property(
                        conn,
                        "rdfs:label",
                        vec![crate::owl::Object::Literal {
                            value: label.clone(),
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        }],
                        "data_sync",
                    )
                    .map_err(|e| format!("update label: {}", e))?;
            }

            // Apply properties from the JSON object.
            if let Some(obj) = properties.as_object() {
                for (prop_iri, val) in obj {
                    if prop_iri == "label" {
                        continue;
                    }
                    const IRI_PREFIXES: &[&str] = &[
                        "foundation:",
                        "anthropic:",
                        "owl:",
                        "rdf:",
                        "rdfs:",
                        "xsd:",
                        "qudt:",
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
                            value: format!("{:.10}", f)
                                .trim_end_matches('0')
                                .trim_end_matches('.')
                                .to_string(),
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
                conn,
                &data_source_iri,
                &sync_namespace,
                &external_id,
                &entity_iri,
            )
            .map_err(|e| format!("upsert_sync_record: {}", e))?;

            // Mark raw as Transformed.
            crate::core_ontology::data_sync::mark_raw_transformed(
                conn,
                &raw_record_iri,
                &[entity_iri.as_str()],
            )
            .map_err(|e| format!("mark_raw_transformed: {}", e))?;

            Ok(entity_iri)
        })
        .await;

    match result {
        Ok(entity_iri) => ToolResult {
            success: true,
            result: Some(serde_json::json!({ "entity_iri": entity_iri })),
            error: None,
            concept: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
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
pub async fn datasync_set_transform_tool(args: &Value, app: Option<&AppHandle>) -> ToolResult {
    let app = match app {
        Some(a) => a,
        None => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("AppHandle required for datasync_set_transform".to_string()),
                concept: None,
            }
        }
    };

    let ds_iri = match args["data_source_iri"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("data_source_iri is required".to_string()),
                concept: None,
            }
        }
    };

    let script = match args["script"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return ToolResult {
                success: false,
                result: None,
                error: Some("script is required".to_string()),
                concept: None,
            }
        }
    };

    let dry_run = args["dry_run"].as_bool().unwrap_or(false);

    // Resolve DataSource → transformAutomation → CodeTask
    let executor = app.state::<crate::owl::DbExecutor>();
    let ds_iri_clone = ds_iri.clone();

    let resolve_result = executor
        .read(move |conn| {
            let auto_iri =
                crate::owl::get_iri_property(conn, &ds_iri_clone, "foundation:transformAutomation")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| {
                        format!("DataSource {} has no transformAutomation", ds_iri_clone)
                    })?;

            let node_iris = crate::owl::find_entities_with_property(
                conn,
                "foundation:partOfProcess",
                &auto_iri,
            )
            .map_err(|e| e.to_string())?;

            let code_task_iri = node_iris.into_iter().find(|node_iri| {
                crate::owl::get_iri_property(conn, node_iri, "rdf:type")
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some("foundation:automation_CodeTask")
            });

            code_task_iri
                .ok_or_else(|| format!("transformAutomation {} has no CodeTask node", auto_iri))
        })
        .await;

    let code_task_iri = match resolve_result {
        Ok(iri) => iri,
        Err(e) => {
            return ToolResult {
                success: false,
                result: None,
                error: Some(e),
                concept: None,
            }
        }
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
                &script_clone,
                &ctx,
                &app_clone,
                &handle,
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
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::{Individual, Object};

    const STATUS_ERROR: &str = "foundation:Status_1781300928559";
    const STATUS_TRANSFORMED: &str = "foundation:Status_1781300928585";

    /// Object properties refuse to point at an IRI that isn't in the graph, so the
    /// DataSource and the status individuals have to exist before a record links them.
    fn ensure(conn: &mut Connection, iri: &str, class: &str) {
        if crate::owl::get_iri_property(conn, iri, "rdf:type")
            .ok()
            .flatten()
            .is_none()
        {
            Individual::new(iri)
                .assert(conn, class, iri, "https://example.com/icon.png", "test")
                .expect("ensure");
        }
    }

    fn record(conn: &mut Connection, iri: &str, class: &str, ds: &str, status: &str) {
        ensure(conn, ds, "foundation:DataSource");
        ensure(conn, status, "foundation:Status");
        let ind = Individual::new(iri);
        ind.assert(conn, class, iri, "https://example.com/icon.png", "test")
            .expect("assert");
        ind.add_property(
            conn,
            "foundation:belongsToDataSource",
            vec![Object::Iri(ds.to_string())],
            "test",
        )
        .expect("ds link");
        ind.add_property(
            conn,
            "foundation:hasStatus",
            vec![Object::Iri(status.to_string())],
            "test",
        )
        .expect("status");
    }

    fn setup(conn: &mut Connection) {
        for (iri, ptype, range) in [
            ("foundation:belongsToDataSource", "object", "owl:Thing"),
            ("foundation:hasStatus", "object", "owl:Thing"),
        ] {
            let call = crate::ai::functions::ToolCall {
                name: "define_property".to_string(),
                arguments: serde_json::json!({"operations": [
                    {"iri": iri, "label": iri, "property_type": ptype, "range": range}
                ]}),
            };
            let r = crate::ai::functions::execute_tool(conn, &call, None, None);
            assert!(r.success, "define_property {}: {:?}", iri, r.error);
        }
        for class in [
            "foundation:RawDataRecord",
            "foundation:SyncRecord",
            "foundation:DataSource",
            "foundation:Status",
        ] {
            let call = crate::ai::functions::ToolCall {
                name: "define_class".to_string(),
                arguments: serde_json::json!({"operations": [{
                    "iri": class, "label": class, "icon": "https://example.com/icon.png", "super_classes": ["owl:Thing"],
                    "add_properties": ["foundation:belongsToDataSource", "foundation:hasStatus"]
                }]}),
            };
            let r = crate::ai::functions::execute_tool(conn, &call, None, None);
            assert!(r.success, "define_class {}: {:?}", class, r.error);
        }
    }

    #[test]
    fn scan_skips_a_sync_record_that_is_also_in_error() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let ds = "foundation:DataSource_scan";
        record(
            &mut conn,
            "foundation:RawDataRecord_bad",
            "foundation:RawDataRecord",
            ds,
            STATUS_ERROR,
        );
        record(
            &mut conn,
            "foundation:SyncRecord_bad",
            "foundation:SyncRecord",
            ds,
            STATUS_ERROR,
        );

        let found =
            crate::core_ontology::data_sync::error_records_of_source(&conn, ds).expect("scan");

        assert_eq!(
            found,
            vec!["foundation:RawDataRecord_bad".to_string()],
            "a failed SyncRecord must never be requeued as raw data"
        );
    }

    #[test]
    fn scan_ignores_records_that_already_transformed() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let ds = "foundation:DataSource_ok";
        record(
            &mut conn,
            "foundation:RawDataRecord_done",
            "foundation:RawDataRecord",
            ds,
            STATUS_TRANSFORMED,
        );

        assert!(
            crate::core_ontology::data_sync::error_records_of_source(&conn, ds)
                .expect("scan")
                .is_empty()
        );
    }

    #[test]
    fn validate_retryable_accepts_an_error_record_and_returns_its_source() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let ds = "foundation:DataSource_v";
        record(
            &mut conn,
            "foundation:RawDataRecord_v",
            "foundation:RawDataRecord",
            ds,
            STATUS_ERROR,
        );

        let resolved = crate::core_ontology::data_sync::validate_retryable(
            &conn,
            "foundation:RawDataRecord_v",
        )
        .expect("retryable");

        assert_eq!(resolved, ds);
    }

    #[test]
    fn validate_retryable_refuses_a_healthy_record() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let ds = "foundation:DataSource_v2";
        record(
            &mut conn,
            "foundation:RawDataRecord_ok",
            "foundation:RawDataRecord",
            ds,
            STATUS_TRANSFORMED,
        );

        let err = crate::core_ontology::data_sync::validate_retryable(
            &conn,
            "foundation:RawDataRecord_ok",
        )
        .expect_err("must refuse");

        assert!(err.contains("only Error records"), "{}", err);
    }

    #[test]
    fn validate_retryable_refuses_something_that_is_not_a_raw_record() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let ds = "foundation:DataSource_v3";
        record(
            &mut conn,
            "foundation:SyncRecord_x",
            "foundation:SyncRecord",
            ds,
            STATUS_ERROR,
        );

        let err =
            crate::core_ontology::data_sync::validate_retryable(&conn, "foundation:SyncRecord_x")
                .expect_err("must refuse");

        assert!(err.contains("is not a RawDataRecord"), "{}", err);
    }

    #[test]
    fn raw_record_page_preserves_counts_filter_and_cursor() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let ds = "foundation:DataSource_page";
        record(
            &mut conn,
            "foundation:RawDataRecord_1",
            "foundation:RawDataRecord",
            ds,
            STATUS_ERROR,
        );
        record(
            &mut conn,
            "foundation:RawDataRecord_2",
            "foundation:RawDataRecord",
            ds,
            STATUS_TRANSFORMED,
        );
        record(
            &mut conn,
            "foundation:RawDataRecord_3",
            "foundation:RawDataRecord",
            ds,
            STATUS_ERROR,
        );

        let first =
            crate::core_ontology::data_sync::list_raw_records(&conn, ds, None, 2, None, None)
                .expect("first page");
        assert_eq!(first.records.len(), 2);
        assert!(first.has_more);
        assert!(first.next_cursor.is_some());
        assert_eq!(first.counts_by_status.get(STATUS_ERROR), Some(&2));
        assert_eq!(first.counts_by_status.get(STATUS_TRANSFORMED), Some(&1));

        let second = crate::core_ontology::data_sync::list_raw_records(
            &conn,
            ds,
            None,
            2,
            Some(first.snapshot_tx),
            first.next_cursor,
        )
        .expect("second page");
        assert_eq!(second.snapshot_tx, first.snapshot_tx);
        assert_eq!(second.records.len(), 1);
        assert!(!second.has_more);

        let errors = crate::core_ontology::data_sync::list_raw_records(
            &conn,
            ds,
            Some(STATUS_ERROR),
            10,
            Some(first.snapshot_tx),
            None,
        )
        .expect("filtered page");
        assert_eq!(errors.records.len(), 2);
        assert!(errors
            .records
            .iter()
            .all(|record| record.transform_status == STATUS_ERROR));
        assert_eq!(errors.counts_by_status, first.counts_by_status);
    }

    #[test]
    fn raw_record_page_propagates_missing_status() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let ds = "foundation:DataSource_invalid";
        ensure(&mut conn, ds, "foundation:DataSource");
        let raw = Individual::new("foundation:RawDataRecord_without_status");
        raw.assert(
            &mut conn,
            "foundation:RawDataRecord",
            "invalid",
            "https://example.com/icon.png",
            "test",
        )
        .expect("assert raw");
        raw.add_property(
            &mut conn,
            "foundation:belongsToDataSource",
            vec![Object::Iri(ds.to_string())],
            "test",
        )
        .expect("source");

        let error =
            crate::core_ontology::data_sync::list_raw_records(&conn, ds, None, 10, None, None)
                .expect_err("missing status must not become an empty string");
        assert!(error.contains("foundation:hasStatus"), "{}", error);
    }
}

#[cfg(test)]
mod sync_run_tests {
    use super::*;
    use crate::core_ontology::data_sync::{finish_sync_run, start_sync_run};
    use crate::eavto::test_helpers::setup_test_db;

    fn setup(conn: &mut Connection) {
        let props: &[(&str, &str, &str)] = &[
            ("foundation:belongsToDataSource", "object", "owl:Thing"),
            ("foundation:hasStatus", "object", "owl:Thing"),
            ("foundation:hasStartTime", "datatype", "xsd:dateTime"),
            ("foundation:hasEndTime", "datatype", "xsd:dateTime"),
            ("foundation:runStagedCount", "datatype", "xsd:integer"),
            ("foundation:runFailedCount", "datatype", "xsd:integer"),
            ("foundation:runError", "datatype", "xsd:string"),
        ];
        for (iri, ptype, range) in props {
            let mut op = serde_json::json!(
                {"iri": iri, "label": iri, "property_type": ptype, "range": range}
            );
            // Numeric properties are required to declare a unit.
            if *range == "xsd:integer" {
                op["unit"] = serde_json::json!("unit:UNITLESS");
            }
            let call = crate::ai::functions::ToolCall {
                name: "define_property".to_string(),
                arguments: serde_json::json!({"operations": [op]}),
            };
            let r = crate::ai::functions::execute_tool(conn, &call, None, None);
            assert!(r.success, "define_property {}: {:?}", iri, r.error);
        }
        for class in [
            "foundation:SyncRun",
            "foundation:DataSource",
            "foundation:Status",
        ] {
            let call = crate::ai::functions::ToolCall {
                name: "define_class".to_string(),
                arguments: serde_json::json!({"operations": [{
                    "iri": class, "label": class, "icon": "https://example.com/icon.png",
                    "super_classes": ["owl:Thing"],
                    "add_properties": props.iter().map(|(i, _, _)| *i).collect::<Vec<_>>()
                }]}),
            };
            let r = crate::ai::functions::execute_tool(conn, &call, None, None);
            assert!(r.success, "define_class {}: {:?}", class, r.error);
        }
        // Icon names are validated against seeded icon entities; the bare test DB
        // has none, so the one this code uses has to exist here.
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, tx, origin_id, created_at, retracted, is_current) \
             VALUES ('foundation:icon-material-symbols-name-sync', 'foundation:iconKey', 'sync', 'literal', 1, 1, 0, 0, 1)",
            [],
        ).expect("seed icon");

        for (iri, class) in [
            ("foundation:DataSource_t", "foundation:DataSource"),
            ("foundation:InProgress", "foundation:Status"),
            ("foundation:Completed", "foundation:Status"),
            ("foundation:Status_1781300928559", "foundation:Status"),
        ] {
            crate::owl::Individual::new(iri)
                .assert(conn, class, iri, "https://example.com/icon.png", "test")
                .expect("seed");
        }
    }

    #[test]
    fn a_run_is_visible_while_it_is_still_going() {
        let mut conn = setup_test_db();
        setup(&mut conn);

        start_sync_run(&mut conn, "foundation:DataSource_t").expect("start");

        let out = datasync_list_runs(&conn, &serde_json::json!({}))
            .result
            .expect("runs");
        assert_eq!(out["count"].as_u64().unwrap(), 1,
            "a run must be listed before it finishes — a crashed cycle is exactly what needs to show");
        assert_eq!(
            out["runs"][0]["status"].as_str().unwrap(),
            "foundation:InProgress"
        );
    }

    #[test]
    fn a_finished_run_carries_its_counts_and_outcome() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let run = start_sync_run(&mut conn, "foundation:DataSource_t").expect("start");

        finish_sync_run(&mut conn, &run, 42, 3, None).expect("finish");

        let out = datasync_list_runs(&conn, &serde_json::json!({}))
            .result
            .expect("runs");
        let first = &out["runs"][0];
        assert_eq!(first["status"].as_str().unwrap(), "foundation:Completed");
        assert_eq!(first["staged_count"].as_i64().unwrap(), 42);
        assert_eq!(first["failed_count"].as_i64().unwrap(), 3);
        assert!(first["finished_at"].as_str().is_some());
        assert!(first["error"].is_null(), "a clean run carries no error");
    }

    #[test]
    fn a_failed_run_keeps_its_reason() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let run = start_sync_run(&mut conn, "foundation:DataSource_t").expect("start");

        finish_sync_run(&mut conn, &run, 0, 0, Some("HTTP 503 from the endpoint")).expect("finish");

        let out = datasync_list_runs(&conn, &serde_json::json!({}))
            .result
            .expect("runs");
        assert_eq!(
            out["runs"][0]["status"].as_str().unwrap(),
            "foundation:Status_1781300928559"
        );
        assert_eq!(
            out["runs"][0]["error"].as_str().unwrap(),
            "HTTP 503 from the endpoint"
        );
    }

    #[test]
    fn filtering_by_outcome_returns_only_that_outcome() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        let ok = start_sync_run(&mut conn, "foundation:DataSource_t").expect("start");
        finish_sync_run(&mut conn, &ok, 5, 0, None).expect("finish ok");
        let bad = start_sync_run(&mut conn, "foundation:DataSource_t").expect("start");
        finish_sync_run(&mut conn, &bad, 0, 0, Some("boom")).expect("finish bad");

        let failed = datasync_list_runs(
            &conn,
            &serde_json::json!({
                "status": "foundation:Status_1781300928559"
            }),
        )
        .result
        .expect("runs");

        assert_eq!(failed["count"].as_u64().unwrap(), 1);
        assert_eq!(failed["runs"][0]["error"].as_str().unwrap(), "boom");
    }

    #[test]
    fn filtering_by_source_excludes_other_sources() {
        let mut conn = setup_test_db();
        setup(&mut conn);
        crate::owl::Individual::new("foundation:DataSource_other")
            .assert(
                &mut conn,
                "foundation:DataSource",
                "other",
                "https://example.com/icon.png",
                "test",
            )
            .expect("seed");
        start_sync_run(&mut conn, "foundation:DataSource_t").expect("start");

        let out = datasync_list_runs(
            &conn,
            &serde_json::json!({
                "data_source_iri": "foundation:DataSource_other"
            }),
        )
        .result
        .expect("runs");

        assert_eq!(out["count"].as_u64().unwrap(), 0);
    }
}
