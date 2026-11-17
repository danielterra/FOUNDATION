use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

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
    let record_iri = external_id
        .as_deref()
        .map(|external_id| deterministic_record_iri(&data_source_iri, external_id))
        .unwrap_or_else(|| format!("foundation:RawDataRecord_{}", Uuid::new_v4()));

    if external_id.is_some() {
        let record_iri_for_read = record_iri.clone();
        if executor
            .read(move |conn| raw_record_exists(conn, &record_iri_for_read))
            .await
            .unwrap_or(false)
        {
            return Ok(record_iri);
        }
    }

    let large_payload = (payload.len() > INLINE_LIMIT_BYTES)
        .then(|| persist_raw_file(&payload))
        .transpose()?;
    let payload_str = if large_payload.is_none() {
        Some(String::from_utf8_lossy(&payload).into_owned())
    } else {
        None
    };
    let payload_hash = large_payload
        .as_ref()
        .map(|_| format!("sha256:{:x}", Sha256::digest(&payload)));
    let payload_size = payload.len() as i64;
    let status_iri = if external_id.is_some() {
        STATUS_PENDING
    } else {
        STATUS_SKIPPED
    };
    let record_iri_for_write = record_iri.clone();
    let data_source_for_write = data_source_iri.clone();
    let source_ref_for_write = source_ref.clone();
    let external_id_for_write = external_id.clone();
    let payload_file_for_write = large_payload
        .as_ref()
        .map(|(stored_path, _)| stored_path.clone());
    let payload_hash_for_write = payload_hash.clone();
    let payload_filename = large_payload
        .as_ref()
        .map(|(_, absolute_path)| {
            absolute_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .ok_or_else(|| "Raw payload file is missing its file name".to_string())
        })
        .transpose()?;

    let write_result = executor
        .write(move |conn| {
            if external_id_for_write.is_some() && raw_record_exists(conn, &record_iri_for_write)? {
                return serde_json::to_string(&(record_iri_for_write, false))
                    .map_err(|e| e.to_string());
            }

            write_record_header(
                conn,
                &record_iri_for_write,
                &data_source_for_write,
                external_id_for_write.as_deref(),
                &source_ref_for_write,
                now_ms,
                status_iri,
            )?;

            if let Some(payload_str) = payload_str {
                Individual::new(&record_iri_for_write)
                    .add_property(
                        conn,
                        "foundation:rawPayload",
                        vec![str_lit(payload_str)],
                        "data_sync",
                    )
                    .map_err(|e| format!("rawPayload: {}", e))?;
            }

            if let (Some(stored_path), Some(hash), Some(file_name)) = (
                payload_file_for_write,
                payload_hash_for_write,
                payload_filename,
            ) {
                let file_iri = format!("foundation:RawFile_{}", Uuid::new_v4());
                assert_file_individual(
                    conn,
                    &FileMetadata {
                        iri: &file_iri,
                        class_iri: "foundation:File",
                        icon: "description",
                        file_name: &file_name,
                        stored_path: &stored_path,
                        size: payload_size,
                        hash: &hash,
                        mime_type: "application/json",
                        timestamp_ms: now_ms,
                        origin: "data_sync",
                    },
                )?;

                Individual::new(&record_iri_for_write)
                    .add_property(
                        conn,
                        "foundation:rawFile",
                        vec![Object::Iri(file_iri)],
                        "data_sync",
                    )
                    .map_err(|e| format!("rawFile: {}", e))?;
            }

            if let Some(external_id) = external_id_for_write.as_deref() {
                crate::core_ontology::ingestion::enqueue_source_item(
                    conn,
                    "data_sync",
                    &format!("{data_source_for_write}:{source_ref_for_write}"),
                    &record_iri_for_write,
                    &format!("{data_source_for_write}:{external_id}"),
                )?;
            }

            serde_json::to_string(&(record_iri_for_write, true)).map_err(|e| e.to_string())
        })
        .await;

    match write_result {
        Ok(serialized) => {
            let (record_iri, created): (String, bool) =
                serde_json::from_str(&serialized).map_err(|e| e.to_string())?;
            if !created {
                cleanup_persisted_file(&large_payload);
            }
            Ok(record_iri)
        }
        Err(error) => {
            cleanup_persisted_file(&large_payload);
            Err(error)
        }
    }
}

fn deterministic_record_iri(data_source_iri: &str, external_id: &str) -> String {
    let identity = format!("{data_source_iri}\0{external_id}");
    format!(
        "foundation:RawDataRecord_{:x}",
        Sha256::digest(identity.as_bytes())
    )
}

fn raw_record_exists(conn: &crate::owl::Connection, record_iri: &str) -> Result<bool, String> {
    crate::owl::query_by_entity_predicate(conn, record_iri, "rdf:type")
        .map(|result| {
            result
                .triples
                .iter()
                .any(|triple| triple.object.as_iri() == Some("foundation:RawDataRecord"))
        })
        .map_err(|e| e.to_string())
}

fn cleanup_persisted_file(file: &Option<(String, std::path::PathBuf)>) {
    if let Some((_, absolute_path)) = file {
        let _ = std::fs::remove_file(absolute_path);
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
    ind.assert(
        conn,
        "foundation:RawDataRecord",
        record_iri,
        "receipt_long",
        "data_sync",
    )
    .map_err(|e| format!("assert RawDataRecord: {}", e))?;

    ind.add_property(
        conn,
        "foundation:hasStatus",
        vec![Object::Iri(status_iri.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("hasStatus: {}", e))?;

    ind.add_property(
        conn,
        "foundation:belongsToDataSource",
        vec![Object::Iri(data_source_iri.to_string())],
        "data_sync",
    )
    .map_err(|e| format!("belongsToDataSource: {}", e))?;

    if let Some(ext_id) = external_id {
        ind.add_property(
            conn,
            "foundation:externalId",
            vec![str_lit(ext_id)],
            "data_sync",
        )
        .map_err(|e| format!("externalId: {}", e))?;
    }

    if !source_ref.is_empty() {
        ind.add_property(
            conn,
            "foundation:rawSourceRef",
            vec![str_lit(source_ref)],
            "data_sync",
        )
        .map_err(|e| format!("rawSourceRef: {}", e))?;
    }

    ind.add_property(
        conn,
        "foundation:receivedAt",
        vec![dt_lit(now_rfc3339)],
        "data_sync",
    )
    .map_err(|e| format!("receivedAt: {}", e))?;

    ind.add_property(conn, "foundation:retryCount", vec![int_lit(0)], "data_sync")
        .map_err(|e| format!("retryCount: {}", e))?;

    Ok(())
}

fn persist_raw_file(payload: &[u8]) -> Result<(String, std::path::PathBuf), String> {
    let dir = crate::paths::foundation_dir().join("data_sync_raw");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create data_sync_raw dir: {}", e))?;
    let file_name = format!("raw_{}.json", Uuid::new_v4());
    let abs_path = dir.join(&file_name);
    std::fs::write(&abs_path, payload).map_err(|e| format!("write raw file: {}", e))?;
    Ok((crate::paths::to_portable_path(&abs_path), abs_path))
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod staging_tests;
