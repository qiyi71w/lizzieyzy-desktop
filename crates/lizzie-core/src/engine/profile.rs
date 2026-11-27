//! Committed engine settings (Java `leelaz.engine-settings-list` and the analysis keys of
//! `leelaz` / `ui`), stored in this product's `config.json` under `engine`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::runtime::LeelaOption;

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

    /// Its process runs on another host (Java `EngineThreadPolicy.isRemoteManaged`): an SSH or
    /// 远程算力 engine, or a command that starts `ssh` / `plink`.
    pub fn is_remote_managed(&self) -> bool {
        self.is_remote() || super::ssh::is_external_ssh(&self.command)
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
    /// `limit-max-suggestion` [10]; 0 shows every candidate. 综合设置's 候选点上限 and the top
    /// toolbar's 候选点 field edit it; a stored value that is not such a count reads as 10.
    #[serde(deserialize_with = "suggestion_limit_or_default")]
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

fn suggestion_limit_or_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_u64().and_then(|v| u32::try_from(v).ok()).unwrap_or(AnalysisSettings::default().limit_max_suggestion))
}

fn text_or_none<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(text) => Some(text),
        _ => None,
    })
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
    /// `ui.rule-preset`: the id of the rule set (`RuleSet`) picked last in the 规则面板 with 启动时自动应用
    /// on. It counts only while KataGo still starts with its rules (`RuleSet::chosen`). A stored
    /// value that is no text is none, and the rest of the settings stays.
    #[serde(deserialize_with = "text_or_none")]
    pub rule_set: Option<String>,
    /// `ui.txt-kata-engine-pda`: last confirmed PDA; Java's empty default is unconfirmed.
    pub kata_pda: Option<f64>,
    /// `ui.chk-kata-engine-pda` [false]; confirmed nonzero PDA enables it.
    pub kata_pda_enabled: bool,
    /// 广度, the start-up values and the toolbar's two groups (引擎参数 window).
    pub kata_params: KataParamSettings,
    /// The Leela Zero / Sai 引擎参数 window (`SetLeelaEngines`).
    pub leela_options: LeelaOptionSettings,
    /// KataGoPda's dedicated dialog; never a stored raw GTP command.
    pub custom_pda: CustomPdaSettings,
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

/// KataGo parameters of the toolbar and the 引擎参数 window (`SetKataEngines`) besides the
/// toolbar's PDA, which [`EngineSettings::kata_pda`] holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct KataParamSettings {
    /// `ui.txt-kata-engine-wrn`: the last 广度 value; Java's empty default is none.
    pub wrn: Option<f64>,
    /// `ui.chk-kata-engine-wrn` [false].
    pub wrn_enabled: bool,
    /// `ui.autoload-kata-engine-pda` [false] with `ui.auto-load-txt-kata-engine-pda`: set on
    /// every KataGo that starts, except a `KataGoPda` one.
    pub auto_load_pda: bool,
    pub auto_load_pda_value: Option<f64>,
    /// `ui.autoload-kata-engine-wrn` [false] with `ui.auto-load-txt-kata-engine-wrn`.
    pub auto_load_wrn: bool,
    pub auto_load_wrn_value: Option<f64>,
    /// `ui.show-pda-in-menu` / `ui.show-wrn-in-menu` [true]: the toolbar shows the group. The
    /// 引擎参数 window's 在工具栏中控制 and the top toolbar's 自定义内容 set them; a stored value
    /// that is not a boolean reads as shown.
    #[serde(deserialize_with = "crate::settings::bool_or_true")]
    pub show_pda: bool,
    #[serde(deserialize_with = "crate::settings::bool_or_true")]
    pub show_wrn: bool,
}

