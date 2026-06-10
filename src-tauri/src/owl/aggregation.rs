use rusqlite::Connection;

const NUMERIC_RANGES: &[&str] = &[
    "xsd:integer", "xsd:decimal", "xsd:float", "xsd:double",
    "xsd:int", "xsd:long", "xsd:short", "xsd:byte",
    "xsd:nonNegativeInteger", "xsd:positiveInteger",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggregationFunc {
    Sum,
    Avg,
    Min,
    Max,
    Count,
}

#[derive(Debug, Clone)]
pub struct AggregationCall {
    pub func: AggregationFunc,
    pub source_prop: String,
    pub sub_prop: Option<String>,
}

fn func_from_name(name: &str) -> Option<AggregationFunc> {
    match name {
        "SOMA" | "SUM" => Some(AggregationFunc::Sum),
        "MÉDIA" | "MEDIA" | "AVG" => Some(AggregationFunc::Avg),
        "MÍNIMO" | "MINIMO" | "MIN" => Some(AggregationFunc::Min),
        "MÁXIMO" | "MAXIMO" | "MAX" => Some(AggregationFunc::Max),
        "CONTAR" | "COUNT" => Some(AggregationFunc::Count),
        _ => None,
    }
}

/// Parse a single aggregation call from a string.
///
/// Accepted syntax:
/// - `SOMA({{sourceProp}}.subProp)` — Sum, Avg, Min, Max require a sub-property
/// - `CONTAR({{sourceProp}})` — Count does not accept a sub-property
///
/// Navigation direction (forward or inverse) is determined automatically at evaluation
/// time by comparing the instance type with the `rdfs:domain` of `sourceProp`.
pub fn parse_aggregation_call(s: &str) -> Result<AggregationCall, String> {
    let s = s.trim();

    let paren_pos = s.find('(').ok_or_else(|| format!(
        "Sintaxe de agregação inválida: '(' esperado em '{}'", s
    ))?;

    let func_name = s[..paren_pos].trim();
    let func = func_from_name(func_name).ok_or_else(|| format!(
        "Função de agregação desconhecida '{}'. Use: SOMA, MÉDIA, MÍNIMO, MÁXIMO, CONTAR \
         (ou SUM, AVG, MIN, MAX, COUNT)",
        func_name
    ))?;

    let after_paren = &s[paren_pos + 1..];
    let close_pos = after_paren.find(')').ok_or_else(|| format!(
        "Sintaxe de agregação inválida: ')' não encontrado em '{}'", s
    ))?;

    let after_call = after_paren[close_pos + 1..].trim();
    if !after_call.is_empty() {
        return Err(format!(
            "Campo de agregação aceita apenas uma chamada de agregação — \
             conteúdo inesperado após ')': '{}'",
            after_call
        ));
    }

    let inner = after_paren[..close_pos].trim();

    if !inner.starts_with("{{") {
        return Err(format!(
            "Sintaxe de agregação inválida: esperado '{{{{' após '(' em '{}'", s
        ));
    }
    let brace_end = inner.find("}}").ok_or_else(|| format!(
        "Sintaxe de agregação inválida: '}}}}' não encontrado em '{}'", s
    ))?;
    let source_prop = inner[2..brace_end].trim().to_string();
    if source_prop.is_empty() {
        return Err(format!("Propriedade de origem vazia em '{}'", s));
    }

    let after_brace = inner[brace_end + 2..].trim();

    let sub_prop = if after_brace.is_empty() {
        None
    } else if after_brace.starts_with('.') {
        let sp = after_brace[1..].trim().to_string();
        if sp.is_empty() {
            return Err(format!("Sub-propriedade vazia após '.' em '{}'", s));
        }
        Some(sp)
    } else {
        return Err(format!(
            "Sintaxe de agregação inválida: conteúdo inesperado após '}}}}': '{}' em '{}'",
            after_brace, s
        ));
    };

    match func {
        AggregationFunc::Count => {
            if sub_prop.is_some() {
                return Err(
                    "CONTAR/COUNT não aceita sub-propriedade. \
                     Use CONTAR({{prop}}) sem '.'".to_string()
                );
            }
        }
        _ => {
            if sub_prop.is_none() {
                return Err(format!(
                    "A função '{}' requer uma sub-propriedade numérica. \
                     Use {}({{{{prop}}}}.subProp)",
                    func_name, func_name
                ));
            }
        }
    }

    Ok(AggregationCall { func, source_prop, sub_prop })
}

/// Validate that `s` is a syntactically correct aggregation call.
pub fn validate_aggregation(s: &str) -> Result<(), crate::owl::OwlError> {
    parse_aggregation_call(s.trim())
        .map(|_| ())
        .map_err(crate::owl::OwlError::ValidationError)
}

fn validate_iri_format(iri: &str, role: &str) -> Result<(), crate::owl::OwlError> {
    if !iri.contains(':') {
        return Err(crate::owl::OwlError::ValidationError(format!(
            "{} '{}' não é um IRI válido; use o prefixo namespace (ex: foundation:{})",
            role, iri, iri
        )));
    }
    Ok(())
}

fn validate_iri_exists(
    conn: &Connection,
    iri: &str,
    role: &str,
) -> Result<(), crate::owl::OwlError> {
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM triples \
         WHERE subject = ?1 AND predicate = 'rdf:type' AND retracted = 0",
        rusqlite::params![iri],
        |row| row.get::<_, i64>(0),
    ).map(|c| c > 0).unwrap_or(false);

    if !exists {
        return Err(crate::owl::OwlError::ValidationError(format!(
            "{} '{}' não encontrada; verifique o IRI completo com prefixo namespace",
            role, iri
        )));
    }
    Ok(())
}

