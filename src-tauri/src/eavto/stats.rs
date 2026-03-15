// ============================================================================
// EAVTO Statistics Module
// ============================================================================
// Provides database statistics and metrics
// ============================================================================

use turso::Connection;
use super::connection::DbError;

/// Database statistics
#[derive(Debug, serde::Serialize)]
pub struct DbStats {
    pub total_facts: u64,
    pub active_facts: u64,
    pub total_transactions: u64,
    pub entities_count: u64,
    pub ontology_imported: bool,
}

/// Get database statistics
pub async fn get_stats(conn: &Connection) -> Result<DbStats, DbError> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM triples").await.map_err(|e| DbError::SchemaError(e.to_string()))?;
    let row = stmt.query_row(()).await.map_err(|e| DbError::SchemaError(e.to_string()))?;
    let total_facts: u64 = row.get_value(0).map_err(|e| DbError::SchemaError(e.to_string()))?.as_integer().copied().unwrap_or(0) as u64;

    let mut stmt = conn.prepare("SELECT COUNT(*) FROM triples WHERE retracted = 0").await.map_err(|e| DbError::SchemaError(e.to_string()))?;
    let row = stmt.query_row(()).await.map_err(|e| DbError::SchemaError(e.to_string()))?;
    let active_facts: u64 = row.get_value(0).map_err(|e| DbError::SchemaError(e.to_string()))?.as_integer().copied().unwrap_or(0) as u64;

    let mut stmt = conn.prepare("SELECT COUNT(*) FROM transactions").await.map_err(|e| DbError::SchemaError(e.to_string()))?;
    let row = stmt.query_row(()).await.map_err(|e| DbError::SchemaError(e.to_string()))?;
    let total_transactions: u64 = row.get_value(0).map_err(|e| DbError::SchemaError(e.to_string()))?.as_integer().copied().unwrap_or(0) as u64;

    let mut stmt = conn.prepare("SELECT COUNT(DISTINCT subject) FROM triples WHERE retracted = 0").await.map_err(|e| DbError::SchemaError(e.to_string()))?;
    let row = stmt.query_row(()).await.map_err(|e| DbError::SchemaError(e.to_string()))?;
    let entities_count: u64 = row.get_value(0).map_err(|e| DbError::SchemaError(e.to_string()))?.as_integer().copied().unwrap_or(0) as u64;

    let ontology_imported_str: String = {
        let mut stmt = conn.prepare("SELECT value FROM metadata WHERE key = 'ontology_imported'").await.map_err(|e| DbError::SchemaError(e.to_string()))?;
        match stmt.query_row(()).await {
            Ok(row) => row.get_value(0).map_err(|e| DbError::SchemaError(e.to_string()))?.as_text().cloned().unwrap_or_else(|| "false".to_string()),
            Err(_) => "false".to_string(),
        }
    };

    let ontology_imported = ontology_imported_str == "true";

    Ok(DbStats {
        total_facts,
        active_facts,
        total_transactions,
        entities_count,
        ontology_imported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    #[tokio::test]
    async fn test_get_stats_empty_db() {
        let conn = setup_test_db().await;
        let stats = get_stats(&conn).await.expect("Failed to get stats");

        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.active_facts, 0);
        assert_eq!(stats.total_transactions, 0);
        assert_eq!(stats.entities_count, 0);
        assert_eq!(stats.ontology_imported, false);
    }

    #[tokio::test]
    async fn test_get_stats_with_data() {
        let conn = setup_test_db().await;

        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 1000)",
            turso::params![],
        ).await.unwrap();

        let mut stmt = conn.prepare("SELECT last_insert_rowid()").await.unwrap();
        let row = stmt.query_row(()).await.unwrap();
        let tx_id: i64 = row.get_value(0).unwrap().as_integer().copied().unwrap_or(0);

        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, tx, origin_id, created_at, retracted) \
             VALUES ('foundation:TestClass', 'rdf:type', 'owl:Class', 'iri', ?, 1, 1000, 0)",
            turso::params![tx_id],
        ).await.unwrap();

        conn.execute(
            "INSERT INTO triples \
             (subject, predicate, object, object_type, tx, origin_id, created_at, retracted) \
             VALUES ('foundation:TestClass', 'rdfs:label', 'owl:Class', 'iri', ?, 1, 1000, 1)",
            turso::params![tx_id],
        ).await.unwrap();

        let stats = get_stats(&conn).await.expect("Failed to get stats");

        assert_eq!(stats.total_facts, 2);
        assert_eq!(stats.active_facts, 1);
        assert_eq!(stats.total_transactions, 1);
        assert_eq!(stats.entities_count, 1);
    }
}
