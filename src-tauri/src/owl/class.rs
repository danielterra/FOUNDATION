use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, OwlError, Thing, vocabulary::{rdf, rdfs, owl}};

const CLASS_INSTANCE_LIMIT: usize = 50;

#[derive(Debug, Clone)]
pub struct Class {
    pub iri: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub comment: Option<String>,
    pub types: Vec<Thing>,
    pub super_classes: Vec<Thing>,
    pub sub_classes: Vec<Thing>,
    pub properties: Vec<(String, String)>,
    pub backlinks: Vec<(String, String, Object)>,
    pub backlink_total: usize,
    pub one_of_values: Vec<String>,
    pub concept_properties: Vec<(String, Object)>,
}

impl Class {
    /// Create a new empty Class reference (only IRI)
    pub fn new(iri: impl Into<String>) -> Self {
        Self {
            iri: iri.into(),
            label: None,
            icon: None,
            comment: None,
            types: Vec::new(),
            super_classes: Vec::new(),
            sub_classes: Vec::new(),
            properties: Vec::new(),
            backlinks: Vec::new(),
            backlink_total: 0,
            one_of_values: Vec::new(),
            concept_properties: Vec::new(),
        }
    }

    /// Cheap existence check: returns true iff `iri` is a known class in the graph.
    /// Accepts OWL/RDFS built-in roots that may not have explicit triples.
    pub fn exists(conn: &Connection, iri: &str) -> bool {
        if matches!(iri, "owl:Thing" | "rdfs:Resource" | "rdfs:Class" | "owl:Class") {
            return true;
        }
        let types_result = match query::get_by_entity_predicate(conn, iri, rdf::TYPE) {
            Ok(r) => r,
            Err(_) => return false,
        };
        types_result.triples.iter().any(|t| {
            t.object.as_iri()
                .map(|type_iri| type_iri == rdfs::CLASS || type_iri == owl::CLASS)
                .unwrap_or(false)
        })
    }

    /// Parse an RDF list (rdf:first/rdf:rest) into a Vec of IRIs
    pub(crate) fn parse_rdf_list(conn: &Connection, list_head: &str) -> Result<Vec<String>> {
        let mut values = Vec::new();
        let mut current = list_head.to_string();

        loop {
            if current == rdf::NIL {
                break;
            }

            let first_result = query::get_by_entity_predicate(conn, &current, rdf::FIRST)?;
            if let Some(triple) = first_result.triples.first() {
                if let Some(iri) = triple.object.as_iri() {
                    values.push(iri.to_string());
                }
            }

            let rest_result = query::get_by_entity_predicate(conn, &current, rdf::REST)?;
            if let Some(triple) = rest_result.triples.first() {
                if let Some(iri) = triple.object.as_iri() {
                    current = iri.to_string();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(values)
    }

    /// Get complete class data from database
    pub fn get(conn: &Connection, iri: impl Into<String>) -> Result<Option<Self>> {
        let iri = iri.into();

        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE)?;
        let is_class = types_result.triples.iter().any(|t| {
            t.object.as_iri()
                .map(|type_iri| type_iri == rdfs::CLASS || type_iri == owl::CLASS)
                .unwrap_or(false)
        });
        if !is_class {
            return Ok(None);
        }

        let label_result = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL)?;
        let label = label_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let icon_result = query::get_by_entity_predicate(conn, &iri, "foundation:hasIcon")?;
        let icon = icon_result.triples.first()
            .and_then(|t| match &t.object {
                crate::eavto::Object::Iri(icon_iri) => crate::owl::icon_iri_to_display(conn, icon_iri),
                crate::eavto::Object::Literal { value, .. } =>
                    Some(crate::owl::icon_literal_to_display(value)),
                _ => None,
            });

        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT)?;
        let comment = comment_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let types: Vec<Thing> = types_result.triples.iter()
            .filter_map(|t| t.object.as_iri())
            .map(|type_iri| Thing::get(conn, type_iri))
            .collect();

        let super_result = query::get_by_entity_predicate(conn, &iri, rdfs::SUB_CLASS_OF)?;
        let super_classes: Vec<Thing> = super_result.triples.iter()
            .filter_map(|t| match &t.object {
                Object::Iri(iri) => Some(iri.as_str()),
                _ => None,
            })
            .map(|super_iri| Thing::get(conn, super_iri))
            .collect();

        let sub_result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, &iri)?;
        let sub_classes: Vec<Thing> = sub_result.triples.iter()
            .map(|t| Thing::get(conn, &t.subject))
            .collect();

