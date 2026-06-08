use handlebars::Handlebars;
use serde_json::{Value};
use std::collections::HashMap;

use crate::owl::{
    materialize_individual_shallow, get_iri_property, get_literal_property,
    DbExecutor, ShallowValue,
};
use super::context::ExecutionContext;

/// Builds a `serde_json::Value::Object` from a shallow materialization of `iri`.
/// - Literal values → JSON string scalar (single) or array.
/// - IRI values → expanded one level (their own shallow props as a nested object).
async fn build_shallow_json(iri: &str, executor: &DbExecutor) -> Value {
    let iri_owned = iri.to_string();
    let shallow = executor
        .read(move |conn| Ok(materialize_individual_shallow(conn, &iri_owned)))
        .await
        .unwrap_or_default();

    shallow_map_to_json(shallow, executor).await
}

async fn shallow_map_to_json(
    shallow: HashMap<String, Vec<ShallowValue>>,
    executor: &DbExecutor,
) -> Value {
    let mut obj = serde_json::Map::new();

    for (predicate, values) in &shallow {
        if values.is_empty() {
            continue;
        }

        if values.len() == 1 {
            match &values[0] {
                ShallowValue::Literal(s) => {
                    obj.insert(predicate.clone(), Value::String(s.clone()));
                }
                ShallowValue::Iri(target_iri) => {
                    let expanded = expand_one_level(target_iri, executor).await;
                    obj.insert(predicate.clone(), expanded);
                }
            }
        } else {
            let mut items = Vec::with_capacity(values.len());
            for sv in values {
                let v = match sv {
                    ShallowValue::Literal(s) => Value::String(s.clone()),
                    ShallowValue::Iri(target_iri) => {
                        expand_one_level(target_iri, executor).await
                    }
                };
                items.push(v);
            }
            obj.insert(predicate.clone(), Value::Array(items));
        }
    }

    Value::Object(obj)
}

/// Expands a single IRI one level: fetches its shallow properties and returns
/// them as a JSON object (literals + IRI strings, not recursed further).
/// Injects `"@iri"` so templates can reference the IRI itself inside `{{#each}}`.
/// Returns a plain string value when the entity has no triples.
async fn expand_one_level(iri: &str, executor: &DbExecutor) -> Value {
    let iri_owned = iri.to_string();
    let shallow = executor
        .read(move |conn| Ok(materialize_individual_shallow(conn, &iri_owned)))
        .await
        .unwrap_or_default();

    if shallow.is_empty() {
        return Value::String(iri.to_string());
    }

    let mut obj = serde_json::Map::new();
    obj.insert("@iri".to_string(), Value::String(iri.to_string()));

    for (predicate, values) in &shallow {
        if values.is_empty() {
            continue;
        }
        if values.len() == 1 {
            let v = match &values[0] {
                ShallowValue::Literal(s) | ShallowValue::Iri(s) => Value::String(s.clone()),
            };
            obj.insert(predicate.clone(), v);
        } else {
            let arr: Vec<Value> = values
                .iter()
                .map(|sv| match sv {
                    ShallowValue::Literal(s) | ShallowValue::Iri(s) => Value::String(s.clone()),
                })
                .collect();
            obj.insert(predicate.clone(), Value::Array(arr));
        }
    }

    Value::Object(obj)
}