/// Validate that the sub-property of a Sum/Avg/Min/Max aggregation has a numeric range.
/// Count does not require a sub-property and is always accepted.
/// Both source_prop and sub_prop must be valid IRIs with namespace prefixes and must exist.
pub fn validate_aggregation_references(
    conn: &Connection,
    s: &str,
) -> Result<(), crate::owl::OwlError> {
    let call = parse_aggregation_call(s.trim())
        .map_err(crate::owl::OwlError::ValidationError)?;

    validate_iri_format(&call.source_prop, "Propriedade de origem")?;
    validate_iri_exists(conn, &call.source_prop, "Propriedade de origem")?;

    if call.func == AggregationFunc::Count {
        return Ok(());
    }

    let sub_prop = call.sub_prop.as_ref().ok_or_else(|| crate::owl::OwlError::ValidationError(
        "Sub-propriedade ausente para função de agregação não-Count".to_string()
    ))?;

    validate_iri_format(sub_prop, "Sub-propriedade")?;
    validate_iri_exists(conn, sub_prop, "Sub-propriedade")?;

    let range: Option<String> = conn.query_row(
        "SELECT object FROM triples \
         WHERE subject = ?1 AND predicate = 'rdfs:range' AND retracted = 0 LIMIT 1",
        rusqlite::params![sub_prop],
        |row| row.get::<_, Option<String>>(0),
    ).unwrap_or(None);

    if let Some(r) = range.as_deref() {
        if !NUMERIC_RANGES.contains(&r) {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Sub-propriedade '{}' tem range não-numérico '{}'; \
                 SOMA/MÉDIA/MÍNIMO/MÁXIMO exigem sub-propriedade numérica",
                sub_prop, r
            )));
        }
    }

    Ok(())
}

/// Determines whether navigation should be inverse by comparing the instance type
/// with the property's `rdfs:domain`.
///
/// - If the instance type matches the domain → forward navigation (subject = instance)
/// - Otherwise → inverse navigation (object = instance)
fn is_inverse_navigation(conn: &Connection, instance_iri: &str, source_prop: &str) -> bool {
    let domain: Option<String> = conn.query_row(
        "SELECT object FROM triples \
         WHERE subject = ? AND predicate = 'rdfs:domain' AND retracted = 0 \
         ORDER BY tx DESC LIMIT 1",
        rusqlite::params![source_prop],
        |row| row.get(0),
    ).ok();

    let instance_type: Option<String> = conn.query_row(
        "SELECT object FROM triples \
         WHERE subject = ? AND predicate = 'rdf:type' AND retracted = 0 \
         ORDER BY tx DESC LIMIT 1",
        rusqlite::params![instance_iri],
        |row| row.get(0),
    ).ok();

    match (domain, instance_type) {
        (Some(d), Some(t)) => d != t,
        _ => false,
    }
}

