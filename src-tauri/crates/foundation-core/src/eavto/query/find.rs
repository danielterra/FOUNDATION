use rusqlite::Connection;
use rusqlite::types::Value as SqlValue;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
pub struct SortSpec {
    pub property_iri: String,
    pub direction: SortDirection,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// A filter applied to a property when querying individuals.
pub enum PropertyFilter<'a> {
    /// Scalar comparison: `(prop_iri, value, operator)`.
    /// Supported operators: `=`, `!=`, `>=`, `<=`, `>`, `<`.
    /// Prefix with `?` (e.g. `?<=`) to make the filter optional —
    /// entities that lack the property are included.
    Compare(&'a str, &'a str, &'a str),

    /// Exclusion list: `(prop_iri, excluded_values)`.
    /// Matches entities whose property value is NOT in the given list.
    /// Entities that lack the property entirely are excluded (behaves like an INNER JOIN).
    NotIn(&'a str, &'a [&'a str]),
}

impl<'a> PropertyFilter<'a> {
    fn prop_iri(&self) -> &str {
        match self {
            PropertyFilter::Compare(prop, _, _) => prop,
            PropertyFilter::NotIn(prop, _) => prop,
        }
    }

    fn is_optional(&self) -> bool {
        match self {
            PropertyFilter::Compare(_, _, op) => is_optional_op(op) || base_op(op) == "not_exists",
            PropertyFilter::NotIn(_, _) => false,
        }
    }
}

pub fn find_by_class_and_properties(
    conn: &Connection,
    class_iri: &str,
    properties: &[(&str, &str)],
) -> Result<Vec<String>> {
    if properties.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = String::from(
        "SELECT DISTINCT t0.subject
         FROM triples_current t0"
    );

    for (i, _) in properties.iter().enumerate() {
        let table_num = i + 1;
        query.push_str(&format!(
            "\n         INNER JOIN triples_current t{} ON t0.subject = t{}.subject",
            table_num, table_num
        ));
    }

    query.push_str(&format!(
        "\n         WHERE t0.predicate = 'rdf:type'
           AND t0.object = '{}'",
        class_iri
    ));

    for (i, (prop_iri, _)) in properties.iter().enumerate() {
        let table_num = i + 1;
        query.push_str(&format!(
            "\n           AND t{}.predicate = '{}'",
            table_num, prop_iri
        ));
    }

    for (i, (_, value)) in properties.iter().enumerate() {
        let table_num = i + 1;
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

pub fn find_entities_by_class_with_date_range(
    conn: &Connection,
    class_iri: &str,
    from_millis: Option<i64>,
    to_millis: Option<i64>,
    include_retracted: bool,
) -> Result<Vec<String>> {
    let table = if include_retracted { "triples" } else { "triples_current" };

    let mut conditions = String::from("predicate = 'rdf:type' AND object = ?1");

    if from_millis.is_some() {
        conditions.push_str(" AND created_at >= ?2");
    }
    if to_millis.is_some() {
        let param_num = if from_millis.is_some() { 3 } else { 2 };
        conditions.push_str(&format!(" AND created_at <= ?{}", param_num));
    }

    let sql = format!(
        "SELECT DISTINCT subject FROM {table} WHERE {}",
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

pub fn find_by_class_iris_and_properties_with_options(
    conn: &Connection,
    class_iris: &[&str],
    properties: &[PropertyFilter<'_>],
    include_retracted: bool,
    limit: usize,
    offset: usize,
    sort: Option<&SortSpec>,
) -> Result<(Vec<String>, usize)> {
    if properties.is_empty() || class_iris.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let table = if include_retracted { "triples" } else { "triples_current" };

    let class_placeholders = class_iris.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let mut joins = String::new();
    let mut join_params: Vec<SqlValue> = Vec::new();
    let mut where_clause = format!(
        "WHERE t0.predicate = 'rdf:type' \
         AND t0.object IN ({class_placeholders})"
    );
    let mut where_params: Vec<SqlValue> = class_iris.iter()
        .map(|iri| SqlValue::Text(iri.to_string()))
        .collect();

    for (i, filter) in properties.iter().enumerate() {
        let n = i + 1;
        let optional = filter.is_optional();

        if optional {
            joins.push_str(&format!(
                "\n         LEFT JOIN {table} t{n} ON t0.subject = t{n}.subject \
                 AND t{n}.predicate = ?"
            ));
            join_params.push(SqlValue::Text(filter.prop_iri().to_string()));
        } else {
            joins.push_str(&format!(
                "\n         INNER JOIN {table} t{n} ON t0.subject = t{n}.subject"
            ));
            where_clause.push_str(&format!(
                "\n           AND t{n}.predicate = ?"
            ));
            where_params.push(SqlValue::Text(filter.prop_iri().to_string()));
        }
    }

    for (i, filter) in properties.iter().enumerate() {
        let n = i + 1;
        match filter {
            PropertyFilter::Compare(_, value, op) => {
                let base = base_op(op);
                if base == "exists" {
                    // INNER JOIN already ensures the property exists — no value condition needed
                } else if base == "not_exists" {
                    where_clause.push_str(&format!("\n           AND t{n}.predicate IS NULL"));
                } else {
                    let optional = is_optional_op(op);
                    let value_cond =
                        build_value_condition_fragment(n, value, base, &mut where_params)?;
                    if optional {
                        where_clause.push_str(&format!(
                            "\n           AND (t{n}.predicate IS NULL OR {value_cond})"
                        ));
                    } else {
                        where_clause.push_str(&format!("\n           AND {value_cond}"));
                    }
                }
            }
            PropertyFilter::NotIn(_, values) if !values.is_empty() => {
                let phs = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                for v in *values {
                    where_params.push(SqlValue::Text(v.to_string()));
                }
                for v in *values {
                    where_params.push(SqlValue::Text(v.to_string()));
                }
                where_clause.push_str(&format!(
                    "\n           AND (t{n}.object IS NULL OR t{n}.object NOT IN ({phs})) \
                     AND (t{n}.object_value IS NULL OR t{n}.object_value NOT IN ({phs}))"
                ));
            }
            PropertyFilter::NotIn(_, _) => {}
        }
    }

    // Sort join (comes before filter joins in SQL so its param is first)
    let sort_join = if sort.is_some() {
        format!(
            "\n         LEFT JOIN {table} tsort ON t0.subject = tsort.subject \
             AND tsort.predicate = ?"
        )
    } else {
        String::new()
    };
    let mut sort_join_param: Vec<SqlValue> = sort
        .map(|s| vec![SqlValue::Text(s.property_iri.clone())])
        .unwrap_or_default();

    // params order: sort join param, filter join params, where params
    sort_join_param.extend(join_params);
    let params: Vec<SqlValue> = sort_join_param.into_iter().chain(where_params).collect();

    let count_query = format!(
        "SELECT COUNT(*) FROM \
         (SELECT DISTINCT t0.subject FROM {table} t0{sort_join}{joins}\n         {where_clause})"
    );
    let total: usize = conn.query_row(
        &count_query,
        rusqlite::params_from_iter(params.iter()),
        |row| row.get::<_, i64>(0),
    )? as usize;

    let limit_val: i64 = if limit == usize::MAX { -1 } else { limit as i64 };
    let mut data_params = params;
    data_params.push(SqlValue::Integer(limit_val));
    data_params.push(SqlValue::Integer(offset as i64));

    let entities: Vec<String> = if let Some(s) = sort {
        let dir = match s.direction {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        };
        let inner = format!(
            "SELECT DISTINCT t0.subject, COALESCE(tsort.object_value, tsort.object) AS _sort \
             FROM {table} t0{sort_join}{joins}\n         {where_clause}"
        );
        let data_query =
            format!("SELECT subject FROM ({inner}) ORDER BY _sort {dir} LIMIT ? OFFSET ?");
        let mut stmt = conn.prepare(&data_query)?;
        let rows: std::result::Result<Vec<_>, _> = stmt
            .query_map(rusqlite::params_from_iter(data_params.iter()), |row| row.get(0))?
            .collect();
        rows?
    } else {
        let data_query = format!(
            "SELECT DISTINCT t0.subject FROM {table} t0{joins}\n         \
             {where_clause}\n         LIMIT ? OFFSET ?"
        );
        let mut stmt = conn.prepare(&data_query)?;
        let rows: std::result::Result<Vec<_>, _> = stmt
            .query_map(rusqlite::params_from_iter(data_params.iter()), |row| row.get(0))?
            .collect();
        rows?
    };

    Ok((entities, total))
}

/// Find entities matching all given property filters, without any class restriction.
pub fn find_by_properties_with_options(
    conn: &Connection,
    properties: &[PropertyFilter<'_>],
    include_retracted: bool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<String>, usize)> {
    if properties.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let table = if include_retracted { "triples" } else { "triples_current" };

    let mut joins = String::new();
    let mut join_params: Vec<SqlValue> = Vec::new();
    let mut where_clause = String::new();
    let mut where_params: Vec<SqlValue> = Vec::new();

    for (i, filter) in properties.iter().enumerate() {
        let optional = filter.is_optional();

        let base = if let PropertyFilter::Compare(_, _, op) = filter { base_op(op) } else { "=" };

        if i == 0 && base == "not_exists" {
            where_clause.push_str(&format!(
                "\n         WHERE t0.predicate = 'rdf:type'\
                 \n           AND NOT EXISTS (\
                 \n               SELECT 1 FROM {table} t_ne\
                 \n               WHERE t_ne.subject = t0.subject\
                 \n               AND t_ne.predicate = ?)"
            ));
            where_params.push(SqlValue::Text(filter.prop_iri().to_string()));
        } else if i == 0 {
            where_clause.push_str(&format!(
                "\n         WHERE t{i}.predicate = ?"
            ));
            where_params.push(SqlValue::Text(filter.prop_iri().to_string()));
        } else if optional {
            joins.push_str(&format!(
                "\n         LEFT JOIN {table} t{i} ON t0.subject = t{i}.subject \
                 AND t{i}.predicate = ?"
            ));
            join_params.push(SqlValue::Text(filter.prop_iri().to_string()));
        } else {
            joins.push_str(&format!(
                "\n         INNER JOIN {table} t{i} ON t0.subject = t{i}.subject"
            ));
            where_clause.push_str(&format!(
                "\n           AND t{i}.predicate = ?"
            ));
            where_params.push(SqlValue::Text(filter.prop_iri().to_string()));
        }
    }

    for (i, filter) in properties.iter().enumerate() {
        match filter {
            PropertyFilter::Compare(_, value, op) => {
                let base = base_op(op);
                if base == "exists" {
                    // INNER JOIN (or first-filter WHERE) already ensures the property exists
                } else if base == "not_exists" {
                    if i > 0 {
                        where_clause.push_str(&format!("\n           AND t{i}.predicate IS NULL"));
                    }
                    // i == 0: handled in the first loop via NOT EXISTS subquery
                } else {
                    let optional = is_optional_op(op);
                    let value_cond =
                        build_value_condition_fragment(i, value, base, &mut where_params)?;
                    if optional && i > 0 {
                        where_clause.push_str(&format!(
                            "\n           AND (t{i}.predicate IS NULL OR {value_cond})"
                        ));
                    } else {
                        where_clause.push_str(&format!("\n           AND {value_cond}"));
                    }
                }
            }
            PropertyFilter::NotIn(_, values) if !values.is_empty() => {
                let phs = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                for v in *values {
                    where_params.push(SqlValue::Text(v.to_string()));
                }
                for v in *values {
                    where_params.push(SqlValue::Text(v.to_string()));
                }
                where_clause.push_str(&format!(
                    "\n           AND (t{i}.object IS NULL OR t{i}.object NOT IN ({phs})) \
                     AND (t{i}.object_value IS NULL OR t{i}.object_value NOT IN ({phs}))"
                ));
            }
            PropertyFilter::NotIn(_, _) => {}
        }
    }

    // params order must match SQL: JOIN params appear before WHERE params in the query
    let params: Vec<SqlValue> = join_params.into_iter().chain(where_params).collect();

    let count_query = format!(
        "SELECT COUNT(*) FROM (SELECT DISTINCT t0.subject FROM {table} t0{joins}{where_clause})"
    );
    let total: usize = conn.query_row(
        &count_query,
        rusqlite::params_from_iter(params.iter()),
        |row| row.get::<_, i64>(0),
    )? as usize;

    let limit_val: i64 = if limit == usize::MAX { -1 } else { limit as i64 };
    let mut data_params = params;
    data_params.push(SqlValue::Integer(limit_val));
    data_params.push(SqlValue::Integer(offset as i64));

    let data_query = format!(
        "SELECT DISTINCT t0.subject FROM {table} t0{joins}{where_clause}\n         LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&data_query)?;
    let entities: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(data_params.iter()), |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok((entities, total))
}

/// Generic: find the most recent instance of `class_iri` that has `guard_predicate` set,
/// ordered by the latest `child_ts_predicate` among its children linked via
/// `child_link_predicate` whose `child_filter_predicate` equals `child_filter_value`.
///
/// Domain-specific predicate names and values are supplied by the caller (OWL layer),
/// keeping this function free of Foundation-specific IRIs.
pub fn find_class_instance_ordered_by_child_timestamp(
    conn: &Connection,
    class_iri: &str,
    guard_predicate: &str,
    child_link_predicate: &str,
    child_ts_predicate: &str,
    child_filter_predicate: &str,
    child_filter_value: &str,
) -> Result<Option<String>> {
    let sql = "
        SELECT conv.subject
        FROM triples_current conv
        WHERE conv.predicate = 'rdf:type'
          AND conv.object = ?1
          AND EXISTS (
              SELECT 1 FROM triples_current h
              WHERE h.subject = conv.subject
                AND h.predicate = ?2
          )
        ORDER BY (
            SELECT MAX(t_sent.object_value)
            FROM triples_current t_conv
            JOIN triples_current t_sent ON t_sent.subject = t_conv.subject
              AND t_sent.predicate = ?4
            JOIN triples_current t_role ON t_role.subject = t_conv.subject
              AND t_role.predicate = ?5
              AND t_role.object_value = ?6
            WHERE t_conv.predicate = ?3
              AND t_conv.object = conv.subject
        ) DESC NULLS LAST
        LIMIT 1
    ";
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(rusqlite::params![
        class_iri, guard_predicate, child_link_predicate,
        child_ts_predicate, child_filter_predicate, child_filter_value,
    ])?;
    Ok(rows.next()?.map(|row| row.get::<_, String>(0)).transpose()?)
}

/// Generic: find subjects that are linked to `parent_iri` via `link_predicate`,
/// ordered newest-first by `order_predicate` on those subjects.
///
/// Domain-specific predicate names are supplied by the caller (OWL layer),
/// keeping this function free of Foundation-specific IRIs.
pub fn find_subjects_linked_to_ordered_by(
    conn: &Connection,
    parent_iri: &str,
    link_predicate: &str,
    order_predicate: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<String>> {
    // Window function over all link_predicate rows (including historical) so we
    // honour the immutability rule: the row with MAX(tx) for each (subject, predicate)
    // is truth, regardless of the retracted flag on older rows.
    let sql = "
        SELECT subject FROM (
            SELECT t_conv.subject, MAX(t_sent.object_value) AS ts
            FROM (
                SELECT subject, retracted,
                       MAX(tx) OVER (PARTITION BY subject, predicate) AS max_tx, tx
                FROM triples
                WHERE predicate = ?1
                  AND (object = ?2 OR object_value = ?2)
            ) t_conv
            INNER JOIN triples t_sent
                ON t_conv.subject = t_sent.subject
               AND t_sent.predicate = ?3
               AND t_sent.retracted = 0
            WHERE t_conv.tx = t_conv.max_tx AND t_conv.retracted = 0
            GROUP BY t_conv.subject
        )
        ORDER BY ts DESC
        LIMIT ?4 OFFSET ?5
    ";
    let limit_i64: i64 = limit.try_into().unwrap_or(-1);
    let mut stmt = conn.prepare(sql)?;
    let iris = stmt
        .query_map(
            rusqlite::params![link_predicate, parent_iri, order_predicate, limit_i64, offset as i64],
            |row| row.get(0),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(iris)
}

fn is_optional_op(op: &str) -> bool {
    op.starts_with('?')
}

fn base_op<'a>(op: &'a str) -> &'a str {
    op.strip_prefix('?').unwrap_or(op)
}

fn build_value_condition_fragment(
    n: usize,
    value: &str,
    base_op: &str,
    params: &mut Vec<SqlValue>,
) -> Result<String> {
    let sql_op = validate_operator(base_op)
        .map_err(|_| format!("Invalid operator '{base_op}': must be one of =, !=, >=, <=, >, <"))?;

    if let Some(date_filter) = normalize_date_filter(value) {
        match date_filter {
            DateFilter::Date(date_str) => {
                params.push(SqlValue::Text(date_str));
                Ok(format!("substr(t{n}.object_value, 1, 10) {sql_op} ?"))
            }
            DateFilter::DateTime(epoch) => {
                params.push(SqlValue::Integer(epoch));
                Ok(format!("unixepoch(t{n}.object_value) {sql_op} ?"))
            }
        }
    } else if value == "true" || value == "false" {
        let bool_val: i64 = if value == "true" { 1 } else { 0 };
        if base_op == "!=" {
            params.push(SqlValue::Text(value.to_string()));
            params.push(SqlValue::Text(value.to_string()));
            params.push(SqlValue::Integer(bool_val));
            Ok(format!(
                "(t{n}.object_value IS NULL OR t{n}.object_value != ?) \
                 AND (t{n}.object IS NULL OR t{n}.object != ?) \
                 AND (t{n}.object_boolean IS NULL OR t{n}.object_boolean != ?)"
            ))
        } else {
            params.push(SqlValue::Text(value.to_string()));
            params.push(SqlValue::Text(value.to_string()));
            params.push(SqlValue::Integer(bool_val));
            Ok(format!(
                "(t{n}.object_value = ? OR t{n}.object = ? OR t{n}.object_boolean = ?)"
            ))
        }
    } else if let Ok(num) = value.parse::<f64>() {
        // Numeric values are stored in object_number (decimal) or object_integer, with
        // the text representation in object_value. Use CAST so that comparisons like >=
        // are numeric rather than lexicographic.
        let param = SqlValue::Real(num);
        params.push(param.clone());
        params.push(param.clone());
        params.push(param);
        Ok(format!(
            "(CAST(t{n}.object_value AS REAL) {sql_op} ? \
             OR t{n}.object_number {sql_op} ? \
             OR CAST(t{n}.object_integer AS REAL) {sql_op} ?)"
        ))
    } else if base_op == "!=" {
        params.push(SqlValue::Text(value.to_string()));
        params.push(SqlValue::Text(value.to_string()));
        Ok(format!(
            "(t{n}.object_value IS NULL OR t{n}.object_value != ?) \
             AND (t{n}.object IS NULL OR t{n}.object != ?)"
        ))
    } else {
        params.push(SqlValue::Text(value.to_string()));
        params.push(SqlValue::Text(value.to_string()));
        Ok(format!("(t{n}.object_value {sql_op} ? OR t{n}.object {sql_op} ?)"))
    }
}

enum DateFilter {
    Date(String),
    DateTime(i64),
}

fn normalize_date_filter(value: &str) -> Option<DateFilter> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(DateFilter::DateTime(dt.timestamp()));
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        use chrono::TimeZone;
        let epoch = chrono::Local.from_local_datetime(&ndt)
            .single()
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|| ndt.and_utc().timestamp());
        return Some(DateFilter::DateTime(epoch));
    }
    if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() {
        return Some(DateFilter::Date(value.to_string()));
    }
    None
}

fn validate_operator(op: &str) -> std::result::Result<&str, ()> {
    match op {
        "=" | "!=" | ">=" | "<=" | ">" | "<" => Ok(op),
        _ => Err(()),
    }
}

#[allow(dead_code)]
pub fn find_entities_by_attribute_value(
    conn: &Connection,
    attribute: &str,
    value: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT subject
         FROM triples_current
         WHERE predicate = ? AND object_value = ?"
    )?;

    let entities: Vec<String> = stmt
        .query_map([attribute, value], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(entities)
}

#[cfg(test)]
mod sort_tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    fn ensure_tx(conn: &Connection) -> i64 {
        if let Ok(id) = conn.query_row(
            "SELECT id FROM transactions WHERE origin = 'test' LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        ) {
            return id;
        }
        conn.execute("INSERT INTO transactions (origin, created_at) VALUES ('test', 0)", []).unwrap();
        conn.last_insert_rowid()
    }

    fn insert_triple(conn: &Connection, subject: &str, predicate: &str, value: &str) {
        let tx = ensure_tx(conn);
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, tx, origin_id, retracted, created_at) \
             VALUES (?1, ?2, ?3, 'literal', ?4, 1, 0, 0)",
            rusqlite::params![subject, predicate, value, tx],
        ).unwrap();
    }

    fn insert_type(conn: &Connection, subject: &str, class: &str) {
        let tx = ensure_tx(conn);
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, retracted, created_at) \
             VALUES (?1, 'rdf:type', ?2, 'iri', ?3, 1, 0, 0)",
            rusqlite::params![subject, class, tx],
        ).unwrap();
    }

    #[test]
    fn sort_by_string_property_ascending_returns_ordered_results() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:B", "ex:Thing");
        insert_type(&conn, "ex:A", "ex:Thing");
        insert_type(&conn, "ex:C", "ex:Thing");
        insert_triple(&conn, "ex:B", "rdfs:label", "B");
        insert_triple(&conn, "ex:A", "rdfs:label", "A");
        insert_triple(&conn, "ex:C", "rdfs:label", "C");

        let sort = SortSpec { property_iri: "rdfs:label".to_string(), direction: SortDirection::Asc };
        let (results, _) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("rdfs:label", "A", "?>=")],
            false, usize::MAX, 0, Some(&sort),
        ).unwrap();

