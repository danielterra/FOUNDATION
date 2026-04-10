use std::collections::{HashMap, HashSet, VecDeque};

pub type ExecutionContext = HashMap<String, String>;

pub type FlowMap = HashMap<String, Vec<(String, Option<String>)>>;

pub(super) fn evaluate_condition_ctx(expr: &str, ctx: &ExecutionContext) -> bool {
    let expr = expr.trim();

    if let Some(dot_pos) = expr.find(".includes(") {
        let key = expr[..dot_pos].trim();
        let rest = expr[dot_pos + ".includes(".len()..].trim_end_matches(')');
        let value = rest.trim().trim_matches('\'').trim_matches('"');
        return ctx.get(key).map(|v| v.contains(value)).unwrap_or(false);
    }

    for op in &["===", "!==", "==", "!="] {
        if let Some(op_pos) = expr.find(op) {
            let key = expr[..op_pos].trim();
            let val = expr[op_pos + op.len()..].trim().trim_matches('\'').trim_matches('"');
            let ctx_val = ctx.get(key).map(|s| s.as_str()).unwrap_or("");
            return if op.starts_with('!') { ctx_val != val } else { ctx_val == val };
        }
    }

    if let Some(key) = expr.strip_prefix('!') {
        return ctx.get(key.trim()).map(|v| v.is_empty()).unwrap_or(true);
    }

    ctx.get(expr).map(|v| !v.is_empty()).unwrap_or(false)
}

async fn resolve_entity_property(
    ctx_key: &str,
    prop_name: &str,
    ctx: &ExecutionContext,
    executor: &crate::owl::DbExecutor,
) -> Option<String> {
    let entity_iri = ctx.get(ctx_key)?.clone();
    if !entity_iri.contains(':') || entity_iri.contains(' ') {
        return None;
    }
    let prop_iri = format!("foundation:{}", prop_name);
    executor.read(move |conn| {
        if let Ok(Some(v)) = crate::owl::get_iri_property(conn, &entity_iri, &prop_iri) {
            return Ok(Some(v));
        }
        crate::owl::get_literal_property(conn, &entity_iri, &prop_iri)
            .map_err(|e| e.to_string())
    }).await.ok().flatten()
}

pub(super) async fn evaluate_condition(
    expr: &str,
    ctx: &ExecutionContext,
    executor: &crate::owl::DbExecutor,
) -> bool {
    let expr_trimmed = expr.trim();

    if let Some(dot_pos) = expr_trimmed.find('.') {
        let potential_key = expr_trimmed[..dot_pos].trim();
        let rest = expr_trimmed[dot_pos + 1..].trim();

        if !rest.starts_with("includes(") && ctx.contains_key(potential_key) {
            for op in &["===", "!==", "==", "!="] {
                if let Some(op_pos) = rest.find(op) {
                    let prop_name = rest[..op_pos].trim();
                    let value = rest[op_pos + op.len()..].trim().trim_matches('\'').trim_matches('"');
                    if let Some(entity_val) = resolve_entity_property(potential_key, prop_name, ctx, executor).await {
                        return if op.starts_with('!') { entity_val != value } else { entity_val == value };
                    }
                }
            }
        }
    }

    evaluate_condition_ctx(expr_trimmed, ctx)
}

pub(super) fn reachable_from(start: &str, adjacency: &FlowMap) -> HashSet<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(start.to_string());
    while let Some(node) = queue.pop_front() {
        if visited.insert(node.clone()) {
            if let Some(neighbors) = adjacency.get(&node) {
                for (target, _) in neighbors {
                    queue.push_back(target.clone());
                }
            }
        }
    }
    visited
}

pub fn interpolate(template: &str, ctx: &ExecutionContext) -> String {
    let mut result = template.to_string();
    for (key, value) in ctx {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

pub async fn interpolate_with_db(
    template: &str,
    ctx: &ExecutionContext,
    executor: &crate::owl::DbExecutor,
) -> String {
    let mut result = template.to_string();
    let mut search = template;
    while let Some(start) = search.find("{{") {
        let rest = &search[start + 2..];
        let Some(end) = rest.find("}}") else { break };
        let key = rest[..end].trim();
        let placeholder = format!("{{{{{}}}}}", key);
        search = &rest[end + 2..];

        if let Some(dot) = key.find('.') {
            let ctx_key = key[..dot].trim();
            let prop_raw = key[dot + 1..].trim();
            let prop_iri = if prop_raw.contains(':') {
                prop_raw.to_string()
            } else {
                format!("foundation:{}", prop_raw)
            };
            if let Some(entity_iri) = ctx.get(ctx_key).cloned() {
                if entity_iri.contains(':') && !entity_iri.contains(' ') {
                    let value = executor.read(move |conn| {
                        if let Ok(Some(v)) = crate::owl::get_iri_property(conn, &entity_iri, &prop_iri) {
                            return Ok(Some(v));
                        }
                        crate::owl::get_literal_property(conn, &entity_iri, &prop_iri)
                            .map_err(|e| e.to_string())
                    }).await.ok().flatten();
                    if let Some(v) = value {
                        result = result.replace(&placeholder, &v);
                    }
                }
            }
        } else if let Some(value) = ctx.get(key) {
            result = result.replace(&placeholder, value);
        }
    }
    result
}
