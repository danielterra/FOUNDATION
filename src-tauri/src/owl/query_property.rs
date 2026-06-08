use serde::{Deserialize, Serialize};
use rusqlite::Connection;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryConfig {
    pub target_class: String,
    pub filters: Vec<QueryFilter>,
    #[serde(default)]
    pub order_by: Vec<QueryOrderBy>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryFilter {
    pub property_iri: String,
    pub operator: String,
    pub value: Option<String>,
    pub value_from: Option<String>,
    pub value_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryOrderBy {
    pub property_iri: String,
    #[serde(default = "default_order_direction")]
    pub direction: String,
}

fn default_order_direction() -> String {
    "asc".to_string()
}

pub fn parse_query_config(json: &str) -> Result<QueryConfig, String> {
    serde_json::from_str(json).map_err(|e| format!("Invalid JSON for queryConfig: {}", e))
}

pub fn validate_query_config(
    conn: &Connection,
    config: &QueryConfig,
) -> Result<(), crate::owl::OwlError> {
    let class_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM triples_current \
         WHERE subject = ? AND predicate = 'rdf:type' AND object IN ('owl:Class', 'rdfs:Class')",
        rusqlite::params![config.target_class],
        |row| row.get(0),
    ).unwrap_or(false);

    if !class_exists {
        return Err(crate::owl::OwlError::ValidationError(format!(
            "targetClass '{}' does not exist in the graph", config.target_class
        )));
    }

    const VALID_OPERATORS: &[&str] = &[
        "eq", "neq", "gt", "lt", "gte", "lte", "between", "exists", "not_exists",
    ];

    for filter in &config.filters {
        if !VALID_OPERATORS.contains(&filter.operator.as_str()) {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Invalid filter operator: '{}'. Accepted operators: {}",
                filter.operator,
                VALID_OPERATORS.join(", ")
            )));
        }

        if filter.operator == "exists" || filter.operator == "not_exists" {
            continue;
        }
        let prop_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM triples_current WHERE subject = ? AND predicate = 'rdf:type'",
            rusqlite::params![filter.property_iri],
            |row| row.get(0),
        ).unwrap_or(false);

        if !prop_exists {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "property_iri '{}' in filters does not exist in the graph", filter.property_iri
            )));
        }
    }

    for ob in &config.order_by {
        let dir = ob.direction.to_ascii_lowercase();
        if dir != "asc" && dir != "desc" {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Invalid direction in orderBy: '{}'. Use 'asc' or 'desc'", ob.direction
            )));
        }
        let prop_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM triples_current WHERE subject = ? AND predicate = 'rdf:type'",
            rusqlite::params![ob.property_iri],
            |row| row.get(0),
        ).unwrap_or(false);
        if !prop_exists {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "property_iri '{}' in orderBy does not exist in the graph", ob.property_iri
            )));
        }
    }

    Ok(())
}

fn extract_self_ref(value: &str) -> Option<&str> {
    let v = value.trim();
    if v.starts_with("{{self.") && v.ends_with("}}") {
        Some(&v[7..v.len() - 2])
    } else {
        None
    }
}

fn resolve_self_value(conn: &Connection, owner_iri: &str, prop_iri: &str) -> Option<String> {
    conn.query_row(
        "SELECT COALESCE(object, object_value) FROM triples_current \
         WHERE subject = ? AND predicate = ? LIMIT 1",
        rusqlite::params![owner_iri, prop_iri],
        |row| row.get::<_, String>(0),
    ).ok()
}

fn is_numeric_str(v: &str) -> bool {
    v.parse::<f64>().is_ok()
}

/// Resolves a raw filter value string. Returns None if it is a self-ref that cannot be resolved
/// (which means the query should return an empty result set).
fn resolve_value(conn: &Connection, owner_iri: &str, raw: &str) -> Option<String> {
    if let Some(prop_ref) = extract_self_ref(raw) {
        resolve_self_value(conn, owner_iri, prop_ref)
    } else {
        Some(raw.to_string())
    }
}

