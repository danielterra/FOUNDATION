use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::owl::Connection;

#[derive(Serialize, Clone)]
pub struct RecoveryResult {
    pub backup_path: String,
    pub new_db_path: String,
    pub workspace: String,
    pub triples_recovered: i64,
}

fn emit_progress(app: &AppHandle, stage: &str, detail: &str) {
    let _ = app.emit(
        "recovery-progress",
        serde_json::json!({ "stage": stage, "detail": detail }),
    );
    super::log_backend("info", &format!("[recovery] {}: {}", stage, detail));
}

fn emit_metric(
    app: &AppHandle,
    stage: &str,
    bytes: u64,
    total_bytes: Option<u64>,
    rate_bps: f64,
) {
    let _ = app.emit(
        "recovery-progress",
        serde_json::json!({
            "stage": stage,
            "bytes": bytes,
            "total_bytes": total_bytes,
            "rate_bps": rate_bps,
        }),
    );
}

fn locate_sqlite3(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let resource = app
            .path()
            .resource_dir()
            .map_err(|e| format!("resource_dir indisponível: {e}"))?
            .join("sqlite3.exe");
        if resource.is_file() {
            return Ok(resource);
        }
        return Err(format!(
            "sqlite3.exe não encontrado em {}. Reinstale o aplicativo.",
            resource.display()
        ));
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        let candidates = ["/usr/bin/sqlite3", "/usr/local/bin/sqlite3", "/opt/homebrew/bin/sqlite3"];
        for c in candidates.iter() {
            if Path::new(c).is_file() {
                return Ok(PathBuf::from(c));
            }
        }
        Err("sqlite3 não encontrado no sistema. Instale o pacote sqlite3.".into())
    }
}

fn is_db_corrupt(db_path: &Path) -> bool {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    match Connection::open_with_flags(db_path, flags) {
        Ok(conn) => {
            let res: rusqlite::Result<i64> =
                conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0));
            res.is_err()
        }
        Err(_) => true,
    }
}

fn timestamp() -> String {
    use chrono::Utc;
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

fn spawn_file_growth_watcher(
    app: AppHandle,
    stage: &'static str,
    path: PathBuf,
    expected_total: Option<u64>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut samples: std::collections::VecDeque<(std::time::Instant, u64)> =
            std::collections::VecDeque::with_capacity(8);
        while !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(800));
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let now = std::time::Instant::now();
            samples.push_back((now, size));
            while samples.len() > 8 {
                samples.pop_front();
            }
            let rate_bps = if samples.len() >= 2 {
                let (t0, b0) = samples.front().copied().unwrap();
                let (t1, b1) = samples.back().copied().unwrap();
                let dt = (t1 - t0).as_secs_f64().max(0.001);
                ((b1 as f64) - (b0 as f64)) / dt
            } else {
                0.0
            };
            emit_metric(&app, stage, size, expected_total, rate_bps);
        }
    })
}

fn run_sqlite3(sqlite3: &Path, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new(sqlite3);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().map_err(|e| format!("falha ao executar sqlite3: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "sqlite3 retornou erro (exit={:?}): {}",
            output.status.code(),
            stderr.trim()
        ));
    }
    Ok(())
}

fn move_or_copy(src: &Path, dst: &Path) -> Result<(), String> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst)
        .map_err(|e| format!("Falha ao copiar {} → {}: {e}", src.display(), dst.display()))?;
    std::fs::remove_file(src)
        .map_err(|e| format!("Falha ao remover origem {}: {e}", src.display()))?;
    Ok(())
}

