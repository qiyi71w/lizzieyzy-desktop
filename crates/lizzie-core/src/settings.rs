//! Committed settings and application data owned by this product, and their files.
//!
//! The product keeps its own files in its own per-user directory (resolved by the platform
//! adapter from the application identifier). Java's `config.txt` / `persist` are never read or
//! written here; importing them is a separate, explicit operation.
//!
//! - `config.json` (Java `config.txt` role): domain settings and the recent-record list.
//! - `persist.json` (Java `persist` role): window placement and the last opened folder. "重置界面
//!   位置" deletes it and stops it being written again before the next start, like Java
//!   `Config.deletePersist`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// UI language (Java `AppLocale`, `ui.use-language`). Changing it takes effect after a restart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Language {
    #[default]
    System,
    ZhCn,
    EnUs,
    KoKr,
    JaJp,
    ZhTw,
    ThTh,
}

impl Language {
    /// `AppLocale.fromSystemLocale`: unknown languages fall back to Simplified Chinese.
    pub fn from_system_locale(tag: Option<&str>) -> Language {
        let Some(tag) = tag else { return Language::ZhCn };
        let lower = tag.replace('_', "-").to_lowercase();
        let mut parts = lower.split('-');
        let lang = parts.next().unwrap_or("");
        let rest: Vec<&str> = parts.collect();
        match lang {
            "zh" if rest.iter().any(|p| matches!(*p, "hant" | "tw" | "hk" | "mo")) => Language::ZhTw,
            "zh" => Language::ZhCn,
            "en" => Language::EnUs,
            "ja" => Language::JaJp,
            "ko" => Language::KoKr,
            "th" => Language::ThTh,
            _ => Language::ZhCn,
        }
    }

    /// Concrete language for a setting, resolving `System` with the OS locale.
    pub fn resolve(self, system_locale: Option<&str>) -> Language {
        match self {
            Language::System => Language::from_system_locale(system_locale),
            other => other,
        }
    }
}

/// Record open/save settings (Java `read-komi` default true, `load-sgf-last` default false,
/// `ui.show-replace-file-hint` default true).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RecordSettings {
    pub read_komi: bool,
    pub load_sgf_last: bool,
    /// Ask before "保存" overwrites the record's own file.
    pub show_replace_file_hint: bool,
}

impl Default for RecordSettings {
    fn default() -> Self {
        RecordSettings {
            read_komi: true,
            load_sgf_last: false,
            show_replace_file_hint: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiSettings {
    pub language: Language,
    /// Java `ui.play-sound`, default true.
    pub play_sound: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            language: Language::default(),
            play_sound: true,
        }
    }
}

pub const RECENT_LIMIT: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub record: RecordSettings,
    pub ui: UiSettings,
    /// Oldest first, at most [`RECENT_LIMIT`] (Java `ui.recent-file-paths`).
    pub recent_files: Vec<String>,
}

impl Config {
    /// `Config.saveRecentFilePaths`: an existing entry moves to the newest position.
    pub fn remember_recent(&mut self, path: &str) {
        self.recent_files.retain(|p| p != path);
        if self.recent_files.len() >= RECENT_LIMIT {
            self.recent_files.remove(0);
        }
        self.recent_files.push(path.to_string());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Persisted {
    pub last_folder: Option<String>,
    pub window: Option<WindowPlacement>,
}

/// File locations inside the product's per-user data directories.
#[derive(Clone, Debug)]
pub struct Storage {
    pub config_file: PathBuf,
    pub persist_file: PathBuf,
    pub logs_dir: PathBuf,
}

impl Storage {
    pub fn in_dirs(config_dir: &Path, data_dir: &Path) -> Storage {
        Storage {
            config_file: config_dir.join("config.json"),
            persist_file: config_dir.join("persist.json"),
            logs_dir: data_dir.join("logs"),
        }
    }
}

/// Reads a JSON file; a missing or unreadable file yields defaults (Java falls back to its
/// default config the same way).
pub fn load_json<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Writes through a temporary file and rename so a crash never leaves a partial file.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)
}
