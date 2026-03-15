use turso::{Connection, Value};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub async fn find_by_class_and_properties(
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

    let mut stmt = conn.prepare(&query).await?;
    let mut rows = stmt.query(()).await?;
    let mut entities = Vec::new();
    while let Some(row) = rows.next().await? {
        entities.push(row.get_value(0)?.as_text().cloned().unwrap_or_default());
    }

    Ok(entities)
}

pub async fn find_entities_by_class_with_date_range(
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

    let mut stmt = conn.prepare(&sql).await?;

    let mut params: Vec<Value> = vec![Value::Text(class_iri.to_string())];
    if let Some(from) = from_millis {
        params.push(Value::Integer(from));
    }
    if let Some(to) = to_millis {
        params.push(Value::Integer(to));
    }

    let p = turso::params_from_iter(params.clone().into_iter());
    let mut rows = stmt.query(p).await?;
    let mut entities = Vec::new();
    while let Some(row) = rows.next().await? {
        entities.push(row.get_value(0)?.as_text().cloned().unwrap_or_default());
    }

    Ok(entities)
}

pub async fn find_by_class_iris_and_properties_with_options(
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
    let mut where_clause = format!(
        "WHERE t0.predicate = 'rdf:type' AND t0.object IN ({class_placeholders}){type_retracted_filter}"
    );
    let mut params: Vec<Value> = class_iris.iter()
        .map(|iri| Value::Text(iri.to_string()))
        .collect();

    for (i, _) in properties.iter().enumerate() {
        let n = i + 1;
        joins.push_str(&format!("\n         INNER JOIN triples t{n} ON t0.subject = t{n}.subject"));
    }

    for (i, (prop_iri, _, _)) in properties.iter().enumerate() {
        let n = i + 1;
        let prop_retracted_filter = if include_retracted {
            String::new()
        } else {
            format!(" AND t{n}.retracted = 0")
        };
        where_clause.push_str(&format!("\n           AND t{n}.predicate = ?{prop_retracted_filter}"));
        params.push(Value::Text(prop_iri.to_string()));
    }

    for (i, (_, value, operator)) in properties.iter().enumerate() {
        let n = i + 1;
        if let Some(date_filter) = normalize_date_filter(value) {
            let sql_op = validate_operator(operator)
                .map_err(|_| format!("Invalid operator '{operator}': must be one of =, >=, <=, >, <"))?;
            match date_filter {
                DateFilter::Date(date_str) => {
                    where_clause.push_str(&format!(
                        "\n           AND substr(t{n}.object_value, 1, 10) {sql_op} ?"
                    ));
                    params.push(Value::Text(date_str));
                }
                DateFilter::DateTime(epoch) => {
                    where_clause.push_str(&format!(
                        "\n           AND unixepoch(t{n}.object_value) {sql_op} ?"
                    ));
                    params.push(Value::Integer(epoch));
                }
            }
        } else if *value == "true" || *value == "false" {
            let bool_val: i64 = if *value == "true" { 1 } else { 0 };
            where_clause.push_str(&format!(
                "\n           AND (t{n}.object_value = ? OR t{n}.object = ? OR t{n}.object_boolean = ?)"
            ));
            params.push(Value::Text(value.to_string()));
            params.push(Value::Text(value.to_string()));
            params.push(Value::Integer(bool_val));
        } else {
            where_clause.push_str(&format!(
                "\n           AND (t{n}.object_value = ? OR t{n}.object = ?)"
            ));
            params.push(Value::Text(value.to_string()));
            params.push(Value::Text(value.to_string()));
        }
    }

    let count_query = format!(
        "SELECT COUNT(*) FROM (SELECT DISTINCT t0.subject FROM triples t0{joins}\n         {where_clause})"
    );
    let p = turso::params_from_iter(params.clone().into_iter());
    let mut count_stmt = conn.prepare(&count_query).await?;
    let count_row = count_stmt.query_row(p).await?;
    let total: usize = count_row.get_value(0)?.as_integer().copied().unwrap_or(0) as usize;

    let mut data_params = params;
    data_params.push(Value::Integer(limit as i64));
    data_params.push(Value::Integer(offset as i64));

    let data_query = format!(
        "SELECT DISTINCT t0.subject FROM triples t0{joins}\n         {where_clause}\n         LIMIT ? OFFSET ?"
    );
    let p = turso::params_from_iter(data_params.into_iter());
    let mut stmt = conn.prepare(&data_query).await?;
    let mut rows = stmt.query(p).await?;
    let mut entities = Vec::new();
    while let Some(row) = rows.next().await? {
        entities.push(row.get_value(0)?.as_text().cloned().unwrap_or_default());
    }

    Ok((entities, total))
}

