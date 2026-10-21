// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod linux_graphics;

fn main() {
    #[cfg(target_os = "linux")]
    linux_graphics::prepare();
    lizzieyzy_tauri_lib::run();
}
