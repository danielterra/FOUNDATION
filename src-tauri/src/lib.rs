#![allow(non_snake_case)]

use tauri_plugin_opener::OpenerExt;

mod namespaces;

// Re-export foundation-core leaf modules that have no app-crate additions.
// `eavto`, `search`, `diagnostics`, and `paths` are thin re-exports so all
// existing `crate::eavto`, `crate::search`, etc. references continue to resolve.
pub use foundation_core::diagnostics;
pub use foundation_core::eavto;
pub use foundation_core::search;
pub use foundation_core::paths;

// `owl` is declared as a local module because the app crate adds two
// tauri-dependent workers (formula_worker, query_worker) on top of the core.
pub mod owl;

mod commands;
pub mod core_ontology;
pub mod ai;
mod mcp;
pub mod process_automation;
pub mod imap;
pub mod data_sync;
pub mod config;
pub mod files;
pub mod realtime;

#[derive(serde::Serialize)]
pub struct TripleData {
    subject: String,
    predicate: String,
    object: Option<String>,           // IRI or blank node (if object_type = 'iri' or 'blank')
    object_value: Option<String>,      // Literal value (if object_type = 'literal')
    object_type: String,               // 'iri', 'literal', or 'blank'
    object_datatype: Option<String>,   // Datatype IRI for literals
    tx: i64,
    origin_id: i64,
    retracted: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DisplayTriple {
    a: String,            // attribute (predicate)
    v: String,            // value (object or object_value)
    v_type: String,       // value type: 'ref' for IRIs, 'literal' for literals
    v_label: Option<String>, // optional label for the value (when v_type is 'ref')
    v_icon: Option<String>, // optional icon for the value (when v_type is 'ref')
    a_comment: Option<String>, // optional comment for the predicate
    domain: Option<String>, // optional domain class IRI for the predicate
    domain_label: Option<String>, // optional label for the domain class
    domain_icon: Option<String>, // optional icon for the domain class
}

#[derive(serde::Serialize)]
pub struct GraphNode {
    id: String,
    label: String,
    group: i32, // 1=Class, 6=Instance
    icon: Option<String>, // Material Symbols icon name
}

#[derive(serde::Serialize)]
pub struct GraphLink {
    source: String,
    target: String,
    label: String,
}

#[derive(serde::Serialize)]
pub struct GraphData {
    nodes: Vec<GraphNode>,
    links: Vec<GraphLink>,
    central_node_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    id: String,
    label: String,
    definition: Option<String>,
    group: i32,
    score: f32,
}

#[derive(serde::Serialize)]
pub struct NodeStatistics {
    children_count: i64,
    backlinks_count: i64,
    synonyms_count: i64,
    related_count: i64,
    examples_count: i64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SetupData {
    person_name: String,
    person_email: Option<String>,
    computer_name: String,
    computer_processor: Option<String>,
    computer_memory: Option<i32>,
}

#[derive(serde::Serialize)]
pub struct SystemInfo {
    hostname: String,
    os_name: String,
    os_version: String,
    cpu_brand: String,
    cpu_cores: usize,
    total_memory_gb: f64,
}

#[derive(Clone, serde::Serialize)]
pub struct ImportProgress {
    pub stage: String,       // "core", "dtype", "foundation"
    pub current_file: String, // Name of the file being imported
    pub current: u32,        // Current file (1-based)
    pub total: u32,          // Total number of files
    pub triples: u64,        // Total triples imported so far
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(process_automation::scheduler::SchedulerState::new())
        .manage(process_automation::task_scheduler::TaskSchedulerState::new())
        .manage(process_automation::task_manager::TaskExecutionState::new())
        .manage(commands::AiCancellationState::new())
        .manage(commands::ConversationProcessingState::new())
        .manage(commands::LocalModelDownloadState::default())
        .manage(realtime::SubscriptionRegistry::default())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let window_config = app.config().app.windows.first()
                .expect("tauri.conf.json must declare at least one window");
            let opener_handle = app_handle.clone();
            let nav_handle = app_handle.clone();
            tauri::WebviewWindowBuilder::from_config(&app_handle, window_config)?
                .on_new_window(move |url, _features| {
                    let scheme = url.scheme();
                    if scheme == "http" || scheme == "https" || scheme == "mailto" {
                        let _ = opener_handle.opener().open_url(url.as_str(), None::<&str>);
                        tauri::webview::NewWindowResponse::Deny
                    } else {
                        tauri::webview::NewWindowResponse::Allow
                    }
                })
                .on_navigation(move |url| {
                    let scheme = url.scheme();
                    if scheme == "http" || scheme == "https" || scheme == "mailto" {
                        let host = url.host_str().unwrap_or("");
                        if host == "localhost" || host == "ipc.localhost" || host == "asset.localhost" {
                            return true;
                        }
                        let _ = nav_handle.opener().open_url(url.as_str(), None::<&str>);
                        return false;
                    }
                    true
                })
                .build()?;

