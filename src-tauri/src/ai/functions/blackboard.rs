use serde_json::Value;
use tauri::Emitter;
use crate::commands::widget::{self, Widget, Position, Size};
use super::FunctionResult;

pub fn blackboard_show(conn: &rusqlite::Connection) -> FunctionResult {
    match widget::db_get_all_widgets(conn) {
        Ok(widgets) => match serde_json::to_value(widgets) {
            Ok(value) => FunctionResult { success: true, result: Some(value), error: None },
            Err(e) => FunctionResult { success: false, result: None, error: Some(e.to_string()) },
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}

pub fn blackboard_add_widget(conn: &rusqlite::Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
    let widget_type = match args.get("widget_type").and_then(|v| v.as_str()) {
        Some(t) => t.to_lowercase(),
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: widget_type".to_string()),
        },
    };

    let params = match args.get("params") {
        Some(p) => p,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: params".to_string()),
        },
    };

    // Validate widget type
    if widget_type != "inspector" {
        return FunctionResult {
            success: false,
            result: None,
            error: Some(format!("Unknown widget type: {}. Available types: Inspector", widget_type)),
        };
    }

    // Extract entity_id from params for Inspector widget
    let entity_id = match params.get("entity_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Inspector widget requires 'entity_id' in params".to_string()),
        },
    };

    // Auto-position: calculate position based on existing widgets
    let position = match widget::db_get_all_widgets(conn) {
        Ok(widgets) => {
            // Simple auto-layout: stack widgets diagonally with offset
            let offset = widgets.len() as f64 * 50.0;
            Position { x: 100.0 + offset, y: 100.0 + offset }
        },
        Err(_) => Position { x: 100.0, y: 100.0 }, // Fallback to default
    };

    let widget_obj = Widget {
        id: format!("widget_{}_{}", widget_type, chrono::Utc::now().timestamp_millis()),
        widget_type: widget_type.to_string(),
        entity_id: entity_id.to_string(),
        position,
        size: Size { width: 400.0, height: 600.0 },
    };

    match widget::db_insert_widget(conn, &widget_obj) {
        Ok(_) => {
            // Emit event to frontend if app handle is available
            if let Some(app_handle) = app {
                app_handle.emit("widget-added", widget_obj.clone()).ok();
            }

            match serde_json::to_value(widget_obj) {
                Ok(value) => FunctionResult { success: true, result: Some(value), error: None },
                Err(e) => FunctionResult { success: false, result: None, error: Some(e.to_string()) },
            }
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}

pub fn blackboard_remove(conn: &rusqlite::Connection, args: &Value, app: Option<&tauri::AppHandle>) -> FunctionResult {
    let widget_id = match args.get("widget_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return FunctionResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: widget_id".to_string()),
        },
    };

    match widget::db_delete_widget(conn, widget_id) {
        Ok(_) => {
            // Emit event to frontend if app handle is available
            if let Some(app_handle) = app {
                app_handle.emit("widget-removed", widget_id.to_string()).ok();
            }

            FunctionResult {
                success: true,
                result: Some(serde_json::json!({"message": "Widget removed"})),
                error: None,
            }
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}

pub fn blackboard_clear(conn: &rusqlite::Connection, app: Option<&tauri::AppHandle>) -> FunctionResult {
    match widget::db_clear_all_widgets(conn) {
        Ok(_) => {
            // Emit event to frontend if app handle is available
            if let Some(app_handle) = app {
                app_handle.emit("widgets-cleared", ()).ok();
            }

            FunctionResult {
                success: true,
                result: Some(serde_json::json!({"message": "All widgets cleared"})),
                error: None,
            }
        },
        Err(e) => FunctionResult {
            success: false,
            result: None,
            error: Some(e),
        },
    }
}
