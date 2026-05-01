use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use mailparse::MailHeaderMap;

use crate::imap::extractor::{ParsedAttachment, ParsedEmail, store_email};
use crate::owl::{DbExecutor, Individual, Object};

const DEFAULT_IMAP_PORT: u16 = 993;
const DEFAULT_SYNC_INTERVAL_MINUTES: i64 = 15;
const MAX_EMAILS_PER_FETCH: usize = 50;
const MAX_FAILURES_BEFORE_ALERT: usize = 3;
const TCP_CONNECT_TIMEOUT_SECS: u64 = 20;
const TCP_SESSION_READ_TIMEOUT_SECS: u64 = 120;
const IDLE_TCP_READ_TIMEOUT_SECS: u64 = 29 * 60 + 30;

#[derive(Clone)]
struct AccountConfig {
    iri: String,
    host: String,
    port: u16,
    use_tls: bool,
    username: String,
    password: String,
    sync_interval_minutes: i64,
    monitored_folders: Vec<String>,
}

pub async fn start_imap_sync(app: AppHandle) {
    let executor = app.state::<DbExecutor>().inner().clone();

    let accounts = match executor.read(|conn| load_account_configs(conn)).await {
        Ok(a) => a,
        Err(_) => return,
    };

    for account in accounts {
        spawn_account_sync(executor.clone(), app.clone(), account);
    }
}

pub async fn start_account_sync(app: AppHandle, account_iri: String) {
    let executor = app.state::<DbExecutor>().inner().clone();

    let account = match executor
        .read(move |conn| {
            load_account_configs(conn).map(|accounts| {
                accounts.into_iter().find(|a| a.iri == account_iri)
            })
        })
        .await
    {
        Ok(Some(a)) => a,
        Ok(None) => {
            crate::imap::log_error("IMAP start_account_sync: account not found in configs");
            return;
        }
        Err(e) => {
            crate::imap::log_error(&format!("IMAP start_account_sync: {}", e));
            return;
        }
    };

    spawn_account_sync(executor, app, account);
}

fn spawn_account_sync(executor: DbExecutor, app: AppHandle, account: AccountConfig) {
    tauri::async_runtime::spawn(async move {
        run_account_loop(executor, app, account).await;
    });
}

