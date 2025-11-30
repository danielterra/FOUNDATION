// ============================================================================
// RDF Validation
// ============================================================================
// Validates RDF triples before storage to ensure RDF compliance
// ============================================================================

use super::types::{Subject, Predicate, Object, Triple, Datatype};

/// Validation errors
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    InvalidIRI(String),
    InvalidBlankNode(String),
    InvalidLiteral(String),
    InvalidDatatype(String),
    InvalidLanguageTag(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::InvalidIRI(msg) => write!(f, "Invalid IRI: {}", msg),
            ValidationError::InvalidBlankNode(msg) => write!(f, "Invalid blank node: {}", msg),
            ValidationError::InvalidLiteral(msg) => write!(f, "Invalid literal: {}", msg),
            ValidationError::InvalidDatatype(msg) => write!(f, "Invalid datatype: {}", msg),
            ValidationError::InvalidLanguageTag(msg) => write!(f, "Invalid language tag: {}", msg),
        }
    }
}

impl std::error::Error for ValidationError {}

/// RDF validator
pub struct RDFValidator;

impl RDFValidator {
    /// Validate a complete triple
    pub fn validate_triple(triple: &Triple) -> Result<(), ValidationError> {
        Self::validate_subject(&triple.subject)?;
        Self::validate_predicate(&triple.predicate)?;
        Self::validate_object(&triple.object)?;
        Ok(())
    }

    /// Validate subject (IRI or Blank Node)
    pub fn validate_subject(subject: &Subject) -> Result<(), ValidationError> {
        match subject {
            Subject::IRI(iri) => Self::validate_iri(iri),
            Subject::BlankNode(id) => Self::validate_blank_node(id),
        }
    }

    /// Validate predicate (must be IRI)
    pub fn validate_predicate(predicate: &Predicate) -> Result<(), ValidationError> {
        Self::validate_iri(predicate.as_str())
    }

    /// Validate object (IRI, Literal, or Blank Node)
    pub fn validate_object(object: &Object) -> Result<(), ValidationError> {
        match object {
            Object::IRI(iri) => Self::validate_iri(iri),
            Object::BlankNode(id) => Self::validate_blank_node(id),
            Object::Literal {
                value,
                datatype,
                language,
            } => {
                Self::validate_literal_value(value, datatype)?;
                if let Some(lang_tag) = language {
                    Self::validate_language_tag(lang_tag.as_str())?;
                }
                Ok(())
            }
        }
    }

    /// Validate IRI format
    fn validate_iri(iri: &str) -> Result<(), ValidationError> {
        if iri.is_empty() {
            return Err(ValidationError::InvalidIRI("IRI cannot be empty".to_string()));
        }

        // Check for valid IRI schemes
        let valid_schemes = ["http://", "https://", "urn:", "file://", "ftp://"];
        let has_valid_scheme = valid_schemes.iter().any(|scheme| iri.starts_with(scheme));

        if !has_valid_scheme {
            // Allow common prefixed names (e.g., "ex:", "owl:", "rdf:")
            if iri.contains(':') && iri.split(':').nth(1).map_or(false, |p| !p.is_empty()) {
                return Ok(());
            }

            return Err(ValidationError::InvalidIRI(format!(
                "IRI must start with a valid scheme (http://, https://, urn:, etc.) or be a prefixed name: {}",
                iri
            )));
        }

        // Check for invalid characters (basic validation)
        if iri.contains(|c: char| c.is_whitespace() || c == '<' || c == '>') {
            return Err(ValidationError::InvalidIRI(format!(
                "IRI contains invalid characters: {}",
                iri
            )));
        }

        Ok(())
    }

