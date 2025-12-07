// ============================================================================
// OWL Vocabulary - RDF/RDFS/OWL Constants
// ============================================================================
// Standard vocabulary for RDF, RDFS, and OWL ontologies
// ============================================================================

/// RDF vocabulary
pub mod rdf {
    pub const TYPE: &str = "rdf:type";
    pub const PROPERTY: &str = "rdf:Property";
    pub const STATEMENT: &str = "rdf:Statement";
    pub const SUBJECT: &str = "rdf:subject";
    pub const PREDICATE: &str = "rdf:predicate";
    pub const OBJECT: &str = "rdf:object";
    pub const LANG_STRING: &str = "rdf:langString";
}

/// RDFS vocabulary
pub mod rdfs {
    pub const CLASS: &str = "rdfs:Class";
    pub const SUB_CLASS_OF: &str = "rdfs:subClassOf";
    pub const SUB_PROPERTY_OF: &str = "rdfs:subPropertyOf";
    pub const DOMAIN: &str = "rdfs:domain";
    pub const RANGE: &str = "rdfs:range";
    pub const LABEL: &str = "rdfs:label";
    pub const COMMENT: &str = "rdfs:comment";
    pub const RESOURCE: &str = "rdfs:Resource";
    pub const LITERAL: &str = "rdfs:Literal";
    pub const DATATYPE: &str = "rdfs:Datatype";
}

/// OWL vocabulary
pub mod owl {
    pub const CLASS: &str = "owl:Class";
    pub const THING: &str = "owl:Thing";
    pub const NOTHING: &str = "owl:Nothing";
    pub const OBJECT_PROPERTY: &str = "owl:ObjectProperty";
    pub const DATATYPE_PROPERTY: &str = "owl:DatatypeProperty";
    pub const ANNOTATION_PROPERTY: &str = "owl:AnnotationProperty";
    pub const FUNCTIONAL_PROPERTY: &str = "owl:FunctionalProperty";
    pub const INVERSE_FUNCTIONAL_PROPERTY: &str = "owl:InverseFunctionalProperty";
    pub const TRANSITIVE_PROPERTY: &str = "owl:TransitiveProperty";
    pub const SYMMETRIC_PROPERTY: &str = "owl:SymmetricProperty";
    pub const ASYMMETRIC_PROPERTY: &str = "owl:AsymmetricProperty";
    pub const REFLEXIVE_PROPERTY: &str = "owl:ReflexiveProperty";
    pub const IRREFLEXIVE_PROPERTY: &str = "owl:IrreflexiveProperty";

    pub const EQUIVALENT_CLASS: &str = "owl:equivalentClass";
    pub const DISJOINT_WITH: &str = "owl:disjointWith";
    pub const EQUIVALENT_PROPERTY: &str = "owl:equivalentProperty";
    pub const INVERSE_OF: &str = "owl:inverseOf";
    pub const SAME_AS: &str = "owl:sameAs";
    pub const DIFFERENT_FROM: &str = "owl:differentFrom";

    pub const RESTRICTION: &str = "owl:Restriction";
    pub const ON_PROPERTY: &str = "owl:onProperty";
    pub const SOME_VALUES_FROM: &str = "owl:someValuesFrom";
    pub const ALL_VALUES_FROM: &str = "owl:allValuesFrom";
    pub const HAS_VALUE: &str = "owl:hasValue";
    pub const MIN_CARDINALITY: &str = "owl:minCardinality";
    pub const MAX_CARDINALITY: &str = "owl:maxCardinality";
    pub const CARDINALITY: &str = "owl:cardinality";
}

/// Vocabulary abstraction
pub struct Vocabulary;

impl Vocabulary {
    /// Check if an IRI is an RDF type predicate
    pub fn is_type_predicate(iri: &str) -> bool {
        iri == rdf::TYPE || iri == "a"
    }

    /// Check if an IRI is a class type
    pub fn is_class_type(iri: &str) -> bool {
        matches!(iri,
            "rdfs:Class" | "owl:Class" | "rdfs:Datatype"
        )
    }

    /// Check if an IRI is a property type
    pub fn is_property_type(iri: &str) -> bool {
        iri.contains("Property")
    }

    /// Check if an IRI is an OWL restriction
    pub fn is_restriction(iri: &str) -> bool {
        iri == owl::RESTRICTION
    }
}
