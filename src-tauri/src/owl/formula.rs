use turso::Connection;

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
pub async fn validate_no_cycle(
    conn: &Connection,
    property_iri: &str,
    formula: &str,
) -> Result<(), crate::owl::OwlError> {
    let refs = extract_references(formula);
    let mut stack: Vec<String> = vec![property_iri.to_string()];
    dfs_cycle_check(conn, property_iri, &refs, &mut stack).await?;
    Ok(())
}

async fn dfs_cycle_check(
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

        let dep_formula = query_formula(conn, dep).await;
        if let Some(f) = dep_formula {
            let sub_deps = extract_references(&f);
            Box::pin(dfs_cycle_check(conn, root, &sub_deps, stack)).await?;
        }

        stack.pop();
    }
    Ok(())
}

async fn query_formula(conn: &Connection, property_iri: &str) -> Option<String> {
    let mut stmt = conn.prepare(
        "SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:formula' AND retracted = 0 LIMIT 1"
    ).await.ok()?;
    let row = stmt.query_row(turso::params![property_iri]).await.ok()?;
    row.get_value(0).ok()?.as_text().cloned()
}

/// Evaluate a formula for a specific instance, substituting property values and computing the result.
pub async fn evaluate_formula_for_instance(
    conn: &Connection,
    instance_iri: &str,
    property_iri: &str,
) -> Result<String, String> {
    evaluate_formula_for_instance_raw(conn, instance_iri, property_iri).await
}

/// Loads the `foundation:formula` triple for `property_iri`, substitutes all `{{ref}}` tokens
/// with the corresponding literal values from the instance, and evaluates the resulting
/// arithmetic expression.
pub async fn evaluate_formula_for_instance_raw(
    conn: &Connection,
    instance_iri: &str,
    property_iri: &str,
) -> Result<String, String> {
    let formula = {
        let mut stmt = conn.prepare(
            "SELECT object_value FROM triples WHERE subject = ? AND predicate = 'foundation:formula' AND retracted = 0 LIMIT 1"
        ).await.map_err(|e| format!("Failed to load formula for {}: {}", property_iri, e))?;
        let row = stmt.query_row(turso::params![property_iri]).await
            .map_err(|e| format!("Failed to load formula for {}: {}", property_iri, e))?;
        row.get_value(0)
            .map_err(|e| format!("Failed to load formula for {}: {}", property_iri, e))?
            .as_text()
            .cloned()
            .ok_or_else(|| format!("Failed to load formula for {}: null value", property_iri))?
    };

    let refs = extract_references(&formula);
    let mut expr = formula.clone();

    for ref_iri in &refs {
        let value: Option<String> = {
            let mut stmt = conn.prepare(
                "SELECT object_value FROM triples WHERE subject = ? AND predicate = ? AND retracted = 0 LIMIT 1"
            ).await.ok();
            if let Some(mut s) = stmt.take() {
                s.query_row(turso::params![instance_iri, ref_iri.clone()]).await.ok()
                    .and_then(|row| row.get_value(0).ok()
                        .and_then(|v| match v {
                            turso::Value::Null => None,
                            other => other.as_text().cloned(),
                        }))
            } else {
                None
            }
        };

        match value {
            Some(v) => {
                let placeholder = format!("{{{{{}}}}}", ref_iri);
                expr = expr.replace(&placeholder, &v);
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
        Ok(result) => {
            if result.fract() == 0.0 && result.abs() < 1e15 {
                Ok(format!("{}", result as i64))
            } else {
                Ok(format!("{}", result))
            }
        }
        Err(reason) => Err(format!("Formula evaluation error: {}", reason)),
    }
}

/// Evaluate a simple arithmetic expression with `+`, `-`, `*`, `/` and proper precedence.
///
/// Uses recursive descent: addition/subtraction are lowest precedence, then
/// multiplication/division, then unary minus and parenthesised sub-expressions.
pub fn eval_expr(expr: &str) -> Result<f64, String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return Err("Empty expression".to_string());
    }

    if let Ok(n) = expr.parse::<f64>() {
        return Ok(n);
    }

    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut last_add_sub: Option<usize> = None;
    let mut last_mul_div: Option<usize> = None;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'+' | b'-' if depth == 0 && i > 0 => {
                last_add_sub = Some(i);
            }
            b'*' | b'/' if depth == 0 => {
                last_mul_div = Some(i);
            }
            _ => {}
        }
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

    if let Some(pos) = last_mul_div {
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
            _ => unreachable!(),
        };
    }

    if expr.starts_with('-') {
        return eval_expr(&expr[1..]).map(|v| -v);
    }

    if expr.starts_with('(') && expr.ends_with(')') {
        return eval_expr(&expr[1..expr.len() - 1]);
    }

    Err(format!("Cannot evaluate: '{}'", expr))
}

