// ============================================================================
// RDF Types
// ============================================================================
// Core RDF data structures following RDF 1.1 specification
// https://www.w3.org/TR/rdf11-concepts/
// ============================================================================

use serde::{Deserialize, Serialize};

/// RDF Subject (IRI or Blank Node)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Subject {
    /// Named node identified by IRI
    IRI(String),
    /// Anonymous resource (blank node)
    BlankNode(String),
}

impl Subject {
    /// Create IRI subject
    pub fn iri(iri: impl Into<String>) -> Self {
        Subject::IRI(iri.into())
    }

    /// Create blank node subject
    pub fn blank(id: impl Into<String>) -> Self {
        Subject::BlankNode(id.into())
    }

    /// Get string representation
    pub fn as_str(&self) -> &str {
        match self {
            Subject::IRI(iri) => iri,
            Subject::BlankNode(id) => id,
        }
    }

    /// Check if this is an IRI
    pub fn is_iri(&self) -> bool {
        matches!(self, Subject::IRI(_))
    }

    /// Check if this is a blank node
    pub fn is_blank(&self) -> bool {
        matches!(self, Subject::BlankNode(_))
    }
}

/// RDF Predicate (always an IRI)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Predicate(String);

impl Predicate {
    /// Create new predicate from IRI
    pub fn new(iri: impl Into<String>) -> Self {
        Predicate(iri.into())
    }

    /// Get IRI as string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get IRI (consumes self)
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<String> for Predicate {
    fn from(iri: String) -> Self {
        Predicate(iri)
    }
}

impl From<&str> for Predicate {
    fn from(iri: &str) -> Self {
        Predicate(iri.to_string())
    }
}

/// RDF Object (IRI, Literal, or Blank Node)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Object {
    /// Named node identified by IRI
    IRI(String),
    /// Literal value with datatype and optional language tag
    Literal {
        value: String,
        datatype: Datatype,
        language: Option<LanguageTag>,
    },
    /// Anonymous resource (blank node)
    BlankNode(String),
}

impl Object {
    /// Create IRI object
    pub fn iri(iri: impl Into<String>) -> Self {
        Object::IRI(iri.into())
    }

    /// Create string literal
    pub fn string_literal(value: impl Into<String>) -> Self {
        Object::Literal {
            value: value.into(),
            datatype: Datatype::XsdString,
            language: None,
        }
    }

    /// Create decimal literal
    pub fn decimal_literal(value: f64) -> Self {
        Object::Literal {
            value: value.to_string(),
            datatype: Datatype::XsdDecimal,
            language: None,
        }
    }

    /// Create integer literal
    pub fn integer_literal(value: i64) -> Self {
        Object::Literal {
            value: value.to_string(),
            datatype: Datatype::XsdInteger,
            language: None,
        }
    }

    /// Create boolean literal
    pub fn boolean_literal(value: bool) -> Self {
        Object::Literal {
            value: value.to_string(),
            datatype: Datatype::XsdBoolean,
            language: None,
        }
    }

    /// Create dateTime literal
    pub fn datetime_literal(value: impl Into<String>) -> Self {
        Object::Literal {
            value: value.into(),
            datatype: Datatype::XsdDateTime,
            language: None,
        }
    }

    /// Create language-tagged string literal
    pub fn lang_string(value: impl Into<String>, lang: impl Into<String>) -> Self {
        Object::Literal {
            value: value.into(),
            datatype: Datatype::XsdString,
            language: Some(LanguageTag(lang.into())),
        }
    }

    /// Create blank node object
    pub fn blank(id: impl Into<String>) -> Self {
        Object::BlankNode(id.into())
    }

    /// Check if this is an IRI
    pub fn is_iri(&self) -> bool {
        matches!(self, Object::IRI(_))
    }

    /// Check if this is a literal
    pub fn is_literal(&self) -> bool {
        matches!(self, Object::Literal { .. })
    }

    /// Check if this is a blank node
    pub fn is_blank(&self) -> bool {
        matches!(self, Object::BlankNode(_))
    }
}

/// XSD Datatypes for RDF Literals
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Datatype {
    XsdString,
    XsdDecimal,
    XsdInteger,
    XsdBoolean,
    XsdDateTime,
    XsdDouble,
    XsdFloat,
    XsdInt,
    XsdLong,
    /// Custom datatype IRI
    Custom(String),
}

