// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod linux_graphics;

fn main() {
    // A copy of this program started to replace the portable folder's files after the update.
    if let Some(code) = lizzieyzy_tauri_lib::update_helper() {
        std::process::exit(code);
    }
    #[cfg(target_os = "linux")]
    linux_graphics::prepare();
    lizzieyzy_tauri_lib::run();
}
