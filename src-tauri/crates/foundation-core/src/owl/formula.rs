use rusqlite::Connection;

/// Extract `{{...}}` references from a formula string, returning the property IRIs.
pub fn extract_references(formula: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut rest = formula;
    while let Some(start) = rest.find("{{") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("}}") {
            let iri = rest[..end].trim().to_string();
            if !iri.is_empty() {
                refs.push(iri);
            }
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    refs
}

/// Validate that adding `formula` to `property_iri` would not create a dependency cycle.
///
/// Uses DFS with a visited stack that preserves the full chain for error reporting.
pub fn validate_no_cycle(
    conn: &Connection,
    property_iri: &str,
    formula: &str,
) -> Result<(), crate::owl::OwlError> {
    let refs = extract_references(formula);
    let mut stack: Vec<String> = vec![property_iri.to_string()];
    dfs_cycle_check(conn, property_iri, &refs, &mut stack)?;
    Ok(())
}

fn dfs_cycle_check(
    conn: &Connection,
    root: &str,
    deps: &[String],
    stack: &mut Vec<String>,
) -> Result<(), crate::owl::OwlError> {
    for dep in deps {
        if stack.contains(dep) {
            let mut chain = stack.clone();
            chain.push(dep.clone());
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Circular dependency: {}",
                chain.join(" → ")
            )));
        }

        stack.push(dep.clone());

        if let Some(f) = query_formula(conn, dep) {
            let sub_deps = extract_references(&f);
            dfs_cycle_check(conn, root, &sub_deps, stack)?;
        }

        if let Some(agg) = query_aggregation(conn, dep) {
            if let Ok(call) = crate::owl::aggregation::parse_aggregation_call(&agg) {
                if let Some(sub_prop) = call.sub_prop {
                    dfs_cycle_check(conn, root, &[sub_prop], stack)?;
                }
            }
        }

        stack.pop();
    }
    Ok(())
}

fn query_formula(conn: &Connection, property_iri: &str) -> Option<String> {
    crate::eavto::query::get_by_entity_predicate(conn, property_iri, "foundation:formula")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_literal().map(|s| s.to_string()))
}

fn query_aggregation(conn: &Connection, property_iri: &str) -> Option<String> {
    crate::eavto::query::get_by_entity_predicate(conn, property_iri, "foundation:aggregation")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_literal().map(|s| s.to_string()))
}

/// Evaluate a formula for a specific instance, substituting property values and computing the result.
///
/// Uses `rusqlite::Connection` directly (same as `crate::eavto::Connection`).
pub fn evaluate_formula_for_instance(
    conn: &Connection,
    instance_iri: &str,
    property_iri: &str,
) -> Result<String, String> {
    evaluate_formula_for_instance_raw(conn, instance_iri, property_iri)
}

/// Resolve the value of a property for a given instance.
///
/// Tries the stored literal first. If absent, computes on-the-fly for aggregation
/// or formula properties so that formulas can reference other calculated fields
/// regardless of whether their cached value has been persisted yet.
fn resolve_ref_value(conn: &Connection, instance_iri: &str, ref_iri: &str) -> Option<String> {
    let stored = conn.query_row(
        "SELECT object_value FROM triples \
         WHERE subject = ? AND predicate = ? AND retracted = 0 AND is_current = 1 \
         LIMIT 1",
        rusqlite::params![instance_iri, ref_iri],
        |row| row.get::<_, Option<String>>(0),
    ).ok().flatten();

    if stored.is_some() {
        return stored;
    }

    if query_aggregation(conn, ref_iri).is_some() {
        return crate::owl::aggregation::evaluate_aggregation_for_instance(conn, instance_iri, ref_iri).ok();
    }

    if query_formula(conn, ref_iri).is_some() {
        return evaluate_formula_for_instance_raw(conn, instance_iri, ref_iri).ok();
    }

    // Numeric properties without a stored value default to zero — avoids formula failure
    if is_numeric_property(conn, ref_iri) {
        return Some("0".to_string());
    }

    None
}

fn is_numeric_property(conn: &Connection, property_iri: &str) -> bool {
    let range: Option<String> = crate::eavto::query::get_by_entity_predicate(conn, property_iri, "rdfs:range")
        .ok()
        .and_then(|r| r.triples.into_iter().next())
        .and_then(|t| t.object.as_iri().map(|s| s.to_string()));

    range.as_deref().map(|r| NUMERIC_RANGES.contains(&r)).unwrap_or(false)
}

