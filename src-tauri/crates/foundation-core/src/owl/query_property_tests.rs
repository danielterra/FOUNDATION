use super::*;
use crate::eavto::test_helpers::setup_test_db;
use crate::eavto::{store, Triple, Object};

fn insert_literal(conn: &mut rusqlite::Connection, subject: &str, predicate: &str, value: &str, datatype: &str) {
    store::assert_triples(conn, &[Triple::new(subject, predicate, Object::Literal {
        value: value.to_string(),
        datatype: Some(datatype.to_string()),
        language: None,
    })], "test").unwrap();
}

fn insert_iri(conn: &mut rusqlite::Connection, subject: &str, predicate: &str, obj: &str) {
    store::assert_triples(conn, &[Triple::new(subject, predicate, Object::Iri(obj.to_string()))], "test").unwrap();
}

fn setup_payment_schema(conn: &mut rusqlite::Connection) {
    insert_iri(conn, "foundation:Payment", "rdf:type", "owl:Class");
    insert_literal(conn, "foundation:Payment", "rdfs:label", "Payment", "xsd:string");

    insert_iri(conn, "foundation:amount", "rdf:type", "owl:DatatypeProperty");
    insert_iri(conn, "foundation:amount", "rdfs:domain", "foundation:Payment");
    insert_iri(conn, "foundation:amount", "rdfs:range", "xsd:decimal");

    insert_iri(conn, "foundation:transactionDate", "rdf:type", "owl:DatatypeProperty");
    insert_iri(conn, "foundation:transactionDate", "rdfs:domain", "foundation:Payment");
    insert_iri(conn, "foundation:transactionDate", "rdfs:range", "xsd:date");

    insert_iri(conn, "foundation:paymentCategory", "rdf:type", "owl:ObjectProperty");
    insert_iri(conn, "foundation:paymentCategory", "rdfs:domain", "foundation:Payment");

    insert_iri(conn, "foundation:Budget", "rdf:type", "owl:Class");

    insert_iri(conn, "foundation:startDate", "rdf:type", "owl:DatatypeProperty");
    insert_iri(conn, "foundation:startDate", "rdfs:domain", "foundation:Budget");
    insert_iri(conn, "foundation:startDate", "rdfs:range", "xsd:date");

    insert_iri(conn, "foundation:endDate", "rdf:type", "owl:DatatypeProperty");
    insert_iri(conn, "foundation:endDate", "rdfs:domain", "foundation:Budget");
    insert_iri(conn, "foundation:endDate", "rdfs:range", "xsd:date");

    insert_iri(conn, "foundation:Payment_1", "rdf:type", "foundation:Payment");
    insert_literal(conn, "foundation:Payment_1", "foundation:transactionDate", "2026-01-15", "xsd:string");
    insert_literal(conn, "foundation:Payment_1", "foundation:amount", "100.0", "xsd:decimal");

    insert_iri(conn, "foundation:Payment_2", "rdf:type", "foundation:Payment");
    insert_literal(conn, "foundation:Payment_2", "foundation:transactionDate", "2026-01-20", "xsd:string");
    insert_literal(conn, "foundation:Payment_2", "foundation:amount", "50.0", "xsd:decimal");

    insert_iri(conn, "foundation:Payment_3", "rdf:type", "foundation:Payment");
    insert_literal(conn, "foundation:Payment_3", "foundation:transactionDate", "2026-02-05", "xsd:string");
    insert_literal(conn, "foundation:Payment_3", "foundation:amount", "200.0", "xsd:decimal");

    insert_iri(conn, "foundation:Budget_1", "rdf:type", "foundation:Budget");
    insert_literal(conn, "foundation:Budget_1", "foundation:startDate", "2026-01-01", "xsd:string");
    insert_literal(conn, "foundation:Budget_1", "foundation:endDate", "2026-01-31", "xsd:string");
}

#[test]
fn parse_valid_query_config() {
    let json = r#"{"targetClass":"foundation:Payment","filters":[{"propertyIri":"foundation:amount","operator":"gt","value":"100"}]}"#;
    let config = parse_query_config(json).unwrap();
    assert_eq!(config.target_class, "foundation:Payment");
    assert_eq!(config.filters.len(), 1);
    assert_eq!(config.filters[0].property_iri, "foundation:amount");
    assert_eq!(config.filters[0].operator, "gt");
    assert_eq!(config.filters[0].value, Some("100".to_string()));
}