/// Renders `template` using Handlebars with a context derived from the
/// execution context and — when `ctx["self"]` is set — from the shallow
/// materialization of that control instance.
///
/// Context root layout:
/// - All properties of ctx["self"] as top-level keys (predicate → scalar/array).
/// - `"self"` key = IRI string of the control instance.
/// - All flat keys from `ctx` injected at the root (compat with plain `{{key}}`).
///
/// Registered helpers:
/// - `resolve <base> "pred1" "pred2" ...` — walks object properties segment by
///   segment from `base` (IRI, `self`, or a ctx key); resolves the last segment
///   as IRI-property or literal-property and writes the value.
///
/// HTML escaping is disabled via `no_escape`: templates produce URLs, JSON and
/// chat text — the default `&`→`&amp;` escaping would corrupt those outputs.
pub async fn render(
    template: &str,
    ctx: &ExecutionContext,
    executor: &DbExecutor,
) -> String {
    let self_iri = ctx.get("self").cloned().unwrap_or_default();

    let mut root_obj = serde_json::Map::new();

    if !self_iri.is_empty() {
        let entity_json = build_shallow_json(&self_iri, executor).await;
        if let Value::Object(entity_map) = entity_json {
            for (k, v) in entity_map {
                root_obj.insert(k, v);
            }
        }
    }

    root_obj.insert("self".to_string(), Value::String(self_iri.clone()));

    for (k, v) in ctx {
        root_obj.entry(k.clone()).or_insert_with(|| Value::String(v.clone()));
    }

    let root = Value::Object(root_obj);

    let mut hb = Handlebars::new();
    hb.register_escape_fn(handlebars::no_escape);

    let self_iri_clone = self_iri.clone();
    let executor_clone = executor.clone();

    hb.register_helper(
        "resolve",
        Box::new(
            move |h: &handlebars::Helper,
                  _hb: &Handlebars,
                  ctx_hb: &handlebars::Context,
                  _rc: &mut handlebars::RenderContext,
                  out: &mut dyn handlebars::Output|
                  -> handlebars::HelperResult {
                let params = h.params();
                if params.is_empty() {
                    return Ok(());
                }

                let (start_iri, seg_offset) = {
                    let first = params[0].value().as_str().unwrap_or("").to_string();
                    if first.contains(':') && !first.contains(' ') {
                        (first, 1usize)
                    } else if first == "self" || first.is_empty() {
                        (self_iri_clone.clone(), 1usize)
                    } else {
                        let from_ctx = ctx_hb
                            .data()
                            .get(&first)
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        (from_ctx, 1usize)
                    }
                };

                if start_iri.is_empty() || params.len() <= seg_offset {
                    return Ok(());
                }

                let segs: Vec<String> = params[seg_offset..]
                    .iter()
                    .filter_map(|p| p.value().as_str())
                    .map(|s| s.to_string())
                    .collect();

                if segs.is_empty() {
                    return Ok(());
                }

                let exec = executor_clone.clone();
                let resolved = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async move {
                        exec.read(move |conn| {
                            let mut current = start_iri.clone();
                            let last_idx = segs.len() - 1;
                            for seg in &segs[..last_idx] {
                                match get_iri_property(conn, &current, seg) {
                                    Ok(Some(next)) => current = next,
                                    _ => return Ok(None::<String>),
                                }
                            }
                            let last = &segs[last_idx];
                            if let Ok(Some(v)) = get_iri_property(conn, &current, last) {
                                return Ok(Some(v));
                            }
                            get_literal_property(conn, &current, last)
                                .map_err(|e| e.to_string())
                        })
                        .await
                        .ok()
                        .flatten()
                    })
                });

                if let Some(val) = resolved {
                    out.write(&val)?;
                }
                Ok(())
            },
        ),
    );

    hb.render_template(template, &root).unwrap_or_else(|e| {
        crate::commands::log_backend(
            "warn",
            &format!("[template_render] Handlebars render error: {}", e),
        );
        template.to_string()
    })
}

