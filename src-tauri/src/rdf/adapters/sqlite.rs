// ============================================================================
// SQLite RDF Adapter
// ============================================================================
// Implements RDFStore trait for SQLite storage backend
// Uses the `triples` table with RDF-native schema
// ============================================================================

use crate::rdf::store::{RDFStore, RDFStoreError, QueryPattern, Transaction, StoreStats};
use crate::rdf::types::{Subject, Predicate, Object, Triple, Datatype};
use crate::rdf::validation::RDFValidator;
use rusqlite::{Connection, params};

/// SQLite adapter for RDF triple store
pub struct SQLiteRDFAdapter {
    conn: Connection,
}

impl SQLiteRDFAdapter {
    /// Create new adapter with existing connection
    pub fn new(conn: Connection) -> Self {
        SQLiteRDFAdapter { conn }
    }

    /// Get next transaction ID
    fn next_tx_id(&self) -> Result<i64, RDFStoreError> {
        let tx_id: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(tx), 0) + 1 FROM transactions",
                [],
                |row| row.get(0),
            )
            .map_err(|e| RDFStoreError::StorageError(format!("Failed to get next tx ID: {}", e)))?;
        Ok(tx_id)
    }

    /// Convert Subject to (TEXT, object_type)
    fn subject_to_sql(&self, subject: &Subject) -> (String, &str) {
        match subject {
            Subject::IRI(iri) => (iri.clone(), "iri"),
            Subject::BlankNode(id) => (id.clone(), "blank"),
        }
    }

    /// Convert Object to SQL parameters
    fn object_to_sql(
        &self,
        object: &Object,
    ) -> (
        Option<String>, // object (IRI/blank)
        Option<String>, // object_value (literal)
        Option<String>, // object_datatype
        Option<String>, // object_language
        &str,           // object_type
        Option<f64>,    // object_number
        Option<i64>,    // object_integer
        Option<i64>,    // object_datetime
        Option<i32>,    // object_boolean
    ) {
        match object {
            Object::IRI(iri) => (
                Some(iri.clone()),
                None,
                None,
                None,
                "iri",
                None,
                None,
                None,
                None,
            ),
            Object::BlankNode(id) => (
                Some(id.clone()),
                None,
                None,
                None,
                "blank",
                None,
                None,
                None,
                None,
            ),
            Object::Literal {
                value,
                datatype,
                language,
            } => {
                let datatype_iri = datatype.as_iri().to_string();
                let lang = language.as_ref().map(|l| l.as_str().to_string());

                // Parse typed values for performance columns
                let number = match datatype {
                    Datatype::XsdDecimal | Datatype::XsdDouble | Datatype::XsdFloat => {
                        value.parse::<f64>().ok()
                    }
                    _ => None,
                };

                let integer = match datatype {
                    Datatype::XsdInteger | Datatype::XsdInt | Datatype::XsdLong => {
                        value.parse::<i64>().ok()
                    }
                    _ => None,
                };

                let boolean = match datatype {
                    Datatype::XsdBoolean => match value.as_str() {
                        "true" | "1" => Some(1),
                        "false" | "0" => Some(0),
                        _ => None,
                    },
                    _ => None,
                };

                // TODO: Parse datetime to Unix epoch milliseconds
                let datetime = None;

                (
                    None,
                    Some(value.clone()),
                    Some(datatype_iri),
                    lang,
                    "literal",
                    number,
                    integer,
                    datetime,
                    boolean,
                )
            }
        }
    }

    /// Convert SQL row to Triple
    fn row_to_triple(&self, row: &rusqlite::Row) -> Result<Triple, rusqlite::Error> {
        let subject_str: String = row.get("subject")?;
        let predicate_str: String = row.get("predicate")?;
        let object_type: String = row.get("object_type")?;

        // Parse subject
        let subject = if subject_str.starts_with("_:") {
            Subject::BlankNode(subject_str)
        } else {
            Subject::IRI(subject_str)
        };

        // Parse predicate
        let predicate = Predicate::new(predicate_str);

        // Parse object based on type
        let object = match object_type.as_str() {
            "iri" => {
                let iri: String = row.get("object")?;
                Object::IRI(iri)
            }
            "blank" => {
                let id: String = row.get("object")?;
                Object::BlankNode(id)
            }
            "literal" => {
                let value: String = row.get("object_value")?;
                let datatype_iri: String = row.get("object_datatype")?;
                let language: Option<String> = row.get("object_language")?;

                Object::Literal {
                    value,
                    datatype: Datatype::from_iri(&datatype_iri),
                    language: language.map(|l| l.into()),
                }
            }
            _ => {
                return Err(rusqlite::Error::InvalidQuery);
            }
        };

        Ok(Triple::new(subject, predicate, object))
    }
}