#[test]
fn parse_between_query_config() {
    let json = r#"{"targetClass":"foundation:Payment","filters":[{"propertyIri":"foundation:amount","operator":"between","valueFrom":"10","valueTo":"200"}]}"#;
    let config = parse_query_config(json).unwrap();
    assert_eq!(config.filters[0].operator, "between");
    assert_eq!(config.filters[0].value_from, Some("10".to_string()));
    assert_eq!(config.filters[0].value_to, Some("200".to_string()));
}

#[test]
fn validate_rejects_nonexistent_target_class() {
    let conn = setup_test_db();
    let config = QueryConfig {
        target_class: "foundation:NonExistentClass".to_string(),
        filters: vec![],
        order_by: vec![],
        limit: None,
    };
    let result = validate_query_config(&conn, &config);
    assert!(result.is_err());
}

#[test]
fn validate_accepts_existing_target_class() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![],
        order_by: vec![],
        limit: None,
    };
    let result = validate_query_config(&conn, &config);
    assert!(result.is_ok(), "Should accept existing class: {:?}", result);
}

#[test]
fn evaluate_query_returns_all_when_no_filters() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![],
        order_by: vec![],
        limit: None,
    };
    let results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn evaluate_query_eq_literal_filter() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![QueryFilter {
            property_iri: "foundation:transactionDate".to_string(),
            operator: "eq".to_string(),
            value: Some("2026-01-15".to_string()),
            value_from: None,
            value_to: None,
        }],
        order_by: vec![],
        limit: None,
    };
    let results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"foundation:Payment_1".to_string()));
}

#[test]
fn evaluate_query_gte_filter() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![QueryFilter {
            property_iri: "foundation:transactionDate".to_string(),
            operator: "gte".to_string(),
            value: Some("2026-01-20".to_string()),
            value_from: None,
            value_to: None,
        }],
        order_by: vec![],
        limit: None,
    };
    let mut results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    results.sort();
    assert_eq!(results.len(), 2);
    assert!(results.contains(&"foundation:Payment_2".to_string()));
    assert!(results.contains(&"foundation:Payment_3".to_string()));
}

#[test]
fn evaluate_query_self_ref_resolves() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![
            QueryFilter {
                property_iri: "foundation:transactionDate".to_string(),
                operator: "gte".to_string(),
                value: Some("{{self.foundation:startDate}}".to_string()),
                value_from: None,
                value_to: None,
            },
            QueryFilter {
                property_iri: "foundation:transactionDate".to_string(),
                operator: "lte".to_string(),
                value: Some("{{self.foundation:endDate}}".to_string()),
                value_from: None,
                value_to: None,
            },
        ],
        order_by: vec![],
        limit: None,
    };
    let mut results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    results.sort();
    assert_eq!(results.len(), 2, "Should return 2 payments in Jan 2026: {:?}", results);
    assert!(results.contains(&"foundation:Payment_1".to_string()));
    assert!(results.contains(&"foundation:Payment_2".to_string()));
}

#[test]
fn evaluate_query_missing_self_ref_returns_empty() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![QueryFilter {
            property_iri: "foundation:transactionDate".to_string(),
            operator: "gte".to_string(),
            value: Some("{{self.foundation:nonExistentProp}}".to_string()),
            value_from: None,
            value_to: None,
        }],
        order_by: vec![],
        limit: None,
    };
    let results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    assert!(results.is_empty(), "Missing self ref should return empty set");
}

#[test]
fn evaluate_query_exists_operator() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    insert_iri(&mut conn, "foundation:Payment_1", "foundation:paymentCategory", "foundation:Category_Food");

    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![QueryFilter {
            property_iri: "foundation:paymentCategory".to_string(),
            operator: "exists".to_string(),
            value: None,
            value_from: None,
            value_to: None,
        }],
        order_by: vec![],
        limit: None,
    };
    let results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"foundation:Payment_1".to_string()));
}

