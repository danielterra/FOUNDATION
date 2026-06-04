use rusqlite;
use sha2::{Sha256, Digest};
use crate::eavto::Connection;
use crate::owl::{Individual, Object};

const STATUS_UNREAD: &str = "foundation:Status_1776975614793";

pub struct ParsedEmail {
    pub message_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub date: Option<String>,
    pub body: String,
    pub body_html: Option<String>,
    pub attachments: Vec<ParsedAttachment>,
}

pub struct ParsedAttachment {
    pub file_name: String,
    pub mime_type: String,
    pub content: Vec<u8>,
}

pub fn store_email(
    conn: &mut Connection,
    account_iri: &str,
    email: &ParsedEmail,
) -> Result<Option<String>, String> {
    if email_exists(conn, &email.message_id) {
        return Ok(None);
    }

    // Wrap the whole insert in one TX so a failure mid-way (e.g. an ontology
    // property mismatch) rolls back all triples — otherwise a partial email
    // with only emailMessageId set would shadow itself on the next sync via
    // email_exists() and never be retried.
    crate::eavto::with_transaction(conn, |conn| {
        store_email_inner(conn, account_iri, email)
    })
    .map(Some)
}

fn store_email_inner(
    conn: &mut Connection,
    account_iri: &str,
    email: &ParsedEmail,
) -> Result<String, String> {
    let from_iri = ensure_email_address(conn, &email.from)?;

    let to_iris: Vec<Object> = email
        .to
        .iter()
        .filter_map(|addr| ensure_email_address(conn, addr).ok())
        .map(|iri| Object::Iri(iri))
        .collect();

    let ts = chrono::Utc::now().timestamp_millis();
    let email_iri = format!("foundation:Email_{}", ts);
    let label = if email.subject.is_empty() {
        "(sem assunto)".to_string()
    } else {
        email.subject.chars().take(80).collect()
    };

    let ind = Individual::new(&email_iri);
    ind.assert(conn, "foundation:Email", &label, "mark_email_unread", "imap")
        .map_err(|e| format!("assert email: {}", e))?;

    ind.add_property(conn, "foundation:hasStatus", vec![Object::Iri(STATUS_UNREAD.to_string())], "imap")
        .map_err(|e| format!("status: {}", e))?;
    ind.add_property(conn, "foundation:emailMessageId", vec![str_lit(&email.message_id)], "imap")
        .map_err(|e| format!("messageId: {}", e))?;
    ind.add_property(conn, "foundation:emailSubject", vec![str_lit(&email.subject)], "imap")
        .map_err(|e| format!("subject: {}", e))?;
    ind.add_property(conn, "foundation:emailBody", vec![str_lit(&email.body)], "imap")
        .map_err(|e| format!("body: {}", e))?;

    // Preserve the original text/html part as rdf:HTML so the inspector can render
    // the email as designed; emailBody (text/plain) stays as the lossless fallback.
    if let Some(html) = email.body_html.as_deref().filter(|h| !h.is_empty()) {
        ind.add_property(conn, "foundation:emailBodyHtml", vec![html_lit(html)], "imap")
            .map_err(|e| format!("body_html: {}", e))?;
    }

    ind.add_property(conn, "foundation:emailFrom", vec![Object::Iri(from_iri)], "imap")
        .map_err(|e| format!("from: {}", e))?;

    if !to_iris.is_empty() {
        ind.add_property(conn, "foundation:emailTo", to_iris, "imap")
            .map_err(|e| format!("to: {}", e))?;
    }

    let normalized_date = email.date.as_deref().and_then(normalize_to_rfc3339);
    if let Some(date) = &normalized_date {
        ind.add_property(conn, "foundation:existsFrom", vec![datetime_lit(date)], "imap")
            .map_err(|e| format!("date: {}", e))?;
    }

    ind.add_property(
        conn,
        "foundation:importedFromAccount",
        vec![Object::Iri(account_iri.to_string())],
        "imap",
    )
    .map_err(|e| format!("account: {}", e))?;

    // Accumulate attachment IRIs and link them in a single add_property call.
    // assert_triples replaces the full TX set, so calling add_property per
    // attachment would leave only the last one as the current value.
    let mut attachment_iris: Vec<Object> = Vec::new();
    for (i, att) in email.attachments.iter().enumerate() {
        match persist_attachment(conn, att, i, normalized_date.as_deref()) {
            Ok(att_iri) => attachment_iris.push(Object::Iri(att_iri)),
            Err(e) => crate::imap::log_error(&format!(
                "IMAP: failed to persist attachment '{}': {}", att.file_name, e,
            )),
        }
    }
    if !attachment_iris.is_empty() {
        ind.add_property(conn, "foundation:emailHasAttachment", attachment_iris, "imap")
            .map_err(|e| format!("attach link: {}", e))?;
    }

    Ok(email_iri)
}

