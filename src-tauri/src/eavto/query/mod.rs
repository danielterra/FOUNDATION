mod find;
mod search;

pub use find::*;
pub use search::*;

use turso::{Connection, Value};
use super::triple_type::Triple;
use super::object_type::Object;
use super::query_result_type::QueryResult;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub async fn get_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND retracted = 0
         ORDER BY predicate, object, object_value, tx DESC"
    ).await?;

    let mut rows = stmt.query(turso::params![entity]).await?;
    let mut all_triples = Vec::new();
    while let Some(row) = rows.next().await? {
        all_triples.push(row_to_triple(&row)?);
    }

    let mut seen_pairs = std::collections::HashSet::new();
    let current_triples: Vec<Triple> = all_triples
        .into_iter()
        .filter(|t| {
            let object_key = match &t.object {
                Object::Iri(iri) => format!("iri:{}", iri),
                Object::Literal { value, datatype, language } => {
                    format!(
                        "lit:{}:{}:{}",
                        value,
                        datatype.as_deref().unwrap_or(""),
                        language.as_deref().unwrap_or(""),
                    )
                },
                Object::Blank(id) => format!("blank:{}", id),
                Object::Integer(n) => format!("int:{}", n),
                Object::Number(n) => format!("num:{}", n),
                Object::Boolean(b) => format!("bool:{}", b),
                Object::DateTime(dt) => format!("dt:{}", dt),
            };
            let key = format!("{}|{}", t.predicate, object_key);
            seen_pairs.insert(key)
        })
        .collect();

    Ok(QueryResult::new(current_triples))
}

pub async fn get_retracted_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND retracted = 1
         ORDER BY predicate, tx DESC"
    ).await?;

    let mut rows = stmt.query(turso::params![entity]).await?;
    let mut triples = Vec::new();
    while let Some(row) = rows.next().await? {
        triples.push(row_to_triple(&row)?);
    }

    Ok(QueryResult::new(triples))
}

pub async fn get_by_object_iri(conn: &Connection, object_iri: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE object = ? AND object_type = 'iri' AND retracted = 0"
    ).await?;

    let mut rows = stmt.query(turso::params![object_iri]).await?;
    let mut triples = Vec::new();
    while let Some(row) = rows.next().await? {
        triples.push(row_to_triple(&row)?);
    }

    Ok(QueryResult::new(triples))
}

pub async fn get_by_predicate(conn: &Connection, predicate: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE predicate = ? AND retracted = 0
         ORDER BY tx DESC"
    ).await?;

    let mut rows = stmt.query(turso::params![predicate]).await?;
    let mut triples = Vec::new();
    while let Some(row) = rows.next().await? {
        triples.push(row_to_triple(&row)?);
    }

    Ok(QueryResult::new(triples))
}

pub async fn get_by_entity_predicate(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<QueryResult> {
    get_by_entity_predicate_internal(conn, entity, predicate, true).await
}

pub async fn get_by_entity_predicate_internal(
    conn: &Connection,
    entity: &str,
    predicate: &str,
    check_functional: bool,
) -> Result<QueryResult> {

    let is_functional = if check_functional {
        Box::pin(crate::owl::Property::is_functional(conn, predicate)).await
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
             WHERE subject = ? AND predicate = ? AND retracted = 0
             ORDER BY tx DESC
             LIMIT 1"
        ).await?;

        let mut rows = stmt.query(turso::params![entity, predicate]).await?;
        let mut triples = Vec::new();
        while let Some(row) = rows.next().await? {
            triples.push(row_to_triple(&row)?);
        }

        Ok(QueryResult::new(triples))
    } else {
        let mut stmt = conn.prepare(
            "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                    object_type, object_number, object_integer, object_boolean,
                    tx, origin_id, retracted, created_at
             FROM triples
             WHERE subject = ? AND predicate = ? AND retracted = 0
             ORDER BY object, object_value, tx DESC"
        ).await?;

        let mut rows = stmt.query(turso::params![entity, predicate]).await?;
        let mut all_triples = Vec::new();
        while let Some(row) = rows.next().await? {
            all_triples.push(row_to_triple(&row)?);
        }

        let mut seen_objects = std::collections::HashSet::new();
        let current_triples: Vec<Triple> = all_triples
            .into_iter()
            .filter(|t| {
                let key = match &t.object {
                    Object::Iri(iri) => format!("iri:{}", iri),
                    Object::Blank(id) => format!("blank:{}", id),
                    Object::Literal { value, .. } => format!("lit:{}", value),
                    Object::Integer(i) => format!("int:{}", i),
                    Object::Number(n) => format!("num:{}", n),
                    Object::Boolean(b) => format!("bool:{}", b),
                    Object::DateTime(dt) => format!("dt:{}", dt),
                };
                seen_objects.insert(key)
            })
            .collect();

        Ok(QueryResult::new(current_triples))
    }
}

