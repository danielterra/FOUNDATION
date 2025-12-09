/// Object Type
///
/// Represents the object part of an RDF triple

use super::xsd_type::XsdType;

/// RDF Object variants
#[derive(Debug, Clone)]
pub enum Object {
    /// IRI reference (e.g., "foundation:Computer")
    Iri(String),

    /// Blank node (e.g., "_:b1")
    Blank(String),

    /// Literal with optional datatype and language
    Literal {
        value: String,
        datatype: Option<String>,
        language: Option<String>,
    },

    /// Typed literals for efficient queries
    Integer(i64),
    Number(f64),
    Boolean(bool),
    DateTime(i64), // Unix epoch milliseconds
}

impl Object {
    /// Get the object type for SQL storage
    pub fn object_type(&self) -> &'static str {
        match self {
            Object::Iri(_) => "iri",
            Object::Blank(_) => "blank",
            Object::Literal { .. } |
            Object::Integer(_) |
            Object::Number(_) |
            Object::Boolean(_) |
            Object::DateTime(_) => "literal",
        }
    }

    /// Get the object IRI (for Iri and Blank)
    pub fn as_iri(&self) -> Option<&str> {
        match self {
            Object::Iri(iri) | Object::Blank(iri) => Some(iri),
            _ => None,
        }
    }

    /// Get the literal value
    pub fn as_literal(&self) -> Option<String> {
        match self {
            Object::Literal { value, .. } => Some(value.clone()),
            Object::Integer(i) => Some(i.to_string()),
            Object::Number(n) => Some(n.to_string()),
            Object::Boolean(b) => Some(b.to_string()),
            Object::DateTime(dt) => Some(dt.to_string()),
            _ => None,
        }
    }

    /// Get the datatype IRI
    pub fn datatype(&self) -> Option<&str> {
        match self {
            Object::Literal { datatype, .. } => datatype.as_deref(),
            Object::Integer(_) => Some(XsdType::Integer.as_iri()),
            Object::Number(_) => Some(XsdType::Decimal.as_iri()),
            Object::Boolean(_) => Some(XsdType::Boolean.as_iri()),
            Object::DateTime(_) => Some(XsdType::DateTime.as_iri()),
            _ => None,
        }
    }

    /// Get the XSD type if this is a typed literal
    pub fn xsd_type(&self) -> Option<XsdType> {
        match self {
            Object::Integer(_) => Some(XsdType::Integer),
            Object::Number(_) => Some(XsdType::Decimal),
            Object::Boolean(_) => Some(XsdType::Boolean),
            Object::DateTime(_) => Some(XsdType::DateTime),
            Object::Literal { datatype: Some(dt), .. } => XsdType::from_iri(dt),
            _ => None,
        }
    }

    /// Check if this is an IRI object
    pub fn is_iri(&self) -> bool {
        matches!(self, Object::Iri(_))
    }

    /// Check if this is a literal object
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            Object::Literal { .. } |
            Object::Integer(_) |
            Object::Number(_) |
            Object::Boolean(_) |
            Object::DateTime(_)
        )
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Object::Iri(a), Object::Iri(b)) => a == b,
            (Object::Blank(a), Object::Blank(b)) => a == b,
            (
                Object::Literal { value: v1, datatype: d1, language: l1 },
                Object::Literal { value: v2, datatype: d2, language: l2 }
            ) => v1 == v2 && d1 == d2 && l1 == l2,
            (Object::Integer(a), Object::Integer(b)) => a == b,
            (Object::Number(a), Object::Number(b)) => (a - b).abs() < f64::EPSILON,
            (Object::Boolean(a), Object::Boolean(b)) => a == b,
            (Object::DateTime(a), Object::DateTime(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_type_iri() {
        let obj = Object::Iri("foundation:Class".to_string());
        assert_eq!(obj.object_type(), "iri");
    }

    #[test]
    fn test_object_type_blank() {
        let obj = Object::Blank("_:b1".to_string());
        assert_eq!(obj.object_type(), "blank");
    }

    #[test]
    fn test_object_type_literal() {
        let obj = Object::Literal {
            value: "test".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        assert_eq!(obj.object_type(), "literal");
    }

    #[test]
    fn test_object_type_integer() {
        let obj = Object::Integer(42);
        assert_eq!(obj.object_type(), "literal");
    }

    #[test]
    fn test_as_iri() {
        let iri_obj = Object::Iri("foundation:Class".to_string());
        assert_eq!(iri_obj.as_iri(), Some("foundation:Class"));

        let blank_obj = Object::Blank("_:b1".to_string());
        assert_eq!(blank_obj.as_iri(), Some("_:b1"));

        let literal_obj = Object::Integer(42);
        assert_eq!(literal_obj.as_iri(), None);
    }

    #[test]
    fn test_as_literal() {
        let lit = Object::Literal {
            value: "test".to_string(),
            datatype: None,
            language: None,
        };
        assert_eq!(lit.as_literal(), Some("test".to_string()));

        let int_obj = Object::Integer(42);
        assert_eq!(int_obj.as_literal(), Some("42".to_string()));

        let num_obj = Object::Number(3.14);
        assert_eq!(num_obj.as_literal(), Some("3.14".to_string()));

        let bool_obj = Object::Boolean(true);
        assert_eq!(bool_obj.as_literal(), Some("true".to_string()));

        let dt_obj = Object::DateTime(1000);
        assert_eq!(dt_obj.as_literal(), Some("1000".to_string()));

        let iri_obj = Object::Iri("test".to_string());
        assert_eq!(iri_obj.as_literal(), None);
    }

    #[test]
    fn test_datatype() {
        let lit = Object::Literal {
            value: "test".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        assert_eq!(lit.datatype(), Some("xsd:string"));

        let int_obj = Object::Integer(42);
        assert_eq!(int_obj.datatype(), Some("xsd:integer"));

        let num_obj = Object::Number(3.14);
        assert_eq!(num_obj.datatype(), Some("xsd:decimal"));

        let bool_obj = Object::Boolean(true);
        assert_eq!(bool_obj.datatype(), Some("xsd:boolean"));

        let dt_obj = Object::DateTime(1000);
        assert_eq!(dt_obj.datatype(), Some("xsd:dateTime"));

        let iri_obj = Object::Iri("test".to_string());
        assert_eq!(iri_obj.datatype(), None);
    }

    #[test]
    fn test_xsd_type() {
        let int_obj = Object::Integer(42);
        assert_eq!(int_obj.xsd_type(), Some(XsdType::Integer));

        let num_obj = Object::Number(3.14);
        assert_eq!(num_obj.xsd_type(), Some(XsdType::Decimal));

        let bool_obj = Object::Boolean(true);
        assert_eq!(bool_obj.xsd_type(), Some(XsdType::Boolean));

        let dt_obj = Object::DateTime(1000);
        assert_eq!(dt_obj.xsd_type(), Some(XsdType::DateTime));

        let lit = Object::Literal {
            value: "test".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        assert_eq!(lit.xsd_type(), Some(XsdType::String));

        let iri_obj = Object::Iri("test".to_string());
        assert_eq!(iri_obj.xsd_type(), None);
    }

    #[test]
    fn test_is_iri() {
        assert!(Object::Iri("test".to_string()).is_iri());
        assert!(!Object::Blank("_:b1".to_string()).is_iri());
        assert!(!Object::Integer(42).is_iri());
    }

    #[test]
    fn test_is_literal() {
        assert!(Object::Literal {
            value: "test".to_string(),
            datatype: None,
            language: None,
        }.is_literal());
        assert!(Object::Integer(42).is_literal());
        assert!(Object::Number(3.14).is_literal());
        assert!(Object::Boolean(true).is_literal());
        assert!(Object::DateTime(1000).is_literal());
        assert!(!Object::Iri("test".to_string()).is_literal());
    }
}
