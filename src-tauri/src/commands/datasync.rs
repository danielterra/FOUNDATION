use tauri::{AppHandle, State};

use crate::owl::{DbExecutor, Individual, Object};

const STATUS_PENDING: &str = "foundation:Pending";

/// Aggregate listing of DataSources for the Data Sync Manager panel.
///
/// Bound-only by label (low-cardinality list, PO decision 2026-06-18): ordered
/// by rdfs:label ASC via find_entities_with_property_bounded. No keyset cursor —
/// offset is stable for a label-ordered bounded list. `snapshot_tx` is the realtime
/// replay cursor for the frontend subscription window, NOT a pagination cursor.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn datasync__list_sources(
    limit: Option<i64>,
    offset: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let effective_limit = limit.unwrap_or(50).max(1);
    let effective_offset = offset.unwrap_or(0).max(0);
    let response = executor
        .read(move |conn| {
            crate::core_ontology::data_sync::list_data_sources(
                conn,
                effective_limit,
                effective_offset,
            )
        })
        .await?;

    serde_json::to_string(&response).map_err(|e| e.to_string())
}

/// List RawDataRecords for a DataSource with optional status filter.
///
/// Delegates to the canonical `list_raw_records` implementation so both this command
/// and the MCP tool share one code path.
///
/// `creation_tx` IS the domain ordering key because RawDataRecord is create-immutable: written
/// once, never updated. This is the ADR exception (foundation:ArchitectureDecisionRecord_1781556688201)
/// for entities whose natural order is creation-recency — keyset-by-creation_tx is stable and correct.
/// `snapshot_tx` in the response is the realtime replay cursor for the frontend subscription
/// window, distinct from `next_cursor` which is the keyset pagination cursor.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn datasync__list_raw(
    data_source_iri: String,
    transform_status: Option<String>,
    limit: Option<i64>,
    snapshot_tx: Option<i64>,
    after_tx: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    if data_source_iri.is_empty() {
        return Err("data_source_iri is required".to_string());
    }

    let effective_limit = limit.unwrap_or(50);
    let status_filter = transform_status.clone();

    let result = executor
        .read(move |conn| {
            crate::core_ontology::data_sync::list_raw_records(
                conn,
                &data_source_iri,
                status_filter.as_deref(),
                effective_limit,
                snapshot_tx,
                after_tx,
            )
        })
        .await?;

    let records_json: Vec<serde_json::Value> = result
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

    let counts_by_status: serde_json::Map<String, serde_json::Value> = result
        .counts_by_status
        .into_iter()
        .map(|(k, v)| (k, serde_json::Value::Number(v.into())))
        .collect();

    let response = serde_json::json!({
        "items": records_json,
        "next_cursor": result.next_cursor,
        "has_more": result.has_more,
        "counts_by_status": counts_by_status,
        "snapshot_tx": result.snapshot_tx,
    });

    serde_json::to_string(&response).map_err(|e| e.to_string())
}

/// Fetch full detail of a single RawDataRecord including payload and transform error.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn datasync__inspect_raw(
    raw_record_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    if raw_record_iri.is_empty() {
        return Err("raw_record_iri is required".to_string());
    }

    let detail = executor
        .read(move |conn| {
            crate::core_ontology::data_sync::inspect_raw_record(conn, &raw_record_iri)
        })
        .await?;

    serde_json::to_string(&detail).map_err(|e| e.to_string())
}

/// Retry transform for a single RawDataRecord that is in Error status.
///
/// Validates type and status before writing — prevents accidental requeue of
/// already-transformed or pending records.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn datasync__retry_raw(
    app: AppHandle,
    raw_record_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    if raw_record_iri.is_empty() {
        return Err("raw_record_iri is required".to_string());
    }

    let raw_iri_clone = raw_record_iri.clone();
    // Same guard the MCP tool uses — one definition of "may this be retried".
    let ds_iri = executor
        .read(move |conn| crate::core_ontology::data_sync::validate_retryable(conn, &raw_iri_clone))
        .await?;

    let raw_iri_write = raw_record_iri.clone();
    executor
        .write(move |conn| {
            crate::core_ontology::data_sync::increment_retry_count(conn, &raw_iri_write)?;
            Individual::clear_property(
                conn,
                &raw_iri_write,
                "foundation:transformError",
                "datasync_retry_raw",
            )
            .map_err(|e| e.to_string())?;
            Individual::new(&raw_iri_write)
                .add_property(
                    conn,
                    "foundation:hasStatus",
                    vec![Object::Iri(STATUS_PENDING.to_string())],
                    "datasync_retry_raw",
                )
                .map_err(|e| e.to_string())?;
            Ok(String::new())
        })
        .await?;

    let triggered = crate::ai::functions::data_sync::trigger_transform(&app, &ds_iri).await;

    let response = serde_json::json!({
        "success": true,
        "triggered": triggered,
    });

    serde_json::to_string(&response).map_err(|e| e.to_string())
}

/// Trigger an extract + transform cycle for the given DataSource.
///
/// Runs `run_extract_cycle` (HTTP fetch → staging) followed by
/// `trigger_transform_automation` so João sees the full pipeline
/// result, not just staged records.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn datasync__run(app: AppHandle, data_source_iri: String) -> Result<String, String> {
    if data_source_iri.is_empty() {
        return Err("data_source_iri is required".to_string());
    }

    let staged = crate::data_sync::request_worker::run_extract_cycle(&app, &data_source_iri)
        .await
        .map_err(|e| e.to_string())?;

    let transform_triggered =
        crate::ai::functions::data_sync::trigger_transform(&app, &data_source_iri).await;

    let result = serde_json::json!({
        "staged_count": staged.len(),
        "staged_iris_sample": staged.iter().take(50).collect::<Vec<_>>(),
        "transform_triggered": transform_triggered,
    });

    serde_json::to_string(&result).map_err(|e| e.to_string())
}