impl RDFStore for SQLiteRDFAdapter {
    fn insert_triple(&mut self, triple: Triple, origin: &str) -> Result<i64, RDFStoreError> {
        // Validate triple before insertion
        RDFValidator::validate_triple(&triple)
            .map_err(|e| RDFStoreError::ValidationError(e.to_string()))?;

        let tx_id = self.next_tx_id()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Begin transaction
        self.conn
            .execute("BEGIN TRANSACTION", [])
            .map_err(|e| RDFStoreError::TransactionError(e.to_string()))?;

        // Insert transaction record
        self.conn
            .execute(
                "INSERT INTO transactions (tx, origin, created_at) VALUES (?, ?, ?)",
                params![tx_id, origin, now],
            )
            .map_err(|e| RDFStoreError::StorageError(e.to_string()))?;

        // Convert triple to SQL parameters
        let (subject_str, _) = self.subject_to_sql(&triple.subject);
        let predicate_str = triple.predicate.as_str();
        let (object_iri, object_value, object_datatype, object_language, object_type, number, integer, datetime, boolean) =
            self.object_to_sql(&triple.object);

        // Insert triple
        self.conn
            .execute(
                "INSERT INTO triples (subject, predicate, object, object_value, object_datatype, object_language, object_type, object_number, object_integer, object_datetime, object_boolean, tx, origin, retracted, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
                params![
                    subject_str,
                    predicate_str,
                    object_iri,
                    object_value,
                    object_datatype,
                    object_language,
                    object_type,
                    number,
                    integer,
                    datetime,
                    boolean,
                    tx_id,
                    origin,
                    now,
                ],
            )
            .map_err(|e| RDFStoreError::StorageError(e.to_string()))?;

        // Commit transaction
        self.conn
            .execute("COMMIT", [])
            .map_err(|e| RDFStoreError::TransactionError(e.to_string()))?;

        Ok(tx_id)
    }

    fn insert_triples(&mut self, triples: Vec<Triple>, origin: &str) -> Result<i64, RDFStoreError> {
        // Validate all triples first
        for triple in &triples {
            RDFValidator::validate_triple(triple)
                .map_err(|e| RDFStoreError::ValidationError(e.to_string()))?;
        }

        let tx_id = self.next_tx_id()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Begin transaction
        self.conn
            .execute("BEGIN TRANSACTION", [])
            .map_err(|e| RDFStoreError::TransactionError(e.to_string()))?;

        // Insert transaction record
        self.conn
            .execute(
                "INSERT INTO transactions (tx, origin, created_at) VALUES (?, ?, ?)",
                params![tx_id, origin, now],
            )
            .map_err(|e| RDFStoreError::StorageError(e.to_string()))?;

        // Insert all triples
        for triple in triples {
            let (subject_str, _) = self.subject_to_sql(&triple.subject);
            let predicate_str = triple.predicate.as_str();
            let (object_iri, object_value, object_datatype, object_language, object_type, number, integer, datetime, boolean) =
                self.object_to_sql(&triple.object);

            self.conn
                .execute(
                    "INSERT INTO triples (subject, predicate, object, object_value, object_datatype, object_language, object_type, object_number, object_integer, object_datetime, object_boolean, tx, origin, retracted, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
                    params![
                        subject_str,
                        predicate_str,
                        object_iri,
                        object_value,
                        object_datatype,
                        object_language,
                        object_type,
                        number,
                        integer,
                        datetime,
                        boolean,
                        tx_id,
                        origin,
                        now,
                    ],
                )
                .map_err(|e| RDFStoreError::StorageError(e.to_string()))?;
        }

        // Commit transaction
        self.conn
            .execute("COMMIT", [])
            .map_err(|e| RDFStoreError::TransactionError(e.to_string()))?;

        Ok(tx_id)
    }

