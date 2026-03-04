use serde_json::Value;
use tauri::Emitter;
use crate::commands::widget::{self, Widget, Position, Size};
use super::ToolResult;

const WIDGET_CASCADE_STEP: f64 = 50.0;
const WIDGET_DEFAULT_X: f64 = 100.0;
const WIDGET_DEFAULT_Y: f64 = 100.0;
const WIDGET_DEFAULT_WIDTH: f64 = 400.0;
const WIDGET_DEFAULT_HEIGHT: f64 = 600.0;

pub fn blackboard_show(conn: &rusqlite::Connection) -> ToolResult {
    match widget::db_get_all_widgets(conn) {
        Ok(widgets) => match serde_json::to_value(widgets) {
            Ok(value) => ToolResult { success: true, result: Some(value), error: None },
            Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()) },
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}

pub fn blackboard_add_widget(
    conn: &rusqlite::Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let widget_type = match args.get("widget_type").and_then(|v| v.as_str()) {
        Some(t) => t.to_lowercase(),
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: widget_type".to_string()),
        },
    };

    let params = match args.get("params") {
        Some(p) => p,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: params".to_string()),
        },
    };

    if widget_type != "inspector" {
        return ToolResult {
            success: false,
            result: None,
            error: Some(format!(
                "Unknown widget type: {}. Available types: Inspector", widget_type,
            )),
        };
    }

    let entity_id = match params.get("entity_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Inspector widget requires 'entity_id' in params".to_string()),
        },
    };

    let position = match widget::db_get_all_widgets(conn) {
        Ok(widgets) => {
            let offset = widgets.len() as f64 * WIDGET_CASCADE_STEP;
            Position { x: WIDGET_DEFAULT_X + offset, y: WIDGET_DEFAULT_Y + offset }
        },
        Err(_) => Position { x: WIDGET_DEFAULT_X, y: WIDGET_DEFAULT_Y },
    };

    let widget_obj = Widget {
        id: format!("widget_{}_{}", widget_type, chrono::Utc::now().timestamp_millis()),
        widget_type: widget_type.to_string(),
        entity_id: entity_id.to_string(),
        position,
        size: Size { width: WIDGET_DEFAULT_WIDTH, height: WIDGET_DEFAULT_HEIGHT },
    };

    match widget::db_insert_widget(conn, &widget_obj) {
        Ok(_) => {
            if let Some(app_handle) = app {
                app_handle.emit("widget-added", widget_obj.clone()).ok();
            }

            match serde_json::to_value(widget_obj) {
                Ok(value) => ToolResult { success: true, result: Some(value), error: None },
                Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()) },
            }
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}

pub fn blackboard_remove(
    conn: &rusqlite::Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    let widget_id = match args.get("widget_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: widget_id".to_string()),
        },
    };

    match widget::db_delete_widget(conn, widget_id) {
        Ok(_) => {
            if let Some(app_handle) = app {
                app_handle.emit("widget-removed", widget_id.to_string()).ok();
            }

            ToolResult {
                success: true,
                result: Some(serde_json::json!({"message": "Widget removed"})),
                error: None,
            }
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}

pub fn blackboard_clear(conn: &rusqlite::Connection, app: Option<&tauri::AppHandle>) -> ToolResult {
    match widget::db_clear_all_widgets(conn) {
        Ok(_) => {
            if let Some(app_handle) = app {
                app_handle.emit("widgets-cleared", ()).ok();
            }

            ToolResult {
                success: true,
                result: Some(serde_json::json!({"message": "All widgets cleared"})),
                error: None,
            }
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}
