/// EVTO Query Functions
///
/// Pure functions for querying the triple store

use rusqlite::{Connection, Row};
use super::triple_type::Triple;
use super::object_type::Object;
use super::query_result_type::QueryResult;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Query triples by entity (E - subject)
pub fn get_by_entity(conn: &Connection, entity: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND retracted = 0
         ORDER BY tx DESC"
    )?;

    let triples = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

/// Query triples by predicate (V - value/property)
pub fn get_by_predicate(conn: &Connection, predicate: &str) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE predicate = ? AND retracted = 0
         ORDER BY tx DESC"
    )?;

    let triples = stmt
        .query_map([predicate], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

/// Query triples by subject and predicate (EV)
pub fn get_by_entity_predicate(
    conn: &Connection,
    entity: &str,
    predicate: &str,
) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND predicate = ? AND retracted = 0
         ORDER BY tx DESC"
    )?;

    let triples = stmt
        .query_map([entity, predicate], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(QueryResult::new(triples))
}

/// Query entity state at specific time (ET - temporal query)
pub fn get_at_time(conn: &Connection, entity: &str, tx: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ? AND tx <= ? AND retracted = 0
         ORDER BY predicate, tx DESC"
    )?;

    let triples = stmt
        .query_map([entity, tx.to_string().as_str()], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Keep only the latest value for each predicate (temporal snapshot)
    let mut seen_predicates = std::collections::HashSet::new();
    let snapshot: Vec<Triple> = triples
        .into_iter()
        .filter(|t| seen_predicates.insert(t.predicate.clone()))
        .collect();

    Ok(QueryResult::new(snapshot))
}

/// Query triples by origin (O)
pub fn get_by_origin(conn: &Connection, origin_id: i64) -> Result<QueryResult> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
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

/// Query complete history of an entity (all transactions)
pub fn get_history(conn: &Connection, entity: &str) -> Result<Vec<(i64, Vec<Triple>)>> {
    let mut stmt = conn.prepare(
        "SELECT subject, predicate, object, object_value, object_datatype, object_language,
                object_type, object_number, object_integer, object_datetime, object_boolean,
                tx, origin_id, retracted, created_at
         FROM triples
         WHERE subject = ?
         ORDER BY tx ASC"
    )?;

    let all_triples: Vec<Triple> = stmt
        .query_map([entity], row_to_triple)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Group by transaction
    let mut history: std::collections::HashMap<i64, Vec<Triple>> = std::collections::HashMap::new();
    for triple in all_triples {
        history.entry(triple.tx).or_insert_with(Vec::new).push(triple);
    }

    let mut result: Vec<(i64, Vec<Triple>)> = history.into_iter().collect();
    result.sort_by_key(|(tx, _)| *tx);

    Ok(result)
}

/// Convert SQLite row to Triple
fn row_to_triple(row: &Row) -> rusqlite::Result<Triple> {
    let subject: String = row.get(0)?;
    let predicate: String = row.get(1)?;
    let object_opt: Option<String> = row.get(2)?;
    let object_value: Option<String> = row.get(3)?;
    let object_datatype: Option<String> = row.get(4)?;
    let object_language: Option<String> = row.get(5)?;
    let object_type: String = row.get(6)?;
    let object_number: Option<f64> = row.get(7)?;
    let object_integer: Option<i64> = row.get(8)?;
    let object_datetime: Option<i64> = row.get(9)?;
    let object_boolean: Option<i64> = row.get(10)?;
    let tx: i64 = row.get(11)?;
    let origin_id: i64 = row.get(12)?;
    let retracted: i64 = row.get(13)?;
    let created_at: i64 = row.get(14)?;

    let object = match object_type.as_str() {
        "iri" => Object::Iri(object_opt.unwrap()),
        "blank" => Object::Blank(object_opt.unwrap()),
        "literal" => {
            // Check for typed literals
            if let Some(int) = object_integer {
                Object::Integer(int)
            } else if let Some(num) = object_number {
                Object::Number(num)
            } else if let Some(dt) = object_datetime {
                Object::DateTime(dt)
            } else if let Some(bool_val) = object_boolean {
                Object::Boolean(bool_val != 0)
            } else {
                // Generic literal
                Object::Literal {
                    value: object_value.unwrap(),
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
        assert_eq!(result.triples.len(), 2); // rdf:type and rdfs:label
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
        assert_eq!(result.triples.len(), 2); // Should have both triples at this tx
    }

    #[test]
    fn test_get_at_time_temporal_snapshot() {
        let mut conn = setup_test_db();
        let tx1 = setup_test_data(&mut conn);

        // Add new triple with same predicate (update)
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

        // Query at tx2 should give latest label
        let result = get_at_time(&conn, "foundation:TestClass", tx2).unwrap();

        // Should only have 2 triples (latest rdfs:label + rdf:type)
        assert_eq!(result.triples.len(), 2);

        // Find the label triple
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

        // Origin ID 1 is "test" from setup_test_db
        let result = get_by_origin(&conn, 1).unwrap();
        assert!(result.triples.len() > 0);
    }

    #[test]
    fn test_get_history() {
        let mut conn = setup_test_db();
        let tx1 = setup_test_data(&mut conn);

        // Add another triple in a new transaction
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

        assert_eq!(history.len(), 2); // Two transactions
        assert_eq!(history[0].0, tx1); // First transaction
        assert_eq!(history[1].0, tx2); // Second transaction
        assert_eq!(history[0].1.len(), 2); // First tx has 2 triples
        assert_eq!(history[1].1.len(), 1); // Second tx has 1 triple
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
}