    fn retract_triple(&mut self, triple: Triple, origin: &str) -> Result<i64, RDFStoreError> {
        let tx_id = self.next_tx_id()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Begin transaction
        self.conn
            .execute("BEGIN TRANSACTION", [])
            .map_err(|e| RDFStoreError::TransactionError(e.to_string()))?;

        // Insert transaction record
        self.conn
            .execute(
                "INSERT INTO transactions (tx, origin, created_at) VALUES (?, ?, ?)",
                params![tx_id, origin, now],
            )
            .map_err(|e| RDFStoreError::StorageError(e.to_string()))?;

        // Update existing triple to mark as retracted
        let (subject_str, _) = self.subject_to_sql(&triple.subject);
        let predicate_str = triple.predicate.as_str();

        let (object_iri, object_value, _, _, object_type, _, _, _, _) =
            self.object_to_sql(&triple.object);

        let rows_updated = if object_type == "iri" || object_type == "blank" {
            self.conn
                .execute(
                    "UPDATE triples SET retracted = 1 WHERE subject = ? AND predicate = ? AND object = ? AND retracted = 0",
                    params![subject_str, predicate_str, object_iri],
                )
                .map_err(|e| RDFStoreError::StorageError(e.to_string()))?
        } else {
            self.conn
                .execute(
                    "UPDATE triples SET retracted = 1 WHERE subject = ? AND predicate = ? AND object_value = ? AND retracted = 0",
                    params![subject_str, predicate_str, object_value],
                )
                .map_err(|e| RDFStoreError::StorageError(e.to_string()))?
        };

        if rows_updated == 0 {
            self.conn.execute("ROLLBACK", []).ok();
            return Err(RDFStoreError::StorageError(
                "Triple not found or already retracted".to_string(),
            ));
        }

        // Commit transaction
        self.conn
            .execute("COMMIT", [])
            .map_err(|e| RDFStoreError::TransactionError(e.to_string()))?;

        Ok(tx_id)
    }

    fn query(&self, pattern: QueryPattern) -> Result<Vec<Triple>, RDFStoreError> {
        let mut sql = String::from(
            "SELECT subject, predicate, object, object_value, object_datatype, object_language, object_type
             FROM triples
             WHERE retracted = 0",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref subject) = pattern.subject {
            sql.push_str(" AND subject = ?");
            let (subject_str, _) = self.subject_to_sql(subject);
            params.push(Box::new(subject_str));
        }

        if let Some(ref predicate) = pattern.predicate {
            sql.push_str(" AND predicate = ?");
            params.push(Box::new(predicate.as_str().to_string()));
        }

        if let Some(ref object) = pattern.object {
            let (object_iri, object_value, _, _, object_type, _, _, _, _) =
                self.object_to_sql(object);

            if object_type == "iri" || object_type == "blank" {
                sql.push_str(" AND object = ?");
                params.push(Box::new(object_iri));
            } else {
                sql.push_str(" AND object_value = ?");
                params.push(Box::new(object_value));
            }
        }

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| RDFStoreError::QueryError(e.to_string()))?;

        let triples: Result<Vec<Triple>, _> = stmt
            .query_map(param_refs.as_slice(), |row| self.row_to_triple(row))
            .map_err(|e| RDFStoreError::QueryError(e.to_string()))?
            .collect();

