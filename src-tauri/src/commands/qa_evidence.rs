use crate::core_ontology::qa_evidence::{
    self, CriterionValidationPage, ListCriterionValidations, RecordCriterionValidation,
    RecordedCriterionValidation,
};
use crate::owl::DbExecutor;
use tauri::State;

#[tauri::command]
pub async fn inspector__record_criterion_validation(
    request: RecordCriterionValidation,
    executor: State<'_, DbExecutor>,
) -> Result<RecordedCriterionValidation, String> {
    let response = executor
        .write(move |conn| {
            let result = qa_evidence::record_criterion_validation(conn, request)?;
            serde_json::to_string(&result).map_err(|error| error.to_string())
        })
        .await?;
    serde_json::from_str(&response).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn inspector__list_criterion_validations(
    request: ListCriterionValidations,
    executor: State<'_, DbExecutor>,
) -> Result<CriterionValidationPage, String> {
    executor
        .read(move |conn| qa_evidence::list_criterion_validations(conn, request))
        .await
}
