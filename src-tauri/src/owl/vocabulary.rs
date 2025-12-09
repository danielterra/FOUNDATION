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

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // RDF Vocabulary Tests
    // ========================================================================

    #[test]
    fn test_rdf_type() {
        assert_eq!(rdf::TYPE, "rdf:type");
    }

    #[test]
    fn test_rdf_property() {
        assert_eq!(rdf::PROPERTY, "rdf:Property");
    }

    #[test]
    fn test_rdf_statement() {
        assert_eq!(rdf::STATEMENT, "rdf:Statement");
    }

    #[test]
    fn test_rdf_subject() {
        assert_eq!(rdf::SUBJECT, "rdf:subject");
    }

    #[test]
    fn test_rdf_predicate() {
        assert_eq!(rdf::PREDICATE, "rdf:predicate");
    }

    #[test]
    fn test_rdf_object() {
        assert_eq!(rdf::OBJECT, "rdf:object");
    }

    #[test]
    fn test_rdf_lang_string() {
        assert_eq!(rdf::LANG_STRING, "rdf:langString");
    }

    // ========================================================================
    // RDFS Vocabulary Tests
    // ========================================================================

    #[test]
    fn test_rdfs_class() {
        assert_eq!(rdfs::CLASS, "rdfs:Class");
    }

    #[test]
    fn test_rdfs_sub_class_of() {
        assert_eq!(rdfs::SUB_CLASS_OF, "rdfs:subClassOf");
    }

    #[test]
    fn test_rdfs_sub_property_of() {
        assert_eq!(rdfs::SUB_PROPERTY_OF, "rdfs:subPropertyOf");
    }

    #[test]
    fn test_rdfs_domain() {
        assert_eq!(rdfs::DOMAIN, "rdfs:domain");
    }

    #[test]
    fn test_rdfs_range() {
        assert_eq!(rdfs::RANGE, "rdfs:range");
    }

    #[test]
    fn test_rdfs_label() {
        assert_eq!(rdfs::LABEL, "rdfs:label");
    }

    #[test]
    fn test_rdfs_comment() {
        assert_eq!(rdfs::COMMENT, "rdfs:comment");
    }

    #[test]
    fn test_rdfs_resource() {
        assert_eq!(rdfs::RESOURCE, "rdfs:Resource");
    }

    #[test]
    fn test_rdfs_literal() {
        assert_eq!(rdfs::LITERAL, "rdfs:Literal");
    }

    #[test]
    fn test_rdfs_datatype() {
        assert_eq!(rdfs::DATATYPE, "rdfs:Datatype");
    }

    // ========================================================================
    // OWL Vocabulary Tests - Classes
    // ========================================================================

    #[test]
    fn test_owl_class() {
        assert_eq!(owl::CLASS, "owl:Class");
    }

    #[test]
    fn test_owl_thing() {
        assert_eq!(owl::THING, "owl:Thing");
    }

    #[test]
    fn test_owl_nothing() {
        assert_eq!(owl::NOTHING, "owl:Nothing");
    }

    // ========================================================================
    // OWL Vocabulary Tests - Properties
    // ========================================================================

    #[test]
    fn test_owl_object_property() {
        assert_eq!(owl::OBJECT_PROPERTY, "owl:ObjectProperty");
    }

    #[test]
    fn test_owl_datatype_property() {
        assert_eq!(owl::DATATYPE_PROPERTY, "owl:DatatypeProperty");
    }

    #[test]
    fn test_owl_annotation_property() {
        assert_eq!(owl::ANNOTATION_PROPERTY, "owl:AnnotationProperty");
    }

    #[test]
    fn test_owl_functional_property() {
        assert_eq!(owl::FUNCTIONAL_PROPERTY, "owl:FunctionalProperty");
    }

    #[test]
    fn test_owl_inverse_functional_property() {
        assert_eq!(owl::INVERSE_FUNCTIONAL_PROPERTY, "owl:InverseFunctionalProperty");
    }

    #[test]
    fn test_owl_transitive_property() {
        assert_eq!(owl::TRANSITIVE_PROPERTY, "owl:TransitiveProperty");
    }

    #[test]
    fn test_owl_symmetric_property() {
        assert_eq!(owl::SYMMETRIC_PROPERTY, "owl:SymmetricProperty");
    }

    #[test]
    fn test_owl_asymmetric_property() {
        assert_eq!(owl::ASYMMETRIC_PROPERTY, "owl:AsymmetricProperty");
    }

    #[test]
    fn test_owl_reflexive_property() {
        assert_eq!(owl::REFLEXIVE_PROPERTY, "owl:ReflexiveProperty");
    }

    #[test]
    fn test_owl_irreflexive_property() {
        assert_eq!(owl::IRREFLEXIVE_PROPERTY, "owl:IrreflexiveProperty");
    }

    // ========================================================================
    // OWL Vocabulary Tests - Relations
    // ========================================================================

    #[test]
    fn test_owl_equivalent_class() {
        assert_eq!(owl::EQUIVALENT_CLASS, "owl:equivalentClass");
    }

    #[test]
    fn test_owl_disjoint_with() {
        assert_eq!(owl::DISJOINT_WITH, "owl:disjointWith");
    }

    #[test]
    fn test_owl_equivalent_property() {
        assert_eq!(owl::EQUIVALENT_PROPERTY, "owl:equivalentProperty");
    }

    #[test]
    fn test_owl_inverse_of() {
        assert_eq!(owl::INVERSE_OF, "owl:inverseOf");
    }

    #[test]
    fn test_owl_same_as() {
        assert_eq!(owl::SAME_AS, "owl:sameAs");
    }

    #[test]
    fn test_owl_different_from() {
        assert_eq!(owl::DIFFERENT_FROM, "owl:differentFrom");
    }

    // ========================================================================
    // OWL Vocabulary Tests - Restrictions
    // ========================================================================

    #[test]
    fn test_owl_restriction() {
        assert_eq!(owl::RESTRICTION, "owl:Restriction");
    }

    #[test]
    fn test_owl_on_property() {
        assert_eq!(owl::ON_PROPERTY, "owl:onProperty");
    }

    #[test]
    fn test_owl_some_values_from() {
        assert_eq!(owl::SOME_VALUES_FROM, "owl:someValuesFrom");
    }

    #[test]
    fn test_owl_all_values_from() {
        assert_eq!(owl::ALL_VALUES_FROM, "owl:allValuesFrom");
    }

    #[test]
    fn test_owl_has_value() {
        assert_eq!(owl::HAS_VALUE, "owl:hasValue");
    }

    #[test]
    fn test_owl_min_cardinality() {
        assert_eq!(owl::MIN_CARDINALITY, "owl:minCardinality");
    }

    #[test]
    fn test_owl_max_cardinality() {
        assert_eq!(owl::MAX_CARDINALITY, "owl:maxCardinality");
    }

    #[test]
    fn test_owl_cardinality() {
        assert_eq!(owl::CARDINALITY, "owl:cardinality");
    }

    // ========================================================================
    // Integration Tests
    // ========================================================================

    #[test]
    fn test_vocabulary_constants_are_consistent() {
        // Verify RDF constants are unique
        assert_ne!(rdf::TYPE, rdf::PROPERTY);
        assert_ne!(rdf::SUBJECT, rdf::PREDICATE);
        assert_ne!(rdf::PREDICATE, rdf::OBJECT);

        // Verify RDFS constants are unique
        assert_ne!(rdfs::CLASS, rdfs::RESOURCE);
        assert_ne!(rdfs::LABEL, rdfs::COMMENT);
        assert_ne!(rdfs::DOMAIN, rdfs::RANGE);

        // Verify OWL constants are unique
        assert_ne!(owl::CLASS, owl::THING);
        assert_ne!(owl::OBJECT_PROPERTY, owl::DATATYPE_PROPERTY);
        assert_ne!(owl::EQUIVALENT_CLASS, owl::DISJOINT_WITH);
    }

    #[test]
    fn test_vocabulary_namespace_prefixes() {
        // All RDF constants should start with "rdf:"
        assert!(rdf::TYPE.starts_with("rdf:"));
        assert!(rdf::PROPERTY.starts_with("rdf:"));
        assert!(rdf::SUBJECT.starts_with("rdf:"));

        // All RDFS constants should start with "rdfs:"
        assert!(rdfs::CLASS.starts_with("rdfs:"));
        assert!(rdfs::LABEL.starts_with("rdfs:"));
        assert!(rdfs::COMMENT.starts_with("rdfs:"));

        // All OWL constants should start with "owl:"
        assert!(owl::CLASS.starts_with("owl:"));
        assert!(owl::THING.starts_with("owl:"));
        assert!(owl::OBJECT_PROPERTY.starts_with("owl:"));
    }

    #[test]
    fn test_property_types_all_contain_property() {
        // Verify all OWL property types contain "Property"
        assert!(owl::OBJECT_PROPERTY.contains("Property"));
        assert!(owl::DATATYPE_PROPERTY.contains("Property"));
        assert!(owl::ANNOTATION_PROPERTY.contains("Property"));
        assert!(owl::FUNCTIONAL_PROPERTY.contains("Property"));
        assert!(owl::INVERSE_FUNCTIONAL_PROPERTY.contains("Property"));
        assert!(owl::TRANSITIVE_PROPERTY.contains("Property"));
        assert!(owl::SYMMETRIC_PROPERTY.contains("Property"));
        assert!(owl::ASYMMETRIC_PROPERTY.contains("Property"));
        assert!(owl::REFLEXIVE_PROPERTY.contains("Property"));
        assert!(owl::IRREFLEXIVE_PROPERTY.contains("Property"));
    }

    #[test]
    fn test_restriction_constants_consistency() {
        // Verify restriction-related constants
        assert!(owl::RESTRICTION.starts_with("owl:"));
        assert!(owl::ON_PROPERTY.starts_with("owl:"));
        assert!(owl::SOME_VALUES_FROM.starts_with("owl:"));
        assert!(owl::ALL_VALUES_FROM.starts_with("owl:"));
        assert!(owl::HAS_VALUE.starts_with("owl:"));
        assert!(owl::MIN_CARDINALITY.starts_with("owl:"));
        assert!(owl::MAX_CARDINALITY.starts_with("owl:"));
        assert!(owl::CARDINALITY.starts_with("owl:"));
    }
}
