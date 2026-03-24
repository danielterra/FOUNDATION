use serde::Serialize;
use tauri::{State, AppHandle, Emitter, Manager};

use crate::owl::{self, Connection, DbExecutor, Individual, Object};
use crate::owl::formula_worker::FormulaWorker;
use super::setup_system_info::{get_cpu_info, get_memory_info, get_os_info, get_locale_info};

/// Initialize the application database
/// This MUST be called before any other commands that use the database
/// The frontend should call this on startup and handle permission requests
#[tauri::command]
#[allow(non_snake_case)]
pub async fn initialize_app(
    app: AppHandle,
) -> Result<(), String> {
    // Skip initialization during CI/CD or build process
    // The build process runs the app to collect metadata but doesn't need DB
    if std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("TAURI_ENV_DEBUG").is_ok() // Set during tauri build
    {
        super::log_backend("info", "Skipping database initialization (build/CI mode)");
        let dummy_conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to create in-memory connection: {}", e))?;
        let executor = DbExecutor::new_in_memory(dummy_conn);
        app.manage(executor);
        let _ = app.emit("import-complete", ());
        return Ok(());
    }

    let (mut conn, db_path) = owl::initialize_with_progress(app.clone())
        .map_err(|e| {
            let error_msg = format!("Failed to initialize database: {:?}", e);
            super::log_backend("error", &error_msg);
            let _ = app.emit("import-error", error_msg.clone());
            error_msg
        })?;

    owl::seed_icon_library(&mut conn);

    if let Ok(stats) = owl::get_stats(&conn) {
        let stats_msg = format!(
            "Database initialized - Triples: {}, Active: {}, Transactions: {}, Entities: {}",
            stats.total_facts, stats.active_facts, stats.total_transactions, stats.entities_count
        );
        super::log_backend("info", &stats_msg);
    }

    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel::<(Vec<String>, Vec<String>)>();
    let executor = DbExecutor::new_with_notify(conn, db_path.clone(), Some(notify_tx));
    let executor_for_worker = executor.clone();
    let executor_for_recover = executor.clone();
    app.manage(executor);

    let app_for_notify = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some((subjects, iri_objects)) = notify_rx.recv().await {
            let mut seen = std::collections::HashSet::new();
            for iri in subjects {
                if seen.insert(iri.clone()) {
                    app_for_notify.emit("entity-updated", serde_json::json!({ "entityId": iri })).ok();
                }
            }
            for iri in iri_objects {
                if seen.insert(iri.clone()) {
                    // entity-referenced: a write created a link pointing TO this IRI (new backlink).
                    app_for_notify.emit("entity-referenced", serde_json::json!({ "entityId": iri })).ok();
                }
            }
        }
    });

    let worker = FormulaWorker::spawn(app.clone(), executor_for_worker);
    recover_pending_jobs(&executor_for_recover, &worker).await;
    app.manage(worker);

    crate::process_automation::executor::recover_interrupted_executions(&app).await;

    tauri::async_runtime::spawn(crate::process_automation::scheduler::reload(app.clone()));

    let _ = app.emit("import-complete", ());

    Ok(())
}