pub async fn get_by_predicate_object(
    conn: &Connection,
    predicate: &str,
    object: &str,
) -> Result<QueryResult> {
    let (where_clause, extra_params): (&str, Vec<Value>) = if object == "true" {
        (
            "WHERE predicate = ? AND object_boolean = 1 AND retracted = 0",
            vec![],
        )
    } else if object == "false" {
        (
            "WHERE predicate = ? AND object_boolean = 0 AND retracted = 0",
            vec![],
        )
    } else {
        (
            "WHERE predicate = ? AND object = ? AND retracted = 0",
            vec![Value::Text(object.to_string())],
        )
    };

    let query = format!(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         {}
         ORDER BY tx DESC",
        where_clause
    );

    let mut all_params = vec![Value::Text(predicate.to_string())];
    all_params.extend(extra_params);

    let mut stmt = conn.prepare(&query).await?;
    let p = turso::params_from_iter(all_params.into_iter());
    let mut rows = stmt.query(p).await?;
    let mut triples = Vec::new();
    while let Some(row) = rows.next().await? {
        triples.push(row_to_triple(&row)?);
    }

    Ok(QueryResult::new(triples))
}

#[derive(Debug, Clone)]
pub struct BacklinkRow {
    pub subject: String,
    pub predicate: String,
    pub source_class: Option<String>,
    pub group_total: usize,
}

pub async fn get_backlinks_grouped_limited(
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
             GROUP BY t.subject, t.predicate
         ),
         backlinks_with_class AS (
             SELECT
                 br.subject,
                 br.predicate,
                 br.last_tx,
                 (SELECT t2.object FROM triples t2
                  WHERE t2.subject = br.subject
                    AND t2.predicate = 'rdf:type'
                    AND t2.object_type = 'iri'
                    AND t2.retracted = 0
                  ORDER BY t2.tx DESC
                  LIMIT 1) AS source_class
             FROM backlinks_raw br
         ),
         group_counts AS (
             SELECT predicate, source_class, COUNT(*) AS total
             FROM backlinks_with_class
             GROUP BY predicate, source_class
         ),
         ranked AS (
             SELECT
                 bwc.subject, bwc.predicate, bwc.source_class, bwc.last_tx,
                 gc.total AS group_total
             FROM backlinks_with_class bwc
             JOIN group_counts gc
               ON gc.predicate = bwc.predicate
              AND gc.source_class IS bwc.source_class
             WHERE (
                 SELECT COUNT(*) FROM backlinks_with_class bwc2
                 WHERE bwc2.predicate = bwc.predicate
                   AND bwc2.source_class IS bwc.source_class
                   AND bwc2.last_tx > bwc.last_tx
             ) < {}
         )
         SELECT subject, predicate, source_class, group_total
         FROM ranked
         ORDER BY last_tx DESC",
        limit_per_group
    );
    let mut stmt = conn.prepare(&sql).await?;
    let mut rows = stmt.query(turso::params![object]).await?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await? {
        let subject: String = row.get_value(0)?.as_text().cloned().unwrap_or_default();
        let predicate: String = row.get_value(1)?.as_text().cloned().unwrap_or_default();
        let source_class: Option<String> = match row.get_value(2)? { Value::Null => None, v => v.as_text().cloned() };
        let group_total: i64 = row.get_value(3)?.as_integer().copied().unwrap_or(0);
        result.push(BacklinkRow {
            subject,
            predicate,
            source_class,
            group_total: group_total as usize,
        });
    }
    Ok(result)
}

