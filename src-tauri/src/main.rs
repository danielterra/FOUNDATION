// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// In debug the exe is a console app so logs reach the terminal when run by hand.
// Under PM2 Windows allocates a fresh visible console for it instead — a blank
// window the user can close by accident, killing the whole dev server. Hide the
// console ONLY when it was allocated exclusively for this process (process list
// of size 1); a shared console (manual run from a terminal) stays untouched.
#[cfg(all(debug_assertions, target_os = "windows"))]
fn hide_orphan_console() {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn GetConsoleProcessList(process_list: *mut u32, process_count: u32) -> u32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
    }
    const SW_HIDE: i32 = 0;
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            let mut pids = [0u32; 2];
            if GetConsoleProcessList(pids.as_mut_ptr(), 2) == 1 {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

fn main() {
    #[cfg(all(debug_assertions, target_os = "windows"))]
    hide_orphan_console();

    if std::env::args().nth(1).as_deref() == Some("build") {
        return;
    }
    FOUNDATION_tauri_app_lib::run()
}
