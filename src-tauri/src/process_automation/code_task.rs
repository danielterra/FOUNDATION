use chrono::NaiveDate;
use chrono::Utc;
use tauri::{AppHandle, Manager};
use tokio::runtime::Handle;

use crate::ai::functions::{execute_async_tool, execute_tool, is_async_tool, ToolCall};
use crate::commands::log_backend;
use crate::owl::{
    get_all_property_values, get_iri_property, get_literal_property, replace_all_property_iris,
    replace_all_property_literals, DbExecutor,
};

use super::executor::{interpolate, split_iri_lines, ExecutionContext};

type Result<T> = std::result::Result<T, String>;

pub(crate) fn rhai_sha256_hex(input: rhai::ImmutableString) -> String {
    crate::files::sha256_hex(input.as_bytes())
}

pub(crate) fn rhai_replace_text(
    input: rhai::ImmutableString,
    from: rhai::ImmutableString,
    to: rhai::ImmutableString,
) -> String {
    input.replace(from.as_str(), to.as_str())
}

pub(crate) fn rhai_normalize_money(input: rhai::ImmutableString) -> String {
    let value = input.trim().replace("R$", "").replace(' ', "");
    let normalized = if value.contains(',') {
        value.replace('.', "").replace(',', ".")
    } else {
        value
    };
    let (integer, fraction) = normalized.split_once('.').unwrap_or((&normalized, ""));
    if integer.is_empty()
        || !integer.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
        || fraction.len() > 2
        || normalized.matches('.').count() > 1
    {
        return String::new();
    }
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let fraction = match fraction.len() {
        0 => "00".to_string(),
        1 => format!("{}0", fraction),
        _ => fraction.to_string(),
    };
    let canonical = format!("{}.{}", integer, fraction);
    if canonical.parse::<f64>().unwrap_or(0.0) <= 0.0 {
        String::new()
    } else {
        canonical
    }
}

pub(crate) fn rhai_trim_text(input: rhai::ImmutableString) -> String {
    input.trim().to_string()
}