pub async fn get_predicates_for_subjects(
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
         FROM triples
         WHERE subject IN ({}) AND predicate IN ({}) AND retracted = 0
         ORDER BY subject, predicate, tx DESC",
        subject_phs, predicate_phs
    );
    let mut params: Vec<Value> = subjects.iter()
        .map(|s| Value::Text(s.clone()))
        .collect();
    params.extend(predicates.iter().map(|p| Value::Text(p.to_string())));
    let p = turso::params_from_iter(params.into_iter());
    let mut stmt = conn.prepare(&sql).await?;
    let mut rows = stmt.query(p).await?;
    let mut triples = Vec::new();
    while let Some(row) = rows.next().await? {
        triples.push(row_to_triple(&row)?);
    }
    Ok(triples.into_iter().map(|t| (t.subject, t.predicate, t.object)).collect())
}

pub async fn get_first_iri_property_batch(
    conn: &Connection,
    subjects: &[String],
    predicate: &str,
) -> Result<std::collections::HashMap<String, String>> {
    if subjects.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = subjects.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject, object FROM triples
         WHERE subject IN ({}) AND predicate = ? AND object_type = 'iri' AND retracted = 0
         ORDER BY subject, tx DESC",
        placeholders
    );
    let mut params: Vec<Value> = subjects.iter()
        .map(|s| Value::Text(s.clone()))
        .collect();
    params.push(Value::Text(predicate.to_string()));
    let p = turso::params_from_iter(params.into_iter());
    let mut stmt = conn.prepare(&sql).await?;
    let mut rows = stmt.query(p).await?;
    let mut map = std::collections::HashMap::new();
    while let Some(row) = rows.next().await? {
        let subject: String = row.get_value(0)?.as_text().cloned().unwrap_or_default();
        let object: String = row.get_value(1)?.as_text().cloned().unwrap_or_default();
        map.entry(subject).or_insert(object);
    }
    Ok(map)
}

#[allow(dead_code)]
pub async fn get_at_time(conn: &Connection, entity: &str, tx: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND tx <= ? AND retracted = 0
         ORDER BY predicate, tx DESC"
    ).await?;

    let mut rows = stmt.query(turso::params![entity, tx]).await?;
    let mut triples = Vec::new();
    while let Some(row) = rows.next().await? {
        triples.push(row_to_triple(&row)?);
    }

    let mut seen_predicates = std::collections::HashSet::new();
    let snapshot: Vec<Triple> = triples
        .into_iter()
        .filter(|t| seen_predicates.insert(t.predicate.clone()))
        .collect();

    Ok(QueryResult::new(snapshot))
}

#[allow(dead_code)]
pub async fn get_by_origin(conn: &Connection, origin_id: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE origin_id = ? AND retracted = 0
         ORDER BY tx DESC"
    ).await?;

    let mut rows = stmt.query(turso::params![origin_id]).await?;
    let mut triples = Vec::new();
    while let Some(row) = rows.next().await? {
        triples.push(row_to_triple(&row)?);
    }

    Ok(QueryResult::new(triples))
}

