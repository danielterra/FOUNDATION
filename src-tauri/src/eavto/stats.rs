// ============================================================================
// EAVTO Statistics Module
// ============================================================================
// Provides database statistics and metrics
// ============================================================================

use rusqlite::Connection;
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
pub fn get_stats(conn: &Connection) -> Result<DbStats, DbError> {
    let total_facts: u64 = conn.query_row(
        "SELECT COUNT(*) FROM triples",
        [],
        |row| row.get(0)
    )?;

    let active_facts: u64 = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE retracted = 0",
        [],
        |row| row.get(0)
    )?;

    let total_transactions: u64 = conn.query_row(
        "SELECT COUNT(*) FROM transactions",
        [],
        |row| row.get(0)
    )?;

    let entities_count: u64 = conn.query_row(
        "SELECT COUNT(DISTINCT subject) FROM triples WHERE retracted = 0",
        [],
        |row| row.get(0)
    )?;

    let ontology_imported_str: String = conn.query_row(
        "SELECT value FROM metadata WHERE key = 'ontology_imported'",
        [],
        |row| row.get(0)
    ).unwrap_or_else(|_| "false".to_string());

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

    #[test]
    fn test_get_stats_empty_db() {
        let conn = setup_test_db();
        let stats = get_stats(&conn).expect("Failed to get stats");

        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.active_facts, 0);
        assert_eq!(stats.total_transactions, 0);
        assert_eq!(stats.entities_count, 0);
        assert_eq!(stats.ontology_imported, false);
    }

    #[test]
    fn test_get_stats_with_data() {
        let mut conn = setup_test_db();

        // Insert test transaction
        conn.execute(
            "INSERT INTO transactions (origin, created_at) VALUES ('test', 1000)",
            [],
        )
        .unwrap();
        let tx_id = conn.last_insert_rowid();

        // Insert test triples
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted)
             VALUES ('foundation:TestClass', 'rdf:type', 'owl:Class', 'iri', ?, 1, 1000, 0)",
            [tx_id],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted)
             VALUES ('foundation:TestClass', 'rdfs:label', 'owl:Class', 'iri', ?, 1, 1000, 1)",
            [tx_id],
        )
        .unwrap();

        let stats = get_stats(&conn).expect("Failed to get stats");

        assert_eq!(stats.total_facts, 2);
        assert_eq!(stats.active_facts, 1); // Only one non-retracted
        assert_eq!(stats.total_transactions, 1);
        assert_eq!(stats.entities_count, 1);
    }
}
