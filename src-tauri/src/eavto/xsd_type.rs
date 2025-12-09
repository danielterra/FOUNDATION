/// XSD (XML Schema Definition) Datatype
///
/// Standard XML Schema datatypes used in RDF literals
/// Specification: https://www.w3.org/TR/xmlschema-2/

/// XSD Datatype enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Complete XSD type system for future RDF compliance
pub enum XsdType {
    // String types
    String,
    NormalizedString,
    Token,
    Language,
    Name,
    NCName,

    // Numeric types
    Integer,
    Int,
    Long,
    Short,
    Byte,
    NonNegativeInteger,
    PositiveInteger,
    NonPositiveInteger,
    NegativeInteger,
    UnsignedLong,
    UnsignedInt,
    UnsignedShort,
    UnsignedByte,

    // Decimal types
    Decimal,
    Float,
    Double,

    // Boolean
    Boolean,

    // Date and time types
    DateTime,
    Date,
    Time,
    Duration,

    // Other types
    AnyURI,
    Base64Binary,
    HexBinary,
}

impl XsdType {
    /// Get the IRI representation (with xsd: prefix)
    pub fn as_iri(&self) -> &'static str {
        match self {
            // String types
            XsdType::String => "xsd:string",
            XsdType::NormalizedString => "xsd:normalizedString",
            XsdType::Token => "xsd:token",
            XsdType::Language => "xsd:language",
            XsdType::Name => "xsd:Name",
            XsdType::NCName => "xsd:NCName",

            // Numeric types
            XsdType::Integer => "xsd:integer",
            XsdType::Int => "xsd:int",
            XsdType::Long => "xsd:long",
            XsdType::Short => "xsd:short",
            XsdType::Byte => "xsd:byte",
            XsdType::NonNegativeInteger => "xsd:nonNegativeInteger",
            XsdType::PositiveInteger => "xsd:positiveInteger",
            XsdType::NonPositiveInteger => "xsd:nonPositiveInteger",
            XsdType::NegativeInteger => "xsd:negativeInteger",
            XsdType::UnsignedLong => "xsd:unsignedLong",
            XsdType::UnsignedInt => "xsd:unsignedInt",
            XsdType::UnsignedShort => "xsd:unsignedShort",
            XsdType::UnsignedByte => "xsd:unsignedByte",

            // Decimal types
            XsdType::Decimal => "xsd:decimal",
            XsdType::Float => "xsd:float",
            XsdType::Double => "xsd:double",

            // Boolean
            XsdType::Boolean => "xsd:boolean",

            // Date and time
            XsdType::DateTime => "xsd:dateTime",
            XsdType::Date => "xsd:date",
            XsdType::Time => "xsd:time",
            XsdType::Duration => "xsd:duration",

            // Other
            XsdType::AnyURI => "xsd:anyURI",
            XsdType::Base64Binary => "xsd:base64Binary",
            XsdType::HexBinary => "xsd:hexBinary",
        }
    }

    /// Parse from IRI string (with or without xsd: prefix)
    pub fn from_iri(iri: &str) -> Option<Self> {
        let local_name = iri.strip_prefix("xsd:").unwrap_or(iri);

        match local_name {
            // String types
            "string" => Some(XsdType::String),
            "normalizedString" => Some(XsdType::NormalizedString),
            "token" => Some(XsdType::Token),
            "language" => Some(XsdType::Language),
            "Name" => Some(XsdType::Name),
            "NCName" => Some(XsdType::NCName),

            // Numeric types
            "integer" => Some(XsdType::Integer),
            "int" => Some(XsdType::Int),
            "long" => Some(XsdType::Long),
            "short" => Some(XsdType::Short),
            "byte" => Some(XsdType::Byte),
            "nonNegativeInteger" => Some(XsdType::NonNegativeInteger),
            "positiveInteger" => Some(XsdType::PositiveInteger),
            "nonPositiveInteger" => Some(XsdType::NonPositiveInteger),
            "negativeInteger" => Some(XsdType::NegativeInteger),
            "unsignedLong" => Some(XsdType::UnsignedLong),
            "unsignedInt" => Some(XsdType::UnsignedInt),
            "unsignedShort" => Some(XsdType::UnsignedShort),
            "unsignedByte" => Some(XsdType::UnsignedByte),

            // Decimal types
            "decimal" => Some(XsdType::Decimal),
            "float" => Some(XsdType::Float),
            "double" => Some(XsdType::Double),

            // Boolean
            "boolean" => Some(XsdType::Boolean),

            // Date and time
            "dateTime" => Some(XsdType::DateTime),
            "date" => Some(XsdType::Date),
            "time" => Some(XsdType::Time),
            "duration" => Some(XsdType::Duration),

            // Other
            "anyURI" => Some(XsdType::AnyURI),
            "base64Binary" => Some(XsdType::Base64Binary),
            "hexBinary" => Some(XsdType::HexBinary),

            _ => None,
        }
    }

    /// Check if this is a numeric type
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            XsdType::Integer |
            XsdType::Int |
            XsdType::Long |
            XsdType::Short |
            XsdType::Byte |
            XsdType::NonNegativeInteger |
            XsdType::PositiveInteger |
            XsdType::NonPositiveInteger |
            XsdType::NegativeInteger |
            XsdType::UnsignedLong |
            XsdType::UnsignedInt |
            XsdType::UnsignedShort |
            XsdType::UnsignedByte |
            XsdType::Decimal |
            XsdType::Float |
            XsdType::Double
        )
    }

    /// Check if this is an integer type (not float/decimal)
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            XsdType::Integer |
            XsdType::Int |
            XsdType::Long |
            XsdType::Short |
            XsdType::Byte |
            XsdType::NonNegativeInteger |
            XsdType::PositiveInteger |
            XsdType::NonPositiveInteger |
            XsdType::NegativeInteger |
            XsdType::UnsignedLong |
            XsdType::UnsignedInt |
            XsdType::UnsignedShort |
            XsdType::UnsignedByte
        )
    }

    /// Check if this is a floating point type
    pub fn is_float(&self) -> bool {
        matches!(self, XsdType::Decimal | XsdType::Float | XsdType::Double)
    }

    /// Check if this is a date/time type
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            XsdType::DateTime | XsdType::Date | XsdType::Time | XsdType::Duration
        )
    }
}

