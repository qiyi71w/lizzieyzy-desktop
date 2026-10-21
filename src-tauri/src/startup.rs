//! Failures before the main window exists. Tauri creates the configured window before `setup`
//! runs, so without this a missing system web view (the WebView2 Runtime on Windows) would end a
//! windowless release build with no message and no log line.

use std::path::{Path, PathBuf};

pub enum Failure {
    /// The Microsoft Edge WebView2 Runtime cannot be found.
    #[cfg(windows)]
    MissingWebView(String),
    /// Any other error while creating the application or its window.
    Other(String),
}

impl Failure {
    fn detail(&self) -> &str {
        match self {
            #[cfg(windows)]
            Failure::MissingWebView(e) => e,
            Failure::Other(e) => e,
        }
    }
}

/// The log directory of the running identity before the application exists: the same
/// `<data dir>/<identifier>/logs` the application resolves through Tauri's path API.
pub fn logs_dir(identifier: &str) -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(identifier).join("logs"))
}

/// Records the failure in `crash.log` under `logs`, writes it to stderr and, on Windows (no
/// console), shows it in a message box.
pub fn report(logs: Option<&Path>, failure: &Failure) {
    let entry = format!("STARTUP {}", failure.detail());
    let crash = logs.and_then(|logs| crate::logging::append_crash(logs, &entry));
    let text = message(failure, crash.as_deref(), chinese());
    eprintln!("{text}");
    #[cfg(windows)]
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("LizzieYzy Tauri")
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn chinese() -> bool {
    sys_locale::get_locale().is_some_and(|l| l.starts_with("zh"))
}

fn message(failure: &Failure, crash: Option<&Path>, chinese: bool) -> String {
    let mut text = match (failure, chinese) {
        #[cfg(windows)]
        (Failure::MissingWebView(e), true) => format!(
            "LizzieYzy Tauri 无法启动：找不到 Microsoft Edge WebView2 Runtime（{e}）。\n\n\
             请从 Microsoft 官网 https://developer.microsoft.com/microsoft-edge/webview2/ \
             下载安装“常青版（Evergreen）”运行时后再启动。安装版安装程序会自动安装它。"
        ),
        #[cfg(windows)]
        (Failure::MissingWebView(e), false) => format!(
            "LizzieYzy Tauri cannot start: the Microsoft Edge WebView2 Runtime was not found ({e}).\n\n\
             Install the Evergreen runtime from https://developer.microsoft.com/microsoft-edge/webview2/ \
             and start the program again. The installer version installs it automatically."
        ),
        (Failure::Other(e), true) => format!("LizzieYzy Tauri 无法启动：{e}"),
        (Failure::Other(e), false) => format!("LizzieYzy Tauri cannot start: {e}"),
    };
    if let Some(path) = crash {
        text.push_str(&if chinese {
            format!("\n\n详细信息已写入：{}", path.display())
        } else {
            format!("\n\nDetails were written to: {}", path.display())
        });
    }
    text
}
