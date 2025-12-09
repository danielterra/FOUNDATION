/// Namespace expansion utilities for converting prefixed IRIs to full IRIs
use std::collections::HashMap;

lazy_static::lazy_static! {
    static ref NAMESPACES: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("rdf:", "http://www.w3.org/1999/02/22-rdf-syntax-ns#");
        m.insert("rdfs:", "http://www.w3.org/2000/01/rdf-schema#");
        m.insert("owl:", "http://www.w3.org/2002/07/owl#");
        m.insert("xsd:", "http://www.w3.org/2001/XMLSchema#");
        m.insert("skos:", "http://www.w3.org/2004/02/skos/core#");
        m.insert("foundation:", "http://foundation.local/ontology/");
        m.insert("qudt:", "http://qudt.org/schema/qudt/");
        m.insert("unit:", "http://qudt.org/vocab/unit/");
        m
    };
}

/// Expands a prefixed IRI to its full form
/// Examples:
/// - "rdfs:label" -> "http://www.w3.org/2000/01/rdf-schema#label"
/// - "owl:Thing" -> "http://www.w3.org/2002/07/owl#Thing"
/// - "http://example.org/full" -> "http://example.org/full" (unchanged)
pub fn expand_iri(iri: &str) -> String {
    for (prefix, namespace) in NAMESPACES.iter() {
        if iri.starts_with(prefix) {
            return iri.replace(prefix, namespace);
        }
    }
    // Already expanded or unknown prefix
    iri.to_string()
}

/// Compresses a full IRI to its prefixed form
/// Examples:
/// - "http://www.w3.org/2000/01/rdf-schema#label" -> "rdfs:label"
/// - "http://www.w3.org/2002/07/owl#Thing" -> "owl:Thing"
/// - "http://foundation.local/ontology/Computer" -> "foundation:Computer"
pub fn compress_iri(iri: &str) -> String {
    for (prefix, namespace) in NAMESPACES.iter() {
        if iri.starts_with(namespace) {
            return iri.replace(namespace, prefix);
        }
    }
    // Already compressed or unknown namespace
    iri.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_iri() {
        assert_eq!(
            expand_iri("rdfs:label"),
            "http://www.w3.org/2000/01/rdf-schema#label"
        );
        assert_eq!(
            expand_iri("owl:Thing"),
            "http://www.w3.org/2002/07/owl#Thing"
        );
        assert_eq!(
            expand_iri("http://example.org/full"),
            "http://example.org/full"
        );
    }

    #[test]
    fn test_compress_iri() {
        assert_eq!(
            compress_iri("http://www.w3.org/2000/01/rdf-schema#label"),
            "rdfs:label"
        );
        assert_eq!(
            compress_iri("http://www.w3.org/2002/07/owl#Thing"),
            "owl:Thing"
        );
        assert_eq!(compress_iri("custom:Thing"), "custom:Thing");
    }
}
