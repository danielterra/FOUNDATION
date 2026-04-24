use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::eavto::Connection;
use crate::owl::{DbExecutor, Individual, Object};

const CONNECTION_TIMEOUT_SECS: u64 = 15;
const DEFAULT_SYNC_HISTORY_LIMIT: i64 = 50;

fn str_lit(v: impl Into<String>) -> Object {
    Object::Literal { value: v.into(), datatype: Some("xsd:string".to_string()), language: None }
}

fn bool_lit(v: bool) -> Object {
    Object::Literal { value: v.to_string(), datatype: Some("xsd:boolean".to_string()), language: None }
}

fn set_prop(
    conn: &mut Connection,
    iri: &str,
    prop: &str,
    values: Vec<Object>,
    origin: &str,
) -> Result<(), String> {
    Individual::clear_property(conn, iri, prop, origin).ok();
    Individual::new(iri)
        .add_property(conn, prop, values, origin)
        .map_err(|e| format!("{}: {}", prop, e))
}

fn get_int(conn: &Connection, iri: &str, prop: &str, default: i64) -> i64 {
    crate::owl::get_literal_property(conn, iri, prop)
        .unwrap_or_default()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImapAccountInput {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub use_tls: bool,
    pub username: String,
    pub password: String,
    pub sync_interval_minutes: i64,
}

#[derive(Debug, Serialize)]
pub struct ImapAccountSummary {
    pub iri: String,
    pub label: String,
    pub host: String,
    pub port: i64,
    pub use_tls: bool,
    pub username: String,
    pub sync_interval_minutes: i64,
    pub last_error: Option<String>,
    pub is_connected: bool,
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn imap__save_account(
    _app: AppHandle,
    account_iri: Option<String>,
    input: ImapAccountInput,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.write(move |conn| {
        let ts = chrono::Utc::now().timestamp_millis();
        let iri = account_iri.unwrap_or_else(|| format!("foundation:IMAPAccount_{}", ts));
        let ind = Individual::new(&iri);

        ind.assert(conn, "foundation:IMAPAccount", &input.label, "alternate_email", "imap")
            .map_err(|e| format!("assert: {}", e))?;

        set_prop(conn, &iri, "foundation:imapHost", vec![str_lit(&input.host)], "imap")?;
        set_prop(conn, &iri, "foundation:imapPort", vec![Object::Integer(input.port as i64)], "imap")?;
        set_prop(conn, &iri, "foundation:imapUseTLS", vec![bool_lit(input.use_tls)], "imap")?;
        set_prop(conn, &iri, "foundation:imapUsername", vec![str_lit(&input.username)], "imap")?;
        set_prop(conn, &iri, "foundation:syncInterval", vec![Object::Integer(input.sync_interval_minutes)], "imap")?;
        set_prop(conn, &iri, "foundation:hasStatus", vec![Object::Iri("foundation:Pending".to_string())], "imap")?;

        let existing_cred = crate::owl::get_iri_property(conn, &iri, "foundation:hasCredential")
            .ok()
            .flatten();

        if !input.password.is_empty() {
            if let Some(old_cred) = &existing_cred {
                Individual::retract(conn, old_cred, "imap").ok();
            }
            let cred_iri = format!("foundation:IMAPCredential_{}", ts);
            let cred = Individual::new(&cred_iri);
            cred.assert(conn, "foundation:UsernamePasswordCredential", &cred_iri, "vpn_key", "imap")
                .map_err(|e| format!("credential: {}", e))?;
            cred.add_property(conn, "foundation:credentialUsername", vec![str_lit(&input.username)], "imap")
                .map_err(|e| format!("credentialUsername: {}", e))?;
            cred.add_property(conn, "foundation:credentialValue", vec![str_lit(&input.password)], "imap")
                .map_err(|e| format!("credentialValue: {}", e))?;
            set_prop(conn, &iri, "foundation:hasCredential", vec![Object::Iri(cred_iri)], "imap")?;
        } else if existing_cred.is_none() {
            return Err("Senha obrigatória para nova conta".to_string());
        }

        Ok(iri)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn imap__get_accounts(
    executor: State<'_, DbExecutor>,
) -> Result<Vec<ImapAccountSummary>, String> {
    executor.read(move |conn| {
        let iris = crate::owl::find_entities_with_property(conn, "rdf:type", "foundation:IMAPAccount")
            .map_err(|e| e.to_string())?;

        iris.into_iter().map(|iri| {
            let label = crate::owl::get_literal_property(conn, &iri, "rdfs:label")
                .unwrap_or_default().unwrap_or_else(|| iri.clone());
            let host = crate::owl::get_literal_property(conn, &iri, "foundation:imapHost")
                .unwrap_or_default().unwrap_or_default();
            let port = get_int(conn, &iri, "foundation:imapPort", 993);
            let use_tls = crate::owl::get_literal_property(conn, &iri, "foundation:imapUseTLS")
                .unwrap_or_default().map(|v| v == "true").unwrap_or(true);
            let username = crate::owl::get_literal_property(conn, &iri, "foundation:imapUsername")
                .unwrap_or_default().unwrap_or_default();
            let sync_interval_minutes = get_int(conn, &iri, "foundation:syncInterval", 15);
            let last_error = crate::owl::get_literal_property(conn, &iri, "foundation:lastConnectionError")
                .unwrap_or_default();
            let is_connected = crate::owl::get_literal_property(conn, &iri, "foundation:isConnected")
                .unwrap_or_default().map(|v| v == "true").unwrap_or(false);

            Ok(ImapAccountSummary {
                iri, label, host, port, use_tls, username,
                sync_interval_minutes, last_error, is_connected,
            })
        }).collect()
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn imap__test_connection(
    host: String,
    port: u16,
    use_tls: bool,
    username: String,
    password: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        connect_and_login(&host, port, use_tls, &username, &password)?;
        Ok(format!("Conexão com {}:{} bem-sucedida.", host, port))
    }).await.map_err(|e| format!("Task error: {}", e))?
}

pub fn connect_and_login(
    host: &str,
    port: u16,
    use_tls: bool,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let addr = (host, port);
    let timeout = std::time::Duration::from_secs(CONNECTION_TIMEOUT_SECS);

    if use_tls {
        let tcp = std::net::TcpStream::connect(addr)
            .map_err(|e| format!("Conexão TCP falhou: {}", e))?;
        tcp.set_read_timeout(Some(timeout)).ok();
        tcp.set_write_timeout(Some(timeout)).ok();
        let tls = native_tls::TlsConnector::new()
            .map_err(|e| format!("TLS init: {}", e))?;
        let tls_stream = tls.connect(host, tcp)
            .map_err(|e| format!("TLS handshake falhou: {}", e))?;
        let mut session = imap::Client::new(tls_stream)
            .login(username, password)
            .map_err(|(e, _)| format!("Login falhou: {}", e))?;
        session.logout().ok();
    } else {
        let stream = std::net::TcpStream::connect(addr)
            .map_err(|e| format!("Conexão falhou: {}", e))?;
        stream.set_read_timeout(Some(timeout)).ok();
        stream.set_write_timeout(Some(timeout)).ok();
        let mut session = imap::Client::new(stream)
            .login(username, password)
            .map_err(|(e, _)| format!("Login falhou: {}", e))?;
        session.logout().ok();
    }
    Ok(())
}

pub fn list_imap_folders(
    host: &str,
    port: u16,
    use_tls: bool,
    username: &str,
    password: &str,
) -> Result<Vec<String>, String> {
    let addr = (host, port);
    let timeout = std::time::Duration::from_secs(CONNECTION_TIMEOUT_SECS);

    let names = if use_tls {
        let tcp = std::net::TcpStream::connect(addr)
            .map_err(|e| format!("TCP: {}", e))?;
        tcp.set_read_timeout(Some(timeout)).ok();
        tcp.set_write_timeout(Some(timeout)).ok();
        let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
        let tls_stream = tls.connect(host, tcp).map_err(|e| format!("TLS: {}", e))?;
        let mut session = imap::Client::new(tls_stream)
            .login(username, password)
            .map_err(|(e, _)| format!("login: {}", e))?;
        let names = session
            .list(Some(""), Some("*"))
            .map_err(|e| e.to_string())?
            .iter()
            .map(|n| n.name().to_string())
            .collect::<Vec<_>>();
        session.logout().ok();
        names
    } else {
        let stream = std::net::TcpStream::connect(addr)
            .map_err(|e| format!("TCP: {}", e))?;
        stream.set_read_timeout(Some(timeout)).ok();
        stream.set_write_timeout(Some(timeout)).ok();
        let mut session = imap::Client::new(stream)
            .login(username, password)
            .map_err(|(e, _)| format!("login: {}", e))?;
        let names = session
            .list(Some(""), Some("*"))
            .map_err(|e| e.to_string())?
            .iter()
            .map(|n| n.name().to_string())
            .collect::<Vec<_>>();
        session.logout().ok();
        names
    };

    Ok(names)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn imap__list_folders(
    host: String,
    port: u16,
    use_tls: bool,
    username: String,
    password: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
        let folders = list_imap_folders(&host, port, use_tls, &username, &password)?;
        Ok(folders)
    }).await.map_err(|e| format!("Task error: {}", e))?
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn imap__save_monitored_folders(
    account_iri: String,
    folders: Vec<String>,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        Individual::clear_property(conn, &account_iri, "foundation:imapMonitoredFolders", "imap").ok();
        if !folders.is_empty() {
            let values: Vec<Object> = folders.iter().map(|f| str_lit(f)).collect();
            Individual::new(&account_iri)
                .add_property(conn, "foundation:imapMonitoredFolders", values, "imap")
                .map_err(|e| format!("folders: {}", e))?;
        }
        Ok(String::new())
    }).await.map(|_| ())
}

#[derive(Debug, Serialize)]
pub struct SyncLogEntry {
    pub iri: String,
    pub started_at: String,
    pub emails_imported: i64,
    pub error: Option<String>,
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn imap__get_sync_history(
    account_iri: String,
    limit: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<SyncLogEntry>, String> {
    executor.read(move |conn| {
        let max = limit.unwrap_or(DEFAULT_SYNC_HISTORY_LIMIT);
        let iris = crate::owl::find_entities_with_property(conn, "foundation:syncForAccount", &account_iri)
            .map_err(|e| e.to_string())?;

        let mut entries: Vec<SyncLogEntry> = iris.into_iter().filter_map(|iri| {
            let started_at = crate::owl::get_literal_property(conn, &iri, "foundation:syncStartedAt")
                .unwrap_or_default().unwrap_or_default();
            let emails_imported = crate::owl::get_literal_property(conn, &iri, "foundation:syncEmailsImported")
                .unwrap_or_default()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let error = crate::owl::get_literal_property(conn, &iri, "foundation:syncErrorMessage")
                .unwrap_or_default();
            Some(SyncLogEntry { iri, started_at, emails_imported, error })
        }).collect();

        entries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        entries.truncate(max as usize);
        Ok(entries)
    }).await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn imap__start_account_sync(
    app: AppHandle,
    account_iri: String,
) -> Result<(), String> {
    crate::imap::sync_worker::start_account_sync(app, account_iri).await;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn imap__delete_account(
    account_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor.write(move |conn| {
        if let Ok(Some(cred_iri)) = crate::owl::get_iri_property(conn, &account_iri, "foundation:hasCredential") {
            Individual::retract(conn, &cred_iri, "imap")
                .map_err(|e| format!("credential: {}", e))?;
        }
        Individual::retract(conn, &account_iri, "imap")
            .map_err(|e| format!("account: {}", e))?;
        Ok(String::new())
    }).await.map(|_| ())
}