impl Default for KataParamSettings {
    fn default() -> Self {
        KataParamSettings {
            wrn: None,
            wrn_enabled: false,
            auto_load_pda: false,
            auto_load_pda_value: None,
            auto_load_wrn: false,
            auto_load_wrn_value: None,
            show_pda: true,
            show_wrn: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CustomPdaMode {
    #[default]
    Disabled,
    Dynamic,
    Fixed,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct CustomPdaSettings {
    pub mode: CustomPdaMode,
    pub dynamic_cap: Option<f64>,
    pub fixed_value: Option<f64>,
    pub auto_apply: bool,
}

impl CustomPdaSettings {
    /// Java's `SetKataPDA` mapping, in the order the dialog sends it: 不启用 `pda 0`; 动态
    /// `pda 0` then `dympdacap <cap>`; 固定 `pda <value>`. A missing value sends nothing (the
    /// dialog saves an empty field without a command), and a zero or non-finite 动态 系数 is
    /// invalid (`SetKataPDA.wrongParameter`), so neither reaches an engine.
    pub fn commands(&self) -> Vec<String> {
        match self.mode {
            CustomPdaMode::Disabled => vec!["pda 0".to_string()],
            CustomPdaMode::Dynamic => self
                .dynamic_cap
                .filter(|cap| cap.is_finite() && *cap != 0.0)
                .map(|cap| vec!["pda 0".to_string(), format!("dympdacap {}", java_double(cap))])
                .unwrap_or_default(),
            CustomPdaMode::Fixed => self
                .fixed_value
                .filter(|value| value.is_finite())
                .map(|value| vec![format!("pda {}", java_double(value))])
                .unwrap_or_default(),
        }
    }

    /// Java's `AutoPDA`, the one command 自动套用 replays at start-up and on a handicap game's
    /// first White move (`Leelaz`, `maybeAjustPDA`): 不启用 `pda 0`; 动态 `dympdacap <cap>`
    /// only, so the engine keeps its own PDA; 固定 `pda <value>`. Only the dialog's 确定 sends
    /// [`Self::commands`].
    pub fn auto_commands(&self) -> Vec<String> {
        match self.mode {
            CustomPdaMode::Dynamic => self
                .dynamic_cap
                .filter(|cap| cap.is_finite() && *cap != 0.0)
                .map(|cap| vec![format!("dympdacap {}", java_double(cap))])
                .unwrap_or_default(),
            CustomPdaMode::Disabled | CustomPdaMode::Fixed => self.commands(),
        }
    }

    /// The dialog's refusal (`SetKataPDA.wrongParameter`): a 动态 系数 that is zero or not a
    /// finite number, or a 固定 value that is not finite.
    pub fn invalid(&self) -> bool {
        match self.mode {
            CustomPdaMode::Disabled => false,
            CustomPdaMode::Dynamic => self.dynamic_cap.is_some_and(|cap| !cap.is_finite() || cap == 0.0),
            CustomPdaMode::Fixed => self.fixed_value.is_some_and(|value| !value.is_finite()),
        }
    }
}

/// `Double.toString` for the values a PDA takes (a whole number keeps `.0`).
fn java_double(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e7 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// One row of the Leela Zero / Sai 引擎参数 window: 修改 (`ui.chk-lzsai-engine<row>` [false]),
/// 自动加载 (`ui.autoload-Lzsai-engine<row>` [false]) and the value as typed
/// (`ui.txt-lzsai-engine<row>` [""]). 自动加载 keeps its tick while 修改 is off.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct LeelaOptionSetting {
    pub enabled: bool,
    pub auto_load: bool,
    pub value: String,
}

/// The four rows of the Leela Zero / Sai 引擎参数 window, global like Java's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct LeelaOptionSettings {
    /// `mem`: `Maximum Memory Use (MiB)`.
    pub memory: LeelaOptionSetting,
    pub visits: LeelaOptionSetting,
    pub lagbuffer: LeelaOptionSetting,
    /// `resign`: `Resign Percentage`.
    pub resign: LeelaOptionSetting,
}

impl LeelaOptionSettings {
    pub fn get(&self, option: LeelaOption) -> &LeelaOptionSetting {
        match option {
            LeelaOption::Memory => &self.memory,
            LeelaOption::Visits => &self.visits,
            LeelaOption::Lagbuffer => &self.lagbuffer,
            LeelaOption::Resign => &self.resign,
        }
    }

    pub fn get_mut(&mut self, option: LeelaOption) -> &mut LeelaOptionSetting {
        match option {
            LeelaOption::Memory => &mut self.memory,
            LeelaOption::Visits => &mut self.visits,
            LeelaOption::Lagbuffer => &mut self.lagbuffer,
            LeelaOption::Resign => &mut self.resign,
        }
    }

    /// The rows whose value is sent now: 修改 checked (`SetLeelaEngines` 确定).
    pub fn edited(&self) -> Vec<(LeelaOption, String)> {
        self.rows(|r| r.enabled)
    }

    /// The rows every Leela Zero / Sai start gets: 修改 and 自动加载 checked.
    pub fn auto_loaded(&self) -> Vec<(LeelaOption, String)> {
        self.rows(|r| r.enabled && r.auto_load)
    }

    fn rows(&self, take: impl Fn(&LeelaOptionSetting) -> bool) -> Vec<(LeelaOption, String)> {
        LeelaOption::ALL
            .into_iter()
            .filter(|&o| take(self.get(o)))
            .map(|o| (o, self.get(o).value.clone()))
            .collect()
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