pub fn evaluate_query(
    conn: &Connection,
    owner_iri: &str,
    config: &QueryConfig,
) -> Result<Vec<String>, crate::owl::OwlError> {
    let mut sql = String::from(
        "SELECT DISTINCT tc.subject FROM triples_current tc \
         WHERE tc.predicate = 'rdf:type' AND tc.object = ?"
    );
    let mut params: Vec<String> = vec![config.target_class.clone()];

    for filter in &config.filters {
        let op = filter.operator.as_str();
        let prop = &filter.property_iri;

        match op {
            "exists" => {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM triples_current f \
                     WHERE f.subject = tc.subject AND f.predicate = ?)"
                );
                params.push(prop.clone());
            }
            "not_exists" => {
                sql.push_str(
                    " AND NOT EXISTS (SELECT 1 FROM triples_current f \
                     WHERE f.subject = tc.subject AND f.predicate = ?)"
                );
                params.push(prop.clone());
            }
            "between" => {
                let from_raw = filter.value_from.as_deref().unwrap_or("");
                let to_raw = filter.value_to.as_deref().unwrap_or("");

                let Some(from_val) = resolve_value(conn, owner_iri, from_raw) else {
                    return Ok(vec![]);
                };
                let Some(to_val) = resolve_value(conn, owner_iri, to_raw) else {
                    return Ok(vec![]);
                };

                let cmp_expr = if is_numeric_str(&from_val) {
                    "COALESCE(CAST(f.object_integer AS REAL), f.object_number, CAST(f.object_value AS REAL)) \
                     BETWEEN CAST(? AS REAL) AND CAST(? AS REAL)"
                } else {
                    "COALESCE(f.object, f.object_value) BETWEEN ? AND ?"
                };
                sql.push_str(&format!(
                    " AND EXISTS (SELECT 1 FROM triples_current f \
                     WHERE f.subject = tc.subject AND f.predicate = ? AND {})",
                    cmp_expr
                ));
                params.push(prop.clone());
                params.push(from_val);
                params.push(to_val);
            }
            cmp_op => {
                let sql_op = match cmp_op {
                    "eq" => "=",
                    "neq" => "!=",
                    "gt" => ">",
                    "lt" => "<",
                    "gte" => ">=",
                    "lte" => "<=",
                    unknown => return Err(crate::owl::OwlError::ValidationError(format!(
                        "Unknown filter operator: '{}'", unknown
                    ))),
                };

                let raw_val = filter.value.as_deref().unwrap_or("");
                let Some(resolved) = resolve_value(conn, owner_iri, raw_val) else {
                    return Ok(vec![]);
                };

                let cmp_expr = if is_numeric_str(&resolved) {
                    format!(
                        "COALESCE(CAST(f.object_integer AS REAL), f.object_number, CAST(f.object_value AS REAL)) \
                         {} CAST(? AS REAL)",
                        sql_op
                    )
                } else {
                    format!("COALESCE(f.object, f.object_value) {} ?", sql_op)
                };

                sql.push_str(&format!(
                    " AND EXISTS (SELECT 1 FROM triples_current f \
                     WHERE f.subject = tc.subject AND f.predicate = ? AND {})",
                    cmp_expr
                ));
                params.push(prop.clone());
                params.push(resolved);
            }
        }
    }

    if !config.order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        let mut parts: Vec<String> = Vec::with_capacity(config.order_by.len());
        for ob in &config.order_by {
            let dir = if ob.direction.eq_ignore_ascii_case("desc") { "DESC" } else { "ASC" };
            params.push(ob.property_iri.clone());
            parts.push(format!(
                "(SELECT COALESCE(f.object, f.object_value) FROM triples_current f \
                 WHERE f.subject = tc.subject AND f.predicate = ? LIMIT 1) {}",
                dir
            ));
        }
        sql.push_str(&parts.join(", "));
    }

    if let Some(limit) = config.limit {
        sql.push_str(&format!(" LIMIT {}", limit));
    }

    let mut stmt = conn.prepare(&sql)
        .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;

    let rows = stmt.query_map(
        rusqlite::params_from_iter(params.iter()),
        |row| row.get::<_, String>(0),
    ).map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

impl QueryConfig {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "query_property_tests.rs"]
mod tests;
