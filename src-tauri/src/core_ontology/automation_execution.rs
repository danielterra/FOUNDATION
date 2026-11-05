use crate::owl::Connection;
use crate::owl::{current_snapshot_tx, page_relation_by_datetime_as_of, DateTimePageItem};
use serde::{Deserialize, Serialize};

const COMPLETED: &str = "foundation:Completed";
const FAILED: &str = "foundation:Status_1772993026091";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionHistoryCursor {
    pub order_key: f64,
    pub iri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionHistoryItem {
    pub iri: String,
    pub started_at: String,
    pub status_iri: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionHistoryKpis {
    pub total_started: i64,
    pub terminal_count: i64,
    pub min_duration_ms: Option<f64>,
    pub average_duration_ms: Option<f64>,
    pub max_duration_ms: Option<f64>,
    pub error_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionHistoryPage {
    pub items: Vec<ExecutionHistoryItem>,
    pub total_count: i64,
    pub loaded_count: usize,
    pub next_cursor: Option<ExecutionHistoryCursor>,
    pub has_more: bool,
    pub snapshot_tx: i64,
    pub kpis: ExecutionHistoryKpis,
}

fn page(
    conn: &Connection,
    automation_iri: &str,
    range_start: Option<&str>,
    range_end: Option<&str>,
    snapshot_tx: Option<i64>,
    after: Option<ExecutionHistoryCursor>,
    limit: usize,
    relation_predicate: &str,
    datetime_predicate: &str,
    end_predicate: &str,
    ascending: bool,
) -> Result<
    (
        Vec<ExecutionHistoryItem>,
        i64,
        Option<ExecutionHistoryCursor>,
        bool,
        i64,
        crate::owl::DateTimePageStats,
    ),
    String,
> {
    if after.is_some() && snapshot_tx.is_none() {
        return Err("A continuation cursor requires the original snapshotTx".to_string());
    }
    let snapshot = match snapshot_tx {
        Some(tx) => tx,
        None => current_snapshot_tx(conn)?,
    };
    let page_size = limit.clamp(1, 100);
    let cursor = after.as_ref().map(|c| (c.order_key, c.iri.as_str()));
    let result = page_relation_by_datetime_as_of(
        conn,
        relation_predicate,
        automation_iri,
        snapshot,
        datetime_predicate,
        range_start,
        range_end,
        ascending,
        cursor,
        page_size as i64 + 1,
        "foundation:hasStatus",
        end_predicate,
        COMPLETED,
        FAILED,
    )?;
    let has_more = result.items.len() > page_size;
    let mut rows: Vec<DateTimePageItem> = result.items.into_iter().take(page_size).collect();
    let next_cursor = if has_more {
        rows.last().map(|row| ExecutionHistoryCursor {
            order_key: row.key_julian_day,
            iri: row.subject.clone(),
        })
    } else {
        None
    };
    let items = rows
        .drain(..)
        .map(|row| ExecutionHistoryItem {
            iri: row.subject,
            started_at: row.key_value,
            status_iri: row.status_iri,
            finished_at: row.end_value,
        })
        .collect();
    Ok((
        items,
        result.stats.total_count,
        next_cursor,
        has_more,
        snapshot,
        result.stats,
    ))
}

pub fn list_workflow_executions(
    conn: &Connection,
    automation_iri: &str,
    range_start: Option<&str>,
    range_end: Option<&str>,
    snapshot_tx: Option<i64>,
    after: Option<ExecutionHistoryCursor>,
    limit: usize,
) -> Result<ExecutionHistoryPage, String> {
    let (items, total_count, next_cursor, has_more, snapshot_tx, stats) = page(
        conn,
        automation_iri,
        range_start,
        range_end,
        snapshot_tx,
        after,
        limit,
        "foundation:executesProcess",
        "foundation:hasStartTime",
        "foundation:hasEndTime",
        false,
    )?;
    let error_rate =
        (stats.terminal_count > 0).then(|| stats.failed_count as f64 / stats.terminal_count as f64);
    Ok(ExecutionHistoryPage {
        loaded_count: items.len(),
        items,
        total_count,
        next_cursor,
        has_more,
        snapshot_tx,
        kpis: ExecutionHistoryKpis {
            total_started: total_count,
            terminal_count: stats.terminal_count,
            min_duration_ms: stats.min_duration_ms,
            average_duration_ms: stats.average_duration_ms,
            max_duration_ms: stats.max_duration_ms,
            error_rate,
        },
    })
}

pub fn list_step_executions(
    conn: &Connection,
    execution_iri: &str,
    snapshot_tx: Option<i64>,
    after: Option<ExecutionHistoryCursor>,
    limit: usize,
) -> Result<
    (
        Vec<ExecutionHistoryItem>,
        i64,
        Option<ExecutionHistoryCursor>,
        bool,
        i64,
    ),
    String,
> {
    let (items, total_count, next_cursor, has_more, snapshot_tx, _) = page(
        conn,
        execution_iri,
        None,
        None,
        snapshot_tx,
        after,
        limit,
        "foundation:belongsToExecution",
        "foundation:stepStartedAt",
        "foundation:stepFinishedAt",
        true,
    )?;
    Ok((items, total_count, next_cursor, has_more, snapshot_tx))
}

pub fn get_input_context(
    conn: &Connection,
    step_iri: &str,
    snapshot_tx: i64,
) -> Result<Option<String>, String> {
    crate::owl::get_literal_property_as_of(conn, step_iri, "foundation:inputContext", snapshot_tx)
}

pub fn get_execution_output(
    conn: &Connection,
    step_iri: &str,
    snapshot_tx: i64,
) -> Result<Vec<String>, String> {
    crate::owl::get_all_iri_properties_as_of(
        conn,
        step_iri,
        "foundation:executionOutput",
        snapshot_tx,
    )
}

pub fn get_step_error(
    conn: &Connection,
    step_iri: &str,
    snapshot_tx: i64,
) -> Result<Option<String>, String> {
    crate::owl::get_literal_property_as_of(conn, step_iri, "foundation:stepError", snapshot_tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Object, Triple};
    use chrono::{DateTime, SecondsFormat, Utc};

    const AUTOMATION: &str = "foundation:Automation_history_test";
    const IN_PROGRESS: &str = "foundation:InProgress";
    const CANCELLED: &str = "foundation:Status_1772570972069";

    fn iso_time(timestamp_ms: i64) -> String {
        DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
            .unwrap()
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    fn insert_execution(
        conn: &mut Connection,
        iri: &str,
        started_at: &str,
        finished_at: Option<&str>,
        status_iri: &str,
    ) {
        let mut triples = vec![
            Triple::new(
                iri,
                "foundation:executesProcess",
                Object::Iri(AUTOMATION.to_string()),
            ),
            Triple::new(
                iri,
                "foundation:hasStartTime",
                Object::DateTime(started_at.to_string()),
            ),
            Triple::new(
                iri,
                "foundation:hasStatus",
                Object::Iri(status_iri.to_string()),
            ),
        ];
        if let Some(finished_at) = finished_at {
            triples.push(Triple::new(
                iri,
                "foundation:hasEndTime",
                Object::DateTime(finished_at.to_string()),
            ));
        }
        store::assert_triples(conn, &triples, "automation-history-test").unwrap();
    }

    #[test]
    fn history_pages_are_stable_across_writes_and_order_ties_by_iri() {
        let mut conn = setup_test_db();
        let same_start = iso_time(1_798_761_600_000);
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_Z",
            &same_start,
            None,
            IN_PROGRESS,
        );
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_A",
            &same_start,
            None,
            IN_PROGRESS,
        );
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_Older",
            &iso_time(1_798_761_599_000),
            None,
            IN_PROGRESS,
        );

        let first_page =
            list_workflow_executions(&conn, AUTOMATION, None, None, None, None, 1).unwrap();

        assert_eq!(first_page.items[0].iri, "foundation:WorkflowExecution_A");
        assert!(first_page.has_more);
        assert_eq!(first_page.total_count, 3);
        let cursor = first_page.next_cursor.clone().unwrap();

        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_Later",
            &iso_time(1_798_761_601_000),
            None,
            IN_PROGRESS,
        );
        let moved_start = iso_time(1_798_761_602_000);
        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "foundation:WorkflowExecution_Older",
                "foundation:hasStartTime",
                Object::DateTime(moved_start),
            )],
            "automation-history-test",
        )
        .unwrap();

        let second_page = list_workflow_executions(
            &conn,
            AUTOMATION,
            None,
            None,
            Some(first_page.snapshot_tx),
            Some(cursor),
            1,
        )
        .unwrap();
        assert_eq!(second_page.items[0].iri, "foundation:WorkflowExecution_Z");
        assert_eq!(second_page.total_count, 3);
        assert_eq!(second_page.snapshot_tx, first_page.snapshot_tx);
    }

    #[test]
    fn date_filter_and_metrics_share_the_selected_period() {
        let mut conn = setup_test_db();
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_Success",
            "2026-10-01T10:00:00.000Z",
            Some("2026-10-01T10:00:01.000Z"),
            COMPLETED,
        );
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_Failed",
            "2026-10-02T10:00:00.000Z",
            Some("2026-10-02T10:00:03.000Z"),
            FAILED,
        );
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_Running",
            "2026-10-02T12:00:00.000Z",
            None,
            IN_PROGRESS,
        );
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_Cancelled",
            "2026-10-02T13:00:00.000Z",
            Some("2026-10-02T13:00:01.000Z"),
            CANCELLED,
        );
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_OutsideRange",
            "2026-10-03T00:00:00.000Z",
            Some("2026-10-03T00:00:10.000Z"),
            COMPLETED,
        );

        let page = list_workflow_executions(
            &conn,
            AUTOMATION,
            Some("2026-10-01T00:00:00.000Z"),
            Some("2026-10-03T00:00:00.000Z"),
            None,
            None,
            50,
        )
        .unwrap();

        assert_eq!(page.total_count, 4);
        assert_eq!(page.kpis.total_started, 4);
        assert_eq!(page.kpis.terminal_count, 2);
        assert_eq!(page.items.len(), 4);
        assert_eq!(page.items[0].iri, "foundation:WorkflowExecution_Cancelled");
        assert_eq!(page.items[1].iri, "foundation:WorkflowExecution_Running");
        assert_eq!(page.items[2].iri, "foundation:WorkflowExecution_Failed");
        assert_eq!(page.items[3].iri, "foundation:WorkflowExecution_Success");
        assert!((page.kpis.min_duration_ms.unwrap() - 1_000.0).abs() < 1.0);
        assert!((page.kpis.max_duration_ms.unwrap() - 3_000.0).abs() < 1.0);
        assert!((page.kpis.average_duration_ms.unwrap() - 2_000.0).abs() < 1.0);
        assert_eq!(page.kpis.error_rate, Some(1.0 / 2.0));
    }

    #[test]
    fn later_status_changes_do_not_rewrite_snapshot_metrics() {
        let mut conn = setup_test_db();
        insert_execution(
            &mut conn,
            "foundation:WorkflowExecution_StableFailure",
            "2026-10-02T10:00:00.000Z",
            Some("2026-10-02T10:00:03.000Z"),
            FAILED,
        );
        let snapshot_page =
            list_workflow_executions(&conn, AUTOMATION, None, None, None, None, 10).unwrap();
        assert_eq!(snapshot_page.kpis.terminal_count, 1);
        assert_eq!(snapshot_page.kpis.error_rate, Some(1.0));

        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "foundation:WorkflowExecution_StableFailure",
                "foundation:hasStatus",
                Object::Iri(COMPLETED.to_string()),
            )],
            "automation-history-test",
        )
        .unwrap();

        let same_snapshot = list_workflow_executions(
            &conn,
            AUTOMATION,
            None,
            None,
            Some(snapshot_page.snapshot_tx),
            None,
            10,
        )
        .unwrap();
        assert_eq!(same_snapshot.items[0].status_iri.as_deref(), Some(FAILED));
        assert_eq!(same_snapshot.kpis.terminal_count, 1);
        assert_eq!(same_snapshot.kpis.error_rate, Some(1.0));
    }

    #[test]
    fn empty_history_returns_zero_count_and_no_duration_metrics() {
        let conn = setup_test_db();
        let page = list_workflow_executions(&conn, AUTOMATION, None, None, None, None, 50).unwrap();

        assert!(page.items.is_empty());
        assert_eq!(page.total_count, 0);
        assert_eq!(page.loaded_count, 0);
        assert!(!page.has_more);
        assert!(page.next_cursor.is_none());
        assert_eq!(page.kpis.total_started, 0);
        assert_eq!(page.kpis.min_duration_ms, None);
        assert_eq!(page.kpis.average_duration_ms, None);
        assert_eq!(page.kpis.max_duration_ms, None);
        assert_eq!(page.kpis.error_rate, None);
    }

    #[test]
    fn step_input_and_output_are_read_from_the_pinned_snapshot() {
        let mut conn = setup_test_db();
        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "foundation:StepExecution_Snapshot",
                    "foundation:inputContext",
                    Object::Literal {
                        value: "input at snapshot".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(
                    "foundation:StepExecution_Snapshot",
                    "foundation:executionOutput",
                    Object::Iri("foundation:Result_A".to_string()),
                ),
                Triple::new(
                    "foundation:StepExecution_Snapshot",
                    "foundation:executionOutput",
                    Object::Iri("foundation:Result_B".to_string()),
                ),
            ],
            "automation-history-test",
        )
        .unwrap();
        let snapshot_tx = current_snapshot_tx(&conn).unwrap();

        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "foundation:StepExecution_Snapshot",
                    "foundation:inputContext",
                    Object::Literal {
                        value: "later input".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(
                    "foundation:StepExecution_Snapshot",
                    "foundation:executionOutput",
                    Object::Iri("foundation:Result_C".to_string()),
                ),
            ],
            "automation-history-test",
        )
        .unwrap();

        assert_eq!(
            get_input_context(&conn, "foundation:StepExecution_Snapshot", snapshot_tx)
                .unwrap()
                .as_deref(),
            Some("input at snapshot")
        );
        assert_eq!(
            get_execution_output(&conn, "foundation:StepExecution_Snapshot", snapshot_tx).unwrap(),
            vec![
                "foundation:Result_A".to_string(),
                "foundation:Result_B".to_string()
            ]
        );
    }

    #[test]
    fn step_pages_keep_snapshot_order_and_limit_after_later_writes() {
        let mut conn = setup_test_db();
        let same_start = "2026-10-02T10:00:00.000Z";
        for step_iri in ["foundation:StepExecution_A", "foundation:StepExecution_B"] {
            store::assert_triples(
                &mut conn,
                &[
                    Triple::new(
                        step_iri,
                        "foundation:belongsToExecution",
                        Object::Iri("foundation:WorkflowExecution_Snapshot".to_string()),
                    ),
                    Triple::new(
                        step_iri,
                        "foundation:stepStartedAt",
                        Object::DateTime(same_start.to_string()),
                    ),
                    Triple::new(
                        step_iri,
                        "foundation:hasStatus",
                        Object::Iri(IN_PROGRESS.to_string()),
                    ),
                ],
                "automation-history-test",
            )
            .unwrap();
        }

        let (first_page, total_count, cursor, has_more, snapshot_tx) = list_step_executions(
            &conn,
            "foundation:WorkflowExecution_Snapshot",
            None,
            None,
            1,
        )
        .unwrap();
        assert_eq!(first_page.len(), 1);
        assert_eq!(first_page[0].iri, "foundation:StepExecution_A");
        assert_eq!(total_count, 2);
        assert!(has_more);

        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "foundation:StepExecution_C",
                    "foundation:belongsToExecution",
                    Object::Iri("foundation:WorkflowExecution_Snapshot".to_string()),
                ),
                Triple::new(
                    "foundation:StepExecution_C",
                    "foundation:stepStartedAt",
                    Object::DateTime("2026-10-02T09:00:00.000Z".to_string()),
                ),
            ],
            "automation-history-test",
        )
        .unwrap();

        let (second_page, second_total, next_cursor, second_has_more, second_snapshot_tx) =
            list_step_executions(
                &conn,
                "foundation:WorkflowExecution_Snapshot",
                Some(snapshot_tx),
                cursor,
                1,
            )
            .unwrap();
        assert_eq!(second_page.len(), 1);
        assert_eq!(second_page[0].iri, "foundation:StepExecution_B");
        assert_eq!(second_total, 2);
        assert!(next_cursor.is_none());
        assert!(!second_has_more);
        assert_eq!(second_snapshot_tx, snapshot_tx);
    }
}
