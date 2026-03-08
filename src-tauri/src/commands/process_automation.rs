use crate::owl::DbExecutor;
use crate::process_automation::{executor, scheduler};
use tauri::{AppHandle, State};

#[tauri::command]
#[allow(non_snake_case)]
pub async fn process__reload_scheduler(
    app: AppHandle,
    _executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    scheduler::reload(app).await;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn process__execute(
    app: AppHandle,
    _executor: State<'_, DbExecutor>,
    process_iri: String,
) -> Result<(), String> {
    executor::run_process(&app, &process_iri).await
}
