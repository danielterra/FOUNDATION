use chrono::Utc;

use crate::commands::connector::test_connector_auth;
use crate::core_ontology::data_sync::DATASOURCE_STATUS_ACTIVE;
use crate::owl::{DbExecutor, Individual, Object};

/// Status IRI for active/running BPMN flow nodes and timer definitions.
const BPMN_STATUS_ACTIVE: &str = "foundation:Status_1773183515321";

fn str_lit(v: impl Into<String>) -> Object {
    Object::Literal {
        value: v.into(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }
}

/// Parameters for creating a new DataSource.
pub struct CreateDataSourceParams {
    pub connector_iri: String,
    pub sync_namespace: String,
    pub sync_schedule: String,
    pub target_endpoint: String,
    pub item_path: String,
    pub label: String,
    /// Optional Rhai script for the transform automation's CodeTask.
    /// When provided, a transform Automation is created and linked via
    /// `foundation:transformAutomation` on the DataSource.
    pub transform_script: Option<String>,
}

/// Creates a `foundation:DataSource`, validates the connector credential,
/// and seeds two child Automations:
///   (a) extract — TimerStartEvent (cron from sync_schedule) → CodeTask calling
///       `datasync_run(run_transform: true)` → EndEvent;
///   (b) transform — StartEvent → CodeTask (transform_script) → EndEvent, linked
///       via `foundation:transformAutomation` on the DataSource.
///
/// Returns the IRI of the created DataSource.
pub async fn create_data_source(
    executor: &DbExecutor,
    params: CreateDataSourceParams,
) -> Result<String, String> {
    let now_ms = Utc::now().timestamp_millis();
    let data_source_iri = format!("foundation:DataSource_{}", now_ms);

    let connector_iri = params.connector_iri.clone();
    let data_source_iri_clone = data_source_iri.clone();
    let label_clone = params.label.clone();
    let ns_clone = params.sync_namespace.clone();
    let sched_clone = params.sync_schedule.clone();
    let endpoint_clone = params.target_endpoint.clone();
    let item_path_clone = params.item_path.clone();

    executor.write(move |conn| {
        let ind = Individual::new(&data_source_iri_clone);
        ind.assert(conn, "foundation:DataSource", &label_clone, "cloud_download", "data_sync")
            .map_err(|e| format!("assert DataSource: {}", e))?;

        ind.add_property(conn, "foundation:hasStatus",
            vec![Object::Iri(DATASOURCE_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("hasStatus: {}", e))?;

        ind.add_property(conn, "foundation:transportKind",
            vec![str_lit("request")], "data_sync")
            .map_err(|e| format!("transportKind: {}", e))?;

        ind.add_property(conn, "foundation:syncDirection",
            vec![str_lit("in")], "data_sync")
            .map_err(|e| format!("syncDirection: {}", e))?;

        ind.add_property(conn, "foundation:usesConnector",
            vec![Object::Iri(connector_iri.clone())], "data_sync")
            .map_err(|e| format!("usesConnector: {}", e))?;

        ind.add_property(conn, "foundation:syncNamespace",
            vec![str_lit(ns_clone)], "data_sync")
            .map_err(|e| format!("syncNamespace: {}", e))?;

        ind.add_property(conn, "foundation:syncSchedule",
            vec![str_lit(sched_clone)], "data_sync")
            .map_err(|e| format!("syncSchedule: {}", e))?;

        if !endpoint_clone.is_empty() {
            ind.add_property(conn, "foundation:targetEndpoint",
                vec![str_lit(endpoint_clone)], "data_sync")
                .map_err(|e| format!("targetEndpoint: {}", e))?;
        }

        if !item_path_clone.is_empty() {
            ind.add_property(conn, "foundation:itemPath",
                vec![str_lit(item_path_clone)], "data_sync")
                .map_err(|e| format!("itemPath: {}", e))?;
        }

        ind.add_property(conn, "foundation:isConnected",
            vec![Object::Boolean(false)], "data_sync")
            .map_err(|e| format!("isConnected: {}", e))?;

        Ok(data_source_iri_clone)
    }).await?;

    let auth_result = test_connector_auth(&params.connector_iri, executor).await;

    match auth_result {
        Ok(_msg) => {
            let ds_iri = data_source_iri.clone();
            executor.write(move |conn| {
                crate::core_ontology::data_sync::update_datasource_connection(
                    conn, &ds_iri, true, None,
                )?;
                Ok(String::new())
            }).await?;
        }
        Err(err_msg) => {
            let ds_iri = data_source_iri.clone();
            let err_clone = err_msg.clone();
            executor.write(move |conn| {
                crate::core_ontology::data_sync::update_datasource_connection(
                    conn, &ds_iri, false, Some(&err_clone),
                )?;
                Ok(String::new())
            }).await?;
        }
    }

    // Seed the transform automation (StartEvent → CodeTask → EndEvent) first, so
    // the extract automation can reference the DataSource IRI that owns it.
    if let Some(script) = params.transform_script {
        seed_transform_automation(executor, &data_source_iri, &params.label, &script).await?;
    }

    // Seed the extract automation (TimerStartEvent → CodeTask → EndEvent).
    seed_extract_automation(executor, &data_source_iri, &params.label, &params.sync_schedule).await?;

    Ok(data_source_iri)
}

/// Creates the extract Automation: TimerStartEvent (timeCycle = cron) → CodeTask
/// (calls datasync_run with run_transform: true) → EndEvent.
/// Links the Automation to the DataSource via `foundation:extractAutomation`.
async fn seed_extract_automation(
    executor: &DbExecutor,
    data_source_iri: &str,
    ds_label: &str,
    cron_expr: &str,
) -> Result<(), String> {
    let now_ms = Utc::now().timestamp_millis();
    let auto_iri = format!("foundation:Automation_extract_{}", now_ms);
    let timer_def_iri = format!("foundation:TimerEventDef_{}", now_ms);
    let start_iri = format!("foundation:FlowNode_timerstart_{}", now_ms);
    let task_iri = format!("foundation:FlowNode_codetask_{}", now_ms);
    let end_iri = format!("foundation:FlowNode_end_{}", now_ms);
    let sf1_iri = format!("foundation:SeqFlow_te_{}", now_ms);
    let sf2_iri = format!("foundation:SeqFlow_tc_{}", now_ms);

    let ds_iri = data_source_iri.to_string();
    let cron = cron_expr.to_string();
    let label = format!("Extract — {}", ds_label);

    let script = format!(
        r#"let r = parse_json(mcp("datasync_run", to_json(#{{"data_source_iri": "{}", "run_transform": true}})));
if r.success {{ "ok" }} else {{ throw r.error }}"#,
        ds_iri
    );

    let auto_iri_clone = auto_iri.clone();

    executor.write(move |conn| {
        // Automation root
        Individual::new(&auto_iri_clone)
            .assert(conn, "foundation:Automation", &label, "bolt", "data_sync")
            .map_err(|e| format!("assert extract Automation: {}", e))?;
        Individual::new(&auto_iri_clone)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri("foundation:Pending".to_string())], "data_sync")
            .map_err(|e| format!("extract Automation hasStatus: {}", e))?;
        Individual::new(&auto_iri_clone)
            .add_property(conn, "foundation:controlClass",
                vec![Object::Iri("foundation:AutomationExecutionControl".to_string())], "data_sync")
            .map_err(|e| format!("extract Automation controlClass: {}", e))?;

        // TimerEventDefinition (carries the cron)
        Individual::new(&timer_def_iri)
            .assert(conn, "foundation:automation_TimerEventDefinition",
                &format!("Timer — {}", label), "schedule", "data_sync")
            .map_err(|e| format!("assert TimerEventDefinition: {}", e))?;
        Individual::new(&timer_def_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("timer def hasStatus: {}", e))?;
        Individual::new(&timer_def_iri)
            .add_property(conn, "foundation:timeCycle",
                vec![str_lit(cron)], "data_sync")
            .map_err(|e| format!("timeCycle: {}", e))?;

        // TimerStartEvent
        Individual::new(&start_iri)
            .assert(conn, "foundation:automation_TimerStartEvent",
                "Timer Start", "play_arrow", "data_sync")
            .map_err(|e| format!("assert TimerStartEvent: {}", e))?;
        Individual::new(&start_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("start event hasStatus: {}", e))?;
        Individual::new(&start_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("start partOfProcess: {}", e))?;
        Individual::new(&start_iri)
            .add_property(conn, "foundation:eventDefinition",
                vec![Object::Iri(timer_def_iri.clone())], "data_sync")
            .map_err(|e| format!("eventDefinition: {}", e))?;

        // CodeTask
        Individual::new(&task_iri)
            .assert(conn, "foundation:automation_CodeTask",
                "Run Extract", "code", "data_sync")
            .map_err(|e| format!("assert CodeTask: {}", e))?;
        Individual::new(&task_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("code task hasStatus: {}", e))?;
        Individual::new(&task_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("code task partOfProcess: {}", e))?;
        Individual::new(&task_iri)
            .add_property(conn, "foundation:script",
                vec![str_lit(script)], "data_sync")
            .map_err(|e| format!("code task script: {}", e))?;

        // EndEvent
        Individual::new(&end_iri)
            .assert(conn, "foundation:automation_EndEvent",
                "End", "stop_circle", "data_sync")
            .map_err(|e| format!("assert EndEvent: {}", e))?;
        Individual::new(&end_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("end event hasStatus: {}", e))?;
        Individual::new(&end_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("end partOfProcess: {}", e))?;

        // SequenceFlow: start → task
        Individual::new(&sf1_iri)
            .assert(conn, "foundation:automation_SequenceFlow",
                "SF1", "arrow_forward", "data_sync")
            .map_err(|e| format!("assert SF1: {}", e))?;
        Individual::new(&sf1_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("sf1 hasStatus: {}", e))?;
        Individual::new(&sf1_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("sf1 partOfProcess: {}", e))?;
        Individual::new(&sf1_iri)
            .add_property(conn, "foundation:sourceRef",
                vec![Object::Iri(start_iri.clone())], "data_sync")
            .map_err(|e| format!("sf1 sourceRef: {}", e))?;
        Individual::new(&sf1_iri)
            .add_property(conn, "foundation:targetRef",
                vec![Object::Iri(task_iri.clone())], "data_sync")
            .map_err(|e| format!("sf1 targetRef: {}", e))?;

        // SequenceFlow: task → end
        Individual::new(&sf2_iri)
            .assert(conn, "foundation:automation_SequenceFlow",
                "SF2", "arrow_forward", "data_sync")
            .map_err(|e| format!("assert SF2: {}", e))?;
        Individual::new(&sf2_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("sf2 hasStatus: {}", e))?;
        Individual::new(&sf2_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("sf2 partOfProcess: {}", e))?;
        Individual::new(&sf2_iri)
            .add_property(conn, "foundation:sourceRef",
                vec![Object::Iri(task_iri.clone())], "data_sync")
            .map_err(|e| format!("sf2 sourceRef: {}", e))?;
        Individual::new(&sf2_iri)
            .add_property(conn, "foundation:targetRef",
                vec![Object::Iri(end_iri.clone())], "data_sync")
            .map_err(|e| format!("sf2 targetRef: {}", e))?;

        Ok(String::new())
    }).await?;

    Ok(())
}

/// Creates the transform Automation: StartEvent → CodeTask (script) → EndEvent.
/// Links the Automation to the DataSource via `foundation:transformAutomation`.
async fn seed_transform_automation(
    executor: &DbExecutor,
    data_source_iri: &str,
    ds_label: &str,
    script: &str,
) -> Result<(), String> {
    let now_ms = Utc::now().timestamp_millis();
    let auto_iri = format!("foundation:Automation_transform_{}", now_ms);
    let start_iri = format!("foundation:FlowNode_tstart_{}", now_ms);
    let task_iri = format!("foundation:FlowNode_ttask_{}", now_ms);
    let end_iri = format!("foundation:FlowNode_tend_{}", now_ms);
    let sf1_iri = format!("foundation:SeqFlow_tts_{}", now_ms);
    let sf2_iri = format!("foundation:SeqFlow_tte_{}", now_ms);

    let ds_iri = data_source_iri.to_string();
    let label = format!("Transform — {}", ds_label);
    let script_owned = script.to_string();
    let auto_iri_clone = auto_iri.clone();
    let ds_iri_clone = ds_iri.clone();

    executor.write(move |conn| {
        // Automation root
        Individual::new(&auto_iri_clone)
            .assert(conn, "foundation:Automation", &label, "sync", "data_sync")
            .map_err(|e| format!("assert transform Automation: {}", e))?;
        Individual::new(&auto_iri_clone)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri("foundation:Pending".to_string())], "data_sync")
            .map_err(|e| format!("transform Automation hasStatus: {}", e))?;
        Individual::new(&auto_iri_clone)
            .add_property(conn, "foundation:controlClass",
                vec![Object::Iri("foundation:AutomationExecutionControl".to_string())], "data_sync")
            .map_err(|e| format!("transform Automation controlClass: {}", e))?;

        // StartEvent
        Individual::new(&start_iri)
            .assert(conn, "foundation:automation_StartEvent",
                "Start", "play_arrow", "data_sync")
            .map_err(|e| format!("assert transform StartEvent: {}", e))?;
        Individual::new(&start_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("transform start hasStatus: {}", e))?;
        Individual::new(&start_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("transform start partOfProcess: {}", e))?;

        // CodeTask
        Individual::new(&task_iri)
            .assert(conn, "foundation:automation_CodeTask",
                "Run Transform", "code", "data_sync")
            .map_err(|e| format!("assert transform CodeTask: {}", e))?;
        Individual::new(&task_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("transform code task hasStatus: {}", e))?;
        Individual::new(&task_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("transform code task partOfProcess: {}", e))?;
        Individual::new(&task_iri)
            .add_property(conn, "foundation:script",
                vec![str_lit(script_owned)], "data_sync")
            .map_err(|e| format!("transform code task script: {}", e))?;

        // EndEvent
        Individual::new(&end_iri)
            .assert(conn, "foundation:automation_EndEvent",
                "End", "stop_circle", "data_sync")
            .map_err(|e| format!("assert transform EndEvent: {}", e))?;
        Individual::new(&end_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("transform end hasStatus: {}", e))?;
        Individual::new(&end_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("transform end partOfProcess: {}", e))?;

        // SequenceFlow: start → task
        Individual::new(&sf1_iri)
            .assert(conn, "foundation:automation_SequenceFlow",
                "SF1", "arrow_forward", "data_sync")
            .map_err(|e| format!("assert transform SF1: {}", e))?;
        Individual::new(&sf1_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("transform sf1 hasStatus: {}", e))?;
        Individual::new(&sf1_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("transform sf1 partOfProcess: {}", e))?;
        Individual::new(&sf1_iri)
            .add_property(conn, "foundation:sourceRef",
                vec![Object::Iri(start_iri.clone())], "data_sync")
            .map_err(|e| format!("transform sf1 sourceRef: {}", e))?;
        Individual::new(&sf1_iri)
            .add_property(conn, "foundation:targetRef",
                vec![Object::Iri(task_iri.clone())], "data_sync")
            .map_err(|e| format!("transform sf1 targetRef: {}", e))?;

        // SequenceFlow: task → end
        Individual::new(&sf2_iri)
            .assert(conn, "foundation:automation_SequenceFlow",
                "SF2", "arrow_forward", "data_sync")
            .map_err(|e| format!("assert transform SF2: {}", e))?;
        Individual::new(&sf2_iri)
            .add_property(conn, "foundation:hasStatus",
                vec![Object::Iri(BPMN_STATUS_ACTIVE.to_string())], "data_sync")
            .map_err(|e| format!("transform sf2 hasStatus: {}", e))?;
        Individual::new(&sf2_iri)
            .add_property(conn, "foundation:partOfProcess",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("transform sf2 partOfProcess: {}", e))?;
        Individual::new(&sf2_iri)
            .add_property(conn, "foundation:sourceRef",
                vec![Object::Iri(task_iri.clone())], "data_sync")
            .map_err(|e| format!("transform sf2 sourceRef: {}", e))?;
        Individual::new(&sf2_iri)
            .add_property(conn, "foundation:targetRef",
                vec![Object::Iri(end_iri.clone())], "data_sync")
            .map_err(|e| format!("transform sf2 targetRef: {}", e))?;

        // Link DataSource → transform Automation
        Individual::new(&ds_iri_clone)
            .add_property(conn, "foundation:transformAutomation",
                vec![Object::Iri(auto_iri_clone.clone())], "data_sync")
            .map_err(|e| format!("transformAutomation link: {}", e))?;

        Ok(String::new())
    }).await?;

    Ok(())
}

