use serde_json::{Value, json};
use crate::eavto::Connection;
use crate::owl::{Property, PropertyType};
use super::ToolResult;

#[cfg(test)]
#[path = "property_tests.rs"]
mod tests;

pub fn learn_property(
    conn: &mut Connection,
    args: &Value,
    app: Option<&tauri::AppHandle>,
) -> ToolResult {
    super::batch::run_atomic(conn, args, app, learn_property_one)
}

fn learn_property_one(conn: &mut Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    let existing = Property::get(conn, iri).ok().flatten();
    let is_new = existing.is_none();

    let label = args.get("label").and_then(|v| v.as_str())
        .or_else(|| existing.as_ref().and_then(|p| p.label.as_deref()));

    if is_new && label.is_none() {
        return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: label (required when creating a new property)".to_string()),
            concept: None,
        };
    }

    let property_type = match args.get("property_type").and_then(|v| v.as_str()) {
        Some("object") => PropertyType::ObjectProperty,
        Some("datatype") => PropertyType::DatatypeProperty,
        Some(other) => return ToolResult {
            success: false,
            result: None,
            error: Some(format!("Invalid property_type '{}'. Must be 'object' or 'datatype'", other)),
            concept: None,
        },
        None if is_new => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: property_type (required when creating a new property)".to_string()),
            concept: None,
        },
        None => existing.as_ref().map(|p| p.property_type).unwrap_or(PropertyType::ObjectProperty),
    };

    let domain_strings: Vec<String> = existing.as_ref()
        .map(|p| p.domains.clone())
        .unwrap_or_default();
    let domains: Vec<&str> = domain_strings.iter().map(|s| s.as_str()).collect();

    let range = args.get("range").and_then(|v| v.as_str())
        .or_else(|| existing.as_ref().and_then(|p| p.ranges.first().map(|s| s.as_str())));

    let comment = args.get("comment").and_then(|v| v.as_str());

    let unit = args.get("unit").and_then(|v| v.as_str())
        .or_else(|| existing.as_ref().and_then(|p| p.unit.as_deref()));

    let label_str = label.unwrap_or("");

    match (|| {
        Property::new(iri).assert(conn, property_type, label_str, comment, &domains, range, unit, "ai")?;

        if let Some(formula_str) = args.get("formula").and_then(|v| v.as_str()) {
            if property_type != PropertyType::DatatypeProperty {
                return Err(crate::owl::OwlError::ValidationError(
                    "formula is only supported on datatype properties".to_string(),
                ));
            }
            crate::owl::formula::validate_no_cycle(conn, iri, formula_str)?;
            use crate::eavto::{store, Triple, Object};
            store::assert_triples(conn, &[Triple::new(iri, "foundation:formula", Object::Literal {
                value: formula_str.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            })], "ai")?;
        }

        super::batch::queue_event("entity-updated", json!({"entityId": iri}));
        for domain_iri in &domain_strings {
            super::batch::queue_event("entity-updated", json!({"entityId": domain_iri}));
        }

        Ok::<_, crate::owl::OwlError>(json!({
            "iri": iri,
        }))
    })() {
        Ok(result) => ToolResult { success: true, result: Some(result), error: None, concept: None },
        Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
    }
}

pub fn forget_property(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> ToolResult {
    super::batch::run_atomic(conn, args, app, |conn, op| {
        let iri = match op.get("iri").and_then(|v| v.as_str()) {
            Some(iri) => iri,
            None => return ToolResult {
                success: false,
                result: None,
                error: Some("Missing required parameter: iri".to_string()),
                concept: None,
            },
        };

        match Property::retract(conn, iri, "ai") {
            Ok(_) => {
                super::batch::queue_event("entity-updated", serde_json::json!({"entityId": iri}));
                ToolResult { success: true, result: Some(serde_json::json!({"iri": iri})), error: None, concept: None }
            }
            Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
        }
    })
}

pub fn remember_property(conn: &Connection, args: &Value) -> ToolResult {
    super::batch::run_multi_read(conn, args, remember_property_one)
}

fn remember_property_one(conn: &Connection, args: &Value) -> ToolResult {
    if args.get("iri").is_some() {
        get_property_one(conn, args)
    } else {
        search_properties_one(conn, args)
    }
}

fn get_property_one(conn: &Connection, args: &Value) -> ToolResult {
    let iri = match args.get("iri").and_then(|v| v.as_str()) {
        Some(iri) => iri,
        None => return ToolResult {
            success: false,
            result: None,
            error: Some("Missing required parameter: iri".to_string()),
            concept: None,
        },
    };

    match (|| {
        let prop = Property::get(conn, iri)?
            .ok_or_else(|| crate::owl::OwlError::NotFound(iri.to_string()))?;

        let property_type = match prop.property_type {
            PropertyType::ObjectProperty => "object",
            PropertyType::DatatypeProperty => "datatype",
            PropertyType::AnnotationProperty => "annotation",
            PropertyType::RdfProperty => "rdf",
        };

        Ok::<_, crate::owl::OwlError>(json!({
            "iri": prop.iri,
            "label": prop.label,
            "comment": prop.comment,
            "property_type": property_type,
            "domains": prop.domains,
            "ranges": prop.ranges,
            "unit": prop.unit,
            "formula": prop.formula,
        }))
    })() {
        Ok(result) => ToolResult { success: true, result: Some(result), error: None, concept: None },
        Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
    }
}

fn search_properties_one(conn: &Connection, args: &Value) -> ToolResult {
    let query_str = args.get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let limit = args.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;

    let offset = args.get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    match (|| {
        let all_iris = Property::find_all_iris(conn)?;
        let query_lower = query_str.to_lowercase();

        let mut results: Vec<Value> = Vec::new();
        for iri in &all_iris {
            let prop = match Property::get(conn, iri)? {
                Some(p) => p,
                None => continue,
            };

            if !query_str.is_empty() {
                let label_lower = prop.label.as_deref().unwrap_or("").to_lowercase();
                let comment_lower = prop.comment.as_deref().unwrap_or("").to_lowercase();
                let iri_lower = iri.to_lowercase();
                if !label_lower.contains(&query_lower)
                    && !comment_lower.contains(&query_lower)
                    && !iri_lower.contains(&query_lower)
                {
                    continue;
                }
            }

            let property_type = match prop.property_type {
                PropertyType::ObjectProperty => "object",
                PropertyType::DatatypeProperty => "datatype",
                PropertyType::AnnotationProperty => "annotation",
                PropertyType::RdfProperty => "rdf",
            };

            results.push(json!({
                "iri": prop.iri,
                "label": prop.label,
                "property_type": property_type,
                "domains": prop.domains,
                "ranges": prop.ranges,
            }));
        }

        let total = results.len();
        let paginated: Vec<_> = results.into_iter().skip(offset).take(limit).collect();

        Ok::<_, crate::owl::OwlError>(json!({
            "properties": paginated,
            "count": paginated.len(),
            "total": total,
            "limit": limit,
            "offset": offset,
        }))
    })() {
        Ok(result) => ToolResult { success: true, result: Some(result), error: None, concept: None },
        Err(e) => ToolResult { success: false, result: None, error: Some(e.to_string()), concept: None },
    }
}
