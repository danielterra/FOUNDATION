mod find;

pub use find::*;

use rusqlite::{Connection, Row, types::Value as SqlValue};
use super::triple_type::Triple;
use super::object_type::Object;
use super::query_result_type::QueryResult;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// SQL fragment that ensures the row is the latest assertion for its
/// (subject, predicate) group — i.e., no newer tx exists for the same group.
const AND_IS_CURRENT: &str =
    "AND t.tx = (SELECT MAX(tx) FROM triples WHERE subject = t.subject AND predicate = t.predicate)";

pub fn get_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, 0 AS retracted, created_at
         FROM triples_current
         WHERE subject = ?"
    )?;

    let triples: Vec<Triple> = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

pub fn get_retracted_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND retracted = 1
         ORDER BY predicate, tx DESC"
    )?;

    let triples = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

pub fn get_retraction_tx(conn: &Connection, entity: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT MAX(tx) FROM triples WHERE subject = ? AND retracted = 1",
        [entity],
        |row| row.get::<_, Option<i64>>(0),
    ).map_err(Into::into)
}


/// Returns all active triples for a subject as they existed just before `before_tx`.
/// For each (subject, predicate), returns the group at MAX(tx) where retracted=0 AND tx < before_tx.
pub fn get_last_active_by_entity_before_tx(
    conn: &Connection,
    entity: &str,
    before_tx: i64,
) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, 0 AS retracted, created_at
         FROM triples t
         WHERE t.subject = ?1
           AND t.retracted = 0
           AND t.tx < ?2
           AND t.tx = (
               SELECT MAX(tx) FROM triples t2
               WHERE t2.subject = ?1
                 AND t2.predicate = t.predicate
                 AND t2.retracted = 0
                 AND t2.tx < ?2
           )"
    )?;
    let triples = stmt
        .query_map(rusqlite::params![entity, before_tx], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(QueryResult::new(triples))
}

/// Returns all active triples with the given predicate as they existed just before `before_tx`.
/// For each (subject, predicate), returns the group at MAX(tx) where retracted=0 AND tx < before_tx.
pub fn get_last_active_by_predicate_before_tx(
    conn: &Connection,
    predicate: &str,
    before_tx: i64,
) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, 0 AS retracted, created_at
         FROM triples t
         WHERE t.predicate = ?1
           AND t.retracted = 0
           AND t.tx < ?2
           AND t.tx = (
               SELECT MAX(tx) FROM triples t2
               WHERE t2.subject = t.subject
                 AND t2.predicate = ?1
                 AND t2.retracted = 0
                 AND t2.tx < ?2
           )"
    )?;
    let triples = stmt
        .query_map(rusqlite::params![predicate, before_tx], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(QueryResult::new(triples))
}

