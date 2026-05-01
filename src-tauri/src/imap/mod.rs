pub mod extractor;
pub mod sync_worker;

pub(crate) fn log_error(msg: &str) {
    let path = dirs::data_local_dir()
        .map(|d| d.join("org.w3id.foundation").join("application.log"));
    if let Some(path) = path {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = f.write_all(format!("[{}] [BACKEND] [ERROR] {}\n", ts, msg).as_bytes());
        }
    }
}
