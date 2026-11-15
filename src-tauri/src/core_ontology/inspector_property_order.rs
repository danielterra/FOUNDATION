use crate::owl::{self, Class, Connection, Object, Triple};
use std::collections::HashSet;

const ORDER_PROPERTY: &str = "foundation:inspectorPropertyOrder";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyOrder {
    pub property_iris: Vec<String>,
    pub schema_fingerprint: String,
}

fn schema_properties(conn: &Connection, class_iri: &str) -> Result<Vec<String>, String> {
    let mut properties: Vec<String> = Class::get_properties(conn, class_iri)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|(property_iri, _)| property_iri)
        .filter(|property_iri| property_iri != ORDER_PROPERTY)
        .collect();
    properties.sort();
    properties.dedup();
    Ok(properties)
}

fn fingerprint(properties: &[String]) -> Result<String, String> {
    serde_json::to_string(properties).map_err(|error| error.to_string())
}

pub fn for_class(conn: &Connection, class_iri: &str) -> Result<PropertyOrder, String> {
    let schema = schema_properties(conn, class_iri)?;
    let schema_fingerprint = fingerprint(&schema)?;
    let saved = owl::get_literal_property(conn, class_iri, ORDER_PROPERTY)
        .map_err(|error| error.to_string())?
        .map(|value| serde_json::from_str::<Vec<String>>(&value))
        .transpose()
        .map_err(|error| format!("Ordem de propriedades salva inválida: {error}"))?
        .unwrap_or_default();

    let schema_set: HashSet<&str> = schema.iter().map(String::as_str).collect();
    let mut ordered = Vec::with_capacity(schema.len());
    let mut seen = HashSet::new();
    for property_iri in saved {
        if schema_set.contains(property_iri.as_str()) && seen.insert(property_iri.clone()) {
            ordered.push(property_iri);
        }
    }
    for property_iri in schema {
        if seen.insert(property_iri.clone()) {
            ordered.push(property_iri);
        }
    }

    Ok(PropertyOrder {
        property_iris: ordered,
        schema_fingerprint,
    })
}

pub fn for_types(conn: &Connection, type_iris: &[String]) -> Result<Vec<String>, String> {
    let mut classes = type_iris.to_vec();
    classes.sort_by(|left, right| {
        class_depth(conn, right)
            .cmp(&class_depth(conn, left))
            .then_with(|| left.cmp(right))
    });

    let mut ordered = Vec::new();
    let mut seen = HashSet::new();
    for class_iri in classes {
        for property_iri in for_class(conn, &class_iri)?.property_iris {
            if seen.insert(property_iri.clone()) {
                ordered.push(property_iri);
            }
        }
    }
    Ok(ordered)
}

fn class_depth(conn: &Connection, class_iri: &str) -> usize {
    fn visit(conn: &Connection, class_iri: &str, seen: &mut HashSet<String>) -> usize {
        if !seen.insert(class_iri.to_string()) {
            return 0;
        }
        let Ok(Some(class)) = Class::get(conn, class_iri) else {
            return 0;
        };
        let depth = class
            .super_classes
            .iter()
            .map(|parent| visit(conn, &parent.iri, seen).saturating_add(1))
            .max()
            .unwrap_or(0);
        seen.remove(class_iri);
        depth
    }

    visit(conn, class_iri, &mut HashSet::new())
}

pub fn save(
    conn: &mut crate::owl::Connection,
    class_iri: &str,
    property_iris: Vec<String>,
    expected_fingerprint: &str,
) -> Result<PropertyOrder, String> {
    if !Class::exists(conn, class_iri) {
        return Err(format!("Classe não encontrada: {class_iri}"));
    }
    if owl::is_system_locked(conn, class_iri) {
        return Err(format!("A classe {class_iri} está bloqueada para edição"));
    }

    let current = for_class(conn, class_iri)?;
    if current.schema_fingerprint != expected_fingerprint {
        return Err("O schema da classe mudou. Recarregue o inspetor antes de salvar.".into());
    }

    let schema_properties: Vec<String> =
        serde_json::from_str(&current.schema_fingerprint).map_err(|error| error.to_string())?;
    let schema: HashSet<&str> = schema_properties.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    if property_iris
        .iter()
        .any(|iri| !schema.contains(iri.as_str()) || !seen.insert(iri.as_str()))
    {
        return Err("A ordem contém propriedades repetidas ou fora do schema atual.".into());
    }
    if property_iris.len() != schema.len() {
        return Err("A ordem deve incluir todas as propriedades do schema atual.".into());
    }

    let value = serde_json::to_string(&property_iris).map_err(|error| error.to_string())?;
    owl::assert_raw_triples(
        conn,
        &[Triple::new(
            class_iri,
            ORDER_PROPERTY,
            Object::Literal {
                value,
                datatype: Some("xsd:string".into()),
                language: None,
            },
        )],
        "user",
    )?;

    Ok(PropertyOrder {
        property_iris,
        schema_fingerprint: current.schema_fingerprint,
    })
}
