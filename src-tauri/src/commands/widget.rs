use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use crate::owl::{Connection, DbExecutor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    pub widget_type: String,
    pub entity_id: String,
    pub position: Position,
    pub size: Size,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetType {
    pub id: String,
    pub name: String,
    pub description: String,
}

pub fn ensure_widget_table(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS widgets (
            id TEXT PRIMARY KEY,
            widget_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            position_x REAL NOT NULL,
            position_y REAL NOT NULL,
            size_width REAL NOT NULL,
            size_height REAL NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| format!("Failed to create widgets table: {}", e))?;
    Ok(())
}

pub fn db_insert_widget(conn: &Connection, widget: &Widget) -> Result<bool, String> {
    ensure_widget_table(conn)?;

    let rows = conn.execute(
        "INSERT OR IGNORE INTO widgets \
         (id, widget_type, entity_id, position_x, position_y, size_width, size_height, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            widget.id,
            widget.widget_type,
            widget.entity_id,
            widget.position.x,
            widget.position.y,
            widget.size.width,
            widget.size.height,
            chrono::Utc::now().timestamp_millis(),
        ],
    ).map_err(|e| format!("Failed to insert widget: {}", e))?;

    Ok(rows > 0)
}

pub fn db_get_all_widgets(conn: &Connection) -> Result<Vec<Widget>, String> {
    ensure_widget_table(conn)?;

    let mut stmt = conn.prepare(
        "SELECT id, widget_type, entity_id, position_x, position_y, size_width, size_height
         FROM widgets
         ORDER BY created_at DESC"
    ).map_err(|e| format!("Failed to prepare query: {}", e))?;

    let widgets = stmt.query_map([], |row| {
        Ok(Widget {
            id: row.get(0)?,
            widget_type: row.get(1)?,
            entity_id: row.get(2)?,
            position: Position {
                x: row.get(3)?,
                y: row.get(4)?,
            },
            size: Size {
                width: row.get(5)?,
                height: row.get(6)?,
            },
        })
    }).map_err(|e| format!("Failed to query widgets: {}", e))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| format!("Failed to collect widgets: {}", e))?;

    Ok(widgets)
}

pub fn db_delete_widget(conn: &Connection, widget_id: &str) -> Result<(), String> {
    ensure_widget_table(conn)?;

    let rows_affected = conn.execute(
        "DELETE FROM widgets WHERE id = ?1",
        rusqlite::params![widget_id],
    ).map_err(|e| format!("Failed to delete widget: {}", e))?;

    if rows_affected == 0 {
        return Err(format!("Widget not found: {}", widget_id));
    }

    Ok(())
}

fn db_update_widget_position(
    conn: &Connection,
    widget_id: &str,
    position: &Position,
) -> Result<(), String> {
    ensure_widget_table(conn)?;

    let rows_affected = conn.execute(
        "UPDATE widgets SET position_x = ?1, position_y = ?2 WHERE id = ?3",
        rusqlite::params![position.x, position.y, widget_id],
    ).map_err(|e| format!("Failed to update widget position: {}", e))?;

    if rows_affected == 0 {
        return Err(format!("Widget not found: {}", widget_id));
    }

    Ok(())
}

fn db_update_widget_size(conn: &Connection, widget_id: &str, size: &Size) -> Result<(), String> {
    ensure_widget_table(conn)?;

    let rows_affected = conn.execute(
        "UPDATE widgets SET size_width = ?1, size_height = ?2 WHERE id = ?3",
        rusqlite::params![size.width, size.height, widget_id],
    ).map_err(|e| format!("Failed to update widget size: {}", e))?;

    if rows_affected == 0 {
        return Err(format!("Widget not found: {}", widget_id));
    }

    Ok(())
}

pub fn db_clear_all_widgets(conn: &Connection) -> Result<(), String> {
    ensure_widget_table(conn)?;

    conn.execute("DELETE FROM widgets", [])
        .map_err(|e| format!("Failed to clear widgets: {}", e))?;

    Ok(())
}

/// List all available widget types
#[tauri::command]
#[allow(non_snake_case)]
pub fn widget__list_types() -> Vec<WidgetType> {
    vec![
        WidgetType {
            id: "inspector".to_string(),
            name: "Inspector".to_string(),
            description: "Display detailed information about a class or instance".to_string(),
        },
    ]
}

/// Get all widgets currently on the blackboard
#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget__get_all(executor: State<'_, DbExecutor>) -> Result<Vec<Widget>, String> {
    executor.read(|conn| {
        db_get_all_widgets(conn)
    }).await
}

/// Add a new widget to the blackboard
#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget__add(
    app: AppHandle,
    widget_type: String,
    entity_id: String,
    position: Option<Position>,
    size: Option<Size>,
    executor: State<'_, DbExecutor>
) -> Result<Widget, String> {
    let valid_types = widget__list_types();
    if !valid_types.iter().any(|t| t.id == widget_type) {
        return Err(format!("Invalid widget type: {}. Available types: {:?}",
            widget_type,
            valid_types.iter().map(|t| &t.id).collect::<Vec<_>>()
        ));
    }

    let sanitized_entity = entity_id.replace([':', '/', '#', ' '], "_");
    let widget = Widget {
        id: format!("widget_{}_{}", widget_type, sanitized_entity),
        widget_type,
        entity_id,
        position: position.unwrap_or(Position { x: 100.0, y: 100.0 }),
        size: size.unwrap_or(Size { width: 400.0, height: 600.0 }),
    };

    app.emit("widget-added", widget.clone()).ok();

    let widget_clone = widget.clone();
    let executor_clone = executor.inner().clone();
    tokio::spawn(async move {
        executor_clone.write(move |conn| {
            db_insert_widget(conn, &widget_clone)?;
            Ok(widget_clone.id.clone())
        }).await.ok();
    });

    Ok(widget)
}

/// Remove a widget from the blackboard
#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget__remove(
    app: AppHandle,
    widget_id: String,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    app.emit("widget-removed", widget_id.clone()).ok();

    let executor_clone = executor.inner().clone();
    tokio::spawn(async move {
        executor_clone.write(move |conn| {
            db_delete_widget(conn, &widget_id)?;
            Ok("deleted".to_string())
        }).await.ok();
    });

    Ok(())
}

/// Update widget position
#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget__update_position(
    widget_id: String,
    position: Position,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    executor.write(move |conn| {
        db_update_widget_position(conn, &widget_id, &position)?;
        Ok("updated".to_string())
    }).await?;
    Ok(())
}

/// Update widget size
#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget__update_size(
    widget_id: String,
    size: Size,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    executor.write(move |conn| {
        db_update_widget_size(conn, &widget_id, &size)?;
        Ok("updated".to_string())
    }).await?;
    Ok(())
}

/// Clear all widgets from the blackboard
#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget__clear_all(
    app: AppHandle,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    app.emit("widgets-cleared", ()).ok();

    let executor_clone = executor.inner().clone();
    tokio::spawn(async move {
        executor_clone.write(|conn| {
            db_clear_all_widgets(conn)?;
            Ok("cleared".to_string())
        }).await.ok();
    });

    Ok(())
}
