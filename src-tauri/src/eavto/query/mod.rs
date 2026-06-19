mod find;

pub use find::*;

use rusqlite::{Connection, Row, types::Value as SqlValue};
use super::triple_type::Triple;
use super::object_type::Object;
use super::query_result_type::QueryResult;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Generic: given a literal `needle` stored under `id_predicate` on a source node S,
/// traverse three hops: S →[via_predicate]→ M →[block_predicate]→ P, where P also
/// has `scope_predicate` pointing to `scope_iri`. Returns the IRI of P.
///
/// Domain-specific predicate names are supplied by the caller (OWL layer),
/// keeping this function free of Foundation-specific IRIs.
pub fn find_parent_by_linked_id_and_scope(
    conn: &Connection,
    needle: &str,
    id_predicate: &str,
    via_predicate: &str,
    block_predicate: &str,
    scope_predicate: &str,
    scope_iri: &str,
) -> Option<String> {
    conn.query_row(
        "SELECT t_msg.subject
         FROM triples t_use_id
         JOIN triples t_result_of
           ON t_result_of.predicate = ?3
          AND t_result_of.object = t_use_id.subject
          AND t_result_of.retracted = 0
         JOIN triples t_has_block
           ON t_has_block.predicate = ?4
          AND t_has_block.object = t_result_of.subject
          AND t_has_block.retracted = 0
         JOIN triples t_msg
           ON t_msg.subject = t_has_block.subject
          AND t_msg.predicate = ?5
          AND t_msg.object = ?6
          AND t_msg.retracted = 0
         WHERE t_use_id.predicate = ?2
           AND t_use_id.object_value = ?1
           AND t_use_id.retracted = 0
         LIMIT 1",
        rusqlite::params![needle, id_predicate, via_predicate, block_predicate, scope_predicate, scope_iri],
        |row| row.get(0),
    ).ok()
}

/// SQL fragment that ensures the row is the latest assertion for its
/// (subject, predicate) group. The `is_current` flag is maintained eagerly by
/// the write path so no correlated subquery is needed.
const AND_IS_CURRENT: &str = "AND t.is_current = 1";

pub fn get_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, 0 AS retracted, created_at
         FROM triples_current
         WHERE subject = ?"
    )?;

    let triples: Vec<Triple> = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

pub fn get_retracted_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND retracted = 1
         ORDER BY predicate, tx DESC"
    )?;

    let triples = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

pub fn get_retraction_tx(conn: &Connection, entity: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT MAX(tx) FROM triples WHERE subject = ? AND retracted = 1",
        [entity],
        |row| row.get::<_, Option<i64>>(0),
    ).map_err(Into::into)
}


/// Returns all active triples for a subject as they existed just before `before_tx`.
/// For each (subject, predicate), returns the group at MAX(tx) where retracted=0 AND tx < before_tx.
pub fn get_last_active_by_entity_before_tx(
    conn: &Connection,
    entity: &str,
    before_tx: i64,
) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, 0 AS retracted, created_at
         FROM triples t
         WHERE t.subject = ?1
           AND t.retracted = 0
           AND t.tx < ?2
           AND t.tx = (
               SELECT MAX(tx) FROM triples t2
               WHERE t2.subject = ?1
                 AND t2.predicate = t.predicate
                 AND t2.retracted = 0
                 AND t2.tx < ?2
           )"
    )?;
    let triples = stmt
        .query_map(rusqlite::params![entity, before_tx], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(QueryResult::new(triples))
}

/// Returns all active triples with the given predicate as they existed just before `before_tx`.
/// For each (subject, predicate), returns the group at MAX(tx) where retracted=0 AND tx < before_tx.
pub fn get_last_active_by_predicate_before_tx(
    conn: &Connection,
    predicate: &str,
    before_tx: i64,
) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, 0 AS retracted, created_at
         FROM triples t
         WHERE t.predicate = ?1
           AND t.retracted = 0
           AND t.tx < ?2
           AND t.tx = (
               SELECT MAX(tx) FROM triples t2
               WHERE t2.subject = t.subject
                 AND t2.predicate = ?1
                 AND t2.retracted = 0
                 AND t2.tx < ?2
           )"
    )?;
    let triples = stmt
        .query_map(rusqlite::params![predicate, before_tx], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(QueryResult::new(triples))
}

/// One row returned by [`page_as_of`]: the subject IRI and the tx at which its
/// ordering-key triple was written as-of the snapshot.
///
/// `order_tx` is `None` when the subject has no assertion for `order_predicate`
/// at or before `snapshot_tx`; those subjects sort last, broken by `subject` ASC.
#[derive(Debug, Clone)]
pub struct AsOfPageRow {
    pub subject: String,
    /// tx of the ordering-key triple as-of snapshot_tx; `None` when absent.
    pub order_tx: Option<i64>,
}