#[test]
fn evaluate_query_not_exists_operator() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    insert_iri(&mut conn, "foundation:Payment_1", "foundation:paymentCategory", "foundation:Category_Food");

    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![QueryFilter {
            property_iri: "foundation:paymentCategory".to_string(),
            operator: "not_exists".to_string(),
            value: None,
            value_from: None,
            value_to: None,
        }],
        order_by: vec![],
        limit: None,
    };
    let mut results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    results.sort();
    assert_eq!(results.len(), 2);
    assert!(results.contains(&"foundation:Payment_2".to_string()));
    assert!(results.contains(&"foundation:Payment_3".to_string()));
}

#[test]
fn evaluate_query_between_operator() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![QueryFilter {
            property_iri: "foundation:transactionDate".to_string(),
            operator: "between".to_string(),
            value: None,
            value_from: Some("2026-01-01".to_string()),
            value_to: Some("2026-01-31".to_string()),
        }],
        order_by: vec![],
        limit: None,
    };
    let mut results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    results.sort();
    assert_eq!(results.len(), 2);
    assert!(results.contains(&"foundation:Payment_1".to_string()));
    assert!(results.contains(&"foundation:Payment_2".to_string()));
}

#[test]
fn evaluate_query_numeric_gte_filter() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![QueryFilter {
            property_iri: "foundation:amount".to_string(),
            operator: "gte".to_string(),
            value: Some("100".to_string()),
            value_from: None,
            value_to: None,
        }],
        order_by: vec![],
        limit: None,
    };
    let mut results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    results.sort();
    assert_eq!(results.len(), 2, "Should return payments with amount >= 100: {:?}", results);
    assert!(results.contains(&"foundation:Payment_1".to_string()));
    assert!(results.contains(&"foundation:Payment_3".to_string()));
}

#[test]
fn parse_query_config_includes_order_by_and_limit() {
    // Regression: orderBy and limit fields were silently dropped because the struct
    // didn't declare them. Serde ignores unknown fields by default.
    let json = r#"{
        "targetClass":"foundation:Payment",
        "filters":[],
        "orderBy":[{"propertyIri":"foundation:transactionDate","direction":"desc"}],
        "limit":1
    }"#;
    let config = parse_query_config(json).unwrap();
    assert_eq!(config.order_by.len(), 1);
    assert_eq!(config.order_by[0].property_iri, "foundation:transactionDate");
    assert_eq!(config.order_by[0].direction, "desc");
    assert_eq!(config.limit, Some(1));
}

#[test]
fn evaluate_query_respects_limit() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![],
        order_by: vec![],
        limit: Some(2),
    };
    let results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    assert_eq!(results.len(), 2, "limit=2 must truncate to 2 results, got {:?}", results);
}

#[test]
fn evaluate_query_respects_order_by_desc() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![],
        order_by: vec![QueryOrderBy {
            property_iri: "foundation:transactionDate".to_string(),
            direction: "desc".to_string(),
        }],
        limit: None,
    };
    let results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    // Payment_3 (Feb 5) > Payment_2 (Jan 20) > Payment_1 (Jan 15)
    assert_eq!(results, vec![
        "foundation:Payment_3".to_string(),
        "foundation:Payment_2".to_string(),
        "foundation:Payment_1".to_string(),
    ]);
}

#[test]
fn evaluate_query_respects_order_by_asc() {
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![],
        order_by: vec![QueryOrderBy {
            property_iri: "foundation:transactionDate".to_string(),
            direction: "asc".to_string(),
        }],
        limit: None,
    };
    let results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    assert_eq!(results, vec![
        "foundation:Payment_1".to_string(),
        "foundation:Payment_2".to_string(),
        "foundation:Payment_3".to_string(),
    ]);
}

#[test]
fn evaluate_query_combines_order_by_and_limit_for_top_n() {
    // Bug_1777771566070: previousMonthBudget needed orderBy=desc + limit=1 to pick
    // exactly one preceding budget. Without these, it returned all candidates.
    let mut conn = setup_test_db();
    setup_payment_schema(&mut conn);
    let config = QueryConfig {
        target_class: "foundation:Payment".to_string(),
        filters: vec![],
        order_by: vec![QueryOrderBy {
            property_iri: "foundation:transactionDate".to_string(),
            direction: "desc".to_string(),
        }],
        limit: Some(1),
    };
    let results = evaluate_query(&conn, "foundation:Budget_1", &config).unwrap();
    assert_eq!(results, vec!["foundation:Payment_3".to_string()],
        "must return only the most recent payment");
}