pub(crate) fn rhai_escape_json_content(input: rhai::ImmutableString) -> String {
    let serialized = serde_json::to_string(input.as_str()).unwrap_or_else(|_| "\"\"".to_string());
    serialized
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn rhai_normalize_date(input: rhai::ImmutableString) -> String {
    let value = input.trim();
    NaiveDate::parse_from_str(value, "%d/%m/%Y")
        .or_else(|_| NaiveDate::parse_from_str(value, "%Y-%m-%d"))
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

pub(crate) fn rhai_c6_source_key(
    account_iri: rhai::ImmutableString,
    date: rhai::ImmutableString,
    direction: rhai::ImmutableString,
    amount: rhai::ImmutableString,
    title: rhai::ImmutableString,
    description: rhai::ImmutableString,
    ordinal: i64,
) -> String {
    let normalized_amount = normalize_source_amount(amount.as_str());
    if normalized_amount.starts_with("invalid:") {
        return String::new();
    }
    let fields = [
        normalize_source_text(account_iri.as_str()),
        normalize_source_text(date.as_str()),
        normalize_source_text(direction.as_str()),
        normalized_amount,
        normalize_source_text(title.as_str()),
        normalize_source_text(description.as_str()),
        ordinal.to_string(),
    ];
    let canonical = fields
        .iter()
        .map(|field| format!("{}:{}", field.len(), field))
        .collect::<Vec<_>>()
        .join("|");
    format!("c6:{}", crate::files::sha256_hex(canonical.as_bytes()))
}

fn normalize_source_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn normalize_source_amount(value: &str) -> String {
    let normalized = value.trim().replace(',', ".");
    let (integer, fraction) = normalized.split_once('.').unwrap_or((&normalized, ""));
    if integer.is_empty()
        || !integer.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
        || fraction.len() > 2
        || normalized.matches('.').count() > 1
    {
        return format!("invalid:{}", normalized);
    }
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let fraction = match fraction.len() {
        0 => "00".to_string(),
        1 => format!("{}0", fraction),
        _ => fraction.to_string(),
    };
    format!("{}.{}", integer, fraction)
}

pub(crate) fn rhai_parse_csv_record(input: rhai::ImmutableString) -> String {
    let result = parse_csv_record(input.as_str());
    match result {
        Ok(fields) => serde_json::json!({"success": true, "fields": fields}).to_string(),
        Err(error) => serde_json::json!({"success": false, "error": error}).to_string(),
    }
}

fn parse_csv_record(input: &str) -> Result<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    let mut quote_closed = false;

    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                    quote_closed = true;
                }
            } else {
                field.push(ch);
            }
            continue;
        }

        match ch {
            ',' => {
                fields.push(std::mem::take(&mut field));
                quote_closed = false;
            }
            '"' if field.trim().is_empty() && !quote_closed => {
                field.clear();
                quoted = true;
            }
            '\r' | '\n' if chars.peek().is_none() => {}
            _ if quote_closed && ch.is_whitespace() => {}
            _ if quote_closed => return Err("texto inesperado após campo entre aspas".to_string()),
            _ => field.push(ch),
        }
    }

    if quoted {
        return Err("campo entre aspas não foi fechado".to_string());
    }

    fields.push(field);
    Ok(fields)
}

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

    let output =
        tokio::task::spawn_blocking(move || run_script(&script, &ctx_clone, &app_clone, &handle))
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
                            replace_all_property_iris(
                                conn,
                                &control,
                                &prop,
                                &refs,
                                "process_automation",
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        None => {
                            replace_all_property_literals(
                                conn,
                                &control,
                                &prop,
                                &[out_clone.as_str()],
                                "process_automation",
                            )
                            .map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(String::new())
                })
                .await
                .unwrap_or_else(|e| {
                    log_backend(
                        "error",
                        &format!(
                            "[code_task] failed to write output to outputProperty {} on {}: {}",
                            prop_log, control_log, e
                        ),
                    );
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

fn dynamic_to_json_value(v: rhai::Dynamic) -> serde_json::Value {
    if v.is_unit() {
        return serde_json::Value::Null;
    }
    if let Some(b) = v.clone().try_cast::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Some(i) = v.clone().try_cast::<i64>() {
        return serde_json::Value::Number(i.into());
    }
    if let Some(f) = v.clone().try_cast::<f64>() {
        return serde_json::json!(f);
    }
    if let Some(s) = v.clone().try_cast::<String>() {
        return serde_json::Value::String(s);
    }
    if let Some(arr) = v.clone().try_cast::<rhai::Array>() {
        let items: Vec<serde_json::Value> = arr.into_iter().map(dynamic_to_json_value).collect();
        return serde_json::Value::Array(items);
    }
    if let Some(map) = v.try_cast::<rhai::Map>() {
        let obj: serde_json::Map<String, serde_json::Value> = map
            .into_iter()
            .map(|(k, val)| (k.to_string(), dynamic_to_json_value(val)))
            .collect();
        return serde_json::Value::Object(obj);
    }
    serde_json::Value::Null
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
    "datasync_upsert_item",
    "datasync_create_source",
    "datasync_run",
    "datasync_retry_transform",
];

fn dry_run_mock(tool_name: &str) -> String {
    if tool_name == "assert_individual" {
        let mock_iri = format!(
            "dryrun:mock_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        serde_json::json!({
            "success": true,
            "result": {"operationsCompleted": 1, "results": [{"iri": mock_iri}]},
            "error": null
        })
        .to_string()
    } else if tool_name == "datasync_upsert_item" {
        let mock_iri = format!(
            "dryrun:mock_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        serde_json::json!({
            "success": true,
            "result": {"entity_iri": mock_iri},
            "error": null
        })
        .to_string()
    } else {
        r#"{"success":true,"result":null,"error":null}"#.to_string()
    }
}