async fn run_account_loop(executor: DbExecutor, app: AppHandle, account: AccountConfig) {
    const BACKOFFS: [u64; 3] = [5, 30, 120];
    let mut failure_count: usize = 0;

    loop {
        let _ = app.emit("imap-sync-started", serde_json::json!({
            "accountIri": account.iri,
            "accountLabel": account.username,
        }));

        let account_clone = account.clone();
        let result =
            tokio::task::spawn_blocking(move || sync_account_blocking(&account_clone)).await;

        match result {
            Ok(Ok(emails)) => {
                if failure_count > 0 {
                    failure_count = 0;
                    let iri = account.iri.clone();
                    executor
                        .write(move |conn| {
                            Individual::clear_property(conn, &iri, "foundation:lastConnectionError", "imap").ok();
                            Individual::clear_property(conn, &iri, "foundation:isConnected", "imap").ok();
                            Individual::new(&iri)
                                .add_property(conn, "foundation:isConnected", vec![bool_lit(true)], "imap")
                                .map_err(|e| e.to_string())?;
                            Ok(String::new())
                        })
                        .await
                        .ok();
                }

                let mut stored_count: i64 = 0;
                for email in emails {
                    let account_iri = account.iri.clone();
                    let subject_log = email.subject.chars().take(40).collect::<String>();
                    match executor
                        .write(move |conn| {
                            store_email(conn, &account_iri, &email)
                                .map(|opt| opt.unwrap_or_default())
                        })
                        .await
                    {
                        Ok(s) if !s.is_empty() => { stored_count += 1; }
                        Ok(_) => { crate::imap::log_error(&format!("IMAP: duplicate skipped: {}", subject_log)); }
                        Err(e) => { crate::imap::log_error(&format!("IMAP store_email error: {} — {}", subject_log, e)); }
                    };
                }

                let _ = app.emit("imap-sync-finished", serde_json::json!({
                    "accountIri": account.iri,
                    "accountLabel": account.username,
                    "storedCount": stored_count,
                }));

                let account_iri = account.iri.clone();
                executor
                    .write(move |conn| write_sync_log(conn, &account_iri, stored_count, None))
                    .await
                    .ok();

                // After delivering emails, wait for server push notification before fetching again.
                let account_for_idle = account.clone();
                tokio::task::spawn_blocking(move || idle_wait_blocking(&account_for_idle)).await.ok();
            }
            err => {
                let msg = match err {
                    Ok(Err(e)) => e,
                    Err(e) => e.to_string(),
                    Ok(Ok(_)) => unreachable!(),
                };

                failure_count += 1;

                let _ = app.emit("imap-sync-finished", serde_json::json!({
                    "accountIri": account.iri,
                    "accountLabel": account.username,
                    "error": msg,
                }));

                if failure_count >= MAX_FAILURES_BEFORE_ALERT {
                    failure_count = 0;
                    let account_iri = account.iri.clone();
                    let err_msg = msg.clone();
                    executor
                        .write(move |conn| {
                            write_sync_log(conn, &account_iri, 0, Some(&err_msg)).ok();
                            record_connection_error(conn, &account_iri, &err_msg)
                        })
                        .await
                        .ok();
                    crate::imap::log_error(&format!(
                        "IMAP sync failed for {} after 3 retries: {}",
                        account.iri, msg
                    ));
                    let wait = (account.sync_interval_minutes.max(1) * 60) as u64;
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                } else {
                    let wait = BACKOFFS[(failure_count.saturating_sub(1)).min(2)];
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
            }
        }
    }
}

fn write_sync_log(
    conn: &mut crate::eavto::Connection,
    account_iri: &str,
    emails_imported: i64,
    error: Option<&str>,
) -> Result<String, String> {
    let ts = chrono::Utc::now();
    let ts_ms = ts.timestamp_millis();
    let ts_iso = ts.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let log_iri = format!("foundation:IMAPSyncLog_{}", ts_ms);

    Individual::new(&log_iri)
        .assert(conn, "foundation:IMAPSyncLog", &ts_iso, "history", "imap")
        .map_err(|e| format!("synclog: {}", e))?;
    Individual::new(&log_iri)
        .add_property(conn, "foundation:syncStartedAt", vec![datetime_lit(&ts_iso)], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&log_iri)
        .add_property(conn, "foundation:syncEmailsImported", vec![Object::Integer(emails_imported)], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&log_iri)
        .add_property(conn, "foundation:syncForAccount", vec![Object::Iri(account_iri.to_string())], "imap")
        .map_err(|e| e.to_string())?;
    if let Some(err) = error {
        Individual::new(&log_iri)
            .add_property(conn, "foundation:syncErrorMessage", vec![str_lit(err)], "imap")
            .map_err(|e| e.to_string())?;
    }

    Ok(String::new())
}

fn datetime_lit(v: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:dateTime".to_string()), language: None }
}

fn record_connection_error(
    conn: &mut crate::eavto::Connection,
    account_iri: &str,
    error: &str,
) -> Result<String, String> {
    Individual::clear_property(conn, account_iri, "foundation:lastConnectionError", "imap").ok();
    Individual::new(account_iri)
        .add_property(conn, "foundation:lastConnectionError", vec![str_lit(error)], "imap")
        .map_err(|e| e.to_string())?;
    Individual::clear_property(conn, account_iri, "foundation:isConnected", "imap").ok();
    Individual::new(account_iri)
        .add_property(conn, "foundation:isConnected", vec![bool_lit(false)], "imap")
        .map_err(|e| e.to_string())?;

    let ts = chrono::Utc::now().timestamp_millis();
    let notif_iri = format!("foundation:AINotification_{}", ts);
    let title = "Falha na conexão IMAP";
    Individual::new(&notif_iri)
        .assert(conn, "foundation:AINotification", title, "warning", "imap")
        .map_err(|e| format!("notif: {}", e))?;
    Individual::new(&notif_iri)
        .add_property(conn, "foundation:hasStatus", vec![Object::Iri("foundation:Pending".to_string())], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&notif_iri)
        .add_property(conn, "foundation:notificationTitle", vec![str_lit(title)], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&notif_iri)
        .add_property(conn, "foundation:notificationBody", vec![str_lit(error)], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&notif_iri)
        .add_property(conn, "foundation:notificationType", vec![str_lit("imap_connection_error")], "imap")
        .map_err(|e| e.to_string())?;
    Individual::new(&notif_iri)
        .add_property(conn, "foundation:notificationSource", vec![Object::Iri(account_iri.to_string())], "imap")
        .map_err(|e| e.to_string())?;

    Ok(String::new())
}

fn str_lit(v: &str) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:string".to_string()), language: None }
}

