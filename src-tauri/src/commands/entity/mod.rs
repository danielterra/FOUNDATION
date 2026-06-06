use serde::Serialize;
use tauri::{State, Manager};
use std::collections::HashMap;
use crate::owl::{self, Class, Individual, Property, Connection, DbExecutor, Object};

mod individual;

fn enqueue_formula_jobs(app: &tauri::AppHandle, job_ids_json: &str) {
    let job_ids: Vec<String> = serde_json::from_str(job_ids_json).unwrap_or_default();
    if job_ids.is_empty() {
        return;
    }
    let worker = app.state::<crate::owl::formula_worker::FormulaWorker>();
    for job_id in job_ids {
        let _ = worker.sender.try_send(
            crate::owl::formula_worker::WorkerCommand::Enqueue { job_id }
        );
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub group: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_broken_ref: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_literal: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphLink {
    pub source: String,
    pub target: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityData {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub comment: Option<String>,

    pub types: Vec<crate::owl::Thing>,
    pub super_classes: Vec<crate::owl::Thing>,
    pub sub_classes: Vec<crate::owl::Thing>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disjoint_groups: Vec<DisjointGroup>,
    pub instances: Vec<crate::owl::Thing>,

    pub is_class: bool,
    pub is_locked: bool,
    pub allowed_statuses: Vec<StatusInfo>,

    pub properties: Vec<PropertyValue>,
    pub backlinks: Vec<PropertyValue>,
    pub status: Option<StatusInfo>,
    pub required_fields: Vec<String>,

    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_processes: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub applicable_automations: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisjointGroup {
    pub kind: String,
    pub members: Vec<crate::owl::Thing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub iri: String,
    pub label: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyValue {
    pub property: String,
    pub property_label: String,
    pub property_comment: Option<String>,
    pub value: String,
    pub value_label: Option<String>,
    pub value_icon: Option<String>,
    pub is_object_property: bool,
    pub source_class: Option<String>,
    pub source_class_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_class_icon: Option<String>,
    pub unit: Option<String>,
    pub unit_label: Option<String>,
    pub datatype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_status: Option<StatusInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_total: Option<usize>,
    pub is_calculated: bool,
    pub is_query_property: bool,
    pub formula_error: Option<String>,
    pub is_empty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_class_iri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_class_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_class_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_info: Option<FileInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_behavior_rules: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_config: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    pub file_size: Option<i64>,
    pub file_type_iri: Option<String>,
}

/// Search for instances by label and property values
#[tauri::command]
#[allow(non_snake_case)]
pub async fn owl__search_entities(
    query: String,
    limit: Option<usize>,
    type_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let limit = limit.unwrap_or(100);
        let results = if type_iri.as_deref() == Some("owl:Class") {
            crate::core_ontology::search::search_classes(conn, &query, limit)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| crate::core_ontology::search::SearchResult {
                    id: r.id,
                    label: r.label,
                    icon: r.icon,
                    entity_type: "class".to_string(),
                    matched_properties: vec![],
                    class_type: None,
                    status: None,
                })
                .collect()
        } else if let Some(ref class_iri) = type_iri {
            let tokens: Vec<String> = query
                .split_whitespace()
                .map(|s| s.to_lowercase())
                .collect();
            let (results, _) = crate::core_ontology::search::search(
                conn, &tokens, None, Some(class_iri.as_str()), None, false, limit, 0,
            ).map_err(|e| e.to_string())?;
            results
        } else {
            crate::core_ontology::search::search_instances(conn, &query, limit)
                .map_err(|e| e.to_string())?
        };
        serde_json::to_string(&results).map_err(|e| e.to_string())
    }).await
}


#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkPageItem {
    pub value: String,
    pub value_label: Option<String>,
    pub value_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_status: Option<StatusInfo>,
}

/// Returns a page of backlink subjects for a given entity + predicate + source_class group.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn inspector__get_backlink_page(
    entity_iri: String,
    predicate: String,
    source_class: Option<String>,
    offset: usize,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    const PAGE_SIZE: usize = 15;
    executor.read(move |conn| {
        let subjects = crate::eavto::query::get_backlinks_page(
            conn, &entity_iri, &predicate, source_class.as_deref(), offset, PAGE_SIZE,
        ).map_err(|e| e.to_string())?;

        let things = crate::owl::Thing::get_batch(conn, &subjects);

        let status_iris = crate::eavto::query::get_first_iri_property_batch(
            conn, &subjects, "foundation:hasStatus",
        ).unwrap_or_default();

        let unique_status_iris: Vec<String> = status_iris.values()
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let mut status_cache: std::collections::HashMap<String, StatusInfo> = std::collections::HashMap::new();
        for status_iri in unique_status_iris {
            if crate::owl::is_instance_of(conn, &status_iri, "foundation:Status") {
                let thing = crate::owl::Thing::get(conn, &status_iri);
                let (icon, color) = crate::core_ontology::status::resolve_status_appearance(conn, &status_iri);
                status_cache.insert(status_iri.clone(), StatusInfo {
                    iri: status_iri,
                    label: thing.label,
                    icon,
                    color,
                });
            }
        }

        let items: Vec<BacklinkPageItem> = subjects.iter().map(|iri| {
            let thing = things.get(iri).cloned().unwrap_or_else(|| crate::owl::Thing::get(conn, iri));
            let value_status = status_iris.get(iri)
                .and_then(|siri| status_cache.get(siri))
                .cloned();
            BacklinkPageItem {
                value: iri.clone(),
                value_label: Some(thing.label),
                value_icon: thing.icon,
                value_status,
            }
        }).collect();

        serde_json::to_string(&items).map_err(|e| e.to_string())
    }).await
}

