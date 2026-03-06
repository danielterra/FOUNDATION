/// EVTO Query Functions
///
/// Pure functions for querying the triple store

use rusqlite::{Connection, Row, types::Value as SqlValue};
use super::triple_type::Triple;
use super::object_type::Object;
use super::query_result_type::QueryResult;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Query triples by entity (E - subject)
///
/// Returns current state of all predicates for the entity:
/// - For functional properties: only the most recent value
/// - For non-functional properties: all distinct current values (deduped by object)
pub fn get_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {

    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND retracted = 0
         ORDER BY predicate, object, object_value, tx DESC"
    )?;

    let all_triples: Vec<Triple> = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Group by predicate and keep most recent value for each unique (predicate, object) pair
    // This allows multiple values for the same predicate but deduplicates updates to the same value
    let mut seen_pairs = std::collections::HashSet::new();
    let current_triples: Vec<Triple> = all_triples
        .into_iter()
        .filter(|t| {
            // Create a unique key for (predicate, object) pair
            let object_key = match &t.object {
                Object::Iri(iri) => format!("iri:{}", iri),
                Object::Literal { value, datatype, language } => {
                    format!(
                        "lit:{}:{}:{}",
                        value,
                        datatype.as_deref().unwrap_or(""),
                        language.as_deref().unwrap_or(""),
                    )
                },
                Object::Blank(id) => format!("blank:{}", id),
                Object::Integer(n) => format!("int:{}", n),
                Object::Number(n) => format!("num:{}", n),
                Object::Boolean(b) => format!("bool:{}", b),
                Object::DateTime(dt) => format!("dt:{}", dt),
            };
            let key = format!("{}|{}", t.predicate, object_key);
            seen_pairs.insert(key)
        })
        .collect();

    Ok(QueryResult::new(current_triples))
}

/// Query retracted triples by entity (E - subject)
///
/// Returns all triples with retracted = 1 for the given entity.
/// Used when include_retracted is true to show historical/removed facts.
pub fn get_retracted_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
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

