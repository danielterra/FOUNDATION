use rusqlite;
use crate::ai::providers::ClaudeProvider;
use crate::ai::functions::get_available_tools;
use crate::owl::{DbExecutor, Individual, Object};
use crate::process_automation::agent_task::run_headless_tool_loop;
use tauri::{AppHandle, Emitter};

const ONTOLOGIST_AGENT: &str = "foundation:SoftwareAgent_1773313705318";
const EXTRACTION_CONFIDENCE_THRESHOLD: f64 = 0.8;
const MAX_EMAIL_BODY_CHARS: usize = 4000;
const EXTRACTION_TIMEOUT_SECS: u64 = 120;
const MAX_TOOL_LOOPS: usize = 8;
const EXTRACTION_TEMPERATURE: f32 = 0.0;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct ExtractionItem {
    class_iri: String,
    label: String,
    #[serde(default)]
    properties: std::collections::HashMap<String, String>,
    confidence: f64,
    #[serde(default)]
    existing_iri: Option<String>,
}

pub async fn extract_email_entities(
    app: &AppHandle,
    executor: &DbExecutor,
    email_iri: String,
    email_subject: String,
    email_body: String,
    email_from: String,
    email_to: Vec<String>,
) {
    let _ = app.emit("imap-extraction-started", serde_json::json!({
        "emailIri": email_iri,
        "subject": email_subject,
    }));

    let result = run_extraction(
        app, executor, &email_iri, &email_subject, &email_body, &email_from, &email_to,
    ).await;

    let (entities_count, error) = match result {
        Ok(n) => (Some(n), None),
        Err(e) => {
            crate::imap::log_error(&format!("extraction: {}", e));
            (None, Some(e))
        }
    };

    let _ = app.emit("imap-extraction-finished", serde_json::json!({
        "emailIri": email_iri,
        "subject": email_subject,
        "entitiesCount": entities_count,
        "error": error,
    }));
}

async fn run_extraction(
    _app: &AppHandle,
    executor: &DbExecutor,
    email_iri: &str,
    email_subject: &str,
    email_body: &str,
    email_from: &str,
    email_to: &[String],
) -> Result<usize, String> {
    let config = load_ontologist_config(executor).await
        .map_err(|e| format!("config: {}", e))?;

    let tools = get_available_tools()
        .into_iter()
        .filter(|t| matches!(t.name.as_str(), "search" | "describe_class" | "describe_individual"))
        .map(|t| t.to_claude_tool())
        .collect();

    let participants = resolve_participants(executor, email_from, email_to).await;

    let provider = ClaudeProvider::with_model(config.0, config.1, config.3);
    let prompt = build_extraction_prompt(
        email_from, email_to, email_subject, email_body, &participants,
    );

    let response = run_headless_tool_loop(
        executor, &provider, config.2, prompt, tools, MAX_TOOL_LOOPS, EXTRACTION_TEMPERATURE,
    ).await.map_err(|e| format!("generate: {}", e))?;

    let items: Vec<ExtractionItem> = parse_extraction_response(&response)
        .map_err(|e| format!("parse: {}", e))?;

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

    let count = items.len();
    if items.is_empty() {
        return Ok(0);
    }

    let email_iri_owned = email_iri.to_string();
    executor
        .write(move |conn| {
            let ts = chrono::Utc::now().timestamp_millis();
            for (i, item) in items.iter().enumerate() {
                if let Some(existing) = item.existing_iri.as_deref().filter(|s| !s.is_empty()) {
                    if let Err(e) = link_existing(conn, existing, &email_iri_owned) {
                        crate::imap::log_error(&format!(
                            "extraction: link_existing failed for {}: {}", existing, e,
                        ));
                    }
                } else if item.confidence >= EXTRACTION_CONFIDENCE_THRESHOLD {
                    if let Err(e) = persist_entity(conn, item, &email_iri_owned) {
                        crate::imap::log_error(&format!(
                            "extraction: persist_entity failed for {} ({}): {}",
                            item.label, item.class_iri, e,
                        ));
                    }
                } else {
                    let unique_id = ts + i as i64;
                    if let Err(e) = persist_pending(conn, item, &email_iri_owned, unique_id) {
                        crate::imap::log_error(&format!(
                            "extraction: persist_pending failed for {}: {}", item.label, e,
                        ));
                    }
                }
            }
            Ok(String::new())
        })
        .await
        .ok();

    Ok(count)
}

