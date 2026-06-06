use tauri::State;

use crate::eavto::DbExecutor;
use crate::eavto::query::PropertyFilter;
use crate::owl::Individual;

const NOTIFICATION_CLASS: &str = "foundation:AINotification";
const HAS_STATUS: &str = "foundation:hasStatus";
const STATUS_PENDING: &str = "foundation:Pending";

#[tauri::command]
#[allow(non_snake_case)]
pub async fn notification__count_pending(
    executor: State<'_, DbExecutor>,
) -> Result<usize, String> {
    executor.read(move |conn| {
        let filters = [PropertyFilter::Compare(HAS_STATUS, STATUS_PENDING, "=")];
        let (_, total) = Individual::find_by_class_and_properties_with_options(
            conn, NOTIFICATION_CLASS, &filters, false, 0, 0, None,
        ).map_err(|e| e.to_string())?;
        Ok(total)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn notification__list_iris(
    status_iri: Option<String>,
    limit: Option<usize>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<String>, String> {
    executor.read(move |conn| {
        let limit = limit.unwrap_or(usize::MAX);
        let filters: Vec<PropertyFilter> = match status_iri.as_deref() {
            Some(iri) if !iri.is_empty() => vec![PropertyFilter::Compare(HAS_STATUS, iri, "=")],
            _ => vec![],
        };
        let (iris, _) = Individual::find_by_class_and_properties_with_options(
            conn, NOTIFICATION_CLASS, &filters, false, limit, 0, None,
        ).map_err(|e| e.to_string())?;
        Ok(iris)
    }).await
}
