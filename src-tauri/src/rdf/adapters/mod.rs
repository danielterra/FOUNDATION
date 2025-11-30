// ============================================================================
// RDF Storage Adapters
// ============================================================================
// Implementations of RDFStore trait for different storage backends
// ============================================================================

pub mod sqlite;

pub use sqlite::SQLiteRDFAdapter;
