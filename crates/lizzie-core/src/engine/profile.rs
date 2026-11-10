//! Committed engine settings (Java `leelaz.engine-settings-list` and the analysis keys of
//! `leelaz` / `ui`), stored in this product's `config.json` under `engine`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One configured engine (`EngineData`). Defaults are those of a new entry in Java.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct EngineProfile {
    /// Stable identity across renames and reordering (Java assigns a UUID).
    pub id: String,
    pub name: String,
    /// Command line, split like `Utils.splitCommand`.
    pub command: String,
    /// GTP commands sent after start-up, separated by `;`.
    pub initial_command: String,
    /// Started in the background at launch so switching to it is immediate.
    pub preload: bool,
    pub width: u16,
    pub height: u16,
    /// Komi applied when the engine starts on an empty game.
    pub komi: f64,
    pub is_default: bool,
    /// Written by engine resource setup: this client's resource store owns the command, as long
    /// as the command is still the one it wrote (Java `BundledKataGoProfile`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub managed: Option<crate::resources::ManagedProfile>,
    /// KataGo search-thread policy and stored benchmark results (Java `threadPolicy`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thread_policy: Option<crate::engine::tuning::ThreadPolicy>,
    /// Settings chosen through the engine's GTP configuration protocol (Java
    /// `gtpConfigurationProtocol` / `gtpConfigurationProfile`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gtp_configuration: Option<crate::engine::tuning::GtpConfiguration>,
    /// 远程引擎 (`useJavaSSH`): the command runs on this SSH host.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ssh: Option<crate::engine::ssh::SshSettings>,
    /// Cloud launch parameters selected in the remote compute center; credentials stay in the
    /// account adapter. The command is only the provider marker, never a bearer token.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cloud: Option<crate::engine::cloud::CloudLaunch>,
}

impl EngineProfile {
    /// Runs on an SSH host: its command, model and threads belong to that host
    /// (`EngineThreadPolicy.isRemoteManaged`).
    pub fn is_remote(&self) -> bool {
        self.ssh.is_some() || self.cloud.is_some()
    }
}

impl Default for EngineProfile {
    fn default() -> Self {
        EngineProfile {
            id: String::new(),
            name: String::new(),
            command: String::new(),
            initial_command: String::new(),
            preload: false,
            width: 19,
            height: 19,
            komi: 7.5,
            is_default: false,
            managed: None,
            thread_policy: None,
            gtp_configuration: None,
            ssh: None,
            cloud: None,
        }
    }
}

/// Which engine starts with the application (`autoload-default` / `autoload-last` /
/// `autoload-empty`; none of them = ask).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum EngineStartup {
    /// The default engine (a new Java profile selects this).
    #[default]
    Default,
    /// The engine used last.
    Last,
    /// No engine.
    Empty,
    /// Ask with the engine list at start-up.
    Manual,
}

