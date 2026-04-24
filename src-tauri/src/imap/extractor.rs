use rusqlite;
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
    pub attachment_names: Vec<String>,
}

pub fn store_email(
    conn: &mut Connection,
    account_iri: &str,
    email: &ParsedEmail,
) -> Result<Option<String>, String> {
    if email_exists(conn, &email.message_id) {
        return Ok(None);
    }

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
    ind.add_property(conn, "foundation:emailFrom", vec![Object::Iri(from_iri)], "imap")
        .map_err(|e| format!("from: {}", e))?;

    if !to_iris.is_empty() {
        ind.add_property(conn, "foundation:emailTo", to_iris, "imap")
            .map_err(|e| format!("to: {}", e))?;
    }

    if let Some(date) = &email.date {
        ind.add_property(conn, "foundation:emailDate", vec![datetime_lit(date)], "imap")
            .map_err(|e| format!("date: {}", e))?;
    }

    ind.add_property(
        conn,
        "foundation:importedFromAccount",
        vec![Object::Iri(account_iri.to_string())],
        "imap",
    )
    .map_err(|e| format!("account: {}", e))?;

    for name in &email.attachment_names {
        let att_ts = chrono::Utc::now().timestamp_millis();
        let att_iri = format!("foundation:EmailAttachment_{}", att_ts);
        let att = Individual::new(&att_iri);
        att.assert(conn, "owl:Thing", name, "attach_file", "imap")
            .map_err(|e| format!("attachment: {}", e))?;
        ind.add_property(conn, "foundation:emailHasAttachment", vec![Object::Iri(att_iri)], "imap")
            .map_err(|e| format!("attach link: {}", e))?;
    }

    Ok(Some(email_iri))
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
