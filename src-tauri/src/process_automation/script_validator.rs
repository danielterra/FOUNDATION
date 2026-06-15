use tauri::{AppHandle, Listener, Manager};

use crate::commands::log_backend;
use crate::owl::{DbExecutor, Individual, Object, get_literal_property};

const STATUS_VALID: &str = "foundation:Status_1773183515321";
const STATUS_INVALID: &str = "foundation:Status_1772993026091";

pub fn register_validator(app: AppHandle) {
    let app_for_closure = app.clone();
    app.listen("entity-changed-internal", move |event| {
        let Some((entity_id, written_predicates)) = parse_payload(event.payload()) else { return };
        // Skip the executor.read() entirely when foundation:script was not part of this
        // write batch. Every code path that requires (re)validation — CodeTask creation
        // with a script or an in-place script edit — writes foundation:script in the same
        // batch. Writes by the validator itself (hasStatus / scriptError) do not carry
        // foundation:script, so they no longer re-enter the handler.
        if !written_predicates.iter().any(|p| p == "foundation:script") {
            return;
        }
        let app2 = app_for_closure.clone();
        tauri::async_runtime::spawn(async move {
            validate_code_task(&app2, &entity_id).await;
        });
    });
}

fn parse_payload(payload: &str) -> Option<(String, Vec<String>)> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    let entity_id = v["entityId"].as_str().map(String::from)?;
    let written_predicates = v["writtenPredicates"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|e| e.as_str().map(String::from)).collect())
        .unwrap_or_default();
    Some((entity_id, written_predicates))
}

async fn validate_code_task(app: &AppHandle, entity_id: &str) {
    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return,
    };

    let entity_id = entity_id.to_string();
    let result = executor
        .read(move |conn| {
            // Fast type check before loading the full individual — entity-changed-internal fires
            // for every write and Individual::get is expensive for high-cardinality entities.
            if !crate::owl::is_instance_of(conn, &entity_id, "foundation:automation_CodeTask") {
                return Ok(None);
            }

            let is_code_task = Individual::get(conn, &entity_id)
                .ok()
                .flatten()
                .map(|ind| {
                    ind.properties
                        .iter()
                        .any(|(p, v)| p == "rdf:type" && v.as_iri() == Some("foundation:automation_CodeTask"))
                })
                .unwrap_or(false);

            if !is_code_task {
                return Ok(None);
            }

            let script = get_literal_property(conn, &entity_id, "foundation:script")
                .map_err(|e| e.to_string())?;

            Ok(script.map(|s| (entity_id, s)))
        })
        .await;

    let (iri, script) = match result {
        Ok(Some(pair)) => pair,
        _ => return,
    };

    let validation = compile_script(&script);

    let executor = match app.try_state::<DbExecutor>() {
        Some(e) => e,
        None => return,
    };

    executor
        .write(move |conn| {
            let ind = Individual::new(&iri);
            match validation {
                Ok(()) => {
                    ind.add_property(
                        conn,
                        "foundation:hasStatus",
                        vec![Object::Iri(STATUS_VALID.to_string())],
                        "script_validator",
                    )
                    .map_err(|e| e.to_string())?;
                    Individual::clear_property(conn, &iri, "foundation:scriptError", "script_validator")
                        .map_err(|e| e.to_string())?;
                    log_backend("info", &format!("[script_validator] {} — valid", iri));
                }
                Err(msg) => {
                    ind.add_property(
                        conn,
                        "foundation:hasStatus",
                        vec![Object::Iri(STATUS_INVALID.to_string())],
                        "script_validator",
                    )
                    .map_err(|e| e.to_string())?;
                    ind.add_property(
                        conn,
                        "foundation:scriptError",
                        vec![Object::Literal {
                            value: msg.clone(),
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        }],
                        "script_validator",
                    )
                    .map_err(|e| e.to_string())?;
                    log_backend("warn", &format!("[script_validator] {} — invalid: {}", iri, msg));
                }
            }
            Ok(String::new())
        })
        .await
        .ok();
}

pub(crate) fn compile_script(script: &str) -> Result<(), String> {
    let mut engine = rhai::Engine::new();
    engine.register_fn("mcp", |_: rhai::ImmutableString, _: rhai::ImmutableString| -> String { String::new() });
    engine.register_fn("ctx_get", |_: rhai::ImmutableString| -> String { String::new() });
    engine.register_fn("parse_json", |_: rhai::ImmutableString| -> rhai::Dynamic { rhai::Dynamic::UNIT });
    engine.register_fn("join", |_: rhai::Array, _: rhai::ImmutableString| -> String { String::new() });
    engine.register_fn("owl_get_property", |_: rhai::ImmutableString, _: rhai::ImmutableString| -> String { String::new() });
    engine.register_fn("owl_get_one", |_: rhai::ImmutableString, _: rhai::ImmutableString| -> String { String::new() });
    engine.register_fn("owl_set_property", |_: rhai::ImmutableString, _: rhai::ImmutableString, _: rhai::ImmutableString| -> String { String::new() });
    engine.register_fn("owl_assert", |_: rhai::ImmutableString, _: rhai::ImmutableString| -> String { String::new() });
    engine.register_fn("owl_read_file", |_: rhai::ImmutableString| -> String { String::new() });
    engine.register_fn("to_json", |_: rhai::Dynamic| -> String { String::new() });
    engine.register_fn("log", |_: rhai::ImmutableString| {});
    engine.register_fn("today", || -> String { String::new() });
    engine.register_fn("today_plus", |_: i64| -> String { String::new() });
    engine.compile(script).map(|_| ()).map_err(|e| e.to_string())
}