#[tauri::command]
pub async fn recover_database(app: AppHandle) -> Result<RecoveryResult, String> {
    let foundation_dir = crate::config::get_foundation_dir(&app);
    let db_path = foundation_dir.join("FOUNDATION.db");

    if !db_path.is_file() {
        return Err(format!("Banco não encontrado em {}", db_path.display()));
    }

    let sqlite3 = locate_sqlite3(&app)?;
    emit_progress(&app, "verifying", "Verificando integridade do banco");

    if !is_db_corrupt(&db_path) {
        return Err("O banco abriu normalmente — não há corrupção a recuperar.".into());
    }

    let ts = timestamp();
    let downloads = dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .ok_or_else(|| "Não foi possível localizar a pasta Downloads".to_string())?;
    let workspace = downloads.join(format!("Foundation-recovery-{ts}"));
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("Falha ao criar pasta de trabalho {}: {e}", workspace.display()))?;

    let backup_path = workspace.join(format!("FOUNDATION.db.corrupt-{ts}.bak"));
    let sql_dump_path = workspace.join(format!("FOUNDATION.recover-{ts}.sql"));
    let new_db_path = workspace.join(format!("FOUNDATION.db.new-{ts}"));

    let original_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    emit_progress(
        &app,
        "backup",
        &format!(
            "Movendo arquivo corrompido para {}",
            workspace.file_name().unwrap_or_default().to_string_lossy()
        ),
    );
    move_or_copy(&db_path, &backup_path)?;
    let _ = std::fs::remove_file(foundation_dir.join("FOUNDATION.db-wal"));
    let _ = std::fs::remove_file(foundation_dir.join("FOUNDATION.db-shm"));
    let _ = std::fs::remove_file(foundation_dir.join("FOUNDATION.db-journal"));

    let result = do_recovery_steps(
        &app,
        &sqlite3,
        &backup_path,
        &sql_dump_path,
        &new_db_path,
        original_size,
    );

    match result {
        Ok(triples_recovered) => {
            emit_progress(&app, "finalizing", "Instalando banco recuperado");
            move_or_copy(&new_db_path, &db_path)?;
            let _ = std::fs::remove_file(&sql_dump_path);
            emit_progress(&app, "done", "Recuperação concluída");
            Ok(RecoveryResult {
                backup_path: backup_path.to_string_lossy().to_string(),
                new_db_path: db_path.to_string_lossy().to_string(),
                workspace: workspace.to_string_lossy().to_string(),
                triples_recovered,
            })
        }
        Err(e) => {
            emit_progress(&app, "rollback", "Restaurando arquivo corrompido");
            let _ = std::fs::remove_file(&new_db_path);
            if let Err(restore_err) = move_or_copy(&backup_path, &db_path) {
                return Err(format!(
                    "Recuperação falhou: {e}. Falha adicional ao restaurar backup ({restore_err}). \
                     O arquivo original está em {}",
                    backup_path.display()
                ));
            }
            Err(e)
        }
    }
}

fn do_recovery_steps(
    app: &AppHandle,
    sqlite3: &Path,
    backup_path: &Path,
    sql_dump_path: &Path,
    new_db_path: &Path,
    original_size: u64,
) -> Result<i64, String> {
    emit_progress(
        app,
        "dumping",
        &format!("Extraindo dados ({} MB de origem)", original_size / (1024 * 1024)),
    );

    let stop_dump = Arc::new(AtomicBool::new(false));
    let dump_watcher = spawn_file_growth_watcher(
        app.clone(),
        "dumping",
        sql_dump_path.to_path_buf(),
        Some(original_size),
        stop_dump.clone(),
    );

    let dump_result = (|| -> Result<(), String> {
        let sql_file = std::fs::File::create(sql_dump_path)
            .map_err(|e| format!("Falha ao criar arquivo de dump: {e}"))?;
        let mut cmd = Command::new(sqlite3);
        cmd.arg(backup_path)
            .arg(".recover")
            .stdout(Stdio::from(sql_file))
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let output = cmd.output().map_err(|e| format!("falha ao executar sqlite3 .recover: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "sqlite3 .recover falhou (exit={:?}): {}",
                output.status.code(),
                stderr.trim()
            ));
        }
        Ok(())
    })();

    stop_dump.store(true, Ordering::Relaxed);
    let _ = dump_watcher.join();
    dump_result?;

    let dump_size = std::fs::metadata(sql_dump_path).map(|m| m.len()).unwrap_or(0);
    emit_progress(
        app,
        "importing",
        &format!("Importando para banco novo ({} MB de SQL)", dump_size / (1024 * 1024)),
    );

    let stop_import = Arc::new(AtomicBool::new(false));
    let import_watcher = spawn_file_growth_watcher(
        app.clone(),
        "importing",
        new_db_path.to_path_buf(),
        Some(original_size),
        stop_import.clone(),
    );

    let import_result = run_sqlite3(
        sqlite3,
        &[
            new_db_path.to_string_lossy().as_ref(),
            "PRAGMA journal_mode = OFF;",
            "PRAGMA synchronous = OFF;",
            "PRAGMA temp_store = MEMORY;",
            "PRAGMA cache_size = -2000000;",
            "PRAGMA locking_mode = EXCLUSIVE;",
            &format!(".read {}", sql_dump_path.to_string_lossy()),
        ],
    );

    stop_import.store(true, Ordering::Relaxed);
    let _ = import_watcher.join();
    import_result?;

    emit_progress(app, "verifying", "Verificando banco recuperado");
    let conn = Connection::open(new_db_path)
        .map_err(|e| format!("Não foi possível abrir banco recuperado: {e}"))?;
    let integrity: String = conn
        .query_row("PRAGMA integrity_check;", [], |r| r.get(0))
        .map_err(|e| format!("integrity_check falhou: {e}"))?;
    if integrity != "ok" {
        return Err(format!("Banco recuperado ainda inconsistente: {integrity}"));
    }
    let triples: i64 = conn
        .query_row("SELECT COUNT(*) FROM triples;", [], |r| r.get(0))
        .unwrap_or(0);

    Ok(triples)
}