/// Thin compatibility wrapper: delegates entirely to `render`.
/// The old hand-rolled placeholder navigation is discarded.
pub async fn interpolate_with_db(
    template: &str,
    ctx: &ExecutionContext,
    executor: &DbExecutor,
) -> String {
    render(template, ctx, executor).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, Triple, Object};

    /// Creates a `DbExecutor` backed by a temporary file so that both write and
    /// read connections share state.  We write triples via `assert_triples` on
    /// the raw connection first, then hand it to the executor.
    fn make_executor_with_data<F>(setup: F) -> DbExecutor
    where
        F: FnOnce(&mut rusqlite::Connection),
    {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.keep().join("test.db");

        let mut conn = rusqlite::Connection::open(&path).expect("open");
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS origins (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                description TEXT
            );
            CREATE TABLE IF NOT EXISTS transactions (
                tx INTEGER PRIMARY KEY AUTOINCREMENT,
                origin TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS triples (
                subject TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object TEXT,
                object_value TEXT,
                object_type TEXT NOT NULL CHECK(object_type IN ('iri', 'literal', 'blank')),
                object_datatype TEXT,
                object_language TEXT,
                object_number REAL,
                object_integer INTEGER,
                object_boolean INTEGER,
                tx INTEGER NOT NULL,
                origin_id INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                retracted INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_triples_subject ON triples(subject);
            CREATE INDEX IF NOT EXISTS idx_triples_predicate ON triples(predicate);
            CREATE VIEW IF NOT EXISTS triples_current AS
            SELECT subject, predicate, object, object_value, object_datatype, object_language,
                   object_number, object_integer, object_boolean, tx, origin_id, object_type, created_at
            FROM triples t
            WHERE t.retracted = 0
              AND t.tx = (
                  SELECT MAX(tx) FROM triples
                  WHERE subject = t.subject AND predicate = t.predicate
              );
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL
            );
            INSERT INTO metadata (key, value, updated_at) VALUES
                ('schema_version', '2', 0), ('ontology_imported', 'false', 0);
            INSERT INTO origins (name, description) VALUES ('test', 'Test origin');
            "#,
        )
        .expect("schema");

        setup(&mut conn);

        DbExecutor::new(conn, path)
    }

    fn lit(v: &str) -> Object {
        Object::Literal {
            value: v.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }
    }

    // ── AC 546740: prop direta {{[foundation:prop]}} ─────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn direct_prop_renders_from_self() {
        let executor = make_executor_with_data(|conn| {
            store::assert_triples(
                conn,
                &[Triple::new("test:Inst1", "test:name", lit("hello world"))],
                "test",
            )
            .unwrap();
        });

        let mut ctx = ExecutionContext::new();
        ctx.insert("self".to_string(), "test:Inst1".to_string());

        let result = render("Value: {{test:name}}", &ctx, &executor).await;
        assert_eq!(result, "Value: hello world");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn self_key_in_root() {
        let executor = make_executor_with_data(|_| {});
        let mut ctx = ExecutionContext::new();
        ctx.insert("self".to_string(), "foundation:Instance_42".to_string());

        let result = render("self={{self}}", &ctx, &executor).await;
        assert_eq!(result, "self=foundation:Instance_42");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn plain_ctx_key_renders() {
        let executor = make_executor_with_data(|_| {});
        let mut ctx = ExecutionContext::new();
        ctx.insert("inputIRIs".to_string(), "foundation:Task_123".to_string());

        let result = render("task={{inputIRIs}}", &ctx, &executor).await;
        assert_eq!(result, "task=foundation:Task_123");
    }

    // ── no_escape: special chars must NOT be HTML-escaped ────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_escape_preserves_url_special_chars() {
        let executor = make_executor_with_data(|_| {});
        let mut ctx = ExecutionContext::new();
        ctx.insert("url".to_string(), "https://x.com/a?b=1&c=2".to_string());

        let result = render("{{url}}", &ctx, &executor).await;
        assert_eq!(result, "https://x.com/a?b=1&c=2");
    }

    // ── AC 546813: aninhamento via {{resolve}} ────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resolve_two_hops() {
        let executor = make_executor_with_data(|conn| {
            store::assert_triples(
                conn,
                &[
                    Triple::new(
                        "test:Root",
                        "test:child",
                        Object::Iri("test:Child".to_string()),
                    ),
                    Triple::new("test:Child", "rdfs:label", lit("child label")),
                ],
                "test",
            )
            .unwrap();
        });

        let mut ctx = ExecutionContext::new();
        ctx.insert("self".to_string(), "test:Root".to_string());

        let result = render(
            r#"{{resolve self "test:child" "rdfs:label"}}"#,
            &ctx,
            &executor,
        )
        .await;
        assert_eq!(result, "child label");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resolve_missing_segment_renders_empty() {
        let executor = make_executor_with_data(|_| {});
        let mut ctx = ExecutionContext::new();
        ctx.insert("self".to_string(), "test:NoSuchEntity".to_string());

        let result = render(
            r#"X{{resolve self "test:missing"}}Y"#,
            &ctx,
            &executor,
        )
        .await;
        assert_eq!(result, "XY");
    }

    // ── AC 1780796961631: LOOP {{#each}} sobre prop multi-valor ──────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn each_over_multi_value_iri_renders_expanded_items() {
        let executor = make_executor_with_data(|conn| {
            store::assert_triples(
                conn,
                &[
                    Triple::new(
                        "test:Root",
                        "test:items",
                        Object::Iri("test:ItemA".to_string()),
                    ),
                    Triple::new(
                        "test:Root",
                        "test:items",
                        Object::Iri("test:ItemB".to_string()),
                    ),
                    Triple::new("test:ItemA", "rdfs:label", lit("Alpha")),
                    Triple::new("test:ItemB", "rdfs:label", lit("Beta")),
                ],
                "test",
            )
            .unwrap();
        });

        let mut ctx = ExecutionContext::new();
        ctx.insert("self".to_string(), "test:Root".to_string());

        let tmpl = "{{#each test:items}}{{this.[rdfs:label]}} {{/each}}";
        let result = render(tmpl, &ctx, &executor).await;
        assert!(result.contains("Alpha"), "got: {}", result);
        assert!(result.contains("Beta"), "got: {}", result);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resolve_inside_each_block() {
        let executor = make_executor_with_data(|conn| {
            store::assert_triples(
                conn,
                &[
                    Triple::new(
                        "test:Root",
                        "test:items",
                        Object::Iri("test:ItemA".to_string()),
                    ),
                    Triple::new(
                        "test:ItemA",
                        "test:child",
                        Object::Iri("test:LeafA".to_string()),
                    ),
                    Triple::new("test:LeafA", "rdfs:label", lit("leaf label")),
                ],
                "test",
            )
            .unwrap();
        });

        let mut ctx = ExecutionContext::new();
        ctx.insert("self".to_string(), "test:Root".to_string());

        let tmpl = r#"{{#each test:items}}{{resolve this.[@iri] "test:child" "rdfs:label"}}{{/each}}"#;
        let result = render(tmpl, &ctx, &executor).await;
        assert_eq!(result, "leaf label");
    }

    // ── interpolate (plano) permanece funcionando ─────────────────────────────

    #[test]
    fn interpolate_flat_unchanged() {
        let mut ctx = ExecutionContext::new();
        ctx.insert("city".to_string(), "London".to_string());
        let result = super::super::context::interpolate("Weather in {{city}}", &ctx);
        assert_eq!(result, "Weather in London");
    }
}
