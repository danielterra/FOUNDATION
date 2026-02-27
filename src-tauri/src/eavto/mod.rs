/// EAVTO (Entity-Attribute-Value-Time-Origin) Module
///
/// Functional interface for the immutable triple store
///
/// # Architecture
///
/// This module provides a functional abstraction over the SQLite triple store,
/// following Rust's functional programming patterns:
///
/// - **Immutable data**: All types are immutable (Clone, not mut)
/// - **Pure functions**: Query functions don't mutate state
/// - **Explicit state**: Connection is passed explicitly, never hidden
/// - **Composition**: Small functions that compose into larger operations
///
/// # EAVTO Dimensions
///
/// - **E (Entity)**: The subject of RDF triples (what we're talking about)
/// - **A (Attribute)**: The predicate (which property)
/// - **V (Value)**: The object (what we're saying about it)
/// - **T (Time)**: Transaction-based timeline (when it was said)
/// - **O (Origin)**: Who/what asserted it (provenance)

// Type modules (one file per type)
mod triple_type;
mod object_type;
mod query_result_type;
mod transaction_type;
mod origin_type;
mod xsd_type;

// Function modules
pub mod query;
pub mod store;
pub mod connection;
pub mod stats;
pub mod executor;

// Test helpers (public for use in other module tests)
#[cfg(test)]
pub mod test_helpers;

// Re-export commonly used types
pub use triple_type::Triple;
pub use object_type::Object;

// Re-export main functions (empty - all functions used via module path)

// Re-export connection and stats functions
pub use connection::initialize_with_progress;

pub use stats::{
    get_stats,
};

pub use executor::DbExecutor;
