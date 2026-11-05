use crate::owl::{
    current_snapshot_tx, get_all_iri_properties_as_of, get_iri_property_as_of,
    get_literal_property_as_of, page_related_objects_by_datetime_as_of, Connection,
    DateTimePageItem, Object, Triple,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

const ACCEPTANCE_CRITERION: &str = "foundation:AcceptanceCriterion";
const VALIDATION: &str = "foundation:AcceptanceCriterionValidation";
const HAS_VALIDATION_EVIDENCE: &str = "foundation:hasValidationEvidence";
const HAS_STATUS: &str = "foundation:hasStatus";
const CREATED_AT: &str = "foundation:createdAt";
const MODIFIED_AT: &str = "foundation:modifiedAt";
const COMPLETED: &str = "foundation:Completed";
const REJECTED: &str = "foundation:Rejected";
const BLOCKED: &str = "foundation:Blocked";
const PAGE_SIZE_MAX: usize = 100;

static VALIDATION_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordCriterionValidation {
    pub criterion_iri: String,
    pub action: String,
    pub expected_result: String,
    pub observed_result: String,
    pub reason: Option<String>,
    pub status_iri: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedCriterionValidation {
    pub iri: String,
    pub transaction: i64,
    pub created_at: String,
    pub status_iri: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationCursor {
    pub created_at_julian_day: f64,
    pub iri: String,
    pub snapshot_tx: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCriterionValidations {
    pub criterion_iri: String,
    pub snapshot_tx: Option<i64>,
    pub cursor: Option<ValidationCursor>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionValidationItem {
    pub iri: String,
    pub created_at: String,
    pub action: Option<String>,
    pub expected_result: Option<String>,
    pub observed_result: Option<String>,
    pub reason: Option<String>,
    pub status_iri: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationConsistency {
    Consistent,
    MissingEvidence,
    Divergent,
    NotTerminal,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionValidationPage {
    pub items: Vec<CriterionValidationItem>,
    pub latest: Option<CriterionValidationItem>,
    pub current_status_iri: Option<String>,
    pub consistency: ValidationConsistency,
    pub next_cursor: Option<ValidationCursor>,
    pub has_more: bool,
    pub snapshot_tx: i64,
}

pub fn record_criterion_validation(
    conn: &mut Connection,
    request: RecordCriterionValidation,
) -> Result<RecordedCriterionValidation, String> {
    validate_request(&request)?;
    if !crate::owl::is_instance_of(conn, &request.criterion_iri, ACCEPTANCE_CRITERION) {
        return Err(format!(
            "{} is not an instance of {}",
            request.criterion_iri, ACCEPTANCE_CRITERION
        ));
    }
    crate::core_ontology::status::validate_allowed_status(conn, VALIDATION, &request.status_iri)
        .map_err(|error| error.to_string())?;
    crate::core_ontology::status::validate_allowed_status(
        conn,
        ACCEPTANCE_CRITERION,
        &request.status_iri,
    )
    .map_err(|error| error.to_string())?;

    let now = chrono::Utc::now();
    let created_at = now.to_rfc3339();
    let sequence = VALIDATION_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let validation_iri = format!(
        "foundation:AcceptanceCriterionValidation_{}_{}",
        now.timestamp_micros(),
        sequence
    );
    let label = format!("Validação de {} em {}", request.criterion_iri, created_at);
    let snapshot_tx = current_snapshot_tx(conn)?;
    let mut existing_evidence = get_all_iri_properties_as_of(
        conn,
        &request.criterion_iri,
        HAS_VALIDATION_EVIDENCE,
        snapshot_tx,
    )?;
    existing_evidence.push(validation_iri.clone());

    let mut triples = vec![
        Triple::new(
            &validation_iri,
            "rdf:type",
            Object::Iri(VALIDATION.to_string()),
        ),
        literal(&validation_iri, "rdfs:label", label),
        Triple::new(
            &validation_iri,
            CREATED_AT,
            Object::DateTime(created_at.clone()),
        ),
        Triple::new(
            &validation_iri,
            MODIFIED_AT,
            Object::DateTime(created_at.clone()),
        ),
        literal(
            &validation_iri,
            "foundation:validationAction",
            request.action,
        ),
        literal(
            &validation_iri,
            "foundation:validationExpectedResult",
            request.expected_result,
        ),
        literal(
            &validation_iri,
            "foundation:validationObservedResult",
            request.observed_result,
        ),
        Triple::new(
            &validation_iri,
            HAS_STATUS,
            Object::Iri(request.status_iri.clone()),
        ),
        Triple::new(
            &request.criterion_iri,
            HAS_STATUS,
            Object::Iri(request.status_iri.clone()),
        ),
        Triple::new(
            &request.criterion_iri,
            MODIFIED_AT,
            Object::DateTime(created_at.clone()),
        ),
    ];
    if let Some(reason) = request.reason.filter(|value| !value.trim().is_empty()) {
        triples.push(literal(
            &validation_iri,
            "foundation:validationReason",
            reason,
        ));
    }
    triples.extend(existing_evidence.into_iter().map(|iri| {
        Triple::new(
            &request.criterion_iri,
            HAS_VALIDATION_EVIDENCE,
            Object::Iri(iri),
        )
    }));

    let transaction = crate::owl::assert_raw_triples_with_tx(conn, &triples, "qa-evidence")
        .map_err(|error| error.to_string())?;

    Ok(RecordedCriterionValidation {
        iri: validation_iri,
        transaction,
        created_at,
        status_iri: request.status_iri,
    })
}

pub fn list_criterion_validations(
    conn: &Connection,
    request: ListCriterionValidations,
) -> Result<CriterionValidationPage, String> {
    if request.cursor.is_some() && request.snapshot_tx.is_none() {
        return Err("A continuation cursor requires the original snapshotTx".to_string());
    }
    let snapshot_tx = request.snapshot_tx.unwrap_or(current_snapshot_tx(conn)?);
    if request
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.snapshot_tx != snapshot_tx)
    {
        return Err("Validation cursor does not match the original snapshotTx".to_string());
    }
    if !crate::owl::is_instance_of(conn, &request.criterion_iri, ACCEPTANCE_CRITERION) {
        return Err(format!(
            "{} is not an instance of {}",
            request.criterion_iri, ACCEPTANCE_CRITERION
        ));
    }

    let page_size = request.limit.clamp(1, PAGE_SIZE_MAX);
    let after = request
        .cursor
        .as_ref()
        .map(|cursor| (cursor.created_at_julian_day, cursor.iri.as_str()));
    let page = page_related_objects_by_datetime_as_of(
        conn,
        &request.criterion_iri,
        HAS_VALIDATION_EVIDENCE,
        snapshot_tx,
        CREATED_AT,
        None,
        None,
        false,
        after,
        (page_size + 1) as i64,
        HAS_STATUS,
        MODIFIED_AT,
        COMPLETED,
        REJECTED,
    )?;

    let has_more = page.items.len() > page_size;
    let mut rows: Vec<DateTimePageItem> = page.items.into_iter().take(page_size).collect();
    let next_cursor = if has_more {
        rows.last().map(|row| ValidationCursor {
            created_at_julian_day: row.key_julian_day,
            iri: row.subject.clone(),
            snapshot_tx,
        })
    } else {
        None
    };
    let items = rows
        .drain(..)
        .map(|row| validation_item(conn, &row.subject, &row.key_value, snapshot_tx))
        .collect::<Result<Vec<_>, _>>()?;

    let latest_page = page_related_objects_by_datetime_as_of(
        conn,
        &request.criterion_iri,
        HAS_VALIDATION_EVIDENCE,
        snapshot_tx,
        CREATED_AT,
        None,
        None,
        false,
        None,
        1,
        HAS_STATUS,
        MODIFIED_AT,
        COMPLETED,
        REJECTED,
    )?;
    let latest = latest_page
        .items
        .first()
        .map(|row| validation_item(conn, &row.subject, &row.key_value, snapshot_tx))
        .transpose()?;
    let current_status_iri =
        get_iri_property_as_of(conn, &request.criterion_iri, HAS_STATUS, snapshot_tx)?;
    let consistency = consistency(current_status_iri.as_deref(), latest.as_ref());

    Ok(CriterionValidationPage {
        items,
        latest,
        current_status_iri,
        consistency,
        next_cursor,
        has_more,
        snapshot_tx,
    })
}

fn validation_item(
    conn: &Connection,
    validation_iri: &str,
    created_at: &str,
    snapshot_tx: i64,
) -> Result<CriterionValidationItem, String> {
    Ok(CriterionValidationItem {
        iri: validation_iri.to_string(),
        created_at: created_at.to_string(),
        action: get_literal_property_as_of(
            conn,
            validation_iri,
            "foundation:validationAction",
            snapshot_tx,
        )?,
        expected_result: get_literal_property_as_of(
            conn,
            validation_iri,
            "foundation:validationExpectedResult",
            snapshot_tx,
        )?,
        observed_result: get_literal_property_as_of(
            conn,
            validation_iri,
            "foundation:validationObservedResult",
            snapshot_tx,
        )?,
        reason: get_literal_property_as_of(
            conn,
            validation_iri,
            "foundation:validationReason",
            snapshot_tx,
        )?,
        status_iri: get_iri_property_as_of(conn, validation_iri, HAS_STATUS, snapshot_tx)?,
    })
}

fn consistency(
    current_status: Option<&str>,
    latest: Option<&CriterionValidationItem>,
) -> ValidationConsistency {
    let Some(current_status) = current_status else {
        return ValidationConsistency::Divergent;
    };
    match (
        is_terminal_status(current_status),
        latest.and_then(|item| item.status_iri.as_deref()),
    ) {
        (true, None) => ValidationConsistency::MissingEvidence,
        (false, None) => ValidationConsistency::NotTerminal,
        (true, Some(latest_status)) if latest_status == current_status => {
            ValidationConsistency::Consistent
        }
        (_, Some(_)) => ValidationConsistency::Divergent,
    }
}

fn is_terminal_status(status_iri: &str) -> bool {
    matches!(status_iri, COMPLETED | REJECTED | BLOCKED)
}

fn validate_request(request: &RecordCriterionValidation) -> Result<(), String> {
    for (field, value) in [
        ("criterionIri", request.criterion_iri.as_str()),
        ("action", request.action.as_str()),
        ("expectedResult", request.expected_result.as_str()),
        ("observedResult", request.observed_result.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{field} must not be empty"));
        }
    }
    if !is_terminal_status(&request.status_iri) {
        return Err("Validation status must be Completed, Rejected, or Blocked".to_string());
    }
    if matches!(request.status_iri.as_str(), REJECTED | BLOCKED)
        && request
            .reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
    {
        return Err("reason must not be empty for Rejected or Blocked validations".to_string());
    }
    Ok(())
}

fn literal(subject: &str, predicate: &str, value: String) -> Triple {
    Triple::new(
        subject,
        predicate,
        Object::Literal {
            value,
            datatype: Some("xsd:string".to_string()),
            language: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db};

    const TEST_CRITERION: &str = "foundation:AcceptanceCriterion_QaEvidenceTest";

    fn setup_validation_db() -> Connection {
        let mut conn = setup_test_db();
        let statuses = [COMPLETED, REJECTED, BLOCKED];
        let mut triples = vec![
            Triple::new(
                "foundation:AcceptanceCriterionValidation",
                "foundation:allowedStatus",
                Object::Iri(COMPLETED.to_string()),
            ),
            Triple::new(
                "foundation:AcceptanceCriterionValidation",
                "foundation:allowedStatus",
                Object::Iri(REJECTED.to_string()),
            ),
            Triple::new(
                "foundation:AcceptanceCriterionValidation",
                "foundation:allowedStatus",
                Object::Iri(BLOCKED.to_string()),
            ),
            Triple::new(
                TEST_CRITERION,
                "rdf:type",
                Object::Iri(ACCEPTANCE_CRITERION.to_string()),
            ),
        ];
        triples.extend(statuses.iter().map(|status| {
            Triple::new(
                ACCEPTANCE_CRITERION,
                "foundation:allowedStatus",
                Object::Iri((*status).to_string()),
            )
        }));
        store::assert_triples(&mut conn, &triples, "qa-evidence-test").unwrap();
        conn
    }

    fn request(status_iri: &str, observed_result: &str) -> RecordCriterionValidation {
        RecordCriterionValidation {
            criterion_iri: TEST_CRITERION.to_string(),
            action: "Execute validation fixture".to_string(),
            expected_result: "Expected fixture result".to_string(),
            observed_result: observed_result.to_string(),
            reason: None,
            status_iri: status_iri.to_string(),
        }
    }

    fn current_tx(conn: &Connection) -> i64 {
        current_snapshot_tx(conn).unwrap()
    }

    #[test]
    fn records_pass_fail_and_blocked_with_status_and_evidence_atomically() {
        let mut conn = setup_validation_db();
        for (status, reason) in [
            (COMPLETED, None),
            (REJECTED, Some("Observed a mismatch")),
            (BLOCKED, Some("Required service unavailable")),
        ] {
            let mut validation_request = request(status, "Observed result");
            validation_request.reason = reason.map(str::to_string);
            let recorded = record_criterion_validation(&mut conn, validation_request).unwrap();
            let evidence_status = crate::owl::get_iri_property_as_of(
                &conn,
                &recorded.iri,
                HAS_STATUS,
                recorded.transaction,
            )
            .unwrap();
            let criterion_status = crate::owl::get_iri_property_as_of(
                &conn,
                TEST_CRITERION,
                HAS_STATUS,
                recorded.transaction,
            )
            .unwrap();
            let links = crate::owl::get_all_iri_properties_as_of(
                &conn,
                TEST_CRITERION,
                HAS_VALIDATION_EVIDENCE,
                recorded.transaction,
            )
            .unwrap();
            assert_eq!(evidence_status.as_deref(), Some(status));
            assert_eq!(criterion_status.as_deref(), Some(status));
            assert!(links.contains(&recorded.iri));
        }
    }

    #[test]
    fn requires_reason_for_failed_or_blocked_validation() {
        let mut conn = setup_validation_db();
        for status in [REJECTED, BLOCKED] {
            for reason in [None, Some("  ".to_string())] {
                let before = current_tx(&conn);
                let mut validation_request = request(status, "Observed result");
                validation_request.reason = reason;
                assert!(record_criterion_validation(&mut conn, validation_request).is_err());
                assert_eq!(current_tx(&conn), before);
                assert!(crate::owl::get_all_iri_properties_as_of(
                    &conn,
                    TEST_CRITERION,
                    HAS_VALIDATION_EVIDENCE,
                    before,
                )
                .unwrap()
                .is_empty());
                assert!(crate::owl::get_iri_property_as_of(
                    &conn,
                    TEST_CRITERION,
                    HAS_STATUS,
                    before,
                )
                .unwrap()
                .is_none());
            }
        }
    }

    #[test]
    fn invalid_fields_status_and_noncriterion_leave_no_partial_writes() {
        let mut conn = setup_validation_db();
        let before = current_tx(&conn);
        let mut empty = request(COMPLETED, "passed");
        empty.expected_result = " \n".to_string();
        assert!(record_criterion_validation(&mut conn, empty).is_err());
        let mut invalid_status = request("foundation:Pending", "pending");
        assert!(record_criterion_validation(&mut conn, invalid_status.clone()).is_err());
        invalid_status.status_iri = COMPLETED.to_string();
        invalid_status.criterion_iri = "foundation:NotAnAcceptanceCriterion".to_string();
        assert!(record_criterion_validation(&mut conn, invalid_status).is_err());
        assert_eq!(current_tx(&conn), before);
        assert!(crate::owl::get_all_iri_properties_as_of(
            &conn,
            TEST_CRITERION,
            HAS_VALIDATION_EVIDENCE,
            before,
        )
        .unwrap()
        .is_empty());
        assert!(
            crate::owl::get_iri_property_as_of(&conn, TEST_CRITERION, HAS_STATUS, before)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn validation_and_criterion_status_share_one_transaction() {
        let mut conn = setup_validation_db();
        let result = record_criterion_validation(&mut conn, request(COMPLETED, "passed")).unwrap();
        let criterion_status =
            crate::owl::query_by_entity_predicate(&conn, TEST_CRITERION, HAS_STATUS)
                .unwrap()
                .triples
                .last()
                .unwrap()
                .tx;
        let evidence_status = crate::owl::query_by_entity_predicate(&conn, &result.iri, HAS_STATUS)
            .unwrap()
            .triples
            .last()
            .unwrap()
            .tx;
        assert_eq!(criterion_status, result.transaction);
        assert_eq!(evidence_status, result.transaction);
    }

    #[test]
    fn history_keeps_prior_records_and_returns_stable_pages() {
        let mut conn = setup_validation_db();
        let first = record_criterion_validation(&mut conn, request(COMPLETED, "passed")).unwrap();
        let mut rejected = request(REJECTED, "failed");
        rejected.reason = Some("Observed failure".to_string());
        let second = record_criterion_validation(&mut conn, rejected).unwrap();
        let page = list_criterion_validations(
            &conn,
            ListCriterionValidations {
                criterion_iri: TEST_CRITERION.to_string(),
                snapshot_tx: None,
                cursor: None,
                limit: 1,
            },
        )
        .unwrap();
        assert!(page.has_more);
        assert_eq!(page.items[0].iri, second.iri);
        let next = list_criterion_validations(
            &conn,
            ListCriterionValidations {
                criterion_iri: TEST_CRITERION.to_string(),
                snapshot_tx: Some(page.snapshot_tx),
                cursor: page.next_cursor,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(next.items[0].iri, first.iri);
        assert!(!next.has_more);
    }

    #[test]
    fn pagination_snapshot_excludes_writes_after_the_first_page() {
        let mut conn = setup_validation_db();
        let first = record_criterion_validation(&mut conn, request(COMPLETED, "first")).unwrap();
        let mut rejected = request(REJECTED, "second");
        rejected.reason = Some("Observed failure".to_string());
        let second = record_criterion_validation(&mut conn, rejected).unwrap();

        let first_page = list_criterion_validations(
            &conn,
            ListCriterionValidations {
                criterion_iri: TEST_CRITERION.to_string(),
                snapshot_tx: None,
                cursor: None,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(first_page.items[0].iri, second.iri);
        assert!(first_page.has_more);

        let mut blocked = request(BLOCKED, "written later");
        blocked.reason = Some("Validation was blocked".to_string());
        let later = record_criterion_validation(&mut conn, blocked).unwrap();
        let second_page = list_criterion_validations(
            &conn,
            ListCriterionValidations {
                criterion_iri: TEST_CRITERION.to_string(),
                snapshot_tx: Some(first_page.snapshot_tx),
                cursor: first_page.next_cursor,
                limit: 1,
            },
        )
        .unwrap();

        assert_eq!(second_page.items.len(), 1);
        assert_eq!(second_page.items[0].iri, first.iri);
        assert!(!second_page.has_more);
        assert_ne!(second_page.items[0].iri, later.iri);

        let fresh_read = list_criterion_validations(
            &conn,
            ListCriterionValidations {
                criterion_iri: TEST_CRITERION.to_string(),
                snapshot_tx: None,
                cursor: None,
                limit: 3,
            },
        )
        .unwrap();
        assert_eq!(fresh_read.items.len(), 3);
        assert_eq!(fresh_read.items[0].iri, later.iri);
        assert_eq!(fresh_read.items[1].iri, second.iri);
        assert_eq!(fresh_read.items[2].iri, first.iri);
        assert_eq!(fresh_read.latest.as_ref().unwrap().iri, later.iri);
    }

    #[test]
    fn consistency_distinguishes_missing_divergent_and_nonterminal_statuses() {
        assert!(matches!(
            consistency(Some(COMPLETED), None),
            ValidationConsistency::MissingEvidence
        ));
        let latest = CriterionValidationItem {
            iri: "foundation:AcceptanceCriterionValidation_Test".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            action: None,
            expected_result: None,
            observed_result: None,
            reason: None,
            status_iri: Some(REJECTED.to_string()),
        };
        assert!(matches!(
            consistency(Some(COMPLETED), Some(&latest)),
            ValidationConsistency::Divergent
        ));
        assert!(matches!(
            consistency(Some("foundation:Pending"), Some(&latest)),
            ValidationConsistency::Divergent
        ));
        assert!(matches!(
            consistency(Some("foundation:Pending"), None),
            ValidationConsistency::NotTerminal
        ));
    }

    #[test]
    fn rejects_empty_test_data_and_nonterminal_results() {
        let mut conn = setup_validation_db();
        let mut empty = request(COMPLETED, "passed");
        empty.action.clear();
        assert!(record_criterion_validation(&mut conn, empty).is_err());
        assert!(
            record_criterion_validation(&mut conn, request("foundation:Pending", "pending"))
                .is_err()
        );
    }
}