impl std::fmt::Display for XsdType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_iri())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_iri() {
        assert_eq!(XsdType::Integer.as_iri(), "xsd:integer");
        assert_eq!(XsdType::String.as_iri(), "xsd:string");
        assert_eq!(XsdType::DateTime.as_iri(), "xsd:dateTime");
    }

    #[test]
    fn test_from_iri() {
        assert_eq!(XsdType::from_iri("xsd:integer"), Some(XsdType::Integer));
        assert_eq!(XsdType::from_iri("integer"), Some(XsdType::Integer));
        assert_eq!(XsdType::from_iri("unknown"), None);
    }

    #[test]
    fn test_predicates() {
        assert!(XsdType::Integer.is_numeric());
        assert!(XsdType::Integer.is_integer());
        assert!(!XsdType::Integer.is_float());

        assert!(XsdType::Decimal.is_numeric());
        assert!(!XsdType::Decimal.is_integer());
        assert!(XsdType::Decimal.is_float());

        assert!(XsdType::DateTime.is_temporal());
        assert!(!XsdType::String.is_temporal());
    }

    #[test]
    fn test_all_string_types() {
        assert_eq!(XsdType::NormalizedString.as_iri(), "xsd:normalizedString");
        assert_eq!(XsdType::Token.as_iri(), "xsd:token");
        assert_eq!(XsdType::Language.as_iri(), "xsd:language");
        assert_eq!(XsdType::Name.as_iri(), "xsd:Name");
        assert_eq!(XsdType::NCName.as_iri(), "xsd:NCName");
    }

    #[test]
    fn test_all_numeric_types_as_iri() {
        assert_eq!(XsdType::Int.as_iri(), "xsd:int");
        assert_eq!(XsdType::Long.as_iri(), "xsd:long");
        assert_eq!(XsdType::Short.as_iri(), "xsd:short");
        assert_eq!(XsdType::Byte.as_iri(), "xsd:byte");
        assert_eq!(XsdType::NonNegativeInteger.as_iri(), "xsd:nonNegativeInteger");
        assert_eq!(XsdType::PositiveInteger.as_iri(), "xsd:positiveInteger");
        assert_eq!(XsdType::NonPositiveInteger.as_iri(), "xsd:nonPositiveInteger");
        assert_eq!(XsdType::NegativeInteger.as_iri(), "xsd:negativeInteger");
        assert_eq!(XsdType::UnsignedLong.as_iri(), "xsd:unsignedLong");
        assert_eq!(XsdType::UnsignedInt.as_iri(), "xsd:unsignedInt");
        assert_eq!(XsdType::UnsignedShort.as_iri(), "xsd:unsignedShort");
        assert_eq!(XsdType::UnsignedByte.as_iri(), "xsd:unsignedByte");
    }

    #[test]
    fn test_decimal_types() {
        assert_eq!(XsdType::Float.as_iri(), "xsd:float");
        assert_eq!(XsdType::Double.as_iri(), "xsd:double");

        assert!(XsdType::Float.is_float());
        assert!(XsdType::Double.is_float());
        assert!(XsdType::Float.is_numeric());
        assert!(XsdType::Double.is_numeric());
    }

    #[test]
    fn test_boolean_type() {
        assert_eq!(XsdType::Boolean.as_iri(), "xsd:boolean");
        assert!(!XsdType::Boolean.is_numeric());
        assert!(!XsdType::Boolean.is_temporal());
    }

    #[test]
    fn test_temporal_types() {
        assert_eq!(XsdType::Date.as_iri(), "xsd:date");
        assert_eq!(XsdType::Time.as_iri(), "xsd:time");
        assert_eq!(XsdType::Duration.as_iri(), "xsd:duration");

        assert!(XsdType::Date.is_temporal());
        assert!(XsdType::Time.is_temporal());
        assert!(XsdType::Duration.is_temporal());
    }

    #[test]
    fn test_other_types() {
        assert_eq!(XsdType::AnyURI.as_iri(), "xsd:anyURI");
        assert_eq!(XsdType::Base64Binary.as_iri(), "xsd:base64Binary");
        assert_eq!(XsdType::HexBinary.as_iri(), "xsd:hexBinary");
    }

    #[test]
    fn test_from_iri_all_string_types() {
        assert_eq!(XsdType::from_iri("xsd:normalizedString"), Some(XsdType::NormalizedString));
        assert_eq!(XsdType::from_iri("token"), Some(XsdType::Token));
        assert_eq!(XsdType::from_iri("language"), Some(XsdType::Language));
        assert_eq!(XsdType::from_iri("Name"), Some(XsdType::Name));
        assert_eq!(XsdType::from_iri("NCName"), Some(XsdType::NCName));
    }

    #[test]
    fn test_from_iri_all_numeric_types() {
        assert_eq!(XsdType::from_iri("int"), Some(XsdType::Int));
        assert_eq!(XsdType::from_iri("long"), Some(XsdType::Long));
        assert_eq!(XsdType::from_iri("short"), Some(XsdType::Short));
        assert_eq!(XsdType::from_iri("byte"), Some(XsdType::Byte));
        assert_eq!(XsdType::from_iri("nonNegativeInteger"), Some(XsdType::NonNegativeInteger));
        assert_eq!(XsdType::from_iri("positiveInteger"), Some(XsdType::PositiveInteger));
        assert_eq!(XsdType::from_iri("nonPositiveInteger"), Some(XsdType::NonPositiveInteger));
        assert_eq!(XsdType::from_iri("negativeInteger"), Some(XsdType::NegativeInteger));
        assert_eq!(XsdType::from_iri("unsignedLong"), Some(XsdType::UnsignedLong));
        assert_eq!(XsdType::from_iri("unsignedInt"), Some(XsdType::UnsignedInt));
        assert_eq!(XsdType::from_iri("unsignedShort"), Some(XsdType::UnsignedShort));
        assert_eq!(XsdType::from_iri("unsignedByte"), Some(XsdType::UnsignedByte));
    }

    #[test]
    fn test_from_iri_decimal_types() {
        assert_eq!(XsdType::from_iri("decimal"), Some(XsdType::Decimal));
        assert_eq!(XsdType::from_iri("xsd:float"), Some(XsdType::Float));
        assert_eq!(XsdType::from_iri("double"), Some(XsdType::Double));
    }

    #[test]
    fn test_from_iri_boolean() {
        assert_eq!(XsdType::from_iri("boolean"), Some(XsdType::Boolean));
        assert_eq!(XsdType::from_iri("xsd:boolean"), Some(XsdType::Boolean));
    }

    #[test]
    fn test_from_iri_temporal_types() {
        assert_eq!(XsdType::from_iri("dateTime"), Some(XsdType::DateTime));
        assert_eq!(XsdType::from_iri("date"), Some(XsdType::Date));
        assert_eq!(XsdType::from_iri("time"), Some(XsdType::Time));
        assert_eq!(XsdType::from_iri("duration"), Some(XsdType::Duration));
    }

    #[test]
    fn test_from_iri_other_types() {
        assert_eq!(XsdType::from_iri("anyURI"), Some(XsdType::AnyURI));
        assert_eq!(XsdType::from_iri("base64Binary"), Some(XsdType::Base64Binary));
        assert_eq!(XsdType::from_iri("hexBinary"), Some(XsdType::HexBinary));
    }

    #[test]
    fn test_is_numeric_comprehensive() {
        // Integer types
        assert!(XsdType::Int.is_numeric());
        assert!(XsdType::Long.is_numeric());
        assert!(XsdType::Short.is_numeric());
        assert!(XsdType::Byte.is_numeric());
        assert!(XsdType::NonNegativeInteger.is_numeric());
        assert!(XsdType::PositiveInteger.is_numeric());
        assert!(XsdType::NonPositiveInteger.is_numeric());
        assert!(XsdType::NegativeInteger.is_numeric());
        assert!(XsdType::UnsignedLong.is_numeric());
        assert!(XsdType::UnsignedInt.is_numeric());
        assert!(XsdType::UnsignedShort.is_numeric());
        assert!(XsdType::UnsignedByte.is_numeric());

        // Non-numeric types
        assert!(!XsdType::String.is_numeric());
        assert!(!XsdType::Boolean.is_numeric());
        assert!(!XsdType::DateTime.is_numeric());
    }

    #[test]
    fn test_is_integer_comprehensive() {
        // Integer types
        assert!(XsdType::Int.is_integer());
        assert!(XsdType::Long.is_integer());
        assert!(XsdType::Short.is_integer());
        assert!(XsdType::Byte.is_integer());
        assert!(XsdType::UnsignedLong.is_integer());

        // Not integer types
        assert!(!XsdType::Float.is_integer());
        assert!(!XsdType::Double.is_integer());
        assert!(!XsdType::Decimal.is_integer());
        assert!(!XsdType::String.is_integer());
    }

    #[test]
    fn test_display_trait() {
        assert_eq!(format!("{}", XsdType::Integer), "xsd:integer");
        assert_eq!(format!("{}", XsdType::String), "xsd:string");
        assert_eq!(format!("{}", XsdType::DateTime), "xsd:dateTime");
        assert_eq!(format!("{}", XsdType::Boolean), "xsd:boolean");
    }
}
