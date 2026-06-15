use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::files::{assert_file_individual, FileMetadata};
use crate::owl::{DbExecutor, Individual, Object};

/// Maximum byte length stored inline as `rawPayload`. Payloads larger than
/// this are written to a File on disk and linked via `rawFile`.
const INLINE_LIMIT_BYTES: usize = 64 * 1024;

/// Status IRI for a newly staged RawDataRecord (awaiting transform).
const STATUS_PENDING: &str = "foundation:Pending";

/// Status IRI for a record that cannot be processed (e.g. missing externalId).
const STATUS_SKIPPED: &str = "foundation:Status_1781300928612";

fn str_lit(v: impl Into<String>) -> Object {
    Object::Literal {
        value: v.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

fn int_lit(v: i64) -> Object {
    Object::Literal {
        value: v.to_string(),
        datatype: Some("xsd:integer".to_string()),
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

/// Parameters for staging a single raw payload from an external source.
pub struct StagingInput<'a> {
    /// IRI of the `foundation:DataSource` this payload came from.
    pub data_source_iri: &'a str,
    /// Raw bytes of the payload.
    pub payload: Vec<u8>,
    /// External identifier for the item (used for deterministic IRI building and idempotency).
    /// When `None`, the record is staged with status `Skipped` so the raw payload is preserved
    /// but no domain entity is produced.
    pub external_id: Option<String>,
    /// Free-form reference back to the origin (e.g. HTTPResponse IRI).
    pub source_ref: String,
}

/// Materialises exactly one `foundation:RawDataRecord` inside a single
/// `DbExecutor::write` call so the notify path fires and the realtime layer
/// picks it up without any manual emit.
///
/// Small payloads (≤ INLINE_LIMIT_BYTES) are stored as `rawPayload` inline.
/// Larger payloads are written to disk first, then linked via `rawFile`. The
/// file write happens *before* the triple is asserted so a crash mid-way leaves
/// the DB consistent (no dangling rawFile reference).
///
/// Returns the IRI of the created `RawDataRecord`.
pub async fn stage_raw_record(
    executor: &DbExecutor,
    input: StagingInput<'_>,
) -> Result<String, String> {
    let StagingInput {
        data_source_iri,
        payload,
        external_id,
        source_ref,
    } = input;

    let data_source_iri = data_source_iri.to_string();
    let now_ms = Utc::now().timestamp_millis();
    let record_iri = format!("foundation:RawDataRecord_{}", now_ms);

    if external_id.is_none() {
        let record_iri_clone = record_iri.clone();
        let data_source_iri_clone = data_source_iri.clone();
        let source_ref_clone = source_ref.clone();
        executor.write(move |conn| {
            write_record_header(
                conn,
                &record_iri_clone,
                &data_source_iri_clone,
                None,
                &source_ref_clone,
                now_ms,
                STATUS_SKIPPED,
            )?;
            Ok(record_iri_clone)
        }).await?;
        return Ok(record_iri);
    }

    let ext_id = external_id.unwrap();

    if payload.len() <= INLINE_LIMIT_BYTES {
        let payload_str = String::from_utf8_lossy(&payload).into_owned();
        let record_iri_clone = record_iri.clone();
        let data_source_iri_clone = data_source_iri.clone();
        let source_ref_clone = source_ref.clone();
        let ext_id_clone = ext_id.clone();
        executor.write(move |conn| {
            write_record_header(
                conn,
                &record_iri_clone,
                &data_source_iri_clone,
                Some(ext_id_clone.as_str()),
                &source_ref_clone,
                now_ms,
                STATUS_PENDING,
            )?;
            Individual::new(&record_iri_clone)
                .add_property(conn, "foundation:rawPayload",
                    vec![str_lit(payload_str)], "data_sync")
                .map_err(|e| format!("rawPayload: {}", e))?;
            Ok(record_iri_clone)
        }).await
    } else {
        // Write file to disk before asserting the triple to preserve DB consistency
        // on a crash between the two operations.
        let stored_path = persist_raw_file(&payload, now_ms)?;
        let hash = format!("sha256:{:x}", Sha256::digest(&payload));
        let size = payload.len() as i64;

        let record_iri_clone = record_iri.clone();
        let data_source_iri_clone = data_source_iri.clone();
        let source_ref_clone = source_ref.clone();
        let ext_id_clone = ext_id.clone();
        executor.write(move |conn| {
            write_record_header(
                conn,
                &record_iri_clone,
                &data_source_iri_clone,
                Some(ext_id_clone.as_str()),
                &source_ref_clone,
                now_ms,
                STATUS_PENDING,
            )?;

            let file_iri = format!("foundation:RawFile_{}", now_ms);
            assert_file_individual(conn, &FileMetadata {
                iri: &file_iri,
                class_iri: "foundation:File",
                icon: "description",
                file_name: &format!("raw_{}_{}.json", data_source_iri_clone.replace(':', "_"), now_ms),
                stored_path: &stored_path,
                size,
                hash: &hash,
                mime_type: "application/json",
                timestamp_ms: now_ms,
                origin: "data_sync",
            })?;

            Individual::new(&record_iri_clone)
                .add_property(conn, "foundation:rawFile",
                    vec![Object::Iri(file_iri)], "data_sync")
                .map_err(|e| format!("rawFile: {}", e))?;

            Ok(record_iri_clone)
        }).await
    }
}

fn write_record_header(
    conn: &mut crate::owl::Connection,
    record_iri: &str,
    data_source_iri: &str,
    external_id: Option<&str>,
    source_ref: &str,
    now_ms: i64,
    status_iri: &str,
) -> Result<(), String> {
    let now_rfc3339 = chrono::DateTime::from_timestamp_millis(now_ms)
        .unwrap_or_default()
        .to_rfc3339();

    let ind = Individual::new(record_iri);
    ind.assert(conn, "foundation:RawDataRecord", record_iri, "receipt_long", "data_sync")
        .map_err(|e| format!("assert RawDataRecord: {}", e))?;

    ind.add_property(conn, "foundation:hasStatus",
        vec![Object::Iri(status_iri.to_string())], "data_sync")
        .map_err(|e| format!("hasStatus: {}", e))?;

    ind.add_property(conn, "foundation:belongsToDataSource",
        vec![Object::Iri(data_source_iri.to_string())], "data_sync")
        .map_err(|e| format!("belongsToDataSource: {}", e))?;

    if let Some(ext_id) = external_id {
        ind.add_property(conn, "foundation:externalId",
            vec![str_lit(ext_id)], "data_sync")
            .map_err(|e| format!("externalId: {}", e))?;
    }

    if !source_ref.is_empty() {
        ind.add_property(conn, "foundation:rawSourceRef",
            vec![str_lit(source_ref)], "data_sync")
            .map_err(|e| format!("rawSourceRef: {}", e))?;
    }

    ind.add_property(conn, "foundation:receivedAt",
        vec![dt_lit(now_rfc3339)], "data_sync")
        .map_err(|e| format!("receivedAt: {}", e))?;

    ind.add_property(conn, "foundation:retryCount",
        vec![int_lit(0)], "data_sync")
        .map_err(|e| format!("retryCount: {}", e))?;

    Ok(())
}

fn persist_raw_file(payload: &[u8], now_ms: i64) -> Result<String, String> {
    let dir = crate::paths::foundation_dir().join("data_sync_raw");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create data_sync_raw dir: {}", e))?;
    let file_name = format!("raw_{}.json", now_ms);
    let abs_path = dir.join(&file_name);
    std::fs::write(&abs_path, payload)
        .map_err(|e| format!("write raw file: {}", e))?;
    Ok(crate::paths::to_portable_path(&abs_path))
}