/// Get entity data with its complete neighborhood for visualization
#[tauri::command]
#[allow(non_snake_case)]
pub async fn inspector__get_entity(
    entity_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let t0 = std::time::Instant::now();
    crate::commands::logging::log_backend("debug", &format!("[CMD] inspector__get_entity({entity_id})"));
    let entity_id_log = entity_id.clone();
    let track_id = entity_id.clone();
    let executor_clone = executor.inner().clone();
    tokio::spawn(async move {
        let _ = executor_clone.write(move |conn| {
            crate::search::track_access(conn, &track_id);
            Ok(String::new())
        }).await;
    });

    let result = executor.read(move |conn| {
        let t_read = std::time::Instant::now();

        let groups = owl::load_graph_node_groups(conn);
        let t2 = std::time::Instant::now();

        let data = if let Some(class) = Class::get(conn, &entity_id).map_err(|e| e.to_string())? {
            get_class_data(conn, &entity_id, groups, class)?
        } else if let Some(individual) = Individual::get(conn, &entity_id).map_err(|e| e.to_string())? {
            individual::get_individual_data(conn, &entity_id, groups, individual)?
        } else {
            return Err(format!("Entity {} not found or unknown type", entity_id));
        };
        let t3 = std::time::Instant::now();

        let json = serde_json::to_string(&data).map_err(|e| e.to_string());
        let t4 = std::time::Instant::now();

        crate::commands::logging::log_backend("DEBUG", &format!(
            "[INSPECTOR] get_entity({entity_id}) \
            read_wait={}ms groups={}ms data={}ms serialize={}ms total={}ms",
            t_read.duration_since(t0).as_millis(),
            t2.duration_since(t_read).as_millis(),
            t3.duration_since(t2).as_millis(),
            t4.duration_since(t3).as_millis(),
            t4.duration_since(t0).as_millis(),
        ));

        json
    }).await;
    crate::commands::logging::log_backend("DEBUG", &format!("[INSPECTOR] get_entity({}) total_async={}ms", entity_id_log, t0.elapsed().as_millis()));
    result
}