impl Datatype {
    /// Get IRI representation of the datatype
    pub fn as_iri(&self) -> &str {
        match self {
            Datatype::XsdString => "http://www.w3.org/2001/XMLSchema#string",
            Datatype::XsdDecimal => "http://www.w3.org/2001/XMLSchema#decimal",
            Datatype::XsdInteger => "http://www.w3.org/2001/XMLSchema#integer",
            Datatype::XsdBoolean => "http://www.w3.org/2001/XMLSchema#boolean",
            Datatype::XsdDateTime => "http://www.w3.org/2001/XMLSchema#dateTime",
            Datatype::XsdDouble => "http://www.w3.org/2001/XMLSchema#double",
            Datatype::XsdFloat => "http://www.w3.org/2001/XMLSchema#float",
            Datatype::XsdInt => "http://www.w3.org/2001/XMLSchema#int",
            Datatype::XsdLong => "http://www.w3.org/2001/XMLSchema#long",
            Datatype::Custom(iri) => iri,
        }
    }

    /// Parse datatype from IRI
    pub fn from_iri(iri: &str) -> Self {
        match iri {
            "http://www.w3.org/2001/XMLSchema#string" | "xsd:string" => Datatype::XsdString,
            "http://www.w3.org/2001/XMLSchema#decimal" | "xsd:decimal" => Datatype::XsdDecimal,
            "http://www.w3.org/2001/XMLSchema#integer" | "xsd:integer" => Datatype::XsdInteger,
            "http://www.w3.org/2001/XMLSchema#boolean" | "xsd:boolean" => Datatype::XsdBoolean,
            "http://www.w3.org/2001/XMLSchema#dateTime" | "xsd:dateTime" => Datatype::XsdDateTime,
            "http://www.w3.org/2001/XMLSchema#double" | "xsd:double" => Datatype::XsdDouble,
            "http://www.w3.org/2001/XMLSchema#float" | "xsd:float" => Datatype::XsdFloat,
            "http://www.w3.org/2001/XMLSchema#int" | "xsd:int" => Datatype::XsdInt,
            "http://www.w3.org/2001/XMLSchema#long" | "xsd:long" => Datatype::XsdLong,
            custom => Datatype::Custom(custom.to_string()),
        }
    }
}

/// Language tag for language-tagged string literals (e.g., "en", "pt-BR")
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageTag(String);

impl LanguageTag {
    pub fn new(tag: impl Into<String>) -> Self {
        LanguageTag(tag.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for LanguageTag {
    fn from(tag: String) -> Self {
        LanguageTag(tag)
    }
}

impl From<&str> for LanguageTag {
    fn from(tag: &str) -> Self {
        LanguageTag(tag.to_string())
    }
}

/// RDF Triple (Subject-Predicate-Object)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Triple {
    pub subject: Subject,
    pub predicate: Predicate,
    pub object: Object,
}

impl Triple {
    /// Create new triple
    pub fn new(subject: Subject, predicate: Predicate, object: Object) -> Self {
        Triple {
            subject,
            predicate,
            object,
        }
    }

    /// Create triple from IRIs
    pub fn from_iris(subject: &str, predicate: &str, object: &str) -> Self {
        Triple {
            subject: Subject::iri(subject),
            predicate: Predicate::new(predicate),
            object: Object::iri(object),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subject_creation() {
        let iri_subject = Subject::iri("http://example.org/subject");
        assert!(iri_subject.is_iri());
        assert!(!iri_subject.is_blank());

        let blank_subject = Subject::blank("_:b1");
        assert!(!blank_subject.is_iri());
        assert!(blank_subject.is_blank());
    }

    #[test]
    fn test_object_types() {
        let iri_obj = Object::iri("http://example.org/object");
        assert!(iri_obj.is_iri());
        assert!(!iri_obj.is_literal());

        let literal_obj = Object::string_literal("hello");
        assert!(literal_obj.is_literal());
        assert!(!literal_obj.is_iri());

        let decimal_obj = Object::decimal_literal(45.50);
        assert!(decimal_obj.is_literal());
    }

    #[test]
    fn test_datatype_iri() {
        assert_eq!(
            Datatype::XsdString.as_iri(),
            "http://www.w3.org/2001/XMLSchema#string"
        );
        assert_eq!(
            Datatype::XsdDecimal.as_iri(),
            "http://www.w3.org/2001/XMLSchema#decimal"
        );
    }

    #[test]
    fn test_triple_creation() {
        let triple = Triple::from_iris(
            "http://example.org/subject",
            "http://example.org/predicate",
            "http://example.org/object",
        );

        assert!(triple.subject.is_iri());
        assert_eq!(triple.predicate.as_str(), "http://example.org/predicate");
        assert!(triple.object.is_iri());
    }
}
