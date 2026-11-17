use crate::owl::{self, Connection, PropertyValueCursor};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_PROPERTIES_PER_SNAPSHOT: usize = 100;
const MAX_PROPERTY_PAGE_SIZE: usize = 100;
const MAX_PROJECTED_PROPERTIES_PER_REFERENCE: usize = 25;
const MAX_RECORD_SOURCES: usize = 20;
const MAX_RECORD_FIELDS_PER_SOURCE: usize = 25;
const MAX_RECORD_FILTERS: usize = 25;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsSource {
    pub source: String,
    pub alias: String,
    pub property_iri: String,
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub membership_watch: Option<EntityRecordsMembershipWatch>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsMembershipWatch {
    pub class_iri: String,
    pub predicate: String,
    pub object_binding: EntityRecordsMembershipBinding,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsMembershipBinding {
    pub source: String,
    pub property_iri: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsFilter {
    pub column_id: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsSort {
    pub column_id: String,
    pub direction: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsPageRequest {
    pub entity_iri: String,
    pub snapshot_tx: Option<i64>,
    pub sources: Vec<EntityRecordsSource>,
    #[serde(default)]
    pub filters: Vec<EntityRecordsFilter>,
    pub sort: EntityRecordsSort,
    pub page_size: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsPage {
    pub entity_iri: String,
    pub snapshot_tx: i64,
    pub sources: Vec<EntityRecordsSourceInfo>,
    pub rows: Vec<EntityRecordRow>,
    pub page_info: EntityRecordsPageInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsSourceInfo {
    pub alias: String,
    pub property_iri: String,
    pub source_class_iri: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordRow {
    pub entity_iri: String,
    pub source_alias: String,
    pub values: BTreeMap<String, Vec<SnapshotPropertyValue>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecordsPageInfo {
    pub returned_count: usize,
    pub total_count: usize,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntityRecordsCursor {
    entity_iri: String,
    snapshot_tx: i64,
    criteria_hash: String,
    next_offset: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntityRecordsCriteria<'a> {
    entity_iri: &'a str,
    sources: &'a [EntityRecordsSource],
    filters: &'a [EntityRecordsFilter],
    sort: &'a EntityRecordsSort,
    page_size: usize,
}

/// Reads all configured record sources at one immutable revision, applies filters and
/// a stable global ordering, then returns the requested page. The cost is proportional
/// to the full matching source collections because filtering and sorting precede paging.
pub fn get_entity_records_page(
    conn: &Connection,
    request: &EntityRecordsPageRequest,
) -> Result<EntityRecordsPage, String> {
    let page_size = request.page_size.unwrap_or(20);
    validate_records_request(request, page_size)?;

    let criteria = EntityRecordsCriteria {
        entity_iri: &request.entity_iri,
        sources: &request.sources,
        filters: &request.filters,
        sort: &request.sort,
        page_size,
    };
    let criteria_json = serde_json::to_vec(&criteria).map_err(|error| error.to_string())?;
    let criteria_hash = Sha256::digest(criteria_json)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let decoded_cursor = request
        .cursor
        .as_deref()
        .map(decode_records_cursor)
        .transpose()?;
    let snapshot_tx = match (request.snapshot_tx, decoded_cursor.as_ref()) {
        (Some(snapshot_tx), Some(cursor)) if snapshot_tx != cursor.snapshot_tx => {
            return Err("Records cursor does not match the requested snapshot".to_string())
        }
        (Some(snapshot_tx), _) => snapshot_tx,
        (None, Some(cursor)) => cursor.snapshot_tx,
        (None, None) => owl::current_snapshot_tx(conn)?,
    };
    let current_snapshot_tx = owl::current_snapshot_tx(conn)?;
    if snapshot_tx < 0 || snapshot_tx > current_snapshot_tx {
        return Err(format!("Invalid records snapshot revision: {snapshot_tx}"));
    }

    let offset = match decoded_cursor {
        Some(cursor) => {
            if cursor.entity_iri != request.entity_iri
                || cursor.snapshot_tx != snapshot_tx
                || cursor.criteria_hash != criteria_hash
            {
                return Err(
                    "Records cursor does not match the requested entity, sources, filters, sort, page size, and snapshot"
                        .to_string(),
                );
            }
            cursor.next_offset
        }
        None => 0,
    };

    let mut source_info = Vec::with_capacity(request.sources.len());
    let mut rows = Vec::new();
    for source in &request.sources {
        let source_class_iri = owl::get_all_iri_properties_as_of(
            conn,
            &source.property_iri,
            "rdfs:range",
            snapshot_tx,
        )?
        .into_iter()
        .next();
        source_info.push(EntityRecordsSourceInfo {
            alias: source.alias.clone(),
            property_iri: source.property_iri.clone(),
            source_class_iri,
        });

        let mut source_cursor = None;
        loop {
            let page = owl::get_property_values_page(
                conn,
                &request.entity_iri,
                &source.property_iri,
                MAX_PROPERTY_PAGE_SIZE,
                snapshot_tx,
                source_cursor.as_ref(),
            )?;
            for triple in page.items {
                let entity_iri = triple
                    .object
                    .as_iri()
                    .ok_or_else(|| {
                        format!(
                            "Records source '{}' contains a non-entity value",
                            source.alias
                        )
                    })?
                    .to_string();
                let mut values = BTreeMap::new();
                for (column_id, property_iri) in &source.fields {
                    values.insert(
                        column_id.clone(),
                        read_all_property_values(conn, &entity_iri, property_iri, snapshot_tx)?,
                    );
                }
                let row = EntityRecordRow {
                    entity_iri,
                    source_alias: source.alias.clone(),
                    values,
                };
                if records_row_matches_filters(&row, &request.filters)? {
                    rows.push(row);
                }
            }
            source_cursor = page.next_cursor;
            if source_cursor.is_none() {
                break;
            }
        }
    }

    rows.sort_by(|left, right| {
        let left_value = left
            .values
            .get(&request.sort.column_id)
            .and_then(|values| values.first());
        let right_value = right
            .values
            .get(&request.sort.column_id)
            .and_then(|values| values.first());
        let comparison = compare_optional_snapshot_values(left_value, right_value);
        let comparison = if request.sort.direction == "desc" {
            comparison.reverse()
        } else {
            comparison
        };
        comparison
            .then_with(|| left.source_alias.cmp(&right.source_alias))
            .then_with(|| left.entity_iri.cmp(&right.entity_iri))
    });

    let total_count = rows.len();
    if offset > total_count {
        return Err("Records cursor offset exceeds the current snapshot result count".to_string());
    }
    let end = offset.saturating_add(page_size).min(total_count);
    let page_rows = rows
        .into_iter()
        .skip(offset)
        .take(end - offset)
        .collect::<Vec<_>>();
    let has_more = end < total_count;
    let next_cursor = if has_more {
        Some(encode_records_cursor(&EntityRecordsCursor {
            entity_iri: request.entity_iri.clone(),
            snapshot_tx,
            criteria_hash,
            next_offset: end,
        })?)
    } else {
        None
    };

    Ok(EntityRecordsPage {
        entity_iri: request.entity_iri.clone(),
        snapshot_tx,
        sources: source_info,
        rows: page_rows.clone(),
        page_info: EntityRecordsPageInfo {
            returned_count: page_rows.len(),
            total_count,
            has_more,
            next_cursor,
        },
    })
}

fn validate_records_request(
    request: &EntityRecordsPageRequest,
    page_size: usize,
) -> Result<(), String> {
    if request.entity_iri.trim().is_empty() {
        return Err("Records owner entity IRI cannot be empty".to_string());
    }
    if request.sources.is_empty() || request.sources.len() > MAX_RECORD_SOURCES {
        return Err(format!(
            "Records source count must be between 1 and {MAX_RECORD_SOURCES}"
        ));
    }
    if page_size == 0 || page_size > MAX_PROPERTY_PAGE_SIZE {
        return Err(format!(
            "Records page size must be between 1 and {MAX_PROPERTY_PAGE_SIZE}"
        ));
    }
    if request.filters.len() > MAX_RECORD_FILTERS {
        return Err(format!(
            "A records request supports at most {MAX_RECORD_FILTERS} filters"
        ));
    }
    if request.sort.column_id.trim().is_empty()
        || !matches!(request.sort.direction.as_str(), "asc" | "desc")
    {
        return Err("Records sort requires a column and direction 'asc' or 'desc'".to_string());
    }
    let mut aliases = HashSet::with_capacity(request.sources.len());
    let mut all_columns = HashSet::new();
    for source in &request.sources {
        if source.source != "entity" {
            return Err(format!(
                "Unsupported records source '{}' for alias '{}'",
                source.source, source.alias
            ));
        }
        if let Some(watch) = &source.membership_watch {
            if watch.class_iri.trim().is_empty()
                || watch.predicate.trim().is_empty()
                || watch.object_binding.source != "entity"
                || watch.object_binding.property_iri.trim().is_empty()
            {
                return Err(format!(
                    "Records source '{}' has an invalid membership watch",
                    source.alias
                ));
            }
        }
        if source.alias.trim().is_empty() || source.property_iri.trim().is_empty() {
            return Err("Records source alias and property IRI cannot be empty".to_string());
        }
        if !aliases.insert(source.alias.as_str()) {
            return Err(format!("Duplicate records source alias: {}", source.alias));
        }
        if source.fields.is_empty() || source.fields.len() > MAX_RECORD_FIELDS_PER_SOURCE {
            return Err(format!(
                "Records source '{}' must project between 1 and {MAX_RECORD_FIELDS_PER_SOURCE} fields",
                source.alias
            ));
        }
        for (column_id, property_iri) in &source.fields {
            if column_id.trim().is_empty() || property_iri.trim().is_empty() {
                return Err(format!(
                    "Records source '{}' contains an empty field ID or property IRI",
                    source.alias
                ));
            }
            all_columns.insert(column_id.as_str());
        }
    }
    if !all_columns.contains(request.sort.column_id.as_str()) {
        return Err(format!(
            "Records sort column '{}' is not projected by any source",
            request.sort.column_id
        ));
    }
    for filter in &request.filters {
        if !all_columns.contains(filter.column_id.as_str()) {
            return Err(format!(
                "Records filter column '{}' is not projected by any source",
                filter.column_id
            ));
        }
        if !matches!(
            filter.operator.as_str(),
            "equals" | "contains" | "gt" | "gte" | "lt" | "lte"
        ) {
            return Err(format!(
                "Unsupported records filter operator: {}",
                filter.operator
            ));
        }
    }
    Ok(())
}

fn decode_records_cursor(value: &str) -> Result<EntityRecordsCursor, String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| "Invalid records cursor encoding".to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "Invalid records cursor payload".to_string())
}

fn encode_records_cursor(cursor: &EntityRecordsCursor) -> Result<String, String> {
    let bytes = serde_json::to_vec(cursor).map_err(|error| error.to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn read_all_property_values(
    conn: &Connection,
    entity_iri: &str,
    property_iri: &str,
    snapshot_tx: i64,
) -> Result<Vec<SnapshotPropertyValue>, String> {
    let mut values = Vec::new();
    let mut cursor = None;
    loop {
        let page = get_property_page_with_projection(
            conn,
            entity_iri,
            property_iri,
            MAX_PROPERTY_PAGE_SIZE,
            snapshot_tx,
            cursor.as_ref(),
            None,
        )?;
        values.extend(page.items);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(values);
        }
    }
}

fn records_row_matches_filters(
    row: &EntityRecordRow,
    filters: &[EntityRecordsFilter],
) -> Result<bool, String> {
    for filter in filters {
        let Some(values) = row.values.get(&filter.column_id) else {
            return Ok(false);
        };
        let mut matched = false;
        for value in values {
            if record_value_matches_filter(value, &filter.operator, &filter.value)? {
                matched = true;
                break;
            }
        }
        if !matched {
            return Ok(false);
        }
    }
    Ok(true)
}

fn record_value_matches_filter(
    value: &SnapshotPropertyValue,
    operator: &str,
    target: &str,
) -> Result<bool, String> {
    if operator == "contains" {
        let target = target.to_lowercase();
        return Ok(value.value.to_lowercase().contains(&target)
            || value
                .label
                .as_deref()
                .is_some_and(|label| label.to_lowercase().contains(&target)));
    }
    if operator == "equals" {
        return Ok(value.value.eq_ignore_ascii_case(target)
            || value
                .label
                .as_deref()
                .is_some_and(|label| label.eq_ignore_ascii_case(target)));
    }
    let ordering = compare_value_to_filter(value, target)?;
    Ok(match operator {
        "gt" => ordering == Ordering::Greater,
        "gte" => ordering != Ordering::Less,
        "lt" => ordering == Ordering::Less,
        "lte" => ordering != Ordering::Greater,
        _ => false,
    })
}

fn compare_value_to_filter(
    value: &SnapshotPropertyValue,
    target: &str,
) -> Result<Ordering, String> {
    if is_numeric_datatype(value.datatype.as_deref()) {
        return decimal_cmp(&value.value, target)
            .ok_or_else(|| format!("Invalid numeric records filter value: {target}"));
    }
    if is_date_datatype(value.datatype.as_deref()) {
        return compare_dates(&value.value, target)
            .ok_or_else(|| format!("Invalid date records filter value: {target}"));
    }
    Ok(value
        .label
        .as_deref()
        .unwrap_or(&value.value)
        .to_lowercase()
        .cmp(&target.to_lowercase()))
}

fn compare_optional_snapshot_values(
    left: Option<&SnapshotPropertyValue>,
    right: Option<&SnapshotPropertyValue>,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => compare_snapshot_values(left, right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_snapshot_values(
    left: &SnapshotPropertyValue,
    right: &SnapshotPropertyValue,
) -> Ordering {
    if is_numeric_datatype(left.datatype.as_deref())
        && is_numeric_datatype(right.datatype.as_deref())
    {
        if let Some(ordering) = decimal_cmp(&left.value, &right.value) {
            return ordering;
        }
    }
    if is_date_datatype(left.datatype.as_deref()) && is_date_datatype(right.datatype.as_deref()) {
        if let Some(ordering) = compare_dates(&left.value, &right.value) {
            return ordering;
        }
    }
    left.label
        .as_deref()
        .unwrap_or(&left.value)
        .to_lowercase()
        .cmp(
            &right
                .label
                .as_deref()
                .unwrap_or(&right.value)
                .to_lowercase(),
        )
}

fn is_numeric_datatype(datatype: Option<&str>) -> bool {
    matches!(
        datatype,
        Some(
            "xsd:decimal"
                | "xsd:integer"
                | "xsd:long"
                | "xsd:int"
                | "xsd:double"
                | "xsd:float"
                | "http://www.w3.org/2001/XMLSchema#decimal"
                | "http://www.w3.org/2001/XMLSchema#integer"
                | "http://www.w3.org/2001/XMLSchema#double"
                | "http://www.w3.org/2001/XMLSchema#float"
        )
    )
}

fn is_date_datatype(datatype: Option<&str>) -> bool {
    matches!(
        datatype,
        Some(
            "xsd:date"
                | "xsd:dateTime"
                | "http://www.w3.org/2001/XMLSchema#date"
                | "http://www.w3.org/2001/XMLSchema#dateTime"
        )
    )
}

fn decimal_cmp(left: &str, right: &str) -> Option<Ordering> {
    let left = decimal_parts(left)?;
    let right = decimal_parts(right)?;
    if left.0 != right.0 {
        return Some(if left.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let magnitude = left
        .1
        .len()
        .cmp(&right.1.len())
        .then_with(|| left.1.cmp(&right.1))
        .then_with(|| {
            let width = left.2.len().max(right.2.len());
            let left_fraction = format!("{:0<width$}", left.2, width = width);
            let right_fraction = format!("{:0<width$}", right.2, width = width);
            left_fraction.cmp(&right_fraction)
        });
    Some(if left.0 {
        magnitude.reverse()
    } else {
        magnitude
    })
}

fn decimal_parts(value: &str) -> Option<(bool, String, String)> {
    let value = value.trim();
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let mut parts = unsigned.split('.');
    let integer = parts.next()?;
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || (integer.is_empty() && fraction.is_empty())
        || !integer.chars().all(|character| character.is_ascii_digit())
        || !fraction.chars().all(|character| character.is_ascii_digit())
    {
        return None;
    }
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let fraction = fraction.trim_end_matches('0');
    let is_zero = integer == "0" && fraction.is_empty();
    Some((
        negative && !is_zero,
        integer.to_string(),
        fraction.to_string(),
    ))
}

fn compare_dates(left: &str, right: &str) -> Option<Ordering> {
    let left_datetime = DateTime::parse_from_rfc3339(left).ok();
    let right_datetime = DateTime::parse_from_rfc3339(right).ok();
    match (left_datetime, right_datetime) {
        (Some(left), Some(right)) => Some(left.cmp(&right)),
        _ => {
            let left = NaiveDate::parse_from_str(left, "%Y-%m-%d").ok()?;
            let right = NaiveDate::parse_from_str(right, "%Y-%m-%d").ok()?;
            Some(left.cmp(&right))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPropertySnapshot {
    pub entity_iri: String,
    pub snapshot_tx: i64,
    pub properties: Vec<SnapshotPropertyPage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPropertyPage {
    pub entity_iri: String,
    pub property_iri: String,
    pub property_label: String,
    pub unit_iri: Option<String>,
    pub unit_label: Option<String>,
    pub items: Vec<SnapshotPropertyValue>,
    pub has_more: bool,
    pub next_cursor: Option<PropertyValueCursor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPropertyValue {
    pub value_type: String,
    pub value: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
    pub entity_kind: Option<super::entity_kind::EntityKind>,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub status: Option<SnapshotStatusInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected_properties: Option<Vec<SnapshotPropertyPage>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStatusInfo {
    pub iri: String,
    pub label: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// Reads only the requested forward properties from one immutable-store revision.
/// The returned cursors are bound to this entity, predicate, and snapshot.
pub fn get_entity_property_snapshot(
    conn: &Connection,
    entity_iri: &str,
    property_iris: &[String],
    requested_snapshot_tx: Option<i64>,
    page_size: usize,
    property_projections: Option<&HashMap<String, Vec<String>>>,
) -> Result<EntityPropertySnapshot, String> {
    if property_iris.len() > MAX_PROPERTIES_PER_SNAPSHOT {
        return Err(format!(
            "A semantic widget snapshot supports at most {MAX_PROPERTIES_PER_SNAPSHOT} properties"
        ));
    }
    if page_size == 0 {
        return Err("Property page size must be greater than zero".to_string());
    }
    if page_size > MAX_PROPERTY_PAGE_SIZE {
        return Err(format!(
            "Property page size cannot exceed {MAX_PROPERTY_PAGE_SIZE}"
        ));
    }

    let snapshot_tx = match requested_snapshot_tx {
        Some(snapshot_tx) => snapshot_tx,
        None => owl::current_snapshot_tx(conn)?,
    };
    if snapshot_tx < 0 || snapshot_tx > owl::current_snapshot_tx(conn)? {
        return Err(format!("Invalid entity snapshot revision: {snapshot_tx}"));
    }
    let mut seen = HashSet::with_capacity(property_iris.len());
    let mut properties = Vec::with_capacity(property_iris.len());
    let requested_properties = property_iris
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if let Some(projections) = property_projections {
        for (property_iri, projected_property_iris) in projections {
            if !requested_properties.contains(property_iri.as_str()) {
                return Err(format!(
                    "Projection property '{property_iri}' must also be requested in the snapshot"
                ));
            }
            validate_projection_properties(projected_property_iris)?;
        }
    }
    for property_iri in property_iris {
        if !seen.insert(property_iri.as_str()) {
            return Err(format!("Duplicate property IRI: {property_iri}"));
        }
        let projection_property_iris = property_projections
            .and_then(|projections| projections.get(property_iri))
            .map(Vec::as_slice);
        let page = get_property_page_with_projection(
            conn,
            entity_iri,
            property_iri,
            page_size,
            snapshot_tx,
            None,
            projection_property_iris,
        )?;
        properties.push(page);
    }

    Ok(EntityPropertySnapshot {
        entity_iri: entity_iri.to_string(),
        snapshot_tx,
        properties,
    })
}

pub fn get_property_page(
    conn: &Connection,
    entity_iri: &str,
    property_iri: &str,
    page_size: usize,
    snapshot_tx: i64,
    cursor: Option<&PropertyValueCursor>,
) -> Result<SnapshotPropertyPage, String> {
    get_property_page_with_projection(
        conn,
        entity_iri,
        property_iri,
        page_size,
        snapshot_tx,
        cursor,
        None,
    )
}

pub fn get_property_page_with_projection(
    conn: &Connection,
    entity_iri: &str,
    property_iri: &str,
    page_size: usize,
    snapshot_tx: i64,
    cursor: Option<&PropertyValueCursor>,
    projection_property_iris: Option<&[String]>,
) -> Result<SnapshotPropertyPage, String> {
    if page_size == 0 {
        return Err("Property page size must be greater than zero".to_string());
    }
    if page_size > MAX_PROPERTY_PAGE_SIZE {
        return Err(format!(
            "Property page size cannot exceed {MAX_PROPERTY_PAGE_SIZE}"
        ));
    }
    if snapshot_tx < 0 || snapshot_tx > owl::current_snapshot_tx(conn)? {
        return Err(format!("Invalid entity snapshot revision: {snapshot_tx}"));
    }
    if let Some(property_iris) = projection_property_iris {
        validate_projection_properties(property_iris)?;
    }
    let page = owl::get_property_values_page(
        conn,
        entity_iri,
        property_iri,
        page_size,
        snapshot_tx,
        cursor,
    )?;
    let property_label =
        owl::get_literal_property_as_of(conn, property_iri, "rdfs:label", snapshot_tx)?
            .unwrap_or_else(|| property_iri.to_string());
    let unit_iri = owl::get_iri_property_as_of(conn, property_iri, "qudt:unit", snapshot_tx)?;
    let unit_label = match unit_iri.as_deref() {
        Some(unit_iri) => {
            owl::get_literal_property_as_of(conn, unit_iri, "qudt:currencyCode", snapshot_tx)?.or(
                owl::get_literal_property_as_of(conn, unit_iri, "rdfs:label", snapshot_tx)?,
            )
        }
        None => None,
    };
    let items = page
        .items
        .into_iter()
        .map(|triple| match triple.object {
            owl::Object::Iri(value) => {
                let mut reference = reference_value(conn, value, snapshot_tx)?;
                if let Some(projected_property_iris) = projection_property_iris {
                    reference.projected_properties = Some(
                        projected_property_iris
                            .iter()
                            .map(|projected_property_iri| {
                                get_property_page_with_projection(
                                    conn,
                                    &reference.value,
                                    projected_property_iri,
                                    page_size,
                                    snapshot_tx,
                                    None,
                                    None,
                                )
                            })
                            .collect::<Result<Vec<_>, String>>()?,
                    );
                }
                Ok(reference)
            }
            owl::Object::Blank(value) => Ok(SnapshotPropertyValue {
                value_type: "blank".to_string(),
                value,
                datatype: None,
                language: None,
                entity_kind: None,
                label: None,
                icon: None,
                status: None,
                projected_properties: None,
            }),
            owl::Object::Literal {
                value,
                datatype,
                language,
            } => literal_value(value, datatype, language),
            owl::Object::Integer(value) => {
                literal_value(value.to_string(), Some("xsd:integer".to_string()), None)
            }
            owl::Object::Number(value) => {
                literal_value(value.to_string(), Some("xsd:decimal".to_string()), None)
            }
            owl::Object::Boolean(value) => {
                literal_value(value.to_string(), Some("xsd:boolean".to_string()), None)
            }
            owl::Object::DateTime(value) => {
                literal_value(value, Some("xsd:dateTime".to_string()), None)
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SnapshotPropertyPage {
        entity_iri: entity_iri.to_string(),
        property_iri: property_iri.to_string(),
        property_label,
        unit_iri,
        unit_label,
        items,
        has_more: page.has_more,
        next_cursor: page.next_cursor,
    })
}

fn literal_value(
    value: String,
    datatype: Option<String>,
    language: Option<String>,
) -> Result<SnapshotPropertyValue, String> {
    Ok(SnapshotPropertyValue {
        value_type: "literal".to_string(),
        value,
        datatype,
        language,
        entity_kind: None,
        label: None,
        icon: None,
        status: None,
        projected_properties: None,
    })
}

fn reference_value(
    conn: &Connection,
    value: String,
    snapshot_tx: i64,
) -> Result<SnapshotPropertyValue, String> {
    let label = owl::get_literal_property_as_of(conn, &value, "rdfs:label", snapshot_tx)?
        .unwrap_or_else(|| value.clone());
    let icon = owl::get_property_values_as_of(conn, &value, "foundation:hasIcon", snapshot_tx)?
        .into_iter()
        .next()
        .and_then(|icon| match (icon.iri, icon.literal) {
            (Some(iri), _) => owl::icon_iri_to_display(conn, &iri),
            (_, Some(icon)) => Some(owl::icon_literal_to_display(&icon)),
            (None, None) => None,
        });
    let types = owl::get_all_iri_properties_as_of(conn, &value, "rdf:type", snapshot_tx)?;
    let entity_kind = if types
        .iter()
        .any(|class_iri| class_iri == "owl:Class" || class_iri == "rdfs:Class")
    {
        Some(super::entity_kind::EntityKind::Class)
    } else if types.is_empty() {
        None
    } else {
        Some(super::entity_kind::EntityKind::Individual)
    };
    let status_iri =
        owl::get_iri_property_as_of(conn, &value, "foundation:hasStatus", snapshot_tx)?;
    let status = match status_iri {
        Some(status_iri)
            if owl::get_all_iri_properties_as_of(conn, &status_iri, "rdf:type", snapshot_tx)?
                .iter()
                .any(|class_iri| class_iri == "foundation:Status") =>
        {
            let (label, color, icon) =
                super::status::get_status_info_as_of(conn, &status_iri, snapshot_tx)?;
            Some(SnapshotStatusInfo {
                iri: status_iri,
                label,
                color,
                icon,
            })
        }
        _ => None,
    };
    Ok(SnapshotPropertyValue {
        value_type: "iri".to_string(),
        value,
        datatype: None,
        language: None,
        entity_kind,
        label: Some(label),
        icon,
        status,
        projected_properties: None,
    })
}

fn validate_projection_properties(property_iris: &[String]) -> Result<(), String> {
    if property_iris.len() > MAX_PROJECTED_PROPERTIES_PER_REFERENCE {
        return Err(format!(
            "A semantic widget reference projection supports at most {MAX_PROJECTED_PROPERTIES_PER_REFERENCE} properties"
        ));
    }
    let mut seen = HashSet::with_capacity(property_iris.len());
    for property_iri in property_iris {
        if !seen.insert(property_iri.as_str()) {
            return Err(format!("Duplicate projected property IRI: {property_iri}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::{store, test_helpers::setup_test_db, Object, Triple};

    #[test]
    fn requested_properties_and_pages_remain_at_the_initial_snapshot() {
        let mut conn = setup_test_db();
        let values = (1..=3)
            .map(|number| {
                Triple::new(
                    "test:entity",
                    "test:property",
                    Object::Literal {
                        value: number.to_string(),
                        datatype: Some("xsd:integer".to_string()),
                        language: None,
                    },
                )
            })
            .collect::<Vec<_>>();
        store::assert_triples(&mut conn, &values, "semantic-widget-test").unwrap();

        let first_snapshot = get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into()],
            None,
            2,
            None,
        )
        .unwrap();
        let first_page = &first_snapshot.properties[0];
        assert_eq!(first_page.items.len(), 2);
        assert!(first_page.has_more);
        let cursor = first_page.next_cursor.clone().unwrap();

        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "test:entity",
                "test:property",
                Object::Literal {
                    value: "4".to_string(),
                    datatype: Some("xsd:integer".to_string()),
                    language: None,
                },
            )],
            "semantic-widget-test",
        )
        .unwrap();

        let second_page = get_property_page(
            &conn,
            "test:entity",
            "test:property",
            2,
            first_snapshot.snapshot_tx,
            Some(&cursor),
        )
        .unwrap();
        assert_eq!(second_page.items.len(), 1);
        assert_eq!(second_page.items[0].value, "3");
        assert!(!second_page.has_more);
    }

    #[test]
    fn snapshot_rejects_duplicate_properties_and_zero_page_size() {
        let conn = setup_test_db();
        assert!(get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into(), "test:property".into()],
            None,
            5,
            None,
        )
        .is_err());
        assert!(get_entity_property_snapshot(&conn, "test:entity", &[], None, 0, None).is_err());
    }

    #[test]
    fn reference_values_include_friendly_metadata_from_the_snapshot() {
        let mut conn = setup_test_db();
        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "test:property",
                    "rdfs:label",
                    Object::Literal {
                        value: "Owner".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:entity",
                    "test:property",
                    Object::Iri("test:owner".into()),
                ),
                Triple::new(
                    "test:owner",
                    "rdf:type",
                    Object::Iri("owl:NamedIndividual".into()),
                ),
                Triple::new(
                    "test:owner",
                    "rdfs:label",
                    Object::Literal {
                        value: "Ada Lovelace".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:owner",
                    "foundation:hasIcon",
                    Object::Iri(owl::icon_name_to_iri("person")),
                ),
                Triple::new(
                    "test:owner",
                    "foundation:hasStatus",
                    Object::Iri("test:active".into()),
                ),
                Triple::new(
                    "test:active",
                    "rdf:type",
                    Object::Iri("foundation:Status".into()),
                ),
                Triple::new(
                    "test:active",
                    "rdfs:label",
                    Object::Literal {
                        value: "Active".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:active",
                    "foundation:color",
                    Object::Literal {
                        value: "green".into(),
                        datatype: None,
                        language: None,
                    },
                ),
            ],
            "semantic-widget-test",
        )
        .unwrap();

        let snapshot = get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into()],
            None,
            5,
            None,
        )
        .unwrap();
        let property = &snapshot.properties[0];
        assert_eq!(property.property_label, "Owner");
        let owner = &property.items[0];
        assert_eq!(owner.label.as_deref(), Some("Ada Lovelace"));
        assert_eq!(owner.icon.as_deref(), Some("person"));
        assert_eq!(
            owner.entity_kind,
            Some(super::super::entity_kind::EntityKind::Individual)
        );
        let status = owner.status.as_ref().unwrap();
        assert_eq!(status.label, "Active");
        assert_eq!(status.color.as_deref(), Some("green"));
    }

    #[test]
    fn reference_projection_exposes_units_and_keeps_all_pages_on_one_snapshot() {
        let mut conn = setup_test_db();
        store::assert_triples(
            &mut conn,
            &[
                Triple::new(
                    "test:entity",
                    "test:payments",
                    Object::Iri("test:payment-1".into()),
                ),
                Triple::new(
                    "test:entity",
                    "test:payments",
                    Object::Iri("test:payment-2".into()),
                ),
                Triple::new(
                    "test:payments",
                    "rdfs:label",
                    Object::Literal {
                        value: "Pagamentos".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:amount",
                    "rdfs:label",
                    Object::Literal {
                        value: "Valor".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new(
                    "test:amount",
                    "qudt:unit",
                    Object::Iri("currency:BRL".into()),
                ),
                Triple::new(
                    "currency:BRL",
                    "qudt:currencyCode",
                    Object::Literal {
                        value: "BRL".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                Triple::new("test:payment-1", "test:amount", Object::Number(10.5)),
                Triple::new("test:payment-2", "test:amount", Object::Number(20.0)),
            ],
            "semantic-widget-test",
        )
        .unwrap();

        let projections =
            HashMap::from([("test:payments".to_string(), vec!["test:amount".to_string()])]);
        let first_snapshot = get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:payments".into()],
            None,
            1,
            Some(&projections),
        )
        .unwrap();
        let first_page = &first_snapshot.properties[0];
        assert_eq!(first_page.entity_iri, "test:entity");
        assert!(first_page.has_more);
        let first_payment = &first_page.items[0];
        assert_eq!(first_payment.value, "test:payment-1");
        let amount = &first_payment.projected_properties.as_ref().unwrap()[0];
        assert_eq!(amount.entity_iri, "test:payment-1");
        assert_eq!(amount.unit_iri.as_deref(), Some("currency:BRL"));
        assert_eq!(amount.unit_label.as_deref(), Some("BRL"));
        assert_eq!(amount.items[0].value, "10.5");

        store::assert_triples(
            &mut conn,
            &[Triple::new(
                "test:payment-2",
                "test:amount",
                Object::Number(99.0),
            )],
            "semantic-widget-test",
        )
        .unwrap();

        let second_page = get_property_page_with_projection(
            &conn,
            "test:entity",
            "test:payments",
            1,
            first_snapshot.snapshot_tx,
            first_page.next_cursor.as_ref(),
            Some(&projections["test:payments"]),
        )
        .unwrap();
        assert_eq!(second_page.items[0].value, "test:payment-2");
        assert_eq!(
            second_page.items[0].projected_properties.as_ref().unwrap()[0].items[0].value,
            "20"
        );
    }

    #[test]
    fn projection_rejects_unrequested_parent_properties_and_duplicates() {
        let conn = setup_test_db();
        let extra_parent = HashMap::from([("test:other".to_string(), vec![])]);
        assert!(get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into()],
            None,
            5,
            Some(&extra_parent),
        )
        .is_err());
        let duplicate_projection = HashMap::from([(
            "test:property".to_string(),
            vec!["test:value".to_string(), "test:value".to_string()],
        )]);
        assert!(get_entity_property_snapshot(
            &conn,
            "test:entity",
            &["test:property".into()],
            None,
            5,
            Some(&duplicate_projection),
        )
        .is_err());
    }
}
