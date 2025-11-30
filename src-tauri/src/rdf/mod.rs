// ============================================================================
// RDF Module
// ============================================================================
// RDF-native data layer with storage abstraction
//
// This module provides:
// - RDF types (Subject, Predicate, Object, Triple)
// - RDF validation (IRI, Literal, Blank Node)
// - RDF store trait (storage-agnostic interface)
// - Storage adapters (SQLite, in-memory, etc.)
// ============================================================================

pub mod types;
pub mod validation;
pub mod store;
pub mod adapters;

pub use types::{Subject, Predicate, Object, Triple, Datatype, LanguageTag};
pub use validation::{RDFValidator, ValidationError};
pub use store::{RDFStore, RDFStoreError, QueryPattern, Transaction};
pub use adapters::sqlite::SQLiteRDFAdapter;
