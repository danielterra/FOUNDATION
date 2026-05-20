use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use crate::owl::{Connection, DbExecutor, Object, Individual};
use crate::owl::vocabulary::rdf;

const WIDGET_CLASS: &str = "foundation:Widget";
const WIDGET_ICON: &str = "widgets";
const BLACKBOARD_CLASS: &str = "foundation:Blackboard";
const BLACKBOARD_ICON: &str = "dashboard";
const PRED_WIDGET_TYPE: &str = "foundation:widgetType";
const PRED_ENTITY_ID: &str = "foundation:widgetEntityId";
const PRED_POSITION_X: &str = "foundation:widgetPositionX";
const PRED_POSITION_Y: &str = "foundation:widgetPositionY";
const PRED_SIZE_WIDTH: &str = "foundation:widgetSizeWidth";
const PRED_SIZE_HEIGHT: &str = "foundation:widgetSizeHeight";
const WIDGET_ORIGIN: &str = "widget";
const PRED_WINDOW_STATE: &str = "foundation:widgetWindowState";
const PRED_BLACKBOARD: &str = "foundation:onBlackboard";
const PRED_FOR_CONVERSATION: &str = "foundation:forConversation";

pub const DEFAULT_BLACKBOARD_IRI: &str = "foundation:DefaultBlackboard";

const DEFAULT_POS_X: f64 = 100.0;
const DEFAULT_POS_Y: f64 = 100.0;

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
    pub position: Position,
    pub size: Size,
    pub window_state: WindowState,
    pub blackboard_iri: String,
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
    /// Class IRI this widget is designed for. `"owl:Thing"` means any entity.
    pub supported_class: String,
    pub default_size: Size,
    pub icon: String,
    /// If set, this widget is AI-creatable and this text is shown to the AI in the system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetDefinitionInfo {
    pub widget_type: String,
    pub description: String,
    pub icon: String,
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

fn prop_iri(ind: &Individual, pred: &str) -> Option<String> {
    ind.properties.iter()
        .find(|(p, _)| p == pred)
        .and_then(|(_, v)| v.as_iri().map(String::from))
}

pub fn resolve_widget_type(conn: &Connection, entity_id: &str, requested_type: &str) -> String {
    if !requested_type.is_empty() {
        return requested_type.to_string();
    }
    let class_iri = crate::owl::get_iri_property(conn, entity_id, "rdf:type")
        .ok()
        .flatten();
    if let Some(class_iri) = class_iri {
        let default = crate::owl::get_literal_property(conn, &class_iri, "foundation:defaultWidgetType")
            .ok()
            .flatten();
        if let Some(t) = default {
            if !t.is_empty() {
                return t;
            }
        }
    }
    "inspector".to_string()
}

fn individual_to_widget(ind: Individual) -> Option<Widget> {
    let widget_type = prop_str(&ind, PRED_WIDGET_TYPE)?;
    let entity_id = prop_str(&ind, PRED_ENTITY_ID)?;
    let x = prop_f64(&ind, PRED_POSITION_X)?;
    let y = prop_f64(&ind, PRED_POSITION_Y)?;
    let width = prop_f64(&ind, PRED_SIZE_WIDTH)?;
    let height = prop_f64(&ind, PRED_SIZE_HEIGHT)?;
    let blackboard_iri = prop_iri(&ind, PRED_BLACKBOARD)?;

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
        position: Position { x, y },
        size: Size { width, height },
        window_state,
        blackboard_iri,
    })
}

fn sanitize_for_iri(s: &str) -> String {
    s.replace([':', '/', '#', ' '], "_")
}