fn link_existing(
    conn: &mut crate::eavto::Connection,
    existing_iri: &str,
    email_iri: &str,
) -> Result<(), String> {
    Individual::new(existing_iri)
        .add_property(conn, "foundation:derivedFromEmail", vec![Object::Iri(email_iri.to_string())], "imap")
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn persist_entity(
    conn: &mut crate::eavto::Connection,
    item: &ExtractionItem,
    email_iri: &str,
) -> Result<(), String> {
    let mut properties: Vec<serde_json::Value> = item.properties.iter()
        .map(|(prop, val)| serde_json::json!({
            "property_iri": prop,
            "values": [val],
        }))
        .collect();

    properties.push(serde_json::json!({
        "property_iri": "foundation:derivedFromEmail",
        "values": [email_iri],
    }));

    let has_status = item.properties.keys()
        .any(|k| k == "foundation:hasStatus");
    if !has_status {
        if let Some(default_status) = default_allowed_status(conn, &item.class_iri) {
            properties.push(serde_json::json!({
                "property_iri": "foundation:hasStatus",
                "values": [default_status],
            }));
        }
    }

    let args = serde_json::json!({
        "class_iri": item.class_iri,
        "label": item.label,
        "icon": "smart_toy",
        "properties": properties,
    });

    let result = crate::ai::functions::individual::assert_individual_one(conn, &args);
    if !result.success {
        return Err(result.error.unwrap_or_else(|| "unknown error".to_string()));
    }
    Ok(())
}

fn default_allowed_status(
    conn: &crate::eavto::Connection,
    class_iri: &str,
) -> Option<String> {
    let result = crate::eavto::query::get_by_entity_predicate(
        conn, class_iri, "foundation:allowedStatus",
    ).ok()?;
    result.triples.iter()
        .find_map(|t| t.object.as_iri().map(|s| s.to_string()))
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

struct ResolvedParticipant {
    email: String,
    iri: String,
    label: String,
    class_iri: String,
}

async fn resolve_participants(
    executor: &DbExecutor,
    from: &str,
    to: &[String],
) -> Vec<ResolvedParticipant> {
    let mut addresses: Vec<String> = Vec::with_capacity(1 + to.len());
    addresses.push(from.trim().to_lowercase());
    for t in to {
        let a = t.trim().to_lowercase();
        if !a.is_empty() && !addresses.contains(&a) {
            addresses.push(a);
        }
    }
    addresses.retain(|a| !a.is_empty());

    if addresses.is_empty() {
        return Vec::new();
    }

    executor.read(move |conn| {
        Ok::<_, String>(
            addresses.into_iter()
                .filter_map(|addr| find_individual_by_email(conn, &addr))
                .collect::<Vec<_>>(),
        )
    }).await.unwrap_or_default()
}

fn find_individual_by_email(
    conn: &crate::eavto::Connection,
    email: &str,
) -> Option<ResolvedParticipant> {
    let mut stmt = conn.prepare(
        "SELECT subject FROM triples t \
         WHERE t.predicate = 'foundation:email' AND t.object_value = ?1 AND t.retracted = 0 \
         AND t.tx = (SELECT MAX(tx) FROM triples \
                     WHERE subject = t.subject AND predicate = t.predicate) \
         LIMIT 1",
    ).ok()?;
    let iri: String = stmt.query_row(rusqlite::params![email], |row| row.get(0)).ok()?;
    let label = crate::owl::get_literal_property(conn, &iri, "rdfs:label")
        .ok().flatten().unwrap_or_default();
    let class_iri = crate::owl::get_iri_property(conn, &iri, "rdf:type")
        .ok().flatten().unwrap_or_else(|| "owl:Thing".to_string());
    Some(ResolvedParticipant { email: email.to_string(), iri, label, class_iri })
}

fn build_extraction_prompt(
    from: &str,
    to: &[String],
    subject: &str,
    body: &str,
    participants: &[ResolvedParticipant],
) -> String {
    let body_truncated = if body.len() > MAX_EMAIL_BODY_CHARS {
        &body[..MAX_EMAIL_BODY_CHARS]
    } else {
        body
    };
    let to_line = if to.is_empty() { "(unknown)".to_string() } else { to.join(", ") };
    let participants_block = if participants.is_empty() {
        "(none of the email addresses above are registered to a known individual)".to_string()
    } else {
        participants.iter()
            .map(|p| format!(
                "- {} → {} (class: {}, label: \"{}\")",
                p.email, p.iri, p.class_iri, p.label,
            ))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "## Email\n\nFrom: {from}\nTo: {to}\nSubject: {subject}\n\nBody:\n{body}\n\n\
        ## Participants already registered in the graph\n\
        Pre-resolved for you — these IRIs exist, DO NOT recreate them:\n\
        {participants}\n\n\
        ## Task\n\n\
        Extract concrete domain entities mentioned in the email that belong in the knowledge graph.\n\n\
        ## STEP 0 — Exclude email participants (HARD RULE)\n\
        The sender (`From`) and recipient(s) (`To`) above are the email's participants. Their relationship to this email is ALREADY captured by `foundation:emailFrom` and `foundation:emailTo` on the Email entity — you do not need to re-assert it.\n\
        DO NOT output a Person (or any other class) for the sender or for any recipient, even when the email body addresses them by name (\"Olá, Daniel\", \"Dear John\"). A name appearing in the salutation is not new information about a third party.\n\
        If an email address in From/To appears in the pre-resolved list above, the corresponding individual is ALREADY in the graph. Skip it entirely.\n\
        Extract a Person ONLY when the email mentions a THIRD party — someone other than the sender and the recipient(s).\n\n\
        ## STEP 1 — Mandatory deduplication protocol\n\
        For every candidate entity that survives STEP 0, follow this protocol IN ORDER. Skipping any sub-step is a bug.\n\
        1a. Call `search` filtered by `class_iri` AND the entity's label.\n\
        1b. For Person, Company, Organization, ALSO search by key property: Person by `foundation:email` (exact, lowercased), Company/Organization by `foundation:name`.\n\
        1c. If search returns any candidates, call `describe_individual` on the top 1–3 hits. Compare name, email, context, backlinks to decide match.\n\
        1d. If a match exists → set `existing_iri` to the matched IRI and OMIT `properties`. Do not attempt to update the existing individual.\n\
        1e. Only after 1a + 1b + 1c confirm no match may you propose a new entity (omit `existing_iri`).\n\
        Never decide \"this is new\" without having executed search + describe_individual.\n\n\
        ## STEP 2 — Property enrichment (mandatory for new entities)\n\
        For every NEW entity that passed STEP 1 (no existing match found), you MUST enrich its properties before emitting:\n\
        2a. Call `describe_class` on the entity's class to retrieve ALL declared and inherited properties.\n\
        2b. Walk through each property. For each one, scan the WHOLE email (subject, body, headers, footer, signature, addresses, links) looking for an explicit unambiguous value.\n\
        2c. If you find a value, include the property in `properties` with that value.\n\
        2d. Required fields (`foundation:hasStatus` and others marked as required by the class) MUST be filled. If the email gives no obvious status, use the most appropriate `foundation:allowedStatus` of the class (e.g. `foundation:Pending` for new obligations, `foundation:Completed` for confirmed purchases, etc.).\n\
        2e. Do NOT skip a property just because the entity is \"new and minimal\". Look for: CNPJ, CPF, address, CEP, phone, email, price (foundation:price/amount/totalAmount), dates (purchaseDate, dueDate, issuedAt), identifiers (orderNumber, invoiceNumber, sku), brand, color, dimensions.\n\
        2f. The email body is structured data — your job is to translate it into the graph, not to summarize.\n\n\
        Common failure: emitting `{{\"class_iri\":\"foundation:Company\",\"label\":\"Traxart\",\"properties\":{{}}}}` when the email footer has CNPJ 00.728.551/0001-76 and address \"Rua Alexandre Dumas, 1489, São Paulo\" right there. That is a bug. Capture it.\n\n\
        ## STEP 3 — Output\n\
        Return ONLY a raw JSON array (no markdown, no explanation). Each item:\n\
        {{\"class_iri\":\"foundation:ClassName\",\"label\":\"entity name\",\
        \"properties\":{{\"foundation:propName\":\"value\"}},\"confidence\":0.0,\"existing_iri\":\"foundation:Optional_If_Matched\"}}\n\n\
        - confidence: 0.0-1.0 (certainty this entity is real and relevant)\n\
        - existing_iri: set ONLY when STEP 1 confirmed a match; omit or null otherwise\n\
        - Only concrete entities explicitly mentioned in the email\n\
        - NEVER use foundation:Email, foundation:EmailAddress, foundation:IMAPAccount or any messaging/infrastructure class\n\
        - Use only property IRIs confirmed via describe_class\n\
        - dateTime values must be ISO 8601 (e.g. \"2026-04-24T09:30:00\")\n\
        - For NEW entities, `properties` must include every field from STEP 2 that has a value in the email — partial extraction is a bug\n\
        - Return [] if nothing relevant remains after STEP 0\n\n\
        ## Anti-example (DO NOT DO THIS)\n\
        Email body: \"Olá, Daniel Borlino de Oliveira, sua cobrança vence em 26/04.\"\n\
        Email To: cristao.borlino@gmail.com\n\
        WRONG: `{{\"class_iri\":\"foundation:Person\",\"label\":\"Daniel Borlino de Oliveira\",...}}` — Daniel is the recipient (see To), extracting him violates STEP 0.\n\
        CORRECT: Do not emit a Person for Daniel at all. Emit only the third-party entities (e.g. a `foundation:FinancialObligation` for the cobrança and its issuing `foundation:Company` — after STEP 1 search confirms neither exists).\n\n\
        ## Class disambiguation (pt-BR)\n\
        - `foundation:Invoice` means **Nota Fiscal** (tax/accounting document issued by a seller). Use ONLY for nota fiscal / NF / NFS-e / NFe.\n\
        - **Boleto / fatura a pagar / conta / mensalidade / obrigação a pagar** is NOT an Invoice. Use `foundation:FinancialObligation` (Obrigação Financeira) for anything the user must pay on a due date (boleto bancário, fatura de cartão, conta de luz, mensalidade, etc).\n\n\
        ## Email pattern recognition (mandatory triggers)\n\
        Certain subjects/phrases ALWAYS imply the existence of a specific domain entity. If the email matches one of these patterns, you MUST emit the corresponding entity (after STEP 1 dedup + STEP 2 enrichment):\n\
        \n\
        - **\"Você comprou X\" / \"Sua compra\" / \"Confirmação do pedido\" / \"Order confirmation\" / \"Pedido #N\"** → `foundation:Purchase` (subclass of foundation:Contract). Required relations:\n\
          - `foundation:identifier` → número do pedido (ex: \"#2000012667334565\")\n\
          - `foundation:contractor` → o vendedor (Mercado Livre, Amazon, the store) as Company/Organization\n\
          - `foundation:contractStartDate` → data da compra\n\
          - `foundation:contractValue` + `foundation:contractCurrency` → valor total (ex: 199.90 + currency:BRL)\n\
          - `foundation:purchasedProduct` → IRI(s) do(s) Product(s) extraído(s). Se o email cita N produtos, emita 1 Purchase + N Products, ligados via `purchasedProduct`.\n\
          - `foundation:hasStatus` → `foundation:Completed` (compra confirmada) ou similar\n\
        - **\"NF-e / NFe / Nota Fiscal Eletrônica / Sua nota fiscal\"** → `foundation:Invoice`. Se o mesmo email/pedido tem Purchase associada, ligue via `foundation:supportedBy` na Purchase apontando para a Invoice.\n\
        - **\"Cobrança / Boleto / Fatura vence em / Sua conta de [luz/água/internet]\"** → `foundation:FinancialObligation` (NUNCA Invoice — ver disambiguation).\n\
        - **\"Recibo / Pagamento confirmado / Pagamento recebido\"** → `foundation:FinancialTransaction` (liquidando uma Obrigation existente, se houver).\n\
        \n\
        Falhar em emitir `foundation:Purchase` para um email \"Você comprou\" é bug — a Purchase é a entidade central desse tipo de email, não pode ser substituída por extrair só Company ou só Product.",
        from = from,
        to = to_line,
        participants = participants_block,
        subject = subject,
        body = body_truncated,
    )
}

fn parse_extraction_response(content: &str) -> Result<Vec<ExtractionItem>, String> {
    let start = content.find('[').ok_or("no JSON array in response")?;
    let end = content.rfind(']').ok_or("no JSON array end in response")?;
    serde_json::from_str(&content[start..=end]).map_err(|e| e.to_string())
}
