use std::collections::HashMap;

use serde::Serialize;

use crate::owl::{
    find_entities_with_property_bounded, find_entities_with_property_keyset, get_iri_property,
    get_literal_property, list_records_page, Connection, Individual, Object, RecordListQuery,
};

/// Status IRIs for RawDataRecord transform outcomes.
pub const STATUS_TRANSFORMED: &str = "foundation:Status_1781300928585";
pub const STATUS_PENDING: &str = "foundation:Pending";
pub const STATUS_ERROR: &str = "foundation:Status_1781300928559";
pub const STATUS_SKIPPED: &str = "foundation:Status_1781300928612";

/// Status IRIs for DataSource.
pub const DATASOURCE_STATUS_ACTIVE: &str = "foundation:Status_1781300928499";
pub const DATASOURCE_STATUS_PAUSED: &str = "foundation:Status_1781300928524";
pub const DATASOURCE_STATUS_ERROR: &str = "foundation:Status_1781300928559";

/// Status IRIs for SyncRecord.
pub const SYNCRECORD_STATUS_SYNCED: &str = "foundation:Status_1781300928637";
pub const SYNCRECORD_STATUS_PENDING: &str = "foundation:Pending";
pub const SYNCRECORD_STATUS_CONFLICT: &str = "foundation:Status_1781300928665";

const DATA_SOURCE_CLASS: &str = "foundation:DataSource";
const RAW_RECORD_CLASS: &str = "foundation:RawDataRecord";
const SYNC_RUN_CLASS: &str = "foundation:SyncRun";
const TYPE_PREDICATE: &str = "rdf:type";
const SOURCE_PREDICATE: &str = "foundation:belongsToDataSource";
const STATUS_PREDICATE: &str = "foundation:hasStatus";

#[derive(Debug, Serialize)]
pub struct RawRecordSummary {
    pub iri: String,
    pub external_id: Option<String>,
    pub received_at: Option<String>,
    pub transform_status: String,
    pub retry_count: Option<i64>,
}

#[derive(Debug)]
pub struct ListRawResult {
    pub records: Vec<RawRecordSummary>,
    pub counts_by_status: HashMap<String, i64>,
    pub snapshot_tx: i64,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct RawRecordDetail {
    pub iri: String,
    pub data_source_iri: Option<String>,
    pub external_id: Option<String>,
    pub transform_status: Option<String>,
    pub received_at: Option<String>,
    pub retry_count: Option<i64>,
    pub raw_source_ref: Option<String>,
    pub raw_payload: Option<String>,
    pub raw_file_iri: Option<String>,
    pub raw_file_path: Option<String>,
    pub transform_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DataSourceStatusSummary {
    pub data_source_iri: String,
    pub status: String,
    pub is_connected: bool,
    pub last_connection_error: Option<String>,
    pub raw_records_by_status: HashMap<String, i64>,
}

#[derive(Debug)]
pub struct DataSourceStatusPage {
    pub items: Vec<DataSourceStatusSummary>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

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
    pub snapshot_tx: i64,
    pub sources: Vec<DataSourceSummary>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct SyncRunSummary {
    pub iri: String,
    pub data_source_iri: Option<String>,
    pub status: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub staged_count: Option<i64>,
    pub failed_count: Option<i64>,
    pub error: Option<String>,
}

fn str_lit(v: impl Into<String>) -> Object {
    Object::Literal {
        value: v.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

fn dt_lit(rfc3339: impl Into<String>) -> Object {
    Object::Literal {
        value: rfc3339.into(),
        datatype: Some("xsd:dateTime".to_string()),
        language: None,
    }
}

fn optional_literal(
    conn: &Connection,
    iri: &str,
    predicate: &str,
) -> Result<Option<String>, String> {
    get_literal_property(conn, iri, predicate).map_err(|e| e.to_string())
}

fn optional_iri(conn: &Connection, iri: &str, predicate: &str) -> Result<Option<String>, String> {
    get_iri_property(conn, iri, predicate).map_err(|e| e.to_string())
}

fn required_iri(conn: &Connection, iri: &str, predicate: &str) -> Result<String, String> {
    optional_iri(conn, iri, predicate)?.ok_or_else(|| format!("{} has no {}", iri, predicate))
}

fn optional_i64(conn: &Connection, iri: &str, predicate: &str) -> Result<Option<i64>, String> {
    optional_literal(conn, iri, predicate)?
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|e| format!("{} {} is not an integer: {}", iri, predicate, e))
        })
        .transpose()
}

fn raw_record_query<'a>(
    data_source_iri: &'a str,
    status_filter: Option<&'a str>,
    snapshot_tx: Option<i64>,
    after_tx: Option<i64>,
    limit: usize,
) -> RecordListQuery<'a> {
    RecordListQuery {
        type_predicate: TYPE_PREDICATE,
        class_iri: RAW_RECORD_CLASS,
        parent_predicate: SOURCE_PREDICATE,
        parent_iri: Some(data_source_iri),
        state_predicate: STATUS_PREDICATE,
        state_iri: status_filter,
        snapshot_tx,
        after_tx,
        limit,
    }
}