fn bool_lit(v: bool) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:boolean".to_string()), language: None }
}

fn sync_account_blocking(account: &AccountConfig) -> Result<Vec<ParsedEmail>, String> {
    if account.use_tls {
        fetch_tls(account)
    } else {
        fetch_plain(account)
    }
}

fn idle_wait_blocking(account: &AccountConfig) {
    let result = if account.use_tls {
        idle_tls(account)
    } else {
        idle_plain(account)
    };
    if let Err(e) = result {
        crate::imap::log_error(&format!("IMAP IDLE error for {}: {}", account.iri, e));
    }
}

fn tcp_connect(host: &str, port: u16) -> Result<TcpStream, String> {
    let addr = std::net::ToSocketAddrs::to_socket_addrs(&(host, port))
        .map_err(|e| format!("DNS: {}", e))?
        .next()
        .ok_or_else(|| format!("DNS: no address for {}", host))?;
    TcpStream::connect_timeout(&addr, Duration::from_secs(TCP_CONNECT_TIMEOUT_SECS))
        .map_err(|e| format!("TCP: {}", e))
}

fn fetch_tls(account: &AccountConfig) -> Result<Vec<ParsedEmail>, String> {
    let tcp = tcp_connect(&account.host, account.port)?;
    tcp.set_read_timeout(Some(Duration::from_secs(TCP_SESSION_READ_TIMEOUT_SECS))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(TCP_SESSION_READ_TIMEOUT_SECS))).ok();
    let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
    let tls_stream = tls.connect(&account.host, tcp).map_err(|e| format!("TLS: {}", e))?;
    let mut session = imap::Client::new(tls_stream)
        .login(&account.username, &account.password)
        .map_err(|(e, _)| format!("login: {}", e))?;
    let emails = sync_folders(&mut session, &account.monitored_folders)?;
    session.logout().ok();
    Ok(emails)
}

fn fetch_plain(account: &AccountConfig) -> Result<Vec<ParsedEmail>, String> {
    let tcp = tcp_connect(&account.host, account.port)?;
    tcp.set_read_timeout(Some(Duration::from_secs(TCP_SESSION_READ_TIMEOUT_SECS))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(TCP_SESSION_READ_TIMEOUT_SECS))).ok();
    let mut session = imap::Client::new(tcp)
        .login(&account.username, &account.password)
        .map_err(|(e, _)| format!("login: {}", e))?;
    let emails = sync_folders(&mut session, &account.monitored_folders)?;
    session.logout().ok();
    Ok(emails)
}

fn idle_tls(account: &AccountConfig) -> Result<(), String> {
    let tcp = tcp_connect(&account.host, account.port)?;
    tcp.set_read_timeout(Some(Duration::from_secs(IDLE_TCP_READ_TIMEOUT_SECS))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(TCP_SESSION_READ_TIMEOUT_SECS))).ok();
    let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
    let tls_stream = tls.connect(&account.host, tcp).map_err(|e| format!("TLS: {}", e))?;
    let mut session = imap::Client::new(tls_stream)
        .login(&account.username, &account.password)
        .map_err(|(e, _)| format!("login: {}", e))?;
    session.select("INBOX").map_err(|e| e.to_string())?;
    if let Ok(idle) = session.idle() {
        let _ = idle.wait_keepalive();
    }
    session.logout().ok();
    Ok(())
}

