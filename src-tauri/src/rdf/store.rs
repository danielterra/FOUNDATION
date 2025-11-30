// ============================================================================
// RDF Store Trait
// ============================================================================
// Storage-agnostic interface for RDF triple stores
// Allows swapping storage backends (SQLite, in-memory, remote, etc.)
// ============================================================================

use super::types::{Subject, Predicate, Object, Triple};
use std::error::Error;
use std::fmt;

/// RDF Store errors
#[derive(Debug)]
pub enum RDFStoreError {
    /// Validation error (triple is not RDF-compliant)
    ValidationError(String),
    /// Storage backend error
    StorageError(String),
    /// Transaction error
    TransactionError(String),
    /// Query error
    QueryError(String),
}

impl fmt::Display for RDFStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RDFStoreError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            RDFStoreError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            RDFStoreError::TransactionError(msg) => write!(f, "Transaction error: {}", msg),
            RDFStoreError::QueryError(msg) => write!(f, "Query error: {}", msg),
        }
    }
}

impl Error for RDFStoreError {}

/// Query pattern for triple matching
/// Use None to match any value in that position
#[derive(Debug, Clone)]
pub struct QueryPattern {
    pub subject: Option<Subject>,
    pub predicate: Option<Predicate>,
    pub object: Option<Object>,
}

impl QueryPattern {
    /// Create pattern matching specific subject
    pub fn with_subject(subject: Subject) -> Self {
        QueryPattern {
            subject: Some(subject),
            predicate: None,
            object: None,
        }
    }

    /// Create pattern matching specific predicate
    pub fn with_predicate(predicate: Predicate) -> Self {
        QueryPattern {
            subject: None,
            predicate: Some(predicate),
            object: None,
        }
    }

    /// Create pattern matching specific object
    pub fn with_object(object: Object) -> Self {
        QueryPattern {
            subject: None,
            predicate: None,
            object: Some(object),
        }
    }

    /// Create pattern matching subject and predicate
    pub fn with_sp(subject: Subject, predicate: Predicate) -> Self {
        QueryPattern {
            subject: Some(subject),
            predicate: Some(predicate),
            object: None,
        }
    }

    /// Create pattern matching all components
    pub fn exact(subject: Subject, predicate: Predicate, object: Object) -> Self {
        QueryPattern {
            subject: Some(subject),
            predicate: Some(predicate),
            object: Some(object),
        }
    }

    /// Create pattern matching everything (return all triples)
    pub fn all() -> Self {
        QueryPattern {
            subject: None,
            predicate: None,
            object: None,
        }
    }
}

/// Transaction metadata
#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: i64,
    pub origin: String,
    pub created_at: i64,
}

/// RDF Store trait - implement this for any storage backend
pub trait RDFStore {
    /// Insert a triple into the store
    /// Returns transaction ID
    fn insert_triple(
        &mut self,
        triple: Triple,
        origin: &str,
    ) -> Result<i64, RDFStoreError>;

    /// Insert multiple triples in a single transaction
    /// Returns transaction ID
    fn insert_triples(
        &mut self,
        triples: Vec<Triple>,
        origin: &str,
    ) -> Result<i64, RDFStoreError>;

    /// Retract a triple (mark as retracted, don't delete)
    /// Returns transaction ID
    fn retract_triple(
        &mut self,
        triple: Triple,
        origin: &str,
    ) -> Result<i64, RDFStoreError>;

    /// Query triples matching a pattern
    /// Only returns non-retracted triples
    fn query(
        &self,
        pattern: QueryPattern,
    ) -> Result<Vec<Triple>, RDFStoreError>;

    /// Query triples with transaction metadata
    fn query_with_metadata(
        &self,
        pattern: QueryPattern,
    ) -> Result<Vec<(Triple, Transaction)>, RDFStoreError>;

    /// Get all triples about a subject (SPO pattern)
    fn get_subject_triples(&self, subject: &Subject) -> Result<Vec<Triple>, RDFStoreError> {
        self.query(QueryPattern::with_subject(subject.clone()))
    }

    /// Get all triples with a predicate (POS pattern)
    fn get_predicate_triples(&self, predicate: &Predicate) -> Result<Vec<Triple>, RDFStoreError> {
        self.query(QueryPattern::with_predicate(predicate.clone()))
    }

    /// Get all triples referencing an object (OPS pattern)
    fn get_object_triples(&self, object: &Object) -> Result<Vec<Triple>, RDFStoreError> {
        self.query(QueryPattern::with_object(object.clone()))
    }

    /// Check if a triple exists (non-retracted)
    fn contains(&self, triple: &Triple) -> Result<bool, RDFStoreError> {
        let results = self.query(QueryPattern::exact(
            triple.subject.clone(),
            triple.predicate.clone(),
            triple.object.clone(),
        ))?;
        Ok(!results.is_empty())
    }

    /// Count total triples (including retracted)
    fn count_total(&self) -> Result<u64, RDFStoreError>;

    /// Count active triples (non-retracted)
    fn count_active(&self) -> Result<u64, RDFStoreError>;

    /// Get store statistics
    fn get_stats(&self) -> Result<StoreStats, RDFStoreError>;
}

/// Store statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoreStats {
    pub total_triples: u64,
    pub active_triples: u64,
    pub total_transactions: u64,
    pub subjects_count: u64,
    pub ontology_imported: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_pattern_creation() {
        let pattern = QueryPattern::with_subject(Subject::iri("http://example.org/s"));
        assert!(pattern.subject.is_some());
        assert!(pattern.predicate.is_none());
        assert!(pattern.object.is_none());

        let exact_pattern = QueryPattern::exact(
            Subject::iri("http://example.org/s"),
            Predicate::new("http://example.org/p"),
            Object::iri("http://example.org/o"),
        );
        assert!(exact_pattern.subject.is_some());
        assert!(exact_pattern.predicate.is_some());
        assert!(exact_pattern.object.is_some());
    }
}