/// Query all active triples that reference a given IRI as their object
pub fn get_by_object_iri(conn: &Connection, object_iri: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE object = ? AND object_type = 'iri' AND retracted = 0"
    )?;

    let triples = stmt
        .query_map([object_iri], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

/// Query triples by predicate (V - value/property)
pub fn get_by_predicate(conn: &Connection, predicate: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE predicate = ? AND retracted = 0
         ORDER BY tx DESC"
    )?;

    let triples = stmt
        .query_map([predicate], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

/// Query triples by subject and predicate (EV)
///
/// Returns current state based on property characteristics:
/// - Functional properties (owl:FunctionalProperty): Single most recent value
/// - Non-functional properties: Most recent value for EACH distinct object
///
/// Example:
/// - `Person -> email -> value` (functional): returns 1 triple (latest email)
/// - `Person -> sentMessage -> [msg1, msg2]` (non-functional): returns 2 triples
///   (latest state of each message)
pub fn get_by_entity_predicate(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<QueryResult> {
    get_by_entity_predicate_internal(conn, entity, predicate, true)
}

/// Internal implementation with check_functional parameter
///
/// The `check_functional` parameter controls whether to check if the property is functional.
/// Set to false when calling from Property::is_functional to avoid infinite recursion.
pub fn get_by_entity_predicate_internal(
    conn: &Connection,
    entity: &str,
    predicate: &str,
    check_functional: bool,
) -> Result<QueryResult> {

    // Check if property is functional (only if check_functional is true)
    let is_functional = if check_functional {
        crate::owl::Property::is_functional(conn, predicate)
            .unwrap_or(false) // Default to non-functional if can't determine
    } else {
        false // Skip functional check to avoid recursion
    };

    if is_functional {
        // Functional property: return only the single most recent value
        let mut stmt = conn.prepare(
            "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_type, object_number, object_integer, object_datetime, object_boolean,
                    tx, origin_id, retracted, created_at
             FROM triples
             WHERE subject = ? AND predicate = ? AND retracted = 0
             ORDER BY tx DESC
             LIMIT 1"
        )?;

        let triples = stmt
            .query_map([entity, predicate], row_to_triple)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(QueryResult::new(triples))
    } else {
        // Non-functional property: return most recent value for each distinct object
        let mut stmt = conn.prepare(
            "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_type, object_number, object_integer, object_datetime, object_boolean,
                    tx, origin_id, retracted, created_at
             FROM triples
             WHERE subject = ? AND predicate = ? AND retracted = 0
             ORDER BY object, object_value, tx DESC"
        )?;

        let all_triples: Vec<Triple> = stmt
            .query_map([entity, predicate], row_to_triple)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Keep only the most recent triple for each distinct object
        let mut seen_objects = std::collections::HashSet::new();
        let current_triples: Vec<Triple> = all_triples
            .into_iter()
            .filter(|t| {
                // Create unique key for this object
                let key = match &t.object {
                    Object::Iri(iri) => format!("iri:{}", iri),
                    Object::Blank(id) => format!("blank:{}", id),
                    Object::Literal { value, .. } => format!("lit:{}", value),
                    Object::Integer(i) => format!("int:{}", i),
                    Object::Number(n) => format!("num:{}", n),
                    Object::Boolean(b) => format!("bool:{}", b),
                    Object::DateTime(dt) => format!("dt:{}", dt),
                };
                seen_objects.insert(key)
            })
            .collect();

        Ok(QueryResult::new(current_triples))
    }
}

/// Query by predicate and object (e.g., all properties with a specific domain)
pub fn get_by_predicate_object(
    conn: &Connection,
    predicate: &str,
    object: &str,
) -> Result<QueryResult> {
    // Support boolean values: "true" maps to object_boolean=1, "false" to object_boolean=0
    let (where_clause, params): (&str, Vec<&dyn rusqlite::ToSql>) = if object == "true" {
        (
            "WHERE predicate = ?1 AND object_boolean = 1 AND retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql],
        )
    } else if object == "false" {
        (
            "WHERE predicate = ?1 AND object_boolean = 0 AND retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql],
        )
    } else {
        (
            "WHERE predicate = ?1 AND object = ?2 AND retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql, &object as &dyn rusqlite::ToSql],
        )
    };

    let query = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         {}
         ORDER BY tx DESC",
        where_clause
    );

    let mut stmt = conn.prepare(&query)?;
    let triples = stmt
        .query_map(params.as_slice(), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

/// A single row returned by `get_backlinks_grouped_limited`.
#[derive(Debug, Clone)]
pub struct BacklinkRow {
    pub subject: String,
    pub predicate: String,
    pub source_class: Option<String>,
    pub group_total: usize,
}

/// Return backlinks grouped by (predicate, source_class), loading at most `limit_per_group`
/// entities per group (the most recently active ones).
/// Each row carries the real total count of distinct entities in its group.
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
             GROUP BY t.subject, t.predicate
         ),
         backlinks_with_class AS (
             SELECT
                 br.subject,
                 br.predicate,
                 br.last_tx,
                 (SELECT t2.object FROM triples t2
                  WHERE t2.subject = br.subject
                    AND t2.predicate = 'rdf:type'
                    AND t2.object_type = 'iri'
                    AND t2.retracted = 0
                  ORDER BY t2.tx DESC
                  LIMIT 1) AS source_class
             FROM backlinks_raw br
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
                     ORDER BY bwc.last_tx DESC
                 ) AS rn
             FROM backlinks_with_class bwc
             JOIN group_counts gc
               ON gc.predicate = bwc.predicate
              AND gc.source_class IS bwc.source_class
         )
         SELECT subject, predicate, source_class, group_total
         FROM ranked
         WHERE rn <= {}
         ORDER BY last_tx DESC",
        limit_per_group
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([object], |row| {
            let subject: String = row.get(0)?;
            let predicate: String = row.get(1)?;
            let source_class: Option<String> = row.get(2)?;
            let group_total: i64 = row.get(3)?;
            Ok(BacklinkRow {
                subject,
                predicate,
                source_class,
                group_total: group_total as usize,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Fetch values for specified predicates across multiple subjects in a single query.
/// Returns Vec<(subject, predicate, object)> ordered by subject, predicate, tx DESC.
/// Useful for batch-loading metadata (labels, icons) without N+1 queries.
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
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject IN ({}) AND predicate IN ({}) AND retracted = 0
         ORDER BY subject, predicate, tx DESC",
        subject_phs, predicate_phs
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

/// Fetch the most recent IRI value of a given predicate for multiple subjects.
/// Returns a HashMap from subject IRI to the first (most recent) matching IRI value.
/// Subjects with no matching triple are omitted.
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
        "SELECT subject, object FROM triples
         WHERE subject IN ({}) AND predicate = ? AND object_type = 'iri' AND retracted = 0
         ORDER BY subject, tx DESC",
        placeholders
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

/// Find entities by class and properties in a single query
///
/// This performs an efficient SQL JOIN to find entities that match a class and property
/// constraints.
/// Can be used with one or multiple properties.
///
/// Example:
/// ```ignore
/// // Single property
/// let releases = find_by_class_and_properties(
///     conn,
///     "foundation:SoftwareRelease",
///     &[("foundation:versionNumber", "0.1.0")]
/// )?;
///
/// // Multiple properties
/// let release = find_by_class_and_properties(
///     conn,
///     "foundation:SoftwareRelease",
///     &[
///         ("foundation:versionNumber", "0.1.0"),
///         ("foundation:releaseOf", "foundation:FoundationProduct"),
///     ]
/// )?;
/// ```
pub fn find_by_class_and_properties(
    conn: &Connection,
    class_iri: &str,
    properties: &[(&str, &str)],
) -> Result<Vec<String>> {
    if properties.is_empty() {
        return Ok(Vec::new());
    }

    // Build dynamic query with multiple JOINs
    let mut query = String::from(
        "SELECT DISTINCT t0.subject
         FROM triples t0"
    );

    // Add JOIN for each property
    for (i, _) in properties.iter().enumerate() {
        let table_num = i + 1;
        query.push_str(&format!(
            "\n         INNER JOIN triples t{} ON t0.subject = t{}.subject",
            table_num, table_num
        ));
    }

    // Add WHERE clause for class
    query.push_str(&format!(
        "\n         WHERE t0.predicate = 'rdf:type'
           AND t0.object = '{}'
           AND t0.retracted = 0",
        class_iri
    ));

    // Add WHERE clause for each property
    for (i, (prop_iri, _)) in properties.iter().enumerate() {
        let table_num = i + 1;
        query.push_str(&format!(
            "\n           AND t{}.predicate = '{}'
           AND t{}.retracted = 0",
            table_num, prop_iri, table_num
        ));
    }

    // Add value matching (supports literal, IRI, and boolean)
    for (i, (_, value)) in properties.iter().enumerate() {
        let table_num = i + 1;
        // Check if value is a boolean
        if value == &"true" || value == &"false" {
            let bool_val = if value == &"true" { 1 } else { 0 };
            query.push_str(&format!(
                "\n           AND (t{}.object_value = '{}' OR t{}.object = '{}'\
                    OR t{}.object_boolean = {})",
                table_num, value, table_num, value, table_num, bool_val
            ));
        } else {
            query.push_str(&format!(
                "\n           AND (t{}.object_value = '{}' OR t{}.object = '{}')",
                table_num, value, table_num, value
            ));
        }
    }

    let mut stmt = conn.prepare(&query)?;
    let entities: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(entities)
}

/// Find entities of a class with optional date range and retraction filtering
///
/// Filters by `created_at` on the `rdf:type` triple.
/// When `include_retracted` is true, retracted entities are included.
pub fn find_entities_by_class_with_date_range(
    conn: &Connection,
    class_iri: &str,
    from_millis: Option<i64>,
    to_millis: Option<i64>,
    include_retracted: bool,
) -> Result<Vec<String>> {
    let retracted_clause = if include_retracted { "" } else { " AND retracted = 0" };

    let mut conditions = format!(
        "predicate = 'rdf:type' AND object = ?1{}",
        retracted_clause,
    );

    if from_millis.is_some() {
        conditions.push_str(" AND created_at >= ?2");
    }
    if to_millis.is_some() {
        let param_num = if from_millis.is_some() { 3 } else { 2 };
        conditions.push_str(&format!(" AND created_at <= ?{}", param_num));
    }

    let sql = format!(
        "SELECT DISTINCT subject FROM triples WHERE {}",
        conditions
    );

    let mut stmt = conn.prepare(&sql)?;

    let entities: Vec<String> = match (from_millis, to_millis) {
        (Some(from), Some(to)) => stmt
            .query_map(rusqlite::params![class_iri, from, to], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?,
        (Some(from), None) => stmt
            .query_map(rusqlite::params![class_iri, from], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?,
        (None, Some(to)) => stmt
            .query_map(rusqlite::params![class_iri, to], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?,
        (None, None) => stmt
            .query_map(rusqlite::params![class_iri], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };

    Ok(entities)
}

/// Find entities by class and properties with operator and retraction support
///
/// Like `find_by_class_and_properties` but supports comparison operators for
/// `xsd:dateTime` property values and an `include_retracted` flag.
///
/// The `operator` parameter applies to all property constraints and accepts
/// `"="`, `">="`, `"<="`, `">"`, `"<"`. For `xsd:dateTime` values the
/// `object_datetime` column (Unix millis) is used with the operator; for all
/// other values equality matching against `object_value` / `object` is used.
pub fn find_by_class_and_properties_with_options(
    conn: &Connection,
    class_iri: &str,
    properties: &[(&str, &str, &str)],  // (predicate, value, operator)
    include_retracted: bool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<String>, usize)> {
    if properties.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let type_retracted_filter = if include_retracted { "" } else { " AND t0.retracted = 0" };

    let mut joins = String::new();
    let mut where_clause = format!(
        "WHERE t0.predicate = 'rdf:type' AND t0.object = ?{type_retracted_filter}"
    );
    let mut params: Vec<SqlValue> = vec![SqlValue::Text(class_iri.to_string())];

    for (i, _) in properties.iter().enumerate() {
        let n = i + 1;
        joins.push_str(&format!("\n         INNER JOIN triples t{n} ON t0.subject = t{n}.subject"));
    }

    for (i, (prop_iri, _, _)) in properties.iter().enumerate() {
        let n = i + 1;
        let prop_retracted_filter = if include_retracted { String::new() } else { format!(" AND t{n}.retracted = 0") };
        where_clause.push_str(&format!("\n           AND t{n}.predicate = ?{prop_retracted_filter}"));
        params.push(SqlValue::Text(prop_iri.to_string()));
    }

    for (i, (_, value, operator)) in properties.iter().enumerate() {
        let n = i + 1;
        if let Ok(millis) = parse_datetime_to_millis(value) {
            let sql_op = validate_operator(operator)
                .map_err(|_| format!("Invalid operator '{operator}': must be one of =, >=, <=, >, <"))?;
            // millis and sql_op are validated — safe to interpolate
            where_clause.push_str(&format!("\n           AND t{n}.object_datetime {sql_op} {millis}"));
        } else if *value == "true" || *value == "false" {
            let bool_val: i64 = if *value == "true" { 1 } else { 0 };
            where_clause.push_str(&format!(
                "\n           AND (t{n}.object_value = ? OR t{n}.object = ? OR t{n}.object_boolean = ?)"
            ));
            params.push(SqlValue::Text(value.to_string()));
            params.push(SqlValue::Text(value.to_string()));
            params.push(SqlValue::Integer(bool_val));
        } else {
            where_clause.push_str(&format!(
                "\n           AND (t{n}.object_value = ? OR t{n}.object = ?)"
            ));
            params.push(SqlValue::Text(value.to_string()));
            params.push(SqlValue::Text(value.to_string()));
        }
    }

    let count_query = format!(
        "SELECT COUNT(*) FROM (SELECT DISTINCT t0.subject FROM triples t0{joins}\n         {where_clause})"
    );
    let total: usize = conn.query_row(
        &count_query,
        rusqlite::params_from_iter(params.iter()),
        |row| row.get::<_, i64>(0),
    )? as usize;

    let mut data_params = params;
    data_params.push(SqlValue::Integer(limit as i64));
    data_params.push(SqlValue::Integer(offset as i64));

    let data_query = format!(
        "SELECT DISTINCT t0.subject FROM triples t0{joins}\n         {where_clause}\n         LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&data_query)?;
    let entities: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(data_params.iter()), |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok((entities, total))
}

/// Returns IRIs of `foundation:AIConversationMessage` instances belonging to
/// `conversation_iri`, ordered by `foundation:sentAt` descending (newest first).
/// `limit = usize::MAX` means no limit (SQLite treats LIMIT -1 as unlimited).
pub fn find_message_iris_by_conversation(
    conn: &Connection,
    conversation_iri: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<String>> {
    let sql = "
        SELECT subject FROM (
            SELECT t_type.subject, MAX(t_sent.object_datetime) AS ts
            FROM triples t_type
            INNER JOIN triples t_conv
                ON t_type.subject = t_conv.subject
                AND t_conv.predicate = 'foundation:partOfConversation'
                AND (t_conv.object = ?1 OR t_conv.object_value = ?1)
                AND t_conv.retracted = 0
            LEFT JOIN triples t_sent
                ON t_type.subject = t_sent.subject
                AND t_sent.predicate = 'foundation:sentAt'
                AND t_sent.retracted = 0
            WHERE t_type.predicate = 'rdf:type'
              AND t_type.object = 'foundation:AIConversationMessage'
              AND t_type.retracted = 0
            GROUP BY t_type.subject
        )
        ORDER BY ts DESC
        LIMIT ?2 OFFSET ?3
    ";
    let limit_i64: i64 = limit.try_into().unwrap_or(-1);
    let mut stmt = conn.prepare(sql)?;
    let iris = stmt
        .query_map(rusqlite::params![conversation_iri, limit_i64, offset as i64], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(iris)
}

fn parse_datetime_to_millis(value: &str) -> std::result::Result<i64, ()> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.timestamp_millis())
        .map_err(|_| ())
}

fn validate_operator(op: &str) -> std::result::Result<&str, ()> {
    match op {
        "=" | ">=" | "<=" | ">" | "<" => Ok(op),
        _ => Err(()),
    }
}

/// Find entities by attribute value (works for any property)
#[allow(dead_code)]
pub fn find_entities_by_attribute_value(
    conn: &Connection,
    attribute: &str,
    value: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT subject
         FROM triples
         WHERE predicate = ? AND object_value = ? AND retracted = 0"
    )?;

    let entities: Vec<String> = stmt
        .query_map([attribute, value], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(entities)
}

/// Query entity state at specific time (ET - temporal query)
#[allow(dead_code)]
pub fn get_at_time(conn: &Connection, entity: &str, tx: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND tx <= ? AND retracted = 0
         ORDER BY predicate, tx DESC"
    )?;

    let triples = stmt
        .query_map([entity, tx.to_string().as_str()], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Keep only the latest value for each predicate (temporal snapshot)
    let mut seen_predicates = std::collections::HashSet::new();
    let snapshot: Vec<Triple> = triples
        .into_iter()
        .filter(|t| seen_predicates.insert(t.predicate.clone()))
        .collect();

    Ok(QueryResult::new(snapshot))
}

/// Query triples by origin (O)
#[allow(dead_code)]
pub fn get_by_origin(conn: &Connection, origin_id: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
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

/// Query complete history of an entity (all transactions)
#[allow(dead_code)]
pub fn get_history(conn: &Connection, entity: &str) -> Result<Vec<(i64, Vec<Triple>)>> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ?
         ORDER BY tx ASC"
    )?;

    let all_triples: Vec<Triple> = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Group by transaction
    let mut history: std::collections::HashMap<i64, Vec<Triple>> = std::collections::HashMap::new();
    for triple in all_triples {
        history.entry(triple.tx).or_insert_with(Vec::new).push(triple);
    }

    let mut result: Vec<(i64, Vec<Triple>)> = history.into_iter().collect();
    result.sort_by_key(|(tx, _)| *tx);

    Ok(result)
}

/// Get the maximum tx (most recent transaction) for each of the given entity IRIs.
/// Returns a HashMap from entity IRI to its max tx value.
/// Entities with no active triples are omitted from the result.
pub fn get_entities_max_tx(
    conn: &Connection,
    entity_iris: &[String],
) -> Result<std::collections::HashMap<String, i64>> {
    if entity_iris.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let placeholders = entity_iris
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT subject, MAX(tx) FROM triples WHERE subject IN ({}) AND retracted = 0 GROUP BY subject",
        placeholders
    );

    let params: Vec<&dyn rusqlite::ToSql> = entity_iris
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();

    let mut stmt = conn.prepare(&sql)?;
    let result = stmt
        .query_map(params.as_slice(), |row| {
            let subject: String = row.get(0)?;
            let max_tx: i64 = row.get(1)?;
            Ok((subject, max_tx))
        })?
        .collect::<std::result::Result<std::collections::HashMap<_, _>, _>>()?;

    Ok(result)
}

/// Convert SQLite row to Triple
fn row_to_triple(row: &Row) -> rusqlite::Result<Triple> {
    let subject: String = row.get(0)?;
    let predicate: String = row.get(1)?;
    let object_opt: Option<String> = row.get(2)?;
    let object_value: Option<String> = row.get(3)?;
    let object_datatype: Option<String> = row.get(4)?;
    let object_language: Option<String> = row.get(5)?;
    let object_type: String = row.get(6)?;
    let object_number: Option<f64> = row.get(7)?;
    let object_integer: Option<i64> = row.get(8)?;
    let object_datetime: Option<i64> = row.get(9)?;
    let object_boolean: Option<i64> = row.get(10)?;
    let tx: i64 = row.get(11)?;
    let origin_id: i64 = row.get(12)?;
    let retracted: i64 = row.get(13)?;
    let created_at: i64 = row.get(14)?;

    let object = match object_type.as_str() {
        "iri" => Object::Iri(object_opt.ok_or(rusqlite::Error::InvalidQuery)?),
        "blank" => Object::Blank(object_opt.ok_or(rusqlite::Error::InvalidQuery)?),
        "literal" => {
            // Check for typed literals
            if let Some(int) = object_integer {
                Object::Integer(int)
            } else if let Some(num) = object_number {
                Object::Number(num)
            } else if let Some(dt) = object_datetime {
                // xsd:date preserves the original YYYY-MM-DD string so the frontend
                // can display it without a time component.
                if object_datatype.as_deref() == Some("xsd:date") {
                    Object::Literal {
                        value: object_value.ok_or(rusqlite::Error::InvalidQuery)?,
                        datatype: object_datatype,
                        language: object_language,
                    }
                } else {
                    Object::DateTime(dt)
                }
            } else if let Some(bool_val) = object_boolean {
                Object::Boolean(bool_val != 0)
            } else {
                // Generic literal
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
        assert_eq!(result.triples.len(), 2); // rdf:type and rdfs:label
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
        assert_eq!(result.triples.len(), 2); // Should have both triples at this tx
    }

    #[test]
    fn test_get_at_time_temporal_snapshot() {
        let mut conn = setup_test_db();
        let _tx1 = setup_test_data(&mut conn);

        // Add new triple with same predicate (update)
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

        // Query at tx2 should give latest label
        let result = get_at_time(&conn, "foundation:TestClass", tx2).unwrap();

        // Should only have 2 triples (latest rdfs:label + rdf:type)
        assert_eq!(result.triples.len(), 2);

        // Find the label triple
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

        // Origin ID 1 is "test" from setup_test_db
        let result = get_by_origin(&conn, 1).unwrap();
        assert!(result.triples.len() > 0);
    }

    #[test]
    fn test_get_history() {
        let mut conn = setup_test_db();
        let tx1 = setup_test_data(&mut conn);

        // Add another triple in a new transaction
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

        assert_eq!(history.len(), 2); // Two transactions
        assert_eq!(history[0].0, tx1); // First transaction
        assert_eq!(history[1].0, tx2); // Second transaction
        assert_eq!(history[0].1.len(), 2); // First tx has 2 triples
        assert_eq!(history[1].1.len(), 1); // Second tx has 1 triple
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
}
