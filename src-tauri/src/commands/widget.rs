use crate::owl::{Connection, DbExecutor, Individual, Object};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

const WIDGET_CLASS: &str = "foundation:Widget";
const WIDGET_ICON: &str = "widgets";
const BLACKBOARD_CLASS: &str = "foundation:Blackboard";
const BLACKBOARD_ICON: &str = "dashboard";
const PRED_WIDGET_TYPE: &str = "foundation:widgetType";
const PRED_ENTITY_ID: &str = "foundation:widgetEntityId";
const PRED_POSITION_X: &str = "foundation:widgetPositionX";
const PRED_POSITION_Y: &str = "foundation:widgetPositionY";
const WIDGET_ORIGIN: &str = "widget";
const PRED_WINDOW_STATE: &str = "foundation:widgetWindowState";
const PRED_BLACKBOARD: &str = "foundation:onBlackboard";
const PRED_FOR_CONVERSATION: &str = "foundation:forConversation";
const CLASS_DEFAULT_WIDGET_SPEC: &str = "foundation:defaultWidgetSpec";
const SEMANTIC_JSON_RENDERER_TYPE: &str = "semantic_json_render";

pub const DEFAULT_BLACKBOARD_IRI: &str = "foundation:DefaultBlackboard";

const DEFAULT_POS_X: f64 = 100.0;
const DEFAULT_POS_Y: f64 = 100.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WindowState {
    #[default]
    Normal,
    Minimized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    pub widget_type: String,
    pub entity_id: String,
    pub position: Position,
    pub width: f64,
    pub window_state: WindowState,
    pub blackboard_iri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_template_iri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetType {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Class IRI this widget is designed for. `"owl:Thing"` means any entity.
    pub supported_class: String,
    pub default_width: f64,
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
    pub default_width: f64,
}

fn str_obj(value: impl Into<String>) -> Object {
    Object::Literal {
        value: value.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

fn prop_str(ind: &Individual, pred: &str) -> Option<String> {
    ind.properties
        .iter()
        .find(|(p, _)| p == pred)
        .and_then(|(_, v)| v.as_literal())
}

fn prop_f64(ind: &Individual, pred: &str) -> Option<f64> {
    ind.properties
        .iter()
        .find(|(p, _)| p == pred)
        .and_then(|(_, v)| match v {
            Object::Number(n) => Some(*n),
            Object::Integer(i) => Some(*i as f64),
            _ => None,
        })
}

fn prop_iri(ind: &Individual, pred: &str) -> Option<String> {
    ind.properties
        .iter()
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
        let default =
            crate::owl::get_literal_property(conn, &class_iri, "foundation:defaultWidgetType")
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

pub fn resolve_widget_spec_template(
    conn: &Connection,
    entity_id: &str,
    widget_type: &str,
) -> Option<String> {
    if widget_type != SEMANTIC_JSON_RENDERER_TYPE {
        return None;
    }

    let class_iri = crate::owl::get_iri_property(conn, entity_id, "rdf:type")
        .ok()
        .flatten()?;
    crate::owl::get_iri_property(conn, &class_iri, CLASS_DEFAULT_WIDGET_SPEC)
        .ok()
        .flatten()
}

fn individual_to_widget(
    ind: Individual,
    type_widths: &std::collections::HashMap<String, f64>,
) -> Option<Widget> {
    let widget_type = prop_str(&ind, PRED_WIDGET_TYPE)?;
    let entity_id = prop_str(&ind, PRED_ENTITY_ID)?;
    let x = prop_f64(&ind, PRED_POSITION_X)?;
    let y = prop_f64(&ind, PRED_POSITION_Y)?;
    let width = *type_widths.get(&widget_type)?;
    let blackboard_iri = prop_iri(&ind, PRED_BLACKBOARD)?;

    let window_state = prop_str(&ind, PRED_WINDOW_STATE)
        .and_then(|s| match s.as_str() {
            "minimized" => Some(WindowState::Minimized),
            _ => Some(WindowState::Normal),
        })
        .unwrap_or_default();

    Some(Widget {
        id: ind.iri,
        widget_type,
        entity_id,
        position: Position { x, y },
        width,
        window_state,
        blackboard_iri,
        spec_template_iri: None,
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
        bb.assert(
            conn,
            BLACKBOARD_CLASS,
            &label,
            BLACKBOARD_ICON,
            WIDGET_ORIGIN,
        )
        .map_err(|e| e.to_string())?;
        if let Some(c) = conversation_iri {
            bb.add_property(
                conn,
                PRED_FOR_CONVERSATION,
                vec![Object::Iri(c.to_string())],
                WIDGET_ORIGIN,
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(bb_iri)
}

pub fn owl_insert_widget(conn: &mut Connection, widget: &Widget) -> Result<(), String> {
    let ind = Individual::new(&widget.id);
    ind.assert(
        conn,
        WIDGET_CLASS,
        &widget.widget_type,
        WIDGET_ICON,
        WIDGET_ORIGIN,
    )
    .map_err(|e| e.to_string())?;
    ind.add_property(
        conn,
        PRED_WIDGET_TYPE,
        vec![str_obj(&widget.widget_type)],
        WIDGET_ORIGIN,
    )
    .map_err(|e| e.to_string())?;
    ind.add_property(
        conn,
        PRED_ENTITY_ID,
        vec![str_obj(&widget.entity_id)],
        WIDGET_ORIGIN,
    )
    .map_err(|e| e.to_string())?;
    ind.add_property(
        conn,
        PRED_POSITION_X,
        vec![Object::Number(widget.position.x)],
        WIDGET_ORIGIN,
    )
    .map_err(|e| e.to_string())?;
    ind.add_property(
        conn,
        PRED_POSITION_Y,
        vec![Object::Number(widget.position.y)],
        WIDGET_ORIGIN,
    )
    .map_err(|e| e.to_string())?;
    let state_str = match widget.window_state {
        WindowState::Normal => "normal",
        WindowState::Minimized => "minimized",
    };
    ind.add_property(
        conn,
        PRED_WINDOW_STATE,
        vec![str_obj(state_str)],
        WIDGET_ORIGIN,
    )
    .map_err(|e| e.to_string())?;
    ind.add_property(
        conn,
        PRED_BLACKBOARD,
        vec![Object::Iri(widget.blackboard_iri.clone())],
        WIDGET_ORIGIN,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Returns widgets on the given blackboard, filtering directly in SQL and
/// applying a safe page bound.  Avoids loading all widgets from all boards.
pub fn owl_get_widgets_for_blackboard(
    conn: &Connection,
    blackboard_iri: &str,
) -> Result<Vec<Widget>, String> {
    owl_get_widgets_for_blackboard_bounded(conn, blackboard_iri, 500, 0)
}

pub fn owl_get_widgets_for_blackboard_bounded(
    conn: &Connection,
    blackboard_iri: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Widget>, String> {
    let widget_iris = crate::owl::find_entities_with_property_bounded(
        conn,
        PRED_BLACKBOARD,
        blackboard_iri,
        limit,
        offset,
        None,
    )
    .map_err(|e| e.to_string())?;

    let mut widgets = Vec::new();
    let type_widths = blackboard__list_widget_types(conn)
        .into_iter()
        .map(|widget_type| (widget_type.id, widget_type.default_width))
        .collect::<std::collections::HashMap<_, _>>();

    for iri in widget_iris {
        if let Ok(Some(ind)) = Individual::get(conn, &iri) {
            if let Some(mut widget) = individual_to_widget(ind, &type_widths) {
                widget.spec_template_iri =
                    resolve_widget_spec_template(conn, &widget.entity_id, &widget.widget_type);
                widgets.push(widget);
            }
        }
    }
    Ok(widgets)
}

pub fn owl_find_widgets_for_entity(
    conn: &Connection,
    entity_iri: &str,
) -> Result<Vec<String>, String> {
    let widget_iris = crate::owl::find_entities_with_predicate(conn, PRED_ENTITY_ID)
        .map_err(|e| e.to_string())?;

    let mut matching_widgets = Vec::new();
    for widget_iri in widget_iris {
        if let Some(widget) = Individual::get(conn, &widget_iri).map_err(|e| e.to_string())? {
            if prop_str(&widget, PRED_ENTITY_ID).as_deref() == Some(entity_iri) {
                matching_widgets.push(widget_iri);
            }
        }
    }

    Ok(matching_widgets)
}

pub fn owl_delete_widget(conn: &mut Connection, widget_id: &str) -> Result<(), String> {
    let exists = Individual::get(conn, widget_id)
        .map_err(|e| e.to_string())?
        .is_some();
    if !exists {
        return Err(format!("Widget not found: {}", widget_id));
    }
    Individual::retract(conn, widget_id, WIDGET_ORIGIN).map_err(|e| e.to_string())
}

fn owl_update_widget_position(
    conn: &mut Connection,
    widget_id: &str,
    position: &Position,
) -> Result<(), String> {
    let ind = Individual::new(widget_id);
    ind.add_property(
        conn,
        PRED_POSITION_X,
        vec![Object::Number(position.x)],
        WIDGET_ORIGIN,
    )
    .map_err(|e| e.to_string())?;
    ind.add_property(
        conn,
        PRED_POSITION_Y,
        vec![Object::Number(position.y)],
        WIDGET_ORIGIN,
    )
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
    };
    let ind = Individual::new(widget_id);
    ind.add_property(
        conn,
        PRED_WINDOW_STATE,
        vec![str_obj(state_str)],
        WIDGET_ORIGIN,
    )
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
        .add_property(
            conn,
            "foundation:diagramSource",
            vec![str_obj(content)],
            WIDGET_ORIGIN,
        )
        .map_err(|e| e.to_string())
}

/// Returns a static system prompt section describing every widget type.
/// Intended to be appended to the cached system prompt block so Claude always
/// knows what widgets exist and how to use them — without querying at runtime.
pub fn widget_system_context(conn: &Connection) -> String {
    format_ai_creatable_widget_context(&blackboard__list_widget_types(conn))
}

fn format_ai_creatable_widget_context(widget_types: &[WidgetType]) -> String {
    let ai_creatable: Vec<&WidgetType> = widget_types
        .iter()
        .filter(|widget_type| widget_type.usage_note.is_some())
        .collect();

    if ai_creatable.is_empty() {
        return String::new();
    }

    let list = ai_creatable
        .iter()
        .map(|t| {
            format!(
                "- **{}** — {}",
                t.id,
                t.usage_note.as_deref().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "## AI-creatable widgets\n\
         Create the required entity first, then pass its IRI in `speak.iris` to display it:\n\
         {list}\n\
         Before creating or changing a `semantic_json_render` specification, discover the current contracts in the ontology: search for `foundation:SemanticComponentDefinition`, then inspect each selected definition's `foundation:componentPropsSchema`, `foundation:componentActionsSchema`, and `foundation:componentExamples`. Use the example matching each intent and follow its declared slots. A spec is a `root` element ID plus an `elements` map; connect nested elements through `children` arrays (include `children: []` on leaf elements), and keep every intended element reachable from `root`. Put live entity values in the element-level `bindings` map beside `type` and `props` (for example, `bindings: {{ facts: {{ source: \"entity\", propertyIri: \"foundation:...\" }} }}`); do not put binding metadata inside `props.facts`. Preserve the component contract version recorded for each element. Do not copy the full catalogue into the conversation or invent props/actions outside those contracts."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget_type(id: &str, usage_note: Option<&str>) -> WidgetType {
        WidgetType {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            supported_class: "foundation:JsonRenderSpec".to_string(),
            default_width: 700.0,
            icon: String::new(),
            usage_note: usage_note.map(str::to_string),
        }
    }

    #[test]
    fn ai_widget_context_exposes_semantic_catalog_without_legacy_catalog() {
        let widget_types = vec![
            widget_type("json_render", None),
            widget_type("semantic_json_render", Some("Intent-based components")),
        ];

        let context = format_ai_creatable_widget_context(&widget_types);

        assert!(context.contains("semantic_json_render"));
        assert!(!context.contains("json_render —"));
    }

    #[test]
    fn ai_widget_context_points_to_ontology_contracts_and_spec_structure() {
        let widget_types = vec![widget_type(
            "semantic_json_render",
            Some("Create an ontology-backed semantic widget."),
        )];

        let context = format_ai_creatable_widget_context(&widget_types);

        assert!(context.contains("foundation:SemanticComponentDefinition"));
        assert!(context.contains("foundation:componentPropsSchema"));
        assert!(context.contains("foundation:componentActionsSchema"));
        assert!(context.contains("foundation:componentExamples"));
        assert!(context.contains("children: []"));
        assert!(context.contains("bindings: { facts:"));
        assert!(context.contains("do not put binding metadata inside `props.facts`"));
        assert!(context.contains("Do not copy the full catalogue into the conversation"));
    }

    #[test]
    fn semantic_spec_error_response_contains_only_the_stable_code() {
        let response = semantic_spec_error_response("invalid_spec_structure");
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(parsed["code"], "invalid_spec_structure");
        assert_eq!(parsed.as_object().unwrap().len(), 1);
        assert!(!response.contains("foundation:JsonRenderSpec"));
        assert!(!response.contains("invalid component references"));
    }
}

#[allow(non_snake_case)]
pub fn blackboard__list_widget_types(conn: &Connection) -> Vec<WidgetType> {
    let iris = match Individual::find_by_class_with_date_range(
        conn,
        "foundation:WidgetType",
        None,
        None,
        false,
    ) {
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
            let icon = ind.icon.clone().unwrap_or_default();
            let usage_note = prop_str(&ind, "foundation:widgetUsageNote");
            types.push(WidgetType {
                id,
                name,
                description,
                supported_class,
                default_width: width,
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
    let result = executor
        .read(move |conn| {
            if entity_iris.is_empty() {
                return serde_json::to_string(&MultiGraphData {
                    nodes: vec![],
                    links: vec![],
                })
                .map_err(|e| e.to_string());
            }

            let groups = crate::owl::load_graph_node_groups(conn);
            let (group_class, group_individual, _) = groups;

            // Build nodes from the provided IRIs only
            let things = crate::owl::Thing::get_batch(conn, &entity_iris);
            let mut nodes: Vec<crate::commands::entity::GraphNode> = entity_iris
                .iter()
                .map(|iri| {
                    let t = things.get(iri);
                    let is_class = crate::owl::Class::get(conn, iri)
                        .map(|c| c.is_some())
                        .unwrap_or(false);
                    crate::commands::entity::GraphNode {
                        id: iri.clone(),
                        label: t.map(|t| t.label.clone()).unwrap_or_else(|| iri.clone()),
                        icon: t.and_then(|t| t.icon.clone()),
                        group: if is_class {
                            group_class
                        } else {
                            group_individual
                        },
                        is_broken_ref: None,
                        is_literal: None,
                    }
                })
                .collect();

            // Deduplicate nodes (entity_iris may contain duplicates)
            nodes.dedup_by(|a, b| a.id == b.id);

            // Query relations where BOTH source and target are in the provided set
            let placeholders: String = entity_iris
                .iter()
                .enumerate()
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

            let params: Vec<&dyn rusqlite::ToSql> = entity_iris
                .iter()
                .map(|s| s as &dyn rusqlite::ToSql)
                .collect();

            let links = conn
                .prepare_cached(&sql)
                .and_then(|mut stmt| {
                    stmt.query_map(params.as_slice(), |row| {
                        Ok(crate::commands::entity::GraphLink {
                            source: row.get(0)?,
                            target: row.get(1)?,
                            label: row.get::<_, String>(2).unwrap_or_default(),
                        })
                    })
                    .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
                })
                .map_err(|e| e.to_string())?;

            let out = MultiGraphData { nodes, links };
            serde_json::to_string(&out).map_err(|e| e.to_string())
        })
        .await?;

    Ok(result)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__resolve_blackboard(
    conversation_id: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor
        .write(move |conn| {
            let iri = resolve_blackboard_iri(conn, conversation_id.as_deref())?;
            Ok(iri)
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__get_widgets(
    blackboard_id: String,
    limit: Option<i64>,
    offset: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<Widget>, String> {
    executor
        .read(move |conn| {
            // bounded-por-natureza; keyset-tx não agrega (não é append-no-topo)
            let page_size = limit.unwrap_or(500).max(1).min(2000);
            let page_offset = offset.unwrap_or(0).max(0);
            owl_get_widgets_for_blackboard_bounded(conn, &blackboard_id, page_size, page_offset)
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__get_semantic_spec(
    spec_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor
        .read(move |conn| {
            match crate::core_ontology::semantic_catalog::inspect_semantic_spec_compatibility(
                conn, &spec_iri,
            ) {
                Ok(compatibility) => {
                    serde_json::to_string(&compatibility).map_err(|error| error.to_string())
                }
                Err(error) => {
                    if let Some(code) = error.user_error_code() {
                        crate::commands::log_backend(
                            "warn",
                            &format!(
                                "[SEMANTIC_WIDGET] Invalid spec structure spec={spec_iri}: {}",
                                error.diagnostic()
                            ),
                        );
                        Err(semantic_spec_error_response(code))
                    } else {
                        Err(error.to_string())
                    }
                }
            }
        })
        .await
}

fn semantic_spec_error_response(code: &str) -> String {
    serde_json::json!({ "code": code }).to_string()
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__add_widget(
    app: AppHandle,
    widget_type: String,
    entity_id: String,
    position: Option<Position>,
    blackboard_id: Option<String>,
    conversation_id: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<Widget, String> {
    let widget_type_check = widget_type.clone();
    let entity_id_check = entity_id.clone();
    let (resolved_type, default_width, spec_template_iri) = executor
        .read(move |conn| {
            let resolved = resolve_widget_type(conn, &entity_id_check, &widget_type_check);
            let valid_types = blackboard__list_widget_types(conn);
            let width = valid_types
                .into_iter()
                .find(|t| t.id == resolved)
                .map(|t| t.default_width)
                .ok_or_else(|| format!("Invalid widget type: {}", resolved.clone()))?;
            let template = resolve_widget_spec_template(conn, &entity_id_check, &resolved);
            Ok((resolved, width, template))
        })
        .await?;

    let bb_iri = match blackboard_id {
        Some(b) => b,
        None => {
            let conv = conversation_id.clone();
            executor
                .write(move |conn| resolve_blackboard_iri(conn, conv.as_deref()))
                .await?
        }
    };

    let sanitized_entity = sanitize_for_iri(&entity_id);
    let sanitized_bb = sanitize_for_iri(&bb_iri);

    let widget = Widget {
        id: format!("foundation:Widget_{resolved_type}_{sanitized_entity}_{sanitized_bb}"),
        widget_type: resolved_type,
        entity_id,
        position: position.unwrap_or(Position {
            x: DEFAULT_POS_X,
            y: DEFAULT_POS_Y,
        }),
        width: default_width,
        window_state: WindowState::Normal,
        blackboard_iri: bb_iri,
        spec_template_iri,
    };

    app.emit("widget-added", widget.clone()).ok();

    let widget_clone = widget.clone();
    let executor_clone = executor.inner().clone();
    tokio::spawn(async move {
        executor_clone
            .write(move |conn| {
                owl_insert_widget(conn, &widget_clone)?;
                Ok(widget_clone.id.clone())
            })
            .await
            .ok();
    });

    Ok(widget)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__remove_widget(
    app: AppHandle,
    widget_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let widget_id_for_delete = widget_id.clone();
    executor
        .write(move |conn| {
            owl_delete_widget(conn, &widget_id_for_delete)?;
            Ok("deleted".to_string())
        })
        .await?;

    app.emit("widget-removed", widget_id).ok();

    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__update_widget_position(
    widget_id: String,
    position: Position,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(move |conn| {
            owl_update_widget_position(conn, &widget_id, &position)?;
            Ok("updated".to_string())
        })
        .await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__update_widget_window_state(
    widget_id: String,
    window_state: WindowState,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(move |conn| {
            owl_update_widget_window_state(conn, &widget_id, &window_state)?;
            Ok("updated".to_string())
        })
        .await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__update_widget_content(
    app: AppHandle,
    widget_id: String,
    content: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let widget_id_clone = widget_id.clone();
    executor
        .write(move |conn| {
            owl_update_widget_content(conn, &widget_id_clone, &content)?;
            Ok("updated".to_string())
        })
        .await?;
    app.emit("widget-content-updated", widget_id).ok();
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_blackboard__list_widget_definitions(
    class_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<WidgetDefinitionInfo>, String> {
    executor
        .read(move |conn| {
            let all_types = blackboard__list_widget_types(conn);

            let results = all_types
                .into_iter()
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
                    default_width: t.default_width,
                })
                .collect();

            Ok(results)
        })
        .await
}