pub async fn find_by_properties_with_options(
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
    let mut where_clause = String::new();
    let mut params: Vec<Value> = Vec::new();

    for (i, _) in properties.iter().enumerate().skip(1) {
        joins.push_str(&format!("\n         INNER JOIN triples t{i} ON t0.subject = t{i}.subject"));
    }

    for (i, (prop_iri, _, _)) in properties.iter().enumerate() {
        let retracted_filter = if include_retracted {
            String::new()
        } else {
            format!(" AND t{i}.retracted = 0")
        };
        let connector = if i == 0 { "WHERE" } else { "  AND" };
        where_clause.push_str(&format!("\n         {connector} t{i}.predicate = ?{retracted_filter}"));
        params.push(Value::Text(prop_iri.to_string()));
    }

    for (i, (_, value, operator)) in properties.iter().enumerate() {
        if let Some(date_filter) = normalize_date_filter(value) {
            let sql_op = validate_operator(operator)
                .map_err(|_| format!("Invalid operator '{operator}': must be one of =, >=, <=, >, <"))?;
            match date_filter {
                DateFilter::Date(date_str) => {
                    where_clause.push_str(&format!(
                        "\n           AND substr(t{i}.object_value, 1, 10) {sql_op} ?"
                    ));
                    params.push(Value::Text(date_str));
                }
                DateFilter::DateTime(epoch) => {
                    where_clause.push_str(&format!(
                        "\n           AND unixepoch(t{i}.object_value) {sql_op} ?"
                    ));
                    params.push(Value::Integer(epoch));
                }
            }
        } else if *value == "true" || *value == "false" {
            let bool_val: i64 = if *value == "true" { 1 } else { 0 };
            where_clause.push_str(&format!(
                "\n           AND (t{i}.object_value = ? OR t{i}.object = ? OR t{i}.object_boolean = ?)"
            ));
            params.push(Value::Text(value.to_string()));
            params.push(Value::Text(value.to_string()));
            params.push(Value::Integer(bool_val));
        } else {
            where_clause.push_str(&format!(
                "\n           AND (t{i}.object_value = ? OR t{i}.object = ?)"
            ));
            params.push(Value::Text(value.to_string()));
            params.push(Value::Text(value.to_string()));
        }
    }

    let count_query = format!(
        "SELECT COUNT(*) FROM (SELECT DISTINCT t0.subject FROM triples t0{joins}{where_clause})"
    );
    let p = turso::params_from_iter(params.clone().into_iter());
    let mut count_stmt = conn.prepare(&count_query).await?;
    let count_row = count_stmt.query_row(p).await?;
    let total: usize = count_row.get_value(0)?.as_integer().copied().unwrap_or(0) as usize;

    let mut data_params = params;
    data_params.push(Value::Integer(limit as i64));
    data_params.push(Value::Integer(offset as i64));

    let data_query = format!(
        "SELECT DISTINCT t0.subject FROM triples t0{joins}{where_clause}\n         LIMIT ? OFFSET ?"
    );
    let p = turso::params_from_iter(data_params.into_iter());
    let mut stmt = conn.prepare(&data_query).await?;
    let mut rows = stmt.query(p).await?;
    let mut entities = Vec::new();
    while let Some(row) = rows.next().await? {
        entities.push(row.get_value(0)?.as_text().cloned().unwrap_or_default());
    }

    Ok((entities, total))
}

pub async fn find_message_iris_by_conversation(
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
    let mut stmt = conn.prepare(sql).await?;
    let mut rows = stmt.query(turso::params![conversation_iri, limit_i64, offset as i64]).await?;
    let mut iris = Vec::new();
    while let Some(row) = rows.next().await? {
        iris.push(row.get_value(0)?.as_text().cloned().unwrap_or_default());
    }
    Ok(iris)
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
        "=" | ">=" | "<=" | ">" | "<" => Ok(op),
        _ => Err(()),
    }
}

#[allow(dead_code)]
pub async fn find_entities_by_attribute_value(
    conn: &Connection,
    attribute: &str,
    value: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT subject
         FROM triples
         WHERE predicate = ? AND object_value = ? AND retracted = 0"
    ).await?;

    let mut rows = stmt.query(turso::params![attribute, value]).await?;
    let mut entities = Vec::new();
    while let Some(row) = rows.next().await? {
        entities.push(row.get_value(0)?.as_text().cloned().unwrap_or_default());
    }

    Ok(entities)
}
