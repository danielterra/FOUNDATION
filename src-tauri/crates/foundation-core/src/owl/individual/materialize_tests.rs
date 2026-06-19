use super::{materialize_individual_shallow, ShallowValue};
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};

fn seed(conn: &mut rusqlite::Connection, subject: &str, predicate: &str, object: Object) {
    store::assert_triples(conn, &[Triple::new(subject, predicate, object)], "test").unwrap();
}

// ── materialize_individual_shallow ───────────────────────────────────────────

#[test]
fn test_materialize_happy_path() {
    let mut conn = setup_test_db();
    seed(&mut conn, "test:Ind", "rdf:type", Object::Iri("test:Class".to_string()));
    seed(&mut conn, "test:Ind", "rdfs:label", Object::Literal {
        value: "My Ind".to_string(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    });

    let map = materialize_individual_shallow(&conn, "test:Ind");
    assert!(!map.is_empty(), "deve retornar mapa não-vazio");

    let types = map.get("rdf:type").expect("rdf:type deve estar presente");
    assert!(types.iter().any(|v| matches!(v, ShallowValue::Iri(iri) if iri == "test:Class")));

    let labels = map.get("rdfs:label").expect("rdfs:label deve estar presente");
    assert!(labels.iter().any(|v| matches!(v, ShallowValue::Literal(s) if s == "My Ind")));
}

#[test]
fn test_materialize_individual_inexistente() {
    let conn = setup_test_db();
    let map = materialize_individual_shallow(&conn, "test:Ghost");
    assert!(map.is_empty(), "individual inexistente deve retornar mapa vazio");
}

#[test]
fn test_materialize_multi_value() {
    let mut conn = setup_test_db();
    // Usa append_triples para gravar múltiplos valores no mesmo predicate
    use crate::eavto::store;
    store::append_triples(&mut conn, &[
        Triple::new("test:Multi", "test:hasTags", Object::Literal {
            value: "rust".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("test:Multi", "test:hasTags", Object::Literal {
            value: "owl".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
        Triple::new("test:Multi", "test:hasTags", Object::Literal {
            value: "sqlite".to_string(), datatype: Some("xsd:string".to_string()), language: None,
        }),
    ], "test").unwrap();

    let map = materialize_individual_shallow(&conn, "test:Multi");
    let tags = map.get("test:hasTags").expect("test:hasTags deve estar presente");
    assert_eq!(tags.len(), 3, "multi-valor: deve ter 3 valores");
    let tag_strs: Vec<&str> = tags.iter().map(|v| match v {
        ShallowValue::Literal(s) => s.as_str(),
        ShallowValue::Iri(s) => s.as_str(),
    }).collect();
    assert!(tag_strs.contains(&"rust"));
    assert!(tag_strs.contains(&"owl"));
    assert!(tag_strs.contains(&"sqlite"));
}

#[test]
fn test_materialize_iri_vs_literal() {
    let mut conn = setup_test_db();
    seed(&mut conn, "test:IriLit", "test:refProp", Object::Iri("test:Target".to_string()));
    seed(&mut conn, "test:IriLit", "test:litProp", Object::Literal {
        value: "42".to_string(),
        datatype: Some("xsd:integer".to_string()),
        language: None,
    });

    let map = materialize_individual_shallow(&conn, "test:IriLit");

    let ref_vals = map.get("test:refProp").expect("refProp deve existir");
    assert!(ref_vals.iter().any(|v| matches!(v, ShallowValue::Iri(_))),
        "valor IRI deve mapeiar para ShallowValue::Iri");

    let lit_vals = map.get("test:litProp").expect("litProp deve existir");
    assert!(lit_vals.iter().any(|v| matches!(v, ShallowValue::Literal(_))),
        "valor literal deve mapeiar para ShallowValue::Literal");
}