/// Evaluate a formula for a specific instance using a raw `rusqlite::Connection`.
///
/// Loads the `foundation:formula` triple for `property_iri`, substitutes all `{{ref}}` tokens
/// with the corresponding literal values from the instance, and evaluates the resulting
/// arithmetic expression.
pub fn evaluate_formula_for_instance_raw(
    conn: &Connection,
    instance_iri: &str,
    property_iri: &str,
) -> Result<String, String> {
    let formula = query_formula(conn, property_iri)
        .ok_or_else(|| format!("Failed to load formula for {}", property_iri))?;

    let refs = extract_references(&formula);
    let mut expr = formula.clone();

    for ref_iri in &refs {
        match resolve_ref_value(conn, instance_iri, ref_iri) {
            Some(v) => {
                let placeholder = format!("{{{{{}}}}}", ref_iri);
                // Wrap in parentheses so negative values don't produce invalid infix
                // unary minus expressions (e.g. `a + -3` — eval_expr explicitly does not
                // support infix unary minus and would fail).
                let safe_value = format!("({})", v);
                expr = expr.replace(&placeholder, &safe_value);
            }
            None => {
                return Err(format!(
                    "Missing value for {{{{{}}}}} on instance {}",
                    ref_iri, instance_iri
                ));
            }
        }
    }

    match eval_expr(expr.trim()) {
        Ok(result) => Ok(format_calculated_number(result)),
        Err(reason) => Err(format!("Formula evaluation error: {}", reason)),
    }
}

/// Format an `f64` formula/aggregation result as a clean string,
/// suppressing the IEEE 754 noise typical of decimal sums.
///
/// Integers (including `42.0`) become `"42"` with no decimals. Non-integers are
/// rounded to 10 decimal places and trailing zeros are stripped
/// — this eliminates tails like `0.580000000024` while keeping enough precision
/// for most cases (currency, percentages, ratios).
pub fn format_calculated_number(value: f64) -> String {
    if !value.is_finite() {
        return format!("{}", value);
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let s = format!("{:.10}", value);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Evaluate a simple arithmetic expression with `+`, `-`, `*`, `/`, `%`, `**` and proper precedence.
///
/// Precedence (lowest to highest): +/- → */÷/% → ** → unary minus → parentheses.
/// `**` is left-associative. All operators require explicit operands on both sides;
/// infix unary minus (e.g. `10 + -3`) is not supported — use parentheses: `10 + (-3)`.
pub fn eval_expr(expr: &str) -> Result<f64, String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return Err("Empty expression".to_string());
    }

    // Try direct parse first
    if let Ok(n) = expr.parse::<f64>() {
        return Ok(n);
    }

    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut last_add_sub: Option<usize> = None;
    let mut last_mul_div_mod: Option<usize> = None;
    let mut last_pow: Option<usize> = None;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'+' | b'-' if depth == 0 && i > 0 => {
                last_add_sub = Some(i);
            }
            b'*' if depth == 0 => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    last_pow = Some(i);
                    i += 1; // skip second *
                } else {
                    last_mul_div_mod = Some(i);
                }
            }
            b'/' | b'%' if depth == 0 => {
                last_mul_div_mod = Some(i);
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(pos) = last_add_sub {
        let left = eval_expr(&expr[..pos])?;
        let right = eval_expr(&expr[pos + 1..])?;
        return match bytes[pos] {
            b'+' => Ok(left + right),
            b'-' => Ok(left - right),
            _ => unreachable!(),
        };
    }

    if let Some(pos) = last_mul_div_mod {
        let left = eval_expr(&expr[..pos])?;
        let right = eval_expr(&expr[pos + 1..])?;
        return match bytes[pos] {
            b'*' => Ok(left * right),
            b'/' => {
                if right == 0.0 {
                    Err("Division by zero".to_string())
                } else {
                    Ok(left / right)
                }
            }
            b'%' => {
                if right == 0.0 {
                    Err("Modulo by zero".to_string())
                } else {
                    Ok(left % right)
                }
            }
            _ => unreachable!(),
        };
    }

    if let Some(pos) = last_pow {
        let left = eval_expr(&expr[..pos])?;
        let right = eval_expr(&expr[pos + 2..])?; // skip both * chars
        return Ok(left.powf(right));
    }

    // Unary minus
    if expr.starts_with('-') {
        return eval_expr(&expr[1..]).map(|v| -v);
    }

    // Parentheses
    if expr.starts_with('(') && expr.ends_with(')') {
        return eval_expr(&expr[1..expr.len() - 1]);
    }

    Err(format!("Cannot evaluate: '{}'", expr))
}