            tauri::async_runtime::spawn(mcp::serve(app_handle.clone()));
            process_automation::trigger::register_listeners(app_handle.clone());
            process_automation::scheduler::listen_for_new_timers(app_handle.clone());
            process_automation::task_scheduler::listen_for_scheduled_tasks(app_handle.clone());
            process_automation::task_blocker::listen_for_blocking(app_handle.clone());
            process_automation::task_manager::listen_for_in_progress(app_handle.clone());
            process_automation::task_manager::listen_for_recurrence(app_handle.clone());
            process_automation::script_validator::register_validator(app_handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::initialize_app,
            commands::setup__check,
            commands::setup__reset,
            commands::setup__init,
            commands::setup__list_ai_services,
            commands::setup__list_ai_models,
            commands::setup__get_current_ai_model,
            commands::setup__save_ai_model,
            commands::setup__get_active_model_info,
            commands::setup__get_current_ai_service,
            commands::setup__save_ai_service,
            commands::agent__get_ai_config,
            commands::agent__set_ai_config,
            commands::inspector__get_entity,
            commands::inspector__get_backlink_page,
            commands::inspector__get_applicable_properties,
            commands::owl__search_entities,
            commands::graph__get_node_type_config,
            commands::notification__count_pending,
            commands::notification__list_iris,
            commands::widget_inspector__clear_property,
            commands::widget_inspector__update_property,
            commands::widget_inspector__set_references,
            commands::widget_inspector__add_property_value,
            commands::widget_inspector__remove_property_value,
            commands::widget_inspector__update_status,
            commands::widget_inspector__get_delete_impact,
            commands::widget_inspector__delete_individual,
            commands::widget_inspector__define_class_property,
            commands::widget_inspector__add_class_disjoint,
            commands::widget_inspector__remove_disjoint_pair,
            commands::widget_inspector__retract_disjoint_set,
            commands::widget_inspector__check_property_usage,
            commands::widget_inspector__remove_class_property,
            commands::widget_inspector__set_system_locked,
            commands::widget_inspector__set_property_cardinality,
            commands::widget_inspector__set_property_query_config,
            commands::log_frontend,
            commands::get_log_file_path_command,
            commands::clear_logs,
            commands::ai__save_api_key,
            commands::ai__get_api_key,
            commands::ai__delete_api_key,
            commands::ai__list_api_calls,
            commands::ai__list_openrouter_models,
            commands::ai__ensure_openrouter_model,
            commands::ai__get_fallback_models,
            commands::ai__save_fallback_models,
            commands::ai__validate_provider_key,
            commands::ai__get_service_base_url,
            commands::ai__save_service_base_url,
            commands::ai__initialize,
            commands::chat__attach_file,
            commands::chat__get_recent_messages,
            commands::chat__get_message_by_iri,
            commands::chat__get_conversation_snapshot_tx,
            commands::chat__send_and_reply,
            commands::chat__cancel,
            commands::chat__edit_and_retry,
            commands::chat__retry_from_message,
            commands::chat__purge_conversations,
            commands::chat__answer_question,
            commands::chat__dismiss_question,
            commands::chat::conversation::chat__create_conversation,
            commands::chat::conversation::chat__list_conversations,
            commands::chat::conversation::chat__rename_conversation,
            commands::chat::conversation::chat__delete_conversation,
            commands::chat::conversation::chat__get_conversation_agent,
            commands::chat::conversation::chat__list_agents,
            commands::chat::conversation::chat__set_conversation_agent,
            commands::widget_blackboard__resolve_blackboard,
            commands::widget_blackboard__get_widgets,
            commands::widget_blackboard__add_widget,
            commands::widget_blackboard__remove_widget,
            commands::widget_blackboard__update_widget_position,
            commands::widget_blackboard__update_widget_size,
            commands::widget_blackboard__update_widget_content,
            commands::widget_blackboard__update_widget_window_state,
            commands::widget_blackboard__list_widget_definitions,
            commands::widget_blackboard__get_graph_data,
            commands::connector__save_credential,
            commands::connector__get_credential_summary,
            commands::connector__test_auth,
            commands::connector__get_oauth2_summary,
            commands::connector__test_oauth2_auth,
            commands::connector__export_package,
            commands::connector__import_package,
            commands::automation__get_graph,
            commands::automation__run,
            commands::automation__find_for_types,
            commands::automation__get_execution,
            commands::task__delegate,
            commands::entity__resolve_iris,
            commands::local_model::setup__get_local_model_status,
            commands::local_model::setup__download_local_model,
            commands::local_model::setup__cancel_local_model_download,
            commands::imap::imap__save_account,
            commands::imap::imap__get_accounts,
            commands::imap::imap__test_connection,
            commands::imap::imap__list_folders,
            commands::imap::imap__save_monitored_folders,
            commands::imap::imap__get_sync_history,
            commands::imap::imap__start_account_sync,
            commands::imap::imap__delete_account,
            commands::settings__is_folder_configured,
            commands::settings__get_foundation_dir,
            commands::settings__save_foundation_dir,
            commands::settings__set_foundation_dir,
            commands::settings__connect_claude_desktop,
            commands::settings__get_history_retention_days,
            commands::settings__set_history_retention_days,
            commands::recover_database,
            commands::datasync__list_sources,
            commands::datasync__run,
            commands::datasync__list_raw,
            commands::datasync__inspect_raw,
            commands::datasync__retry_raw,
            realtime::events__set_subscriptions,
            realtime::events__set_subscriptions_v2,
            realtime::events__replay_since
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
