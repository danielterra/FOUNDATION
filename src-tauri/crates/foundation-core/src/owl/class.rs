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
    pub disjoint_with: Vec<Thing>,
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
            disjoint_with: Vec::new(),
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
        let t0 = std::time::Instant::now();

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

        let disjoint_iris = Self::collect_direct_disjoint_iris(conn, &iri)?;
        let disjoint_with: Vec<Thing> = disjoint_iris.iter()
            .map(|d| Thing::get(conn, d))
            .collect();

        let properties = Self::get_properties(conn, &iri)?;

        // Window function picks the row with MAX(tx) per (subject, predicate) across ALL rows
        // (including historically-retracted ones), then filters to retracted=0. This correctly
        // handles entities whose rdf:type was ever superseded without a retraction triple,
        // and avoids the O(N) correlated subquery that previously took ~79ms for 1178 emails.
        let backlink_total: usize = conn.query_row(
            "SELECT COUNT(DISTINCT subject) FROM (
                SELECT subject,
                       MAX(tx) OVER (PARTITION BY subject, predicate) AS max_tx,
                       tx, retracted
                FROM triples
                WHERE predicate = 'rdf:type' AND object = ?
             ) WHERE tx = max_tx AND retracted = 0",
            rusqlite::params![&iri],
            |row| row.get(0),
        ).unwrap_or(0);

        let mut instance_stmt = conn.prepare(
            "SELECT DISTINCT subject FROM (
                SELECT subject,
                       MAX(tx) OVER (PARTITION BY subject, predicate) AS max_tx,
                       tx, retracted
                FROM triples
                WHERE predicate = 'rdf:type' AND object = ?
             ) WHERE tx = max_tx AND retracted = 0
             ORDER BY tx DESC LIMIT ?"
        ).map_err(|e| crate::owl::OwlError::DatabaseError(e.to_string()))?;
        let instance_iris: Vec<String> = instance_stmt
            .query_map(rusqlite::params![&iri, CLASS_INSTANCE_LIMIT as i64], |row| row.get(0))
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

        let elapsed = t0.elapsed().as_millis();
        if elapsed > 20 {
            crate::diagnostics::log_backend("debug", &format!(
                "[OWL] Class::get({}) instances={} props={} {}ms",
                iri, backlink_total, properties.len(), elapsed
            ));
        }

        Ok(Some(Self {
            iri,
            label,
            icon,
            comment,
            types,
            super_classes,
            sub_classes,
            disjoint_with,
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

    /// Get the given class IRI plus all ancestor class IRIs (BFS traversal upward via rdfs:subClassOf).
    pub fn get_ancestor_iris(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        queue.push_back(class_iri.to_string());

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            result.push(current.clone());
            let parents = query::get_by_entity_predicate(conn, &current, rdfs::SUB_CLASS_OF)?;
            for triple in parents.triples {
                if let Some(parent_iri) = triple.object.as_iri() {
                    if !visited.contains(parent_iri) {
                        queue.push_back(parent_iri.to_string());
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get properties whose rdfs:domain is in `domain_class_iris`, bounded by `limit`/`offset`.
    ///
    /// Returns `(property_iri, label, icon, first_range, property_type_str)` tuples.
    /// Fully parametric — no Foundation/Anthropic IRIs hardcoded.
    /// `limit = 0` means no cap (caller is responsible for choosing a safe bound).
    pub fn get_properties_for_domain_classes(
        conn: &Connection,
        domain_class_iris: &[String],
        property_type_iris: &[&str],
    ) -> Result<Vec<(String, Option<String>, Option<String>, Option<String>, String)>> {
        Self::get_properties_for_domain_classes_bounded(conn, domain_class_iris, property_type_iris, 500, 0)
    }

    /// Paginated variant; `limit = 0` means no cap.
    pub fn get_properties_for_domain_classes_bounded(
        conn: &Connection,
        domain_class_iris: &[String],
        property_type_iris: &[&str],
        limit: usize,
        offset: usize,
    ) -> Result<Vec<(String, Option<String>, Option<String>, Option<String>, String)>> {
        if domain_class_iris.is_empty() {
            return Ok(vec![]);
        }

        let mut prop_iris: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for class_iri in domain_class_iris {
            let result = query::get_by_predicate_object(conn, rdfs::DOMAIN, class_iri)?;
            for triple in result.triples {
                if seen.insert(triple.subject.clone()) {
                    prop_iris.push(triple.subject);
                }
            }
        }

        if prop_iris.is_empty() {
            return Ok(vec![]);
        }

        let prop_triples_map = query::batch_load_triples_for_subjects(conn, &prop_iris)?;

        let mut results = Vec::new();
        for prop_iri in &prop_iris {
            let triples = match prop_triples_map.get(prop_iri) {
                Some(t) => t,
                None => continue,
            };

            let prop_type_iri = triples.iter()
                .filter(|t| t.predicate == rdf::TYPE)
                .find_map(|t| t.object.as_iri().map(|s| s.to_string()));

            let prop_type_str = prop_type_iri.as_deref().unwrap_or("");
            let matches_type = property_type_iris.is_empty()
                || property_type_iris.iter().any(|&pt| pt == prop_type_str);
            if !matches_type {
                continue;
            }

            let label = triples.iter()
                .find(|t| t.predicate == rdfs::LABEL)
                .and_then(|t| t.object.as_literal());

            let icon = triples.iter()
                .find(|t| t.predicate == "foundation:icon")
                .and_then(|t| t.object.as_literal().or_else(|| t.object.as_iri().map(|s| s.to_string())));

            let range = triples.iter()
                .find(|t| t.predicate == rdfs::RANGE)
                .and_then(|t| t.object.as_iri().map(|s| s.to_string()));

            let type_category = match prop_type_str {
                t if t == "owl:ObjectProperty" => "object",
                t if t == "owl:DatatypeProperty" => "datatype",
                _ => "datatype",
            };

            results.push((
                prop_iri.clone(),
                label,
                icon,
                range,
                type_category.to_string(),
            ));
        }

        results.sort_by(|a, b| a.0.cmp(&b.0));
        if limit > 0 {
            Ok(results.into_iter().skip(offset).take(limit).collect())
        } else {
            Ok(results.into_iter().skip(offset).collect())
        }
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

    /// Direct owl:disjointWith targets — pairwise triples plus co-members of every
    /// owl:AllDisjointClasses set this class participates in.
    fn collect_direct_disjoint_iris(conn: &Connection, iri: &str) -> Result<Vec<String>> {
        let mut result = Self::get_direct_disjoint_pair_iris(conn, iri)?;
        let mut seen: std::collections::HashSet<String> = result.iter().cloned().collect();

        for adc_iri in Self::find_all_disjoint_class_sets(conn, iri)? {
            let members = Self::get_all_disjoint_classes_members(conn, &adc_iri)?;
            for m in members {
                if m != iri && seen.insert(m.clone()) {
                    result.push(m);
                }
            }
        }

        Ok(result)
    }

    /// Class IRIs pairwise disjoint with `iri` via direct owl:disjointWith triples.
    /// Excludes co-members of owl:AllDisjointClasses sets — those are returned by
    /// `find_all_disjoint_class_sets` + `get_all_disjoint_classes_members` instead.
    /// Symmetric: matches both (iri, ⊥, ?) and (?, ⊥, iri) directions.
    pub fn get_direct_disjoint_pair_iris(conn: &Connection, iri: &str) -> Result<Vec<String>> {
        let mut result: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        let forward = query::get_by_entity_predicate(conn, iri, owl::DISJOINT_WITH)?;
        for triple in forward.triples {
            if let Some(other) = triple.object.as_iri() {
                if other != iri && seen.insert(other.to_string()) {
                    result.push(other.to_string());
                }
            }
        }
        let backward = query::get_by_predicate_object(conn, owl::DISJOINT_WITH, iri)?;
        for triple in backward.triples {
            if triple.subject != iri && seen.insert(triple.subject.clone()) {
                result.push(triple.subject);
            }
        }
        Ok(result)
    }

    /// All blank-node IRIs of type owl:AllDisjointClasses whose owl:members list
    /// contains the given class IRI.
    pub fn find_all_disjoint_class_sets(conn: &Connection, class_iri: &str) -> Result<Vec<String>> {
        let typed = query::get_by_predicate_object(conn, rdf::TYPE, owl::ALL_DISJOINT_CLASSES)?;
        let mut hits = Vec::new();
        for t in typed.triples {
            let adc_iri = t.subject;
            let members = Self::get_all_disjoint_classes_members(conn, &adc_iri)?;
            if members.iter().any(|m| m == class_iri) {
                hits.push(adc_iri);
            }
        }
        Ok(hits)
    }

    /// Resolve owl:members of an AllDisjointClasses node to a flat list of class IRIs.
    pub fn get_all_disjoint_classes_members(conn: &Connection, adc_iri: &str) -> Result<Vec<String>> {
        let members_triple = query::get_by_entity_predicate(conn, adc_iri, owl::MEMBERS)?;
        if let Some(triple) = members_triple.triples.first() {
            if let Some(list_head) = triple.object.as_iri() {
                return Self::parse_rdf_list(conn, list_head);
            }
        }
        Ok(Vec::new())
    }

    /// Replace pairwise owl:disjointWith targets for `iri` with the given list.
    /// Asserts the symmetric triple (B owl:disjointWith A) so reads from either side agree.
    /// Does NOT touch owl:AllDisjointClasses sets that include `iri` — passing []
    /// only clears direct pair triples; the class will still appear disjoint with the
    /// other ADC members until the ADC is retracted via `retract_all_disjoint_classes`.
    pub fn set_disjoint_with(
        conn: &mut Connection,
        iri: &str,
        disjoint_with: &[&str],
        origin: &str,
    ) -> Result<()> {
        for d in disjoint_with {
            if *d == iri {
                return Err(OwlError::ValidationError(format!(
                    "Class '{}' cannot be declared disjoint with itself", iri
                )));
            }
            if !Self::exists(conn, d) {
                return Err(OwlError::ValidationError(format!(
                    "Class '{}' does not exist — cannot declare disjointness with it", d
                )));
            }
        }

        let forward = query::get_by_entity_predicate(conn, iri, owl::DISJOINT_WITH)?;
        for triple in forward.triples {
            store::retract_triples(
                conn,
                &[Triple::new(iri, owl::DISJOINT_WITH, triple.object)],
                origin,
            )?;
        }
        let backward = query::get_by_predicate_object(conn, owl::DISJOINT_WITH, iri)?;
        for triple in backward.triples {
            store::retract_triples(
                conn,
                &[Triple::new(triple.subject, owl::DISJOINT_WITH, Object::Iri(iri.to_string()))],
                origin,
            )?;
        }

        let mut new_triples = Vec::new();
        for d in disjoint_with {
            new_triples.push(Triple::new(iri, owl::DISJOINT_WITH, Object::Iri(d.to_string())));
            new_triples.push(Triple::new(*d, owl::DISJOINT_WITH, Object::Iri(iri.to_string())));
        }
        if !new_triples.is_empty() {
            store::assert_triples(conn, &new_triples, origin)?;
        }
        Ok(())
    }

    /// Append pairwise owl:disjointWith between `iri` and `disjoint_iri` (idempotent).
    /// Used by inspector commands that grow the set incrementally.
    pub fn add_disjoint_with(
        conn: &mut Connection,
        iri: &str,
        disjoint_iri: &str,
        origin: &str,
    ) -> Result<()> {
        if iri == disjoint_iri {
            return Err(OwlError::ValidationError(format!(
                "Class '{}' cannot be declared disjoint with itself", iri
            )));
        }
        if !Self::exists(conn, disjoint_iri) {
            return Err(OwlError::ValidationError(format!(
                "Class '{}' does not exist — cannot declare disjointness with it", disjoint_iri
            )));
        }

        let existing = Self::collect_direct_disjoint_iris(conn, iri)?;
        if existing.iter().any(|d| d == disjoint_iri) {
            return Ok(());
        }

        let triples = vec![
            Triple::new(iri, owl::DISJOINT_WITH, Object::Iri(disjoint_iri.to_string())),
            Triple::new(disjoint_iri, owl::DISJOINT_WITH, Object::Iri(iri.to_string())),
        ];
        store::assert_triples(conn, &triples, origin)?;
        Ok(())
    }

    /// Retract a pairwise owl:disjointWith between `iri` and `disjoint_iri` (both directions).
    pub fn remove_disjoint_with(
        conn: &mut Connection,
        iri: &str,
        disjoint_iri: &str,
        origin: &str,
    ) -> Result<()> {
        let triples = vec![
            Triple::new(iri, owl::DISJOINT_WITH, Object::Iri(disjoint_iri.to_string())),
            Triple::new(disjoint_iri, owl::DISJOINT_WITH, Object::Iri(iri.to_string())),
        ];
        store::retract_triples(conn, &triples, origin)?;
        Ok(())
    }

    /// Create a new owl:AllDisjointClasses blank node listing the given members.
    /// Returns the blank node IRI. Identical member sets reuse the same deterministic IRI,
    /// so calling twice with the same members is idempotent.
    /// After a retract + re-assert of the same set the original blank IRI is reused;
    /// the early-return below skips list rebuild only when the previous triples are
    /// still active (unretracted), so a cycle of retract → assert cleanly recreates the set.
    pub fn assert_all_disjoint_classes(
        conn: &mut Connection,
        members: &[&str],
        origin: &str,
    ) -> Result<String> {
        if members.len() < 2 {
            return Err(OwlError::ValidationError(
                "owl:AllDisjointClasses requires at least 2 members".to_string()
            ));
        }
        let mut sorted: Vec<&str> = members.to_vec();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != members.len() {
            return Err(OwlError::ValidationError(
                "owl:AllDisjointClasses members must be distinct".to_string()
            ));
        }
        for m in &sorted {
            if !Self::exists(conn, m) {
                return Err(OwlError::ValidationError(format!(
                    "Class '{}' does not exist — cannot include in AllDisjointClasses", m
                )));
            }
        }

        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for m in &sorted {
            hasher.update(m.as_bytes());
            hasher.update(b":");
        }
        let hash = hasher.finalize();
        let adc_iri = format!(
            "_:adc_{}",
            hash[..8].iter().map(|b| format!("{:02x}", b)).collect::<String>()
        );

        let existing_members = Self::get_all_disjoint_classes_members(conn, &adc_iri)?;
        if !existing_members.is_empty() {
            return Ok(adc_iri);
        }

        let mut list_triples: Vec<Triple> = Vec::new();
        let mut nodes: Vec<String> = (0..sorted.len())
            .map(|i| format!("{}_l{}", adc_iri, i))
            .collect();

        for (i, member) in sorted.iter().enumerate() {
            let node = &nodes[i];
            list_triples.push(Triple::new(
                node, rdf::FIRST, Object::Iri(member.to_string()),
            ));
            let rest = if i + 1 == sorted.len() {
                Object::Iri(rdf::NIL.to_string())
            } else {
                Object::Blank(nodes[i + 1].clone())
            };
            list_triples.push(Triple::new(node, rdf::REST, rest));
        }
        let head = nodes.remove(0);

        list_triples.push(Triple::new(
            &adc_iri, rdf::TYPE, Object::Iri(owl::ALL_DISJOINT_CLASSES.to_string()),
        ));
        list_triples.push(Triple::new(
            &adc_iri, owl::MEMBERS, Object::Blank(head),
        ));

        store::assert_triples(conn, &list_triples, origin)?;
        Ok(adc_iri)
    }

    /// Retract every triple of an AllDisjointClasses blank node, including its
    /// owl:members RDF list nodes.
    pub fn retract_all_disjoint_classes(
        conn: &mut Connection,
        adc_iri: &str,
        origin: &str,
    ) -> Result<()> {
        let members_result = query::get_by_entity_predicate(conn, adc_iri, owl::MEMBERS)?;
        if let Some(members_triple) = members_result.triples.first() {
            if let Some(list_head) = members_triple.object.as_iri() {
                let mut current = list_head.to_string();
                loop {
                    if current == rdf::NIL {
                        break;
                    }
                    let node_triples = query::get_by_entity(conn, &current)?;
                    let next = node_triples.triples.iter()
                        .find(|t| t.predicate == rdf::REST)
                        .and_then(|t| t.object.as_iri().map(|s| s.to_string()))
                        .unwrap_or_else(|| rdf::NIL.to_string());
                    let triples: Vec<Triple> = node_triples.triples.into_iter()
                        .map(|t| Triple::new(t.subject, t.predicate, t.object))
                        .collect();
                    if !triples.is_empty() {
                        store::retract_triples(conn, &triples, origin)?;
                    }
                    current = next;
                }
            }
        }

        let adc_triples = query::get_by_entity(conn, adc_iri)?;
        let to_retract: Vec<Triple> = adc_triples.triples.into_iter()
            .map(|t| Triple::new(t.subject, t.predicate, t.object))
            .collect();
        if !to_retract.is_empty() {
            store::retract_triples(conn, &to_retract, origin)?;
        }
        Ok(())
    }

    /// Effective set of class IRIs that conflict with `iri`, expanded over the
    /// subClassOf hierarchy: a disjointness on an ancestor implies disjointness on
    /// all descendants.
    pub fn get_effective_disjoint_iris(
        conn: &Connection,
        iri: &str,
    ) -> Result<std::collections::HashSet<String>> {
        let mut result = std::collections::HashSet::new();
        let ancestors = Self::ancestors_inclusive(conn, iri)?;
        for ancestor in &ancestors {
            for direct in Self::collect_direct_disjoint_iris(conn, ancestor)? {
                for descendant in Self::get_descendant_iris(conn, &direct)? {
                    result.insert(descendant);
                }
            }
        }
        Ok(result)
    }

    /// `iri` plus every class reachable by walking rdfs:subClassOf upward.
    fn ancestors_inclusive(conn: &Connection, iri: &str) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(iri.to_string());
        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            result.push(current.clone());
            let supers = query::get_by_entity_predicate(conn, &current, rdfs::SUB_CLASS_OF)?;
            for triple in supers.triples {
                if let Some(parent) = triple.object.as_iri() {
                    if !visited.contains(parent) {
                        queue.push_back(parent.to_string());
                    }
                }
            }
        }
        Ok(result)
    }

    /// Reject super-class lists that mix branches declared disjoint.
    /// Used by define_class before persisting subClassOf triples.
    pub fn validate_super_classes_not_disjoint(
        conn: &Connection,
        super_classes: &[&str],
    ) -> Result<()> {
        for (i, a) in super_classes.iter().enumerate() {
            let conflicts = Self::get_effective_disjoint_iris(conn, a)?;
            for b in &super_classes[i + 1..] {
                if conflicts.contains(*b) {
                    return Err(OwlError::ValidationError(format!(
                        "Super-classes '{}' and '{}' are declared disjoint — \
                         cannot be combined as parents of the same class",
                        a, b
                    )));
                }
            }
        }
        Ok(())
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

#[cfg(test)]
#[path = "class_disjoint_tests.rs"]
mod disjoint_tests;