fn idle_plain(account: &AccountConfig) -> Result<(), String> {
    let tcp = tcp_connect(&account.host, account.port)?;
    tcp.set_read_timeout(Some(Duration::from_secs(IDLE_TCP_READ_TIMEOUT_SECS))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(TCP_SESSION_READ_TIMEOUT_SECS))).ok();
    let mut session = imap::Client::new(tcp)
        .login(&account.username, &account.password)
        .map_err(|(e, _)| format!("login: {}", e))?;
    session.select("INBOX").map_err(|e| e.to_string())?;
    if let Ok(idle) = session.idle() {
        let _ = idle.wait_keepalive();
    }
    session.logout().ok();
    Ok(())
}

fn sync_folders<T: Read + Write>(
    session: &mut imap::Session<T>,
    folders: &[String],
) -> Result<Vec<ParsedEmail>, String> {
    let folders_to_sync: Vec<&str> = if folders.is_empty() {
        vec!["INBOX"]
    } else {
        folders.iter().map(|s| s.as_str()).collect()
    };

    let mut all_emails = vec![];
    for folder in folders_to_sync {
        session.select(folder).map_err(|e| e.to_string())?;
        all_emails.extend(fetch_unseen(session)?);
    }
    Ok(all_emails)
}

fn fetch_unseen<T: Read + Write>(session: &mut imap::Session<T>) -> Result<Vec<ParsedEmail>, String> {
    let uids = session.uid_search("UNSEEN").map_err(|e| e.to_string())?;

    if uids.is_empty() {
        return Ok(vec![]);
    }
    crate::imap::log_error(&format!("IMAP: found {} UNSEEN uid(s): {:?}", uids.len(), uids.iter().take(5).collect::<Vec<_>>()));

    let uid_set: String = uids
        .iter()
        .take(MAX_EMAILS_PER_FETCH)
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let messages = session
        .uid_fetch(&uid_set, "(UID BODY[])")
        .map_err(|e| e.to_string())?;

    let mut result = vec![];
    for msg in messages.iter() {
        if let Some(body) = msg.body() {
            match parse_raw_email(body) {
                Ok(email) => result.push(email),
                Err(e) => crate::imap::log_error(&format!("IMAP parse error uid={:?}: {}", msg.uid, e)),
            }
            continue;
        }
        // Fallback: combine RFC822 header + text when BODY[] not available
        if let (Some(hdr), Some(txt)) = (msg.header(), msg.text()) {
            crate::imap::log_error(&format!("IMAP: uid={:?} using header+text fallback", msg.uid));
            let mut combined = Vec::with_capacity(hdr.len() + 2 + txt.len());
            combined.extend_from_slice(hdr);
            combined.extend_from_slice(b"\r\n");
            combined.extend_from_slice(txt);
            match parse_raw_email(&combined) {
                Ok(email) => result.push(email),
                Err(e) => crate::imap::log_error(&format!("IMAP parse error (fallback) uid={:?}: {}", msg.uid, e)),
            }
            continue;
        }
        crate::imap::log_error(&format!(
            "IMAP: uid={:?} no body, header={}, text={}",
            msg.uid, msg.header().is_some(), msg.text().is_some()
        ));
    }

    Ok(result)
}

fn extract_addr(s: &str) -> String {
    let s = s.trim();
    if let (Some(a), Some(b)) = (s.find('<'), s.rfind('>')) {
        if a < b { return s[a+1..b].trim().to_lowercase(); }
    }
    s.to_lowercase()
}

fn extract_first_addr(header: &str) -> String {
    extract_addr(header)
}