/// Returns a stable, as-of-snapshot page of subjects whose membership triple
/// (subject, `membership_predicate`, `membership_object`) was active at `snapshot_tx`.
///
/// **Membership as-of**: a subject belongs to the set iff its MAX(tx) ≤ snapshot_tx
/// for that (subject, membership_predicate, membership_object) triple is NOT retracted.
/// A retraction with tx ≤ snapshot_tx removes the subject; a retraction with
/// tx > snapshot_tx is invisible and does NOT remove it.
///
/// **Extra membership filter** (`extra_membership`): when `Some((pred2, obj2))`, the
/// subject must ALSO satisfy a second membership triple `(subject, pred2, obj2)` active
/// as-of snapshot_tx under the same MAX(tx) ≤ snapshot_tx / not-retracted rule. When
/// `None`, no additional filter is applied — SQL is identical to the single-membership
/// path (zero regression).
///
/// **Ordering key as-of**: for each member subject the function resolves the tx of the
/// winning assertion for `order_predicate` at MAX(tx) ≤ snapshot_tx. Tie-breaking by
/// `m.subject ASC` makes the order fully deterministic. Subjects with no assertion for
/// `order_predicate` as-of `snapshot_tx` sort last (NULLS LAST) and are broken by
/// `m.subject ASC`.
///
/// **Keyset pagination**: pass `after = Some((cursor_order_tx, cursor_subject))` where
/// the pair is taken from the last row of the previous page. The cursor is composite —
/// `(order_tx, subject)` — which eliminates two bugs that arise with a scalar cursor:
///
/// * **NULL-tail duplication**: a scalar `order_tx IS NULL` clause re-includes ALL
///   null-keyed subjects on every continuation page. The composite cursor tracks the
///   last delivered subject within the null tail and advances correctly.
/// * **Same-tx gap**: multiple subjects can share the same `order_tx` (batch writes in
///   one transaction). A strict `order_tx < cursor` skips the siblings that fall on the
///   page boundary. The composite cursor uses `(order_tx = cursor AND subject > cursor_subject)`
///   to include them.
///
/// Cursor semantics — "rows strictly AFTER (cursor_tx, cursor_subject)" under the
/// ordering `order_tx DESC NULLS LAST, subject ASC`:
///
/// * When `cursor_tx` is `Some(v)`:
///   `(ok.order_tx < v) OR (ok.order_tx = v AND m.subject > cursor_subject) OR (ok.order_tx IS NULL)`
///
/// * When `cursor_tx` is `None` (already inside the null tail):
///   `(ok.order_tx IS NULL AND m.subject > cursor_subject)`
///
/// Pass `after = None` to fetch from the beginning of the ordered set.
pub fn page_as_of(
    conn: &Connection,
    membership_predicate: &str,
    membership_object: &str,
    snapshot_tx: i64,
    order_predicate: &str,
    limit: i64,
    after: Option<(Option<i64>, &str)>,
    extra_membership: Option<(&str, &str)>,
) -> Result<Vec<AsOfPageRow>> {
    // The query proceeds in up to four logical steps expressed as a single SQL statement:
    //
    // 1. MEMBERS CTE — identify subjects whose primary membership triple was active
    //    as-of snapshot_tx (MAX(tx) ≤ snapshot_tx, winner not retracted).
    //
    // 2. MEMBERS2 CTE (only when extra_membership is Some) — same semantics for the
    //    secondary (predicate, object) pair. INNER JOIN with MEMBERS narrows the set.
    //
    // 3. ORDER_KEYS CTE — resolve the ordering-key tx as-of snapshot_tx for each
    //    remaining member. LEFT JOIN so members without the key are preserved (NULL).
    //
    // 4. Final SELECT — sort DESC by order_tx NULLS LAST, ASC by subject as
    //    deterministic tie-breaker; apply composite keyset filter + LIMIT.
    let (members2_cte, members2_join) = match extra_membership {
        Some((pred2, obj2)) => {
            let cte = format!(
                ",\n        members2 AS (\n\
                             SELECT t.subject\n\
                 FROM triples t\n\
                 WHERE t.predicate = '{pred2}'\n\
                   AND t.object = '{obj2}'\n\
                   AND t.tx <= ?3\n\
                   AND t.tx = (\n\
                       SELECT MAX(t2.tx) FROM triples t2\n\
                       WHERE t2.subject = t.subject\n\
                         AND t2.predicate = '{pred2}'\n\
                         AND t2.object = '{obj2}'\n\
                         AND t2.tx <= ?3\n\
                   )\n\
                   AND t.retracted = 0\n\
                 )"
            );
            let join = "\n        INNER JOIN members2 ON members2.subject = m.subject".to_string();
            (cte, join)
        }
        None => (String::new(), String::new()),
    };

    // Build the SQL for the final query.
    // Parameters are always: ?1=membership_predicate, ?2=membership_object,
    // ?3=snapshot_tx, ?4=order_predicate, then cursor fields (if any), then limit.
    let sql: String = match after {
        None => {
            // No cursor: full set from the top.
            let s = format!(
                "WITH members AS (\n\
                     SELECT t.subject\n\
                     FROM triples t\n\
                     WHERE t.predicate = ?1\n\
                       AND t.object = ?2\n\
                       AND t.tx <= ?3\n\
                       AND t.tx = (\n\
                           SELECT MAX(t2.tx) FROM triples t2\n\
                           WHERE t2.subject = t.subject\n\
                             AND t2.predicate = ?1\n\
                             AND t2.object = ?2\n\
                             AND t2.tx <= ?3\n\
                       )\n\
                       AND t.retracted = 0\n\
                 ){members2_cte},\n\
                 order_keys AS (\n\
                     SELECT t.subject,\n\
                            t.tx AS order_tx\n\
                     FROM triples t\n\
                     WHERE t.predicate = ?4\n\
                       AND t.retracted = 0\n\
                       AND t.tx <= ?3\n\
                       AND t.tx = (\n\
                           SELECT MAX(t2.tx) FROM triples t2\n\
                           WHERE t2.subject = t.subject\n\
                             AND t2.predicate = ?4\n\
                             AND t2.retracted = 0\n\
                             AND t2.tx <= ?3\n\
                       )\n\
                 )\n\
                 SELECT m.subject,\n\
                        ok.order_tx\n\
                 FROM members m{members2_join}\n\
                 LEFT JOIN order_keys ok ON ok.subject = m.subject\n\
                 ORDER BY ok.order_tx DESC NULLS LAST,\n\
                          m.subject ASC\n\
                 LIMIT ?5"
            );
            s
        }

        Some((Some(_cursor_tx), _cursor_subject)) => {
            // Cursor has a non-NULL order_tx: we are still in the keyed region or entering the null tail.
            // Composite predicate:
            //   (ok.order_tx < ?5)
            //   OR (ok.order_tx = ?5 AND m.subject > ?6)
            //   OR (ok.order_tx IS NULL)
            let kc = "AND (\n\
                           ok.order_tx < ?5\n\
                           OR (ok.order_tx = ?5 AND m.subject > ?6)\n\
                           OR ok.order_tx IS NULL\n\
                       )\n\
                       ";
            let s = format!(
                "WITH members AS (\n\
                     SELECT t.subject\n\
                     FROM triples t\n\
                     WHERE t.predicate = ?1\n\
                       AND t.object = ?2\n\
                       AND t.tx <= ?3\n\
                       AND t.tx = (\n\
                           SELECT MAX(t2.tx) FROM triples t2\n\
                           WHERE t2.subject = t.subject\n\
                             AND t2.predicate = ?1\n\
                             AND t2.object = ?2\n\
                             AND t2.tx <= ?3\n\
                       )\n\
                       AND t.retracted = 0\n\
                 ){members2_cte},\n\
                 order_keys AS (\n\
                     SELECT t.subject,\n\
                            t.tx AS order_tx\n\
                     FROM triples t\n\
                     WHERE t.predicate = ?4\n\
                       AND t.retracted = 0\n\
                       AND t.tx <= ?3\n\
                       AND t.tx = (\n\
                           SELECT MAX(t2.tx) FROM triples t2\n\
                           WHERE t2.subject = t.subject\n\
                             AND t2.predicate = ?4\n\
                             AND t2.retracted = 0\n\
                             AND t2.tx <= ?3\n\
                       )\n\
                 )\n\
                 SELECT m.subject,\n\
                        ok.order_tx\n\
                 FROM members m{members2_join}\n\
                 LEFT JOIN order_keys ok ON ok.subject = m.subject\n\
                 WHERE {kc}\
                 ORDER BY ok.order_tx DESC NULLS LAST,\n\
                          m.subject ASC\n\
                 LIMIT ?7"
            );
            s
        }

        Some((None, _cursor_subject)) => {
            // Cursor is inside the null tail: only advance within null-keyed subjects.
            // Composite predicate:
            //   (ok.order_tx IS NULL AND m.subject > ?5)
            let kc = "AND ok.order_tx IS NULL\n\
                       AND m.subject > ?5\n\
                       ";
            let s = format!(
                "WITH members AS (\n\
                     SELECT t.subject\n\
                     FROM triples t\n\
                     WHERE t.predicate = ?1\n\
                       AND t.object = ?2\n\
                       AND t.tx <= ?3\n\
                       AND t.tx = (\n\
                           SELECT MAX(t2.tx) FROM triples t2\n\
                           WHERE t2.subject = t.subject\n\
                             AND t2.predicate = ?1\n\
                             AND t2.object = ?2\n\
                             AND t2.tx <= ?3\n\
                       )\n\
                       AND t.retracted = 0\n\
                 ){members2_cte},\n\
                 order_keys AS (\n\
                     SELECT t.subject,\n\
                            t.tx AS order_tx\n\
                     FROM triples t\n\
                     WHERE t.predicate = ?4\n\
                       AND t.retracted = 0\n\
                       AND t.tx <= ?3\n\
                       AND t.tx = (\n\
                           SELECT MAX(t2.tx) FROM triples t2\n\
                           WHERE t2.subject = t.subject\n\
                             AND t2.predicate = ?4\n\
                             AND t2.retracted = 0\n\
                             AND t2.tx <= ?3\n\
                       )\n\
                 )\n\
                 SELECT m.subject,\n\
                        ok.order_tx\n\
                 FROM members m{members2_join}\n\
                 LEFT JOIN order_keys ok ON ok.subject = m.subject\n\
                 WHERE {kc}\
                 ORDER BY ok.order_tx DESC NULLS LAST,\n\
                          m.subject ASC\n\
                 LIMIT ?6"
            );
            s
        }
    };

    let mut stmt = conn.prepare(&sql)?;

    let map_row = |row: &rusqlite::Row<'_>| {
        Ok(AsOfPageRow {
            subject: row.get(0)?,
            order_tx: row.get(1)?,
        })
    };

    let rows = match after {
        None => stmt
            .query_map(
                rusqlite::params![
                    membership_predicate,
                    membership_object,
                    snapshot_tx,
                    order_predicate,
                    limit
                ],
                map_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?,

        Some((Some(cursor_tx), cursor_subject)) => stmt
            .query_map(
                rusqlite::params![
                    membership_predicate,
                    membership_object,
                    snapshot_tx,
                    order_predicate,
                    cursor_tx,
                    cursor_subject,
                    limit
                ],
                map_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?,

        Some((None, cursor_subject)) => stmt
            .query_map(
                rusqlite::params![
                    membership_predicate,
                    membership_object,
                    snapshot_tx,
                    order_predicate,
                    cursor_subject,
                    limit
                ],
                map_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };

    Ok(rows)
}

pub fn get_by_object_iri(conn: &Connection, object_iri: &str) -> Result<QueryResult> {
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         WHERE t.object = ? AND t.object_type = 'iri' AND t.retracted = 0
         {}",
        AND_IS_CURRENT
    );
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map([object_iri], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

pub fn get_by_predicate(conn: &Connection, predicate: &str) -> Result<QueryResult> {
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         WHERE t.predicate = ? AND t.retracted = 0
         {}
         ORDER BY t.tx DESC",
        AND_IS_CURRENT
    );
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map([predicate], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

pub fn get_by_entity_predicate(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<QueryResult> {
    get_by_entity_predicate_internal(conn, entity, predicate, true)
}

pub fn get_by_entity_predicate_internal(
    conn: &Connection,
    entity: &str,
    predicate: &str,
    check_functional: bool,
) -> Result<QueryResult> {

    let is_functional = if check_functional {
        crate::owl::Property::is_functional(conn, predicate)
            .unwrap_or(false)
    } else {
        false
    };

    if is_functional {
        let mut stmt = conn.prepare(
            "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_type, object_number, object_integer, object_boolean,
                    tx, origin_id, retracted, created_at
             FROM triples
             WHERE subject = ? AND predicate = ?
             ORDER BY tx DESC
             LIMIT 1"
        )?;

        let triples: Vec<Triple> = stmt
            .query_map([entity, predicate], row_to_triple)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let active: Vec<Triple> = triples.into_iter().filter(|t| !t.retracted).collect();
        Ok(QueryResult::new(active))
    } else {
        let mut stmt = conn.prepare(
            "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_type, object_number, object_integer, object_boolean,
                    tx, origin_id, 0 AS retracted, created_at
             FROM triples_current
             WHERE subject = ? AND predicate = ?"
        )?;

        let triples: Vec<Triple> = stmt
            .query_map([entity, predicate], row_to_triple)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(QueryResult::new(triples))
    }
}

pub fn get_by_predicate_object(
    conn: &Connection,
    predicate: &str,
    object: &str,
) -> Result<QueryResult> {
    let (where_clause, params): (&str, Vec<&dyn rusqlite::ToSql>) = if object == "true" {
        (
            "WHERE t.predicate = ?1 AND t.object_boolean = 1 AND t.retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql],
        )
    } else if object == "false" {
        (
            "WHERE t.predicate = ?1 AND t.object_boolean = 0 AND t.retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql],
        )
    } else {
        (
            "WHERE t.predicate = ?1 AND t.object = ?2 AND t.retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql, &object as &dyn rusqlite::ToSql],
        )
    };

    let query = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         {}
         {}
         ORDER BY t.tx DESC",
        where_clause, AND_IS_CURRENT
    );

    let mut stmt = conn.prepare(&query)?;
    let triples = stmt
        .query_map(params.as_slice(), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

#[derive(Debug, Clone)]
pub struct BacklinkRow {
    pub subject: String,
    pub predicate: String,
    pub source_class: Option<String>,
    pub group_total: usize,
    /// MAX(tx) of the backlink triple for this subject+predicate — same key
    /// `get_backlinks_page` orders by, so snapshot and paginated pages share
    /// the same cursor basis.
    pub last_tx: i64,
}

/// Returns up to `limit_per_group` backlinks per (predicate × source_class) group, ordered by
/// `last_tx DESC, subject ASC` — the same total order that `get_backlinks_page` uses so the
/// composite cursor derived from page-1 by the command layer is guaranteed to be contiguous with
/// subsequent paginated pages (no gap, no duplicate at the boundary).
///
/// **is_current = as-of snapshot_tx**: this function is always called inside the same
/// `executor.read` closure that computes `MAX(tx) = snapshot_tx`, so the WAL snapshot is
/// identical for both reads. Therefore `is_current = 1` is equivalent to `tx <= snapshot_tx`
/// on `triples_current` — no additional as-of filtering is needed.
pub fn get_backlinks_grouped_limited(
    conn: &Connection,
    object: &str,
    limit_per_group: usize,
) -> Result<Vec<BacklinkRow>> {
    let sql = format!(
        "WITH
         backlinks_raw AS (
             SELECT t.subject, t.predicate, MAX(t.tx) AS last_tx
             FROM triples t
             WHERE t.object = ?1
               AND t.object_type = 'iri'
               AND t.retracted = 0
               AND t.predicate != 'rdf:type'
               AND t.subject != ?1
               AND t.is_current = 1
             GROUP BY t.subject, t.predicate
         ),
         subject_class AS (
             SELECT br.subject, MIN(tc.object) AS source_class
             FROM backlinks_raw br
             LEFT JOIN triples_current tc
               ON tc.subject = br.subject
              AND tc.predicate = 'rdf:type'
              AND tc.object_type = 'iri'
             GROUP BY br.subject
         ),
         backlinks_with_class AS (
             SELECT
                 br.subject,
                 br.predicate,
                 br.last_tx,
                 sc.source_class
             FROM backlinks_raw br
             LEFT JOIN subject_class sc ON sc.subject = br.subject
         ),
         group_counts AS (
             SELECT predicate, source_class, COUNT(*) AS total
             FROM backlinks_with_class
             GROUP BY predicate, source_class
         ),
         ranked AS (
             SELECT
                 bwc.subject, bwc.predicate, bwc.source_class, bwc.last_tx,
                 gc.total AS group_total,
                 ROW_NUMBER() OVER (
                     PARTITION BY bwc.predicate, bwc.source_class
                     ORDER BY bwc.last_tx DESC, bwc.subject ASC
                 ) AS rn
             FROM backlinks_with_class bwc
             JOIN group_counts gc
               ON gc.predicate = bwc.predicate
              AND gc.source_class IS bwc.source_class
         )
         SELECT subject, predicate, source_class, group_total, last_tx
         FROM ranked
         WHERE rn <= {}
         ORDER BY last_tx DESC, subject ASC",
        limit_per_group
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([object], |row| {
            let subject: String = row.get(0)?;
            let predicate: String = row.get(1)?;
            let source_class: Option<String> = row.get(2)?;
            let group_total: i64 = row.get(3)?;
            let last_tx: i64 = row.get(4)?;
            Ok(BacklinkRow {
                subject,
                predicate,
                source_class,
                group_total: group_total as usize,
                last_tx,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Keyset-paginated backlinks page ordered by `last_tx DESC, subject ASC`, as-of `snapshot_tx`.
///
/// **As-of semantics**: a subject is a backlink iff its latest assertion of
/// (subject, `predicate`, `object`) with tx ≤ snapshot_tx is NOT retracted.
/// This matches page_as_of's membership model so the page-1 snapshot and the
/// "load more" calls share the same frozen set.
///
/// `source_class` is resolved as-of snapshot_tx using the same MAX(tx) rule.
///
/// **Composite cursor**: the caller supplies `after_cursor: Option<(i64, String)>` —
/// `(last_tx, subject)` of the last item returned on the previous page. The predicate
/// `(br.last_tx < cursor_tx) OR (br.last_tx = cursor_tx AND br.subject > cursor_subject)`
/// advances past items that share the same `last_tx`, eliminating the gap that a scalar
/// `last_tx` cursor produces when multiple backlinks land on the same transaction.
///
/// Returns `(subject_iri, last_tx)` pairs so the command layer can derive `next_cursor`
/// without a second query.
pub fn get_backlinks_page(
    conn: &Connection,
    object: &str,
    predicate: &str,
    source_class: Option<&str>,
    after_cursor: Option<(i64, String)>,
    limit: usize,
    snapshot_tx: i64,
) -> Result<Vec<(String, i64)>> {
    let base_ctes = format!("
        WITH
        backlinks_raw AS (
            SELECT t.subject, t.tx AS last_tx
            FROM triples t
            WHERE t.object = ?1
              AND t.object_type = 'iri'
              AND t.predicate = ?2
              AND t.subject != ?1
              AND t.tx <= {snapshot_tx}
              AND t.tx = (
                  SELECT MAX(t2.tx) FROM triples t2
                  WHERE t2.subject = t.subject
                    AND t2.predicate = ?2
                    AND t2.object = ?1
                    AND t2.object_type = 'iri'
                    AND t2.tx <= {snapshot_tx}
              )
              AND t.retracted = 0
        ),
        subject_class AS (
            SELECT br.subject, MIN(tc.object) AS source_class
            FROM backlinks_raw br
            LEFT JOIN triples tc
              ON tc.subject = br.subject
             AND tc.predicate = 'rdf:type'
             AND tc.object_type = 'iri'
             AND tc.tx <= {snapshot_tx}
             AND tc.tx = (
                 SELECT MAX(t3.tx) FROM triples t3
                 WHERE t3.subject = tc.subject
                   AND t3.predicate = 'rdf:type'
                   AND t3.object_type = 'iri'
                   AND t3.tx <= {snapshot_tx}
                   AND t3.retracted = 0
             )
             AND tc.retracted = 0
            GROUP BY br.subject
        )
        SELECT br.subject, br.last_tx
        FROM backlinks_raw br
        LEFT JOIN subject_class sc ON sc.subject = br.subject
        WHERE sc.source_class IS ?3
    ");
    let rows: Vec<(String, i64)> = if let Some((cursor_tx, ref cursor_subject)) = after_cursor {
        let sql = format!("{base_ctes}
          AND (br.last_tx < ?4 OR (br.last_tx = ?4 AND br.subject > ?5))
          ORDER BY br.last_tx DESC, br.subject ASC
          LIMIT ?6");
        let mut stmt = conn.prepare(&sql)?;
        let x = stmt.query_map(
            rusqlite::params![object, predicate, source_class, cursor_tx, cursor_subject, limit as i64],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        x
    } else {
        let sql = format!("{base_ctes}
          ORDER BY br.last_tx DESC, br.subject ASC
          LIMIT ?4");
        let mut stmt = conn.prepare(&sql)?;
        let x = stmt.query_map(
            rusqlite::params![object, predicate, source_class, limit as i64],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        x
    };
    Ok(rows)
}

pub fn get_predicates_for_subjects(
    conn: &Connection,
    subjects: &[String],
    predicates: &[&str],
) -> Result<Vec<(String, String, Object)>> {
    if subjects.is_empty() || predicates.is_empty() {
        return Ok(Vec::new());
    }
    let subject_phs = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let predicate_phs = predicates.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         WHERE t.subject IN ({}) AND t.predicate IN ({}) AND t.retracted = 0
         {}
         ORDER BY t.subject, t.predicate, t.tx DESC",
        subject_phs, predicate_phs, AND_IS_CURRENT
    );
    let mut params: Vec<SqlValue> = subjects.iter()
        .map(|s| SqlValue::Text(s.clone()))
        .collect();
    params.extend(predicates.iter().map(|p| SqlValue::Text(p.to_string())));
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(triples.into_iter().map(|t| (t.subject, t.predicate, t.object)).collect())
}

pub fn get_first_iri_property_batch(
    conn: &Connection,
    subjects: &[String],
    predicate: &str,
) -> Result<std::collections::HashMap<String, String>> {
    if subjects.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, object FROM triples t
         WHERE t.subject IN ({}) AND t.predicate = ? AND t.object_type = 'iri'
           AND t.retracted = 0
           {}
         ORDER BY t.subject, t.tx DESC",
        placeholders, AND_IS_CURRENT
    );
    let mut params: Vec<SqlValue> = subjects.iter()
        .map(|s| SqlValue::Text(s.clone()))
        .collect();
    params.push(SqlValue::Text(predicate.to_string()));
    let mut stmt = conn.prepare(&sql)?;
    let mut map = std::collections::HashMap::new();
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        let subject: String = row.get(0)?;
        let object: String = row.get(1)?;
        Ok((subject, object))
    })?;
    for row in rows {
        let (subject, object) = row?;
        map.entry(subject).or_insert(object);
    }
    Ok(map)
}

#[allow(dead_code)]
pub fn get_at_time(conn: &Connection, entity: &str, tx: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND tx <= ? AND retracted = 0
         ORDER BY predicate, tx DESC"
    )?;

    let triples = stmt
        .query_map([entity, tx.to_string().as_str()], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut seen_predicates = std::collections::HashSet::new();
    let snapshot: Vec<Triple> = triples
        .into_iter()
        .filter(|t| seen_predicates.insert(t.predicate.clone()))
        .collect();

    Ok(QueryResult::new(snapshot))
}

#[allow(dead_code)]
pub fn get_by_origin(conn: &Connection, origin_id: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE origin_id = ? AND retracted = 0
         ORDER BY tx DESC"
    )?;

    let triples = stmt
        .query_map([origin_id], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

#[allow(dead_code)]
pub fn get_history(conn: &Connection, entity: &str) -> Result<Vec<(i64, Vec<Triple>)>> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ?
         ORDER BY tx ASC"
    )?;

    let all_triples: Vec<Triple> = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut history: std::collections::HashMap<i64, Vec<Triple>> = std::collections::HashMap::new();
    for triple in all_triples {
        history.entry(triple.tx).or_insert_with(Vec::new).push(triple);
    }

    let mut result: Vec<(i64, Vec<Triple>)> = history.into_iter().collect();
    result.sort_by_key(|(tx, _)| *tx);

    Ok(result)
}

/// For each (subject, predicate) pair in `pairs`, returns the MAX(tx) of the triple
/// where that subject→predicate points to `object`. This is the `last_tx` of the
/// backlink triple — the same key `get_backlinks_page` orders by, ensuring the
/// snapshot sort and page-2+ sort use the same cursor basis.
pub fn get_backlink_last_tx_batch(
    conn: &Connection,
    object: &str,
    pairs: &[(String, String)],
) -> Result<std::collections::HashMap<(String, String), i64>> {
    if pairs.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = pairs.iter().map(|_| "(?,?)").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, predicate, MAX(tx) \
         FROM triples \
         WHERE retracted = 0 \
           AND object = ? \
           AND object_type = 'iri' \
           AND (subject, predicate) IN ({}) \
         GROUP BY subject, predicate",
        placeholders
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(1 + pairs.len() * 2);
    params.push(Box::new(object.to_string()));
    for (subj, pred) in pairs {
        params.push(Box::new(subj.clone()));
        params.push(Box::new(pred.clone()));
    }
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let result = stmt
        .query_map(param_refs.as_slice(), |row| {
            let subject: String = row.get(0)?;
            let predicate: String = row.get(1)?;
            let max_tx: i64 = row.get(2)?;
            Ok(((subject, predicate), max_tx))
        })?
        .collect::<std::result::Result<std::collections::HashMap<_, _>, _>>()?;
    Ok(result)
}

pub fn batch_load_triples_for_subjects(
    conn: &Connection,
    subjects: &[String],
) -> Result<std::collections::HashMap<String, Vec<Triple>>> {
    if subjects.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    // Window function (single pass over the filtered rows) instead of a correlated subquery
    // per row — avoids N re-executions of the inner MAX query when batching many subjects.
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM (
             SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_type, object_number, object_integer, object_boolean,
                    tx, origin_id, retracted, created_at,
                    MAX(tx) OVER (PARTITION BY subject, predicate) AS max_tx
             FROM triples
             WHERE subject IN ({}) AND retracted = 0
         )
         WHERE tx = max_tx
         ORDER BY subject, predicate, tx DESC",
        placeholders
    );
    let params: Vec<SqlValue> = subjects.iter().map(|s| SqlValue::Text(s.clone())).collect();
    let t0 = std::time::Instant::now();
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut map: std::collections::HashMap<String, Vec<Triple>> = std::collections::HashMap::new();
    for triple in triples {
        map.entry(triple.subject.clone()).or_default().push(triple);
    }
    let elapsed = t0.elapsed().as_millis();
    if elapsed > 10 || subjects.len() > 50 {
        crate::diagnostics::log_backend("debug", &format!(
            "[EAVTO] batch_load({} subjects) → {} triples {}ms",
            subjects.len(), map.len(), elapsed
        ));
    }
    Ok(map)
}

pub fn batch_load_retracted_triples_for_subjects(
    conn: &Connection,
    subjects: &[String],
) -> Result<std::collections::HashMap<String, Vec<Triple>>> {
    if subjects.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject IN ({}) AND retracted = 1
         ORDER BY subject, predicate, tx DESC",
        placeholders
    );
    let params: Vec<SqlValue> = subjects.iter().map(|s| SqlValue::Text(s.clone())).collect();
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut map: std::collections::HashMap<String, Vec<Triple>> = std::collections::HashMap::new();
    for triple in triples {
        map.entry(triple.subject.clone()).or_default().push(triple);
    }
    Ok(map)
}

pub(crate) fn row_to_triple(row: &Row) -> rusqlite::Result<Triple> {
    let subject: String = row.get(0)?;
    let predicate: String = row.get(1)?;
    let object_opt: Option<String> = row.get(2)?;
    let object_value: Option<String> = row.get(3)?;
    let object_datatype: Option<String> = row.get(4)?;
    let object_language: Option<String> = row.get(5)?;
    let object_type: String = row.get(6)?;
    let object_number: Option<f64> = row.get(7)?;
    let object_integer: Option<i64> = row.get(8)?;
    let object_boolean: Option<i64> = row.get(9)?;
    let tx: i64 = row.get(10)?;
    let origin_id: i64 = row.get(11)?;
    let retracted: i64 = row.get(12)?;
    let created_at: i64 = row.get(13)?;

    let object = match object_type.as_str() {
        "iri" => Object::Iri(object_opt.ok_or(rusqlite::Error::InvalidQuery)?),
        "blank" => Object::Blank(object_opt.ok_or(rusqlite::Error::InvalidQuery)?),
        "literal" => {
            if let Some(int) = object_integer {
                Object::Integer(int)
            } else if let Some(num) = object_number {
                Object::Number(num)
            } else if object_datatype.as_deref() == Some("xsd:dateTime") {
                Object::DateTime(object_value.ok_or(rusqlite::Error::InvalidQuery)?)
            } else if object_datatype.as_deref() == Some("xsd:date") {
                Object::Literal {
                    value: object_value.ok_or(rusqlite::Error::InvalidQuery)?,
                    datatype: object_datatype,
                    language: object_language,
                }
            } else if let Some(bool_val) = object_boolean {
                Object::Boolean(bool_val != 0)
            } else {
                Object::Literal {
                    value: object_value.ok_or(rusqlite::Error::InvalidQuery)?,
                    datatype: object_datatype,
                    language: object_language,
                }
            }
        }
        _ => unreachable!("Invalid object_type in database"),
    };

    Ok(Triple {
        subject,
        predicate,
        object,
        tx,
        origin_id,
        retracted: retracted != 0,
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::{setup_test_db, create_test_triples};
    use crate::eavto::store::assert_triples;

    fn setup_test_data(conn: &mut Connection) -> i64 {
        let triples = create_test_triples();
        assert_triples(conn, &triples, "test").unwrap()
    }

    #[test]
    fn test_get_by_entity() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity(&conn, "foundation:TestClass").unwrap();
        assert_eq!(result.triples.len(), 2);
    }

    #[test]
    fn test_get_by_entity_nonexistent() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity(&conn, "foundation:NonExistent").unwrap();
        assert_eq!(result.triples.len(), 0);
    }

    #[test]
    fn test_get_by_predicate() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_predicate(&conn, "rdf:type").unwrap();
        assert_eq!(result.triples.len(), 1);
    }

    #[test]
    fn test_get_by_entity_predicate() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity_predicate(&conn, "foundation:TestClass", "rdfs:label").unwrap();
        assert_eq!(result.triples.len(), 1);

        let triple = &result.triples[0];
        match &triple.object {
            Object::Literal { value, .. } => assert_eq!(value, "Test Class"),
            _ => panic!("Expected literal object"),
        }
    }

    #[test]
    fn test_get_at_time() {
        let mut conn = setup_test_db();
        let tx_id = setup_test_data(&mut conn);

        let result = get_at_time(&conn, "foundation:TestClass", tx_id).unwrap();
        assert_eq!(result.triples.len(), 2);
    }

    #[test]
    fn test_get_at_time_temporal_snapshot() {
        let mut conn = setup_test_db();
        let _tx1 = setup_test_data(&mut conn);

        let updated_triple = vec![Triple {
            subject: "foundation:TestClass".to_string(),
            predicate: "rdfs:label".to_string(),
            object: Object::Literal {
                value: "Updated Label".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            tx: 0,
            created_at: 2000,
            origin_id: 1,
            retracted: false,
        }];
        let tx2 = assert_triples(&mut conn, &updated_triple, "test").unwrap();

        let result = get_at_time(&conn, "foundation:TestClass", tx2).unwrap();

        assert_eq!(result.triples.len(), 2);

        let label_triple = result.triples.iter()
            .find(|t| t.predicate == "rdfs:label")
            .expect("Should have label");

        match &label_triple.object {
            Object::Literal { value, .. } => assert_eq!(value, "Updated Label"),
            _ => panic!("Expected literal"),
        }
    }

    #[test]
    fn test_get_by_origin() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_origin(&conn, 1).unwrap();
        assert!(result.triples.len() > 0);
    }

    #[test]
    fn test_get_history() {
        let mut conn = setup_test_db();
        let tx1 = setup_test_data(&mut conn);

        let new_triple = vec![Triple {
            subject: "foundation:TestClass".to_string(),
            predicate: "rdfs:comment".to_string(),
            object: Object::Literal {
                value: "A comment".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            tx: 0,
            created_at: 2000,
            origin_id: 1,
            retracted: false,
        }];
        let tx2 = assert_triples(&mut conn, &new_triple, "test").unwrap();

        let history = get_history(&conn, "foundation:TestClass").unwrap();

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].0, tx1);
        assert_eq!(history[1].0, tx2);
        assert_eq!(history[0].1.len(), 2);
        assert_eq!(history[1].1.len(), 1);
    }

    #[test]
    fn test_row_to_triple_with_iri() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity(&conn, "foundation:TestClass").unwrap();
        let iri_triple = result.triples.iter()
            .find(|t| t.predicate == "rdf:type")
            .expect("Should have rdf:type");

        match &iri_triple.object {
            Object::Iri(iri) => assert_eq!(iri, "owl:Class"),
            _ => panic!("Expected IRI object"),
        }
    }

    #[test]
    fn test_row_to_triple_with_integer() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity(&conn, "foundation:TestProperty").unwrap();
        let int_triple = result.triples.iter()
            .find(|t| t.predicate == "foundation:someValue")
            .expect("Should have foundation:someValue");

        match &int_triple.object {
            Object::Integer(i) => assert_eq!(*i, 42),
            _ => panic!("Expected Integer object"),
        }
    }

    #[test]
    fn test_find_by_class_iris_and_properties_returns_subclass_instances() {
        let mut conn = setup_test_db();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Animal".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("owl:Class".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Dog".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("owl:Class".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:Dog".to_string(), predicate: "rdfs:subClassOf".to_string(),
                object: Object::Iri("foundation:Animal".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Rex".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Dog".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:Rex".to_string(), predicate: "foundation:name".to_string(),
                object: Object::Literal { value: "Rex".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn,
            &["foundation:Animal", "foundation:Dog"],
            &[PropertyFilter::Compare("foundation:name", "Rex", "=")],
            false,
            100,
            0,
            None,
        ).unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"foundation:Rex".to_string()));
    }

    #[test]
    fn test_find_by_class_iris_single_class_filters_by_property() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn,
            &["owl:Class"],
            &[PropertyFilter::Compare("rdfs:label", "Test Class", "=")],
            false,
            100,
            0,
            None,
        ).unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"foundation:TestClass".to_string()));
    }

    #[test]
    fn test_find_by_properties_without_class_constraint() {
        let mut conn = setup_test_db();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:PersonA".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Person".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:PersonA".to_string(), predicate: "foundation:status".to_string(),
                object: Object::Literal { value: "active".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:CompanyX".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Company".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:CompanyX".to_string(), predicate: "foundation:status".to_string(),
                object: Object::Literal { value: "active".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:PersonB".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Person".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:PersonB".to_string(), predicate: "foundation:status".to_string(),
                object: Object::Literal { value: "inactive".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        let (results, total) = find_by_properties_with_options(
            &conn,
            &[PropertyFilter::Compare("foundation:status", "active", "=")],
            false,
            100,
            0,
        ).unwrap();

        assert_eq!(total, 2, "should find both active entities regardless of class");
        assert!(results.contains(&"foundation:PersonA".to_string()));
        assert!(results.contains(&"foundation:CompanyX".to_string()));
        assert!(!results.contains(&"foundation:PersonB".to_string()));
    }
}

#[cfg(test)]
mod page_as_of_tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    // Helpers that bypass assert_triples so we control exact tx values.

    fn insert_tx(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_iri(conn: &Connection, subject: &str, predicate: &str, object: &str, tx: i64) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, tx, origin_id, retracted, created_at, is_current) \
             VALUES (?1, ?2, ?3, 'iri', ?4, 1, 0, 0, 1)",
            rusqlite::params![subject, predicate, object, tx],
        )
        .unwrap();
    }

    fn insert_literal(conn: &Connection, subject: &str, predicate: &str, value: &str, tx: i64) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object_value, object_type, tx, origin_id, retracted, created_at, is_current) \
             VALUES (?1, ?2, ?3, 'literal', ?4, 1, 0, 0, 1)",
            rusqlite::params![subject, predicate, value, tx],
        )
        .unwrap();
    }

    fn retract(conn: &Connection, subject: &str, predicate: &str, object: &str, tx: i64) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, tx, origin_id, retracted, created_at, is_current) \
             VALUES (?1, ?2, ?3, 'iri', ?4, 1, 1, 0, 0)",
            rusqlite::params![subject, predicate, object, tx],
        )
        .unwrap();
    }

    // AC1 — Snapshot estável sob escrita posterior
    #[test]
    fn snapshot_stable_under_later_writes() {
        let conn = setup_test_db();

        let t1 = insert_tx(&conn);
        // Initial membership: A, B, C
        insert_iri(&conn, "ex:A", "ex:member", "ex:Set", t1);
        insert_iri(&conn, "ex:B", "ex:member", "ex:Set", t1);
        insert_iri(&conn, "ex:C", "ex:member", "ex:Set", t1);
        // Order keys at T1 — all share the same tx (t1), tie-broken by subject ASC
        insert_literal(&conn, "ex:A", "ex:rank", "10", t1);
        insert_literal(&conn, "ex:B", "ex:rank", "20", t1);
        insert_literal(&conn, "ex:C", "ex:rank", "30", t1);

        let snapshot_tx = t1;

        // Later writes — must not affect the snapshot result
        let t2 = insert_tx(&conn);
        // (a) new member after snapshot
        insert_iri(&conn, "ex:D", "ex:member", "ex:Set", t2);
        // (b) update ordering key of existing member after snapshot (order_tx becomes t2)
        insert_literal(&conn, "ex:A", "ex:rank", "99", t2);
        // (c) retract a member after snapshot
        retract(&conn, "ex:C", "ex:member", "ex:Set", t2);

        let page = page_as_of(&conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 100, None, None).unwrap();
        let subjects: Vec<&str> = page.iter().map(|r| r.subject.as_str()).collect();

        // Snapshot must return exactly A, B, C (D absent, C present, A uses rank tx=t1 not t2)
        assert_eq!(subjects.len(), 3, "snapshot must have exactly 3 members");
        assert!(subjects.contains(&"ex:A"), "ex:A must be present");
        assert!(subjects.contains(&"ex:B"), "ex:B must be present");
        assert!(subjects.contains(&"ex:C"), "ex:C must be present");
        assert!(!subjects.contains(&"ex:D"), "ex:D added after snapshot must be absent");

        // All three order keys were written at t1 → same order_tx; tie-break by subject ASC
        assert!(page.iter().all(|r| r.order_tx == Some(t1)), "all order_tx must be t1 as-of snapshot");
        assert_eq!(page[0].subject, "ex:A");
        assert_eq!(page[1].subject, "ex:B");
        assert_eq!(page[2].subject, "ex:C");
    }

    // AC2 — Retração as-of: presente antes, ausente depois
    #[test]
    fn retraction_as_of_present_before_retracted_after() {
        let conn = setup_test_db();

        let t1 = insert_tx(&conn);
        insert_iri(&conn, "ex:M", "ex:member", "ex:Set", t1);
        insert_literal(&conn, "ex:M", "ex:rank", "5", t1);

        let t_retract = insert_tx(&conn);
        retract(&conn, "ex:M", "ex:member", "ex:Set", t_retract);

        // snapshot < retraction tx → member present
        let before = page_as_of(&conn, "ex:member", "ex:Set", t1, "ex:rank", 100, None, None).unwrap();
        assert_eq!(before.len(), 1, "member must be present before retraction");
        assert_eq!(before[0].subject, "ex:M");

        // snapshot >= retraction tx → member absent
        let after = page_as_of(&conn, "ex:member", "ex:Set", t_retract, "ex:rank", 100, None, None).unwrap();
        assert_eq!(after.len(), 0, "member must be absent at or after retraction tx");
    }

    // AC3 — Chave de ordenação as-of vs atual: order_tx deve ser o tx da assertiva vencedora ≤ snapshot
    #[test]
    fn order_key_uses_as_of_tx_not_current() {
        let conn = setup_test_db();

        let t1 = insert_tx(&conn);
        insert_iri(&conn, "ex:X", "ex:member", "ex:Set", t1);
        insert_literal(&conn, "ex:X", "ex:rank", "alpha", t1);

        let snapshot_tx = t1;

        let t2 = insert_tx(&conn);
        insert_literal(&conn, "ex:X", "ex:rank", "zeta", t2); // updated after snapshot

        let page = page_as_of(&conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 100, None, None).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(
            page[0].order_tx,
            Some(t1),
            "order_tx must reflect the t1 assertion, not the post-snapshot t2 update"
        );
    }

    // AC4 — Subjects sem chave de ordenação ficam por último, desempate por subject ASC
    #[test]
    fn missing_order_key_sorts_last_deterministic() {
        let conn = setup_test_db();

        let t1 = insert_tx(&conn);
        insert_iri(&conn, "ex:HasKey", "ex:member", "ex:Set", t1);
        insert_iri(&conn, "ex:NoKey1", "ex:member", "ex:Set", t1);
        insert_iri(&conn, "ex:NoKey2", "ex:member", "ex:Set", t1);
        insert_literal(&conn, "ex:HasKey", "ex:rank", "beta", t1);

        let page = page_as_of(&conn, "ex:member", "ex:Set", t1, "ex:rank", 100, None, None).unwrap();
        assert_eq!(page.len(), 3);
        assert_eq!(page[0].subject, "ex:HasKey", "keyed subject must come first");
        assert_eq!(page[0].order_tx, Some(t1), "keyed subject must have order_tx");
        // No-key subjects last, ordered by subject ASC
        assert!(page[1].order_tx.is_none());
        assert!(page[2].order_tx.is_none());
        assert!(page[1].subject < page[2].subject, "tie-break must be subject ASC");
    }

    // AC5 — Duplo filtro de membership: sujeito deve satisfazer AMBOS os pares as-of
    #[test]
    fn extra_membership_filter_requires_both_predicates() {
        let conn = setup_test_db();

        let t1 = insert_tx(&conn);
        // ex:Both satisfies primary (ex:type = ex:Conv) AND secondary (ex:participant = ex:User)
        insert_iri(&conn, "ex:Both",    "ex:type",        "ex:Conv", t1);
        insert_iri(&conn, "ex:Both",    "ex:participant", "ex:User", t1);
        insert_literal(&conn, "ex:Both", "ex:rank", "10", t1);

        // ex:OnlyType satisfies primary but NOT secondary
        insert_iri(&conn, "ex:OnlyType", "ex:type", "ex:Conv", t1);
        insert_literal(&conn, "ex:OnlyType", "ex:rank", "20", t1);

        // ex:OnlyPart satisfies secondary but NOT primary
        insert_iri(&conn, "ex:OnlyPart", "ex:participant", "ex:User", t1);
        insert_literal(&conn, "ex:OnlyPart", "ex:rank", "30", t1);

        let snapshot_tx = t1;

        // Verify: with extra filter, only ex:Both appears
        let page = page_as_of(
            &conn,
            "ex:type", "ex:Conv",
            snapshot_tx,
            "ex:rank",
            100, None,
            Some(("ex:participant", "ex:User")),
        ).unwrap();

        assert_eq!(page.len(), 1, "only the subject satisfying both predicates must appear");
        assert_eq!(page[0].subject, "ex:Both");

        // Verify without extra filter: both ex:Both and ex:OnlyType appear
        let page_no_extra = page_as_of(
            &conn,
            "ex:type", "ex:Conv",
            snapshot_tx,
            "ex:rank",
            100, None,
            None,
        ).unwrap();
        assert_eq!(page_no_extra.len(), 2, "without extra filter both type members appear");

        // Verify as-of semantics of the secondary filter:
        // retract ex:Both's participant at t2 — at snapshot t1 it was present, at t2 it is absent.
        let t2 = insert_tx(&conn);
        retract(&conn, "ex:Both", "ex:participant", "ex:User", t2);

        let page_after_retract = page_as_of(
            &conn,
            "ex:type", "ex:Conv",
            t2,
            "ex:rank",
            100, None,
            Some(("ex:participant", "ex:User")),
        ).unwrap();
        assert_eq!(
            page_after_retract.len(), 0,
            "after secondary membership retraction the subject must not appear"
        );

        // but at snapshot_tx (t1) it is still present
        let page_at_snapshot = page_as_of(
            &conn,
            "ex:type", "ex:Conv",
            snapshot_tx,
            "ex:rank",
            100, None,
            Some(("ex:participant", "ex:User")),
        ).unwrap();
        assert_eq!(
            page_at_snapshot.len(), 1,
            "at the earlier snapshot the secondary membership was active"
        );
    }

    // AC6 — Paginação keyset estável: sem duplicata, sem buraco (subjects com order_tx distintos)
    #[test]
    fn keyset_pagination_no_duplicate_no_gap() {
        let conn = setup_test_db();

        // Insert 5 subjects each with a distinct tx for their order key so that
        // keyset cursoring on order_tx is unambiguous.
        let mut tkeys: Vec<(String, i64)> = Vec::new();
        for i in 0..5u8 {
            let s = format!("ex:S{}", i);
            let tm = insert_tx(&conn);
            insert_iri(&conn, &s, "ex:member", "ex:Set", tm);
            let tk = insert_tx(&conn);
            insert_literal(&conn, &s, "ex:rank", &i.to_string(), tk);
            tkeys.push((s, tk));
        }
        let snapshot_tx = tkeys.last().unwrap().1;

        // Page 0: first 2 rows
        let page0 = page_as_of(&conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, None, None).unwrap();
        assert_eq!(page0.len(), 2);
        let last0 = page0.last().unwrap();
        let cursor0: (Option<i64>, &str) = (last0.order_tx, &last0.subject);

        // Page 1: next 2 rows using composite keyset cursor
        let page1 = page_as_of(&conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, Some(cursor0), None).unwrap();
        assert_eq!(page1.len(), 2);
        let last1 = page1.last().unwrap();
        let cursor1: (Option<i64>, &str) = (last1.order_tx, &last1.subject);

        // Page 2: last row
        let page2 = page_as_of(&conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, Some(cursor1), None).unwrap();
        assert_eq!(page2.len(), 1);

        let combined: Vec<String> = page0.iter()
            .chain(page1.iter())
            .chain(page2.iter())
            .map(|r| r.subject.clone())
            .collect();
        assert_eq!(combined.len(), 5, "all 5 subjects across 3 pages");

        let mut unique = combined.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "no duplicates across pages");

        // Verify strict descending order by order_tx across all pages
        let all_txs: Vec<i64> = page0.iter()
            .chain(page1.iter())
            .chain(page2.iter())
            .map(|r| r.order_tx.unwrap())
            .collect();
        for w in all_txs.windows(2) {
            assert!(w[0] > w[1], "order_tx must be strictly descending across pages");
        }
    }

    // AC7 — Snapshot congela membership: item inserido após snapshot_tx não aparece na continuação
    #[test]
    fn keyset_snapshot_stable_new_item_excluded() {
        let conn = setup_test_db();

        // Insert 4 subjects before snapshot
        let mut last_tk = 0i64;
        for i in 0..4u8 {
            let s = format!("ex:P{}", i);
            let tm = insert_tx(&conn);
            insert_iri(&conn, &s, "ex:member", "ex:Set", tm);
            let tk = insert_tx(&conn);
            insert_literal(&conn, &s, "ex:rank", &i.to_string(), tk);
            last_tk = tk;
        }
        let snapshot_tx = last_tk;

        // Fetch first page (2 rows)
        let page0 = page_as_of(&conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, None, None).unwrap();
        assert_eq!(page0.len(), 2);
        let last = page0.last().unwrap();
        let cursor = (last.order_tx, last.subject.as_str());

        // Insert a new subject AFTER snapshot with a very high tx → must NOT appear in continuation
        let t_new = insert_tx(&conn);
        insert_iri(&conn, "ex:NEW", "ex:member", "ex:Set", t_new);
        insert_literal(&conn, "ex:NEW", "ex:rank", "999", t_new);

        // Fetch second page with the same snapshot_tx
        let page1 = page_as_of(&conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, Some(cursor), None).unwrap();

        let all_subjects: Vec<&str> = page0.iter()
            .chain(page1.iter())
            .map(|r| r.subject.as_str())
            .collect();
        assert!(!all_subjects.contains(&"ex:NEW"), "post-snapshot subject must not appear in any page");
        assert_eq!(all_subjects.len(), 4, "exactly the 4 pre-snapshot subjects");

        let mut unique = all_subjects.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 4, "no duplicates");
    }

    // AC-NULL-1 — Paginação da cauda NULL sem duplicata e sem buraco em >=3 páginas
    #[test]
    fn null_tail_pagination_no_duplicate_no_gap() {
        let conn = setup_test_db();

        // 2 keyed subjects + 4 unkeyed subjects; limit=2 forces >=3 pages.
        let t1 = insert_tx(&conn);
        insert_iri(&conn, "ex:Keyed1", "ex:member", "ex:Set", t1);
        let tk1 = insert_tx(&conn);
        insert_literal(&conn, "ex:Keyed1", "ex:rank", "alpha", tk1);

        let t2 = insert_tx(&conn);
        insert_iri(&conn, "ex:Keyed2", "ex:member", "ex:Set", t2);
        let tk2 = insert_tx(&conn);
        insert_literal(&conn, "ex:Keyed2", "ex:rank", "beta", tk2);

        let t3 = insert_tx(&conn);
        insert_iri(&conn, "ex:Null1", "ex:member", "ex:Set", t3);
        insert_iri(&conn, "ex:Null2", "ex:member", "ex:Set", t3);
        insert_iri(&conn, "ex:Null3", "ex:member", "ex:Set", t3);
        insert_iri(&conn, "ex:Null4", "ex:member", "ex:Set", t3);

        let snapshot_tx = t3;

        // Collect all pages with limit=2; stop when has_more = (returned rows == limit).
        let mut all_subjects: Vec<String> = Vec::new();
        let mut cursor: Option<(Option<i64>, String)> = None;
        loop {
            let after = cursor.as_ref().map(|(tx, s)| (*tx, s.as_str()));
            let page = page_as_of(
                &conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, after, None,
            ).unwrap();
            let fetched = page.len();
            for r in &page {
                all_subjects.push(r.subject.clone());
            }
            if fetched < 2 {
                break;
            }
            let last = page.last().unwrap();
            cursor = Some((last.order_tx, last.subject.clone()));
        }

        assert_eq!(all_subjects.len(), 6, "all 6 subjects must be visited");

        let mut unique = all_subjects.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 6, "no duplicates");

        // The two keyed subjects must come before the four unkeyed ones.
        let keyed_positions: Vec<usize> = all_subjects.iter()
            .enumerate()
            .filter(|(_, s)| s.starts_with("ex:Keyed"))
            .map(|(i, _)| i)
            .collect();
        let null_positions: Vec<usize> = all_subjects.iter()
            .enumerate()
            .filter(|(_, s)| s.starts_with("ex:Null"))
            .map(|(i, _)| i)
            .collect();
        assert!(
            keyed_positions.iter().all(|k| null_positions.iter().all(|n| k < n)),
            "keyed subjects must appear before all null-keyed subjects"
        );
    }

    // AC-TIE-1 — Fronteira de página no meio de um grupo com mesmo order_tx: sem gap, sem duplicata
    #[test]
    fn same_order_tx_no_gap_no_duplicate() {
        let conn = setup_test_db();

        // Write membership for all 5 subjects.
        let t_mem = insert_tx(&conn);
        for name in &["ex:A", "ex:B", "ex:C", "ex:D", "ex:E"] {
            insert_iri(&conn, name, "ex:member", "ex:Set", t_mem);
        }

        // Write order key for all 5 in the SAME transaction → same order_tx.
        let t_key = insert_tx(&conn);
        for name in &["ex:A", "ex:B", "ex:C", "ex:D", "ex:E"] {
            insert_literal(&conn, name, "ex:rank", "shared", t_key);
        }

        let snapshot_tx = t_key;

        // With limit=2 the first page cuts in the middle of the tied group.
        let page0 = page_as_of(
            &conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, None, None,
        ).unwrap();
        assert_eq!(page0.len(), 2, "first page must have 2 rows");

        let last0 = page0.last().unwrap();
        let cursor0 = (last0.order_tx, last0.subject.as_str());

        let page1 = page_as_of(
            &conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, Some(cursor0), None,
        ).unwrap();
        assert_eq!(page1.len(), 2, "second page must have 2 rows (no gap in tied group)");

        let last1 = page1.last().unwrap();
        let cursor1 = (last1.order_tx, last1.subject.as_str());

        let page2 = page_as_of(
            &conn, "ex:member", "ex:Set", snapshot_tx, "ex:rank", 2, Some(cursor1), None,
        ).unwrap();
        assert_eq!(page2.len(), 1, "third page must have 1 remaining row");

        let combined: Vec<String> = page0.iter()
            .chain(page1.iter())
            .chain(page2.iter())
            .map(|r| r.subject.clone())
            .collect();
        assert_eq!(combined.len(), 5, "all 5 tied subjects across 3 pages");

        let mut unique = combined.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "no duplicates in tied group pagination");

        // All rows share the same order_tx.
        let all_txs: Vec<Option<i64>> = page0.iter()
            .chain(page1.iter())
            .chain(page2.iter())
            .map(|r| r.order_tx)
            .collect();
        assert!(all_txs.iter().all(|tx| *tx == Some(t_key)), "all rows must share the same order_tx");

        // Within the tied group the order must be subject ASC across pages.
        let subjects_in_order: Vec<String> = page0.iter()
            .chain(page1.iter())
            .chain(page2.iter())
            .map(|r| r.subject.clone())
            .collect();
        for w in subjects_in_order.windows(2) {
            assert!(w[0] < w[1], "tied subjects must be ordered by subject ASC across pages");
        }
    }
}

