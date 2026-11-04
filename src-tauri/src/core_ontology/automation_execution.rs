use crate::owl::Connection;
use crate::owl::{
    current_snapshot_tx, page_related_objects_by_datetime_as_of, page_relation_by_datetime_as_of,
    DateTimePageItem,
};
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
    objects_from_owner: bool,
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
    let result = if objects_from_owner {
        page_related_objects_by_datetime_as_of(
            conn,
            automation_iri,
            relation_predicate,
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
        )?
    } else {
        page_relation_by_datetime_as_of(
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
        )?
    };
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
        false,
    )?;
    Ok((items, total_count, next_cursor, has_more, snapshot_tx))
}

pub fn get_input_context(conn: &Connection, step_iri: &str) -> Result<Option<String>, String> {
    crate::owl::get_literal_property(conn, step_iri, "foundation:inputContext")
        .map_err(|error| error.to_string())
}

pub fn get_execution_output(conn: &Connection, step_iri: &str) -> Result<Vec<String>, String> {
    crate::owl::get_all_iri_properties(conn, step_iri, "foundation:executionOutput")
        .map_err(|error| error.to_string())
}

pub fn get_step_error(conn: &Connection, step_iri: &str) -> Result<Option<String>, String> {
    crate::owl::get_literal_property(conn, step_iri, "foundation:stepError")
        .map_err(|error| error.to_string())
}