        assert_eq!(results, vec!["ex:A", "ex:B", "ex:C"]);
    }

    #[test]
    fn sort_by_string_property_descending_returns_reverse_order() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:B", "ex:Thing");
        insert_type(&conn, "ex:A", "ex:Thing");
        insert_type(&conn, "ex:C", "ex:Thing");
        insert_triple(&conn, "ex:B", "rdfs:label", "B");
        insert_triple(&conn, "ex:A", "rdfs:label", "A");
        insert_triple(&conn, "ex:C", "rdfs:label", "C");

        let sort = SortSpec { property_iri: "rdfs:label".to_string(), direction: SortDirection::Desc };
        let (results, _) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("rdfs:label", "A", "?>=")],
            false, usize::MAX, 0, Some(&sort),
        ).unwrap();

        assert_eq!(results, vec!["ex:C", "ex:B", "ex:A"]);
    }

    #[test]
    fn missing_sort_property_value_sorts_last_for_desc() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:HasLabel", "ex:Thing");
        insert_type(&conn, "ex:NoLabel", "ex:Thing");
        insert_triple(&conn, "ex:HasLabel", "rdfs:label", "Z");

        let sort = SortSpec { property_iri: "rdfs:label".to_string(), direction: SortDirection::Desc };
        let (results, _) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("rdf:type", "ex:Thing", "exists")],
            false, usize::MAX, 0, Some(&sort),
        ).unwrap();

        assert_eq!(results.last().unwrap(), "ex:NoLabel");
    }

    #[test]
    fn missing_sort_property_value_sorts_first_for_asc() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:HasLabel", "ex:Thing");
        insert_type(&conn, "ex:NoLabel", "ex:Thing");
        insert_triple(&conn, "ex:HasLabel", "rdfs:label", "A");

        let sort = SortSpec { property_iri: "rdfs:label".to_string(), direction: SortDirection::Asc };
        let (results, _) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("rdf:type", "ex:Thing", "exists")],
            false, usize::MAX, 0, Some(&sort),
        ).unwrap();

        assert_eq!(results.first().unwrap(), "ex:NoLabel");
    }

    #[test]
    fn no_sort_spec_preserves_existing_behavior() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:X", "ex:Thing");
        insert_triple(&conn, "ex:X", "rdfs:label", "X");

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("rdfs:label", "X", "=")],
            false, usize::MAX, 0, None,
        ).unwrap();

        assert_eq!(total, 1);
        assert_eq!(results, vec!["ex:X"]);
    }

    #[test]
    fn sort_by_datetime_property_orders_by_epoch() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:Early", "ex:Thing");
        insert_type(&conn, "ex:Late", "ex:Thing");
        insert_triple(&conn, "ex:Early", "foundation:createdAt", "2020-01-01T00:00:00Z");
        insert_triple(&conn, "ex:Late", "foundation:createdAt", "2025-06-01T00:00:00Z");

        let sort = SortSpec {
            property_iri: "foundation:createdAt".to_string(),
            direction: SortDirection::Asc,
        };
        let (results, _) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("rdf:type", "ex:Thing", "exists")],
            false, usize::MAX, 0, Some(&sort),
        ).unwrap();

        assert_eq!(results, vec!["ex:Early", "ex:Late"]);
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    fn ensure_tx(conn: &Connection) -> i64 {
        if let Ok(id) = conn.query_row(
            "SELECT id FROM transactions WHERE origin = 'test' LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        ) {
            return id;
        }
        conn.execute("INSERT INTO transactions (origin, created_at) VALUES ('test', 0)", []).unwrap();
        conn.last_insert_rowid()
    }

    fn insert_type(conn: &Connection, subject: &str, class: &str) {
        let tx = ensure_tx(conn);
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, retracted, created_at) \
             VALUES (?1, 'rdf:type', ?2, 'iri', ?3, 1, 0, 0)",
            rusqlite::params![subject, class, tx],
        ).unwrap();
    }

    fn insert_decimal(conn: &Connection, subject: &str, predicate: &str, value: f64) {
        let tx = ensure_tx(conn);
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_datatype, object_number, \
             object_type, tx, origin_id, retracted, created_at) \
             VALUES (?1, ?2, ?3, 'xsd:decimal', ?4, 'literal', ?5, 1, 0, 0)",
            rusqlite::params![subject, predicate, value.to_string(), value, tx],
        ).unwrap();
    }

    fn insert_integer(conn: &Connection, subject: &str, predicate: &str, value: i64) {
        let tx = ensure_tx(conn);
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_datatype, object_integer, \
             object_type, tx, origin_id, retracted, created_at) \
             VALUES (?1, ?2, ?3, 'xsd:integer', ?4, 'literal', ?5, 1, 0, 0)",
            rusqlite::params![subject, predicate, value.to_string(), value, tx],
        ).unwrap();
    }

    #[test]
    fn numeric_eq_matches_decimal_stored_in_object_number() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:P1", "ex:Payment");
        insert_decimal(&conn, "ex:P1", "ex:amount", 128.7);
        insert_type(&conn, "ex:P2", "ex:Payment");
        insert_decimal(&conn, "ex:P2", "ex:amount", 200.0);

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Payment"],
            &[PropertyFilter::Compare("ex:amount", "128.7", "=")],
            false, 100, 0, None,
        ).unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"ex:P1".to_string()));
        assert!(!results.contains(&"ex:P2".to_string()));
    }

    #[test]
    fn numeric_gte_uses_numeric_comparison_not_lexicographic() {
        // Lexicographic: "9" > "128.7" (wrong). Numeric: 9.0 < 128.7 (correct).
        let conn = setup_test_db();
        insert_type(&conn, "ex:A", "ex:Thing");
        insert_decimal(&conn, "ex:A", "ex:val", 9.0);
        insert_type(&conn, "ex:B", "ex:Thing");
        insert_decimal(&conn, "ex:B", "ex:val", 100.0);
        insert_type(&conn, "ex:C", "ex:Thing");
        insert_decimal(&conn, "ex:C", "ex:val", 200.0);

        let (results, _) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("ex:val", "100.0", ">=")],
            false, 100, 0, None,
        ).unwrap();

        assert!(!results.contains(&"ex:A".to_string()), "9 < 100, must be excluded");
        assert!(results.contains(&"ex:B".to_string()), "100 >= 100, must be included");
        assert!(results.contains(&"ex:C".to_string()), "200 >= 100, must be included");
    }

    #[test]
    fn numeric_eq_matches_integer_stored_in_object_integer() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:X", "ex:Thing");
        insert_integer(&conn, "ex:X", "ex:count", 42);
        insert_type(&conn, "ex:Y", "ex:Thing");
        insert_integer(&conn, "ex:Y", "ex:count", 99);

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("ex:count", "42", "=")],
            false, 100, 0, None,
        ).unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"ex:X".to_string()));
        assert!(!results.contains(&"ex:Y".to_string()));
    }

    #[test]
    fn exists_filter_includes_only_entities_with_property() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:WithProp", "ex:Thing");
        insert_decimal(&conn, "ex:WithProp", "ex:amount", 100.0);
        insert_type(&conn, "ex:NoProp", "ex:Thing");

        let (results, _) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("ex:amount", "", "exists")],
            false, 100, 0, None,
        ).unwrap();

        assert!(results.contains(&"ex:WithProp".to_string()));
        assert!(!results.contains(&"ex:NoProp".to_string()));
    }

    #[test]
    fn not_exists_filter_includes_only_entities_without_property() {
        let conn = setup_test_db();
        insert_type(&conn, "ex:WithProp", "ex:Thing");
        insert_decimal(&conn, "ex:WithProp", "ex:amount", 100.0);
        insert_type(&conn, "ex:NoProp", "ex:Thing");

        let (results, _) = find_by_class_iris_and_properties_with_options(
            &conn, &["ex:Thing"],
            &[PropertyFilter::Compare("ex:amount", "", "not_exists")],
            false, 100, 0, None,
        ).unwrap();

        assert!(!results.contains(&"ex:WithProp".to_string()));
        assert!(results.contains(&"ex:NoProp".to_string()));
    }
}