thread_local! {
    /// Accumulates write-tool calls intercepted during a dry_run execution.
    /// Cleared by `run_script_dry_run_collect` before each invocation.
    static DRY_RUN_INTERCEPTED: std::cell::RefCell<Vec<serde_json::Value>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Run `script` with `dryRun=true` and return `(script_output, intercepted_write_calls)`.
///
/// The second element lists every write-tool call that was intercepted, each as
/// `{"tool": "<name>", "params": <json>}`.  This lets callers surface what *would*
/// have been persisted without actually persisting anything.
pub(crate) fn run_script_dry_run_collect(
    script: &str,
    ctx: &ExecutionContext,
    app: &AppHandle,
    handle: &Handle,
) -> Result<(String, Vec<serde_json::Value>)> {
    DRY_RUN_INTERCEPTED.with(|cell| cell.borrow_mut().clear());
    let output = run_script(script, ctx, app, handle)?;
    let intercepted = DRY_RUN_INTERCEPTED.with(|cell| cell.borrow().clone());
    Ok((output, intercepted))
}

pub(crate) fn run_script(
    script: &str,
    ctx: &ExecutionContext,
    app: &AppHandle,
    handle: &Handle,
) -> Result<String> {
    let mut engine = rhai::Engine::new();
    engine.set_max_operations(1_000_000);
    engine.set_max_string_size(10 * 1024 * 1024);
    engine.register_fn("sha256_hex", rhai_sha256_hex);
    engine.register_fn("replace_text", rhai_replace_text);
    engine.register_fn("normalize_money", rhai_normalize_money);
    engine.register_fn("trim_text", rhai_trim_text);
    engine.register_fn("escape_json_content", rhai_escape_json_content);
    engine.register_fn("normalize_date", rhai_normalize_date);
    engine.register_fn("c6_source_key", rhai_c6_source_key);
    engine.register_fn("csv_parse_record", rhai_parse_csv_record);

    let dry_run = ctx.get("dryRun").map(|v| v == "true").unwrap_or(false);

    engine.register_fn("mcp", {
        let app = app.clone();
        let handle = handle.clone();
        move |tool_name: rhai::ImmutableString, params_json: rhai::ImmutableString| -> String {
            let tool_name = tool_name.to_string();
            if dry_run && DRY_RUN_WRITE_TOOLS.contains(&tool_name.as_str()) {
                log_backend(
                    "info",
                    &format!(
                        "[code_task] dry_run: intercepted write tool '{}'",
                        tool_name
                    ),
                );
                let params_val: serde_json::Value =
                    serde_json::from_str(params_json.as_str()).unwrap_or(serde_json::Value::Null);
                DRY_RUN_INTERCEPTED.with(|cell| {
                    cell.borrow_mut().push(serde_json::json!({
                        "tool": tool_name,
                        "params": params_val,
                    }));
                });
                return dry_run_mock(&tool_name);
            }
            let args: serde_json::Value =
                serde_json::from_str(&params_json).unwrap_or(serde_json::Value::Null);
            let call = ToolCall {
                name: tool_name,
                arguments: args,
            };
            let executor = app.state::<DbExecutor>();
            let app_inner = app.clone();
            if is_async_tool(&call.name) {
                let r = handle.block_on(execute_async_tool(&call, Some(&app_inner)));
                serde_json::to_string(&serde_json::json!({
                    "success": r.success,
                    "result": r.result,
                    "error": r.error,
                }))
                .unwrap_or_else(|e| {
                    serde_json::json!({"success": false, "result": null, "error": e.to_string()})
                        .to_string()
                })
            } else {
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
                        serde_json::json!({"success": false, "result": null, "error": e})
                            .to_string()
                    })
            }
        }
    });

    engine.register_fn("ctx_get", {
        let ctx = ctx.clone();
        move |key: rhai::ImmutableString| -> String {
            ctx.get(key.as_str()).cloned().unwrap_or_default()
        }
    });

    engine.register_fn(
        "parse_json",
        |json_str: rhai::ImmutableString| -> rhai::Dynamic {
            let v: serde_json::Value =
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
            json_value_to_dynamic(v)
        },
    );

    engine.register_fn(
        "join",
        |arr: rhai::Array, sep: rhai::ImmutableString| -> String {
            arr.iter()
                .map(|d| d.clone().try_cast::<String>().unwrap_or_default())
                .collect::<Vec<_>>()
                .join(sep.as_str())
        },
    );

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
                DRY_RUN_INTERCEPTED.with(|cell| {
                    cell.borrow_mut().push(serde_json::json!({
                        "tool": "owl_set_property",
                        "params": {"iri": iri.as_str(), "property_iri": prop_iri.as_str(), "value": value.as_str()},
                    }));
                });
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
                log_backend(
                    "info",
                    &format!(
                        "[code_task] dry_run: intercepted owl_assert for class {}",
                        class_iri
                    ),
                );
                DRY_RUN_INTERCEPTED.with(|cell| {
                    cell.borrow_mut().push(serde_json::json!({
                        "tool": "owl_assert",
                        "params": {"class_iri": class_iri.as_str(), "label": label.as_str()},
                    }));
                });
                return dry_run_mock(&tool_name);
            }
            let args = serde_json::json!({
                "operations": [{"class_iri": class_iri.as_str(), "label": label.as_str()}]
            });
            let call = ToolCall {
                name: tool_name,
                arguments: args,
            };
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
            let v: serde_json::Value =
                serde_json::from_str(&result_json).unwrap_or(serde_json::Value::Null);
            v["result"]["results"][0]["iri"]
                .as_str()
                .unwrap_or("")
                .to_string()
        }
    });

    engine.register_fn("owl_read_file", |path: rhai::ImmutableString| -> String {
        std::fs::read_to_string(path.as_str()).unwrap_or_default()
    });

    engine.register_fn("to_json", |value: rhai::Dynamic| -> String {
        let json_val = dynamic_to_json_value(value);
        serde_json::to_string(&json_val).unwrap_or_else(|_| "null".to_string())
    });

    engine.register_fn("log", {
        move |msg: rhai::ImmutableString| {
            log_backend("info", &format!("[rhai] {}", msg.as_str()));
        }
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
            let s = val
                .clone()
                .try_cast::<String>()
                .unwrap_or_else(|| val.to_string());
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
        engine.register_fn("sha256_hex", rhai_sha256_hex);
        engine.register_fn("replace_text", rhai_replace_text);
        engine.register_fn("normalize_money", rhai_normalize_money);
        engine.register_fn("trim_text", rhai_trim_text);
        engine.register_fn("escape_json_content", rhai_escape_json_content);
        engine.register_fn("normalize_date", rhai_normalize_date);
        engine.register_fn("c6_source_key", rhai_c6_source_key);
        engine.register_fn("csv_parse_record", rhai_parse_csv_record);
        engine.register_fn(
            "mcp",
            |_: rhai::ImmutableString, _: rhai::ImmutableString| -> String {
                r#"{"success":true,"result":null,"error":null}"#.to_string()
            },
        );
        engine.register_fn("ctx_get", move |key: rhai::ImmutableString| -> String {
            ctx.get(key.as_str()).cloned().unwrap_or_default()
        });
        engine.register_fn("to_json", |value: rhai::Dynamic| -> String {
            let json_val = dynamic_to_json_value(value);
            serde_json::to_string(&json_val).unwrap_or_else(|_| "null".to_string())
        });
        engine.register_fn("log", |_msg: rhai::ImmutableString| {});
        engine
    }

    #[test]
    fn csv_record_parser_supports_quoted_commas_and_escaped_quotes() {
        assert_eq!(
            parse_csv_record(r#"2026-01-02,"Compra, mercado","Ele disse ""oi""""#).unwrap(),
            vec![
                "2026-01-02".to_string(),
                "Compra, mercado".to_string(),
                "Ele disse \"oi\"".to_string()
            ]
        );
    }

    #[test]
    fn csv_record_parser_rejects_unclosed_quotes() {
        assert!(parse_csv_record("2026-01-02,\"incompleto").is_err());
    }

    #[test]
    fn c6_source_key_is_stable_after_normalization_and_sensitive_to_occurrence() {
        let first = rhai_c6_source_key(
            " account  123 ".into(),
            "2026-01-02".into(),
            "debit".into(),
            "0005,0".into(),
            " MERCADO   CENTRAL ".into(),
            " COMPRA,   CARTAO ".into(),
            1,
        );
        let normalized = rhai_c6_source_key(
            "account 123".into(),
            "2026-01-02".into(),
            "DEBIT".into(),
            "5.00".into(),
            "mercado central".into(),
            "compra, cartao".into(),
            1,
        );
        let duplicate = rhai_c6_source_key(
            "account 123".into(),
            "2026-01-02".into(),
            "debit".into(),
            "5.00".into(),
            "mercado central".into(),
            "compra, cartao".into(),
            2,
        );
        assert_eq!(first, normalized);
        assert_ne!(first, duplicate);
        assert!(first.starts_with("c6:sha256:"));
        assert_eq!(
            rhai_c6_source_key(
                "account".into(),
                "2026-01-02".into(),
                "debit".into(),
                "5.123".into(),
                "title".into(),
                "description".into(),
                1,
            ),
            ""
        );
    }

    #[test]
    fn sha256_helper_returns_prefixed_digest() {
        assert_eq!(
            rhai_sha256_hex("abc".into()),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn c6_money_normalizer_handles_decimal_and_grouping_separators() {
        assert_eq!(rhai_normalize_money("12,50".into()), "12.50");
        assert_eq!(rhai_normalize_money("1.234,5".into()), "1234.50");
        assert_eq!(rhai_normalize_money("1234.50".into()), "1234.50");
        assert_eq!(rhai_normalize_money("12,345".into()), "");
    }

    #[test]
    fn json_content_escaper_omits_surrounding_quotes_and_escapes_content() {
        assert_eq!(rhai_escape_json_content("a\"b".into()), "a\\\"b");
    }

    #[test]
    fn bank_date_normalizer_rejects_invalid_dates() {
        assert_eq!(rhai_normalize_date("02/11/2026".into()), "2026-11-02");
        assert_eq!(rhai_normalize_date("2026-11-02".into()), "2026-11-02");
        assert_eq!(rhai_normalize_date("31/02/2026".into()), "");
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
        engine.register_fn("ctx_get", |_: rhai::ImmutableString| -> String {
            String::new()
        });
        engine.register_fn(
            "mcp",
            |_: rhai::ImmutableString, _: rhai::ImmutableString| -> String { String::new() },
        );
        let result = engine.eval::<rhai::Dynamic>("loop {}");
        assert!(result.is_err());
    }

    fn test_engine_dry_run(ctx: ExecutionContext) -> rhai::Engine {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(1_000_000);
        let dry_run = ctx.get("dryRun").map(|v| v == "true").unwrap_or(false);
        engine.register_fn(
            "mcp",
            move |tool_name: rhai::ImmutableString, _params: rhai::ImmutableString| -> String {
                if dry_run && DRY_RUN_WRITE_TOOLS.contains(&tool_name.as_str()) {
                    dry_run_mock(tool_name.as_str())
                } else {
                    r#"{"success":true,"result":{"live":true},"error":null}"#.to_string()
                }
            },
        );
        engine.register_fn("ctx_get", move |key: rhai::ImmutableString| -> String {
            ctx.get(key.as_str()).cloned().unwrap_or_default()
        });
        engine.register_fn("to_json", |value: rhai::Dynamic| -> String {
            let json_val = dynamic_to_json_value(value);
            serde_json::to_string(&json_val).unwrap_or_else(|_| "null".to_string())
        });
        engine.register_fn("log", |_msg: rhai::ImmutableString| {});
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
        assert!(
            iri.starts_with("dryrun:mock_"),
            "expected dryrun IRI, got: {}",
            iri
        );
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
        let result: String = engine.eval(r#"mcp("search", "{}")"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["result"]["live"].as_bool().unwrap());
    }

    #[test]
    fn without_dry_run_write_tools_pass_through() {
        let ctx = ExecutionContext::new();
        let engine = test_engine_dry_run(ctx);
        let result: String = engine.eval(r#"mcp("assert_individual", "{}")"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["result"]["live"].as_bool().unwrap());
    }

    #[test]
    fn extract_rhai_message_returns_thrown_string_without_position_noise() {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(1_000_000);
        let err = engine
            .eval::<rhai::Dynamic>(r#"throw "validation failed""#)
            .unwrap_err();
        let msg = extract_rhai_message(*err);
        assert_eq!(
            msg, "validation failed",
            "expected clean message, got: {}",
            msg
        );
    }

    #[test]
    fn extract_rhai_message_non_runtime_falls_back_to_display() {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(1_000_000);
        let err = engine.eval::<rhai::Dynamic>("let x = ;").unwrap_err();
        let msg = extract_rhai_message(*err);
        assert!(!msg.is_empty(), "expected non-empty fallback message");
    }

    #[test]
    fn to_json_serializes_map_to_json_string() {
        let ctx = ExecutionContext::new();
        let engine = test_engine(ctx);
        let result: String = engine
            .eval(r#"let m = #{"key": "value", "num": 42}; to_json(m)"#)
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["key"].as_str().unwrap(), "value");
        assert_eq!(parsed["num"].as_i64().unwrap(), 42);
    }

    #[test]
    fn to_json_serializes_array() {
        let ctx = ExecutionContext::new();
        let engine = test_engine(ctx);
        let result: String = engine.eval(r#"to_json([1, "two", true])"#).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[0].as_i64().unwrap(), 1);
        assert_eq!(parsed[1].as_str().unwrap(), "two");
        assert!(parsed[2].as_bool().unwrap());
    }

    #[test]
    fn log_fn_does_not_panic() {
        let ctx = ExecutionContext::new();
        let engine = test_engine(ctx);
        let result: rhai::Dynamic = engine.eval(r#"log("hello from test"); 42"#).unwrap();
        assert_eq!(result.try_cast::<i64>().unwrap(), 42);
    }
}