#[allow(dead_code)]
pub async fn get_history(conn: &Connection, entity: &str) -> Result<Vec<(i64, Vec<Triple>)>> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ?
         ORDER BY tx ASC"
    ).await?;

    let mut rows = stmt.query(turso::params![entity]).await?;
    let mut all_triples = Vec::new();
    while let Some(row) = rows.next().await? {
        all_triples.push(row_to_triple(&row)?);
    }

    let mut history: std::collections::HashMap<i64, Vec<Triple>> = std::collections::HashMap::new();
    for triple in all_triples {
        history.entry(triple.tx).or_insert_with(Vec::new).push(triple);
    }

    let mut result: Vec<(i64, Vec<Triple>)> = history.into_iter().collect();
    result.sort_by_key(|(tx, _)| *tx);

    Ok(result)
}

pub async fn get_entities_max_tx(
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
        "SELECT subject, MAX(tx) FROM triples WHERE subject IN ({}) AND retracted = 0 GROUP BY subject",
        placeholders
    );

    let params: Vec<Value> = entity_iris
        .iter()
        .map(|s| Value::Text(s.clone()))
        .collect();

    let p = turso::params_from_iter(params.into_iter());
    let mut stmt = conn.prepare(&sql).await?;
    let mut rows = stmt.query(p).await?;
    let mut result = std::collections::HashMap::new();
    while let Some(row) = rows.next().await? {
        let subject: String = row.get_value(0)?.as_text().cloned().unwrap_or_default();
        let max_tx: i64 = row.get_value(1)?.as_integer().copied().unwrap_or(0);
        result.insert(subject, max_tx);
    }

    Ok(result)
}

pub async fn batch_load_triples_for_subjects(
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
         WHERE subject IN ({}) AND retracted = 0
         ORDER BY subject, predicate, tx DESC",
        placeholders
    );
    let params: Vec<Value> = subjects.iter().map(|s| Value::Text(s.clone())).collect();
    let p = turso::params_from_iter(params.into_iter());
    let mut stmt = conn.prepare(&sql).await?;
    let mut rows = stmt.query(p).await?;
    let mut map: std::collections::HashMap<String, Vec<Triple>> = std::collections::HashMap::new();
    while let Some(row) = rows.next().await? {
        let triple = row_to_triple(&row)?;
        map.entry(triple.subject.clone()).or_default().push(triple);
    }
    Ok(map)
}

pub async fn batch_load_retracted_triples_for_subjects(
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
    let params: Vec<Value> = subjects.iter().map(|s| Value::Text(s.clone())).collect();
    let p = turso::params_from_iter(params.into_iter());
    let mut stmt = conn.prepare(&sql).await?;
    let mut rows = stmt.query(p).await?;
    let mut map: std::collections::HashMap<String, Vec<Triple>> = std::collections::HashMap::new();
    while let Some(row) = rows.next().await? {
        let triple = row_to_triple(&row)?;
        map.entry(triple.subject.clone()).or_default().push(triple);
    }
    Ok(map)
}