pub fn list_raw_records(
    conn: &Connection,
    data_source_iri: &str,
    status_filter: Option<&str>,
    limit: i64,
    snapshot_tx: Option<i64>,
    after_tx: Option<i64>,
) -> Result<ListRawResult, String> {
    let limit =
        usize::try_from(limit).map_err(|_| "limit must be greater than zero".to_string())?;
    let page = list_records_page(
        conn,
        raw_record_query(data_source_iri, status_filter, snapshot_tx, after_tx, limit),
    )
    .map_err(|e| e.to_string())?;

    let counts_by_status = page
        .state_groups
        .into_iter()
        .filter_map(|group| group.state_iri.map(|status| (status, group.count)))
        .map(|(status, count)| {
            i64::try_from(count)
                .map(|count| (status, count))
                .map_err(|_| "raw record count exceeds i64".to_string())
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    let records = page
        .items
        .into_iter()
        .map(|item| {
            Ok(RawRecordSummary {
                transform_status: required_iri(conn, &item.iri, STATUS_PREDICATE)?,
                external_id: optional_literal(conn, &item.iri, "foundation:externalId")?,
                received_at: optional_literal(conn, &item.iri, "foundation:receivedAt")?,
                retry_count: optional_i64(conn, &item.iri, "foundation:retryCount")?,
                iri: item.iri,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(ListRawResult {
        records,
        counts_by_status,
        snapshot_tx: page.snapshot_tx,
        next_cursor: page.next_after_tx,
        has_more: page.has_more,
    })
}

pub fn count_raw_by_status(
    conn: &Connection,
    data_source_iri: &str,
) -> Result<HashMap<String, i64>, String> {
    Ok(list_raw_records(conn, data_source_iri, None, 1, None, None)?.counts_by_status)
}

pub fn inspect_raw_record(conn: &Connection, raw_iri: &str) -> Result<RawRecordDetail, String> {
    let rdf_type = required_iri(conn, raw_iri, TYPE_PREDICATE)?;
    if rdf_type != RAW_RECORD_CLASS {
        return Err(format!("{} is not a RawDataRecord", raw_iri));
    }

    let raw_file_iri = optional_iri(conn, raw_iri, "foundation:rawFile")?;
    let raw_file_path = raw_file_iri
        .as_deref()
        .map(|file_iri| optional_literal(conn, file_iri, "foundation:filePath"))
        .transpose()?
        .flatten();

    Ok(RawRecordDetail {
        iri: raw_iri.to_string(),
        data_source_iri: optional_iri(conn, raw_iri, SOURCE_PREDICATE)?,
        external_id: optional_literal(conn, raw_iri, "foundation:externalId")?,
        transform_status: optional_iri(conn, raw_iri, STATUS_PREDICATE)?,
        received_at: optional_literal(conn, raw_iri, "foundation:receivedAt")?,
        retry_count: optional_i64(conn, raw_iri, "foundation:retryCount")?,
        raw_source_ref: optional_literal(conn, raw_iri, "foundation:rawSourceRef")?,
        raw_payload: optional_literal(conn, raw_iri, "foundation:rawPayload")?,
        raw_file_iri,
        raw_file_path,
        transform_error: optional_literal(conn, raw_iri, "foundation:transformError")?,
    })
}

pub fn validate_retryable(conn: &Connection, raw_iri: &str) -> Result<String, String> {
    let rdf_type = required_iri(conn, raw_iri, TYPE_PREDICATE)?;
    if rdf_type != RAW_RECORD_CLASS {
        return Err(format!("{} is not a RawDataRecord", raw_iri));
    }

    let status = required_iri(conn, raw_iri, STATUS_PREDICATE)?;
    if status != STATUS_ERROR {
        return Err(format!(
            "record {} has status {} — only Error records can be retried",
            raw_iri, status
        ));
    }

    required_iri(conn, raw_iri, SOURCE_PREDICATE)
}

pub fn error_records_of_source(
    conn: &Connection,
    data_source_iri: &str,
) -> Result<Vec<String>, String> {
    const MAX_RETRY_SCAN: usize = 5_000;
    let page = list_records_page(
        conn,
        raw_record_query(
            data_source_iri,
            Some(STATUS_ERROR),
            None,
            None,
            MAX_RETRY_SCAN,
        ),
    )
    .map_err(|e| e.to_string())?;
    Ok(page.items.into_iter().map(|item| item.iri).collect())
}

pub fn get_data_source_status(
    conn: &Connection,
    data_source_iri: &str,
) -> Result<DataSourceStatusSummary, String> {
    Ok(DataSourceStatusSummary {
        data_source_iri: data_source_iri.to_string(),
        status: required_iri(conn, data_source_iri, STATUS_PREDICATE)?,
        is_connected: optional_literal(conn, data_source_iri, "foundation:isConnected")?
            .is_some_and(|value| value == "true"),
        last_connection_error: optional_literal(
            conn,
            data_source_iri,
            "foundation:lastConnectionError",
        )?,
        raw_records_by_status: count_raw_by_status(conn, data_source_iri)?,
    })
}

pub fn list_data_source_statuses(
    conn: &Connection,
    limit: i64,
    after_tx: Option<i64>,
) -> Result<DataSourceStatusPage, String> {
    if limit <= 0 {
        return Err("limit must be greater than zero".to_string());
    }
    let fetch_limit = limit
        .checked_add(1)
        .ok_or_else(|| "limit is too large".to_string())?;
    let mut page = find_entities_with_property_keyset(
        conn,
        TYPE_PREDICATE,
        DATA_SOURCE_CLASS,
        after_tx,
        fetch_limit,
    )
    .map_err(|e| e.to_string())?;
    let has_more = page.len() as i64 > limit;
    if has_more {
        page.pop();
    }
    let next_cursor = has_more.then(|| page.last().expect("non-empty page").1);
    let items = page
        .into_iter()
        .map(|(iri, _)| get_data_source_status(conn, &iri))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(DataSourceStatusPage {
        items,
        next_cursor,
        has_more,
    })
}

pub fn list_data_sources(
    conn: &Connection,
    limit: i64,
    offset: i64,
) -> Result<DataSourceListResponse, String> {
    if limit <= 0 {
        return Err("limit must be greater than zero".to_string());
    }
    if offset < 0 {
        return Err("offset must not be negative".to_string());
    }
    let fetch_limit = limit
        .checked_add(1)
        .ok_or_else(|| "limit is too large".to_string())?;
    let mut data_source_iris = find_entities_with_property_bounded(
        conn,
        TYPE_PREDICATE,
        DATA_SOURCE_CLASS,
        fetch_limit,
        offset,
        Some("rdfs:label"),
    )
    .map_err(|e| e.to_string())?;
    let has_more = data_source_iris.len() as i64 > limit;
    if has_more {
        data_source_iris.pop();
    }

    let snapshot_tx = list_records_page(
        conn,
        RecordListQuery {
            type_predicate: TYPE_PREDICATE,
            class_iri: RAW_RECORD_CLASS,
            parent_predicate: SOURCE_PREDICATE,
            parent_iri: None,
            state_predicate: STATUS_PREDICATE,
            state_iri: None,
            snapshot_tx: None,
            after_tx: None,
            limit: 1,
        },
    )
    .map_err(|e| e.to_string())?
    .snapshot_tx;

    let sources = data_source_iris
        .into_iter()
        .map(|iri| {
            let counts = count_raw_by_status(conn, &iri)?;
            Ok(DataSourceSummary {
                label: optional_literal(conn, &iri, "rdfs:label")?
                    .ok_or_else(|| format!("{} has no rdfs:label", iri))?,
                status: required_iri(conn, &iri, STATUS_PREDICATE)?,
                is_connected: optional_literal(conn, &iri, "foundation:isConnected")?
                    .is_some_and(|value| value == "true"),
                last_connection_error: optional_literal(
                    conn,
                    &iri,
                    "foundation:lastConnectionError",
                )?,
                sync_direction: optional_literal(conn, &iri, "foundation:syncDirection")?,
                transport_kind: optional_literal(conn, &iri, "foundation:transportKind")?,
                sync_schedule: optional_literal(conn, &iri, "foundation:syncSchedule")?,
                counts: DataSourceCounts {
                    pending: counts.get(STATUS_PENDING).copied().unwrap_or(0),
                    error: counts.get(STATUS_ERROR).copied().unwrap_or(0),
                    transformed: counts.get(STATUS_TRANSFORMED).copied().unwrap_or(0),
                    skipped: counts.get(STATUS_SKIPPED).copied().unwrap_or(0),
                },
                iri,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(DataSourceListResponse {
        snapshot_tx,
        sources,
        has_more,
    })
}

pub fn list_sync_runs(
    conn: &Connection,
    data_source_filter: Option<&str>,
    status_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<SyncRunSummary>, String> {
    if limit == 0 {
        return Err("limit must be greater than zero".to_string());
    }
    const SCAN_PAGE_SIZE: i64 = 500;
    let mut runs = Vec::with_capacity(limit);
    let mut offset = 0;
    while runs.len() < limit {
        let iris = find_entities_with_property_bounded(
            conn,
            TYPE_PREDICATE,
            SYNC_RUN_CLASS,
            SCAN_PAGE_SIZE,
            offset,
            Some("foundation:hasStartTime"),
        )
        .map_err(|e| e.to_string())?;
        let exhausted = iris.len() < SCAN_PAGE_SIZE as usize;
        offset += i64::try_from(iris.len()).map_err(|_| "run scan overflow".to_string())?;

        for iri in iris {
            let data_source_iri = optional_iri(conn, &iri, SOURCE_PREDICATE)?;
            let status = optional_iri(conn, &iri, STATUS_PREDICATE)?;
            if data_source_filter.is_some_and(|filter| data_source_iri.as_deref() != Some(filter))
                || status_filter.is_some_and(|filter| status.as_deref() != Some(filter))
            {
                continue;
            }
            runs.push(SyncRunSummary {
                data_source_iri,
                status,
                started_at: optional_literal(conn, &iri, "foundation:hasStartTime")?,
                finished_at: optional_literal(conn, &iri, "foundation:hasEndTime")?,
                staged_count: optional_i64(conn, &iri, "foundation:runStagedCount")?,
                failed_count: optional_i64(conn, &iri, "foundation:runFailedCount")?,
                error: optional_literal(conn, &iri, "foundation:runError")?,
                iri,
            });
            if runs.len() == limit {
                break;
            }
        }
        if exhausted {
            break;
        }
    }
    Ok(runs)
}

/// Builds the deterministic IRI for a synced entity.
///
/// Format: `{namespace}:{class_local_name}_{sanitized_external_id}`
///
/// Sanitization rules (injective — distinct ids never collide):
/// - Reserved punctuation uses readable tokens
/// - Source underscores are escaped so those tokens cannot collide with input text
/// - Other characters are byte-encoded with uppercase hex
///
/// The namespace and class_local_name are left as-is (callers supply clean values).
pub fn build_deterministic_iri(
    namespace: &str,
    class_local_name: &str,
    external_id: &str,
) -> String {
    let sanitized = sanitize_external_id(external_id);
    format!("{}:{}_{}", namespace, class_local_name, sanitized)
}

fn sanitize_external_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len() * 2);
    for ch in id.chars() {
        match ch {
            ':' => out.push_str("__colon__"),
            '/' => out.push_str("__slash__"),
            '\\' => out.push_str("__backslash__"),
            '#' => out.push_str("__hash__"),
            '?' => out.push_str("__q__"),
            ' ' => out.push('_'),
            '_' => out.push_str("%5F"),
            '%' => out.push_str("%25"),
            c if c.is_ascii_alphanumeric() || c == '-' || c == '.' => out.push(c),
            c => {
                for byte in c.to_string().as_bytes() {
                    out.push_str(&format!("_{:02X}", byte));
                }
            }
        }
    }
    out
}

/// Asserts (upsert) a SyncRecord linking `data_source_iri` + `external_id` to `thing_iri`.
///
/// The SyncRecord IRI is itself deterministic: `{namespace}_sync_{sanitized_external_id}`.
/// If the record already exists the latest-TX write silently updates it.
pub fn upsert_sync_record(
    conn: &mut crate::owl::Connection,
    data_source_iri: &str,
    namespace: &str,
    external_id: &str,
    thing_iri: &str,
) -> Result<String, String> {
    let sanitized = sanitize_external_id(external_id);
    let sync_iri = format!("{}_sync_{}", namespace, sanitized);
    let now = chrono::Utc::now().to_rfc3339();

    let ind = Individual::new(&sync_iri);

    let already_exists = get_iri_property(conn, &sync_iri, "rdf:type")
        .map_err(|e| e.to_string())?
        .is_some();

    if !already_exists {
        ind.assert(
            conn,
            "foundation:SyncRecord",
            &sync_iri,
            "sync_alt",
            "data_sync",
        )
        .map_err(|e| format!("assert SyncRecord: {}", e))?;
        ind.add_property(
            conn,
            "foundation:hasStatus",
            vec![Object::Iri(SYNCRECORD_STATUS_SYNCED.to_string())],
            "data_sync",
        )
        .map_err(|e| format!("SyncRecord hasStatus: {}", e))?;
    }

    ind.add_property(
        conn,
        "foundation:belongsToDataSource",
        vec![Object::Iri(data_source_iri.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("SyncRecord belongsToDataSource: {}", e))?;

    ind.add_property(
        conn,
        "foundation:externalId",
        vec![str_lit(external_id)],
        "data_sync",
    )
    .map_err(|e| format!("SyncRecord externalId: {}", e))?;

    ind.add_property(
        conn,
        "foundation:mapsToThing",
        vec![Object::Iri(thing_iri.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("SyncRecord mapsToThing: {}", e))?;

    ind.add_property(
        conn,
        "foundation:lastSyncedAt",
        vec![dt_lit(now)],
        "data_sync",
    )
    .map_err(|e| format!("SyncRecord lastSyncedAt: {}", e))?;

    ind.add_property(
        conn,
        "foundation:syncStatus",
        vec![Object::Iri(SYNCRECORD_STATUS_SYNCED.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("SyncRecord syncStatus: {}", e))?;

    Ok(sync_iri)
}

/// Status IRIs for SyncRun.
pub const SYNCRUN_STATUS_RUNNING: &str = "foundation:InProgress";
pub const SYNCRUN_STATUS_COMPLETED: &str = "foundation:Completed";
pub const SYNCRUN_STATUS_ERROR: &str = STATUS_ERROR;

/// Opens a SyncRun for a source and returns its IRI.
///
/// The run is written BEFORE the work starts, in Running, so a cycle that dies
/// mid-flight still leaves a trace. A run that only appeared on success would make
/// the crashed ones invisible — exactly the ones worth seeing.
pub fn start_sync_run(
    conn: &mut crate::owl::Connection,
    data_source_iri: &str,
) -> Result<String, String> {
    let started = chrono::Utc::now();
    let run_iri = format!("foundation:SyncRun_{}", started.timestamp_millis());
    let label = format!("Sincronização {}", started.format("%d/%m/%Y %H:%M"));

    let ind = Individual::new(&run_iri);
    ind.assert(conn, "foundation:SyncRun", &label, "sync", "data_sync")
        .map_err(|e| format!("start_sync_run assert: {}", e))?;
    ind.add_property(
        conn,
        "foundation:belongsToDataSource",
        vec![Object::Iri(data_source_iri.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("start_sync_run source: {}", e))?;
    ind.add_property(
        conn,
        "foundation:hasStatus",
        vec![Object::Iri(SYNCRUN_STATUS_RUNNING.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("start_sync_run status: {}", e))?;
    ind.add_property(
        conn,
        "foundation:hasStartTime",
        vec![dt_lit(started.to_rfc3339())],
        "data_sync",
    )
    .map_err(|e| format!("start_sync_run startTime: {}", e))?;

    Ok(run_iri)
}

/// Closes a SyncRun with its outcome and counts.
pub fn finish_sync_run(
    conn: &mut crate::owl::Connection,
    run_iri: &str,
    staged: usize,
    failed: usize,
    error: Option<&str>,
) -> Result<(), String> {
    let ind = Individual::new(run_iri);
    let status = if error.is_some() {
        SYNCRUN_STATUS_ERROR
    } else {
        SYNCRUN_STATUS_COMPLETED
    };

    ind.add_property(
        conn,
        "foundation:hasStatus",
        vec![Object::Iri(status.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("finish_sync_run status: {}", e))?;
    ind.add_property(
        conn,
        "foundation:hasEndTime",
        vec![dt_lit(chrono::Utc::now().to_rfc3339())],
        "data_sync",
    )
    .map_err(|e| format!("finish_sync_run endTime: {}", e))?;
    ind.add_property(
        conn,
        "foundation:runStagedCount",
        vec![Object::Integer(staged as i64)],
        "data_sync",
    )
    .map_err(|e| format!("finish_sync_run staged: {}", e))?;
    ind.add_property(
        conn,
        "foundation:runFailedCount",
        vec![Object::Integer(failed as i64)],
        "data_sync",
    )
    .map_err(|e| format!("finish_sync_run failed: {}", e))?;

    match error {
        Some(msg) => ind
            .add_property(conn, "foundation:runError", vec![str_lit(msg)], "data_sync")
            .map_err(|e| format!("finish_sync_run error: {}", e))?,
        // A rerun that succeeds must not keep showing the previous failure.
        None => crate::owl::Individual::clear_property(
            conn,
            run_iri,
            "foundation:runError",
            "data_sync",
        )
        .map_err(|e| format!("finish_sync_run clear error: {}", e))?,
    }

    Ok(())
}

/// Marks a RawDataRecord as Transformed and links `transformed_into_iris`.
pub fn mark_raw_transformed(
    conn: &mut crate::owl::Connection,
    raw_iri: &str,
    transformed_into_iris: &[&str],
) -> Result<(), String> {
    let ind = Individual::new(raw_iri);

    // A record that transformed successfully has no error. Leaving the previous
    // transformError behind makes a healthy record render its old failure in the
    // inspector — the record reads as broken while its status says Transformed.
    Individual::clear_property(conn, raw_iri, "foundation:transformError", "data_sync")
        .map_err(|e| format!("mark_raw_transformed clear transformError: {}", e))?;

    ind.add_property(
        conn,
        "foundation:hasStatus",
        vec![Object::Iri(STATUS_TRANSFORMED.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("mark_raw_transformed hasStatus: {}", e))?;

    if !transformed_into_iris.is_empty() {
        let objects: Vec<Object> = transformed_into_iris
            .iter()
            .map(|iri| Object::Iri(iri.to_string()))
            .collect();
        ind.add_property(conn, "foundation:transformedInto", objects, "data_sync")
            .map_err(|e| format!("mark_raw_transformed transformedInto: {}", e))?;
    }

    Ok(())
}

/// Marks a RawDataRecord as Error and records the error message.
/// The rawPayload/rawFile remains intact.
pub fn mark_raw_error(
    conn: &mut crate::owl::Connection,
    raw_iri: &str,
    error_msg: &str,
) -> Result<(), String> {
    let ind = Individual::new(raw_iri);

    ind.add_property(
        conn,
        "foundation:hasStatus",
        vec![Object::Iri(STATUS_ERROR.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("mark_raw_error hasStatus: {}", e))?;

    ind.add_property(
        conn,
        "foundation:transformError",
        vec![str_lit(error_msg)],
        "data_sync",
    )
    .map_err(|e| format!("mark_raw_error transformError: {}", e))?;

    Ok(())
}

/// Reads the current retry count for a RawDataRecord and increments it.
pub fn increment_retry_count(
    conn: &mut crate::owl::Connection,
    raw_iri: &str,
) -> Result<u32, String> {
    let current: u32 = get_literal_property(conn, raw_iri, "foundation:retryCount")
        .map_err(|e| e.to_string())?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let next = current + 1;
    Individual::new(raw_iri)
        .add_property(
            conn,
            "foundation:retryCount",
            vec![Object::Literal {
                value: next.to_string(),
                datatype: Some("xsd:integer".to_string()),
                language: None,
            }],
            "data_sync",
        )
        .map_err(|e| format!("retryCount: {}", e))?;
    Ok(next)
}

/// Updates the DataSource status and connection fields atomically.
pub fn update_datasource_connection(
    conn: &mut crate::owl::Connection,
    data_source_iri: &str,
    connected: bool,
    error_msg: Option<&str>,
) -> Result<(), String> {
    let status_iri = if connected {
        DATASOURCE_STATUS_ACTIVE
    } else {
        DATASOURCE_STATUS_ERROR
    };

    let ind = Individual::new(data_source_iri);

    ind.add_property(
        conn,
        "foundation:hasStatus",
        vec![Object::Iri(status_iri.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("datasource hasStatus: {}", e))?;

    ind.add_property(
        conn,
        "foundation:isConnected",
        vec![Object::Boolean(connected)],
        "data_sync",
    )
    .map_err(|e| format!("isConnected: {}", e))?;

    if let Some(msg) = error_msg {
        ind.add_property(
            conn,
            "foundation:lastConnectionError",
            vec![str_lit(msg)],
            "data_sync",
        )
        .map_err(|e| format!("lastConnectionError: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    #[test]
    fn data_source_page_without_sources_has_a_snapshot() {
        let conn = setup_test_db();

        let page = list_data_sources(&conn, 10, 0).unwrap();

        assert!(page.sources.is_empty());
        assert!(page.snapshot_tx >= 0);
    }

    #[test]
    fn deterministic_iri_stable_for_same_input() {
        let iri1 = build_deterministic_iri("github", "Issue", "123");
        let iri2 = build_deterministic_iri("github", "Issue", "123");
        assert_eq!(iri1, iri2);
        assert_eq!(iri1, "github:Issue_123");
    }

    #[test]
    fn sanitize_colon_is_injective_from_underscore() {
        let with_colon = build_deterministic_iri("ns", "C", "a:b");
        let with_encoded = build_deterministic_iri("ns", "C", "a__colon__b");
        assert_ne!(
            with_colon, with_encoded,
            "collision: 'a:b' and 'a__colon__b' must differ"
        );
    }

    #[test]
    fn sanitize_slash_does_not_collide() {
        let with_slash = build_deterministic_iri("ns", "C", "a/b");
        let with_text = build_deterministic_iri("ns", "C", "a__slash__b");
        assert_ne!(with_slash, with_text);
    }

    #[test]
    fn sanitize_space_becomes_underscore() {
        let iri = build_deterministic_iri("ns", "C", "hello world");
        assert_eq!(iri, "ns:C_hello_world");
    }

    #[test]
    fn sanitize_unicode_is_percent_encoded() {
        let iri = build_deterministic_iri("ns", "C", "café");
        assert!(
            iri.contains("_C3_A9") || iri.contains("caf"),
            "got: {}",
            iri
        );
    }
}