/// Returns the GraphNodeType configuration stored in the ontology.
/// The frontend uses this to map group discriminators to visual styles.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn graph__get_node_type_config(
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let configs = owl::get_graph_node_type_config(conn);
        serde_json::to_string(&configs).map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__clear_property(
    entity_id: String,
    property_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    let property_iri_clone = property_iri.clone();
    let job_ids_json = executor.write(move |conn| {
        Individual::clear_property(conn, &entity_id_clone, &property_iri_clone, "user")
            .map_err(|e| e.to_string())?;
        let job_ids = crate::owl::formula_worker::create_instance_recalc_jobs(
            conn, &entity_id_clone, &property_iri_clone,
        );
        serde_json::to_string(&job_ids).map_err(|e| e.to_string())
    }).await?;
    enqueue_formula_jobs(&app, &job_ids_json);
    crate::realtime::emit_entity_updated(&app, &entity_id);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__update_property(
    entity_id: String,
    property_iri: String,
    value: String,
    datatype: Option<String>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    let property_iri_clone = property_iri.clone();
    let job_ids_json = executor.write(move |conn| {
        if !value.is_empty() {
            let effective_datatype = datatype.unwrap_or_else(|| {
                Property::get(conn, &property_iri_clone)
                    .ok()
                    .flatten()
                    .and_then(|p| p.ranges.into_iter().find(|r| r.starts_with("xsd:")))
                    .unwrap_or_else(|| "xsd:string".to_string())
            });
            let individual = Individual::new(&entity_id_clone);
            individual.add_property(conn, &property_iri_clone, vec![Object::Literal {
                value,
                datatype: Some(effective_datatype),
                language: None,
            }], "user").map_err(|e| e.to_string())?;
        }
        let job_ids = if property_iri_clone == "foundation:formula"
            || property_iri_clone == "foundation:aggregation"
        {
            crate::owl::formula_worker::cancel_and_create_property_job(conn, &entity_id_clone)
                .map(|id| vec![id])
                .unwrap_or_default()
        } else {
            crate::owl::formula_worker::create_instance_recalc_jobs(
                conn, &entity_id_clone, &property_iri_clone,
            )
        };
        serde_json::to_string(&job_ids).map_err(|e| e.to_string())
    }).await?;
    enqueue_formula_jobs(&app, &job_ids_json);
    crate::realtime::emit_entity_updated(&app, &entity_id);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__set_references(
    entity_id: String,
    property_iri: String,
    iris: Vec<String>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    let property_iri_clone = property_iri.clone();
    let job_ids_json = executor.write(move |conn| {
        Individual::clear_property(conn, &entity_id_clone, &property_iri_clone, "user")
            .map_err(|e| e.to_string())?;
        if !iris.is_empty() {
            let individual = Individual::new(&entity_id_clone);
            let values: Vec<owl::Object> = iris.into_iter().map(owl::Object::Iri).collect();
            individual.add_property(conn, &property_iri_clone, values, "user")
                .map_err(|e| e.to_string())?;
        }
        let job_ids = crate::owl::formula_worker::create_instance_recalc_jobs(
            conn, &entity_id_clone, &property_iri_clone,
        );
        serde_json::to_string(&job_ids).map_err(|e| e.to_string())
    }).await?;
    enqueue_formula_jobs(&app, &job_ids_json);
    crate::realtime::emit_entity_updated(&app, &entity_id);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__add_property_value(
    entity_id: String,
    property_iri: String,
    value_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    executor.write(move |conn| {
        Individual::add_iri_value(conn, &entity_id_clone, &property_iri, &value_iri, "user")
            .map_err(|e| e.to_string())?;
        Ok("added".to_string())
    }).await?;
    crate::realtime::emit_entity_updated(&app, &entity_id);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__remove_property_value(
    entity_id: String,
    property_iri: String,
    value_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    executor.write(move |conn| {
        Individual::remove_iri_value(conn, &entity_id_clone, &property_iri, &value_iri, "user")
            .map_err(|e| e.to_string())?;
        Ok("removed".to_string())
    }).await?;
    crate::realtime::emit_entity_updated(&app, &entity_id);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__update_status(
    entity_id: String,
    status_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    executor.write(move |conn| {
        let individual = Individual::new(&entity_id_clone);
        individual.add_property(conn, "foundation:hasStatus", vec![Object::Iri(status_iri)], "user")
            .map_err(|e| e.to_string())?;
        Ok("updated".to_string())
    }).await?;
    crate::realtime::emit_entity_updated(&app, &entity_id);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__get_delete_impact(
    entity_id: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let (cascade_items, backlink_count) =
            Individual::compute_delete_impact(conn, &entity_id)
                .map_err(|e| e.to_string())?;
        let items_json: Vec<serde_json::Value> = cascade_items.into_iter()
            .map(|(iri, label, type_label)| serde_json::json!({
                "iri": iri,
                "label": label,
                "type_label": type_label,
            }))
            .collect();
        serde_json::to_string(&serde_json::json!({
            "cascade_items": items_json,
            "backlink_count": backlink_count,
        })).map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__delete_individual(
    entity_id: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    let retracted_json = executor.write(move |conn| {
        let iris = Individual::retract_collecting(conn, &entity_id_clone, "user")
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&iris).map_err(|e| e.to_string())
    }).await?;
    let all_retracted: Vec<String> = serde_json::from_str(&retracted_json).unwrap_or_default();
    crate::search::remove_from_index(&all_retracted);
    for iri in &all_retracted {
        crate::realtime::emit_entity_deleted(&app, iri);
    }
    crate::realtime::emit_entity_updated(&app, &entity_id);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__define_class_property(
    class_iri: String,
    property_iri: Option<String>,
    label: String,
    property_type: String,
    range: String,
    unit: Option<String>,
    comment: Option<String>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let class_iri_clone = class_iri.clone();
    let final_iri = property_iri.unwrap_or_else(|| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("foundation:Property_{}", ts)
    });
    let final_iri_clone = final_iri.clone();

    executor.write(move |conn| {
        let prop_type = match property_type.as_str() {
            "object" => crate::owl::PropertyType::ObjectProperty,
            _ => crate::owl::PropertyType::DatatypeProperty,
        };

        let existing = Property::get(conn, &final_iri_clone).map_err(|e| e.to_string())?;
        let domains: Vec<String> = if let Some(ref existing_prop) = existing {
            let mut domains = existing_prop.domains.clone();
            if !domains.contains(&class_iri_clone) {
                domains.push(class_iri_clone.clone());
            }
            domains
        } else {
            vec![class_iri_clone.clone()]
        };
        let domain_refs: Vec<&str> = domains.iter().map(|s| s.as_str()).collect();

        Property::new(&final_iri_clone)
            .assert(
                conn,
                prop_type,
                &label,
                comment.as_deref(),
                &domain_refs,
                Some(range.as_str()),
                unit.as_deref(),
                "user",
            )
            .map_err(|e| e.to_string())?;

        Ok(final_iri_clone.clone())
    }).await?;

    crate::realtime::emit_entity_updated(&app, &final_iri);
    crate::realtime::emit_entity_updated(&app, &class_iri);
    Ok(final_iri)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__check_property_usage(
    property_iri: String,
    class_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let all_class_iris = owl::Class::get_descendant_iris(conn, &class_iri)
            .unwrap_or_else(|_| vec![class_iri.clone()]);

        let in_placeholders: Vec<String> = (0..all_class_iris.len())
            .map(|i| format!("?{}", i + 2))
            .collect();
        let in_clause = in_placeholders.join(", ");

        let mut params: Vec<String> = vec![property_iri.clone()];
        params.extend(all_class_iris.iter().cloned());

        let count_sql = format!(
            "SELECT COUNT(DISTINCT t_val.subject)
             FROM triples t_val
             JOIN triples t_type ON t_type.subject = t_val.subject
                AND t_type.predicate = 'rdf:type'
                AND t_type.object IN ({in_clause})
                AND t_type.retracted = 0
             WHERE t_val.predicate = ?1
               AND t_val.retracted = 0",
        );
        let count: usize = conn
            .prepare(&count_sql).map_err(|e| e.to_string())?
            .query_row(rusqlite::params_from_iter(params.iter()), |row| row.get::<_, i64>(0))
            .map_err(|e| e.to_string())
            .map(|n| n as usize)?;

        let examples_sql = format!(
            "SELECT DISTINCT t_val.subject, t_label.object_value
             FROM triples t_val
             JOIN triples t_type ON t_type.subject = t_val.subject
                AND t_type.predicate = 'rdf:type'
                AND t_type.object IN ({in_clause})
                AND t_type.retracted = 0
             LEFT JOIN triples t_label ON t_label.subject = t_val.subject
                AND t_label.predicate = 'rdfs:label'
                AND t_label.retracted = 0
             WHERE t_val.predicate = ?1
               AND t_val.retracted = 0
             LIMIT 5",
        );
        let mut stmt = conn.prepare(&examples_sql).map_err(|e| e.to_string())?;
        let examples: Vec<String> = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .map(|(iri, label)| label.unwrap_or(iri))
            .collect();

        serde_json::to_string(&serde_json::json!({ "count": count, "examples": examples }))
            .map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__remove_class_property(
    property_iri: String,
    class_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let class_iri_clone = class_iri.clone();
    let property_iri_clone = property_iri.clone();

    executor.write(move |conn| {
        let prop = Property::get(conn, &property_iri_clone)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Property {} not found", property_iri_clone))?;

        let new_domains: Vec<String> = prop.domains.into_iter()
            .filter(|d| d != &class_iri_clone)
            .collect();
        let domain_refs: Vec<&str> = new_domains.iter().map(|s| s.as_str()).collect();

        let label = prop.label.as_deref().unwrap_or("");
        let prop_type = prop.property_type;
        let range = prop.ranges.first().map(|s| s.as_str());
        let unit = prop.unit.as_deref();
        let comment = prop.comment.as_deref();

        Property::new(&property_iri_clone)
            .assert(conn, prop_type, label, comment, &domain_refs, range, unit, "user")
            .map_err(|e| e.to_string())?;

        Ok("removed".to_string())
    }).await?;

    crate::realtime::emit_entity_updated(&app, &class_iri);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__set_system_locked(
    entity_id: String,
    locked: bool,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let entity_id_clone = entity_id.clone();
    executor.write(move |conn| {
        owl::set_system_locked(conn, &entity_id_clone, locked)
            .map_err(|e| e.to_string())?;
        Ok(String::new())
    }).await?;
    crate::realtime::emit_entity_updated(&app, &entity_id);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__set_property_cardinality(
    class_id: String,
    property_iri: String,
    min_count: Option<u32>,
    max_count: Option<u32>,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let class_id_clone = class_id.clone();
    executor.write(move |conn| {
        let mut restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, &class_id_clone)
            .map_err(|e| e.to_string())?;

        restrictions.retain(|r| r.property_iri != property_iri);

        let has_constraint = min_count.map(|m| m > 0).unwrap_or(false) || max_count.is_some();
        if has_constraint {
            restrictions.push(crate::owl::cardinality::CardinalityRestriction {
                property_iri: property_iri.clone(),
                min: min_count,
                max: max_count,
                exact: None,
            });
        }

        let prop_refs: Vec<crate::owl::cardinality::PropertyRestriction<'_>> = restrictions.iter()
            .map(|r| crate::owl::cardinality::PropertyRestriction {
                property_iri: &r.property_iri,
                min: r.min,
                max: r.max,
            })
            .collect();

        crate::owl::cardinality::set_class_cardinality_restrictions(conn, &class_id_clone, &prop_refs, "user")
            .map_err(|e| e.to_string())?;

        Ok(String::new())
    }).await?;
    crate::realtime::emit_entity_updated(&app, &class_id);
    Ok(())
}

#[tauri::command]
pub async fn widget_inspector__set_property_query_config(
    class_id: String,
    property_iri: String,
    query_config_json: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let prop_iri = property_iri.clone();
    let json = query_config_json.clone();

    let domain_iris: Vec<String> = executor.write(move |conn| {
        let config = crate::owl::query_property::parse_query_config(&json)
            .map_err(|e| e.to_string())?;
        crate::owl::query_property::validate_query_config(conn, &config)
            .map_err(|e| e.to_string())?;

        use crate::eavto::{store, Triple, Object};
        store::assert_triples(conn, &[Triple::new(&prop_iri, "foundation:queryConfig", Object::Literal {
            value: json.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })], "user").map_err(|e| e.to_string())?;

        let domains: Vec<String> = conn
            .prepare("SELECT DISTINCT object FROM triples_current WHERE subject = ? AND predicate = 'rdfs:domain'")
            .ok()
            .and_then(|mut stmt| {
                stmt.query_map(rusqlite::params![prop_iri], |row| row.get::<_, String>(0))
                    .ok()
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();

        Ok(domains.join(","))
    }).await.map(|s| s.split(',').filter(|s| !s.is_empty()).map(String::from).collect())?;

    for domain_iri in &domain_iris {
        let dom = domain_iri.clone();
        let instances: Vec<String> = executor.read(move |conn| {
            let rows: Vec<String> = conn
                .prepare("SELECT DISTINCT subject FROM triples_current WHERE predicate = 'rdf:type' AND object = ?")
                .ok()
                .and_then(|mut stmt| {
                    stmt.query_map(rusqlite::params![dom], |row| row.get::<_, String>(0))
                        .ok()
                        .map(|rows| rows.filter_map(|r| r.ok()).collect())
                })
                .unwrap_or_default();
            Ok(rows.join(","))
        }).await.map(|s| s.split(',').filter(|s| !s.is_empty()).map(String::from).collect())?;

        for iri in instances {
            crate::realtime::emit_entity_updated(&app, &iri);
        }
    }

    crate::realtime::emit_entity_updated(&app, &class_id);
    Ok(())
}

pub(super) fn sort_backlinks_by_recency(conn: &Connection, backlinks: &mut Vec<PropertyValue>) {
    let entity_iris: Vec<String> = backlinks.iter().map(|b| b.value.clone()).collect();
    let max_tx_map = crate::eavto::query::get_entities_max_tx(conn, &entity_iris)
        .unwrap_or_default();
    backlinks.sort_by(|a, b| {
        let tx_a = max_tx_map.get(&a.value).copied().unwrap_or(0);
        let tx_b = max_tx_map.get(&b.value).copied().unwrap_or(0);
        tx_b.cmp(&tx_a)
    });
}

pub(super) fn resolve_unit_label(conn: &Connection, unit_iri: &str) -> Option<String> {
    owl::get_literal_property(conn, unit_iri, "qudt:currencyCode")
        .ok()
        .flatten()
        .or_else(|| {
            let label = crate::owl::Thing::get(conn, unit_iri).label;
            if label == unit_iri {
                unit_iri.rsplit_once(':')
                    .map(|(_, local)| local.to_string())
                    .or(Some(label))
            } else {
                Some(label)
            }
        })
}

pub(super) fn resolve_entity_status(conn: &Connection, properties: &[PropertyValue]) -> Option<StatusInfo> {
    for prop in properties {
        if !prop.is_object_property || prop.value.is_empty() {
            continue;
        }
        if owl::is_instance_of(conn, &prop.value, "foundation:Status") {
            let (icon, color) = crate::core_ontology::status::resolve_status_appearance(conn, &prop.value);
            return Some(StatusInfo {
                iri: prop.value.clone(),
                label: prop.value_label.clone().unwrap_or_else(|| prop.value.clone()),
                icon,
                color,
            });
        }
    }
    None
}

fn get_class_data(conn: &Connection, class_id: &str, groups: (u8, u8, u8), class: Class) -> Result<EntityData, String> {
    let (group_class, _group_individual, group_literal) = groups;

    let label = class.label.unwrap_or_else(|| class_id.to_string());
    let icon = class.icon;
    let comment = class.comment;


    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut added_node_ids = std::collections::HashSet::new();

    nodes.push(GraphNode {
        id: class_id.to_string(),
        label: label.clone(),
        icon: icon.clone(),
        group: group_class,
        is_broken_ref: None,
        is_literal: None,
    });
    added_node_ids.insert(class_id.to_string());

    for super_class in &class.super_classes {
        if !added_node_ids.contains(&super_class.iri) {
            nodes.push(GraphNode {
                id: super_class.iri.clone(),
                label: super_class.label.clone(),
                icon: super_class.icon.clone(),
                group: group_class,
                is_broken_ref: None,
                is_literal: None,
            });
            added_node_ids.insert(super_class.iri.clone());
        }

        links.push(GraphLink {
            source: class_id.to_string(),
            target: super_class.iri.clone(),
            label: "subClassOf".to_string(),
        });
    }

    for sub_class in &class.sub_classes {
        if !added_node_ids.contains(&sub_class.iri) {
            nodes.push(GraphNode {
                id: sub_class.iri.clone(),
                label: sub_class.label.clone(),
                icon: sub_class.icon.clone(),
                group: group_class,
                is_broken_ref: None,
                is_literal: None,
            });
            added_node_ids.insert(sub_class.iri.clone());
        }

        links.push(GraphLink {
            source: sub_class.iri.clone(),
            target: class_id.to_string(),
            label: "subClassOf".to_string(),
        });
    }

    for (property_iri, _source_class_iri) in &class.properties {
        let prop = Property::get(conn, property_iri)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Property {} not found", property_iri))?;

        let property_label = prop.label.clone().unwrap_or_else(|| property_iri.clone());

        if prop.property_type == crate::owl::PropertyType::ObjectProperty {
            for range_iri in &prop.ranges {
                if !added_node_ids.contains(range_iri) {
                    let range_thing = crate::owl::Thing::get(conn, range_iri);
                    nodes.push(GraphNode {
                        id: range_iri.clone(),
                        label: range_thing.label,
                        icon: range_thing.icon,
                        group: group_class,
                        is_broken_ref: None,
                        is_literal: None,
                    });
                    added_node_ids.insert(range_iri.clone());
                }

                links.push(GraphLink {
                    source: class_id.to_string(),
                    target: range_iri.clone(),
                    label: property_label.clone(),
                });
            }
        } else {
            for range_iri in &prop.ranges {
                let literal_node_id = format!("{}#datatype#{}", class_id, range_iri);

                if !added_node_ids.contains(&literal_node_id) {
                    let range_thing = crate::owl::Thing::get(conn, range_iri);

                    nodes.push(GraphNode {
                        id: literal_node_id.clone(),
                        label: range_thing.label,
                        icon: range_thing.icon,
                        group: group_literal,
                        is_broken_ref: None,
                        is_literal: Some(true),
                    });
                    added_node_ids.insert(literal_node_id.clone());
                }

                links.push(GraphLink {
                    source: class_id.to_string(),
                    target: literal_node_id,
                    label: property_label.clone(),
                });
            }
        }
    }


    let cardinality_restrictions = crate::owl::cardinality::get_class_cardinality_restrictions(conn, class_id)
        .unwrap_or_default();

    let cardinality_map: std::collections::HashMap<String, (Option<u32>, Option<u32>)> =
        cardinality_restrictions.iter()
            .map(|r| {
                let min = r.min.or(r.exact);
                let max = r.max.or(r.exact);
                (r.property_iri.clone(), (min, max))
            })
            .collect();

    let mut properties = Vec::new();

    for type_thing in &class.types {
        properties.push(PropertyValue {
            property: "rdf:type".to_string(),
            property_label: "type".to_string(),
            property_comment: Some("The type of this entity".to_string()),
            value: type_thing.iri.clone(),
            value_label: Some(type_thing.label.clone()),
            value_icon: type_thing.icon.clone(),
            is_object_property: true,
            source_class: None,
            source_class_label: None,
            source_class_icon: None,
            unit: None,
            unit_label: None,
            datatype: None,
            value_status: None,
            group_total: None,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
            min_count: None,
            max_count: None,
            ai_behavior_rules: None,
            is_query_property: false,
            query_config: None,
        });
    }

    for super_class in &class.super_classes {
        properties.push(PropertyValue {
            property: "rdfs:subClassOf".to_string(),
            property_label: "subClassOf".to_string(),
            property_comment: Some("Parent class of this class".to_string()),
            value: super_class.iri.clone(),
            value_label: Some(super_class.label.clone()),
            value_icon: super_class.icon.clone(),
            is_object_property: true,
            source_class: None,
            source_class_label: None,
            source_class_icon: None,
            unit: None,
            unit_label: None,
            datatype: None,
            value_status: None,
            group_total: None,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
            min_count: None,
            max_count: None,
            ai_behavior_rules: None,
            is_query_property: false,
            query_config: None,
        });
    }

    for (property_iri, source_class_iri) in &class.properties {
        let prop = Property::get(conn, property_iri)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Property {} not found", property_iri))?;

        let property_label = prop.label.unwrap_or_else(|| property_iri.clone());
        let property_comment = prop.comment;

        let is_object_property = prop.property_type == crate::owl::PropertyType::ObjectProperty;
        let (value, value_label, value_icon) = prop.ranges.first()
            .map(|range_iri| {
                let range_thing = crate::owl::Thing::get(conn, range_iri);
                (range_iri.clone(), range_thing.label, range_thing.icon)
            })
            .unwrap_or_else(|| ("owl:Thing".to_string(), "Any".to_string(), None));

        let (source_class, source_class_label, source_class_icon) = if source_class_iri != class_id {
            let source_thing = crate::owl::Thing::get(conn, source_class_iri);
            (Some(source_class_iri.clone()), Some(source_thing.label), source_thing.icon)
        } else {
            (None, None, None)
        };

        let (unit, unit_label) = if let Some(unit_iri) = &prop.unit {
            let unit_display = owl::get_literal_property(conn, unit_iri, "qudt:symbol")
                .ok()
                .flatten()
                .or_else(|| {
                    let unit_thing = crate::owl::Thing::get(conn, unit_iri);
                    Some(unit_thing.label)
                });

            (Some(unit_iri.clone()), unit_display)
        } else {
            (None, None)
        };

        let (min_count, max_count) = cardinality_map.get(property_iri.as_str()).copied().unwrap_or((None, None));
        let ai_behavior_rules = prop.ai_behavior_rules.clone();

        properties.push(PropertyValue {
            property: property_iri.clone(),
            property_label,
            property_comment,
            value,
            value_label: Some(value_label),
            value_icon,
            is_object_property,
            source_class,
            source_class_label,
            source_class_icon,
            unit,
            unit_label,
            datatype: None,
            value_status: None,
            group_total: None,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
            min_count,
            max_count,
            ai_behavior_rules,
            is_query_property: prop.query_config.is_some(),
            query_config: prop.query_config.clone(),
        });
    }

    for (predicate, value_obj) in &class.concept_properties {
        let (property_label, property_comment) = if let Ok(Some(prop)) = Property::get(conn, predicate) {
            (prop.label.unwrap_or_else(|| predicate.clone()), prop.comment)
        } else {
            (predicate.clone(), None)
        };

        let is_object_property = value_obj.is_iri();
        let value = if is_object_property {
            value_obj.as_iri().unwrap_or("").to_string()
        } else {
            value_obj.as_literal().unwrap_or_default()
        };

        let value_label = if is_object_property {
            Some(crate::owl::Thing::get(conn, &value).label)
        } else {
            None
        };

        properties.push(PropertyValue {
            property: predicate.clone(),
            property_label,
            property_comment,
            value,
            value_label,
            value_icon: None,
            is_object_property,
            source_class: None,
            source_class_label: None,
            source_class_icon: None,
            unit: None,
            unit_label: None,
            datatype: value_obj.datatype().map(|s| s.to_string()),
            value_status: None,
            group_total: None,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
            min_count: None,
            max_count: None,
            ai_behavior_rules: None,
            is_query_property: false,
            query_config: None,
        });
    }

    let instance_iris: Vec<String> = class.backlinks.iter()
        .map(|(s, _, _)| s.clone())
        .collect();

    let source_things = crate::owl::Thing::get_batch(conn, &instance_iris);

    let source_status_iris = crate::eavto::query::get_first_iri_property_batch(
        conn, &instance_iris, "foundation:hasStatus",
    ).unwrap_or_default();

    let unique_status_iris: Vec<String> = source_status_iris.values()
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mut status_cache: HashMap<String, StatusInfo> = HashMap::new();
    for status_iri in unique_status_iris {
        if owl::is_instance_of(conn, &status_iri, "foundation:Status") {
            let status_thing = crate::owl::Thing::get(conn, &status_iri);
            let (icon, color) = crate::core_ontology::status::resolve_status_appearance(conn, &status_iri);
            status_cache.insert(status_iri.clone(), StatusInfo {
                iri: status_iri,
                label: status_thing.label,
                icon,
                color,
            });
        }
    }

    let group_total = if class.backlink_total > class.backlinks.len() {
        Some(class.backlink_total)
    } else {
        None
    };

    let mut backlinks = Vec::new();
    for (source_entity, property_iri, _) in &class.backlinks {
        let source_thing = source_things.get(source_entity)
            .cloned()
            .unwrap_or_else(|| crate::owl::Thing::get(conn, source_entity));

        let value_status = source_status_iris.get(source_entity)
            .and_then(|s| status_cache.get(s))
            .cloned();

        backlinks.push(PropertyValue {
            property: property_iri.clone(),
            property_label: "type of".to_string(),
            property_comment: None,
            value: source_entity.clone(),
            value_label: Some(source_thing.label),
            value_icon: source_thing.icon,
            is_object_property: true,
            source_class: Some(class_id.to_string()),
            source_class_label: None,
            source_class_icon: None,
            unit: None,
            unit_label: None,
            datatype: None,
            value_status,
            group_total,
            is_calculated: false,
            formula_error: None,
            is_empty: false,
            range_class_iri: None,
            range_class_label: None,
            range_class_icon: None,
            file_info: None,
            min_count: None,
            max_count: None,
            ai_behavior_rules: None,
            is_query_property: false,
            query_config: None,
        });
    }

    sort_backlinks_by_recency(conn, &mut backlinks);

    let status = resolve_entity_status(conn, &properties);

    let required_fields = cardinality_restrictions.iter()
        .filter(|r| r.is_required())
        .map(|r| r.property_iri.clone())
        .collect();

    let allowed_statuses = crate::owl::get_all_iri_properties(conn, class_id, "foundation:allowedStatus")
        .unwrap_or_default()
        .into_iter()
        .map(|status_iri| {
            let thing = crate::owl::Thing::get(conn, &status_iri);
            let (icon, color) = crate::core_ontology::status::resolve_status_appearance(conn, &status_iri);
            StatusInfo { iri: status_iri, label: thing.label, icon, color }
        })
        .collect();

    let disjoint_groups = build_disjoint_groups(conn, class_id).unwrap_or_default();
    let related_processes = crate::ai::functions::find_related_processes_for_class(conn, class_id);
    let applicable_automations = crate::ai::functions::find_automations_for_types(conn, &[class_id.to_string()]);

    Ok(EntityData {
        id: class_id.to_string(),
        label,
        icon,
        comment,
        is_class: true,
        is_locked: crate::owl::is_system_locked(conn, class_id),
        allowed_statuses,
        types: class.types.clone(),
        super_classes: class.super_classes.clone(),
        sub_classes: class.sub_classes.clone(),
        disjoint_groups,
        instances: vec![],
        properties,
        backlinks,
        status,
        required_fields,
        nodes,
        links,
        related_processes,
        applicable_automations,
    })
}

fn build_disjoint_groups(conn: &Connection, class_iri: &str) -> Result<Vec<DisjointGroup>, String> {
    let mut groups: Vec<DisjointGroup> = Vec::new();

    let pair_iris = Class::get_direct_disjoint_pair_iris(conn, class_iri)
        .map_err(|e| e.to_string())?;
    for other in pair_iris {
        let thing = crate::owl::Thing::get(conn, &other);
        groups.push(DisjointGroup {
            kind: "pair".to_string(),
            members: vec![thing],
            group_id: None,
        });
    }

    let adcs = Class::find_all_disjoint_class_sets(conn, class_iri)
        .map_err(|e| e.to_string())?;
    for adc_iri in adcs {
        let members = Class::get_all_disjoint_classes_members(conn, &adc_iri)
            .map_err(|e| e.to_string())?;
        let things: Vec<crate::owl::Thing> = members.iter()
            .filter(|m| m.as_str() != class_iri)
            .map(|m| crate::owl::Thing::get(conn, m))
            .collect();
        if things.is_empty() {
            continue;
        }
        groups.push(DisjointGroup {
            kind: "all".to_string(),
            members: things,
            group_id: Some(adc_iri),
        });
    }

    Ok(groups)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__add_class_disjoint(
    class_iri: String,
    disjoint_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let class_iri_clone = class_iri.clone();
    let disjoint_iri_clone = disjoint_iri.clone();
    executor.write(move |conn| {
        Class::add_disjoint_with(conn, &class_iri_clone, &disjoint_iri_clone, "user")
            .map_err(|e| e.to_string())?;
        Ok(String::new())
    }).await?;
    crate::realtime::emit_entity_updated(&app, &class_iri);
    crate::realtime::emit_entity_updated(&app, &disjoint_iri);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__remove_disjoint_pair(
    class_iri: String,
    disjoint_iri: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let class_iri_clone = class_iri.clone();
    let disjoint_iri_clone = disjoint_iri.clone();
    executor.write(move |conn| {
        Class::remove_disjoint_with(conn, &class_iri_clone, &disjoint_iri_clone, "user")
            .map_err(|e| e.to_string())?;
        Ok(String::new())
    }).await?;
    crate::realtime::emit_entity_updated(&app, &class_iri);
    crate::realtime::emit_entity_updated(&app, &disjoint_iri);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn widget_inspector__retract_disjoint_set(
    group_id: String,
    app: tauri::AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let group_id_clone = group_id.clone();
    let members_json = executor.write(move |conn| {
        let members = Class::get_all_disjoint_classes_members(conn, &group_id_clone)
            .map_err(|e| e.to_string())?;
        Class::retract_all_disjoint_classes(conn, &group_id_clone, "user")
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&members).map_err(|e| e.to_string())
    }).await?;
    let members: Vec<String> = serde_json::from_str(&members_json).unwrap_or_default();
    for iri in members {
        crate::realtime::emit_entity_updated(&app, &iri);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IriResolution {
    pub label: String,
    pub icon: Option<String>,
}

pub fn resolve_iris_batch(conn: &owl::Connection, iris: Vec<String>) -> HashMap<String, IriResolution> {
    use crate::eavto::query;
    use crate::owl::vocabulary::rdfs;

    if iris.is_empty() {
        return HashMap::new();
    }

    let predicates = &[rdfs::LABEL, "foundation:hasIcon", "rdf:type"];
    let rows = match query::get_predicates_for_subjects(conn, &iris, predicates) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };

    struct Raw { label: Option<String>, icon: Option<String> }
    let mut raw: HashMap<String, Raw> = HashMap::new();

    for (subject, predicate, object) in rows {
        let entry = raw.entry(subject).or_insert(Raw { label: None, icon: None });
        match predicate.as_str() {
            p if p == rdfs::LABEL => {
                if entry.label.is_none() { entry.label = object.as_literal(); }
            }
            "foundation:hasIcon" => {
                if entry.icon.is_none() {
                    entry.icon = match &object {
                        crate::eavto::Object::Iri(icon_iri) =>
                            crate::owl::icon_iri_to_display(conn, icon_iri),
                        crate::eavto::Object::Literal { value, .. } =>
                            Some(crate::owl::icon_literal_to_display(value)),
                        _ => None,
                    };
                }
            }
            _ => {}
        }
    }

    raw.into_iter().map(|(iri, r)| {
        let label = r.label.unwrap_or_else(|| iri.clone());
        (iri, IriResolution { label, icon: r.icon })
    }).collect()
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn entity__resolve_iris(
    iris: Vec<String>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.read(move |conn| {
        let result = resolve_iris_batch(conn, iris);
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await
}

#[cfg(test)]
mod resolve_iris_tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Triple, Object};
    use crate::owl::vocabulary::rdfs;

    #[test]
    fn test_resolve_iris_empty_returns_empty_map() {
        let conn = setup_test_db();
        let result = resolve_iris_batch(&conn, vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_iris_unknown_iri_not_included() {
        let conn = setup_test_db();
        let result = resolve_iris_batch(&conn, vec!["foundation:Unknown_999".to_string()]);
        assert!(!result.contains_key("foundation:Unknown_999"));
    }

    #[test]
    fn test_resolve_iris_returns_label_and_icon() {
        let mut conn = setup_test_db();
        store::assert_triples(&mut conn, &[
            Triple::new("foundation:Known_123", rdfs::LABEL, Object::Literal {
                value: "Known Entity".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new("foundation:Known_123", "foundation:hasIcon", Object::Literal {
                value: "star".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
        ], "test").unwrap();

        let result = resolve_iris_batch(&conn, vec!["foundation:Known_123".to_string()]);
        let entry = result.get("foundation:Known_123").unwrap();
        assert_eq!(entry.label, "Known Entity");
        assert_eq!(entry.icon.as_deref(), Some("star"));
    }
}
