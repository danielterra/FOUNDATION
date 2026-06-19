use serde::Serialize;
use tauri::{AppHandle, State};

use crate::owl::{DbExecutor, Individual, Object, get_iri_property, get_literal_property};

const STATUS_PENDING: &str = "foundation:Pending";
const STATUS_TRANSFORMED: &str = "foundation:Status_1781300928585";
const STATUS_ERROR: &str = "foundation:Status_1781300928559";
const STATUS_SKIPPED: &str = "foundation:Status_1781300928612";

#[derive(Debug, Serialize)]
pub struct DataSourceCounts {
    pub pending: i64,
    pub error: i64,
    pub transformed: i64,
    pub skipped: i64,
}

#[derive(Debug, Serialize)]
pub struct DataSourceSummary {
    pub iri: String,
    pub label: String,
    pub status: String,
    pub is_connected: bool,
    pub last_connection_error: Option<String>,
    pub sync_direction: Option<String>,
    pub transport_kind: Option<String>,
    pub sync_schedule: Option<String>,
    pub counts: DataSourceCounts,
}

#[derive(Debug, Serialize)]
pub struct DataSourceListResponse {
    /// MAX(tx) at query time — used by the frontend as a realtime replay cursor.
    /// Distinct from pagination; pin on the first page, echo on subsequent pages.
    pub snapshot_tx: i64,
    pub sources: Vec<DataSourceSummary>,
    /// Convenience flag: true when the returned count equals the requested limit.
    /// Clients may request the next offset page; no keyset cursor is needed because
    /// DataSources are low-cardinality and ordered by label (stable, bounded list).
    pub has_more: bool,
}

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
            let snapshot_tx: i64 = conn
                .query_row("SELECT COALESCE(MAX(tx), 0) FROM triples", [], |row| row.get(0))
                .map_err(|e| e.to_string())?;

            let ds_iris = crate::owl::find_entities_with_property_bounded(
                conn, "rdf:type", "foundation:DataSource",
                effective_limit, effective_offset, Some("rdfs:label"),
            ).map_err(|e| e.to_string())?;

            let has_more = ds_iris.len() as i64 == effective_limit;

            if ds_iris.is_empty() {
                return Ok(DataSourceListResponse {
                    snapshot_tx,
                    sources: vec![],
                    has_more: false,
                });
            }

            // Batch-count RawDataRecords per (belongsToDataSource, hasStatus) in one pass.
            // Uses triples_current which already reflects latest-TX semantics.
            let placeholders: String = ds_iris
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(", ");

            let count_sql = format!(
                "SELECT
                    ds_link.object        AS ds_iri,
                    status_link.object    AS status_iri,
                    COUNT(DISTINCT type_link.subject) AS cnt
                 FROM triples_current type_link
                 JOIN triples_current ds_link
                   ON ds_link.subject   = type_link.subject
                  AND ds_link.predicate = 'foundation:belongsToDataSource'
                  AND ds_link.object    IN ({})
                 JOIN triples_current status_link
                   ON status_link.subject   = type_link.subject
                  AND status_link.predicate = 'foundation:hasStatus'
                 WHERE type_link.predicate = 'rdf:type'
                   AND type_link.object    = 'foundation:RawDataRecord'
                 GROUP BY ds_link.object, status_link.object",
                placeholders
            );

            let params: Vec<rusqlite::types::Value> = ds_iris
                .iter()
                .map(|s| rusqlite::types::Value::Text(s.clone()))
                .collect();

            let mut counts: std::collections::HashMap<String, DataSourceCounts> = ds_iris
                .iter()
                .map(|iri| {
                    (
                        iri.clone(),
                        DataSourceCounts {
                            pending: 0,
                            error: 0,
                            transformed: 0,
                            skipped: 0,
                        },
                    )
                })
                .collect();

            {
                let mut stmt = conn.prepare(&count_sql).map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(
                        rusqlite::params_from_iter(params.iter()),
                        |row| {
                            let ds: String = row.get(0)?;
                            let status: String = row.get(1)?;
                            let cnt: i64 = row.get(2)?;
                            Ok((ds, status, cnt))
                        },
                    )
                    .map_err(|e| e.to_string())?;

                for row in rows {
                    let (ds_iri, status_iri, cnt) = row.map_err(|e| e.to_string())?;
                    if let Some(entry) = counts.get_mut(&ds_iri) {
                        if status_iri == STATUS_PENDING {
                            entry.pending = cnt;
                        } else if status_iri == STATUS_ERROR {
                            entry.error = cnt;
                        } else if status_iri == STATUS_TRANSFORMED {
                            entry.transformed = cnt;
                        } else if status_iri == STATUS_SKIPPED {
                            entry.skipped = cnt;
                        }
                    }
                }
            }

            let mut sources = Vec::with_capacity(ds_iris.len());
            for ds_iri in &ds_iris {
                let label = get_literal_property(conn, ds_iri, "rdfs:label")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| ds_iri.clone());
                let status = get_iri_property(conn, ds_iri, "foundation:hasStatus")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let is_connected = get_literal_property(conn, ds_iri, "foundation:isConnected")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false);
                let last_connection_error =
                    get_literal_property(conn, ds_iri, "foundation:lastConnectionError")
                        .ok()
                        .flatten();
                let sync_direction =
                    get_literal_property(conn, ds_iri, "foundation:syncDirection")
                        .ok()
                        .flatten();
                let transport_kind =
                    get_literal_property(conn, ds_iri, "foundation:transportKind")
                        .ok()
                        .flatten();
                let sync_schedule =
                    get_literal_property(conn, ds_iri, "foundation:syncSchedule")
                        .ok()
                        .flatten();

                let entry_counts = counts.remove(ds_iri).unwrap_or(DataSourceCounts {
                    pending: 0,
                    error: 0,
                    transformed: 0,
                    skipped: 0,
                });

                sources.push(DataSourceSummary {
                    iri: ds_iri.clone(),
                    label,
                    status,
                    is_connected,
                    last_connection_error,
                    sync_direction,
                    transport_kind,
                    sync_schedule,
                    counts: entry_counts,
                });
            }

            Ok(DataSourceListResponse {
                snapshot_tx,
                sources,
                has_more,
            })
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
            crate::ai::functions::data_sync::list_raw_records(
                conn,
                &data_source_iri,
                status_filter.as_deref(),
                effective_limit,
                after_tx,
            )
        })
        .await?;

    let records_json: Vec<serde_json::Value> = result.records.iter().map(|r| serde_json::json!({
        "iri": r.iri,
        "external_id": r.external_id,
        "received_at": r.received_at,
        "transform_status": r.transform_status,
        "retry_count": r.retry_count,
    })).collect();

    let counts_by_status: serde_json::Map<String, serde_json::Value> = result.counts_by_status
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
            crate::ai::functions::data_sync::inspect_raw_record(conn, &raw_record_iri)
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
    let ds_iri = executor
        .read(move |conn| {
            let rdf_type = crate::owl::get_iri_property(conn, &raw_iri_clone, "rdf:type")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if rdf_type != "foundation:RawDataRecord" {
                return Err(format!("{} is not a RawDataRecord", raw_iri_clone));
            }

            let status = crate::owl::get_iri_property(conn, &raw_iri_clone, "foundation:hasStatus")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if status != STATUS_ERROR {
                return Err(format!(
                    "record {} has status {} — only Error records can be retried",
                    raw_iri_clone, status
                ));
            }

            crate::owl::get_iri_property(conn, &raw_iri_clone, "foundation:belongsToDataSource")
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("{} has no belongsToDataSource", raw_iri_clone))
        })
        .await?;

    let raw_iri_write = raw_record_iri.clone();
    executor
        .write(move |conn| {
            crate::core_ontology::data_sync::increment_retry_count(conn, &raw_iri_write)?;
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
pub async fn datasync__run(
    app: AppHandle,
    data_source_iri: String,
) -> Result<String, String> {
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