pub(crate) fn row_to_triple(row: &turso::Row) -> Result<Triple> {
    let subject: String = row.get_value(0)?.as_text().cloned().unwrap_or_default();
    let predicate: String = row.get_value(1)?.as_text().cloned().unwrap_or_default();
    let object_opt: Option<String> = match row.get_value(2)? { Value::Null => None, v => v.as_text().cloned() };
    let object_value: Option<String> = match row.get_value(3)? { Value::Null => None, v => v.as_text().cloned() };
    let object_datatype: Option<String> = match row.get_value(4)? { Value::Null => None, v => v.as_text().cloned() };
    let object_language: Option<String> = match row.get_value(5)? { Value::Null => None, v => v.as_text().cloned() };
    let object_type: String = row.get_value(6)?.as_text().cloned().unwrap_or_default();
    let object_number: Option<f64> = row.get_value(7)?.as_real().copied();
    let object_integer: Option<i64> = row.get_value(8)?.as_integer().copied();
    let object_boolean: Option<i64> = row.get_value(9)?.as_integer().copied();
    let tx: i64 = row.get_value(10)?.as_integer().copied().unwrap_or(0);
    let origin_id: i64 = row.get_value(11)?.as_integer().copied().unwrap_or(0);
    let retracted: i64 = row.get_value(12)?.as_integer().copied().unwrap_or(0);
    let created_at: i64 = row.get_value(13)?.as_integer().copied().unwrap_or(0);

    let object = match object_type.as_str() {
        "iri" => Object::Iri(object_opt.ok_or("Missing IRI object")?),
        "blank" => Object::Blank(object_opt.ok_or("Missing blank node object")?),
        "literal" => {
            if let Some(int) = object_integer {
                Object::Integer(int)
            } else if let Some(num) = object_number {
                Object::Number(num)
            } else if object_datatype.as_deref() == Some("xsd:dateTime") {
                Object::DateTime(object_value.ok_or("Missing dateTime object_value")?)
            } else if object_datatype.as_deref() == Some("xsd:date") {
                Object::Literal {
                    value: object_value.ok_or("Missing date object_value")?,
                    datatype: object_datatype,
                    language: object_language,
                }
            } else if let Some(bool_val) = object_boolean {
                Object::Boolean(bool_val != 0)
            } else {
                Object::Literal {
                    value: object_value.ok_or("Missing literal object_value")?,
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

    async fn setup_test_data(conn: &mut Connection) -> i64 {
        let triples = create_test_triples();
        assert_triples(conn, &triples, "test").await.unwrap()
    }

    #[tokio::test]
    async fn test_get_by_entity() {
        let mut conn = setup_test_db().await;
        setup_test_data(&mut conn).await;

        let result = get_by_entity(&conn, "foundation:TestClass").await.unwrap();
        assert_eq!(result.triples.len(), 2);
    }

    #[tokio::test]
    async fn test_get_by_entity_nonexistent() {
        let mut conn = setup_test_db().await;
        setup_test_data(&mut conn).await;

        let result = get_by_entity(&conn, "foundation:NonExistent").await.unwrap();
        assert_eq!(result.triples.len(), 0);
    }

    #[tokio::test]
    async fn test_get_by_predicate() {
        let mut conn = setup_test_db().await;
        setup_test_data(&mut conn).await;

        let result = get_by_predicate(&conn, "rdf:type").await.unwrap();
        assert_eq!(result.triples.len(), 1);
    }

    #[tokio::test]
    async fn test_get_by_entity_predicate() {
        let mut conn = setup_test_db().await;
        setup_test_data(&mut conn).await;

        let result = get_by_entity_predicate(&conn, "foundation:TestClass", "rdfs:label").await.unwrap();
        assert_eq!(result.triples.len(), 1);

        let triple = &result.triples[0];
        match &triple.object {
            Object::Literal { value, .. } => assert_eq!(value, "Test Class"),
            _ => panic!("Expected literal object"),
        }
    }

    #[tokio::test]
    async fn test_get_at_time() {
        let mut conn = setup_test_db().await;
        let tx_id = setup_test_data(&mut conn).await;

        let result = get_at_time(&conn, "foundation:TestClass", tx_id).await.unwrap();
        assert_eq!(result.triples.len(), 2);
    }

    #[tokio::test]
    async fn test_get_at_time_temporal_snapshot() {
        let mut conn = setup_test_db().await;
        let _tx1 = setup_test_data(&mut conn).await;

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
        let tx2 = assert_triples(&mut conn, &updated_triple, "test").await.unwrap();

        let result = get_at_time(&conn, "foundation:TestClass", tx2).await.unwrap();

        assert_eq!(result.triples.len(), 2);

        let label_triple = result.triples.iter()
            .find(|t| t.predicate == "rdfs:label")
            .expect("Should have label");

        match &label_triple.object {
            Object::Literal { value, .. } => assert_eq!(value, "Updated Label"),
            _ => panic!("Expected literal"),
        }
    }

    #[tokio::test]
    async fn test_get_by_origin() {
        let mut conn = setup_test_db().await;
        setup_test_data(&mut conn).await;

        let result = get_by_origin(&conn, 1).await.unwrap();
        assert!(result.triples.len() > 0);
    }

    #[tokio::test]
    async fn test_get_history() {
        let mut conn = setup_test_db().await;
        let tx1 = setup_test_data(&mut conn).await;

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
        let tx2 = assert_triples(&mut conn, &new_triple, "test").await.unwrap();

        let history = get_history(&conn, "foundation:TestClass").await.unwrap();

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].0, tx1);
        assert_eq!(history[1].0, tx2);
        assert_eq!(history[0].1.len(), 2);
        assert_eq!(history[1].1.len(), 1);
    }

    #[tokio::test]
    async fn test_row_to_triple_with_iri() {
        let mut conn = setup_test_db().await;
        setup_test_data(&mut conn).await;

        let result = get_by_entity(&conn, "foundation:TestClass").await.unwrap();
        let iri_triple = result.triples.iter()
            .find(|t| t.predicate == "rdf:type")
            .expect("Should have rdf:type");

        match &iri_triple.object {
            Object::Iri(iri) => assert_eq!(iri, "owl:Class"),
            _ => panic!("Expected IRI object"),
        }
    }

    #[tokio::test]
    async fn test_row_to_triple_with_integer() {
        let mut conn = setup_test_db().await;
        setup_test_data(&mut conn).await;

        let result = get_by_entity(&conn, "foundation:TestProperty").await.unwrap();
        let int_triple = result.triples.iter()
            .find(|t| t.predicate == "foundation:someValue")
            .expect("Should have foundation:someValue");

        match &int_triple.object {
            Object::Integer(i) => assert_eq!(*i, 42),
            _ => panic!("Expected Integer object"),
        }
    }

    #[tokio::test]
    async fn test_find_by_class_iris_and_properties_returns_subclass_instances() {
        let mut conn = setup_test_db().await;

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Animal".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("owl:Class".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").await.unwrap();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Dog".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("owl:Class".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:Dog".to_string(), predicate: "rdfs:subClassOf".to_string(),
                object: Object::Iri("foundation:Animal".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").await.unwrap();

        assert_triples(&mut conn, &[
            Triple { subject: "foundation:Rex".to_string(), predicate: "rdf:type".to_string(),
                object: Object::Iri("foundation:Dog".to_string()), tx: 0, created_at: 0, origin_id: 1, retracted: false },
            Triple { subject: "foundation:Rex".to_string(), predicate: "foundation:name".to_string(),
                object: Object::Literal { value: "Rex".to_string(), datatype: Some("xsd:string".to_string()), language: None },
                tx: 0, created_at: 0, origin_id: 1, retracted: false },
        ], "test").await.unwrap();

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn,
            &["foundation:Animal", "foundation:Dog"],
            &[("foundation:name", "Rex", "=")],
            false,
            100,
            0,
        ).await.unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"foundation:Rex".to_string()));
    }

    #[tokio::test]
    async fn test_find_by_class_iris_single_class_filters_by_property() {
        let mut conn = setup_test_db().await;
        setup_test_data(&mut conn).await;

        let (results, total) = find_by_class_iris_and_properties_with_options(
            &conn,
            &["owl:Class"],
            &[("rdfs:label", "Test Class", "=")],
            false,
            100,
            0,
        ).await.unwrap();

        assert_eq!(total, 1);
        assert!(results.contains(&"foundation:TestClass".to_string()));
    }

    #[tokio::test]
    async fn test_find_by_properties_without_class_constraint() {
        let mut conn = setup_test_db().await;

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
        ], "test").await.unwrap();

        let (results, total) = find_by_properties_with_options(
            &conn,
            &[("foundation:status", "active", "=")],
            false,
            100,
            0,
        ).await.unwrap();

        assert_eq!(total, 2, "should find both active entities regardless of class");
        assert!(results.contains(&"foundation:PersonA".to_string()));
        assert!(results.contains(&"foundation:CompanyX".to_string()));
        assert!(!results.contains(&"foundation:PersonB".to_string()));
    }
}