        let properties = Self::get_properties(conn, &iri)?;

        let backlink_total: usize = conn.query_row(
            "SELECT COUNT(DISTINCT subject) FROM triples t
             WHERE predicate = 'rdf:type' AND object = ? AND retracted = 0
               AND t.tx = (SELECT MAX(tx) FROM triples WHERE subject = t.subject AND predicate = 'rdf:type' AND object = ?)",
            rusqlite::params![&iri, &iri],
            |row| row.get(0),
        ).unwrap_or(0);

        let mut instance_stmt = conn.prepare(
            "SELECT DISTINCT subject FROM triples t
             WHERE predicate = 'rdf:type' AND object = ? AND retracted = 0
               AND t.tx = (SELECT MAX(tx) FROM triples WHERE subject = t.subject AND predicate = 'rdf:type' AND object = ?)
             ORDER BY tx DESC LIMIT ?"
        ).map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        let instance_iris: Vec<String> = instance_stmt
            .query_map(rusqlite::params![&iri, &iri, CLASS_INSTANCE_LIMIT as i64], |row| row.get(0))
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        let backlinks: Vec<(String, String, Object)> = instance_iris.into_iter()
            .map(|subject| (subject, rdf::TYPE.to_string(), Object::Iri(iri.clone())))
            .collect();

