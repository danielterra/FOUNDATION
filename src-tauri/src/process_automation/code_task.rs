use chrono::Utc;
use tauri::{AppHandle, Manager};
use tokio::runtime::Handle;

use crate::ai::functions::{execute_tool, ToolCall};
use crate::commands::log_backend;
use crate::owl::{get_all_property_values, get_iri_property, get_literal_property, replace_all_property_iris, replace_all_property_literals, DbExecutor};

use super::executor::{interpolate, split_iri_lines, ExecutionContext};

type Result<T> = std::result::Result<T, String>;

pub async fn execute_code_task(
    app: &AppHandle,
    node_iri: &str,
    ctx: &ExecutionContext,
) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let script = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                get_literal_property(conn, &node_iri, "foundation:script")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("CodeTask {} has no script", node_iri))
            }
        })
        .await?;

    let output_property_iri = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                get_iri_property(conn, &node_iri, "foundation:outputProperty")
                    .map_err(|e| e.to_string())
            }
        })
        .await?;

    let script = interpolate(&script, ctx);
    let ctx_clone = ctx.clone();
    let app_clone = app.clone();
    let handle = Handle::current();

    let output = tokio::task::spawn_blocking(move || {
        run_script(&script, &ctx_clone, &app_clone, &handle)
    })
    .await
    .map_err(|e| e.to_string())??;

    if let (Some(prop), Some(control)) = (output_property_iri, ctx.get("self").cloned()) {
        if !output.is_empty() {
            let prop_log = prop.clone();
            let control_log = control.clone();
            let out_clone = output.clone();
            executor
                .write(move |conn| {
                    match split_iri_lines(&out_clone) {
                        Some(iris) => {
                            let refs: Vec<&str> = iris.iter().map(|s| s.as_str()).collect();
                            replace_all_property_iris(conn, &control, &prop, &refs, "process_automation")
                                .map_err(|e| e.to_string())?;
                        }
                        None => {
                            replace_all_property_literals(conn, &control, &prop, &[out_clone.as_str()], "process_automation")
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(String::new())
                })
                .await
                .unwrap_or_else(|e| {
                    log_backend("error", &format!(
                        "[code_task] failed to write output to outputProperty {} on {}: {}",
                        prop_log, control_log, e
                    ));
                    String::new()
                });
        } else {
            log_backend("warn", &format!(
                "[code_task] outputProperty {} configured but script returned empty string — controlInstance {} was not written",
                prop, control
            ));
        }
    }

    Ok(output)
}

fn json_value_to_dynamic(v: serde_json::Value) -> rhai::Dynamic {
    match v {
        serde_json::Value::Null => rhai::Dynamic::UNIT,
        serde_json::Value::Bool(b) => rhai::Dynamic::from(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rhai::Dynamic::from(i)
            } else if let Some(f) = n.as_f64() {
                rhai::Dynamic::from(f)
            } else {
                rhai::Dynamic::UNIT
            }
        }
        serde_json::Value::String(s) => rhai::Dynamic::from(s),
        serde_json::Value::Array(arr) => {
            let a: rhai::Array = arr.into_iter().map(json_value_to_dynamic).collect();
            rhai::Dynamic::from_array(a)
        }
        serde_json::Value::Object(obj) => {
            let mut map = rhai::Map::new();
            for (k, val) in obj {
                map.insert(k.into(), json_value_to_dynamic(val));
            }
            rhai::Dynamic::from_map(map)
        }
    }
}

const DRY_RUN_WRITE_TOOLS: &[&str] = &[
    "assert_individual",
    "add_property_values",
    "replace_property_values",
    "remove_property_values",
    "clear_property",
    "retract_individual",
    "retract_class",
    "retract_property",
    "define_class",
    "define_property",
    "run_automation",
];

