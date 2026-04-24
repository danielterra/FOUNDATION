use crate::ai::providers::ClaudeProvider;
use crate::ai::functions::get_available_tools;
use crate::owl::{DbExecutor, Individual, Object};
use crate::process_automation::agent_task::run_headless_tool_loop;
use tauri::AppHandle;

const ONTOLOGIST_AGENT: &str = "foundation:SoftwareAgent_1773313705318";
const EXTRACTION_CONFIDENCE_THRESHOLD: f64 = 0.8;
const MAX_EMAIL_BODY_CHARS: usize = 4000;
const EXTRACTION_TIMEOUT_SECS: u64 = 120;
const MAX_TOOL_LOOPS: usize = 8;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct ExtractionItem {
    class_iri: String,
    label: String,
    #[serde(default)]
    properties: std::collections::HashMap<String, String>,
    confidence: f64,
}

pub async fn extract_email_entities(
    _app: &AppHandle,
    executor: &DbExecutor,
    email_iri: String,
    email_subject: String,
    email_body: String,
    email_from: String,
) {
    let config = match load_ontologist_config(executor).await {
        Ok(c) => c,
        Err(e) => {
            crate::imap::log_error(&format!("extraction config: {}", e));
            return;
        }
    };

    let tools = get_available_tools()
        .into_iter()
        .filter(|t| matches!(t.name.as_str(), "search" | "describe_class"))
        .map(|t| t.to_claude_tool())
        .collect();

    let provider = ClaudeProvider::with_model(config.0, config.1, config.3);
    let prompt = build_extraction_prompt(&email_from, &email_subject, &email_body);

    let response = match run_headless_tool_loop(
        executor, &provider, config.2, prompt, tools, MAX_TOOL_LOOPS,
    ).await {
        Ok(r) => r,
        Err(e) => {
            crate::imap::log_error(&format!("extraction generate: {}", e));
            return;
        }
    };

    let items: Vec<ExtractionItem> = match parse_extraction_response(&response) {
        Ok(items) => items,
        Err(e) => {
            crate::imap::log_error(&format!("extraction parse: {}", e));
            return;
        }
    };

    const BLOCKED_CLASSES: &[&str] = &[
        "foundation:Email",
        "foundation:EmailAddress",
        "foundation:IMAPAccount",
        "foundation:IMAPSyncLog",
    ];

    let items: Vec<ExtractionItem> = items.into_iter().filter(|item| {
        if BLOCKED_CLASSES.contains(&item.class_iri.as_str()) {
            crate::imap::log_error(&format!("extraction: blocked infrastructure class {}", item.class_iri));
            false
        } else {
            true
        }
    }).collect();

    if items.is_empty() {
        return;
    }

    executor
        .write(move |conn| {
            let ts = chrono::Utc::now().timestamp_millis();
            for (i, item) in items.iter().enumerate() {
                let unique_id = ts + i as i64;
                if item.confidence >= EXTRACTION_CONFIDENCE_THRESHOLD {
                    let iri = format!("foundation:Extracted_{}", unique_id);
                    persist_entity(conn, &iri, item, &email_iri).ok();
                } else {
                    persist_pending(conn, item, &email_iri, unique_id).ok();
                }
            }
            Ok(String::new())
        })
        .await
        .ok();
}

fn persist_entity(
    conn: &mut crate::eavto::Connection,
    iri: &str,
    item: &ExtractionItem,
    email_iri: &str,
) -> Result<(), String> {
    Individual::new(iri)
        .assert(conn, &item.class_iri, &item.label, "smart_toy", "imap")
        .map_err(|e| e.to_string())?;
    for (prop, val) in &item.properties {
        let obj = property_value_to_object(conn, prop, val);
        Individual::new(iri)
            .add_property(conn, prop, vec![obj], "imap")
            .ok();
    }
    Individual::new(iri)
        .add_property(conn, "foundation:derivedFromEmail", vec![Object::Iri(email_iri.to_string())], "imap")
        .ok();
    Ok(())
}

fn typed_lit(v: &str, dt: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some(dt.to_string()), language: None }
}

fn property_value_to_object(
    conn: &crate::eavto::Connection,
    prop: &str,
    val: &str,
) -> Object {
    let range = crate::owl::get_iri_property(conn, prop, "rdfs:range")
        .unwrap_or_default()
        .unwrap_or_default();
    match range.as_str() {
        "xsd:dateTime" => typed_lit(val, "xsd:dateTime"),
        "xsd:date" => typed_lit(val, "xsd:date"),
        "xsd:boolean" => typed_lit(val, "xsd:boolean"),
        "xsd:anyURI" => typed_lit(val, "xsd:anyURI"),
        "xsd:integer" | "xsd:int" | "xsd:long"
        | "xsd:nonNegativeInteger" | "xsd:positiveInteger"
        | "xsd:decimal" | "xsd:float" | "xsd:double" =>
            typed_lit(val, &range),
        r if !r.is_empty() && !r.starts_with("xsd:") && !r.starts_with("rdfs:") =>
            Object::Iri(val.to_string()),
        _ => str_lit(val),
    }
}