fn parse_addr_list(header: &str) -> Vec<String> {
    if header.contains('<') {
        let mut addrs = vec![];
        let mut remaining = header;
        while let Some(a) = remaining.find('<') {
            if let Some(b) = remaining[a..].find('>') {
                let addr = remaining[a+1..a+b].trim().to_lowercase();
                if !addr.is_empty() { addrs.push(addr); }
                remaining = &remaining[a+b+1..];
            } else { break; }
        }
        addrs
    } else {
        header.split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty() && s.contains('@'))
            .collect()
    }
}

fn parse_raw_email(raw: &[u8]) -> Result<ParsedEmail, String> {
    let parsed = mailparse::parse_mail(raw).map_err(|e| e.to_string())?;

    let message_id = parsed
        .headers
        .get_first_value("Message-ID")
        .map(|id| {
            id.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string()
        })
        .unwrap_or_else(|| format!("gen-{}", chrono::Utc::now().timestamp_millis()));

    let from = extract_first_addr(&parsed.headers.get_first_value("From").unwrap_or_default());

    let to: Vec<String> = parse_addr_list(&parsed.headers.get_first_value("To").unwrap_or_default());

    let subject = parsed.headers.get_first_value("Subject").unwrap_or_default();

    let date = parsed
        .headers
        .get_first_value("Date")
        .and_then(|d| mailparse::dateparse(&d).ok())
        .and_then(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        });

    let body = extract_body_text(&parsed);
    let attachments = collect_attachments(&parsed);

    Ok(ParsedEmail { message_id, from, to, subject, date, body, attachments })
}

fn extract_body_text(mail: &mailparse::ParsedMail) -> String {
    if mail.subparts.is_empty() {
        if mail.ctype.mimetype.starts_with("text/") {
            return mail.get_body().unwrap_or_default();
        }
        return String::new();
    }
    for part in &mail.subparts {
        if part.ctype.mimetype == "text/plain" {
            return part.get_body().unwrap_or_default();
        }
    }
    mail.subparts
        .first()
        .and_then(|p| p.get_body().ok())
        .unwrap_or_default()
}

fn collect_attachments(mail: &mailparse::ParsedMail) -> Vec<ParsedAttachment> {
    let mut out = Vec::new();
    collect_attachments_recursive(mail, &mut out);
    out
}

fn collect_attachments_recursive(
    mail: &mailparse::ParsedMail,
    out: &mut Vec<ParsedAttachment>,
) {
    for part in &mail.subparts {
        if part_is_attachment(part) {
            if let Some(name) = extract_attachment_filename(part) {
                match part.get_body_raw() {
                    Ok(content) => {
                        out.push(ParsedAttachment {
                            file_name: name,
                            mime_type: part.ctype.mimetype.clone(),
                            content,
                        });
                    }
                    Err(e) => {
                        crate::imap::log_error(&format!(
                            "IMAP: attachment body decode failed for '{}': {}", name, e,
                        ));
                    }
                }
            }
        }
        if !part.subparts.is_empty() {
            collect_attachments_recursive(part, out);
        }
    }
}

fn part_is_attachment(part: &mailparse::ParsedMail) -> bool {
    match part.headers.get_first_value("Content-Disposition") {
        Some(disp) => disp.to_lowercase().trim_start().starts_with("attachment"),
        None => false,
    }
}