fn dry_run_mock(tool_name: &str) -> String {
    if tool_name == "assert_individual" {
        let mock_iri = format!("dryrun:mock_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0));
        serde_json::json!({
            "success": true,
            "result": {"operationsCompleted": 1, "results": [{"iri": mock_iri}]},
            "error": null
        }).to_string()
    } else {
        r#"{"success":true,"result":null,"error":null}"#.to_string()
    }
}

fn run_script(
    script: &str,
    ctx: &ExecutionContext,
    app: &AppHandle,
    handle: &Handle,
) -> Result<String> {
    let mut engine = rhai::Engine::new();
    engine.set_max_operations(1_000_000);
    engine.set_max_string_size(10 * 1024 * 1024);

    let dry_run = ctx.get("dryRun").map(|v| v == "true").unwrap_or(false);

    engine.register_fn("mcp", {
        let app = app.clone();
        let handle = handle.clone();
        move |tool_name: rhai::ImmutableString, params_json: rhai::ImmutableString| -> String {
            let tool_name = tool_name.to_string();
            if dry_run && DRY_RUN_WRITE_TOOLS.contains(&tool_name.as_str()) {
                log_backend("info", &format!("[code_task] dry_run: intercepted write tool '{}'", tool_name));
                return dry_run_mock(&tool_name);
            }
            let args: serde_json::Value = serde_json::from_str(&params_json)
                .unwrap_or(serde_json::Value::Null);
            let call = ToolCall { name: tool_name, arguments: args };
            let executor = app.state::<DbExecutor>();
            let app_inner = app.clone();
            handle
                .block_on(executor.write(move |conn| {
                    let r = execute_tool(conn, &call, Some(&app_inner), None);
                    serde_json::to_string(&serde_json::json!({
                        "success": r.success,
                        "result": r.result,
                        "error": r.error,
                    }))
                    .map_err(|e| e.to_string())
                }))
                .unwrap_or_else(|e| {
                    serde_json::json!({"success": false, "result": null, "error": e}).to_string()
                })
        }
    });

    engine.register_fn("ctx_get", {
        let ctx = ctx.clone();
        move |key: rhai::ImmutableString| -> String { ctx.get(key.as_str()).cloned().unwrap_or_default() }
    });

    engine.register_fn("parse_json", |json_str: rhai::ImmutableString| -> rhai::Dynamic {
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
        json_value_to_dynamic(v)
    });

    engine.register_fn("join", |arr: rhai::Array, sep: rhai::ImmutableString| -> String {
        arr.iter()
            .map(|d| d.clone().try_cast::<String>().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(sep.as_str())
    });

    engine.register_fn("owl_get_property", {
        let app = app.clone();
        let handle = handle.clone();
        move |iri: rhai::ImmutableString, prop_iri: rhai::ImmutableString| -> String {
            let iri_s = iri.to_string();
            let prop_iri_s = prop_iri.to_string();
            let executor = app.state::<DbExecutor>();
            let values = handle
                .block_on(executor.read(move |conn| {
                    get_all_property_values(conn, &iri_s, &prop_iri_s).map_err(|e| e.to_string())
                }))
                .unwrap_or_default();
            serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string())
        }
    });

    engine.register_fn("owl_get_one", {
        let app = app.clone();
        let handle = handle.clone();
        move |iri: rhai::ImmutableString, prop_iri: rhai::ImmutableString| -> String {
            let iri_s = iri.to_string();
            let prop_iri_s = prop_iri.to_string();
            let executor = app.state::<DbExecutor>();
            let values = handle
                .block_on(executor.read(move |conn| {
                    get_all_property_values(conn, &iri_s, &prop_iri_s).map_err(|e| e.to_string())
                }))
                .unwrap_or_default();
            values.into_iter().next().unwrap_or_default()
        }
    });

    engine.register_fn("owl_set_property", {
        let app = app.clone();
        let handle = handle.clone();
        move |iri: rhai::ImmutableString, prop_iri: rhai::ImmutableString, value: rhai::ImmutableString| -> String {
            let tool_name = "replace_property_values".to_string();
            if dry_run && DRY_RUN_WRITE_TOOLS.contains(&tool_name.as_str()) {
                log_backend("info", &format!("[code_task] dry_run: intercepted owl_set_property on {} {}", iri, prop_iri));
                return dry_run_mock(&tool_name);
            }
            let args = serde_json::json!({
                "operations": [{"iri": iri.as_str(), "property_iri": prop_iri.as_str(), "values": [value.as_str()]}]
            });
            let call = ToolCall { name: tool_name, arguments: args };
            let executor = app.state::<DbExecutor>();
            let app_inner = app.clone();
            handle
                .block_on(executor.write(move |conn| {
                    let r = execute_tool(conn, &call, Some(&app_inner), None);
                    serde_json::to_string(&serde_json::json!({
                        "success": r.success,
                        "result": r.result,
                        "error": r.error,
                    }))
                    .map_err(|e| e.to_string())
                }))
                .unwrap_or_else(|e| {
                    serde_json::json!({"success": false, "result": null, "error": e}).to_string()
                })
        }
    });

    engine.register_fn("owl_assert", {
        let app = app.clone();
        let handle = handle.clone();
        move |class_iri: rhai::ImmutableString, label: rhai::ImmutableString| -> String {
            let tool_name = "assert_individual".to_string();
            if dry_run && DRY_RUN_WRITE_TOOLS.contains(&tool_name.as_str()) {
                log_backend("info", &format!("[code_task] dry_run: intercepted owl_assert for class {}", class_iri));
                return dry_run_mock(&tool_name);
            }
            let args = serde_json::json!({
                "operations": [{"class_iri": class_iri.as_str(), "label": label.as_str()}]
            });
            let call = ToolCall { name: tool_name, arguments: args };
            let executor = app.state::<DbExecutor>();
            let app_inner = app.clone();
            let result_json = handle
                .block_on(executor.write(move |conn| {
                    let r = execute_tool(conn, &call, Some(&app_inner), None);
                    serde_json::to_string(&serde_json::json!({
                        "success": r.success,
                        "result": r.result,
                        "error": r.error,
                    }))
                    .map_err(|e| e.to_string())
                }))
                .unwrap_or_else(|e| {
                    serde_json::json!({"success": false, "result": null, "error": e}).to_string()
                });
            let v: serde_json::Value = serde_json::from_str(&result_json).unwrap_or(serde_json::Value::Null);
            v["result"]["results"][0]["iri"].as_str().unwrap_or("").to_string()
        }
    });

    engine.register_fn("owl_read_file", |path: rhai::ImmutableString| -> String {
        std::fs::read_to_string(path.as_str()).unwrap_or_default()
    });

    engine.register_fn("today", || -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    });

    engine.register_fn("today_plus", |days: i64| -> String {
        (Utc::now() + chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string()
    });

    log_backend("info", "[code_task] Evaluating Rhai script");

    let dynamic: rhai::Dynamic = engine.eval(script).map_err(|e| extract_rhai_message(*e))?;
    Ok(dynamic.try_cast::<String>().unwrap_or_default())
}

/// Extracts a human-readable message from a Rhai evaluation error.
///
/// For `throw "msg"` the inner Dynamic is a string; for other runtime errors
/// the message is taken from the inner Dynamic's string representation when
/// available, and falls back to the full error display otherwise.
fn extract_rhai_message(err: rhai::EvalAltResult) -> String {
    match err {
        rhai::EvalAltResult::ErrorRuntime(val, _) => {
            let s = val.clone().try_cast::<String>().unwrap_or_else(|| val.to_string());
            if s.is_empty() {
                "Script runtime error".to_string()
            } else {
                s
            }
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine(ctx: ExecutionContext) -> rhai::Engine {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(1_000_000);
        engine.register_fn("mcp", |_: rhai::ImmutableString, _: rhai::ImmutableString| -> String {
            r#"{"success":true,"result":null,"error":null}"#.to_string()
        });
        engine.register_fn("ctx_get", move |key: rhai::ImmutableString| -> String {
            ctx.get(key.as_str()).cloned().unwrap_or_default()
        });
        engine
    }

    #[test]
    fn ctx_get_returns_input_iris_injected_by_executor() {
        let mut ctx = ExecutionContext::new();
        ctx.insert("inputIRIs".to_string(), "foundation:File_123".to_string());
        let engine = test_engine(ctx);
        let result: rhai::Dynamic = engine.eval(r#"ctx_get("inputIRIs")"#).unwrap();
        assert_eq!(result.try_cast::<String>().unwrap(), "foundation:File_123");
    }

    #[test]
    fn ctx_get_missing_key_returns_empty_string() {
        let ctx = ExecutionContext::new();
        let engine = test_engine(ctx);
        let result: rhai::Dynamic = engine.eval(r#"ctx_get("missing")"#).unwrap();
        assert_eq!(result.try_cast::<String>().unwrap(), "");
    }

    #[test]
    fn script_error_propagated_as_err() {
        let ctx = ExecutionContext::new();
        let engine = test_engine(ctx);
        let result = engine.eval::<rhai::Dynamic>(r#"throw "custom error""#);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("custom error"));
    }

    #[test]
    fn script_returning_non_string_gives_empty_output() {
        let ctx = ExecutionContext::new();
        let engine = test_engine(ctx);
        let result: rhai::Dynamic = engine.eval("42").unwrap();
        let output = result.try_cast::<String>().unwrap_or_default();
        assert_eq!(output, "");
    }

    #[test]
    fn script_sandbox_enforces_operation_limit() {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(100);
        engine.register_fn("ctx_get", |_: rhai::ImmutableString| -> String { String::new() });
        engine.register_fn("mcp", |_: rhai::ImmutableString, _: rhai::ImmutableString| -> String { String::new() });
        let result = engine.eval::<rhai::Dynamic>("loop {}");
        assert!(result.is_err());
    }

    fn test_engine_dry_run(ctx: ExecutionContext) -> rhai::Engine {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(1_000_000);
        let dry_run = ctx.get("dryRun").map(|v| v == "true").unwrap_or(false);
        engine.register_fn("mcp", move |tool_name: rhai::ImmutableString, _params: rhai::ImmutableString| -> String {
            if dry_run && DRY_RUN_WRITE_TOOLS.contains(&tool_name.as_str()) {
                dry_run_mock(tool_name.as_str())
            } else {
                r#"{"success":true,"result":{"live":true},"error":null}"#.to_string()
            }
        });
        engine.register_fn("ctx_get", move |key: rhai::ImmutableString| -> String {
            ctx.get(key.as_str()).cloned().unwrap_or_default()
        });
        engine
    }

    #[test]
    fn dry_run_intercepts_assert_individual_with_mock_iri() {
        let mut ctx = ExecutionContext::new();
        ctx.insert("dryRun".to_string(), "true".to_string());
        let engine = test_engine_dry_run(ctx);
        let result: String = engine
            .eval(r#"let r = mcp("assert_individual", "{}"); r"#)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["success"].as_bool().unwrap());
        let iri = v["result"]["results"][0]["iri"].as_str().unwrap();
        assert!(iri.starts_with("dryrun:mock_"), "expected dryrun IRI, got: {}", iri);
    }

    #[test]
    fn dry_run_intercepts_replace_property_values() {
        let mut ctx = ExecutionContext::new();
        ctx.insert("dryRun".to_string(), "true".to_string());
        let engine = test_engine_dry_run(ctx);
        let result: String = engine
            .eval(r#"mcp("replace_property_values", "{}")"#)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["success"].as_bool().unwrap());
        assert!(v["result"].is_null());
    }

    #[test]
    fn dry_run_allows_read_tools_through() {
        let mut ctx = ExecutionContext::new();
        ctx.insert("dryRun".to_string(), "true".to_string());
        let engine = test_engine_dry_run(ctx);
        let result: String = engine
            .eval(r#"mcp("search", "{}")"#)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["result"]["live"].as_bool().unwrap());
    }

    #[test]
    fn without_dry_run_write_tools_pass_through() {
        let ctx = ExecutionContext::new();
        let engine = test_engine_dry_run(ctx);
        let result: String = engine
            .eval(r#"mcp("assert_individual", "{}")"#)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["result"]["live"].as_bool().unwrap());
    }

    #[test]
    fn extract_rhai_message_returns_thrown_string_without_position_noise() {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(1_000_000);
        let err = engine.eval::<rhai::Dynamic>(r#"throw "validation failed""#)
            .unwrap_err();
        let msg = extract_rhai_message(*err);
        assert_eq!(msg, "validation failed", "expected clean message, got: {}", msg);
    }

    #[test]
    fn extract_rhai_message_non_runtime_falls_back_to_display() {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(1_000_000);
        let err = engine.eval::<rhai::Dynamic>("let x = ;")
            .unwrap_err();
        let msg = extract_rhai_message(*err);
        assert!(!msg.is_empty(), "expected non-empty fallback message");
    }
}