/// Reads the configuration needed to build the extract URL for a DataSource.
pub struct DataSourceConfig {
    pub data_source_iri: String,
    pub connector_iri: String,
    pub sync_namespace: String,
    pub sync_schedule: String,
    pub target_endpoint: String,
    pub item_path: String,
    pub base_url: String,
    pub cred_iri: Option<String>,
    pub transform_automation_iri: Option<String>,
}

pub async fn load_data_source_config(
    executor: &DbExecutor,
    data_source_iri: &str,
) -> Result<DataSourceConfig, String> {
    let ds_iri = data_source_iri.to_string();
    executor.read(move |conn| {
        let connector_iri = crate::owl::get_iri_property(conn, &ds_iri, "foundation:usesConnector")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("DataSource {} has no usesConnector", ds_iri))?;

        let sync_namespace = crate::owl::get_literal_property(conn, &ds_iri, "foundation:syncNamespace")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        let sync_schedule = crate::owl::get_literal_property(conn, &ds_iri, "foundation:syncSchedule")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        let target_endpoint = crate::owl::get_literal_property(conn, &ds_iri, "foundation:targetEndpoint")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        let item_path = crate::owl::get_literal_property(conn, &ds_iri, "foundation:itemPath")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        let base_url = crate::owl::get_literal_property(conn, &connector_iri, "foundation:baseUrl")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        let cred_iri = crate::owl::get_iri_property(conn, &connector_iri, "foundation:hasCredential")
            .map_err(|e| e.to_string())?;

        let transform_automation_iri = crate::owl::get_iri_property(conn, &ds_iri, "foundation:transformAutomation")
            .map_err(|e| e.to_string())?;

        Ok(DataSourceConfig {
            data_source_iri: ds_iri.clone(),
            connector_iri,
            sync_namespace,
            sync_schedule,
            target_endpoint,
            item_path,
            base_url,
            cred_iri,
            transform_automation_iri,
        })
    }).await
}