fn extract_attachment_filename(part: &mailparse::ParsedMail) -> Option<String> {
    let disp = part.headers.get_first_value("Content-Disposition")?;
    let from_disp = disp.split(';')
        .find(|s| s.trim().to_lowercase().starts_with("filename="))
        .map(|s| {
            s.trim()
                .splitn(2, '=')
                .nth(1)
                .unwrap_or("")
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
        .filter(|s| !s.is_empty());

    if from_disp.is_some() {
        return from_disp;
    }

    part.headers.get_first_value("Content-Type")
        .and_then(|ct| {
            ct.split(';')
                .find(|s| s.trim().to_lowercase().starts_with("name="))
                .map(|s| {
                    s.trim()
                        .splitn(2, '=')
                        .nth(1)
                        .unwrap_or("")
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string()
                })
        })
        .filter(|s| !s.is_empty())
}

fn load_account_configs(
    conn: &crate::eavto::Connection,
) -> Result<Vec<AccountConfig>, String> {
    let iris =
        crate::owl::find_entities_with_property(conn, "rdf:type", "foundation:IMAPAccount")
            .map_err(|e| e.to_string())?;

    let mut configs = vec![];
    for iri in iris {
        let host = match crate::owl::get_literal_property(conn, &iri, "foundation:imapHost") {
            Ok(Some(h)) => h,
            _ => continue,
        };
        let port = crate::owl::get_literal_property(conn, &iri, "foundation:imapPort")
            .unwrap_or_default()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(DEFAULT_IMAP_PORT);
        let use_tls = crate::owl::get_literal_property(conn, &iri, "foundation:imapUseTLS")
            .unwrap_or_default()
            .map(|v| v == "true")
            .unwrap_or(true);
        let username = crate::owl::get_literal_property(conn, &iri, "foundation:imapUsername")
            .unwrap_or_default()
            .unwrap_or_default();
        let sync_interval_minutes =
            crate::owl::get_literal_property(conn, &iri, "foundation:syncInterval")
                .unwrap_or_default()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(DEFAULT_SYNC_INTERVAL_MINUTES);

        let cred_iri =
            match crate::owl::get_iri_property(conn, &iri, "foundation:hasCredential") {
                Ok(Some(c)) => c,
                _ => continue,
            };
        let password = match crate::owl::get_literal_property(
            conn,
            &cred_iri,
            "foundation:credentialValue",
        ) {
            Ok(Some(p)) => p,
            _ => continue,
        };

        let monitored_folders =
            crate::owl::get_all_literal_properties(conn, &iri, "foundation:imapMonitoredFolders")
                .unwrap_or_default();

        configs.push(AccountConfig {
            iri,
            host,
            port,
            use_tls,
            username,
            password,
            sync_interval_minutes,
            monitored_folders,
        });
    }

    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTIPART_WITH_CSV: &[u8] = b"From: alice@example.com\r\n\
To: bob@example.com\r\n\
Subject: Extrato\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"BOUNDARY42\"\r\n\
\r\n\
--BOUNDARY42\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Segue o extrato em anexo.\r\n\
--BOUNDARY42\r\n\
Content-Type: text/csv; name=\"extrato.csv\"\r\n\
Content-Disposition: attachment; filename=\"extrato.csv\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
RGF0ZSxBbW91bnQKMjAyNi0wNC0wOSwxMDAuMDAK\r\n\
--BOUNDARY42--\r\n";

    #[test]
    fn test_parse_raw_email_extracts_attachment_bytes() {
        // Regression for Bug_1777034520503: attachments were parsed by name only,
        // with no body content persisted — so downstream read_binary_file failed.
        let parsed = parse_raw_email(MULTIPART_WITH_CSV).expect("parse should succeed");
        assert_eq!(parsed.attachments.len(), 1, "must collect one attachment");
        let att = &parsed.attachments[0];
        assert_eq!(att.file_name, "extrato.csv");
        assert_eq!(att.mime_type, "text/csv");
        let decoded = String::from_utf8(att.content.clone()).unwrap();
        assert!(decoded.contains("Date,Amount"), "body bytes must be decoded; got: {}", decoded);
        assert!(decoded.contains("2026-04-09,100.00"), "body bytes must be decoded");
    }

    #[test]
    fn test_parse_raw_email_inline_part_is_not_attachment() {
        let parsed = parse_raw_email(MULTIPART_WITH_CSV).expect("parse should succeed");
        assert!(
            parsed.attachments.iter().all(|a| a.file_name == "extrato.csv"),
            "inline text/plain part must not be collected as attachment"
        );
    }
}
