use tauri::{AppHandle, Manager};

use crate::owl::{get_iri_property, get_literal_property, replace_all_property_literals, DbExecutor};

use super::executor::{ExecutionContext, interpolate_with_db};

type Result<T> = std::result::Result<T, String>;

/// Executes an automation_TemplateTask node: reads foundation:templateBody, interpolates
/// {{self.prop_iri}} placeholders against the execution control instance, and writes the
/// resolved string to the control instance's outputProperty.
///
/// outputProperty is required. Without it the task logs a clear warning and returns Ok — this
/// mirrors the behaviour of RequestTask/AgentTask (no write if not configured) and avoids
/// orphaned properties on the ontology.
pub async fn execute_template_task(
    app: &AppHandle,
    node_iri: &str,
    ctx: &ExecutionContext,
) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let (template_body, output_prop_iri) = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                let body = get_literal_property(conn, &node_iri, "foundation:templateBody")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                let output_prop = get_iri_property(conn, &node_iri, "foundation:outputProperty")
                    .map_err(|e| e.to_string())?;
                Ok((body, output_prop))
            }
        })
        .await?;

    let resolved = interpolate_with_db(&template_body, ctx, &executor).await;

    let Some(output_prop) = output_prop_iri else {
        crate::commands::log_backend(
            "warn",
            &format!(
                "[template_task] node {} has no outputProperty configured — result discarded",
                node_iri
            ),
        );
        return Ok(resolved);
    };

    let Some(control_iri) = ctx.get("self").cloned() else {
        crate::commands::log_backend(
            "warn",
            &format!(
                "[template_task] node {} — ctx[\"self\"] missing, cannot write to outputProperty",
                node_iri
            ),
        );
        return Ok(resolved);
    };

    let resolved_clone = resolved.clone();
    executor
        .write(move |conn| {
            replace_all_property_literals(
                conn,
                &control_iri,
                &output_prop,
                &[resolved_clone.as_str()],
                "process_automation",
            )
            .map_err(|e| e.to_string())?;
            Ok(String::new())
        })
        .await
        .unwrap_or_else(|e| {
            crate::commands::log_backend(
                "error",
                &format!(
                    "[template_task] node {} — failed to write result to outputProperty: {}",
                    node_iri, e
                ),
            );
            String::new()
        });

    Ok(resolved)
}