/// Returns true if the string contains an aggregation function call (SOMA, MÉDIA, etc.).
/// Used to reject aggregation syntax in `foundation:formula` fields.
pub fn contains_aggregation_call(s: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "SOMA(", "SUM(",
        "MÉDIA(", "MEDIA(", "AVG(",
        "MÍNIMO(", "MINIMO(", "MIN(",
        "MÁXIMO(", "MAXIMO(", "MAX(",
        "CONTAR(", "COUNT(",
    ];
    PREFIXES.iter().any(|p| s.contains(p))
}

/// Validate that the formula expression is syntactically correct by substituting all
/// `{{ref}}` placeholders with `1` and performing a dry-run evaluation.
pub fn validate_expression(formula: &str) -> Result<(), crate::owl::OwlError> {
    if contains_aggregation_call(formula) {
        return Err(crate::owl::OwlError::ValidationError(
            "Fórmulas aritméticas não suportam chamadas de agregação (SOMA, MÉDIA, etc.). \
             Use o campo 'aggregation' para definir propriedades de agregação.".to_string()
        ));
    }
    let mut expr = formula.to_string();
    for ref_iri in extract_references(formula) {
        let placeholder = format!("{{{{{}}}}}", ref_iri);
        expr = expr.replace(&placeholder, "1");
    }
    eval_expr(expr.trim()).map(|_| ()).map_err(|e| {
        crate::owl::OwlError::ValidationError(format!("Invalid formula expression: {}", e))
    })
}

const NUMERIC_RANGES: &[&str] = &[
    "xsd:integer", "xsd:decimal", "xsd:float", "xsd:double",
    "xsd:int", "xsd:long", "xsd:short", "xsd:byte",
    "xsd:nonNegativeInteger", "xsd:positiveInteger",
];

/// Validate that every `{{ref}}` in the formula points to an existing property with a numeric range.
///
/// Aggregation and formula properties are always considered numeric regardless of their
/// declared `rdfs:range`, since their computed values are always numbers.
pub fn validate_references_numeric(
    conn: &Connection,
    formula: &str,
) -> Result<(), crate::owl::OwlError> {
    for ref_iri in extract_references(formula) {
        let exists = crate::eavto::query::get_by_entity_predicate(conn, &ref_iri, "rdf:type")
            .map(|r| !r.triples.is_empty())
            .unwrap_or(false);

        if !exists {
            return Err(crate::owl::OwlError::ValidationError(format!(
                "Referenced property '{}' does not exist",
                ref_iri
            )));
        }

        let is_computed = query_aggregation(conn, &ref_iri).is_some()
            || query_formula(conn, &ref_iri).is_some();

        if is_computed {
            continue;
        }

        let range: Option<String> = crate::eavto::query::get_by_entity_predicate(conn, &ref_iri, "rdfs:range")
            .ok()
            .and_then(|r| r.triples.into_iter().next())
            .and_then(|t| t.object.as_iri().map(|s| s.to_string()));

        if let Some(r) = range.as_deref() {
            if !NUMERIC_RANGES.contains(&r) {
                return Err(crate::owl::OwlError::ValidationError(format!(
                    "Referenced property '{}' has non-numeric range '{}'; formula references must be numeric",
                    ref_iri, r
                )));
            }
        }
    }
    Ok(())
}

