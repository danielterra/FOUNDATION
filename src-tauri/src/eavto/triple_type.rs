/// Triple Type
///
/// Represents a single RDF triple with EVTO metadata

use super::object_type::Object;

/// A single RDF triple with EVTO metadata
#[derive(Debug, Clone)]
pub struct Triple {
    // RDF Triple (Entity-Value)
    pub subject: String,
    pub predicate: String,
    pub object: Object,

    // Time dimension
    pub tx: i64,
    pub created_at: i64,

    // Origin dimension
    pub origin_id: i64,

    // Retraction (immutable timeline)
    pub retracted: bool,
}

impl Triple {
    /// Create a new Triple
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: Object,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object,
            tx: 0,
            created_at: 0,
            origin_id: 0,
            retracted: false,
        }
    }

    /// Check if this triple is currently active (not retracted)
    pub fn is_active(&self) -> bool {
        !self.retracted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triple_new() {
        let triple = Triple::new(
            "foundation:Class",
            "rdf:type",
            Object::Iri("owl:Class".to_string()),
        );

        assert_eq!(triple.subject, "foundation:Class");
        assert_eq!(triple.predicate, "rdf:type");
        assert_eq!(triple.tx, 0);
        assert_eq!(triple.created_at, 0);
        assert_eq!(triple.origin_id, 0);
        assert_eq!(triple.retracted, false);
    }

    #[test]
    fn test_triple_is_active() {
        let mut triple = Triple::new(
            "test:Subject",
            "test:predicate",
            Object::Integer(42),
        );

        assert!(triple.is_active());

        triple.retracted = true;
        assert!(!triple.is_active());
    }

    #[test]
    fn test_triple_clone() {
        let triple1 = Triple::new(
            "test:Subject",
            "test:predicate",
            Object::Iri("test:Object".to_string()),
        );

        let triple2 = triple1.clone();

        assert_eq!(triple1.subject, triple2.subject);
        assert_eq!(triple1.predicate, triple2.predicate);
    }
}
