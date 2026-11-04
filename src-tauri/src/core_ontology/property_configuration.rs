use crate::owl::{self, Connection, Object};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct PropertyPresentation {
    pub mode: Option<String>,
    pub language: Option<String>,
}

pub fn property_presentation(
    conn: &Connection,
    property_iri: &str,
) -> owl::Result<PropertyPresentation> {
    Ok(PropertyPresentation {
        mode: owl::get_literal_property(conn, property_iri, "foundation:propertyPresentation")?,
        language: owl::get_literal_property(conn, property_iri, "foundation:propertyLanguage")?,
    })
}

pub const SECRET_MASK: &str = "••••••••";

const ALWAYS_SECRET: &[&str] = &["foundation:credentialValue"];

pub fn secret_property_iris(conn: &Connection) -> owl::Result<HashSet<String>> {
    let mut properties: HashSet<String> = ALWAYS_SECRET.iter().map(|iri| iri.to_string()).collect();
    for property_iri in owl::find_entities_with_predicate(conn, "foundation:isSecret")? {
        let is_secret = owl::get_all_current_triples(conn, &property_iri)?
            .into_iter()
            .find(|triple| triple.predicate == "foundation:isSecret")
            .map(|triple| match triple.object {
                Object::Boolean(value) => value,
                Object::Literal { value, .. } => value == "true" || value == "1",
                _ => false,
            })
            .unwrap_or(false);
        if is_secret {
            properties.insert(property_iri);
        }
    }
    Ok(properties)
}

pub fn property_dependencies(conn: &Connection) -> owl::Result<HashMap<String, String>> {
    let mut dependencies = HashMap::new();
    for property_iri in owl::find_entities_with_predicate(conn, "foundation:dependsOnProperty")? {
        if let Some(parent_iri) =
            owl::get_iri_property(conn, &property_iri, "foundation:dependsOnProperty")?
        {
            dependencies.insert(property_iri, parent_iri);
        }
    }
    Ok(dependencies)
}