/// Sort the given property IRIs topologically so dependencies come before dependents.
///
/// Properties with no formula are treated as having no dependencies. If no formulas
/// exist among the given IRIs, the original order is preserved.
pub fn topological_sort_properties(
    conn: &Connection,
    property_iris: &[&str],
) -> Vec<String> {
    let iris_set: std::collections::HashSet<&str> = property_iris.iter().copied().collect();

    // Build adjacency: dep -> dependents (dep must come before dependent)
    // We want properties that are dependencies to come first.
    // Kahn's algorithm: in-degree counts.

    let mut in_degree: std::collections::HashMap<String, usize> = property_iris
        .iter()
        .map(|iri| (iri.to_string(), 0))
        .collect();

    // adj[dep] = list of nodes that depend on dep
    let mut adj: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for &iri in property_iris {
        if let Some(formula) = query_formula(conn, iri) {
            for dep in extract_references(&formula) {
                if iris_set.contains(dep.as_str()) {
                    // iri depends on dep → dep must come first → dep -> iri edge
                    adj.entry(dep.clone()).or_default().push(iri.to_string());
                    *in_degree.entry(iri.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut queue: std::collections::VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(iri, _)| iri.clone())
        .collect();

    // Preserve original order for stable output among zero-in-degree nodes
    let order: std::collections::HashMap<&str, usize> = property_iris
        .iter()
        .enumerate()
        .map(|(i, &iri)| (iri, i))
        .collect();
    let mut queue_vec: Vec<String> = queue.drain(..).collect();
    queue_vec.sort_by_key(|iri| order.get(iri.as_str()).copied().unwrap_or(usize::MAX));
    let mut queue: std::collections::VecDeque<String> = queue_vec.into_iter().collect();

    let mut result = Vec::new();
    while let Some(node) = queue.pop_front() {
        if let Some(dependents) = adj.get(&node) {
            let mut next: Vec<String> = Vec::new();
            for dep in dependents {
                let deg = in_degree.entry(dep.clone()).or_insert(0);
                *deg -= 1;
                if *deg == 0 {
                    next.push(dep.clone());
                }
            }
            next.sort_by_key(|iri| order.get(iri.as_str()).copied().unwrap_or(usize::MAX));
            for n in next {
                queue.push_back(n);
            }
        }
        result.push(node);
    }

    // If there were cycles (result shorter than input), append remaining in original order
    if result.len() < property_iris.len() {
        for &iri in property_iris {
            if !result.contains(&iri.to_string()) {
                result.push(iri.to_string());
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    #[test]
    fn format_calculated_number_supresses_ieee_754_noise_on_decimal_sums() {
        // Regression: Bug_1777120091054 — f64 sums leak IEEE 754 noise
        // (e.g.: -100.50 + (-200.25) + ... = -20389.580000000024).
        assert_eq!(format_calculated_number(-20389.580000000024_f64), "-20389.58");
        assert_eq!(format_calculated_number(35554.910000000025_f64), "35554.91");
        assert_eq!(format_calculated_number(1114.729999999974_f64), "1114.73");
        assert_eq!(format_calculated_number(6062.209999999999_f64), "6062.21");
        assert_eq!(format_calculated_number(1372.8600000000006_f64), "1372.86");
    }

    #[test]
    fn format_calculated_number_preserves_integers_without_decimals() {
        assert_eq!(format_calculated_number(0.0), "0");
        assert_eq!(format_calculated_number(42.0), "42");
        assert_eq!(format_calculated_number(-7.0), "-7");
    }

    #[test]
    fn format_calculated_number_keeps_legitimate_fractional_precision() {
        assert_eq!(format_calculated_number(0.5), "0.5");
        assert_eq!(format_calculated_number(3.14159), "3.14159");
        assert_eq!(format_calculated_number(-0.001), "-0.001");
    }

    #[test]
    fn format_calculated_number_handles_non_finite() {
        // NaN/Infinity fall back to format!("{}", ...) even though the
        // product rarely produces them — the point is not to crash.
        let nan = format_calculated_number(f64::NAN);
        assert!(nan.contains("NaN"));
        assert_eq!(format_calculated_number(f64::INFINITY), "inf");
    }

    fn insert_tx(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_formula(conn: &Connection, tx: i64, property_iri: &str, formula: &str) {
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params![property_iri, formula, tx],
        )
        .unwrap();
    }

    fn insert_value(conn: &Connection, tx: i64, instance_iri: &str, predicate: &str, value: &str) {
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, ?, ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            rusqlite::params![instance_iri, predicate, value, tx],
        )
        .unwrap();
    }

    // ── extract_references ────────────────────────────────────────────────────

    #[test]
    fn test_extract_references() {
        let refs = extract_references("{{foundation:hasWidth}} * {{foundation:hasHeight}}");
        assert_eq!(refs, vec!["foundation:hasWidth", "foundation:hasHeight"]);
    }

    #[test]
    fn test_extract_references_empty() {
        let refs = extract_references("42");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_references_single() {
        let refs = extract_references("{{foundation:hasWidth}} + 5");
        assert_eq!(refs, vec!["foundation:hasWidth"]);
    }

    #[test]
    fn test_extract_references_trims_whitespace() {
        let refs = extract_references("{{ foundation:hasWidth }}");
        assert_eq!(refs, vec!["foundation:hasWidth"]);
    }

    #[test]
    fn test_extract_references_no_closing_brace() {
        let refs = extract_references("{{foundation:broken");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_references_duplicate() {
        let refs = extract_references("{{a}} + {{a}}");
        assert_eq!(refs, vec!["a", "a"]);
    }

    // ── eval_expr ─────────────────────────────────────────────────────────────

    #[test]
    fn test_eval_expr_literal() {
        assert_eq!(eval_expr("42").unwrap(), 42.0);
        assert_eq!(eval_expr("3.14").unwrap(), 3.14);
    }

    #[test]
    fn test_eval_expr_add() {
        assert_eq!(eval_expr("2 + 3").unwrap(), 5.0);
    }

    #[test]
    fn test_eval_expr_sub() {
        assert_eq!(eval_expr("10 - 4").unwrap(), 6.0);
    }

    #[test]
    fn test_eval_expr_mul() {
        assert_eq!(eval_expr("3 * 4").unwrap(), 12.0);
    }

    #[test]
    fn test_eval_expr_div() {
        assert_eq!(eval_expr("10 / 2").unwrap(), 5.0);
    }

    #[test]
    fn test_eval_expr_precedence() {
        assert_eq!(eval_expr("2 + 3 * 4").unwrap(), 14.0);
    }

    #[test]
    fn test_eval_expr_parens() {
        assert_eq!(eval_expr("(2 + 3) * 4").unwrap(), 20.0);
    }

    #[test]
    fn test_eval_expr_division_by_zero() {
        assert!(eval_expr("1 / 0").is_err());
    }

    #[test]
    fn test_eval_expr_unary_minus() {
        assert_eq!(eval_expr("-5").unwrap(), -5.0);
    }

    #[test]
    fn test_eval_expr_negative_in_expression() {
        // The evaluator does not support infix unary minus like "10 + -3";
        // use parentheses to wrap the negative operand.
        let result = eval_expr("10 + (-3)").unwrap();
        assert!((result - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_eval_expr_nested_parens() {
        let result = eval_expr("((2 + 3) * (4 - 1))").unwrap();
        assert!((result - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_eval_expr_float() {
        let result = eval_expr("1.5 * 2").unwrap();
        assert!((result - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_eval_expr_complex() {
        // 10*2=20, 5/1=5, 20+5-3=22
        let result = eval_expr("10 * 2 + 5 / 1 - 3").unwrap();
        assert!((result - 22.0).abs() < 1e-10);
    }

    #[test]
    fn test_eval_expr_pow() {
        assert_eq!(eval_expr("2 ** 10").unwrap(), 1024.0);
    }

    #[test]
    fn test_eval_expr_pow_precedence_over_mul() {
        // 3 * 2**4 = 3 * 16 = 48, not (3*2)**4 = 1296
        let result = eval_expr("3 * 2 ** 4").unwrap();
        assert!((result - 48.0).abs() < 1e-10, "expected 48, got {}", result);
    }

    #[test]
    fn test_eval_expr_mod() {
        assert_eq!(eval_expr("10 % 3").unwrap(), 1.0);
    }

    #[test]
    fn test_eval_expr_mod_by_zero() {
        assert!(eval_expr("5 % 0").is_err());
    }

    #[test]
    fn test_validate_expression_rejects_aggregation_call() {
        let err = validate_expression("SOMA({{p:items}}.p:value)").unwrap_err();
        assert!(
            err.to_string().contains("aggregation") || err.to_string().contains("agrega"),
            "error should mention aggregation: {}",
            err
        );
    }

    #[test]
    fn test_validate_expression_valid() {
        assert!(validate_expression("{{p:a}} * {{p:b}} + 10").is_ok());
    }

    #[test]
    fn test_validate_expression_gibberish() {
        assert!(validate_expression("foo bar ??").is_err());
    }

    #[test]
    fn test_validate_expression_constant() {
        assert!(validate_expression("42").is_ok());
    }

    #[test]
    fn test_eval_expr_empty() {
        assert!(eval_expr("").is_err());
    }

    #[test]
    fn test_eval_expr_invalid_token() {
        let err = eval_expr("abc").unwrap_err();
        assert!(err.contains("Cannot evaluate"), "unexpected error: {}", err);
    }

    // ── validate_no_cycle ─────────────────────────────────────────────────────

    #[test]
    fn test_validate_no_cycle_no_deps() {
        let conn = setup_test_db();
        assert!(validate_no_cycle(&conn, "p:A", "42").is_ok());
    }

    #[test]
    fn test_validate_no_cycle_linear_chain_no_cycle() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        // p:A has no formula; p:B depends on p:A — no cycle
        insert_formula(&conn, tx, "p:A", "10");
        assert!(validate_no_cycle(&conn, "p:B", "{{p:A}} + 1").is_ok());
    }

    #[test]
    fn test_validate_no_cycle_direct_self_reference() {
        let conn = setup_test_db();
        let err = validate_no_cycle(&conn, "p:A", "{{p:A}} + 1").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Circular dependency"), "unexpected error: {}", msg);
        assert!(msg.contains("p:A"), "chain should mention p:A: {}", msg);
    }

    #[test]
    fn test_validate_no_cycle_two_node_cycle() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        // p:A already has formula referencing p:B in DB
        insert_formula(&conn, tx, "p:A", "{{p:B}}");
        // Now try to set p:B to reference p:A → cycle: p:B → p:A → p:B
        let err = validate_no_cycle(&conn, "p:B", "{{p:A}}").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Circular dependency"), "unexpected error: {}", msg);
        assert!(msg.contains("p:B"), "chain should mention p:B: {}", msg);
        assert!(msg.contains("p:A"), "chain should mention p:A: {}", msg);
    }

    #[test]
    fn test_validate_no_cycle_long_chain_cycle() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        // p:A → p:C and p:B → p:A already in DB
        insert_formula(&conn, tx, "p:A", "{{p:C}}");
        insert_formula(&conn, tx, "p:B", "{{p:A}}");
        // Now validate p:C with formula "{{p:B}}" → p:C → p:B → p:A → p:C (cycle)
        let err = validate_no_cycle(&conn, "p:C", "{{p:B}}").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Circular dependency"), "unexpected error: {}", msg);
        assert!(msg.contains("p:A"), "chain should mention p:A: {}", msg);
        assert!(msg.contains("p:B"), "chain should mention p:B: {}", msg);
        assert!(msg.contains("p:C"), "chain should mention p:C: {}", msg);
    }

    #[test]
    fn test_validate_no_cycle_long_chain_no_cycle() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        // p:A has no formula; p:B → p:A; p:C → p:B
        insert_formula(&conn, tx, "p:B", "{{p:A}}");
        insert_formula(&conn, tx, "p:C", "{{p:B}}");
        // p:D → p:C is a new leaf — no cycle
        assert!(validate_no_cycle(&conn, "p:D", "{{p:C}}").is_ok());
    }

    #[test]
    fn test_validate_no_cycle_error_includes_full_chain() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_formula(&conn, tx, "p:B", "{{p:A}}");
        // p:A → p:B → p:A: chain must list all nodes
        let err = validate_no_cycle(&conn, "p:A", "{{p:B}}").unwrap_err();
        let msg = err.to_string();
        // The formatted chain must contain "→" separators and all three occurrences
        assert!(msg.contains('→') || msg.contains("->"), "missing chain separator: {}", msg);
        assert!(msg.contains("p:A"), "missing p:A in chain: {}", msg);
        assert!(msg.contains("p:B"), "missing p:B in chain: {}", msg);
    }

    // ── evaluate_formula_for_instance_raw ─────────────────────────────────────

    #[test]
    fn test_evaluate_formula_basic_multiplication() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_formula(&conn, tx, "p:area", "{{p:width}} * {{p:height}}");
        insert_value(&conn, tx, "inst:box", "p:width", "3");
        insert_value(&conn, tx, "inst:box", "p:height", "4");
        let result = evaluate_formula_for_instance_raw(&conn, "inst:box", "p:area").unwrap();
        assert_eq!(result, "12");
    }

    #[test]
    fn test_evaluate_formula_integer_result_no_decimal() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_formula(&conn, tx, "p:sum", "{{p:a}} + {{p:b}}");
        insert_value(&conn, tx, "inst:x", "p:a", "2");
        insert_value(&conn, tx, "inst:x", "p:b", "3");
        let result = evaluate_formula_for_instance_raw(&conn, "inst:x", "p:sum").unwrap();
        assert_eq!(result, "5");
        assert!(!result.contains('.'), "integer result should not contain decimal point: {}", result);
    }

    #[test]
    fn test_evaluate_formula_float_result() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_formula(&conn, tx, "p:ratio", "{{p:x}} / {{p:y}}");
        insert_value(&conn, tx, "inst:r", "p:x", "7");
        insert_value(&conn, tx, "inst:r", "p:y", "2");
        let result = evaluate_formula_for_instance_raw(&conn, "inst:r", "p:ratio").unwrap();
        let parsed: f64 = result.parse().expect("result should be a valid float");
        assert!((parsed - 3.5).abs() < 1e-10, "expected 3.5, got {}", result);
    }

    #[test]
    fn test_evaluate_formula_missing_property_gives_descriptive_error() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_formula(&conn, tx, "p:calc", "{{p:missing}} + 1");
        let err = evaluate_formula_for_instance_raw(&conn, "inst:obj", "p:calc").unwrap_err();
        assert!(err.contains("Missing value for {{p:missing}}"), "unexpected error: {}", err);
        assert!(err.contains("inst:obj"), "error should mention instance IRI: {}", err);
    }

    #[test]
    fn test_evaluate_formula_no_formula_on_property_gives_error() {
        let conn = setup_test_db();
        let err = evaluate_formula_for_instance_raw(&conn, "inst:obj", "p:no_formula").unwrap_err();
        assert!(err.contains("Failed to load formula"), "unexpected error: {}", err);
    }

    #[test]
    fn test_evaluate_formula_constant_no_refs() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_formula(&conn, tx, "p:const", "42");
        let result = evaluate_formula_for_instance_raw(&conn, "inst:any", "p:const").unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_evaluate_formula_with_negative_substituted_value() {
        // Regression: closingBalance = openingBalance + actualMargin
        // When actualMargin is negative (e.g. -29055.86), substitution produced
        // "20499.96 + -29055.86" which eval_expr rejects (no infix unary minus).
        // Substitution must wrap values in parentheses so the resulting expression
        // is parseable for any sign.
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_formula(&conn, tx, "p:closing", "{{p:opening}} + {{p:margin}}");
        insert_value(&conn, tx, "inst:budget", "p:opening", "20499.96");
        insert_value(&conn, tx, "inst:budget", "p:margin", "-29055.86");
        let result = evaluate_formula_for_instance_raw(&conn, "inst:budget", "p:closing").unwrap();
        let parsed: f64 = result.parse().expect("must parse as f64");
        assert!((parsed - (-8555.90)).abs() < 1e-2, "expected ~-8555.90, got {}", result);
    }

    // ── topological_sort_properties ───────────────────────────────────────────

    #[test]
    fn test_topological_sort_no_formulas() {
        let conn = setup_test_db();
        let result = topological_sort_properties(&conn, &["p:c", "p:a", "p:b"]);
        assert_eq!(result, vec!["p:c", "p:a", "p:b"]);
    }

    #[test]
    fn test_topological_sort_simple_dependency() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        // p:b depends on p:a → p:a must come first
        insert_formula(&conn, tx, "p:b", "{{p:a}}");
        let result = topological_sort_properties(&conn, &["p:b", "p:a"]);
        let pos_a = result.iter().position(|s| s == "p:a").unwrap();
        let pos_b = result.iter().position(|s| s == "p:b").unwrap();
        assert!(pos_a < pos_b, "p:a must come before p:b, got: {:?}", result);
    }

    #[test]
    fn test_topological_sort_chain() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        insert_formula(&conn, tx, "p:c", "{{p:b}}");
        insert_formula(&conn, tx, "p:b", "{{p:a}}");
        let result = topological_sort_properties(&conn, &["p:c", "p:b", "p:a"]);
        let pos_a = result.iter().position(|s| s == "p:a").unwrap();
        let pos_b = result.iter().position(|s| s == "p:b").unwrap();
        let pos_c = result.iter().position(|s| s == "p:c").unwrap();
        assert!(pos_a < pos_b, "p:a must come before p:b, got: {:?}", result);
        assert!(pos_b < pos_c, "p:b must come before p:c, got: {:?}", result);
    }

    #[test]
    fn test_topological_sort_independent_properties_preserve_order() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        // p:y depends on p:z; p:x is independent
        insert_formula(&conn, tx, "p:y", "{{p:z}}");
        let result = topological_sort_properties(&conn, &["p:x", "p:y", "p:z"]);
        let pos_z = result.iter().position(|s| s == "p:z").unwrap();
        let pos_y = result.iter().position(|s| s == "p:y").unwrap();
        assert!(pos_z < pos_y, "p:z must come before p:y, got: {:?}", result);
        assert!(result.contains(&"p:x".to_string()), "p:x must be present");
    }

    // ── contains_aggregation_call ─────────────────────────────────────────────

    #[test]
    fn test_contains_aggregation_call_true_for_soma() {
        assert!(contains_aggregation_call("SOMA({{p:x}})"));
    }

    #[test]
    fn test_contains_aggregation_call_true_for_sum() {
        assert!(contains_aggregation_call("SUM({{p:x}})"));
    }

    #[test]
    fn test_contains_aggregation_call_true_for_media() {
        assert!(contains_aggregation_call("MÉDIA({{p:x}})"));
        assert!(contains_aggregation_call("MEDIA({{p:x}})"));
        assert!(contains_aggregation_call("AVG({{p:x}})"));
    }

    #[test]
    fn test_contains_aggregation_call_true_for_min() {
        assert!(contains_aggregation_call("MÍNIMO({{p:x}})"));
        assert!(contains_aggregation_call("MINIMO({{p:x}})"));
        assert!(contains_aggregation_call("MIN({{p:x}})"));
    }

    #[test]
    fn test_contains_aggregation_call_true_for_max() {
        assert!(contains_aggregation_call("MÁXIMO({{p:x}})"));
        assert!(contains_aggregation_call("MAXIMO({{p:x}})"));
        assert!(contains_aggregation_call("MAX({{p:x}})"));
    }

    #[test]
    fn test_contains_aggregation_call_true_for_count() {
        assert!(contains_aggregation_call("CONTAR({{p:x}})"));
        assert!(contains_aggregation_call("COUNT({{p:x}})"));
    }

    #[test]
    fn test_contains_aggregation_call_false_for_plain_arithmetic() {
        assert!(!contains_aggregation_call("{{p:width}} * {{p:height}}"));
    }

    #[test]
    fn test_contains_aggregation_call_false_for_empty_string() {
        assert!(!contains_aggregation_call(""));
    }

    // ── validate_references_numeric ───────────────────────────────────────────

    #[test]
    fn test_validate_references_numeric_accepts_numeric_range() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, origin_id, tx, created_at, retracted) \
             VALUES ('p:width', 'rdf:type', 'owl:DatatypeProperty', 'iri', 1, ?, 0, 0)",
            rusqlite::params![tx],
        ).unwrap();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, origin_id, tx, created_at, retracted) \
             VALUES ('p:width', 'rdfs:range', 'xsd:integer', 'iri', 1, ?, 0, 0)",
            rusqlite::params![tx],
        ).unwrap();

        let result = validate_references_numeric(&conn, "{{p:width}} * 2");
        assert!(result.is_ok(), "formula referencing numeric property must pass");
    }

    #[test]
    fn test_validate_references_numeric_rejects_non_numeric_range() {
        let conn = setup_test_db();
        let tx = insert_tx(&conn);
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, origin_id, tx, created_at, retracted) \
             VALUES ('p:name', 'rdf:type', 'owl:DatatypeProperty', 'iri', 1, ?, 0, 0)",
            rusqlite::params![tx],
        ).unwrap();
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, origin_id, tx, created_at, retracted) \
             VALUES ('p:name', 'rdfs:range', 'xsd:string', 'iri', 1, ?, 0, 0)",
            rusqlite::params![tx],
        ).unwrap();

        let result = validate_references_numeric(&conn, "{{p:name}} + 1");
        assert!(result.is_err(), "formula referencing string property must be rejected");
    }

    #[test]
    fn test_validate_references_numeric_rejects_nonexistent_property() {
        let conn = setup_test_db();
        let result = validate_references_numeric(&conn, "{{p:ghost}} + 1");
        assert!(result.is_err(), "formula referencing non-existent property must be rejected");
    }

    #[test]
    fn test_validate_references_numeric_accepts_no_references() {
        let conn = setup_test_db();
        let result = validate_references_numeric(&conn, "42 + 8");
        assert!(result.is_ok(), "formula with no references must always pass");
    }
}