#[cfg(test)]
mod backlinks_grouped_tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    fn insert_tx(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_backlink(conn: &Connection, subject: &str, predicate: &str, object: &str, tx: i64) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, tx, origin_id, retracted, created_at, is_current) \
             VALUES (?1, ?2, ?3, 'iri', ?4, 1, 0, 0, 1)",
            rusqlite::params![subject, predicate, object, tx],
        )
        .unwrap();
    }

    fn insert_rdf_type(conn: &Connection, subject: &str, class: &str, tx: i64) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, tx, origin_id, retracted, created_at, is_current) \
             VALUES (?1, 'rdf:type', ?2, 'iri', ?3, 1, 0, 0, 1)",
            rusqlite::params![subject, class, tx],
        )
        .unwrap();
    }

    /// Builds a scenario with 17 backlinks (predicate=ex:ref, object=ex:Target, source_class=ex:Cls).
    /// Items at rank 14 and 15 (0-indexed) in last_tx DESC, subject ASC order share the SAME last_tx
    /// as items at rank 15 and 16, forcing a tie across the page-1 boundary (rn<=15).
    /// Specifically:
    ///   subjects ex:S01..ex:S12 → each written at a unique tx (high → low), filling ranks 0-11.
    ///   subjects ex:S13, ex:S14, ex:S15, ex:S16 → all written at the SAME tx (tie_tx),
    ///     filling ranks 12-15 in subject ASC order — so ex:S13 and ex:S14 fall inside page-1,
    ///     and ex:S15 and ex:S16 fall on page-2.
    ///   subject ex:S17 → written at a tx lower than tie_tx, falls last.
    fn setup_boundary_tie(conn: &Connection) -> (i64, Vec<String>) {
        let t_high = insert_tx(conn);
        let t_class = insert_tx(conn);

        // Register class for all subjects.
        for i in 1..=17 {
            let s = format!("ex:S{:02}", i);
            insert_rdf_type(conn, &s, "ex:Cls", t_class);
        }

        // Subjects with distinct last_tx, ranked 0-11 (highest tx first).
        let mut txs: Vec<i64> = Vec::new();
        for _ in 0..12 {
            txs.push(insert_tx(conn));
        }
        // Write in descending order so S01 has the highest tx.
        for (i, &tx) in txs.iter().rev().enumerate() {
            let s = format!("ex:S{:02}", i + 1);
            insert_backlink(conn, &s, "ex:ref", "ex:Target", tx);
        }
        // Tie group: S13, S14, S15, S16 all share the same tx (tie_tx).
        let tie_tx = insert_tx(conn);
        for i in 13..=16 {
            let s = format!("ex:S{:02}", i);
            insert_backlink(conn, &s, "ex:ref", "ex:Target", tie_tx);
        }
        // S17 gets the lowest tx.
        let t_low = insert_tx(conn);
        insert_backlink(conn, "ex:S17", "ex:ref", "ex:Target", t_low);

        // Expected full order (last_tx DESC, subject ASC):
        // ranks 0-11: ex:S01..ex:S12 (distinct tx, descending)
        // ranks 12-15: ex:S13, ex:S14, ex:S15, ex:S16 (same tie_tx, subject ASC)
        // rank 16: ex:S17
        let expected_order: Vec<String> = (1..=12)
            .map(|i| format!("ex:S{:02}", i))
            .chain([13, 14, 15, 16].iter().map(|i| format!("ex:S{:02}", i)))
            .chain(std::iter::once("ex:S17".to_string()))
            .collect();

        let _ = t_high;
        (tie_tx, expected_order)
    }

    /// Assert A: page-1 (rn<=15) is exactly the first 15 subjects in last_tx DESC, subject ASC.
    #[test]
    fn assert_a_page1_deterministic_order() {
        let conn = setup_test_db();
        let (_, expected_order) = setup_boundary_tie(&conn);

        let rows = get_backlinks_grouped_limited(&conn, "ex:Target", 15).unwrap();
        let page1_subjects: Vec<String> = rows.iter().map(|r| r.subject.clone()).collect();

        assert_eq!(page1_subjects.len(), 15, "page-1 must contain exactly 15 items");
        assert_eq!(
            page1_subjects,
            expected_order[..15].to_vec(),
            "page-1 must be the first 15 subjects in last_tx DESC, subject ASC"
        );
    }

    /// Assert B: union of page-1 and page-2 is contiguous — no gap, no duplicate.
    /// Derives the cursor exactly as group_cursor does in commands/entity/individual.rs.
    #[test]
    fn assert_b_page1_plus_page2_contiguous() {
        let conn = setup_test_db();
        let (_, expected_order) = setup_boundary_tie(&conn);

        let rows = get_backlinks_grouped_limited(&conn, "ex:Target", 15).unwrap();
        assert_eq!(rows.len(), 15);

        // Derive cursor: min last_tx among the 15 included, and max subject among those sharing
        // that min last_tx — exactly as group_cursor does.
        let mut min_tx = i64::MAX;
        let mut max_subject_at_min = String::new();
        for r in &rows {
            if r.last_tx < min_tx {
                min_tx = r.last_tx;
                max_subject_at_min = r.subject.clone();
            } else if r.last_tx == min_tx && r.subject > max_subject_at_min {
                max_subject_at_min = r.subject.clone();
            }
        }

        // snapshot_tx is the max tx in the DB at read time.
        let snapshot_tx: i64 = conn
            .query_row("SELECT MAX(tx) FROM transactions", [], |row| row.get(0))
            .unwrap();

        let page2 = get_backlinks_page(
            &conn,
            "ex:Target",
            "ex:ref",
            Some("ex:Cls"),
            Some((min_tx, max_subject_at_min)),
            17,
            snapshot_tx,
        )
        .unwrap();

        let page1_subjects: Vec<String> = rows.iter().map(|r| r.subject.clone()).collect();
        let page2_subjects: Vec<String> = page2.iter().map(|(s, _)| s.clone()).collect();

        // No duplicates between pages.
        for s in &page2_subjects {
            assert!(
                !page1_subjects.contains(s),
                "subject {s} appears in both page-1 and page-2 (duplicate)"
            );
        }

        // Union must cover all 17 subjects.
        let mut union: Vec<String> = page1_subjects.iter().chain(page2_subjects.iter()).cloned().collect();
        union.sort();
        let mut expected_sorted = expected_order.clone();
        expected_sorted.sort();
        assert_eq!(union, expected_sorted, "union must contain all 17 subjects without gap");
    }

    /// Assert C: concatenated order (page-1 followed by page-2) is monotonic in
    /// (last_tx DESC, subject ASC).
    #[test]
    fn assert_c_concatenated_order_monotonic() {
        let conn = setup_test_db();
        let (_, _) = setup_boundary_tie(&conn);

        let rows = get_backlinks_grouped_limited(&conn, "ex:Target", 15).unwrap();

        let mut min_tx = i64::MAX;
        let mut max_subject_at_min = String::new();
        for r in &rows {
            if r.last_tx < min_tx {
                min_tx = r.last_tx;
                max_subject_at_min = r.subject.clone();
            } else if r.last_tx == min_tx && r.subject > max_subject_at_min {
                max_subject_at_min = r.subject.clone();
            }
        }

        let snapshot_tx: i64 = conn
            .query_row("SELECT MAX(tx) FROM transactions", [], |row| row.get(0))
            .unwrap();

        let page2 = get_backlinks_page(
            &conn,
            "ex:Target",
            "ex:ref",
            Some("ex:Cls"),
            Some((min_tx, max_subject_at_min)),
            17,
            snapshot_tx,
        )
        .unwrap();

        let combined: Vec<(i64, String)> = rows
            .iter()
            .map(|r| (r.last_tx, r.subject.clone()))
            .chain(page2.iter().map(|(s, tx)| (*tx, s.clone())))
            .collect();

        for w in combined.windows(2) {
            let (tx_a, ref sub_a) = w[0];
            let (tx_b, ref sub_b) = w[1];
            assert!(
                tx_a > tx_b || (tx_a == tx_b && sub_a < sub_b),
                "order violation at ({tx_a}, {sub_a}) → ({tx_b}, {sub_b}): must be last_tx DESC, subject ASC"
            );
        }
    }
}
