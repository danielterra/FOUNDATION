use serde_json::Value;
use crate::eavto::Connection;
use super::ToolResult;

pub(super) fn create_detail_one(
    conn: &mut Connection, args: &Value,
) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
        },
    };

    let label = match args.get("label").and_then(|v| v.as_str()) {
        Some(label) => label,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: label".to_string()),
        },
    };

    let detail_type_str = match args.get("detail_type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: detail_type".to_string()),
        },
    };

    let detail_type = match detail_type_str {
        "object" => crate::owl::PropertyType::ObjectProperty,
        "datatype" => crate::owl::PropertyType::DatatypeProperty,
        _ => return ToolResult {
            success: false,
            result: None,
            error: Some("Invalid detail_type. Must be 'object' or 'datatype'".to_string()),
        },
    };

    let comment = args.get("comment").and_then(|v| v.as_str());
    let domain_strings: Vec<String> = match args.get("domain") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
        _ => vec![],
    };
    let domains: Vec<&str> = domain_strings.iter().map(|s| s.as_str()).collect();
    let range = args.get("range").and_then(|v| v.as_str());
    let unit = args.get("unit").and_then(|v| v.as_str());
    let formula = args.get("formula").and_then(|v| v.as_str());

    let domain_strings_owned: Vec<String> = domain_strings.clone();

    match (|| {
        use crate::owl::Property;

        let detail = Property::new(iri);
        detail.assert(conn, detail_type, label, comment, &domains, range, unit, "ai")?;

        if let Some(formula_str) = formula {
            if detail_type == crate::owl::PropertyType::ObjectProperty {
                return Err(crate::owl::OwlError::ValidationError(
                    "Formula is only supported on datatype properties, not object properties. \
                     A formula computes a literal value — use detail_type: 'datatype' for calculated fields.".to_string()
                ));
            }
            crate::owl::formula::validate_no_cycle(conn, iri, formula_str)?;
            use crate::eavto::{store, Triple, Object};
            let formula_triple = Triple::new(iri, "foundation:formula", Object::Literal {
                value: formula_str.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            });
            store::assert_triples(conn, &[formula_triple], "ai")?;
        }

        super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
        for domain_iri in &domain_strings_owned {
            super::batch::queue_event("entity-updated", serde_json::json!({"entityId": domain_iri}));
        }

        Ok::<_, crate::owl::OwlError>(serde_json::json!({
            "success": true,
            "iri": iri,
            "message": format!("Detail {} created successfully", label),
        }))
    })() {
        Ok(result) => ToolResult {
            success: true,
            result: Some(result),
            error: None,
        },
        Err(e) => ToolResult {
            success: false,
            result: None,
            error: Some(e.to_string()),
        },
    }
}