        let one_of_result = query::get_by_entity_predicate(conn, &iri, owl::ONE_OF)?;
        let one_of_values = if let Some(triple) = one_of_result.triples.first() {
            if let Some(list_head) = triple.object.as_iri() {
                Self::parse_rdf_list(conn, list_head)?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        const SKIP: &[&str] = &[
            rdf::TYPE, rdfs::LABEL, rdfs::COMMENT, rdfs::SUB_CLASS_OF,
            "foundation:hasIcon", "foundation:allowedStatus", owl::ONE_OF,
        ];

        let all_triples_result = query::get_by_entity(conn, &iri)?;
        let concept_properties: Vec<(String, Object)> = all_triples_result.triples
            .into_iter()
            .filter(|t| !SKIP.contains(&t.predicate.as_str()) && !matches!(t.object, Object::Blank(_)))
            .map(|t| (t.predicate, t.object))
            .collect();

        Ok(Some(Self {
            iri,
            label,
            icon,
            comment,
            types,
            super_classes,
            sub_classes,
            properties,
            backlinks,
            backlink_total,
            one_of_values,
            concept_properties,
        }))
    }

    /// Check if a property is valid for a class (declared, universal, or inherited).
    /// Much cheaper than Class::get() — does not load instances or backlinks.
    pub fn has_property(conn: &Connection, class_iri: &str, property_iri: &str) -> bool {
        Self::get_properties(conn, class_iri)
            .map(|props| props.iter().any(|(p, _)| p == property_iri))
            .unwrap_or(false)
    }

    /// Get all properties for this class (declared, used, and inherited)
    /// Returns Vec<(property_iri, source_class_iri)>
    pub fn get_properties(
        conn: &Connection,
        class_iri: &str
    ) -> Result<Vec<(String, String)>> {
        let mut all_properties: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let declared_result = query::get_by_predicate_object(conn, rdfs::DOMAIN, class_iri)?;
        for triple in declared_result.triples {
            if seen.insert(triple.subject.clone()) {
                all_properties.push((triple.subject.clone(), class_iri.to_string()));
            }
        }

        for universal_class in &["owl:Thing", "rdfs:Resource"] {
            let universal_props_result =
                query::get_by_predicate_object(conn, rdfs::DOMAIN, universal_class)?;
            for triple in universal_props_result.triples {
                if seen.insert(triple.subject.clone()) {
                    all_properties.push((triple.subject.clone(), universal_class.to_string()));
                }
            }
        }

        let super_result = query::get_by_entity_predicate(conn, class_iri, rdfs::SUB_CLASS_OF)?;
        let super_classes: Vec<String> = super_result.triples.iter()
            .filter_map(|t| match &t.object {
                Object::Iri(iri) | Object::Blank(iri) => Some(iri.clone()),
                _ => None,
            })
            .collect();

        for super_class_iri in super_classes {
            if super_class_iri != "owl:Thing" && super_class_iri != "rdfs:Resource" {
                let inherited_props = Self::get_properties(conn, &super_class_iri)?;
                for (prop, source) in inherited_props {
                    if seen.insert(prop.clone()) {
                        all_properties.push((prop, source));
                    }
                }
            }
        }

        Ok(all_properties)
    }

    /// Assert class with required metadata (label and icon)
    /// If super_class is None, automatically assigns owl:Thing as parent
    pub fn assert(
        &self,
        conn: &mut Connection,
        class_type: ClassType,
        label: &str,
        icon: &str,
        super_class: Option<&str>,
        origin: &str
    ) -> Result<()> {
        crate::owl::check_system_locked(conn, &self.iri, None)?;
        let type_iri = match class_type {
            ClassType::RdfsClass => rdfs::CLASS,
            ClassType::OwlClass => owl::CLASS,
        };

        let triple = Triple::new(&self.iri, rdf::TYPE, Object::Iri(type_iri.to_string()));
        store::assert_triples(conn, &[triple], origin)?;

        let label_obj = Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        let label_triple = Triple::new(&self.iri, rdfs::LABEL, label_obj);
        store::assert_triples(conn, &[label_triple], origin)?;

        let (icon_pred, icon_obj) = crate::owl::icon_store_value(icon);
        let icon_triple = Triple::new(&self.iri, icon_pred, icon_obj);
        store::assert_triples(conn, &[icon_triple], origin)?;

        let parent = super_class.unwrap_or(owl::THING);
        let subclass_triple =
            Triple::new(&self.iri, rdfs::SUB_CLASS_OF, Object::Iri(parent.to_string()));
        store::assert_triples(conn, &[subclass_triple], origin)?;

        Ok(())
    }


    /// Get all instances of this class and all its subclasses (polymorphic, returned as IRIs only)
    pub fn get_instances(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let descendant_iris = Self::get_descendant_iris(conn, class_iri)?;
        let mut seen = std::collections::HashSet::new();
        let mut instances = Vec::new();
        for iri in &descendant_iris {
            let result = query::get_by_predicate_object(conn, rdf::TYPE, iri)?;
            for t in result.triples {
                if seen.insert(t.subject.clone()) {
                    instances.push(t.subject);
                }
            }
        }
        Ok(instances)
    }

    /// Get all class IRIs (owl:Class and rdfs:Class)
    pub fn find_all_iris(conn: &Connection) -> Result<Vec<String>> {
        let owl_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::CLASS)?;
        let rdfs_result = query::get_by_predicate_object(conn, rdf::TYPE, rdfs::CLASS)?;
        let mut iris: Vec<String> = owl_result.triples.into_iter()
            .chain(rdfs_result.triples)
            .map(|t| t.subject)
            .collect();
        iris.sort();
        iris.dedup();
        Ok(iris)
    }

