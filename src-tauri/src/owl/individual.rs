// ============================================================================
// OWL Individual - Individual/Instance Operations
// ============================================================================
// High-level operations for managing individuals (instances of classes)
// ============================================================================

use rusqlite::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, vocabulary::{rdf, rdfs, owl}};

/// Represents an OWL Individual (instance of a class)
#[derive(Debug, Clone)]
pub struct Individual {
    pub iri: String,
}

impl Individual {
    /// Create a new Individual reference
    pub fn new(iri: impl Into<String>) -> Self {
        Self { iri: iri.into() }
    }

    /// Assert that this individual is an instance of a class
    /// DEPRECATED: Use assert instead to ensure proper label and icon
    pub fn assert_type(&self, conn: &mut Connection, class_iri: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(class_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Assert individual with required metadata (label and icon)
    /// This is the recommended way to create individuals
    pub fn assert(
        &self,
        conn: &mut Connection,
        class_iri: &str,
        label: &str,
        icon: &str,
        origin: &str
    ) -> Result<()> {
        // Create individual type
        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(class_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;

        // Add required label
        let label_obj = Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let label_triple = Triple::new(&self.iri, rdfs::LABEL, label_obj);
        store::assert_triples(conn, &[label_triple], origin)?;

        // Add required icon
        let icon_obj = Object::Literal {
            value: icon.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let icon_triple = Triple::new(&self.iri, "foundation:icon", icon_obj);
        store::assert_triples(conn, &[icon_triple], origin)?;

        Ok(())
    }

    /// Add a property value (object property)
    pub fn add_object_property(&self, conn: &mut Connection, property: &str, value_iri: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, property, Object::Iri(value_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add a datatype property value
    pub fn add_datatype_property(&self, conn: &mut Connection, property: &str, value: Object, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, property, value);
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add a literal string property
    pub fn add_string_property(&self, conn: &mut Connection, property: &str, value: &str, language: Option<&str>, origin: &str) -> Result<()> {
        let object = Object::Literal {
            value: value.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: language.map(|l| l.to_string()),
        };
        self.add_datatype_property(conn, property, object, origin)
    }

    /// Add an integer property
    pub fn add_integer_property(&self, conn: &mut Connection, property: &str, value: i64, origin: &str) -> Result<()> {
        let object = Object::Integer(value);
        self.add_datatype_property(conn, property, object, origin)
    }

    /// Add a number property
    pub fn add_number_property(&self, conn: &mut Connection, property: &str, value: f64, origin: &str) -> Result<()> {
        let object = Object::Number(value);
        self.add_datatype_property(conn, property, object, origin)
    }

    /// Add a boolean property
    pub fn add_boolean_property(&self, conn: &mut Connection, property: &str, value: bool, origin: &str) -> Result<()> {
        let object = Object::Boolean(value);
        self.add_datatype_property(conn, property, object, origin)
    }

    /// Add owl:sameAs relationship
    pub fn add_same_as(&self, conn: &mut Connection, other_iri: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, owl::SAME_AS, Object::Iri(other_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Add owl:differentFrom relationship
    pub fn add_different_from(&self, conn: &mut Connection, other_iri: &str, origin: &str) -> Result<()> {
        let triple = Triple::new(&self.iri, owl::DIFFERENT_FROM, Object::Iri(other_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;
        Ok(())
    }

    /// Get all types (classes) of this individual
    pub fn get_types(&self, conn: &Connection) -> Result<Vec<String>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, rdf::TYPE)?;
        Ok(result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|s| s.to_string())
            .collect())
    }

    /// Get all triples where this individual is the subject
    pub fn get_all_properties(&self, conn: &Connection) -> Result<Vec<Triple>> {
        let result = query::get_by_entity(conn, &self.iri)?;
        Ok(result.triples)
    }

    /// Get values of a specific property
    pub fn get_property_values(&self, conn: &Connection, property: &str) -> Result<Vec<Object>> {
        let result = query::get_by_entity_predicate(conn, &self.iri, property)?;
        Ok(result.triples.iter()
            .map(|t| t.object.clone())
            .collect())
    }

    /// Check if this individual exists (has at least one triple)
    pub fn exists(&self, conn: &Connection) -> Result<bool> {
        let result = query::get_by_entity(conn, &self.iri)?;
        Ok(!result.triples.is_empty())
    }

    /// Check if this individual is an instance of a specific class
    pub fn is_instance_of(&self, conn: &Connection, class_iri: &str) -> Result<bool> {
        let types = self.get_types(conn)?;
        Ok(types.contains(&class_iri.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;

    #[test]
    fn test_assert_type() {
        let mut conn = setup_test_db();
        let individual = Individual::new("foundation:john");

        let result = individual.assert_type(&mut conn, "foundation:Person", "test");
        assert!(result.is_ok());

        let types = individual.get_types(&conn).unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0], "foundation:Person");
    }

    #[test]
    fn test_add_properties() {
        let mut conn = setup_test_db();
        let individual = Individual::new("foundation:john");

        individual.assert_type(&mut conn, "foundation:Person", "test").unwrap();
        individual.add_string_property(&mut conn, "foundation:name", "John Doe", Some("en"), "test").unwrap();
        individual.add_integer_property(&mut conn, "foundation:age", 30, "test").unwrap();
        individual.add_boolean_property(&mut conn, "foundation:isActive", true, "test").unwrap();

        let properties = individual.get_all_properties(&conn).unwrap();
        assert!(properties.len() >= 4); // type + name + age + isActive
    }

    #[test]
    fn test_object_property() {
        let mut conn = setup_test_db();
        let john = Individual::new("foundation:john");
        let mary = Individual::new("foundation:mary");

        john.assert_type(&mut conn, "foundation:Person", "test").unwrap();
        mary.assert_type(&mut conn, "foundation:Person", "test").unwrap();
        john.add_object_property(&mut conn, "foundation:knows", "foundation:mary", "test").unwrap();

        let knows_values = john.get_property_values(&conn, "foundation:knows").unwrap();
        assert_eq!(knows_values.len(), 1);
        assert_eq!(knows_values[0].as_iri(), Some("foundation:mary"));
    }

    #[test]
    fn test_is_instance_of() {
        let mut conn = setup_test_db();
        let individual = Individual::new("foundation:myDog");

        individual.assert_type(&mut conn, "foundation:Dog", "test").unwrap();

        assert!(individual.is_instance_of(&conn, "foundation:Dog").unwrap());
        assert!(!individual.is_instance_of(&conn, "foundation:Cat").unwrap());
    }

    #[test]
    fn test_same_as() {
        let mut conn = setup_test_db();
        let individual = Individual::new("foundation:john");

        individual.add_same_as(&mut conn, "dbpedia:John_Doe", "test").unwrap();

        let same_as = individual.get_property_values(&conn, owl::SAME_AS).unwrap();
        assert_eq!(same_as.len(), 1);
        assert_eq!(same_as[0].as_iri(), Some("dbpedia:John_Doe"));
    }
}