/// Sort the given property IRIs topologically so dependencies come before dependents.
///
/// Properties with no formula are treated as having no dependencies. If no formulas
/// exist among the given IRIs, the original order is preserved.
pub async fn topological_sort_properties(
    conn: &Connection,
    property_iris: &[&str],
) -> Vec<String> {
    let iris_set: std::collections::HashSet<&str> = property_iris.iter().copied().collect();

    let mut in_degree: std::collections::HashMap<String, usize> = property_iris
        .iter()
        .map(|iri| (iri.to_string(), 0))
        .collect();

    let mut adj: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for &iri in property_iris {
        if let Some(formula) = query_formula(conn, iri).await {
            for dep in extract_references(&formula) {
                if iris_set.contains(dep.as_str()) {
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

    async fn insert_tx(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 0)",
            (),
        )
        .await
        .unwrap();
        let mut s = conn.prepare("SELECT last_insert_rowid()").await.unwrap();
        let row = s.query_row(()).await.unwrap();
        row.get_value(0).unwrap().as_integer().copied().unwrap_or(0)
    }

    async fn insert_formula(conn: &Connection, tx: i64, property_iri: &str, formula: &str) {
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, 'foundation:formula', ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            turso::params![property_iri, formula, tx],
        )
        .await
        .unwrap();
    }

    async fn insert_value(conn: &Connection, tx: i64, instance_iri: &str, predicate: &str, value: &str) {
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, origin_id, tx, created_at, retracted) \
             VALUES (?, ?, ?, 'literal', 'xsd:string', 1, ?, 0, 0)",
            turso::params![instance_iri, predicate, value, tx],
        )
        .await
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
        let result = eval_expr("10 * 2 + 5 / 1 - 3").unwrap();
        assert!((result - 22.0).abs() < 1e-10);
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

    #[tokio::test]
    async fn test_validate_no_cycle_no_deps() {
        let conn = setup_test_db().await;
        assert!(validate_no_cycle(&conn, "p:A", "42").await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_no_cycle_linear_chain_no_cycle() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:A", "10").await;
        assert!(validate_no_cycle(&conn, "p:B", "{{p:A}} + 1").await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_no_cycle_direct_self_reference() {
        let conn = setup_test_db().await;
        let err = validate_no_cycle(&conn, "p:A", "{{p:A}} + 1").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Circular dependency"), "unexpected error: {}", msg);
        assert!(msg.contains("p:A"), "chain should mention p:A: {}", msg);
    }

    #[tokio::test]
    async fn test_validate_no_cycle_two_node_cycle() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:A", "{{p:B}}").await;
        let err = validate_no_cycle(&conn, "p:B", "{{p:A}}").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Circular dependency"), "unexpected error: {}", msg);
        assert!(msg.contains("p:B"), "chain should mention p:B: {}", msg);
        assert!(msg.contains("p:A"), "chain should mention p:A: {}", msg);
    }

    #[tokio::test]
    async fn test_validate_no_cycle_long_chain_cycle() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:A", "{{p:C}}").await;
        insert_formula(&conn, tx, "p:B", "{{p:A}}").await;
        let err = validate_no_cycle(&conn, "p:C", "{{p:B}}").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Circular dependency"), "unexpected error: {}", msg);
        assert!(msg.contains("p:A"), "chain should mention p:A: {}", msg);
        assert!(msg.contains("p:B"), "chain should mention p:B: {}", msg);
        assert!(msg.contains("p:C"), "chain should mention p:C: {}", msg);
    }

    #[tokio::test]
    async fn test_validate_no_cycle_long_chain_no_cycle() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:B", "{{p:A}}").await;
        insert_formula(&conn, tx, "p:C", "{{p:B}}").await;
        assert!(validate_no_cycle(&conn, "p:D", "{{p:C}}").await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_no_cycle_error_includes_full_chain() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:B", "{{p:A}}").await;
        let err = validate_no_cycle(&conn, "p:A", "{{p:B}}").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains('→') || msg.contains("->"), "missing chain separator: {}", msg);
        assert!(msg.contains("p:A"), "missing p:A in chain: {}", msg);
        assert!(msg.contains("p:B"), "missing p:B in chain: {}", msg);
    }

    // ── evaluate_formula_for_instance_raw ─────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_formula_basic_multiplication() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:area", "{{p:width}} * {{p:height}}").await;
        insert_value(&conn, tx, "inst:box", "p:width", "3").await;
        insert_value(&conn, tx, "inst:box", "p:height", "4").await;
        let result = evaluate_formula_for_instance_raw(&conn, "inst:box", "p:area").await.unwrap();
        assert_eq!(result, "12");
    }

    #[tokio::test]
    async fn test_evaluate_formula_integer_result_no_decimal() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:sum", "{{p:a}} + {{p:b}}").await;
        insert_value(&conn, tx, "inst:x", "p:a", "2").await;
        insert_value(&conn, tx, "inst:x", "p:b", "3").await;
        let result = evaluate_formula_for_instance_raw(&conn, "inst:x", "p:sum").await.unwrap();
        assert_eq!(result, "5");
        assert!(!result.contains('.'), "integer result should not contain decimal point: {}", result);
    }

    #[tokio::test]
    async fn test_evaluate_formula_float_result() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:ratio", "{{p:x}} / {{p:y}}").await;
        insert_value(&conn, tx, "inst:r", "p:x", "7").await;
        insert_value(&conn, tx, "inst:r", "p:y", "2").await;
        let result = evaluate_formula_for_instance_raw(&conn, "inst:r", "p:ratio").await.unwrap();
        let parsed: f64 = result.parse().expect("result should be a valid float");
        assert!((parsed - 3.5).abs() < 1e-10, "expected 3.5, got {}", result);
    }

    #[tokio::test]
    async fn test_evaluate_formula_missing_property_gives_descriptive_error() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:calc", "{{p:missing}} + 1").await;
        let err = evaluate_formula_for_instance_raw(&conn, "inst:obj", "p:calc").await.unwrap_err();
        assert!(err.contains("Missing value for {{p:missing}}"), "unexpected error: {}", err);
        assert!(err.contains("inst:obj"), "error should mention instance IRI: {}", err);
    }

    #[tokio::test]
    async fn test_evaluate_formula_no_formula_on_property_gives_error() {
        let conn = setup_test_db().await;
        let err = evaluate_formula_for_instance_raw(&conn, "inst:obj", "p:no_formula").await.unwrap_err();
        assert!(err.contains("Failed to load formula"), "unexpected error: {}", err);
    }

    #[tokio::test]
    async fn test_evaluate_formula_constant_no_refs() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:const", "42").await;
        let result = evaluate_formula_for_instance_raw(&conn, "inst:any", "p:const").await.unwrap();
        assert_eq!(result, "42");
    }

    // ── topological_sort_properties ───────────────────────────────────────────

    #[tokio::test]
    async fn test_topological_sort_no_formulas() {
        let conn = setup_test_db().await;
        let result = topological_sort_properties(&conn, &["p:c", "p:a", "p:b"]).await;
        assert_eq!(result, vec!["p:c", "p:a", "p:b"]);
    }

    #[tokio::test]
    async fn test_topological_sort_simple_dependency() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:b", "{{p:a}}").await;
        let result = topological_sort_properties(&conn, &["p:b", "p:a"]).await;
        let pos_a = result.iter().position(|s| s == "p:a").unwrap();
        let pos_b = result.iter().position(|s| s == "p:b").unwrap();
        assert!(pos_a < pos_b, "p:a must come before p:b, got: {:?}", result);
    }

    #[tokio::test]
    async fn test_topological_sort_chain() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:c", "{{p:b}}").await;
        insert_formula(&conn, tx, "p:b", "{{p:a}}").await;
        let result = topological_sort_properties(&conn, &["p:c", "p:b", "p:a"]).await;
        let pos_a = result.iter().position(|s| s == "p:a").unwrap();
        let pos_b = result.iter().position(|s| s == "p:b").unwrap();
        let pos_c = result.iter().position(|s| s == "p:c").unwrap();
        assert!(pos_a < pos_b, "p:a must come before p:b, got: {:?}", result);
        assert!(pos_b < pos_c, "p:b must come before p:c, got: {:?}", result);
    }

    #[tokio::test]
    async fn test_topological_sort_independent_properties_preserve_order() {
        let conn = setup_test_db().await;
        let tx = insert_tx(&conn).await;
        insert_formula(&conn, tx, "p:y", "{{p:z}}").await;
        let result = topological_sort_properties(&conn, &["p:x", "p:y", "p:z"]).await;
        let pos_z = result.iter().position(|s| s == "p:z").unwrap();
        let pos_y = result.iter().position(|s| s == "p:y").unwrap();
        assert!(pos_z < pos_y, "p:z must come before p:y, got: {:?}", result);
        assert!(result.contains(&"p:x".to_string()), "p:x must be present");
    }
}
