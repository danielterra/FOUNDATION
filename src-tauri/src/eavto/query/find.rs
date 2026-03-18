use rusqlite::Connection;
use rusqlite::types::Value as SqlValue;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
         FROM triples t0"
    );

    for (i, _) in properties.iter().enumerate() {
        let table_num = i + 1;
        query.push_str(&format!(
            "\n         INNER JOIN triples t{} ON t0.subject = t{}.subject",
            table_num, table_num
        ));
    }

    query.push_str(&format!(
        "\n         WHERE t0.predicate = 'rdf:type'
           AND t0.object = '{}'
           AND t0.retracted = 0",
        class_iri
    ));

    for (i, (prop_iri, _)) in properties.iter().enumerate() {
        let table_num = i + 1;
        query.push_str(&format!(
            "\n           AND t{}.predicate = '{}'
           AND t{}.retracted = 0",
            table_num, prop_iri, table_num
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

pub fn find_by_class_iris_and_properties_with_options(
    conn: &Connection,
    class_iris: &[&str],
    properties: &[(&str, &str, &str)],
    include_retracted: bool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<String>, usize)> {
    if properties.is_empty() || class_iris.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let type_retracted_filter = if include_retracted { "" } else { " AND t0.retracted = 0" };

    let class_placeholders = class_iris.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let mut joins = String::new();
    let mut join_params: Vec<SqlValue> = Vec::new();
    let mut where_clause = format!(
        "WHERE t0.predicate = 'rdf:type' \
         AND t0.object IN ({class_placeholders}){type_retracted_filter}"
    );
    let mut where_params: Vec<SqlValue> = class_iris.iter()
        .map(|iri| SqlValue::Text(iri.to_string()))
        .collect();

    for (i, (prop_iri, _, operator)) in properties.iter().enumerate() {
        let n = i + 1;
        let optional = is_optional_op(operator);
        let prop_retracted_filter = if include_retracted {
            String::new()
        } else {
            format!(" AND t{n}.retracted = 0")
        };

        if optional {
            joins.push_str(&format!(
                "\n         LEFT JOIN triples t{n} ON t0.subject = t{n}.subject \
                 AND t{n}.predicate = ?{prop_retracted_filter}"
            ));
            join_params.push(SqlValue::Text(prop_iri.to_string()));
        } else {
            joins.push_str(&format!(
                "\n         INNER JOIN triples t{n} ON t0.subject = t{n}.subject"
            ));
            where_clause.push_str(&format!(
                "\n           AND t{n}.predicate = ?{prop_retracted_filter}"
            ));
            where_params.push(SqlValue::Text(prop_iri.to_string()));
        }
    }

    for (i, (_, value, operator)) in properties.iter().enumerate() {
        let n = i + 1;
        let optional = is_optional_op(operator);
        let op = base_op(operator);

        let value_cond = build_value_condition_fragment(n, value, op, &mut where_params)?;

        if optional {
            where_clause.push_str(&format!(
                "\n           AND (t{n}.predicate IS NULL OR {value_cond})"
            ));
        } else {
            where_clause.push_str(&format!("\n           AND {value_cond}"));
        }
    }

    // params order must match SQL: JOIN params appear before WHERE params in the query
    let params: Vec<SqlValue> = join_params.into_iter().chain(where_params).collect();

    let count_query = format!(
        "SELECT COUNT(*) FROM \
         (SELECT DISTINCT t0.subject FROM triples t0{joins}\n         {where_clause})"
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
        "SELECT DISTINCT t0.subject FROM triples t0{joins}\n         \
         {where_clause}\n         LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&data_query)?;
    let entities: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(data_params.iter()), |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok((entities, total))
}

/// Find entities matching all given property filters, without any class restriction.
pub fn find_by_properties_with_options(
    conn: &Connection,
    properties: &[(&str, &str, &str)],
    include_retracted: bool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<String>, usize)> {
    if properties.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let mut joins = String::new();
    let mut join_params: Vec<SqlValue> = Vec::new();
    let mut where_clause = String::new();
    let mut where_params: Vec<SqlValue> = Vec::new();

    for (i, (prop_iri, _, operator)) in properties.iter().enumerate() {
        let retracted_filter = if include_retracted {
            String::new()
        } else {
            format!(" AND t{i}.retracted = 0")
        };
        let optional = is_optional_op(operator);

        if i == 0 {
            where_clause.push_str(&format!(
                "\n         WHERE t{i}.predicate = ?{retracted_filter}"
            ));
            where_params.push(SqlValue::Text(prop_iri.to_string()));
        } else if optional {
            joins.push_str(&format!(
                "\n         LEFT JOIN triples t{i} ON t0.subject = t{i}.subject \
                 AND t{i}.predicate = ?{retracted_filter}"
            ));
            join_params.push(SqlValue::Text(prop_iri.to_string()));
        } else {
            joins.push_str(&format!(
                "\n         INNER JOIN triples t{i} ON t0.subject = t{i}.subject"
            ));
            where_clause.push_str(&format!(
                "\n           AND t{i}.predicate = ?{retracted_filter}"
            ));
            where_params.push(SqlValue::Text(prop_iri.to_string()));
        }
    }

    for (i, (_, value, operator)) in properties.iter().enumerate() {
        let optional = is_optional_op(operator);
        let op = base_op(operator);

        let value_cond = build_value_condition_fragment(i, value, op, &mut where_params)?;

        if optional && i > 0 {
            where_clause.push_str(&format!(
                "\n           AND (t{i}.predicate IS NULL OR {value_cond})"
            ));
        } else {
            where_clause.push_str(&format!("\n           AND {value_cond}"));
        }
    }

    // params order must match SQL: JOIN params appear before WHERE params in the query
    let params: Vec<SqlValue> = join_params.into_iter().chain(where_params).collect();

    let count_query = format!(
        "SELECT COUNT(*) FROM (SELECT DISTINCT t0.subject FROM triples t0{joins}{where_clause})"
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
        "SELECT DISTINCT t0.subject FROM triples t0{joins}{where_clause}\n         LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&data_query)?;
    let entities: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(data_params.iter()), |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok((entities, total))
}

pub fn find_message_iris_by_conversation(
    conn: &Connection,
    conversation_iri: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<String>> {
    let sql = "
        SELECT subject FROM (
            SELECT t_type.subject, MAX(t_sent.object_value) AS ts
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
        Ok(format!("(t{n}.object_value = ? OR t{n}.object = ?)"))
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
         FROM triples
         WHERE predicate = ? AND object_value = ? AND retracted = 0"
    )?;

    let entities: Vec<String> = stmt
        .query_map([attribute, value], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(entities)
}