        triples.map_err(|e| RDFStoreError::QueryError(e.to_string()))
    }

    fn query_with_metadata(
        &self,
        pattern: QueryPattern,
    ) -> Result<Vec<(Triple, Transaction)>, RDFStoreError> {
        let mut sql = String::from(
            "SELECT t.subject, t.predicate, t.object, t.object_value, t.object_datatype, t.object_language, t.object_type,
                    t.tx, t.origin, t.created_at
             FROM triples t
             WHERE t.retracted = 0",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref subject) = pattern.subject {
            sql.push_str(" AND t.subject = ?");
            let (subject_str, _) = self.subject_to_sql(subject);
            params.push(Box::new(subject_str));
        }

        if let Some(ref predicate) = pattern.predicate {
            sql.push_str(" AND t.predicate = ?");
            params.push(Box::new(predicate.as_str().to_string()));
        }

        if let Some(ref object) = pattern.object {
            let (object_iri, object_value, _, _, object_type, _, _, _, _) =
                self.object_to_sql(object);

            if object_type == "iri" || object_type == "blank" {
                sql.push_str(" AND t.object = ?");
                params.push(Box::new(object_iri));
            } else {
                sql.push_str(" AND t.object_value = ?");
                params.push(Box::new(object_value));
            }
        }

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| RDFStoreError::QueryError(e.to_string()))?;

        let results: Result<Vec<(Triple, Transaction)>, _> = stmt
            .query_map(param_refs.as_slice(), |row| {
                let triple = self.row_to_triple(row)?;
                let tx = Transaction {
                    id: row.get("tx")?,
                    origin: row.get("origin")?,
                    created_at: row.get("created_at")?,
                };
                Ok((triple, tx))
            })
            .map_err(|e| RDFStoreError::QueryError(e.to_string()))?
            .collect();

        results.map_err(|e| RDFStoreError::QueryError(e.to_string()))
    }

    fn count_total(&self) -> Result<u64, RDFStoreError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM triples", [], |row| row.get(0))
            .map_err(|e| RDFStoreError::QueryError(e.to_string()))?;
        Ok(count as u64)
    }

    fn count_active(&self) -> Result<u64, RDFStoreError> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE retracted = 0",
                [],
                |row| row.get(0),
            )
            .map_err(|e| RDFStoreError::QueryError(e.to_string()))?;
        Ok(count as u64)
    }

    fn get_stats(&self) -> Result<StoreStats, RDFStoreError> {
        let total_triples = self.count_total()?;
        let active_triples = self.count_active()?;

        let total_transactions: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            .map_err(|e| RDFStoreError::QueryError(e.to_string()))?;

        let subjects_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(DISTINCT subject) FROM triples WHERE retracted = 0",
                [],
                |row| row.get(0),
            )
            .map_err(|e| RDFStoreError::QueryError(e.to_string()))?;

        let ontology_imported_str: String = self
            .conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'ontology_imported'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "false".to_string());

        Ok(StoreStats {
            total_triples,
            active_triples,
            total_transactions: total_transactions as u64,
            subjects_count: subjects_count as u64,
            ontology_imported: ontology_imported_str == "true",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn create_test_store() -> SQLiteRDFAdapter {
        let conn = Connection::open_in_memory().unwrap();

        // Create schema
        conn.execute_batch(include_str!("../../../../db/schema.sql"))
            .unwrap();

        SQLiteRDFAdapter::new(conn)
    }

    #[test]
    fn test_insert_and_query_triple() {
        let mut store = create_test_store();

        let triple = Triple::from_iris(
            "http://example.org/subject",
            "http://example.org/predicate",
            "http://example.org/object",
        );

        let tx_id = store.insert_triple(triple.clone(), "test").unwrap();
        assert!(tx_id > 0);

        let results = store
            .query(QueryPattern::with_subject(Subject::iri(
                "http://example.org/subject",
            )))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], triple);
    }

    #[test]
    fn test_insert_literal_triple() {
        let mut store = create_test_store();

        let triple = Triple::new(
            Subject::iri("http://example.org/person"),
            Predicate::new("http://example.org/hasAge"),
            Object::integer_literal(25),
        );

        store.insert_triple(triple.clone(), "test").unwrap();

        let results = store
            .query(QueryPattern::with_subject(Subject::iri(
                "http://example.org/person",
            )))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], triple);
    }

    #[test]
    fn test_retract_triple() {
        let mut store = create_test_store();

        let triple = Triple::from_iris(
            "http://example.org/subject",
            "http://example.org/predicate",
            "http://example.org/object",
        );

        store.insert_triple(triple.clone(), "test").unwrap();
        store.retract_triple(triple.clone(), "test").unwrap();

        let results = store
            .query(QueryPattern::with_subject(Subject::iri(
                "http://example.org/subject",
            )))
            .unwrap();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_validation_error() {
        let mut store = create_test_store();

        let invalid_triple = Triple::new(
            Subject::iri(""), // Empty IRI - invalid
            Predicate::new("http://example.org/predicate"),
            Object::iri("http://example.org/object"),
        );

        let result = store.insert_triple(invalid_triple, "test");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RDFStoreError::ValidationError(_)
        ));
    }
}