/// Returns the IRI of the Blackboard tied to a conversation (lazy-create).
/// If `conversation_iri` is None, returns the singleton DefaultBlackboard.
pub fn resolve_blackboard_iri(
    conn: &mut Connection,
    conversation_iri: Option<&str>,
) -> Result<String, String> {
    let (bb_iri, label) = match conversation_iri {
        Some(c) => (
            format!("foundation:Blackboard_for_{}", sanitize_for_iri(c)),
            format!("Blackboard for {}", c),
        ),
        None => (
            DEFAULT_BLACKBOARD_IRI.to_string(),
            "Default Blackboard".to_string(),
        ),
    };

    let exists = Individual::get(conn, &bb_iri)
        .map_err(|e| e.to_string())?
        .is_some();

    if !exists {
        let bb = Individual::new(&bb_iri);
        bb.assert(conn, BLACKBOARD_CLASS, &label, BLACKBOARD_ICON, WIDGET_ORIGIN)
            .map_err(|e| e.to_string())?;
        if let Some(c) = conversation_iri {
            bb.add_property(conn, PRED_FOR_CONVERSATION, vec![Object::Iri(c.to_string())], WIDGET_ORIGIN)
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(bb_iri)
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
    let state_str = match widget.window_state {
        WindowState::Normal => "normal",
        WindowState::Minimized => "minimized",
        WindowState::Maximized => "maximized",
    };
    ind.add_property(conn, PRED_WINDOW_STATE, vec![str_obj(state_str)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())?;
    ind.add_property(conn, PRED_BLACKBOARD, vec![Object::Iri(widget.blackboard_iri.clone())], WIDGET_ORIGIN)
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

pub fn owl_get_widgets_for_blackboard(conn: &Connection, blackboard_iri: &str) -> Result<Vec<Widget>, String> {
    Ok(owl_get_all_widgets(conn)?.into_iter()
        .filter(|w| w.blackboard_iri == blackboard_iri)
        .collect())
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
    let ind = match Individual::get(conn, widget_id).map_err(|e| e.to_string())? {
        Some(ind) => ind,
        None => return Ok(()),
    };
    let entity_id = match prop_str(&ind, PRED_ENTITY_ID) {
        Some(id) => id,
        None => return Ok(()),
    };
    Individual::new(&entity_id)
        .add_property(conn, "foundation:diagramSource", vec![str_obj(content)], WIDGET_ORIGIN)
        .map_err(|e| e.to_string())
}

/// Returns a static system prompt section describing every widget type.
/// Intended to be appended to the cached system prompt block so Claude always
/// knows what widgets exist and how to use them — without querying at runtime.
pub fn widget_system_context(conn: &Connection) -> String {
    let all_types = blackboard__list_widget_types(conn);

    let ai_creatable: Vec<&WidgetType> = all_types.iter()
        .filter(|t| t.usage_note.is_some())
        .collect();

    if ai_creatable.is_empty() {
        return String::new();
    }

    let list = ai_creatable.iter()
        .map(|t| format!("- **{}** — {}", t.id, t.usage_note.as_deref().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "## AI-creatable widgets\n\
         Create the required entity first, then pass its IRI in `speak.iris` to display it:\n\
         {list}"
    )
}

#[allow(non_snake_case)]
pub fn blackboard__list_widget_types(conn: &Connection) -> Vec<WidgetType> {
    let iris = match Individual::find_by_class_with_date_range(conn, "foundation:WidgetType", None, None, false) {
        Ok(iris) => iris,
        Err(_) => return vec![],
    };

    let mut types = Vec::new();
    for iri in iris {
        if let Ok(Some(ind)) = Individual::get(conn, &iri) {
            let id = match prop_str(&ind, "foundation:widgetTypeId") {
                Some(id) => id,
                None => continue,
            };
            let name = ind.label.clone().unwrap_or_else(|| id.clone());
            let description = ind.comment.clone().unwrap_or_default();
            let supported_class = prop_str(&ind, "foundation:widgetSupportedClass")
                .unwrap_or_else(|| "owl:Thing".to_string());
            let width = match prop_f64(&ind, "foundation:widgetDefaultWidth") {
                Some(w) => w,
                None => continue,
            };
            let height = match prop_f64(&ind, "foundation:widgetDefaultHeight") {
                Some(h) => h,
                None => continue,
            };
            let icon = ind.icon.clone().unwrap_or_default();
            let usage_note = prop_str(&ind, "foundation:widgetUsageNote");
            types.push(WidgetType {
                id,
                name,
                description,
                supported_class,
                default_size: Size { width, height },
                icon,
                usage_note,
            });
        }
    }
    types
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiGraphData {
    pub nodes: Vec<crate::commands::entity::GraphNode>,
    pub links: Vec<crate::commands::entity::GraphLink>,
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__get_graph_data(
    entity_iris: Vec<String>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let result = executor.read(move |conn| {
        if entity_iris.is_empty() {
            return serde_json::to_string(&MultiGraphData { nodes: vec![], links: vec![] })
                .map_err(|e| e.to_string());
        }

        let groups = crate::owl::load_graph_node_groups(conn);
        let (group_class, group_individual, _) = groups;

        // Build nodes from the provided IRIs only
        let things = crate::owl::Thing::get_batch(conn, &entity_iris);
        let mut nodes: Vec<crate::commands::entity::GraphNode> = entity_iris.iter().map(|iri| {
            let t = things.get(iri);
            let is_class = crate::owl::Class::get(conn, iri).map(|c| c.is_some()).unwrap_or(false);
            crate::commands::entity::GraphNode {
                id: iri.clone(),
                label: t.map(|t| t.label.clone()).unwrap_or_else(|| iri.clone()),
                icon: t.and_then(|t| t.icon.clone()),
                group: if is_class { group_class } else { group_individual },
                is_broken_ref: None,
                is_literal: None,
            }
        }).collect();

        // Deduplicate nodes (entity_iris may contain duplicates)
        nodes.dedup_by(|a, b| a.id == b.id);

        // Query relations where BOTH source and target are in the provided set
        let placeholders: String = entity_iris.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            "SELECT t.subject, t.object, COALESCE(p.label, t.predicate) as link_label
             FROM triples t
             LEFT JOIN (
                 SELECT subject, object_value as label
                 FROM triples
                 WHERE predicate = 'rdfs:label' AND object_type = 'literal' AND retracted = 0
             ) p ON p.subject = t.predicate
             WHERE t.subject IN ({placeholders})
               AND t.object IN ({placeholders})
               AND t.object_type = 'iri'
               AND t.predicate NOT IN ('rdf:type', 'foundation:icon', 'rdfs:subClassOf')
               AND t.retracted = 0"
        );

        let params: Vec<&dyn rusqlite::ToSql> = entity_iris.iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();

        let links = conn.prepare_cached(&sql)
            .and_then(|mut stmt| {
                stmt.query_map(params.as_slice(), |row| {
                    Ok(crate::commands::entity::GraphLink {
                        source: row.get(0)?,
                        target: row.get(1)?,
                        label: row.get::<_, String>(2).unwrap_or_default(),
                    })
                }).and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
            })
            .map_err(|e| e.to_string())?;

        let out = MultiGraphData { nodes, links };
        serde_json::to_string(&out).map_err(|e| e.to_string())
    }).await?;

    Ok(result)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__resolve_blackboard(
    conversation_id: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.write(move |conn| {
        let iri = resolve_blackboard_iri(conn, conversation_id.as_deref())?;
        Ok(iri)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__get_widgets(
    blackboard_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<Widget>, String> {
    executor.read(move |conn| {
        owl_get_widgets_for_blackboard(conn, &blackboard_id)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__add_widget(
    app: AppHandle,
    widget_type: String,
    entity_id: String,
    position: Option<Position>,
    size: Option<Size>,
    blackboard_id: Option<String>,
    conversation_id: Option<String>,
    executor: State<'_, DbExecutor>
) -> Result<Widget, String> {
    let widget_type_check = widget_type.clone();
    let entity_id_check = entity_id.clone();
    let (resolved_type, default_size) = executor.read(move |conn| {
        let resolved = resolve_widget_type(conn, &entity_id_check, &widget_type_check);
        let valid_types = blackboard__list_widget_types(conn);
        let size = valid_types.into_iter()
            .find(|t| t.id == resolved)
            .map(|t| t.default_size)
            .ok_or_else(|| format!("Invalid widget type: {}", resolved.clone()))?;
        Ok((resolved, size))
    }).await?;

    let bb_iri = match blackboard_id {
        Some(b) => b,
        None => {
            let conv = conversation_id.clone();
            executor.write(move |conn| {
                resolve_blackboard_iri(conn, conv.as_deref())
            }).await?
        }
    };

    let sanitized_entity = sanitize_for_iri(&entity_id);
    let sanitized_bb = sanitize_for_iri(&bb_iri);

    let widget = Widget {
        id: format!("foundation:Widget_{resolved_type}_{sanitized_entity}_{sanitized_bb}"),
        widget_type: resolved_type,
        entity_id,
        position: position.unwrap_or(Position { x: DEFAULT_POS_X, y: DEFAULT_POS_Y }),
        size: size.unwrap_or(default_size),
        window_state: WindowState::Normal,
        blackboard_iri: bb_iri,
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
        Ok("updated".to_string())
    }).await?;
    app.emit("widget-content-updated", widget_id).ok();
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__list_widget_definitions(
    class_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<WidgetDefinitionInfo>, String> {
    executor.read(move |conn| {
        let all_types = blackboard__list_widget_types(conn);

        let results = all_types.into_iter()
            .filter(|t| {
                if let Some(ref filter_iri) = class_iri {
                    t.supported_class == "owl:Thing"
                        || &t.supported_class == filter_iri
                        || crate::owl::is_subclass_of(conn, filter_iri, &t.supported_class)
                } else {
                    true
                }
            })
            .map(|t| WidgetDefinitionInfo {
                widget_type: t.id,
                description: t.description,
                icon: t.icon,
            })
            .collect();

        Ok(results)
    }).await
}