fn persist_attachment(
    conn: &mut Connection,
    att: &ParsedAttachment,
    index: usize,
    email_date: Option<&str>,
) -> Result<String, String> {
    let attachments_dir = crate::paths::attachments_dir();
    std::fs::create_dir_all(&attachments_dir)
        .map_err(|e| format!("create dir: {}", e))?;

    let ts = chrono::Utc::now().timestamp_millis() + index as i64;
    let safe_name = sanitize_filename(&att.file_name);
    let target_path = attachments_dir.join(format!("{}_{}", ts, safe_name));
    std::fs::write(&target_path, &att.content)
        .map_err(|e| format!("write file: {}", e))?;
    // Stored as a portable path (e.g. "attachments/123_file.pdf") so the DB
    // can move between machines without breaking attachment references.
    let stored_path = crate::paths::to_portable_path(&target_path);

    let hash = format!("sha256:{:x}", Sha256::digest(&att.content));
    let size = att.content.len() as i64;
    let is_csv = is_csv_attachment(&att.mime_type, &att.file_name);
    let (class_iri, icon) = if is_csv {
        ("foundation:CSVFile", "csv")
    } else {
        ("foundation:File", "attach_file")
    };
    let att_iri = format!("foundation:EmailAttachment_{}", ts);
    let att_ind = Individual::new(&att_iri);

    att_ind.assert(conn, class_iri, &att.file_name, icon, "imap")
        .map_err(|e| format!("assert: {}", e))?;
    att_ind.add_property(conn, "foundation:fileName", vec![str_lit(&att.file_name)], "imap")
        .map_err(|e| format!("fileName: {}", e))?;
    att_ind.add_property(conn, "foundation:filePath", vec![str_lit(&stored_path)], "imap")
        .map_err(|e| format!("filePath: {}", e))?;
    att_ind.add_property(conn, "foundation:fileSize", vec![Object::Integer(size)], "imap")
        .map_err(|e| format!("fileSize: {}", e))?;
    att_ind.add_property(conn, "foundation:fileHash", vec![str_lit(&hash)], "imap")
        .map_err(|e| format!("fileHash: {}", e))?;

    let upload_date = email_date
        .map(|s| s.to_string())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    att_ind.add_property(conn, "foundation:uploadDate", vec![Object::DateTime(upload_date)], "imap")
        .map_err(|e| format!("uploadDate: {}", e))?;

    if let Some(ft_iri) = mime_to_file_type_iri(&att.mime_type) {
        att_ind.add_property(conn, "foundation:hasFileType",
            vec![Object::Iri(ft_iri.to_string())], "imap")
            .map_err(|e| format!("hasFileType: {}", e))?;
    }

    Ok(att_iri)
}

fn sanitize_filename(name: &str) -> String {
    let cleaned = name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    if cleaned.trim().is_empty() { "attachment".to_string() } else { cleaned }
}

fn is_csv_attachment(mime_type: &str, file_name: &str) -> bool {
    matches!(mime_type,
        "text/csv" | "application/csv" | "application/vnd.ms-excel")
        || file_name.to_lowercase().ends_with(".csv")
}

fn mime_to_file_type_iri(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        "image/jpeg" => Some("foundation:FileType_JPEG"),
        "image/png"  => Some("foundation:FileType_PNG"),
        "image/gif"  => Some("foundation:FileType_GIF"),
        "image/webp" => Some("foundation:FileType_WEBP"),
        "image/bmp"  => Some("foundation:FileType_BMP"),
        "image/tiff" => Some("foundation:FileType_TIFF"),
        "image/svg+xml" => Some("foundation:FileType_SVG"),
        "application/pdf" => Some("foundation:FileType_PDF"),
        "text/plain" => Some("foundation:FileType_TXT"),
        _ => None,
    }
}

pub fn email_exists(conn: &Connection, message_id: &str) -> bool {
    find_by_literal(conn, "foundation:emailMessageId", message_id)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

fn ensure_email_address(conn: &mut Connection, address: &str) -> Result<String, String> {
    let address = address.trim().to_lowercase();
    if address.is_empty() {
        return Err("empty email address".to_string());
    }

    let existing = find_by_literal(conn, "foundation:emailAddress", &address)
        .unwrap_or_default();
    if let Some(iri) = existing.into_iter().next() {
        return Ok(iri);
    }

    let ts = chrono::Utc::now().timestamp_millis();
    let iri = format!("foundation:EmailAddress_{}", ts);
    let ind = Individual::new(&iri);
    ind.assert(conn, "foundation:EmailAddress", &address, "mail", "imap")
        .map_err(|e| format!("email address: {}", e))?;
    ind.add_property(conn, "foundation:emailAddress", vec![str_lit(&address)], "imap")
        .map_err(|e| format!("emailAddress prop: {}", e))?;
    Ok(iri)
}

fn find_by_literal(conn: &Connection, predicate: &str, value: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT subject FROM triples t \
             WHERE t.predicate = ?1 AND t.object_value = ?2 AND t.retracted = 0 \
             AND t.tx = (SELECT MAX(tx) FROM triples WHERE subject = t.subject AND predicate = t.predicate) \
             LIMIT 1",
        )
        .map_err(|e| e.to_string())?;
    let rows: Result<Vec<String>, _> = stmt
        .query_map(rusqlite::params![predicate, value], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    rows
}

fn str_lit(v: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None }
}

fn datetime_lit(v: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:dateTime".to_string()), language: None }
}

fn html_lit(v: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("rdf:HTML".to_string()), language: None }
}

/// mailparse devolve a string Date: bruta do header (geralmente RFC2822).
/// Os campos xsd:dateTime do graph esperam RFC3339, então convertemos antes
/// de gravar — caso contrário queries por data falham silenciosamente.
fn normalize_to_rfc3339(raw: &str) -> Option<String> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(raw) {
        return Some(dt.to_rfc3339());
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.to_rfc3339());
    }
    None
}
