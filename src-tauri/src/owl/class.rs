use crate::eavto::Connection;
use crate::eavto::{store, query, Triple, Object};
use crate::owl::{Result, Thing, vocabulary::{rdf, rdfs, owl}};

#[derive(Debug, Clone)]
pub struct Class {
    pub iri: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub comment: Option<String>,
    pub types: Vec<Thing>,
    pub super_classes: Vec<Thing>,
    pub sub_classes: Vec<Thing>,
    pub properties: Vec<(String, String)>, // (property_iri, source_class_iri)
    pub backlinks: Vec<(String, String, Object)>, // (source_entity, property_iri, value)
    pub one_of_values: Vec<String>, // owl:oneOf enumerated individuals
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
            one_of_values: Vec::new(),
        }
    }

    /// Parse an RDF list (rdf:first/rdf:rest) into a Vec of IRIs
    pub(crate) async fn parse_rdf_list(conn: &Connection, list_head: &str) -> Result<Vec<String>> {
        let mut values = Vec::new();
        let mut current = list_head.to_string();

        loop {
            if current == rdf::NIL {
                break;
            }

            let first_result = query::get_by_entity_predicate(conn, &current, rdf::FIRST).await?;
            if let Some(triple) = first_result.triples.first() {
                if let Some(iri) = triple.object.as_iri() {
                    values.push(iri.to_string());
                }
            }

            let rest_result = query::get_by_entity_predicate(conn, &current, rdf::REST).await?;
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
    pub async fn get(conn: &Connection, iri: impl Into<String>) -> Result<Option<Self>> {
        let iri = iri.into();

        let types_result = query::get_by_entity_predicate(conn, &iri, rdf::TYPE).await?;
        let is_class = types_result.triples.iter().any(|t| {
            t.object.as_iri()
                .map(|type_iri| type_iri == rdfs::CLASS || type_iri == owl::CLASS)
                .unwrap_or(false)
        });
        if !is_class {
            return Ok(None);
        }

        let label_result = query::get_by_entity_predicate(conn, &iri, rdfs::LABEL).await?;
        let label = label_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let icon_result = query::get_by_entity_predicate(conn, &iri, "foundation:hasIcon").await?;
        let has_icon_iri = icon_result.triples.first()
            .and_then(|t| t.object.as_iri())
            .map(|s| s.to_string());
        let icon = if let Some(icon_iri) = has_icon_iri {
            crate::owl::icon_iri_to_display(conn, &icon_iri).await
        } else {
            query::get_by_entity_predicate(conn, &iri, "foundation:icon").await.ok()
                .and_then(|r| r.triples.into_iter().next().and_then(|t| t.object.as_literal().map(|s| s.to_string())))
        };

        let comment_result = query::get_by_entity_predicate(conn, &iri, rdfs::COMMENT).await?;
        let comment = comment_result.triples.first()
            .and_then(|t| t.object.as_literal());

        let mut types: Vec<Thing> = Vec::new();
        for t in &types_result.triples {
            if let Some(type_iri) = t.object.as_iri() {
                types.push(Thing::get(conn, type_iri).await);
            }
        }

        let super_result = query::get_by_entity_predicate(conn, &iri, rdfs::SUB_CLASS_OF).await?;
        let mut super_classes: Vec<Thing> = Vec::new();
        for t in &super_result.triples {
            if let Object::Iri(super_iri) = &t.object {
                super_classes.push(Thing::get(conn, super_iri.as_str()).await);
            }
        }

        let sub_result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, &iri).await?;
        let mut sub_classes: Vec<Thing> = Vec::new();
        for t in &sub_result.triples {
            sub_classes.push(Thing::get(conn, &t.subject).await);
        }

        let properties = Self::get_properties(conn, &iri).await?;

        let backlinks_result = query::get_by_predicate_object(conn, rdf::TYPE, &iri).await?;
        let backlinks: Vec<(String, String, Object)> = backlinks_result.triples.iter()
            .map(|t| {
                (t.subject.clone(), rdf::TYPE.to_string(), Object::Iri(iri.clone()))
            })
            .collect();

        let one_of_result = query::get_by_entity_predicate(conn, &iri, owl::ONE_OF).await?;
        let one_of_values = if let Some(triple) = one_of_result.triples.first() {
            if let Some(list_head) = triple.object.as_iri() {
                Self::parse_rdf_list(conn, list_head).await?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

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
            one_of_values,
        }))
    }

    /// Get all properties for this class (declared, used, and inherited)
    /// Returns Vec<(property_iri, source_class_iri)>
    async fn get_properties(
        conn: &Connection,
        class_iri: &str
    ) -> Result<Vec<(String, String)>> {
        let mut all_properties: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let declared_result = query::get_by_predicate_object(conn, rdfs::DOMAIN, class_iri).await?;
        for triple in declared_result.triples {
            if seen.insert(triple.subject.clone()) {
                all_properties.push((triple.subject.clone(), class_iri.to_string()));
            }
        }

        for universal_class in &["owl:Thing", "rdfs:Resource"] {
            let universal_props_result =
                query::get_by_predicate_object(conn, rdfs::DOMAIN, universal_class).await?;
            for triple in universal_props_result.triples {
                if seen.insert(triple.subject.clone()) {
                    all_properties.push((triple.subject.clone(), universal_class.to_string()));
                }
            }
        }

        let super_result = query::get_by_entity_predicate(conn, class_iri, rdfs::SUB_CLASS_OF).await?;
        let super_classes: Vec<String> = super_result.triples.iter()
            .filter_map(|t| match &t.object {
                Object::Iri(iri) => Some(iri.clone()),
                _ => None,
            })
            .collect();

        for super_class_iri in super_classes {
            if super_class_iri != "owl:Thing" && super_class_iri != "rdfs:Resource" {
                let inherited_props = Box::pin(Self::get_properties(conn, &super_class_iri)).await?;
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
    pub async fn assert(
        &self,
        conn: &Connection,
        class_type: ClassType,
        label: &str,
        icon: &str,
        super_class: Option<&str>,
        origin: &str
    ) -> Result<()> {
        let type_iri = match class_type {
            ClassType::RdfsClass => rdfs::CLASS,
            ClassType::OwlClass => owl::CLASS,
        };

        let (icon_pred, icon_obj) = crate::owl::icon_store_value(icon);
        let parent = super_class.unwrap_or(owl::THING);
        store::assert_triples(conn, &[
            Triple::new(&self.iri, rdf::TYPE, Object::Iri(type_iri.to_string())),
            Triple::new(&self.iri, rdfs::LABEL, Object::Literal {
                value: label.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }),
            Triple::new(&self.iri, icon_pred, icon_obj),
            Triple::new(&self.iri, rdfs::SUB_CLASS_OF, Object::Iri(parent.to_string())),
        ], origin).await.map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Get all instances of this class and all its subclasses (polymorphic, returned as IRIs only)
    pub async fn get_instances(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let descendant_iris = Self::get_descendant_iris(conn, class_iri).await?;
        let mut seen = std::collections::HashSet::new();
        let mut instances = Vec::new();
        for iri in &descendant_iris {
            let result = query::get_by_predicate_object(conn, rdf::TYPE, iri).await?;
            for t in result.triples {
                if seen.insert(t.subject.clone()) {
                    instances.push(t.subject);
                }
            }
        }
        Ok(instances)
    }

    /// Get all class IRIs (owl:Class and rdfs:Class)
    pub async fn find_all_iris(conn: &Connection) -> Result<Vec<String>> {
        let owl_result = query::get_by_predicate_object(conn, rdf::TYPE, owl::CLASS).await?;
        let rdfs_result = query::get_by_predicate_object(conn, rdf::TYPE, rdfs::CLASS).await?;
        let mut iris: Vec<String> = owl_result.triples.into_iter()
            .chain(rdfs_result.triples)
            .map(|t| t.subject)
            .collect();
        iris.sort();
        iris.dedup();
        Ok(iris)
    }

    /// Get IRIs of all direct subclasses
    pub async fn get_subclass_iris(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let result = query::get_by_predicate_object(conn, rdfs::SUB_CLASS_OF, class_iri).await?;
        Ok(result.triples.into_iter().map(|t| t.subject).collect())
    }

    /// Get the given class IRI plus all descendant class IRIs (BFS traversal of rdfs:subClassOf)
    pub async fn get_descendant_iris(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        queue.push_back(class_iri.to_string());

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            result.push(current.clone());
            for child in Self::get_subclass_iris(conn, &current).await? {
                if !visited.contains(&child) {
                    queue.push_back(child);
                }
            }
        }

        Ok(result)
    }

    /// Replace the label of an existing class
    pub async fn set_label(conn: &Connection, iri: &str, label: &str, origin: &str) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::LABEL).await?;
        for triple in old.triples {
            store::retract_triples(conn, &[Triple::new(iri, rdfs::LABEL, triple.object)], origin).await
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        }
        store::assert_triples(conn, &[Triple::new(iri, rdfs::LABEL, Object::Literal {
            value: label.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })], origin).await
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Replace the comment of an existing class (or add one if not present)
    pub async fn set_comment(conn: &Connection, iri: &str, comment: &str, origin: &str) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::COMMENT).await?;
        for triple in old.triples {
            store::retract_triples(conn, &[Triple::new(iri, rdfs::COMMENT, triple.object)], origin).await
                .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        }
        store::assert_triples(conn, &[Triple::new(iri, rdfs::COMMENT, Object::Literal {
            value: comment.to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        })], origin).await
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Replace the icon of an existing class (validates icon name)
    pub async fn set_icon(conn: &Connection, iri: &str, icon: &str, origin: &str) -> Result<()> {
        crate::owl::validate_icon(conn, icon).await?;
        let (icon_pred, icon_obj) = crate::owl::icon_store_value(icon);
        store::assert_triples(conn, &[Triple::new(iri, icon_pred, icon_obj)], origin).await
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Replace all rdfs:subClassOf relationships with the given list.
    ///
    /// Only IRI-type subClassOf triples are replaced. Blank node triples
    /// (OWL restriction nodes added by set_class_required_fields) are preserved.
    pub async fn set_super_classes(
        conn: &Connection,
        iri: &str,
        super_classes: &[&str],
        origin: &str,
    ) -> Result<()> {
        let old = query::get_by_entity_predicate(conn, iri, rdfs::SUB_CLASS_OF).await?;
        for triple in old.triples {
            if matches!(triple.object, Object::Iri(_)) {
                store::retract_triples(
                    conn,
                    &[Triple::new(iri, rdfs::SUB_CLASS_OF, triple.object)],
                    origin,
                ).await.map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
            }
        }
        let new_triples: Vec<Triple> = super_classes
            .iter()
            .map(|sc| Triple::new(iri, rdfs::SUB_CLASS_OF, Object::Iri(sc.to_string())))
            .collect();
        store::append_triples(conn, &new_triples, origin).await
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Replace the rdfs:subClassOf relationship of an existing class with a single superclass
    pub async fn set_super_class(
        conn: &Connection,
        iri: &str,
        super_class: &str,
        origin: &str,
    ) -> Result<()> {
        Self::set_super_classes(conn, iri, &[super_class], origin).await
    }

    /// Retract all triples about this class IRI
    pub async fn retract_all(conn: &Connection, iri: &str, origin: &str) -> Result<()> {
        let result = query::get_by_entity(conn, iri).await?;
        let triples: Vec<Triple> = result.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        store::retract_triples(conn, &triples, origin).await
            .map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
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
