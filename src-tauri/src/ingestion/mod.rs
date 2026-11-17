mod worker;

#[cfg(test)]
mod eval;

pub fn start(app: tauri::AppHandle) {
    worker::start(app);
}