    /// Validate blank node format
    fn validate_blank_node(id: &str) -> Result<(), ValidationError> {
        if id.is_empty() {
            return Err(ValidationError::InvalidBlankNode(
                "Blank node ID cannot be empty".to_string(),
            ));
        }

        // Blank nodes should start with "_:"
        if !id.starts_with("_:") {
            return Err(ValidationError::InvalidBlankNode(format!(
                "Blank node must start with '_:': {}",
                id
            )));
        }

        // Check that the ID part is not empty
        let id_part = &id[2..];
        if id_part.is_empty() {
            return Err(ValidationError::InvalidBlankNode(
                "Blank node ID part cannot be empty after '_:'".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate literal value matches datatype
    fn validate_literal_value(value: &str, datatype: &Datatype) -> Result<(), ValidationError> {
        match datatype {
            Datatype::XsdInteger | Datatype::XsdInt | Datatype::XsdLong => {
                value.parse::<i64>().map_err(|_| {
                    ValidationError::InvalidLiteral(format!(
                        "Value '{}' is not a valid integer",
                        value
                    ))
                })?;
            }
            Datatype::XsdDecimal | Datatype::XsdDouble | Datatype::XsdFloat => {
                value.parse::<f64>().map_err(|_| {
                    ValidationError::InvalidLiteral(format!(
                        "Value '{}' is not a valid decimal/float",
                        value
                    ))
                })?;
            }
            Datatype::XsdBoolean => {
                if value != "true" && value != "false" && value != "0" && value != "1" {
                    return Err(ValidationError::InvalidLiteral(format!(
                        "Value '{}' is not a valid boolean (must be 'true', 'false', '0', or '1')",
                        value
                    )));
                }
            }
            Datatype::XsdDateTime => {
                // Basic ISO 8601 format check (simplified)
                if !value.contains('T') && !value.contains('-') {
                    return Err(ValidationError::InvalidLiteral(format!(
                        "Value '{}' is not a valid dateTime (expected ISO 8601 format)",
                        value
                    )));
                }
            }
            Datatype::XsdString => {
                // Strings are always valid
            }
            Datatype::Custom(iri) => {
                // Validate custom datatype IRI
                Self::validate_iri(iri).map_err(|_| {
                    ValidationError::InvalidDatatype(format!("Invalid custom datatype IRI: {}", iri))
                })?;
            }
        }

        Ok(())
    }

    /// Validate language tag (RFC 5646)
    fn validate_language_tag(tag: &str) -> Result<(), ValidationError> {
        if tag.is_empty() {
            return Err(ValidationError::InvalidLanguageTag(
                "Language tag cannot be empty".to_string(),
            ));
        }

        // Basic validation: lowercase letters, hyphens, 2-8 characters for primary subtag
        let parts: Vec<&str> = tag.split('-').collect();
        let primary = parts[0];

        if primary.len() < 2 || primary.len() > 8 {
            return Err(ValidationError::InvalidLanguageTag(format!(
                "Primary language subtag must be 2-8 characters: {}",
                tag
            )));
        }

        if !primary.chars().all(|c| c.is_ascii_lowercase()) {
            return Err(ValidationError::InvalidLanguageTag(format!(
                "Language tag must contain only lowercase ASCII letters and hyphens: {}",
                tag
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_iri() {
        assert!(RDFValidator::validate_iri("http://example.org/resource").is_ok());
        assert!(RDFValidator::validate_iri("https://example.org/resource").is_ok());
        assert!(RDFValidator::validate_iri("urn:isbn:0451450523").is_ok());
        assert!(RDFValidator::validate_iri("ex:resource").is_ok());
    }

    #[test]
    fn test_invalid_iri() {
        assert!(RDFValidator::validate_iri("").is_err());
        assert!(RDFValidator::validate_iri("invalid iri").is_err());
        assert!(RDFValidator::validate_iri("<http://example.org>").is_err());
    }

    #[test]
    fn test_valid_blank_node() {
        assert!(RDFValidator::validate_blank_node("_:b1").is_ok());
        assert!(RDFValidator::validate_blank_node("_:node123").is_ok());
    }

    #[test]
    fn test_invalid_blank_node() {
        assert!(RDFValidator::validate_blank_node("").is_err());
        assert!(RDFValidator::validate_blank_node("_:").is_err());
        assert!(RDFValidator::validate_blank_node("b1").is_err());
    }

    #[test]
    fn test_valid_literals() {
        assert!(RDFValidator::validate_literal_value("123", &Datatype::XsdInteger).is_ok());
        assert!(RDFValidator::validate_literal_value("45.50", &Datatype::XsdDecimal).is_ok());
        assert!(RDFValidator::validate_literal_value("true", &Datatype::XsdBoolean).is_ok());
        assert!(RDFValidator::validate_literal_value("false", &Datatype::XsdBoolean).is_ok());
        assert!(RDFValidator::validate_literal_value("hello", &Datatype::XsdString).is_ok());
    }

    #[test]
    fn test_invalid_literals() {
        assert!(RDFValidator::validate_literal_value("abc", &Datatype::XsdInteger).is_err());
        assert!(RDFValidator::validate_literal_value("not_a_number", &Datatype::XsdDecimal).is_err());
        assert!(RDFValidator::validate_literal_value("maybe", &Datatype::XsdBoolean).is_err());
    }

    #[test]
    fn test_valid_language_tags() {
        assert!(RDFValidator::validate_language_tag("en").is_ok());
        assert!(RDFValidator::validate_language_tag("pt").is_ok());
        assert!(RDFValidator::validate_language_tag("en-us").is_ok());
    }

    #[test]
    fn test_invalid_language_tags() {
        assert!(RDFValidator::validate_language_tag("").is_err());
        assert!(RDFValidator::validate_language_tag("e").is_err());
        assert!(RDFValidator::validate_language_tag("EN").is_err());
    }

    #[test]
    fn test_validate_triple() {
        let valid_triple = Triple::from_iris(
            "http://example.org/subject",
            "http://example.org/predicate",
            "http://example.org/object",
        );
        assert!(RDFValidator::validate_triple(&valid_triple).is_ok());

        let triple_with_literal = Triple::new(
            Subject::iri("http://example.org/subject"),
            Predicate::new("http://example.org/hasAge"),
            Object::integer_literal(25),
        );
        assert!(RDFValidator::validate_triple(&triple_with_literal).is_ok());
    }
}