async fn recover_pending_jobs(
    executor: &crate::owl::DbExecutor,
    worker: &FormulaWorker,
) {
    use crate::owl::formula_worker::WorkerCommand;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let _ = executor.write(move |conn| {
        conn.execute(
            "UPDATE formula_recalc_jobs SET status = 'pending', updated_at = ? WHERE status = 'running'",
            rusqlite::params![now],
        ).map(|_| String::new()).map_err(|e| e.to_string())
    }).await;

    let job_ids: Vec<String> = executor.read(|conn| {
        let mut stmt = match conn.prepare(
            "SELECT id FROM formula_recalc_jobs WHERE status = 'pending' ORDER BY created_at",
        ) {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let rows = match stmt.query_map([], |row| row.get(0)) {
            Ok(r) => r,
            Err(_) => return Ok(vec![]),
        };
        Ok(rows.filter_map(|r| r.ok()).collect())
    }).await.unwrap_or_default();

    for job_id in job_ids {
        let _ = worker.sender.try_send(WorkerCommand::Enqueue { job_id });
    }
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub already_setup: bool,
    pub user: UserInfo,
    pub computer: ComputerInfo,
    pub foundation: FoundationInfo,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub iri: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessorInfo {
    pub iri: String,
    pub model: String,
    pub cores: Option<i64>,
    pub architecture: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    pub iri: String,
    pub capacity_gb: i64,
    pub memory_type: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatingSystemInfo {
    pub iri: String,
    pub name: String,
    pub version: String,
    pub kernel: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerInfo {
    pub iri: String,
    pub hostname: String,
    pub operating_system: OperatingSystemInfo,
    pub processor: ProcessorInfo,
    pub memory: MemoryInfo,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareReleaseInfo {
    pub iri: String,
    pub version_number: String,
    pub license_type: Option<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundationInfo {
    pub iri: String,
    pub release: SoftwareReleaseInfo,
}

/// Check if setup has been completed
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__check(
    executor: State<'_, DbExecutor>,
) -> Result<bool, String> {
    executor.read(|conn| {
        Individual::get(conn, "foundation:ThisFoundationInstance")
            .map(|opt| opt.is_some())
            .map_err(|e| format!("Failed to check setup status: {}", e))
    }).await
}

/// Initialize setup: detect system, create instances, establish relationships
/// Should only be called when setup__check returns false
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__init(
    user_name: String,
    email: Option<String>,
    ai_service_iri: Option<String>,
    ai_model_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<SetupResult, String> {
    // Setup involves writes, so we use the write executor
    let result_json = executor.write(move |conn| {

    // Don't check again - assume caller used setup__check first

    let hostname = hostname::get()
        .map_err(|e| format!("Failed to get hostname: {}", e))?
        .to_string_lossy()
        .to_string();

    let os_info = get_os_info();
    let cpu_info = get_cpu_info();
    let memory_info = get_memory_info();

    let user = Individual::new("foundation:ThisUser");
    user.assert(conn, "foundation:Person", &user_name, "person", "setup")
        .map_err(|e| format!("Failed to create Person: {}", e))?;

    let name_obj = Object::Literal {
        value: user_name.clone(),
        datatype: Some("xsd:string".to_string()),
        language: Some("en".to_string()),
    };
    user.add_property(conn, "foundation:name", vec![name_obj], "setup")
        .map_err(|e| format!("Failed to add user name: {}", e))?;

    if let Some(ref email_val) = email {
        let email_obj = Object::Literal {
            value: email_val.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        user.add_property(conn, "foundation:email", vec![email_obj], "setup")
            .map_err(|e| format!("Failed to add user email: {}", e))?;
    }

    let processor = Individual::new("foundation:ThisProcessor");
    processor.assert(conn, "foundation:Processor", &cpu_info.model, "computer", "setup")
        .map_err(|e| format!("Failed to create Processor: {}", e))?;

    let model_obj = Object::Literal {
        value: cpu_info.model.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    processor.add_property(conn, "foundation:processorModel", vec![model_obj], "setup")
        .map_err(|e| format!("Failed to add processor model: {}", e))?;

    if let Some(cores) = cpu_info.cores {
        processor.add_property(conn, "foundation:coreCount", vec![Object::Integer(cores)], "setup")
            .map_err(|e| format!("Failed to add core count: {}", e))?;
    }

    let arch_obj = Object::Literal {
        value: cpu_info.architecture.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    processor.add_property(conn, "foundation:architecture", vec![arch_obj], "setup")
        .map_err(|e| format!("Failed to add architecture: {}", e))?;

    let memory = Individual::new("foundation:ThisMemory");
    let memory_label = format!("{}GB RAM", memory_info.capacity_gb);
    memory.assert(conn, "foundation:Memory", &memory_label, "computer", "setup")
        .map_err(|e| format!("Failed to create Memory: {}", e))?;

    memory.add_property(
        conn,
        "foundation:memoryCapacity",
        vec![Object::Integer(memory_info.capacity_gb)],
        "setup",
    ).map_err(|e| format!("Failed to add memory capacity: {}", e))?;

    let mem_type_obj = Object::Literal {
        value: memory_info.memory_type.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    memory.add_property(conn, "foundation:memoryType", vec![mem_type_obj], "setup")
        .map_err(|e| format!("Failed to add memory type: {}", e))?;

    let os = Individual::new("foundation:ThisOperatingSystem");
    let os_label = format!("{} {}", os_info.name, os_info.version);
    os.assert(conn, "foundation:OperatingSystem", &os_label, "computer", "setup")
        .map_err(|e| format!("Failed to create OperatingSystem: {}", e))?;

    let os_name_obj = Object::Literal {
        value: os_info.name.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osName", vec![os_name_obj], "setup")
        .map_err(|e| format!("Failed to add OS name: {}", e))?;

    let os_version_obj = Object::Literal {
        value: os_info.version.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osVersion", vec![os_version_obj], "setup")
        .map_err(|e| format!("Failed to add OS version: {}", e))?;

    let os_kernel_obj = Object::Literal {
        value: os_info.kernel.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osKernel", vec![os_kernel_obj], "setup")
        .map_err(|e| format!("Failed to add OS kernel: {}", e))?;

    let computer = Individual::new("foundation:ThisComputer");
    computer.assert(conn, "foundation:Computer", &hostname, "computer", "setup")
        .map_err(|e| format!("Failed to create Computer: {}", e))?;

    let hostname_obj = Object::Literal {
        value: hostname.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    computer.add_property(conn, "foundation:hostname", vec![hostname_obj], "setup")
        .map_err(|e| format!("Failed to add hostname: {}", e))?;

    computer.add_property(
        conn,
        "foundation:hasProcessor",
        vec![Object::Iri("foundation:ThisProcessor".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link Computer -> Processor: {}", e))?;

    computer.add_property(
        conn,
        "foundation:hasMemory",
        vec![Object::Iri("foundation:ThisMemory".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link Computer -> Memory: {}", e))?;

    computer.add_property(
        conn,
        "foundation:hasOperatingSystem",
        vec![Object::Iri("foundation:ThisOperatingSystem".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link Computer -> OperatingSystem: {}", e))?;

    let version = env!("CARGO_PKG_VERSION").to_string();

    let releases = Individual::find_by_class_and_properties(
        conn,
        "foundation:SoftwareRelease",
        &[
            ("foundation:versionNumber", &version),
            ("foundation:releaseOf", "foundation:FoundationProduct"),
        ]
    ).map_err(|e| format!("Failed to query for release: {}", e))?;

    let release_iri = releases.first().ok_or_else(|| {
        format!(
            "SoftwareRelease for FOUNDATION version {} not found in ontology. \
             Please create it via MCP tools before releasing.",
            version
        )
    })?.clone();

    let foundation_label = format!("FOUNDATION v{}", version);
    let foundation = Individual::new("foundation:ThisFoundationInstance");
    foundation.assert(conn, "foundation:Application", &foundation_label, "apps", "setup")
        .map_err(|e| format!("Failed to create Application instance: {}", e))?;

    foundation.add_property(
        conn,
        "foundation:installedFrom",
        vec![Object::Iri(release_iri.clone())],
        "setup",
    ).map_err(|e| format!("Failed to link to SoftwareRelease: {}", e))?;

    let ai_assistant = Individual::new("foundation:LocalAIAssistant");
    ai_assistant.assert(conn, "foundation:SoftwareAgent", "FOUNDATION AI Assistant", "smart_toy", "setup")
        .map_err(|e| format!("Failed to create AI assistant: {}", e))?;

    let service_iri = ai_service_iri.unwrap_or_else(|| "foundation:ClaudeAIService".to_string());

    let ai_description = Object::Literal {
        value: format!("AI assistant powered by {}", service_iri),
        datatype: Some("xsd:string".to_string()),
        language: Some("en".to_string()),
    };
    ai_assistant.add_property(conn, "rdfs:comment", vec![ai_description], "setup")
        .map_err(|e| format!("Failed to add AI description: {}", e))?;

    ai_assistant.add_property(
        conn,
        "foundation:usesService",
        vec![Object::Iri(service_iri.clone())],
        "setup"
    ).map_err(|e| format!("Failed to link AI to service: {}", e))?;

    // Only create user-specific settings if values are provided (non-default)
    if let Some(model_iri) = ai_model_iri {
        // User selected a specific model - create a setting to override default
        let timestamp = chrono::Utc::now().timestamp_millis();
        let model_setting_iri = format!("foundation:AIModelSetting_{}", timestamp);
        let model_setting = Individual::new(&model_setting_iri);
        model_setting.assert(
            conn,
            "foundation:SoftwareSetting",
            "Selected AI Model",
            "settings",
            "setup",
        ).map_err(|e| format!("Failed to create model setting: {}", e))?;

        model_setting.add_property(conn, "foundation:settingKey", vec![Object::Literal {
            value: "aiModel".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "setup").map_err(|e| format!("Failed to set settingKey: {}", e))?;

        model_setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
            value: model_iri,
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "setup").map_err(|e| format!("Failed to set settingValue: {}", e))?;

        model_setting.add_property(conn, "foundation:settingCategory", vec![Object::Literal {
            value: "ai".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }], "setup").map_err(|e| format!("Failed to set settingCategory: {}", e))?;

        model_setting.add_property(
            conn,
            "foundation:origin",
            vec![Object::Iri("foundation:ThisFoundationInstance".to_string())],
            "setup"
        ).map_err(|e| format!("Failed to set origin: {}", e))?;

        model_setting.add_property(
            conn,
            "foundation:appliedTo",
            vec![Object::Iri(service_iri)],
            "setup"
        ).map_err(|e| format!("Failed to set appliedTo: {}", e))?;
    }
    // If no model provided, the default from ontology (DefaultAIModelSetting) will be used

    // Detect and update locale settings (retraction is automatic when setting same property)
    let locale_info = get_locale_info();

    let language_setting = Individual::get(conn, "foundation:DefaultLanguageSetting")
        .map_err(|e| format!("Failed to get DefaultLanguageSetting: {}", e))?
        .ok_or_else(|| "DefaultLanguageSetting not found".to_string())?;
    language_setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
        value: locale_info.language.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "setup").map_err(|e| format!("Failed to update language setting: {}", e))?;

    let locale_setting = Individual::get(conn, "foundation:DefaultLocaleSetting")
        .map_err(|e| format!("Failed to get DefaultLocaleSetting: {}", e))?
        .ok_or_else(|| "DefaultLocaleSetting not found".to_string())?;
    locale_setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
        value: locale_info.locale.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "setup").map_err(|e| format!("Failed to update locale setting: {}", e))?;

    let country_setting = Individual::get(conn, "foundation:DefaultCountrySetting")
        .map_err(|e| format!("Failed to get DefaultCountrySetting: {}", e))?
        .ok_or_else(|| "DefaultCountrySetting not found".to_string())?;
    country_setting.add_property(conn, "foundation:settingValue", vec![Object::Literal {
        value: locale_info.country.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    }], "setup").map_err(|e| format!("Failed to update country setting: {}", e))?;

    computer.add_property(
        conn,
        "foundation:hasUser",
        vec![Object::Iri("foundation:ThisUser".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link Computer -> User: {}", e))?;
    foundation.add_property(
        conn,
        "foundation:runsOn",
        vec![Object::Iri("foundation:ThisComputer".to_string())],
        "setup",
    ).map_err(|e| format!("Failed to link FOUNDATION -> Computer: {}", e))?;

    let result = SetupResult {
        already_setup: false,
        user: UserInfo {
            iri: "foundation:ThisUser".to_string(),
            name: user_name,
            email,
        },
        computer: ComputerInfo {
            iri: "foundation:ThisComputer".to_string(),
            hostname,
            operating_system: OperatingSystemInfo {
                iri: "foundation:ThisOperatingSystem".to_string(),
                name: os_info.name,
                version: os_info.version,
                kernel: os_info.kernel,
            },
            processor: ProcessorInfo {
                iri: "foundation:ThisProcessor".to_string(),
                model: cpu_info.model,
                cores: cpu_info.cores,
                architecture: cpu_info.architecture,
            },
            memory: MemoryInfo {
                iri: "foundation:ThisMemory".to_string(),
                capacity_gb: memory_info.capacity_gb,
                memory_type: memory_info.memory_type,
            },
        },
        foundation: FoundationInfo {
            iri: "foundation:ThisFoundationInstance".to_string(),
            release: SoftwareReleaseInfo {
                iri: release_iri,
                version_number: version,
                license_type: Some("MIT".to_string()),
            },
        },
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await?;

    serde_json::from_str(&result_json).map_err(|e| e.to_string())
}

/// List available AI services from the ontology
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__list_ai_services(
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor.read(|conn| {
        let service_iris = owl::find_entities_with_property(conn, "rdf:type", "foundation:AIAPIService")
            .map_err(|e| format!("Failed to query services: {}", e))?;

        let mut result = Vec::new();
        for service_iri in service_iris {
            let service_iri = &service_iri;

            if let Ok(Some(service_ind)) = Individual::get(conn, service_iri) {
                let label = service_ind.label;
                let comment = service_ind.properties.iter()
                    .find(|(k, _)| k == "rdfs:comment")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                result.push(serde_json::json!({
                    "iri": service_iri,
                    "label": label,
                    "description": comment,
                }));
            }
        }

        Ok(result)
    }).await
}

/// List available AI models for a specific service
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__list_ai_models(
    service_iri: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<serde_json::Value>, String> {
    executor.read(move |conn| {
        let model_iris = if let Some(ref service) = service_iri {
            owl::find_entities_with_property(conn, "foundation:offeredBy", service)
                .map_err(|e| format!("Failed to query models for service: {}", e))?
        } else {
            owl::find_entities_with_property(conn, "rdf:type", "foundation:AIModel")
                .map_err(|e| format!("Failed to query models: {}", e))?
        };

        let mut result = Vec::new();
        for model_iri in model_iris {
            let model_iri = &model_iri;

            if let Ok(Some(model_ind)) = Individual::get(conn, model_iri) {
                let label = model_ind.label;
                let comment = model_ind.properties.iter()
                    .find(|(k, _)| k == "rdfs:comment")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                let model_identifier = model_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:modelIdentifier")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                let is_default = model_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:isDefaultModel")
                    .and_then(|(_, v)| v.as_literal())
                    .map(|v| v == "true")
                    .unwrap_or(false);

                let model_version = model_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:modelVersion")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                result.push(serde_json::json!({
                    "iri": model_iri,
                    "label": label,
                    "description": comment,
                    "modelIdentifier": model_identifier,
                    "modelVersion": model_version,
                    "isDefault": is_default,
                }));
            }
        }

        result.sort_by(|a, b| {
            let a_default = a["isDefault"].as_bool().unwrap_or(false);
            let b_default = b["isDefault"].as_bool().unwrap_or(false);

            match (b_default, a_default) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a["label"].as_str().cmp(&b["label"].as_str()),
            }
        });

        Ok(result)
    }).await
}
