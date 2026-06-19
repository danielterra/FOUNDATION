use crate::eavto::Connection;
use crate::owl::{Result, OwlError};
use rusqlite::types::Value as SqlValue;

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;

/// One row from [`list_ai_models_as_of`].
#[derive(Debug, Clone)]
pub struct AiModelRow {
    pub subject: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub model_identifier: Option<String>,
    pub model_version: Option<String>,
    pub is_default: bool,
}

/// Returns a stable, as-of-snapshot page of AI model individuals, ordered by
/// (isDefaultModel DESC, lower(label) ASC, modelIdentifier ASC, subject ASC).
///
/// **Membership as-of**: when `service_iri` is Some, membership is defined by
/// `(subject, offeredBy_predicate, service_iri)` active at snapshot_tx; otherwise
/// by `(subject, type_predicate, model_class)` active at snapshot_tx.
///
/// **Property values as-of**: `isDefaultModel`, `label`, `modelIdentifier`,
/// `modelVersion`, and `description` are all resolved at MAX(tx) ≤ snapshot_tx.
///
/// All predicate IRIs and class IRIs are supplied by the caller — this function
/// contains no hardcoded `foundation:*` or `anthropic:*` IRIs.
///
/// Returns `(rows, has_more)`. The caller derives `next_cursor` from `offset + limit`
/// when `has_more` is true.
pub fn list_ai_models_as_of(
    conn: &Connection,
    service_iri: Option<&str>,
    offered_by_predicate: &str,
    type_predicate: &str,
    model_class: &str,
    default_model_predicate: &str,
    label_predicate: &str,
    identifier_predicate: &str,
    version_predicate: &str,
    description_predicate: &str,
    snapshot_tx: i64,
    limit: i64,
    offset: i64,
) -> Result<(Vec<AiModelRow>, bool)> {
    // Build the membership CTE dynamically based on whether a service filter is present.
    let (mem_predicate, mem_object) = if let Some(svc) = service_iri {
        (offered_by_predicate.to_string(), svc.to_string())
    } else {
        (type_predicate.to_string(), model_class.to_string())
    };

    // The SQL is a single query:
    // 1. members CTE — active membership as-of snapshot_tx.
    // 2. Per-member property CTEs — each resolved at MAX(tx) ≤ snapshot_tx.
    // 3. Final SELECT — ORDER BY (is_default DESC, lower(label) ASC, identifier ASC, subject ASC),
    //    LIMIT/OFFSET applied in SQL for stable offset pagination.
    //
    // We fetch limit+1 rows so the caller can detect has_more without a COUNT query.
    let sql = format!(
        "WITH members AS (\
             SELECT t.subject \
             FROM triples t \
             WHERE t.predicate = ?1 \
               AND t.object = ?2 \
               AND t.tx <= ?3 \
               AND t.tx = (\
                   SELECT MAX(t2.tx) FROM triples t2 \
                   WHERE t2.subject = t.subject \
                     AND t2.predicate = ?1 \
                     AND t2.object = ?2 \
                     AND t2.tx <= ?3 \
               ) \
               AND t.retracted = 0 \
         ), \
         prop_default AS (\
             SELECT t.subject, \
                    t.object_value AS v, \
                    t.object_boolean AS bv \
             FROM triples t \
             WHERE t.predicate = ?4 \
               AND t.retracted = 0 \
               AND t.tx <= ?3 \
               AND t.tx = (\
                   SELECT MAX(t2.tx) FROM triples t2 \
                   WHERE t2.subject = t.subject \
                     AND t2.predicate = ?4 \
                     AND t2.retracted = 0 AND t2.tx <= ?3 \
               ) \
         ), \
         prop_label AS (\
             SELECT t.subject, t.object_value AS v \
             FROM triples t \
             WHERE t.predicate = ?5 \
               AND t.retracted = 0 \
               AND t.tx <= ?3 \
               AND t.tx = (\
                   SELECT MAX(t2.tx) FROM triples t2 \
                   WHERE t2.subject = t.subject \
                     AND t2.predicate = ?5 \
                     AND t2.retracted = 0 AND t2.tx <= ?3 \
               ) \
         ), \
         prop_id AS (\
             SELECT t.subject, t.object_value AS v \
             FROM triples t \
             WHERE t.predicate = ?6 \
               AND t.retracted = 0 \
               AND t.tx <= ?3 \
               AND t.tx = (\
                   SELECT MAX(t2.tx) FROM triples t2 \
                   WHERE t2.subject = t.subject \
                     AND t2.predicate = ?6 \
                     AND t2.retracted = 0 AND t2.tx <= ?3 \
               ) \
         ), \
         prop_ver AS (\
             SELECT t.subject, t.object_value AS v \
             FROM triples t \
             WHERE t.predicate = ?7 \
               AND t.retracted = 0 \
               AND t.tx <= ?3 \
               AND t.tx = (\
                   SELECT MAX(t2.tx) FROM triples t2 \
                   WHERE t2.subject = t.subject \
                     AND t2.predicate = ?7 \
                     AND t2.retracted = 0 AND t2.tx <= ?3 \
               ) \
         ), \
         prop_desc AS (\
             SELECT t.subject, t.object_value AS v \
             FROM triples t \
             WHERE t.predicate = ?8 \
               AND t.retracted = 0 \
               AND t.tx <= ?3 \
               AND t.tx = (\
                   SELECT MAX(t2.tx) FROM triples t2 \
                   WHERE t2.subject = t.subject \
                     AND t2.predicate = ?8 \
                     AND t2.retracted = 0 AND t2.tx <= ?3 \
               ) \
         ) \
         SELECT m.subject, \
                pl.v AS label, \
                pd.v AS description, \
                pi.v AS model_identifier, \
                pv.v AS model_version, \
                CASE WHEN COALESCE(pdef.v, CAST(pdef.bv AS TEXT)) IN ('true', '1') THEN 1 ELSE 0 END AS is_default \
         FROM members m \
         LEFT JOIN prop_default pdef ON pdef.subject = m.subject \
         LEFT JOIN prop_label pl     ON pl.subject   = m.subject \
         LEFT JOIN prop_id pi        ON pi.subject   = m.subject \
         LEFT JOIN prop_ver pv       ON pv.subject   = m.subject \
         LEFT JOIN prop_desc pd      ON pd.subject   = m.subject \
         ORDER BY \
             is_default DESC, \
             LOWER(COALESCE(pl.v, '')) ASC, \
             COALESCE(pi.v, '') ASC, \
             m.subject ASC \
         LIMIT ?9 OFFSET ?10"
    );

    let mut stmt = conn.prepare(&sql)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let params: Vec<SqlValue> = vec![
        SqlValue::Text(mem_predicate),
        SqlValue::Text(mem_object),
        SqlValue::Integer(snapshot_tx),
        SqlValue::Text(default_model_predicate.to_string()),
        SqlValue::Text(label_predicate.to_string()),
        SqlValue::Text(identifier_predicate.to_string()),
        SqlValue::Text(version_predicate.to_string()),
        SqlValue::Text(description_predicate.to_string()),
        SqlValue::Integer(limit + 1),
        SqlValue::Integer(offset),
    ];

    let mut all_rows: Vec<AiModelRow> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let is_default_int: i64 = row.get(5)?;
            Ok(AiModelRow {
                subject:          row.get(0)?,
                label:            row.get(1)?,
                description:      row.get(2)?,
                model_identifier: row.get(3)?,
                model_version:    row.get(4)?,
                is_default:       is_default_int != 0,
            })
        })
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| OwlError::DatabaseError(e.to_string()))?;

    let has_more = all_rows.len() as i64 > limit;
    if has_more {
        all_rows.truncate(limit as usize);
    }

    Ok((all_rows, has_more))
}