/// Analysis limits and update interval (Java defaults in brackets).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct AnalysisSettings {
    /// `analyze-update-interval-centisec` [10].
    pub interval_centisec: u32,
    /// `analyze-update-interval-centisecssh` [10]: the interval of engines that run over SSH
    /// (`Leelaz.getInterval`).
    pub interval_centisec_ssh: u32,
    /// `limit-time` [true] with `max-analyze-time-seconds` [600].
    pub limit_time: bool,
    pub max_analyze_time_seconds: u32,
    /// `limit-playout` [false] with `limit-playouts` [100000].
    pub limit_playout: bool,
    pub limit_playouts: u64,
    /// `limit-max-suggestion` [10]; 0 shows every candidate.
    pub limit_max_suggestion: u32,
    /// `limit-branch-length` [15]; 0 shows the whole variation.
    pub limit_branch_length: u32,
    /// `fast-engine-change` [true]: a switched-away engine keeps running in the background.
    pub fast_engine_change: bool,
    /// `show-ponder-limited-tips` [true]: explain once when a limit stops the analysis.
    pub show_ponder_limited_tips: bool,
    /// `ui.auto-check-engine-alive` [true]: a foreground engine that ends unexpectedly is
    /// started again and brought back to the current position (`EngineManager.checkEngineAlive`).
    pub auto_check_engine_alive: bool,
    /// `leelaz.enable-lizzie-cache` [true]: a node's stored result gives way only to a stronger
    /// one (more visits, or marked to give way); off, every new result replaces it.
    pub enable_lizzie_cache: bool,
    /// `leelaz.stop-at-empty-board` [false]: an analysis of the empty board stops at its first
    /// result, without the limit notice (`Leelaz.java:7458-7468`).
    pub stop_at_empty_board: bool,
    /// `ui.always-gtp` [true]: the GTP console records while it is hidden; off, a hidden console
    /// records only engines that are still starting.
    pub always_gtp: bool,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        AnalysisSettings {
            interval_centisec: 10,
            interval_centisec_ssh: 10,
            limit_time: true,
            max_analyze_time_seconds: 600,
            limit_playout: false,
            limit_playouts: 100_000,
            limit_max_suggestion: 10,
            limit_branch_length: 15,
            fast_engine_change: true,
            show_ponder_limited_tips: true,
            auto_check_engine_alive: true,
            enable_lizzie_cache: true,
            stop_at_empty_board: false,
            always_gtp: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EngineSettings {
    pub engines: Vec<EngineProfile>,
    pub startup: EngineStartup,
    /// Id of the engine used last (`last-engine`).
    pub last_engine: Option<String>,
    pub analysis: AnalysisSettings,
    /// `kata-rules`: the rules chosen in 引擎规则, JSON or preset name; empty = engine default.
    pub kata_rules: String,
    /// `auto-load-kata-rules` [false]: apply `kata_rules` whenever KataGo starts.
    pub auto_load_kata_rules: bool,
    /// `ui.txt-kata-engine-pda`: last confirmed PDA; Java's empty default is unconfirmed.
    pub kata_pda: Option<f64>,
    /// `ui.chk-kata-engine-pda` [false]; confirmed nonzero PDA enables it.
    pub kata_pda_enabled: bool,
    /// `ui.enable-startup-benchmark` [false on a new profile, Config.applyFirstLaunchDefaults]:
    /// the user opted in to the official benchmark after start-up when the foreground KataGo has
    /// no stored result (Settings > Advanced).
    pub startup_benchmark: bool,
    /// `ui.katago-startup-benchmark-dismissed-signature`: benchmark signature the user dismissed
    /// the start-up benchmark for (KataGoRuntimeHelper.java:5880-5905).
    pub startup_benchmark_dismissed_signature: String,
    /// `ui.katago-startup-benchmark-dismissed-version`.
    pub startup_benchmark_dismissed_version: u32,
}

impl EngineSettings {
    /// The engine `EngineStartup::Default` loads: the one marked default, else the first.
    pub fn default_index(&self) -> Option<usize> {
        if self.engines.is_empty() {
            return None;
        }
        Some(self.engines.iter().position(|e| e.is_default).unwrap_or(0))
    }

    /// The engine to start with the application, if any.
    pub fn startup_index(&self) -> Option<usize> {
        match self.startup {
            EngineStartup::Default => self.default_index(),
            EngineStartup::Last => {
                let id = self.last_engine.as_deref()?;
                self.engines.iter().position(|e| e.id == id)
            }
            EngineStartup::Empty | EngineStartup::Manual => None,
        }
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.engines.iter().position(|e| e.id == id)
    }
}

/// A fresh id for a new engine entry.
pub fn new_profile_id(existing: &[EngineProfile]) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mut n = nanos;
    loop {
        let id = format!("{n:032x}");
        if !existing.iter().any(|e| e.id == id) {
            return id;
        }
        n = n.wrapping_add(1);
    }
}
