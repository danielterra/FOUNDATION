use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use crate::owl::{Connection, DbExecutor, Object, Individual};
use crate::owl::vocabulary::rdf;

const WIDGET_CLASS: &str = "foundation:Widget";
const WIDGET_ICON: &str = "widgets";
const PRED_WIDGET_TYPE: &str = "foundation:widgetType";
const PRED_ENTITY_ID: &str = "foundation:widgetEntityId";
const PRED_CONTENT: &str = "foundation:widgetContent";
const PRED_POSITION_X: &str = "foundation:widgetPositionX";
const PRED_POSITION_Y: &str = "foundation:widgetPositionY";
const PRED_SIZE_WIDTH: &str = "foundation:widgetSizeWidth";
const PRED_SIZE_HEIGHT: &str = "foundation:widgetSizeHeight";
const WIDGET_ORIGIN: &str = "widget";
const PRED_WINDOW_STATE: &str = "foundation:widgetWindowState";

const DEFAULT_POS_X: f64 = 100.0;
const DEFAULT_POS_Y: f64 = 100.0;
const DEFAULT_WIDTH_MERMAID: f64 = 600.0;
const DEFAULT_HEIGHT_MERMAID: f64 = 500.0;
const DEFAULT_WIDTH_META_PROCESS: f64 = 400.0;
const DEFAULT_HEIGHT_META_PROCESS: f64 = 600.0;
const DEFAULT_WIDTH_STANDARD: f64 = 400.0;
const DEFAULT_HEIGHT_STANDARD: f64 = 600.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WindowState {
    #[default]
    Normal,
    Minimized,
    Maximized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    pub widget_type: String,
    pub entity_id: String,
    pub content: Option<String>,
    pub position: Position,
    pub size: Size,
    pub window_state: WindowState,
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
    pub supports_entity: bool,
    pub default_size: Size,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetDefinitionInfo {
    pub widget_type: String,
    pub description: String,
}

fn str_obj(value: impl Into<String>) -> Object {
    Object::Literal {
        value: value.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

fn prop_str(ind: &Individual, pred: &str) -> Option<String> {
    ind.properties.iter()
        .find(|(p, _)| p == pred)
        .and_then(|(_, v)| v.as_literal())
}

fn prop_f64(ind: &Individual, pred: &str) -> Option<f64> {
    ind.properties.iter()
        .find(|(p, _)| p == pred)
        .and_then(|(_, v)| match v {
            Object::Number(n) => Some(*n),
            Object::Integer(i) => Some(*i as f64),
            _ => None,
        })
}

fn individual_to_widget(ind: Individual) -> Option<Widget> {
    let widget_type = prop_str(&ind, PRED_WIDGET_TYPE)?;
    let entity_id = prop_str(&ind, PRED_ENTITY_ID)?;
    let x = prop_f64(&ind, PRED_POSITION_X)?;
    let y = prop_f64(&ind, PRED_POSITION_Y)?;
    let width = prop_f64(&ind, PRED_SIZE_WIDTH)?;
    let height = prop_f64(&ind, PRED_SIZE_HEIGHT)?;
    let content = prop_str(&ind, PRED_CONTENT);

    let window_state = prop_str(&ind, PRED_WINDOW_STATE)
        .and_then(|s| match s.as_str() {
            "minimized" => Some(WindowState::Minimized),
            "maximized" => Some(WindowState::Maximized),
            _ => Some(WindowState::Normal),
        })
        .unwrap_or_default();

    Some(Widget {
        id: ind.iri,
        widget_type,
        entity_id,
        content,
        position: Position { x, y },
        size: Size { width, height },
        window_state,
    })
}

pub fn owl_insert_widget(conn: &mut Connection, widget: &Widget) -> Result<(), String> {
    let ind = Individual::new(&widget.id);
    ind.assert(conn, WIDGET_CLASS, &widget.widget_type, WIDGET_ICON, WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_WIDGET_TYPE, vec![str_obj(&widget.widget_type)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_ENTITY_ID, vec![str_obj(&widget.entity_id)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_POSITION_X, vec![Object::Number(widget.position.x)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_POSITION_Y, vec![Object::Number(widget.position.y)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_SIZE_WIDTH, vec![Object::Number(widget.size.width)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_SIZE_HEIGHT, vec![Object::Number(widget.size.height)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    if let Some(content) = &widget.content {
        ind.add_property(conn, PRED_CONTENT, vec![str_obj(content)], WIDGET_ORIGIN)
            .map_err(|e| e.to_string())?;
    }
    let state_str = match widget.window_state {
        WindowState::Normal => "normal",
        WindowState::Minimized => "minimized",
        WindowState::Maximized => "maximized",
    };
    ind.add_property(conn, PRED_WINDOW_STATE, vec![str_obj(state_str)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn owl_get_all_widgets(conn: &Connection) -> Result<Vec<Widget>, String> {
    let widget_iris = crate::owl::find_entities_with_property(conn, rdf::TYPE, WIDGET_CLASS)
        .map_err(|e| e.to_string())?;
    let mut widgets = Vec::new();
    for iri in widget_iris {
        if let Ok(Some(ind)) = Individual::get(conn, &iri) {
            if let Some(widget) = individual_to_widget(ind) {
                widgets.push(widget);
            }
        }
    }
    Ok(widgets)
}

pub fn owl_delete_widget(conn: &mut Connection, widget_id: &str) -> Result<(), String> {
    let exists = Individual::get(conn, widget_id)
        .map_err(|e| e.to_string())?
        .is_some();
    if !exists {
        return Err(format!("Widget not found: {}", widget_id));
    }
    Individual::retract(conn, widget_id, WIDGET_ORIGIN)
        .map_err(|e| e.to_string())
}

fn owl_update_widget_position(
    conn: &mut Connection,
    widget_id: &str,
    position: &Position,
) -> Result<(), String> {
    let ind = Individual::new(widget_id);
    ind.add_property(conn, PRED_POSITION_X, vec![Object::Number(position.x)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_POSITION_Y, vec![Object::Number(position.y)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())
}

fn owl_update_widget_size(
    conn: &mut Connection,
    widget_id: &str,
    size: &Size,
) -> Result<(), String> {
    let ind = Individual::new(widget_id);
    ind.add_property(conn, PRED_SIZE_WIDTH, vec![Object::Number(size.width)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_SIZE_HEIGHT, vec![Object::Number(size.height)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())
}

fn owl_update_widget_window_state(
    conn: &mut Connection,
    widget_id: &str,
    state: &WindowState,
) -> Result<(), String> {
    let state_str = match state {
        WindowState::Normal => "normal",
        WindowState::Minimized => "minimized",
        WindowState::Maximized => "maximized",
    };
    let ind = Individual::new(widget_id);
    ind.add_property(conn, PRED_WINDOW_STATE, vec![str_obj(state_str)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())
}

fn owl_update_widget_content(
    conn: &mut Connection,
    widget_id: &str,
    content: &str,
) -> Result<(), String> {
    let ind = Individual::new(widget_id);
    ind.add_property(conn, PRED_CONTENT, vec![str_obj(content)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())
}

#[allow(non_snake_case)]
pub fn blackboard__list_widget_types() -> Vec<WidgetType> {
    vec![
        WidgetType {
            id: "inspector".to_string(),
            name: "Inspector".to_string(),
            description: "Display detailed information about a class or instance".to_string(),
            supports_entity: true,
            default_size: Size { width: DEFAULT_WIDTH_STANDARD, height: DEFAULT_HEIGHT_STANDARD },
        },
        WidgetType {
            id: "mermaid".to_string(),
            name: "Mermaid Diagram".to_string(),
            description: "Display a Mermaid diagram".to_string(),
            supports_entity: false,
            default_size: Size { width: DEFAULT_WIDTH_MERMAID, height: DEFAULT_HEIGHT_MERMAID },
        },
        WidgetType {
            id: "process_status".to_string(),
            name: "Process Status".to_string(),
            description: "Display real-time execution status of a BPMN process".to_string(),
            supports_entity: true,
            default_size: Size { width: DEFAULT_WIDTH_STANDARD, height: DEFAULT_HEIGHT_STANDARD },
        },
        WidgetType {
            id: "connector_credential".to_string(),
            name: "Connector Credentials".to_string(),
            description: "Configure authentication credentials for an external service connector".to_string(),
            supports_entity: true,
            default_size: Size { width: DEFAULT_WIDTH_STANDARD, height: DEFAULT_HEIGHT_STANDARD },
        },
        WidgetType {
            id: "connector_manager".to_string(),
            name: "Connector Manager".to_string(),
            description: "Export, import and manage credentials for a service connector".to_string(),
            supports_entity: true,
            default_size: Size { width: DEFAULT_WIDTH_STANDARD, height: DEFAULT_HEIGHT_STANDARD },
        },
        WidgetType {
            id: "meta_process".to_string(),
            name: "MetaProcess".to_string(),
            description: "Interactive flow diagram of a MetaProcess".to_string(),
            supports_entity: true,
            default_size: Size { width: DEFAULT_WIDTH_META_PROCESS, height: DEFAULT_HEIGHT_META_PROCESS },
        },
    ]
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__get_widgets(executor: State<'_, DbExecutor>) -> Result<Vec<Widget>, String> {
    executor.read(|conn| {
        owl_get_all_widgets(conn)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__add_widget(
    app: AppHandle,
    widget_type: String,
    entity_id: String,
    content: Option<String>,
    position: Option<Position>,
    size: Option<Size>,
    executor: State<'_, DbExecutor>
) -> Result<Widget, String> {
    let valid_types = blackboard__list_widget_types();
    if !valid_types.iter().any(|t| t.id == widget_type) {
        return Err(format!("Invalid widget type: {}. Available types: {:?}",
            widget_type,
            valid_types.iter().map(|t| &t.id).collect::<Vec<_>>()
        ));
    }

    let sanitized_entity = entity_id.replace([':', '/', '#', ' '], "_");
    let widget_def = valid_types.iter().find(|t| t.id == widget_type).unwrap();
    let default_size = widget_def.default_size.clone();

    let widget = Widget {
        id: format!("foundation:Widget_{widget_type}_{sanitized_entity}"),
        widget_type,
        entity_id,
        content,
        position: position.unwrap_or(Position { x: DEFAULT_POS_X, y: DEFAULT_POS_Y }),
        size: size.unwrap_or(default_size),
        window_state: WindowState::Normal,
    };

    app.emit("widget-added", widget.clone()).ok();

    let widget_clone = widget.clone();
    let executor_clone = executor.inner().clone();
    tokio::spawn(async move {
        executor_clone.write(move |conn| {
            owl_insert_widget(conn, &widget_clone)?;
            Ok(widget_clone.id.clone())
        }).await.ok();
    });

    Ok(widget)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__remove_widget(
    app: AppHandle,
    widget_id: String,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    app.emit("widget-removed", widget_id.clone()).ok();

    let executor_clone = executor.inner().clone();
    tokio::spawn(async move {
        executor_clone.write(move |conn| {
            owl_delete_widget(conn, &widget_id)?;
            Ok("deleted".to_string())
        }).await.ok();
    });

    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__update_widget_position(
    widget_id: String,
    position: Position,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    executor.write(move |conn| {
        owl_update_widget_position(conn, &widget_id, &position)?;
        Ok("updated".to_string())
    }).await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__update_widget_size(
    widget_id: String,
    size: Size,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    executor.write(move |conn| {
        owl_update_widget_size(conn, &widget_id, &size)?;
        Ok("updated".to_string())
    }).await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__update_widget_window_state(
    widget_id: String,
    window_state: WindowState,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    executor.write(move |conn| {
        owl_update_widget_window_state(conn, &widget_id, &window_state)?;
        Ok("updated".to_string())
    }).await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__update_widget_content(
    app: AppHandle,
    widget_id: String,
    content: String,
    executor: State<'_, DbExecutor>
) -> Result<(), String> {
    let widget_id_clone = widget_id.clone();
    executor.write(move |conn| {
        owl_update_widget_content(conn, &widget_id_clone, &content)?;
        if let Ok(Some(ind)) = Individual::get(conn, &widget_id_clone) {
            let is_mermaid = ind.properties.iter()
                .any(|(p, v)| p == PRED_WIDGET_TYPE && v.as_literal().map_or(false, |s| s == "mermaid"));
            if is_mermaid {
                if let Some(entity_id) = ind.properties.iter()
                    .find(|(p, _)| p == PRED_ENTITY_ID)
                    .and_then(|(_, v)| v.as_literal())
                {
                    Individual::new(&entity_id)
                        .add_property(conn, "foundation:diagramSource", vec![str_obj(&content)], WIDGET_ORIGIN)
                        .ok();
                }
            }
        }
        Ok("updated".to_string())
    }).await?;
    app.emit("widget-content-updated", widget_id).ok();
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__list_widget_definitions(
    concept_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<WidgetDefinitionInfo>, String> {
    executor.read(move |conn| {
        let widget_iris = crate::owl::find_entities_with_property(conn, rdf::TYPE, "foundation:WidgetDefinition")
            .map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for iri in widget_iris {
            let ind = match Individual::get(conn, &iri) {
                Ok(Some(ind)) => ind,
                _ => continue,
            };

            if let Some(ref filter_iri) = concept_iri {
                let supports_entity = ind.properties.iter()
                    .find(|(p, _)| p == "foundation:widgetDefSupportsEntity")
                    .and_then(|(_, v)| v.as_literal())
                    .map(|s| s == "true")
                    .unwrap_or(false);

                if !supports_entity {
                    continue;
                }

                let supported_concepts: Vec<String> = ind.properties.iter()
                    .filter(|(p, _)| p == "foundation:widgetDefSupportedConcepts")
                    .filter_map(|(_, v)| v.as_iri().map(String::from))
                    .collect();

                if !supported_concepts.is_empty() && !supported_concepts.iter().any(|c| c == filter_iri) {
                    continue;
                }
            }

            let id = ind.properties.iter()
                .find(|(p, _)| p == "foundation:widgetDefId")
                .and_then(|(_, v)| v.as_literal())
                .unwrap_or_default();

            let description = ind.properties.iter()
                .find(|(p, _)| p == "foundation:widgetDefDescription")
                .and_then(|(_, v)| v.as_literal())
                .unwrap_or_default();

            results.push(WidgetDefinitionInfo {
                widget_type: id.to_string(),
                description: description.to_string(),
            });
        }

        Ok(results)
    }).await
}

