/// QueryResult Type
///
/// Represents the result of a query operation

use super::triple_type::Triple;

/// Query result containing triples and metadata
#[derive(Debug)]
pub struct QueryResult {
    pub triples: Vec<Triple>,
    pub count: usize,
}

impl QueryResult {
    /// Create a new QueryResult from triples
    pub fn new(triples: Vec<Triple>) -> Self {
        let count = triples.len();
        Self { triples, count }
    }

    /// Create an empty QueryResult
    pub fn empty() -> Self {
        Self {
            triples: Vec::new(),
            count: 0,
        }
    }

    /// Check if result is empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get first triple if exists
    pub fn first(&self) -> Option<&Triple> {
        self.triples.first()
    }

    /// Filter triples by predicate
    pub fn filter_by_predicate(&self, predicate: &str) -> Vec<&Triple> {
        self.triples
            .iter()
            .filter(|t| t.predicate == predicate)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::Object;

    fn create_test_triple(subject: &str, predicate: &str) -> Triple {
        Triple::new(subject, predicate, Object::Iri("test:Object".to_string()))
    }

    #[test]
    fn test_query_result_new() {
        let triples = vec![
            create_test_triple("test:S1", "test:P1"),
            create_test_triple("test:S2", "test:P2"),
        ];

        let result = QueryResult::new(triples);

        assert_eq!(result.count, 2);
        assert_eq!(result.triples.len(), 2);
    }

    #[test]
    fn test_query_result_empty() {
        let result = QueryResult::empty();

        assert_eq!(result.count, 0);
        assert_eq!(result.triples.len(), 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_query_result_is_empty() {
        let empty_result = QueryResult::empty();
        assert!(empty_result.is_empty());

        let non_empty_result = QueryResult::new(vec![create_test_triple("test:S", "test:P")]);
        assert!(!non_empty_result.is_empty());
    }

    #[test]
    fn test_query_result_first() {
        let empty_result = QueryResult::empty();
        assert!(empty_result.first().is_none());

        let triple = create_test_triple("test:Subject", "test:predicate");
        let result = QueryResult::new(vec![triple]);

        let first = result.first().unwrap();
        assert_eq!(first.subject, "test:Subject");
    }

    #[test]
    fn test_query_result_filter_by_predicate() {
        let triples = vec![
            create_test_triple("test:S1", "rdf:type"),
            create_test_triple("test:S2", "rdfs:label"),
            create_test_triple("test:S3", "rdf:type"),
        ];

        let result = QueryResult::new(triples);
        let filtered = result.filter_by_predicate("rdf:type");

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].predicate, "rdf:type");
        assert_eq!(filtered[1].predicate, "rdf:type");
    }

    #[test]
    fn test_query_result_filter_no_matches() {
        let triples = vec![create_test_triple("test:S1", "rdf:type")];
        let result = QueryResult::new(triples);
        let filtered = result.filter_by_predicate("nonexistent:predicate");

        assert_eq!(filtered.len(), 0);
    }
}