pub fn get_by_object_iri(conn: &Connection, object_iri: &str) -> Result<QueryResult> {
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         WHERE t.object = ? AND t.object_type = 'iri' AND t.retracted = 0
         {}",
        AND_IS_CURRENT
    );
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map([object_iri], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

pub fn get_by_predicate(conn: &Connection, predicate: &str) -> Result<QueryResult> {
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         WHERE t.predicate = ? AND t.retracted = 0
         {}
         ORDER BY t.tx DESC",
        AND_IS_CURRENT
    );
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map([predicate], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

pub fn get_by_entity_predicate(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<QueryResult> {
    get_by_entity_predicate_internal(conn, entity, predicate, true)
}

pub fn get_by_entity_predicate_internal(
    conn: &Connection,
    entity: &str,
    predicate: &str,
    check_functional: bool,
) -> Result<QueryResult> {

    let is_functional = if check_functional {
        crate::owl::Property::is_functional(conn, predicate)
            .unwrap_or(false)
    } else {
        false
    };

    if is_functional {
        let mut stmt = conn.prepare(
            "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_type, object_number, object_integer, object_boolean,
                    tx, origin_id, retracted, created_at
             FROM triples
             WHERE subject = ? AND predicate = ?
             ORDER BY tx DESC
             LIMIT 1"
        )?;

        let triples: Vec<Triple> = stmt
            .query_map([entity, predicate], row_to_triple)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let active: Vec<Triple> = triples.into_iter().filter(|t| !t.retracted).collect();
        Ok(QueryResult::new(active))
    } else {
        let mut stmt = conn.prepare(
            "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_type, object_number, object_integer, object_boolean,
                    tx, origin_id, 0 AS retracted, created_at
             FROM triples_current
             WHERE subject = ? AND predicate = ?"
        )?;

        let triples: Vec<Triple> = stmt
            .query_map([entity, predicate], row_to_triple)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(QueryResult::new(triples))
    }
}

pub fn get_by_predicate_object(
    conn: &Connection,
    predicate: &str,
    object: &str,
) -> Result<QueryResult> {
    let (where_clause, params): (&str, Vec<&dyn rusqlite::ToSql>) = if object == "true" {
        (
            "WHERE t.predicate = ?1 AND t.object_boolean = 1 AND t.retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql],
        )
    } else if object == "false" {
        (
            "WHERE t.predicate = ?1 AND t.object_boolean = 0 AND t.retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql],
        )
    } else {
        (
            "WHERE t.predicate = ?1 AND t.object = ?2 AND t.retracted = 0",
            vec![&predicate as &dyn rusqlite::ToSql, &object as &dyn rusqlite::ToSql],
        )
    };

    let query = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         {}
         {}
         ORDER BY t.tx DESC",
        where_clause, AND_IS_CURRENT
    );

    let mut stmt = conn.prepare(&query)?;
    let triples = stmt
        .query_map(params.as_slice(), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

#[derive(Debug, Clone)]
pub struct BacklinkRow {
    pub subject: String,
    pub predicate: String,
    pub source_class: Option<String>,
    pub group_total: usize,
}

pub fn get_backlinks_grouped_limited(
    conn: &Connection,
    object: &str,
    limit_per_group: usize,
) -> Result<Vec<BacklinkRow>> {
    let sql = format!(
        "WITH
         backlinks_raw AS (
             SELECT t.subject, t.predicate, MAX(t.tx) AS last_tx
             FROM triples t
             WHERE t.object = ?1
               AND t.object_type = 'iri'
               AND t.retracted = 0
               AND t.predicate != 'rdf:type'
               AND t.subject != ?1
               AND t.tx = (SELECT MAX(tx) FROM triples WHERE subject = t.subject AND predicate = t.predicate)
             GROUP BY t.subject, t.predicate
         ),
         subject_class AS (
             SELECT subject, object AS source_class
             FROM (
                 SELECT t.subject, t.object, t.tx,
                        ROW_NUMBER() OVER (PARTITION BY t.subject ORDER BY t.tx DESC) AS rn
                 FROM triples t
                 INNER JOIN backlinks_raw br ON t.subject = br.subject
                 WHERE t.predicate = 'rdf:type'
                   AND t.object_type = 'iri'
                   AND t.retracted = 0
                   AND t.tx = (SELECT MAX(tx) FROM triples WHERE subject = t.subject AND predicate = t.predicate)
             )
             WHERE rn = 1
         ),
         backlinks_with_class AS (
             SELECT
                 br.subject,
                 br.predicate,
                 br.last_tx,
                 sc.source_class
             FROM backlinks_raw br
             LEFT JOIN subject_class sc ON sc.subject = br.subject
         ),
         group_counts AS (
             SELECT predicate, source_class, COUNT(*) AS total
             FROM backlinks_with_class
             GROUP BY predicate, source_class
         ),
         ranked AS (
             SELECT
                 bwc.subject, bwc.predicate, bwc.source_class, bwc.last_tx,
                 gc.total AS group_total,
                 ROW_NUMBER() OVER (
                     PARTITION BY bwc.predicate, bwc.source_class
                     ORDER BY bwc.last_tx DESC
                 ) AS rn
             FROM backlinks_with_class bwc
             JOIN group_counts gc
               ON gc.predicate = bwc.predicate
              AND gc.source_class IS bwc.source_class
         )
         SELECT subject, predicate, source_class, group_total
         FROM ranked
         WHERE rn <= {}
         ORDER BY last_tx DESC",
        limit_per_group
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([object], |row| {
            let subject: String = row.get(0)?;
            let predicate: String = row.get(1)?;
            let source_class: Option<String> = row.get(2)?;
            let group_total: i64 = row.get(3)?;
            Ok(BacklinkRow {
                subject,
                predicate,
                source_class,
                group_total: group_total as usize,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_predicates_for_subjects(
    conn: &Connection,
    subjects: &[String],
    predicates: &[&str],
) -> Result<Vec<(String, String, Object)>> {
    if subjects.is_empty() || predicates.is_empty() {
        return Ok(Vec::new());
    }
    let subject_phs = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let predicate_phs = predicates.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         WHERE t.subject IN ({}) AND t.predicate IN ({}) AND t.retracted = 0
         {}
         ORDER BY t.subject, t.predicate, t.tx DESC",
        subject_phs, predicate_phs, AND_IS_CURRENT
    );
    let mut params: Vec<SqlValue> = subjects.iter()
        .map(|s| SqlValue::Text(s.clone()))
        .collect();
    params.extend(predicates.iter().map(|p| SqlValue::Text(p.to_string())));
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(triples.into_iter().map(|t| (t.subject, t.predicate, t.object)).collect())
}

pub fn get_first_iri_property_batch(
    conn: &Connection,
    subjects: &[String],
    predicate: &str,
) -> Result<std::collections::HashMap<String, String>> {
    if subjects.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, object FROM triples t
         WHERE t.subject IN ({}) AND t.predicate = ? AND t.object_type = 'iri'
           AND t.retracted = 0
           {}
         ORDER BY t.subject, t.tx DESC",
        placeholders, AND_IS_CURRENT
    );
    let mut params: Vec<SqlValue> = subjects.iter()
        .map(|s| SqlValue::Text(s.clone()))
        .collect();
    params.push(SqlValue::Text(predicate.to_string()));
    let mut stmt = conn.prepare(&sql)?;
    let mut map = std::collections::HashMap::new();
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        let subject: String = row.get(0)?;
        let object: String = row.get(1)?;
        Ok((subject, object))
    })?;
    for row in rows {
        let (subject, object) = row?;
        map.entry(subject).or_insert(object);
    }
    Ok(map)
}

#[allow(dead_code)]
pub fn get_at_time(conn: &Connection, entity: &str, tx: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND tx <= ? AND retracted = 0
         ORDER BY predicate, tx DESC"
    )?;

    let triples = stmt
        .query_map([entity, tx.to_string().as_str()], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut seen_predicates = std::collections::HashSet::new();
    let snapshot: Vec<Triple> = triples
        .into_iter()
        .filter(|t| seen_predicates.insert(t.predicate.clone()))
        .collect();

    Ok(QueryResult::new(snapshot))
}

#[allow(dead_code)]
pub fn get_by_origin(conn: &Connection, origin_id: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE origin_id = ? AND retracted = 0
         ORDER BY tx DESC"
    )?;

    let triples = stmt
        .query_map([origin_id], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

#[allow(dead_code)]
pub fn get_history(conn: &Connection, entity: &str) -> Result<Vec<(i64, Vec<Triple>)>> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ?
         ORDER BY tx ASC"
    )?;

    let all_triples: Vec<Triple> = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut history: std::collections::HashMap<i64, Vec<Triple>> = std::collections::HashMap::new();
    for triple in all_triples {
        history.entry(triple.tx).or_insert_with(Vec::new).push(triple);
    }

    let mut result: Vec<(i64, Vec<Triple>)> = history.into_iter().collect();
    result.sort_by_key(|(tx, _)| *tx);

    Ok(result)
}

pub fn get_entities_max_tx(
    conn: &Connection,
    entity_iris: &[String],
) -> Result<std::collections::HashMap<String, i64>> {
    if entity_iris.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let placeholders = entity_iris
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT subject, MAX(tx) FROM triples WHERE subject IN ({}) GROUP BY subject",
        placeholders
    );

    let params: Vec<&dyn rusqlite::ToSql> = entity_iris
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();

    let mut stmt = conn.prepare(&sql)?;
    let result = stmt
        .query_map(params.as_slice(), |row| {
            let subject: String = row.get(0)?;
            let max_tx: i64 = row.get(1)?;
            Ok((subject, max_tx))
        })?
        .collect::<std::result::Result<std::collections::HashMap<_, _>, _>>()?;

    Ok(result)
}

pub fn batch_load_triples_for_subjects(
    conn: &Connection,
    subjects: &[String],
) -> Result<std::collections::HashMap<String, Vec<Triple>>> {
    if subjects.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples t
         WHERE t.subject IN ({}) AND t.retracted = 0
         {}
         ORDER BY t.subject, t.predicate, t.tx DESC",
        placeholders, AND_IS_CURRENT
    );
    let params: Vec<SqlValue> = subjects.iter().map(|s| SqlValue::Text(s.clone())).collect();
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut map: std::collections::HashMap<String, Vec<Triple>> = std::collections::HashMap::new();
    for triple in triples {
        map.entry(triple.subject.clone()).or_default().push(triple);
    }
    Ok(map)
}

pub fn batch_load_retracted_triples_for_subjects(
    conn: &Connection,
    subjects: &[String],
) -> Result<std::collections::HashMap<String, Vec<Triple>>> {
    if subjects.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject IN ({}) AND retracted = 1
         ORDER BY subject, predicate, tx DESC",
        placeholders
    );
    let params: Vec<SqlValue> = subjects.iter().map(|s| SqlValue::Text(s.clone())).collect();
    let mut stmt = conn.prepare(&sql)?;
    let triples = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut map: std::collections::HashMap<String, Vec<Triple>> = std::collections::HashMap::new();
    for triple in triples {
        map.entry(triple.subject.clone()).or_default().push(triple);
    }
    Ok(map)
}

pub(crate) fn row_to_triple(row: &Row) -> rusqlite::Result<Triple> {
    let subject: String = row.get(0)?;
    let predicate: String = row.get(1)?;
    let object_opt: Option<String> = row.get(2)?;
    let object_value: Option<String> = row.get(3)?;
    let object_datatype: Option<String> = row.get(4)?;
    let object_language: Option<String> = row.get(5)?;
    let object_type: String = row.get(6)?;
    let object_number: Option<f64> = row.get(7)?;
    let object_integer: Option<i64> = row.get(8)?;
    let object_boolean: Option<i64> = row.get(9)?;
    let tx: i64 = row.get(10)?;
    let origin_id: i64 = row.get(11)?;
    let retracted: i64 = row.get(12)?;
    let created_at: i64 = row.get(13)?;

    let object = match object_type.as_str() {
        "iri" => Object::Iri(object_opt.ok_or(rusqlite::Error::InvalidQuery)?),
        "blank" => Object::Blank(object_opt.ok_or(rusqlite::Error::InvalidQuery)?),
        "literal" => {
            if let Some(int) = object_integer {
                Object::Integer(int)
            } else if let Some(num) = object_number {
                Object::Number(num)
            } else if object_datatype.as_deref() == Some("xsd:dateTime") {
                Object::DateTime(object_value.ok_or(rusqlite::Error::InvalidQuery)?)
            } else if object_datatype.as_deref() == Some("xsd:date") {
                Object::Literal {
                    value: object_value.ok_or(rusqlite::Error::InvalidQuery)?,
                    datatype: object_datatype,
                    language: object_language,
                }
            } else if let Some(bool_val) = object_boolean {
                Object::Boolean(bool_val != 0)
            } else {
                Object::Literal {
                    value: object_value.ok_or(rusqlite::Error::InvalidQuery)?,
                    datatype: object_datatype,
                    language: object_language,
                }
            }
        }
        _ => unreachable!("Invalid object_type in database"),
    };

    Ok(Triple {
        subject,
        predicate,
        object,
        tx,
        origin_id,
        retracted: retracted != 0,
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::{setup_test_db, create_test_triples};
    use crate::eavto::store::assert_triples;

    fn setup_test_data(conn: &mut Connection) -> i64 {
        let triples = create_test_triples();
        assert_triples(conn, &triples, "test").unwrap()
    }

    #[test]
    fn test_get_by_entity() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity(&conn, "foundation:TestClass").unwrap();
        assert_eq!(result.triples.len(), 2);
    }

    #[test]
    fn test_get_by_entity_nonexistent() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity(&conn, "foundation:NonExistent").unwrap();
        assert_eq!(result.triples.len(), 0);
    }

    #[test]
    fn test_get_by_predicate() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_predicate(&conn, "rdf:type").unwrap();
        assert_eq!(result.triples.len(), 1);
    }

    #[test]
    fn test_get_by_entity_predicate() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity_predicate(&conn, "foundation:TestClass", "rdfs:label").unwrap();
        assert_eq!(result.triples.len(), 1);

        let triple = &result.triples[0];
        match &triple.object {
            Object::Literal { value, .. } => assert_eq!(value, "Test Class"),
            _ => panic!("Expected literal object"),
        }
    }

    #[test]
    fn test_get_at_time() {
        let mut conn = setup_test_db();
        let tx_id = setup_test_data(&mut conn);

        let result = get_at_time(&conn, "foundation:TestClass", tx_id).unwrap();
        assert_eq!(result.triples.len(), 2);
    }

    #[test]
    fn test_get_at_time_temporal_snapshot() {
        let mut conn = setup_test_db();
        let _tx1 = setup_test_data(&mut conn);

        let updated_triple = vec![Triple {
            subject: "foundation:TestClass".to_string(),
            predicate: "rdfs:label".to_string(),
            object: Object::Literal {
                value: "Updated Label".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            tx: 0,
            created_at: 2000,
            origin_id: 1,
            retracted: false,
        }];
        let tx2 = assert_triples(&mut conn, &updated_triple, "test").unwrap();

        let result = get_at_time(&conn, "foundation:TestClass", tx2).unwrap();

        assert_eq!(result.triples.len(), 2);

        let label_triple = result.triples.iter()
            .find(|t| t.predicate == "rdfs:label")
            .expect("Should have label");

        match &label_triple.object {
            Object::Literal { value, .. } => assert_eq!(value, "Updated Label"),
            _ => panic!("Expected literal"),
        }
    }

    #[test]
    fn test_get_by_origin() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_origin(&conn, 1).unwrap();
        assert!(result.triples.len() > 0);
    }

    #[test]
    fn test_get_history() {
        let mut conn = setup_test_db();
        let tx1 = setup_test_data(&mut conn);

        let new_triple = vec![Triple {
            subject: "foundation:TestClass".to_string(),
            predicate: "rdfs:comment".to_string(),
            object: Object::Literal {
                value: "A comment".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            tx: 0,
            created_at: 2000,
            origin_id: 1,
            retracted: false,
        }];
        let tx2 = assert_triples(&mut conn, &new_triple, "test").unwrap();

        let history = get_history(&conn, "foundation:TestClass").unwrap();

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].0, tx1);
        assert_eq!(history[1].0, tx2);
        assert_eq!(history[0].1.len(), 2);
        assert_eq!(history[1].1.len(), 1);
    }

    #[test]
    fn test_row_to_triple_with_iri() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity(&conn, "foundation:TestClass").unwrap();
        let iri_triple = result.triples.iter()
            .find(|t| t.predicate == "rdf:type")
            .expect("Should have rdf:type");

        match &iri_triple.object {
            Object::Iri(iri) => assert_eq!(iri, "owl:Class"),
            _ => panic!("Expected IRI object"),
        }
    }

    #[test]
    fn test_row_to_triple_with_integer() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let result = get_by_entity(&conn, "foundation:TestProperty").unwrap();
        let int_triple = result.triples.iter()
            .find(|t| t.predicate == "foundation:someValue")
            .expect("Should have foundation:someValue");

        match &int_triple.object {
            Object::Integer(i) => assert_eq!(*i, 42),
            _ => panic!("Expected Integer object"),
        }
    }

    #[test]
    fn test_find_by_class_iris_and_properties_returns_subclass_instances() {
        let mut conn = setup_test_db();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Animal".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("owl:Class".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Dog".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("owl:Class".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:Dog".to_string(), predicate: "rdfs:subClassOf".to_string(),
                object: Object::Iri("foundation:Animal".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Rex".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Dog".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:Rex".to_string(), predicate: "foundation:name".to_string(),
                object: Object::Literal { value: "Rex".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn,
            &["foundation:Animal", "foundation:Dog"],
            &[PropertyFilter::Compare("foundation:name", "Rex", "=")],
            false,
            100,
            0,
        ).unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"foundation:Rex".to_string()));
    }

    #[test]
    fn test_find_by_class_iris_single_class_filters_by_property() {
        let mut conn = setup_test_db();
        setup_test_data(&mut conn);

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn,
            &["owl:Class"],
            &[PropertyFilter::Compare("rdfs:label", "Test Class", "=")],
            false,
            100,
            0,
        ).unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"foundation:TestClass".to_string()));
    }

    #[test]
    fn test_find_by_properties_without_class_constraint() {
        let mut conn = setup_test_db();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:PersonA".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Person".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:PersonA".to_string(), predicate: "foundation:status".to_string(),
                object: Object::Literal { value: "active".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:CompanyX".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Company".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:CompanyX".to_string(), predicate: "foundation:status".to_string(),
                object: Object::Literal { value: "active".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:PersonB".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Person".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:PersonB".to_string(), predicate: "foundation:status".to_string(),
                object: Object::Literal { value: "inactive".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").unwrap();

        let (results, total) = find_by_properties_with_options(
            &conn,
            &[PropertyFilter::Compare("foundation:status", "active", "=")],
            false,
            100,
            0,
        ).unwrap();

        assert_eq!(total, 2, "should find both active entities regardless of class");
        assert!(results.contains(&"foundation:PersonA".to_string()));
        assert!(results.contains(&"foundation:CompanyX".to_string()));
        assert!(!results.contains(&"foundation:PersonB".to_string()));
    }
}
