use tauri::{AppHandle, State};

use crate::owl::{DbExecutor, get_iri_property};
use crate::process_automation::task_manager;

#[tauri::command]
pub async fn task__delegate(
    task_iri: String,
    app: AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .read({
            let task_iri = task_iri.clone();
            move |conn| {
                get_iri_property(conn, &task_iri, "foundation:assignee")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Task {} has no assignee", task_iri))?;
                Ok(())
            }
        })
        .await?;

    let app_clone = app.clone();
    let iri = task_iri.clone();
    tokio::spawn(async move {
        if let Err(e) = task_manager::execute_task(&app_clone, &iri).await {
            crate::commands::log_backend("error", &format!(
                "[task_manager] execute_task failed for {}: {}", iri, e
            ));
        }
    });

    Ok(())
}
