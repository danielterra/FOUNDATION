use serde_json::Value;
use tauri::Emitter;
use crate::commands::widget::{self, Widget, Position, Size};
use crate::owl::Connection;
use super::ToolResult;

const WIDGET_CASCADE_STEP: f64 = 50.0;
const WIDGET_DEFAULT_X: f64 = 100.0;
const WIDGET_DEFAULT_Y: f64 = 100.0;
const WIDGET_DEFAULT_WIDTH: f64 = 400.0;
const WIDGET_DEFAULT_HEIGHT: f64 = 600.0;

pub fn blackboard_update(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
    conversation_id: Option<&str>,
) -> ToolResult {
    let ops = match args.as_array() {
        Some(ops) if !ops.is_empty() => ops.clone(),
        Some(_) => return ToolResult {
            success: false,
            result: None,
            error: Some("Operations array must not be empty".to_string()),
            concept: None,
        },
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Arguments must be a non-empty array of operations".to_string()),
            concept: None,
        },
    };

    let mut results = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        let operation = op.get("operation").and_then(|v| v.as_str()).unwrap_or("");
        let result = match operation {
            "add" => blackboard_add_widget_one(conn, op, app, conversation_id),
            "remove" => {
                let widget_id = op.get("params")
                    .and_then(|p| p.get("widget_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let remove_args = serde_json::json!({"widget_id": widget_id});
                blackboard_remove_one(conn, &remove_args, app)
            }
            "replace" => blackboard_replace(conn, op, app, conversation_id),
            _ => ToolResult {
                success: false,
                result: None,
                error: Some(format!(
                    "Unknown operation '{}'. Use 'add', 'remove', or 'replace'.",
                    operation
                )),
                concept: None,
            },
        };
        if !result.success {
            return ToolResult {
                success: false,
                result: None,
                error: Some(format!(
                    "Operation {} failed: {}",
                    i,
                    result.error.unwrap_or_else(|| "unknown error".to_string()),
                )),
                concept: None,
            };
        }
        results.push(result.result);
    }

    ToolResult {
        success: true,
        result: Some(serde_json::json!({
            "operationsCompleted": results.len(),
        })),
        error: None,
        concept: None,
    }
}

fn blackboard_replace(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
    conversation_id: Option<&str>,
) -> ToolResult {
    let widgets_result = if let Some(conv_id) = conversation_id {
        widget::owl_get_widgets_for_conversation(conn, conv_id)
    } else {
        widget::owl_get_all_widgets(conn)
    };
    match widgets_result {
        Ok(widgets) => {
            for w in &widgets {
                let _ = widget::owl_delete_widget(conn, &w.id);
                if let Some(app_handle) = app {
                    app_handle.emit("widget-removed", w.id.clone()).ok();
                }
            }
        }
        Err(e) => return ToolResult { success: false, result: None, error: Some(e), concept: None },
    }

    if args.get("widget_type").is_some() {
        blackboard_add_widget_one(conn, args, app, conversation_id)
    } else {
        ToolResult {
            success: true,
            result: None,
            error: None,
            concept: None,
        }
    }
}

fn blackboard_add_widget_one(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
    conversation_id: Option<&str>,
) -> ToolResult {
    let widget_type = match args.get("widget_type").and_then(|v| v.as_str()) {
        Some(t) => t.to_lowercase(),
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: widget_type".to_string()),
            concept: None,
        },
    };

    let params = match args.get("params") {
        Some(p) => p,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: params".to_string()),
            concept: None,
        },
    };

    let valid_types = widget::blackboard__list_widget_types();
    if !valid_types.iter().any(|t| t.id == widget_type) {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!(
                "Unknown widget type: {}. Available types: {}",
                widget_type,
                valid_types.iter().map(|t| t.id.as_str()).collect::<Vec<_>>().join(", ")
            )),
            concept: None,
        };
    }

    let entity_id = match params.get("entity_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some(format!("{} widget requires 'entity_id' in params", widget_type)),
            concept: None,
        },
    };

    let content = params.get("content").and_then(|v| v.as_str()).map(String::from)
        .or_else(|| {
            if widget_type == "mermaid" {
                crate::owl::Individual::get(conn, entity_id).ok().flatten()
                    .and_then(|ind| ind.properties.into_iter()
                        .find(|(p, _)| p == "foundation:diagramSource")
                        .and_then(|(_, v)| v.as_literal()))
            } else {
                None
            }
        });

    let existing = if let Some(conv_id) = conversation_id {
        widget::owl_get_widgets_for_conversation(conn, conv_id).unwrap_or_default()
    } else {
        widget::owl_get_all_widgets(conn).unwrap_or_default()
    };
    let offset = existing.len() as f64 * WIDGET_CASCADE_STEP;
    let position = Position { x: WIDGET_DEFAULT_X + offset, y: WIDGET_DEFAULT_Y + offset };

    let sanitized_entity = entity_id.replace([':', '/', '#', ' '], "_");
    let conv_suffix = conversation_id
        .map(|c| format!("_{}", c.replace([':', '/', '#', ' '], "_")))
        .unwrap_or_default();
    let widget_obj = Widget {
        id: format!("foundation:Widget_{widget_type}_{sanitized_entity}{conv_suffix}"),
        widget_type: widget_type.to_string(),
        entity_id: entity_id.to_string(),
        content,
        position,
        size: Size { width: WIDGET_DEFAULT_WIDTH, height: WIDGET_DEFAULT_HEIGHT },
        window_state: widget::WindowState::Normal,
        conversation_iri: conversation_id.map(String::from),
    };

    match widget::owl_insert_widget(conn, &widget_obj) {
        Ok(_) => {
            if let Some(app_handle) = app {
                app_handle.emit("widget-added", widget_obj.clone()).ok();
            }

            match serde_json::to_value(widget_obj) {
                Ok(value) => ToolResult { success: true, result: Some(value), error: None, concept: None },
                Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
            }
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
    }
}


fn blackboard_remove_one(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let widget_id = match args.get("widget_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: widget_id".to_string()),
            concept: None,
        },
    };

    match widget::owl_delete_widget(conn, widget_id) {
        Ok(_) => {
            if let Some(app_handle) = app {
                app_handle.emit("widget-removed", widget_id.to_string()).ok();
            }

            ToolResult {
                success: true,
                result: None,
                error: None,
                concept: None,
            }
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
            concept: None,
        },
    }
}