    /// Get IRIs of all direct subclasses
    pub fn get_subclass_iris(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, class_iri)?;
        Ok(result.triples.into_iter().map(|t| t.subject).collect())
    }

    /// Get the given class IRI plus all descendant class IRIs (BFS traversal of rdfs:subClassOf)
    pub fn get_descendant_iris(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        queue.push_back(class_iri.to_string());

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            result.push(current.clone());
            for child in Self::get_subclass_iris(conn, &current)? {
                if !visited.contains(&child) {
                    queue.push_back(child);
                }
            }
        }

        Ok(result)
    }

    /// Replace the label of an existing class
    pub fn set_label(conn: &mut Connection, iri: &str, label: &str, origin: &str) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::LABEL)?;
        for triple in old.triples {
            store::retract_triples(conn, &[Triple::new(iri, rdfs::LABEL, triple.object)], origin)?;
        }
        store::assert_triples(conn, &[Triple::new(iri, rdfs::LABEL, Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })], origin)?;
        Ok(())
    }

    /// Replace the comment of an existing class (or add one if not present)
    pub fn set_comment(conn: &mut Connection, iri: &str, comment: &str, origin: &str) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::COMMENT)?;
        for triple in old.triples {
            store::retract_triples(conn, &[Triple::new(iri, rdfs::COMMENT, triple.object)], origin)?;
        }
        store::assert_triples(conn, &[Triple::new(iri, rdfs::COMMENT, Object::Literal {
            value: comment.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })], origin)?;
        Ok(())
    }

    /// Replace the icon of an existing class (validates icon name)
    pub fn set_icon(conn: &mut Connection, iri: &str, icon: &str, origin: &str) -> Result<()> {
        crate::owl::validate_icon(conn, icon)?;
        let (icon_pred, icon_obj) = crate::owl::icon_store_value(icon);
        store::assert_triples(conn, &[Triple::new(iri, icon_pred, icon_obj)], origin)?;
        Ok(())
    }

    /// Replace all rdfs:subClassOf relationships with the given list.
    ///
    /// Only IRI-type subClassOf triples are replaced. Blank node triples
    /// (OWL restriction nodes added by set_class_required_fields) are preserved.
    pub fn set_super_classes(
        conn: &mut Connection,
        iri: &str,
        super_classes: &[&str],
        origin: &str,
    ) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::SUB_CLASS_OF)?;
        for triple in old.triples {
            if matches!(triple.object, Object::Iri(_)) {
                store::retract_triples(
                    conn,
                    &[Triple::new(iri, rdfs::SUB_CLASS_OF, triple.object)],
                    origin,
                )?;
            }
        }
        let new_triples: Vec<Triple> = super_classes
            .iter()
            .map(|sc| Triple::new(iri, rdfs::SUB_CLASS_OF, Object::Iri(sc.to_string())))
            .collect();
        store::append_triples(conn, &new_triples, origin)?;
        Ok(())
    }

    /// Replace the rdfs:subClassOf relationship of an existing class with a single superclass
    pub fn set_super_class(
        conn: &mut Connection,
        iri: &str,
        super_class: &str,
        origin: &str,
    ) -> Result<()> {
        Self::set_super_classes(conn, iri, &[super_class], origin)
    }

    /// Restore a retracted class and all instances that were cascade-deleted with it.
    /// Re-asserts triples as new rows (immutable store — never mutates existing rows).
    /// Only restores instances retracted in the same cascade (tx >= class_retract_tx).
    pub fn restore(conn: &mut Connection, iri: &str, origin: &str) -> Result<usize> {
        use crate::owl::Individual;

        let class_retract_tx = query::get_retraction_tx(conn, iri)?
            .ok_or_else(|| OwlError::NotFound(
                format!("Class '{}' has no retracted triples to restore", iri)
            ))?;

        Individual::restore(conn, iri, origin)?;

        let instance_iris: Vec<String> = conn.prepare(
            "SELECT DISTINCT subject FROM triples
             WHERE predicate = 'rdf:type' AND object = ? AND retracted = 1 AND tx >= ?"
        ).map_err(|e| OwlError::DatabaseError(e.to_string()))
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![iri, class_retract_tx], |row| row.get(0))
                .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
                .map_err(|e| OwlError::DatabaseError(e.to_string()))
        })?;

        let count = instance_iris.len();
        for instance_iri in instance_iris {
            Individual::restore(conn, &instance_iri, origin)?;
        }

        Ok(count)
    }

    /// Retract all triples about this class IRI
    pub fn retract_all(conn: &mut Connection, iri: &str, origin: &str) -> Result<()> {
        crate::owl::check_system_locked(conn, iri, None)?;
        let result = query::get_by_entity(conn, iri)?;
        let triples: Vec<Triple> = result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        store::retract_triples(conn, &triples, origin)?;
        Ok(())
    }
}

/// Type of class (RDFS or OWL)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassType {
    #[allow(dead_code)]
    RdfsClass,
    OwlClass,
}

#[cfg(test)]
#[path = "class_tests.rs"]
mod tests;
