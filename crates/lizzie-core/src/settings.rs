//! Committed settings and application data owned by this product, and their files.
//!
//! The product keeps its own files in its own per-user directory (resolved by the platform
//! adapter from the application identifier). Java's `config.txt` / `persist` are never read or
//! written here; the explicit import ([`crate::java_import`]) only reads them.
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

/// Editing settings (Java `ui.*` keys; defaults from `Config.java`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct EditSettings {
    /// 启用双击找子 (`allow-double-click`, default on).
    pub allow_double_click: bool,
    /// 启用拖动棋子 (`allow-drag`, default off).
    pub allow_drag: bool,
    /// Right click opens the board context menu; off, it goes back one move
    /// (`show-right-menu`, default on).
    pub show_right_menu: bool,
    /// 显示新建棋盘确认对话框 (`show-new-board-hint`, default on).
    pub show_new_board_hint: bool,
    /// 标记工具 shown in the top toolbar (`show-markup-tools`, default off).
    pub show_markup_tools: bool,
    /// 设置棋盘大小 "其他" width and height (`other-size-width` / `-height`, default 21).
    pub other_size_width: u16,
    pub other_size_height: u16,
    /// 五子棋无提子规则 (`no-capture`, default off): stones placed or read while it is on remove
    /// nothing (`Board.place`, `BoardHistoryList.place`).
    pub no_capture: bool,
}

impl Default for EditSettings {
    fn default() -> Self {
        EditSettings {
            allow_double_click: true,
            allow_drag: false,
            show_right_menu: true,
            show_new_board_hint: true,
            show_markup_tools: false,
            other_size_width: 21,
            other_size_height: 21,
            no_capture: false,
        }
    }
}

/// 存档与读档 check boxes (Java `ui.auto-save-exit` default on, `ui.resume-previous-game` default
/// off, `ui.load-save-zoom` default on).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct RecoverySettings {
    /// 自动存档: the automatic slot is written on exit and every 60 seconds.
    pub auto_save_on_exit: bool,
    /// 启动时自动恢复: start with the automatic slot's record.
    pub resume_previous_game: bool,
    /// 鼠标悬停放大缩略棋盘.
    pub zoom_thumbnails: bool,
}

impl Default for RecoverySettings {
    fn default() -> Self {
        RecoverySettings {
            auto_save_on_exit: true,
            resume_previous_game: false,
            zoom_thumbnails: true,
        }
    }
}

/// How analysis numbers are shown, in every display and in the 带评论 export (Java
/// `ui.win-rate-always-black`, `ui.show-katago-score-lead-with-komi`, both default off). The
/// numbers follow them through [`crate::viewpoint::Viewpoint`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct DisplaySettings {
    /// 总是显示黑胜率: win rates and score leads are Black's instead of the side to move's.
    pub winrate_always_black: bool,
    /// 目差显示 > 盘面: score leads are the board difference (KataGo's lead plus the komi for
    /// Black, minus it for White) instead of the lead after komi (目差).
    pub score_lead_with_komi: bool,
}

pub const RECENT_LIMIT: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub record: RecordSettings,
    pub ui: UiSettings,
    /// Oldest first, at most [`RECENT_LIMIT`] (Java `ui.recent-file-paths`).
    pub recent_files: Vec<String>,
    /// Engines and analysis (Java `leelaz.engine-settings-list` and analysis keys).
    pub engine: crate::engine::profile::EngineSettings,
    /// Editing (Java `ui.allow-*`, `show-new-board-hint`, ...).
    pub edit: EditSettings,
    /// Network access (Java `ui.network-proxy-*`).
    pub network: NetworkSettings,
    /// 存档与读档 (Java `ui.auto-save-exit`, `resume-previous-game`, `load-save-zoom`).
    pub recovery: RecoverySettings,
    /// Analysis numbers (Java `ui.win-rate-always-black`, `show-katago-score-lead-with-komi`).
    pub display: DisplaySettings,
}

/// How this client reaches the network (Java `NetworkProxy`, `ui.network-proxy-mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ProxyMode {
    /// `direct` (Java default): no proxy.
    #[default]
    Direct,
    /// `system`: the operating system's proxy settings.
    System,
    /// `manual`: an HTTP proxy at `proxy_host:proxy_port`.
    Manual,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct NetworkSettings {
    pub proxy_mode: ProxyMode,
    /// `network-proxy-host` (manual mode; non-empty when manual).
    pub proxy_host: String,
    /// `network-proxy-port` (manual mode; 1–65535 when manual).
    pub proxy_port: u16,
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
    /// Where the picture chooser starts (Java `filesystem.last-image-folder`, else `last-folder`).
    pub last_image_folder: Option<String>,
    pub window: Option<WindowPlacement>,
}

/// File locations inside the product's per-user data directories.
#[derive(Clone, Debug)]
pub struct Storage {
    pub config_file: PathBuf,
    pub persist_file: PathBuf,
    pub logs_dir: PathBuf,
    /// Downloaded and imported engine resources (packages, weights, runtimes, partial downloads).
    pub resources_dir: PathBuf,
    /// 存档与读档 slots (Java `save/`).
    pub save_dir: PathBuf,
}

impl Storage {
    pub fn in_dirs(config_dir: &Path, data_dir: &Path) -> Storage {
        Storage {
            config_file: config_dir.join("config.json"),
            persist_file: config_dir.join("persist.json"),
            logs_dir: data_dir.join("logs"),
            resources_dir: data_dir.join("resources"),
            save_dir: data_dir.join("save"),
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
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

/// Replaces `path` with `bytes` through a temporary file and rename; on failure the temporary
/// file is removed and `path` is unchanged.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let result = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Writes `config` and, if given, `persisted` as one change: when the second write fails, the
/// first file is put back as it was (or removed if it did not exist). An existing settings file
/// that cannot be read (and so could not be put back) is not replaced at all.
pub fn save_both(storage: &Storage, config: &Config, persisted: Option<&Persisted>) -> std::io::Result<()> {
    let previous = match std::fs::read(&storage.config_file) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    save_json(&storage.config_file, config)?;
    if let Some(p) = persisted {
        if let Err(e) = save_json(&storage.persist_file, p) {
            let restored = match previous {
                Some(bytes) => write_atomic(&storage.config_file, &bytes),
                None => std::fs::remove_file(&storage.config_file),
            };
            return Err(match restored {
                Ok(()) => e,
                Err(r) => std::io::Error::new(
                    e.kind(),
                    format!("{e}; putting {} back failed: {r}", storage.config_file.display()),
                ),
            });
        }
    }
    Ok(())
}