/// Evaluate an aggregation formula for a specific instance.
///
/// Loads the `foundation:aggregation` triple for `property_iri`, parses it,
/// queries all related instances via `source_prop`, then applies the aggregation function.
pub fn evaluate_aggregation_for_instance(
    conn: &Connection,
    instance_iri: &str,
    property_iri: &str,
) -> Result<String, String> {
    let formula = conn.query_row(
        "SELECT object_value FROM triples \
         WHERE subject = ? AND predicate = 'foundation:aggregation' AND retracted = 0 \
         ORDER BY tx DESC LIMIT 1",
        rusqlite::params![property_iri],
        |row| row.get::<_, String>(0),
    ).map_err(|e| format!("Falha ao carregar agregação para {}: {}", property_iri, e))?;

    let call = parse_aggregation_call(formula.trim())
        .map_err(|e| format!("Fórmula de agregação inválida '{}': {}", formula, e))?;

    let inverse = is_inverse_navigation(conn, instance_iri, &call.source_prop);

    let related_iris: Vec<String> = if inverse {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT subject FROM triples \
             WHERE predicate = ?1 AND object = ?2 AND retracted = 0",
        ).map_err(|e| format!("Erro na query inversa: {}", e))?;
        let rows = stmt.query_map(rusqlite::params![call.source_prop, instance_iri], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("Erro na query inversa: {}", e))?;
        rows.filter_map(|r| r.ok()).collect()
    } else {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT object FROM triples \
             WHERE subject = ?1 AND predicate = ?2 AND retracted = 0 AND object IS NOT NULL \
               AND is_current = 1",
        ).map_err(|e| format!("Erro na query: {}", e))?;
        let rows = stmt.query_map(rusqlite::params![instance_iri, call.source_prop], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("Erro na query: {}", e))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    if call.func == AggregationFunc::Count {
        return Ok(format!("{}", related_iris.len() as i64));
    }

    let sub_prop = call.sub_prop.as_ref().ok_or_else(|| {
        "Sub-propriedade ausente para função de agregação não-Count".to_string()
    })?;
    let mut values: Vec<f64> = Vec::new();

    for related_iri in &related_iris {
        let val: Option<String> = conn.query_row(
            "SELECT object_value FROM triples \
             WHERE subject = ? AND predicate = ? AND retracted = 0 \
             ORDER BY tx DESC LIMIT 1",
            rusqlite::params![related_iri, sub_prop],
            |row| row.get::<_, Option<String>>(0),
        ).unwrap_or(None);

        match val {
            Some(v) => {
                let n: f64 = v.parse().map_err(|_| format!(
                    "Sub-propriedade '{}' valor '{}' em '{}' não é numérico",
                    sub_prop, v, related_iri
                ))?;
                values.push(n);
            }
            None => {}
        }
    }

    if values.is_empty() {
        return match call.func {
            AggregationFunc::Sum => Ok("0".to_string()),
            _ => Err(format!(
                "Não é possível computar MÉDIA/MÍNIMO/MÁXIMO: \
                 nenhum relacionado encontrado via '{}' em '{}'",
                call.source_prop, instance_iri
            )),
        };
    }

    let result = match call.func {
        AggregationFunc::Sum => values.iter().sum(),
        AggregationFunc::Avg => values.iter().sum::<f64>() / values.len() as f64,
        AggregationFunc::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
        AggregationFunc::Max => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        AggregationFunc::Count => unreachable!(),
    };

    Ok(crate::owl::formula::format_calculated_number(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    fn insert_tx(conn: &Connection) -> i64 {
        conn.execute("INSERT INTO transactions (origin, created_at) VALUES ('test', 0)", [])
            .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_aggregation(conn: &Connection, tx: i64, property_iri: &str, formula: &str) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object_value, object_type, \
              object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'foundation:aggregation', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params![property_iri, formula, tx],
        ).unwrap();
    }

    fn insert_object_ref(
        conn: &Connection, tx: i64, subject: &str, predicate: &str, object: &str,
    ) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, \
              object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, ?, ?, 'iri', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params![subject, predicate, object, tx],
        ).unwrap();
    }

    fn insert_numeric_value(
        conn: &Connection, tx: i64, subject: &str, predicate: &str, value: &str,
    ) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object_value, object_type, \
              object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, ?, ?, 'literal', 'xsd:decimal', 1, ?, 0, 0)",
            rusqlite::params![subject, predicate, value, tx],
        ).unwrap();
    }

    fn insert_range(conn: &Connection, tx: i64, property_iri: &str, range: &str) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, \
              object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'rdfs:range', ?, 'iri', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params![property_iri, range, tx],
        ).unwrap();
    }

    fn insert_rdf_type(conn: &Connection, tx: i64, subject: &str, rdf_type: &str) {
        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, \
              object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'rdf:type', ?, 'iri', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params![subject, rdf_type, tx],
        ).unwrap();
    }

    #[test]
    fn test_evaluate_aggregation_sum() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:total", "SOMA({{p:hasItem}}.p:value)");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child2");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child3");
        insert_numeric_value(&conn, tx, "inst:child1", "p:value", "10");
        insert_numeric_value(&conn, tx, "inst:child2", "p:value", "20");
        insert_numeric_value(&conn, tx, "inst:child3", "p:value", "30");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:total").unwrap();
        assert_eq!(result, "60");
    }

    #[test]
    fn test_evaluate_aggregation_avg() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:avg", "MÉDIA({{p:hasItem}}.p:value)");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child2");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child3");
        insert_numeric_value(&conn, tx, "inst:child1", "p:value", "10");
        insert_numeric_value(&conn, tx, "inst:child2", "p:value", "20");
        insert_numeric_value(&conn, tx, "inst:child3", "p:value", "30");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:avg").unwrap();
        let parsed: f64 = result.parse().unwrap();
        assert!((parsed - 20.0).abs() < 1e-10, "expected 20.0, got {}", result);
    }

    #[test]
    fn test_evaluate_aggregation_min() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:minval", "MÍNIMO({{p:hasItem}}.p:value)");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child2");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child3");
        insert_numeric_value(&conn, tx, "inst:child1", "p:value", "10");
        insert_numeric_value(&conn, tx, "inst:child2", "p:value", "3");
        insert_numeric_value(&conn, tx, "inst:child3", "p:value", "7");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:minval").unwrap();
        assert_eq!(result, "3");
    }

    #[test]
    fn test_evaluate_aggregation_max() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:maxval", "MÁXIMO({{p:hasItem}}.p:value)");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child2");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child3");
        insert_numeric_value(&conn, tx, "inst:child1", "p:value", "10");
        insert_numeric_value(&conn, tx, "inst:child2", "p:value", "3");
        insert_numeric_value(&conn, tx, "inst:child3", "p:value", "7");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:maxval").unwrap();
        assert_eq!(result, "10");
    }

    #[test]
    fn test_evaluate_aggregation_count() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:count", "CONTAR({{p:hasItem}})");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child2");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child3");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child4");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:count").unwrap();
        assert_eq!(result, "4");
    }

    #[test]
    fn test_evaluate_aggregation_avg_empty_fails() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:avg", "MÉDIA({{p:hasItem}}.p:value)");

        let err = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:avg").unwrap_err();
        assert!(
            err.to_lowercase().contains("média") || err.to_lowercase().contains("media")
                || err.contains("related") || err.contains("relacionad"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_evaluate_aggregation_missing_subprop_skips() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:total", "SOMA({{p:hasItem}}.p:missing)");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:total").unwrap();
        assert_eq!(result, "0");
    }

    #[test]
    fn test_evaluate_aggregation_partial_missing_subprop_sums_present_values() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:total", "SOMA({{p:hasItem}}.p:value)");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child2");
        insert_numeric_value(&conn, tx, "inst:child1", "p:value", "100");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:total").unwrap();
        assert_eq!(result, "100");
    }

    #[test]
    fn test_validate_aggregation_valid_syntax_soma() {
        assert!(validate_aggregation("SOMA({{p:items}}.p:value)").is_ok());
    }

    #[test]
    fn test_validate_aggregation_valid_syntax_count() {
        assert!(validate_aggregation("CONTAR({{p:members}})").is_ok());
    }

    #[test]
    fn test_validate_aggregation_valid_syntax_english() {
        assert!(validate_aggregation("SUM({{p:items}}.p:value)").is_ok());
        assert!(validate_aggregation("COUNT({{p:items}})").is_ok());
        assert!(validate_aggregation("AVG({{p:items}}.p:value)").is_ok());
        assert!(validate_aggregation("MIN({{p:items}}.p:value)").is_ok());
        assert!(validate_aggregation("MAX({{p:items}}.p:value)").is_ok());
    }

    #[test]
    fn test_validate_aggregation_rejects_arithmetic() {
        let err = validate_aggregation("SOMA({{p:items}}.p:value) + 5").unwrap_err();
        assert!(!err.to_string().is_empty(), "should return a non-empty error");
    }

    #[test]
    fn test_validate_aggregation_references_accepts_object_property_source() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_rdf_type(&conn, tx, "p:sourceProp", "owl:ObjectProperty");
        insert_rdf_type(&conn, tx, "p:subProp", "owl:DatatypeProperty");
        insert_range(&conn, tx, "p:subProp", "xsd:decimal");
        let result = validate_aggregation_references(&conn, "SOMA({{p:sourceProp}}.p:subProp)");
        assert!(result.is_ok(), "should accept object property as source: {:?}", result);
    }

    #[test]
    fn test_validate_aggregation_references_rejects_bare_source_prop() {
        let conn = setup_test_db();
        let err = validate_aggregation_references(
            &conn, "SOMA({{bareSourceProp}}.p:subProp)",
        ).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("bareSourceProp") && (msg.contains("IRI") || msg.contains("prefixo")),
            "error should mention invalid IRI: {}", msg
        );
    }

    #[test]
    fn test_validate_aggregation_references_rejects_bare_sub_prop() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_rdf_type(&conn, tx, "p:sourceProp", "owl:ObjectProperty");
        let err = validate_aggregation_references(
            &conn, "SOMA({{p:sourceProp}}.bareSubProp)",
        ).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("bareSubProp") && (msg.contains("IRI") || msg.contains("prefixo")),
            "error should mention invalid IRI: {}", msg
        );
    }

    #[test]
    fn test_validate_aggregation_references_rejects_nonexistent_source_prop() {
        let conn = setup_test_db();
        let err = validate_aggregation_references(
            &conn, "SOMA({{p:nonexistent}}.p:subProp)",
        ).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("p:nonexistent")
                && (msg.contains("encontrada") || msg.contains("not found")),
            "error should mention nonexistent property: {}", msg
        );
    }

    #[test]
    fn test_validate_aggregation_references_count_accepts_bare_source_iri_format_check() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_rdf_type(&conn, tx, "p:members", "owl:ObjectProperty");
        let result = validate_aggregation_references(&conn, "CONTAR({{p:members}})");
        assert!(result.is_ok(), "CONTAR with valid source_prop should pass: {:?}", result);
    }

    #[test]
    fn test_parse_aggregation_call_soma() {
        let call = parse_aggregation_call("SOMA({{p:items}}.p:value)").unwrap();
        assert_eq!(call.func, AggregationFunc::Sum);
        assert_eq!(call.source_prop, "p:items");
        assert_eq!(call.sub_prop.as_deref(), Some("p:value"));
    }

    #[test]
    fn test_parse_aggregation_call_count_no_subprop() {
        let call = parse_aggregation_call("CONTAR({{p:members}})").unwrap();
        assert_eq!(call.func, AggregationFunc::Count);
        assert_eq!(call.source_prop, "p:members");
        assert!(call.sub_prop.is_none());
    }

    #[test]
    fn test_parse_aggregation_call_rejects_count_with_subprop() {
        let err = parse_aggregation_call("CONTAR({{p:members}}.p:name)").unwrap_err();
        assert!(!err.is_empty(), "should return non-empty error");
    }

    #[test]
    fn test_parse_aggregation_call_rejects_sum_without_subprop() {
        let err = parse_aggregation_call("SOMA({{p:items}})").unwrap_err();
        assert!(!err.is_empty(), "should return non-empty error");
    }

    #[test]
    fn test_parse_aggregation_call_rejects_unknown_function() {
        let err = parse_aggregation_call("UNKNOWN({{p:items}}.p:value)").unwrap_err();
        assert!(!err.is_empty(), "should return non-empty error");
    }

    #[test]
    fn test_evaluate_aggregation_sum_returns_integer_when_whole() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:total", "SOMA({{p:hasItem}}.p:value)");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");
        insert_numeric_value(&conn, tx, "inst:child1", "p:value", "5");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:total").unwrap();
        assert!(!result.contains('.'), "integer result should not have decimal point: {}", result);
        assert_eq!(result, "5");
    }

    #[test]
    fn test_evaluate_aggregation_avg_returns_decimal() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_aggregation(&conn, tx, "p:avg", "MÉDIA({{p:hasItem}}.p:value)");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child1");
        insert_object_ref(&conn, tx, "inst:parent", "p:hasItem", "inst:child2");
        insert_numeric_value(&conn, tx, "inst:child1", "p:value", "1");
        insert_numeric_value(&conn, tx, "inst:child2", "p:value", "2");

        let result = evaluate_aggregation_for_instance(&conn, "inst:parent", "p:avg").unwrap();
        let parsed: f64 = result.parse().unwrap();
        assert!((parsed - 1.5).abs() < 1e-10, "expected 1.5, got {}", result);
    }
}