fn persist_pending(
    conn: &mut crate::eavto::Connection,
    item: &ExtractionItem,
    email_iri: &str,
    unique_id: i64,
) -> Result<(), String> {
    let iri = format!("foundation:PendingExtraction_{}", unique_id);
    let payload = serde_json::to_string(item).map_err(|e| e.to_string())?;
    Individual::new(&iri)
        .assert(conn, "foundation:PendingExtraction", &item.label, "pending_actions", "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&iri)
        .add_property(conn, "foundation:extractionEmail", vec![Object::Iri(email_iri.to_string())], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&iri)
        .add_property(conn, "foundation:extractionPayload", vec![str_lit(&payload)], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&iri)
        .add_property(conn, "foundation:extractionConfidence", vec![decimal_lit(item.confidence)], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&iri)
        .add_property(conn, "foundation:hasStatus", vec![Object::Iri("foundation:Pending".to_string())], "imap")
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn str_lit(v: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None }
}

fn decimal_lit(v: f64) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:decimal".to_string()), language: None }
}

async fn load_ontologist_config(executor: &DbExecutor) -> Result<(String, String, String, u64), String> {
    executor.read(move |conn| {
        let service_iri = crate::owl::get_iri_property(conn, ONTOLOGIST_AGENT, "foundation:usesService")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Ontologist has no usesService".to_string())?;

        let api_key_iri = crate::owl::get_iri_property(conn, &service_iri, "foundation:apiKey")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Ontologist service has no apiKey".to_string())?;

        let api_key = crate::owl::get_literal_property(conn, &api_key_iri, "foundation:credentialValue")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "API key has no value".to_string())?;

        let model_iri = crate::owl::get_iri_property(conn, ONTOLOGIST_AGENT, "foundation:usesModel")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Ontologist has no usesModel".to_string())?;

        let model_id = crate::owl::get_literal_property(conn, &model_iri, "foundation:modelIdentifier")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Model has no modelIdentifier".to_string())?;

        let base = crate::commands::chat::settings::load_base_system_prompt(conn);
        let agent_prompt = crate::owl::get_literal_property(conn, ONTOLOGIST_AGENT, "foundation:basePrompt")
            .unwrap_or_default()
            .unwrap_or_default();
        let system_prompt = if agent_prompt.is_empty() {
            base
        } else {
            format!("{}\n\n{}", base, agent_prompt)
        };

        let timeout_secs = crate::owl::get_literal_property(conn, ONTOLOGIST_AGENT, "foundation:requestTimeout")
            .unwrap_or_default()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(EXTRACTION_TIMEOUT_SECS);

        Ok((api_key, model_id, system_prompt, timeout_secs))
    }).await
}

fn build_extraction_prompt(from: &str, subject: &str, body: &str) -> String {
    let body_truncated = if body.len() > MAX_EMAIL_BODY_CHARS {
        &body[..MAX_EMAIL_BODY_CHARS]
    } else {
        body
    };
    format!(
        "## Email\n\nFrom: {from}\nSubject: {subject}\n\nBody:\n{body}\n\n\
        ## Task\n\n\
        Analyze the email. Identify concrete entities that should be created in the knowledge graph.\n\
        Use `search` and `describe_class` to look up relevant class schemas before extracting.\n\
        Return ONLY a raw JSON array (no markdown, no explanation). Each item:\n\
        {{\"class_iri\":\"foundation:ClassName\",\"label\":\"entity name\",\
        \"properties\":{{\"foundation:propName\":\"value\"}},\"confidence\":0.0}}\n\n\
        - confidence: 0.0-1.0 (certainty this entity is real and relevant)\n\
        - Only include concrete, factual entities explicitly mentioned in the email\n\
        - NEVER use foundation:Email, foundation:EmailAddress, foundation:IMAPAccount or any messaging/infrastructure class\n\
        - Use only property IRIs confirmed via describe_class\n\
        - dateTime values must be ISO 8601 format (e.g. \"2026-04-24T09:30:00\")\n\
        - Return [] if no relevant domain entities found",
        from = from,
        subject = subject,
        body = body_truncated
    )
}

fn parse_extraction_response(content: &str) -> Result<Vec<ExtractionItem>, String> {
    let start = content.find('[').ok_or("no JSON array in response")?;
    let end = content.rfind(']').ok_or("no JSON array end in response")?;
    serde_json::from_str(&content[start..=end]).map_err(|e| e.to_string())
}
