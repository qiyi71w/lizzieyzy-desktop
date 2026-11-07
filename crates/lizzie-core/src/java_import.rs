//! Reading the fixed Java program's (LizzieYzy Next) configuration for an explicit import.
//!
//! Java keeps its settings in its work directory: `config.txt` (sections `ui`, `leelaz`,
//! `logging`) and `persist` (`filesystem`, `ui-persist`, `autosave`, …), both UTF-8 JSON
//! (`Config.java:1345-1415, 2846-3028`). This module only reads them: it never writes, renames or
//! creates anything in the Java folder, and nothing is shared with Java afterwards.
//!
//! [`read`] turns the Java files into a [`Candidate`]: the values this product can use
//! ([`Mapped`]) and a report entry for every Java setting found — taken over, present but not
//! usable here, or left out (feature not in this product yet, or credentials). Credential values
//! are never copied into report entries. [`Mapped::apply`] produces this product's settings from
//! the current ones; writing them is the workbench's job.
//!
//! Engines keep referring to the engines and models the Java configuration uses; their files are
//! only checked for existence. Relative paths are resolved the way Java launches them
//! (`CommandLaunchHelper.prepare`), against the Java folder and an optional Java program folder.
//!
//! The 存档与读档 slots in `save/` are read by [`slots`] and imported only when the user chooses.

pub mod slots;
mod appearance;
mod matches;

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::api::{
    EngineProblem, ImportEngineView, ImportFailure, ImportItemView, ImportReason, ImportStatus, ImportTarget,
};
use crate::engine::gtp::{join_command, prepare_launch, split_command, FILE_PATH_OPTIONS};
use crate::engine::profile::{new_profile_id, EngineProfile, EngineStartup};
use crate::engine::tuning::{GtpConfiguration, ThreadPolicy};
use crate::sanitize::sanitize;
use crate::settings::{
    Config, DisplaySettings, ExtraMode, HawkeyeSettings, Language, NetworkSettings, Persisted, ProblemSide, ProblemSort,
    ProxyMode, DEFAULT_ANALYSIS_COMMAND, RECENT_LIMIT,
};

pub const CONFIG_FILE: &str = "config.txt";
pub const PERSIST_FILE: &str = "persist";

/// Where to read from: the Java work directory and, optionally, the Java program folder
/// (`CommandLaunchHelper.collectCandidateRoots`: the program location and its ancestors).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaSource {
    pub config_dir: PathBuf,
    pub program_dir: Option<PathBuf>,
}

impl JavaSource {
    /// `path` may be the work directory or its `config.txt`.
    pub fn new(path: &Path, program_dir: Option<&Path>) -> JavaSource {
        let config_dir = if path.file_name().is_some_and(|n| n == CONFIG_FILE) && !path.is_dir() {
            path.parent().unwrap_or(path).to_path_buf()
        } else {
            path.to_path_buf()
        };
        JavaSource {
            config_dir,
            program_dir: program_dir.map(Path::to_path_buf),
        }
    }

    /// Roots for relative engine paths, in Java's order.
    fn roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.config_dir.clone()];
        if let Some(dir) = &self.program_dir {
            for a in dir.ancestors().take(9) {
                if !roots.iter().any(|r| r == a) {
                    roots.push(a.to_path_buf());
                }
            }
        }
        roots
    }
}

/// Java's conventional work directories (`WorkDirectoryResolver`): the shared folders Windows
/// uses, then the per-user folder of every system. Portable and current-directory installs
/// cannot be found this way; the user chooses them.
pub fn conventional_dirs(home: Option<&Path>, public: Option<&Path>, program_data: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = public {
        dirs.push(p.join("Documents").join("LizzieYzyNext"));
        dirs.push(p.join("LizzieYzyNext"));
    }
    if let Some(p) = program_data {
        dirs.push(p.join("LizzieYzyNext"));
    }
    if let Some(h) = home {
        dirs.push(h.join(".lizzieyzy-next"));
        dirs.push(h.join(".lizzieyzy-next-foxuid"));
    }
    dirs
}

/// Settings this product can take over. `None` = not present (or not usable) in the Java files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mapped {
    pub read_komi: Option<bool>,
    pub load_sgf_last: Option<bool>,
    pub show_replace_file_hint: Option<bool>,
    pub append_winrate_to_comment: Option<bool>,
    pub language: Option<Language>,
    pub play_sound: Option<bool>,
    /// 布局模式 (`ui.extra-mode`).
    pub extra_mode: Option<ExtraMode>,
    pub recent_files: Option<Vec<String>>,
    pub last_folder: Option<String>,
    /// Local engines in Java order (remote ones are not included).
    pub engines: Option<Vec<MappedEngine>>,
    pub startup: Option<EngineStartup>,
    /// Java index of the engine used last.
    pub last_engine: Option<usize>,
    /// Java index of the default engine (`Utils.normalizeDefaultEngineFlags`).
    pub default_engine: Option<usize>,
    pub interval_centisec: Option<u32>,
    pub limit_time: Option<bool>,
    pub max_analyze_time_seconds: Option<u32>,
    pub limit_playout: Option<bool>,
    pub limit_playouts: Option<u64>,
    pub limit_max_suggestion: Option<u32>,
    pub limit_branch_length: Option<u32>,
    pub fast_engine_change: Option<bool>,
    pub show_ponder_limited_tips: Option<bool>,
    pub kata_rules: Option<String>,
    pub auto_load_kata_rules: Option<bool>,
    pub startup_benchmark: Option<bool>,
    /// The start-up benchmark the user dismissed (signature, version).
    pub startup_benchmark_dismissed_signature: Option<String>,
    pub startup_benchmark_dismissed_version: Option<u32>,
    /// `ui.network-proxy-*` as Java `NetworkProxy.settings` would accept them.
    pub network: Option<NetworkSettings>,
    pub commentary: Option<crate::commentary::settings::CommentarySettings>,
    pub allow_double_click: Option<bool>,
    pub allow_drag: Option<bool>,
    pub show_right_menu: Option<bool>,
    pub show_new_board_hint: Option<bool>,
    pub show_markup_tools: Option<bool>,
    /// `other-size-width` / `-height` within the board sizes this product opens (2–52).
    pub other_size_width: Option<u16>,
    pub other_size_height: Option<u16>,
    pub no_capture: Option<bool>,
    /// `ui.auto-save-exit`, `ui.resume-previous-game`, `ui.load-save-zoom`.
    pub auto_save_on_exit: Option<bool>,
    pub resume_previous_game: Option<bool>,
    pub zoom_thumbnails: Option<bool>,
    /// `persist:filesystem.last-image-folder`.
    pub last_image_folder: Option<String>,
    /// `ui.win-rate-always-black`, `ui.show-katago-score-lead-with-komi`.
    pub winrate_always_black: Option<bool>,
    pub score_lead_with_komi: Option<bool>,
    /// Board display settings ([`DisplaySettings`] fields other than the two above).
    pub display: Vec<DisplayValue>,
    pub panels: Vec<DisplayValue>,
    pub panel_sizes: Vec<DisplayValue>,
    pub bottom_toolbar: Option<crate::settings::BottomToolbar>,
    pub custom_layouts: [Option<crate::settings::CustomLayout>; 2],
    pub independent_main_board: Option<crate::settings::WindowPlacement>,
    pub independent_sub_board: Option<crate::settings::WindowPlacement>,
    /// `ui.auto-quick-analyze-on-load`.
    pub auto_quick_analyze_on_load: Option<bool>,
    /// 精析设置 (`ui.analysis-*`); a relative command resolved like an engine's.
    pub analysis_command: Option<String>,
    pub analysis_command_customized: Option<bool>,
    pub analysis_max_visits: Option<u32>,
    pub analysis_use_current_rules: Option<bool>,
    pub analysis_specific_rules: Option<String>,
    pub analysis_always_override: Option<bool>,
    pub analysis_preload: Option<bool>,
    pub analysis_auto_quit: Option<bool>,
    pub analysis_reuse_current_engine: Option<bool>,
    /// 批量精析 visits (`ui.batch-analysis-playouts`).
    pub analysis_batch_visits: Option<u32>,
    /// 局部精析's move range (`ui.analysis-start-move`, `analysis-end-move`).
    pub flash_start_move: Option<i32>,
    pub flash_end_move: Option<i32>,
    pub whole_game_visits: Option<u32>,
    /// 强制计算区域 depths (`ui.select-*-moves`).
    pub region_allow_moves: Option<i32>,
    pub region_avoid_moves: Option<i32>,
    pub region_allow_custom_moves: Option<i32>,
    pub region_avoid_custom_moves: Option<i32>,
    /// 选点重点研究 (`ui.tracking-analysis-max-visits`, outline appearance).
    pub tracking_max_visits: Option<u32>,
    pub tracking_show_outline: Option<bool>,
    pub tracking_outline_opacity: Option<u32>,
    /// 快速曲线轻量模型.
    pub quick_model_enabled: Option<bool>,
    pub quick_model_path: Option<String>,
    pub human_sl_training: Option<crate::settings::HumanSlTrainingSettings>,
    pub human_sl_advanced_visible: Option<bool>,
    pub human_sl_model_path: Option<String>,
    pub strength_background: Option<bool>,
    /// 死活题 (`TsumeGoFrame`; a value outside the dialog's choices acts as 继承 / 自动检测 / 无).
    pub tsumego_to_play: Option<crate::tsumego::TsumegoToPlay>,
    pub tsumego_attacker: Option<crate::tsumego::TsumegoAttacker>,
    pub tsumego_ko_threat: Option<crate::tsumego::TsumegoKoThreat>,
    pub tsumego_wall_distance: Option<i32>,
    pub tsumego_show_button: Option<bool>,
    /// 抓取死活题 参数设置.
    pub capture_black_offset: Option<i32>,
    pub capture_black_percent: Option<i32>,
    pub capture_white_offset: Option<i32>,
    pub capture_white_percent: Option<i32>,
    pub capture_gray_offset: Option<i32>,
    /// 问题手 panel.
    pub problems_showing: Option<bool>,
    pub problem_side: Option<ProblemSide>,
    pub problem_sort: Option<ProblemSort>,
    pub problem_winrate_threshold: Option<f64>,
    pub problem_playouts_threshold: Option<u64>,
    pub problem_hide_control_pane: Option<bool>,
    pub auto_analysis: Vec<DisplayValue>,
    /// 人机对弈 settings, including nested `aiTime` and `randomStart` fields.
    pub game: Vec<DisplayValue>,
    /// Bottom toolbar 自动落子 sides and conditions (not its session enable flag).
    pub autoplay: Vec<DisplayValue>,
    pub matches: Vec<DisplayValue>,
    pub match_toolbar: Vec<DisplayValue>,
    /// 超级鹰眼 fields ([`HawkeyeSettings`] camelCase name and value).
    pub hawkeye: Vec<(&'static str, Value)>,
    /// 超级鹰眼's 统计 range as Java keeps it: `move-list-filter-current`, `match-ai-firstmove`,
    /// `match-ai-lastmove`.
    pub hawkeye_range: (Option<bool>, Option<i32>, Option<i32>),
    /// Appearance/default-theme fields, validated against the same types as the live command.
    pub appearance: Vec<(&'static str, Value)>,
    pub theme: Vec<(&'static str, Value)>,
}

/// [`HawkeyeSettings`] with the imported values; the 统计 range selects the period Java shows for
/// it (`setFilterStatus`). 其他 then applies the imported `game-statistics-custom-*` moves, not
/// the saved range.
fn with_hawkeye(current: &HawkeyeSettings, mapped: &Mapped) -> HawkeyeSettings {
    let mut next = serde_json::to_value(current)
        .ok()
        .and_then(|mut json| {
            for (field, value) in &mapped.hawkeye {
                json[*field] = value.clone();
            }
            serde_json::from_value::<HawkeyeSettings>(json).ok()
        })
        .unwrap_or_else(|| current.clone());
    if next.top_page > 2 {
        next.top_page = current.top_page;
    }
    if next.chart > 8 {
        next.chart = current.chart;
    }
    let (filter_current, first, last) = mapped.hawkeye_range;
    if filter_current.is_some() || first.is_some() || last.is_some() {
        let (first, last) = (first.unwrap_or(-1), last.unwrap_or(1000));
        next.period = HawkeyeSettings::period_of(
            filter_current.unwrap_or(false),
            first,
            last,
            next.opening_end_move,
            next.middle_end_move,
        );
    }
    next
}

/// One imported [`DisplaySettings`] value: `field` is its camelCase name, with `.n` for an
/// element of an array field or `.name` for a member of an object field.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayValue {
    pub target: ImportTarget,
    pub field: &'static str,
    pub value: Value,
}

impl DisplayValue {
    fn set(&self, settings: &mut Value) {
        let Some(slot) = (match self.field.split_once('.') {
            Some((name, part)) => settings.get_mut(name).and_then(|s| match part.parse::<usize>() {
                Ok(i) => s.get_mut(i),
                Err(_) => s.get_mut(part),
            }),
            None => settings.get_mut(self.field),
        }) else {
            return;
        };
        *slot = self.value.clone();
    }

    fn current(&self, settings: &Value) -> Option<Value> {
        match self.field.split_once('.') {
            Some((name, part)) => {
                let slot = settings.get(name)?;
                match part.parse::<usize>() {
                    Ok(i) => slot.get(i).cloned(),
                    Err(_) => slot.get(part).cloned(),
                }
            }
            None => settings.get(self.field).cloned(),
        }
    }
}

/// [`DisplaySettings`] with the imported values; a value it does not accept leaves the field.
fn with_display(current: &DisplaySettings, values: &[DisplayValue]) -> DisplaySettings {
    let Ok(mut json) = serde_json::to_value(current) else { return current.clone() };
    for v in values {
        v.set(&mut json);
    }
    serde_json::from_value(json).unwrap_or_else(|_| current.clone())
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappedEngine {
    pub java_index: usize,
    pub profile: EngineProfile,
    /// The command or initial commands hold credentials: imported only when confirmed.
    pub credential: bool,
}

/// The Java configuration as read, before anything is written.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub source: JavaSource,
    pub config_file: PathBuf,
    pub persist_file: Option<PathBuf>,
    pub mapped: Mapped,
    pub items: Vec<ImportItemView>,
    /// Every Java engine entry; an entry with `credential` is imported only when confirmed.
    pub engines: Vec<ImportEngineView>,
    /// Java 存档与读档 slots ([`crate::java_import::slots`]); imported only when chosen.
    pub slots: Vec<slots::JavaSlot>,
    /// Non-secret API-key choice; the adapter reads its secret only after explicit consent.
    pub commentary_credential: Option<crate::api::ImportCommentaryCredential>,
}

impl Candidate {
    /// Adds the OS-store credential choice after the adapter's read-only availability lookup.
    pub fn set_commentary_credential(&mut self, source: &str) {
        if self.commentary_credential.is_some() { return; }
        let Some(settings) = &self.mapped.commentary else { return; };
        self.commentary_credential = Some(crate::api::ImportCommentaryCredential {
            source: source.into(), base_url: settings.base_url.clone(),
        });
        self.items.push(ImportItemView {
            key: "credential:commentary-api-key".into(), status: ImportStatus::NotMigrated,
            reason: Some(ImportReason::CredentialNotConfirmed), target: Some(ImportTarget::Commentary),
            value: None, detail: None, credential: true,
        });
    }
    /// The engines as the report shows them once the user chose which credentials to copy.
    pub fn engines_after(&self, copy_credentials: &[u32]) -> Vec<ImportEngineView> {
        self.engines
            .iter()
            .map(|e| {
                let mut e = e.clone();
                if e.status == ImportStatus::Imported && e.credential && !copy_credentials.contains(&e.index) {
                    e.status = ImportStatus::NotMigrated;
                    e.reason = Some(ImportReason::CredentialNotConfirmed);
                }
                e
            })
            .collect()
    }

    /// The setting entries as the report shows them once the user chose which credentials to
    /// copy: the default and last engine choices of an engine that is left out are left out too.
    pub fn items_after(&self, copy_credentials: &[u32]) -> Vec<ImportItemView> {
        let engines = self.engines_after(copy_credentials);
        self.items
            .iter()
            .map(|i| {
                let mut i = i.clone();
                let index = match i.key.as_str() {
                    "ui.default-engine" => self.mapped.default_engine,
                    "ui.last-engine" => self.mapped.last_engine,
                    _ => None,
                };
                let left_out = index
                    .and_then(|k| engines.get(k))
                    .filter(|e| e.status != ImportStatus::Imported);
                if let (ImportStatus::Imported, Some(engine)) = (i.status, left_out) {
                    i.status = ImportStatus::NotMigrated;
                    i.reason = engine.reason;
                    i.detail = Some(engine.name.clone());
                }
                i
            })
            .collect()
    }

    /// Indexes of the engines whose credentials could be copied (all of them, for a preview that
    /// shows what confirming would import).
    pub fn credential_engines(&self) -> Vec<u32> {
        self.engines
            .iter()
            .filter(|e| e.credential && e.status == ImportStatus::Imported)
            .map(|e| e.index)
            .collect()
    }
}

/// Reads the Java configuration. Only `config.txt` is required; an unreadable `persist` is
/// reported and its settings are left out.
pub fn read(source: &JavaSource) -> Result<Candidate, ImportFailure> {
    let config_file = source.config_dir.join(CONFIG_FILE);
    let shown_path = config_file.to_string_lossy().into_owned();
    if !config_file.is_file() {
        return Err(ImportFailure::NoConfig { path: shown_path });
    }
    let config = match read_json(&config_file) {
        Ok(Value::Object(m)) => m,
        Ok(_) => {
            return Err(ImportFailure::Invalid {
                path: shown_path,
                detail: "not a JSON object".into(),
            })
        }
        Err(JsonError::Io(detail)) => {
            return Err(ImportFailure::Unreadable {
                path: shown_path,
                detail,
            })
        }
        Err(JsonError::Syntax(detail)) => {
            return Err(ImportFailure::Invalid {
                path: shown_path,
                detail,
            })
        }
    };
    let mut r = Reader {
        source,
        mapped: Mapped::default(),
        items: Vec::new(),
        engines: Vec::new(),
    };
    r.config(&config);

    let persist_path = source.config_dir.join(PERSIST_FILE);
    let persist_file = persist_path.is_file().then(|| persist_path.clone());
    if persist_file.is_some() {
        match read_json(&persist_path) {
            Ok(Value::Object(m)) => r.persist(&m),
            Ok(_) => r.unreadable("persist:", "not a JSON object".into()),
            Err(JsonError::Io(d) | JsonError::Syntax(d)) => r.unreadable("persist:", d),
        }
    }
    r.other_files();
    let legacy_key = r.teacher_settings();
    let slots = slots::read(source, &mut r);
    let mut candidate = Candidate {
        source: source.clone(),
        config_file,
        persist_file,
        mapped: r.mapped,
        items: r.items,
        engines: r.engines,
        slots,
        commentary_credential: None,
    };
    if legacy_key { candidate.set_commentary_credential("teacher.properties"); }
    Ok(candidate)
}

enum JsonError {
    Io(String),
    Syntax(String),
}

/// UTF-8 JSON with an optional byte order mark (`Config.java:1352-1355`).
fn read_json(path: &Path) -> Result<Value, JsonError> {
    let bytes = std::fs::read(path).map_err(|e| JsonError::Io(e.to_string()))?;
    let text = String::from_utf8(bytes).map_err(|e| JsonError::Io(e.to_string()))?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| JsonError::Syntax(e.to_string()))
}

// ---------------------------------------------------------------- org.json `opt*` coercions

/// `JSONObject.optBoolean`: a boolean or the strings "true" / "false" in any case.
fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) if s.eq_ignore_ascii_case("true") => Some(true),
        Value::String(s) if s.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

/// `JSONObject.optInt` / `optLong`: a number (fractions truncated) or a numeric string.
fn as_i64(v: &Value) -> Option<i64> {
    let truncate = |f: f64| f.is_finite().then(|| f.trunc() as i64);
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().and_then(truncate)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>()
                .ok()
                .or_else(|| s.parse::<f64>().ok().and_then(truncate))
        }
        _ => None,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|f| f.is_finite())
}

/// Shown value of a JSON scalar.
fn shown(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Keys whose value is a secret or an account (Java's credential settings).
fn is_credential_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "token",
        "secret",
        "api-key",
        "apikey",
        "credential",
        "remote-code",
        "identifier",
    ]
    .iter()
    .any(|w| k.contains(w))
        || matches!(
            k.as_str(),
            "up-load-user" | "contribute-user-name" | "username" | "keygenpath"
        )
}

/// A JSON value that holds a secret anywhere inside it.
fn holds_credential(key: &str, v: &Value) -> bool {
    is_credential_key(key)
        || match v {
            Value::Object(m) => m.iter().any(|(k, v)| holds_credential(k, v)),
            Value::Array(a) => a.iter().any(|v| holds_credential("", v)),
            _ => false,
        }
}

struct Reader<'a> {
    source: &'a JavaSource,
    mapped: Mapped,
    items: Vec<ImportItemView>,
    engines: Vec<ImportEngineView>,
}

/// How one mapped key is read.
enum Kind {
    Bool(fn(&mut Mapped, bool)),
    /// Non-negative integer up to `u32::MAX`.
    U32(fn(&mut Mapped, u32)),
    U64(fn(&mut Mapped, u64)),
    /// Any `int` (`optInt`).
    I32(fn(&mut Mapped, i32)),
    Str(fn(&mut Mapped, String)),
    /// A finite number of at least 0.
    NonNegative(fn(&mut Mapped, f64)),
}

/// `config.txt` keys mapped one to one: section, key, target, reader. Sections and defaults:
/// `Config.java:1774-2156`.
const SIMPLE: &[(&str, &str, ImportTarget, Kind)] = &[
    (
        "ui",
        "read-komi",
        ImportTarget::ReadKomi,
        Kind::Bool(|m, v| m.read_komi = Some(v)),
    ),
    (
        "ui",
        "load-sgf-last",
        ImportTarget::LoadSgfLast,
        Kind::Bool(|m, v| m.load_sgf_last = Some(v)),
    ),
    (
        "ui",
        "show-replace-file-hint",
        ImportTarget::ReplaceFileHint,
        Kind::Bool(|m, v| m.show_replace_file_hint = Some(v)),
    ),
    (
        "ui",
        "append-winrate-to-comment",
        ImportTarget::AppendWinrateToComment,
        Kind::Bool(|m, v| m.append_winrate_to_comment = Some(v)),
    ),
    (
        "ui",
        "allow-double-click",
        ImportTarget::AllowDoubleClick,
        Kind::Bool(|m, v| m.allow_double_click = Some(v)),
    ),
    (
        "ui",
        "allow-drag",
        ImportTarget::AllowDrag,
        Kind::Bool(|m, v| m.allow_drag = Some(v)),
    ),
    (
        "ui",
        "show-right-menu",
        ImportTarget::RightClickMenu,
        Kind::Bool(|m, v| m.show_right_menu = Some(v)),
    ),
    (
        "ui",
        "show-new-board-hint",
        ImportTarget::NewBoardHint,
        Kind::Bool(|m, v| m.show_new_board_hint = Some(v)),
    ),
    (
        "ui",
        "show-markup-tools",
        ImportTarget::MarkupTools,
        Kind::Bool(|m, v| m.show_markup_tools = Some(v)),
    ),
    (
        "ui",
        "other-size-width",
        ImportTarget::OtherBoardSize,
        Kind::U32(|m, v| m.other_size_width = u16::try_from(v).ok().filter(|v| (2..=52).contains(v))),
    ),
    (
        "ui",
        "other-size-height",
        ImportTarget::OtherBoardSize,
        Kind::U32(|m, v| m.other_size_height = u16::try_from(v).ok().filter(|v| (2..=52).contains(v))),
    ),
    (
        "ui",
        "no-capture",
        ImportTarget::NoCapture,
        Kind::Bool(|m, v| m.no_capture = Some(v)),
    ),
    (
        "ui",
        "win-rate-always-black",
        ImportTarget::WinrateAlwaysBlack,
        Kind::Bool(|m, v| m.winrate_always_black = Some(v)),
    ),
    (
        "ui",
        "show-katago-score-lead-with-komi",
        ImportTarget::ScoreLeadWithKomi,
        Kind::Bool(|m, v| m.score_lead_with_komi = Some(v)),
    ),
    (
        "ui",
        "play-sound",
        ImportTarget::PlaySound,
        Kind::Bool(|m, v| m.play_sound = Some(v)),
    ),
    (
        "ui",
        "limit-time",
        ImportTarget::LimitTime,
        Kind::Bool(|m, v| m.limit_time = Some(v)),
    ),
    (
        "ui",
        "limit-playout",
        ImportTarget::LimitPlayout,
        Kind::Bool(|m, v| m.limit_playout = Some(v)),
    ),
    (
        "ui",
        "limit-playouts",
        ImportTarget::LimitPlayouts,
        Kind::U64(|m, v| m.limit_playouts = Some(v)),
    ),
    (
        "ui",
        "show-ponder-limited-tips",
        ImportTarget::PonderLimitTips,
        Kind::Bool(|m, v| m.show_ponder_limited_tips = Some(v)),
    ),
    (
        "ui",
        "kata-rules",
        ImportTarget::KataRules,
        Kind::Str(|m, v| m.kata_rules = Some(v)),
    ),
    (
        "ui",
        "auto-load-kata-rules",
        ImportTarget::AutoLoadKataRules,
        Kind::Bool(|m, v| m.auto_load_kata_rules = Some(v)),
    ),
    (
        "ui",
        "enable-startup-benchmark",
        ImportTarget::StartupBenchmark,
        Kind::Bool(|m, v| m.startup_benchmark = Some(v)),
    ),
    (
        "ui",
        "katago-startup-benchmark-dismissed-signature",
        ImportTarget::StartupBenchmark,
        Kind::Str(|m, v| m.startup_benchmark_dismissed_signature = Some(v)),
    ),
    (
        "ui",
        "katago-startup-benchmark-dismissed-version",
        ImportTarget::StartupBenchmark,
        Kind::U32(|m, v| m.startup_benchmark_dismissed_version = Some(v)),
    ),
    (
        "leelaz",
        "max-analyze-time-seconds",
        ImportTarget::MaxAnalyzeTime,
        Kind::U32(|m, v| m.max_analyze_time_seconds = Some(v)),
    ),
    (
        "leelaz",
        "limit-max-suggestion",
        ImportTarget::LimitMaxSuggestion,
        Kind::U32(|m, v| m.limit_max_suggestion = Some(v)),
    ),
    (
        "leelaz",
        "limit-branch-length",
        ImportTarget::LimitBranchLength,
        Kind::U32(|m, v| m.limit_branch_length = Some(v)),
    ),
    (
        "leelaz",
        "analyze-update-interval-centisec",
        ImportTarget::AnalysisInterval,
        Kind::U32(|m, v| m.interval_centisec = Some(v)),
    ),
    (
        "leelaz",
        "fast-engine-change",
        ImportTarget::FastEngineChange,
        Kind::Bool(|m, v| m.fast_engine_change = Some(v)),
    ),
    (
        "ui",
        "auto-save-exit",
        ImportTarget::AutoSaveOnExit,
        Kind::Bool(|m, v| m.auto_save_on_exit = Some(v)),
    ),
    (
        "ui",
        "resume-previous-game",
        ImportTarget::ResumePreviousGame,
        Kind::Bool(|m, v| m.resume_previous_game = Some(v)),
    ),
    (
        "ui",
        "load-save-zoom",
        ImportTarget::ZoomThumbnails,
        Kind::Bool(|m, v| m.zoom_thumbnails = Some(v)),
    ),
    // `Config.java:1857-1866`, `AnalysisSettings.saveConfig`.
    (
        "ui",
        "auto-quick-analyze-on-load",
        ImportTarget::AutoQuickAnalysis,
        Kind::Bool(|m, v| m.auto_quick_analyze_on_load = Some(v)),
    ),
    (
        "ui",
        "analysis-engine-command",
        ImportTarget::AnalysisEngine,
        Kind::Str(|m, v| m.analysis_command = Some(v)),
    ),
    (
        "ui",
        "analysis-engine-command-customized",
        ImportTarget::AnalysisEngine,
        Kind::Bool(|m, v| m.analysis_command_customized = Some(v)),
    ),
    (
        "ui",
        "analysis-max-visits",
        ImportTarget::AnalysisEngine,
        Kind::U32(|m, v| m.analysis_max_visits = Some(v.max(1))),
    ),
    (
        "ui",
        "batch-analysis-playouts",
        ImportTarget::AnalysisEngine,
        Kind::U32(|m, v| m.analysis_batch_visits = Some(v)),
    ),
    (
        "ui",
        "analysis-use-current-rules",
        ImportTarget::AnalysisEngine,
        Kind::Bool(|m, v| m.analysis_use_current_rules = Some(v)),
    ),
    (
        "ui",
        "analysis-specific-rules",
        ImportTarget::AnalysisEngine,
        Kind::Str(|m, v| m.analysis_specific_rules = Some(v)),
    ),
    (
        "ui",
        "analysis-always-override",
        ImportTarget::AnalysisEngine,
        Kind::Bool(|m, v| m.analysis_always_override = Some(v)),
    ),
    (
        "ui",
        "analysis-engine-preload",
        ImportTarget::AnalysisEngine,
        Kind::Bool(|m, v| m.analysis_preload = Some(v)),
    ),
    (
        "ui",
        "analysis-auto-quit",
        ImportTarget::AnalysisEngine,
        Kind::Bool(|m, v| m.analysis_auto_quit = Some(v)),
    ),
    (
        "ui",
        "analysis-reuse-current-engine",
        ImportTarget::AnalysisEngine,
        Kind::Bool(|m, v| m.analysis_reuse_current_engine = Some(v)),
    ),
    // `AnalysisPartGame`'s range (`Config.java:1855-1856`).
    (
        "ui",
        "analysis-start-move",
        ImportTarget::FlashRange,
        Kind::I32(|m, v| m.flash_start_move = Some(v)),
    ),
    (
        "ui",
        "analysis-end-move",
        ImportTarget::FlashRange,
        Kind::I32(|m, v| m.flash_end_move = Some(v)),
    ),
    (
        "ui",
        "whole-game-analysis-deep-visits",
        ImportTarget::WholeGameVisits,
        Kind::I32(|m, v| m.whole_game_visits = Some(crate::settings::WholeGameSettings::from_stored(i64::from(v)).deep_visits)),
    ),
    // 强制计算区域's depth choices (`Config.java:1879-1882`). Java reads the avoid custom depth
    // from `select-allow-custom-moves`; its own key is read here.
    ("ui", "select-allow-moves", ImportTarget::AnalysisRegion, Kind::I32(|m, v| m.region_allow_moves = Some(v))),
    ("ui", "select-avoid-moves", ImportTarget::AnalysisRegion, Kind::I32(|m, v| m.region_avoid_moves = Some(v))),
    (
        "ui",
        "select-allow-custom-moves",
        ImportTarget::AnalysisRegion,
        Kind::I32(|m, v| m.region_allow_custom_moves = Some(v)),
    ),
    (
        "ui",
        "select-avoid-custom-moves",
        ImportTarget::AnalysisRegion,
        Kind::I32(|m, v| m.region_avoid_custom_moves = Some(v)),
    ),
    // 选点重点研究 (`Config.migrateTrackingAnalysisConfig`, `loadTrackingPointAppearanceConfig`): the
    // older key first, so the current one wins; 0 or less is the default 500.
    (
        "ui",
        "tracking-engine-max-visits",
        ImportTarget::Tracking,
        Kind::I32(|m, v| m.tracking_max_visits = Some(if v <= 0 { 500 } else { v as u32 })),
    ),
    (
        "ui",
        "tracking-analysis-max-visits",
        ImportTarget::Tracking,
        Kind::I32(|m, v| m.tracking_max_visits = Some(if v <= 0 { 500 } else { v as u32 })),
    ),
    ("ui", "show-tracking-point-outline", ImportTarget::Tracking, Kind::Bool(|m, v| m.tracking_show_outline = Some(v))),
    (
        "ui",
        "tracking-point-outline-opacity",
        ImportTarget::Tracking,
        Kind::I32(|m, v| m.tracking_outline_opacity = Some(v.clamp(0, 100) as u32)),
    ),
    (
        "ui",
        "quick-analysis-lightweight-model-enabled",
        ImportTarget::QuickModel,
        Kind::Bool(|m, v| m.quick_model_enabled = Some(v)),
    ),
    (
        "ui",
        "katago-quick-analysis-model-path",
        ImportTarget::QuickModel,
        Kind::Str(|m, v| m.quick_model_path = Some(v)),
    ),
    (
        "ui",
        "katago-human-sl-model-path",
        ImportTarget::HumanSlModel,
        Kind::Str(|m, v| m.human_sl_model_path = Some(v)),
    ),
    (
        "ui",
        "strength-humansl-background-enabled",
        ImportTarget::StrengthBackground,
        Kind::Bool(|m, v| m.strength_background = Some(v)),
    ),
    // 死活题 (`Config.java:1819, 2105-2114`). An unknown 先手方 or 进攻方 selects no radio button and
    // builds like 继承 / 自动检测; an unknown 劫材 value adds no ko threats.
    (
        "ui",
        "tsume-go-to-play",
        ImportTarget::Tsumego,
        Kind::I32(|m, v| {
            use crate::tsumego::TsumegoToPlay as P;
            m.tsumego_to_play = Some(match v { 2 => P::Black, 3 => P::White, _ => P::Keep });
        }),
    ),
    (
        "ui",
        "tsume-go-attaker",
        ImportTarget::Tsumego,
        Kind::I32(|m, v| {
            use crate::tsumego::TsumegoAttacker as A;
            m.tsumego_attacker = Some(match v { 2 => A::Black, 3 => A::White, _ => A::Auto });
        }),
    ),
    (
        "ui",
        "tsume-go-ko-threat",
        ImportTarget::Tsumego,
        Kind::I32(|m, v| {
            use crate::tsumego::TsumegoKoThreat as K;
            m.tsumego_ko_threat = Some(match v { 1 => K::Both, 2 => K::Attacker, 3 => K::Defender, _ => K::None });
        }),
    ),
    ("ui", "tsume-go-wall-distance", ImportTarget::Tsumego, Kind::I32(|m, v| m.tsumego_wall_distance = Some(v))),
    ("ui", "show-tsume-go-menu", ImportTarget::Tsumego, Kind::Bool(|m, v| m.tsumego_show_button = Some(v))),
    ("ui", "capture-black-offset", ImportTarget::CaptureTsumego, Kind::I32(|m, v| m.capture_black_offset = Some(v))),
    ("ui", "capture-black-percent", ImportTarget::CaptureTsumego, Kind::I32(|m, v| m.capture_black_percent = Some(v))),
    ("ui", "capture-white-offset", ImportTarget::CaptureTsumego, Kind::I32(|m, v| m.capture_white_offset = Some(v))),
    ("ui", "capture-white-percent", ImportTarget::CaptureTsumego, Kind::I32(|m, v| m.capture_white_percent = Some(v))),
    // Java keeps the gray offset under this name.
    ("ui", "capture-gray-percent", ImportTarget::CaptureTsumego, Kind::I32(|m, v| m.capture_gray_offset = Some(v))),
    // `SidebarPanel`, `ProblemListSideFilter.fromConfigValue`, `ProblemListSort`.
    (
        "ui",
        "is-showing-blunder-table",
        ImportTarget::ProblemList,
        Kind::Bool(|m, v| m.problems_showing = Some(v)),
    ),
    (
        "ui",
        "problem-list-side-filter",
        ImportTarget::ProblemList,
        Kind::Str(|m, v| {
            m.problem_side = Some(if v.trim().eq_ignore_ascii_case("white") {
                ProblemSide::White
            } else {
                ProblemSide::Black
            })
        }),
    ),
    (
        "ui",
        "problem-list-sort",
        ImportTarget::ProblemList,
        Kind::Str(|m, v| {
            m.problem_sort = Some(match v.trim().to_ascii_lowercase().as_str() {
                "move-asc" => ProblemSort::MoveAsc,
                "move-desc" => ProblemSort::MoveDesc,
                _ => ProblemSort::LossDesc,
            })
        }),
    ),
    (
        "ui",
        "problem-list-winrate-threshold",
        ImportTarget::ProblemList,
        Kind::NonNegative(|m, v| m.problem_winrate_threshold = Some(v)),
    ),
    (
        "ui",
        "blunder-playouts-threshold",
        ImportTarget::ProblemList,
        Kind::U64(|m, v| m.problem_playouts_threshold = Some(v)),
    ),
    (
        "ui",
        "hide-blunder-table-control-pane",
        ImportTarget::ProblemList,
        Kind::Bool(|m, v| m.problem_hide_control_pane = Some(v)),
    ),
];

enum DisplayKind {
    Bool,
    /// An integer within the bounds.
    Int(i64, i64),
    Num,
    /// A number of at least 0.
    NonNegative,
}

/// Board display keys mapped one to one: Java key (`ui`), target, [`DisplaySettings`] field,
/// reader (`Config.java:1600-2130`).
const DISPLAY: &[(&str, ImportTarget, &str, DisplayKind)] = &[
    ("show-coordinates", ImportTarget::Coordinates, "coordinates", DisplayKind::Bool),
    ("show-name-in-board", ImportTarget::NamesOnBoard, "namesOnBoard", DisplayKind::Bool),
    ("movenumber-from-one", ImportTarget::MoveNumbers, "moveNumbersFromOne", DisplayKind::Bool),
    ("new-move-number-in-branch", ImportTarget::MoveNumbers, "branchNumbersFromOne", DisplayKind::Bool),
    ("show-moveall-inbranch", ImportTarget::MoveNumbers, "allNumbersInBranch", DisplayKind::Bool),
    ("show-var-move", ImportTarget::MoveNumbers, "treeMoveNumbers", DisplayKind::Bool),
    ("move-rank-mark-last-move", ImportTarget::MoveRank, "moveRank", DisplayKind::Int(-1, i32::MAX as i64)),
    ("txt-move-rank-mark-last-move", ImportTarget::MoveRank, "moveRankCustom", DisplayKind::Int(1, i32::MAX as i64)),
    ("win-loss-threshold-1", ImportTarget::MoveRank, "winLossThresholds.0", DisplayKind::Num),
    ("win-loss-threshold-2", ImportTarget::MoveRank, "winLossThresholds.1", DisplayKind::Num),
    ("win-loss-threshold-3", ImportTarget::MoveRank, "winLossThresholds.2", DisplayKind::Num),
    ("win-loss-threshold-4", ImportTarget::MoveRank, "winLossThresholds.3", DisplayKind::Num),
    ("win-loss-threshold-5", ImportTarget::MoveRank, "winLossThresholds.4", DisplayKind::Num),
    ("score-loss-threshold-1", ImportTarget::MoveRank, "scoreLossThresholds.0", DisplayKind::Num),
    ("score-loss-threshold-2", ImportTarget::MoveRank, "scoreLossThresholds.1", DisplayKind::Num),
    ("score-loss-threshold-3", ImportTarget::MoveRank, "scoreLossThresholds.2", DisplayKind::Num),
    ("score-loss-threshold-4", ImportTarget::MoveRank, "scoreLossThresholds.3", DisplayKind::Num),
    ("score-loss-threshold-5", ImportTarget::MoveRank, "scoreLossThresholds.4", DisplayKind::Num),
    ("show-winrate-in-suggestion", ImportTarget::Suggestions, "suggestionWinrate", DisplayKind::Bool),
    ("show-playouts-in-suggestion", ImportTarget::Suggestions, "suggestionPlayouts", DisplayKind::Bool),
    ("show-scoremean-in-suggestion", ImportTarget::Suggestions, "suggestionScore", DisplayKind::Bool),
    ("show-score-as-diff", ImportTarget::Suggestions, "scoreAsDiff", DisplayKind::Bool),
    ("suggestion-info-winrate", ImportTarget::Suggestions, "suggestionInfoOrder.0", DisplayKind::Int(1, 3)),
    ("suggestion-info-playouts", ImportTarget::Suggestions, "suggestionInfoOrder.1", DisplayKind::Int(1, 3)),
    ("suggestion-info-scorelead", ImportTarget::Suggestions, "suggestionInfoOrder.2", DisplayKind::Int(1, 3)),
    ("delay-show-candidates", ImportTarget::Suggestions, "delayCandidates", DisplayKind::Bool),
    ("delay-candidates-seconds", ImportTarget::Suggestions, "delayCandidatesSeconds", DisplayKind::Num),
    ("show-suggestion-order", ImportTarget::Suggestions, "suggestionOrder", DisplayKind::Bool),
    ("show-suggestion-maxred", ImportTarget::Suggestions, "suggestionMaxRed", DisplayKind::Bool),
    ("white-suggestion-white", ImportTarget::Suggestions, "whiteSuggestionWhite", DisplayKind::Bool),
    ("show-blue-ring", ImportTarget::Suggestions, "blueRing", DisplayKind::Bool),
    ("suggestion-color-ratio", ImportTarget::Suggestions, "suggestionColorRatio", DisplayKind::Int(1, 3)),
    ("white-suggestion-order-white", ImportTarget::Suggestions, "whiteSuggestionOrderWhite", DisplayKind::Bool),
    ("pv-visits-limit", ImportTarget::Suggestions, "pvVisitsLimit", DisplayKind::Int(i32::MIN as i64, i32::MAX as i64)),
    ("remove-dead-in-variation", ImportTarget::Suggestions, "removeDeadInVariation", DisplayKind::Bool),
    ("auto-replay-branch", ImportTarget::Suggestions, "autoReplayBranch", DisplayKind::Bool),
    ("auto-replay-display-entire-variations-first", ImportTarget::Suggestions, "entireVariationFirst", DisplayKind::Bool),
    // `optDouble`: LizzieFrame.autoReplayBranch waits that long, so 0 or less shows no wait.
    ("display-entire-variations-first-seconds", ImportTarget::Suggestions, "entireVariationFirstSeconds", DisplayKind::Num),
    ("continue-with-best-move", ImportTarget::Suggestions, "continueWithBestMove", DisplayKind::Bool),
    ("directly-with-best-move", ImportTarget::Suggestions, "directlyWithBestMove", DisplayKind::Bool),
    ("show-suggestion-variations", ImportTarget::Suggestions, "suggestionVariations", DisplayKind::Bool),
    ("norefresh-onmouse-move", ImportTarget::Suggestions, "noRefreshOnMouse", DisplayKind::Bool),
    ("replay-branch-interval-seconds", ImportTarget::Suggestions, "replayIntervalSeconds", DisplayKind::Num),
    ("min-playout-ratio-for-stats", ImportTarget::Suggestions, "minPlayoutRatio", DisplayKind::Num),
    ("show-next-moves", ImportTarget::NextMoves, "nextMoves", DisplayKind::Bool),
    ("show-next-move-blunder", ImportTarget::NextMoves, "nextMoveInfo", DisplayKind::Bool),
    ("min-playouts-for-next-move", ImportTarget::NextMoves, "nextMoveMinPlayouts", DisplayKind::Int(i32::MIN as i64, i32::MAX as i64)),
    ("no-refresh-on-sub", ImportTarget::SubBoard, "noRefreshOnSub", DisplayKind::Bool),
    ("show-win-rate-line", ImportTarget::WinrateGraph, "winrateLine", DisplayKind::Bool),
    ("show-score-lead-line", ImportTarget::WinrateGraph, "scoreLeadLine", DisplayKind::Bool),
    ("show-winrate-graph-fill", ImportTarget::WinrateGraph, "winrateGraphFill", DisplayKind::Bool),
    ("show-mouse-over-winrate-graph", ImportTarget::WinrateGraph, "mouseOverWinrateGraph", DisplayKind::Bool),
    ("initial-max-score-lead", ImportTarget::WinrateGraph, "initialMaxScoreLead", DisplayKind::NonNegative),
    ("use-moves-ownership", ImportTarget::KataEstimate, "useMovesOwnership", DisplayKind::Bool),
    ("shortcut-kata-estimate", ImportTarget::KataEstimate, "kataEstimateShortcut", DisplayKind::Bool),
    ("save-kata-estimate-status", ImportTarget::KataEstimate, "kataEstimateSaveStatus", DisplayKind::Bool),
    ("show-katago-estimate", ImportTarget::KataEstimate, "kataEstimate.shown", DisplayKind::Bool),
    ("show-katago-estimate-onmainboard", ImportTarget::KataEstimate, "kataEstimate.onMain", DisplayKind::Bool),
    ("show-katago-estimate-onsubbord", ImportTarget::KataEstimate, "kataEstimate.onSub", DisplayKind::Bool),
    ("use-territory-in-score", ImportTarget::Scoring, "territoryScoring", DisplayKind::Bool),
    ("match-ai-moves", ImportTarget::MatchAi, "matchAiMoves", DisplayKind::Int(0, u32::MAX as i64)),
    ("match-ai-percents-playouts", ImportTarget::MatchAi, "matchAiPercentsPlayouts", DisplayKind::Num),
    ("show-suggestions-frame", ImportTarget::SuggestionsFrame, "suggestionsFrame", DisplayKind::Bool),
    ("suggestions-always-ontop", ImportTarget::SuggestionsFrame, "suggestionsOnTop", DisplayKind::Bool),
    ("show-bestmoves-list", ImportTarget::SuggestionsFrame, "bestMovesList", DisplayKind::Bool),
    ("show-bestmoves-graph", ImportTarget::SuggestionsFrame, "bestMovesGraph", DisplayKind::Bool),
    ("anaframe-show-next", ImportTarget::SuggestionsFrame, "frameShowNext", DisplayKind::Bool),
    ("anaframe-use-mousemove", ImportTarget::SuggestionsFrame, "frameMouseMove", DisplayKind::Bool),
];

/// Human-engine game controls (`Config.java:1668-2143`, `loadNewGameDialogSettings`).
/// Java stores the human clock choices as combo indices, not minutes/seconds.
const GAME: &[(&str, &str, ImportTarget, &str, DisplayKind)] = &[
    ("ui", "check-play-black", ImportTarget::HumanEngineGame, "playBlack", DisplayKind::Bool),
    ("ui", "check-continue-play", ImportTarget::HumanEngineGame, "continuePlay", DisplayKind::Bool),
    ("ui", "use-free-handicap", ImportTarget::HumanEngineGame, "freeHandicap", DisplayKind::Bool),
    ("ui", "new-game-komi", ImportTarget::HumanEngineGame, "komi", DisplayKind::Num),
    ("ui", "limit-my-time", ImportTarget::HumanEngineGame, "limitMyTime", DisplayKind::Bool),
    ("ui", "my-save-time", ImportTarget::HumanEngineGame, "mySaveTime", DisplayKind::Int(0, 7)),
    ("ui", "my-byoyomo-seconds", ImportTarget::HumanEngineGame, "myByoyomiSeconds", DisplayKind::Int(0, 7)),
    ("ui", "my-byoyomo-times", ImportTarget::HumanEngineGame, "myByoyomiTimes", DisplayKind::Int(0, 5)),
    ("ui", "kata-time-main-time-mins", ImportTarget::GameAiTime, "aiTime.kataMainMinutes", DisplayKind::Int(0, i32::MAX as i64)),
    ("ui", "kata-time-byoyomi-secs", ImportTarget::GameAiTime, "aiTime.kataByoyomiSeconds", DisplayKind::Int(0, i32::MAX as i64)),
    ("ui", "kata-time-byoyomi-times", ImportTarget::GameAiTime, "aiTime.kataByoyomiTimes", DisplayKind::Int(0, i32::MAX as i64)),
    ("ui", "kata-time-fisher-increment-secs", ImportTarget::GameAiTime, "aiTime.kataFischerIncrement", DisplayKind::Int(0, i32::MAX as i64)),
    ("ui", "kata-visits-playouts-settings", ImportTarget::HumanEngineGame, "kataLimits", DisplayKind::Bool),
    ("ui", "kata-visits-txt", ImportTarget::HumanEngineGame, "kataVisits", DisplayKind::Int(i32::MIN as i64, i32::MAX as i64)),
    ("ui", "kata-playouts-txt", ImportTarget::HumanEngineGame, "kataPlayouts", DisplayKind::Int(i32::MIN as i64, i32::MAX as i64)),
    ("ui", "use-play-mode", ImportTarget::HumanEngineGame, "usePlayMode", DisplayKind::Bool),
    ("ui", "new-game-show-black", ImportTarget::HumanEngineGame, "showBlack", DisplayKind::Bool),
    ("ui", "new-game-show-white", ImportTarget::HumanEngineGame, "showWhite", DisplayKind::Bool),
    ("ui", "auto-save-played-game", ImportTarget::HumanEngineGame, "autoSave", DisplayKind::Bool),
    ("ui", "use-pure-net-in-game", ImportTarget::HumanEngineGame, "pureNet", DisplayKind::Bool),
    ("ui", "anagame-resign-start-move", ImportTarget::HumanEngineGame, "resignStartMove", DisplayKind::Int(0, i32::MAX as i64)),
    ("ui", "anagame-resign-move", ImportTarget::HumanEngineGame, "resignMove", DisplayKind::Int(0, i32::MAX as i64)),
    ("ui", "anagame-resign-percent", ImportTarget::HumanEngineGame, "resignPercent", DisplayKind::Num),
    ("ui", "disable-wrn-in-game", ImportTarget::HumanEngineGame, "disableWrn", DisplayKind::Bool),
    ("ui", "enable-anagame-randomstart", ImportTarget::GameRandomStart, "randomStart.enabled", DisplayKind::Bool),
    ("ui", "anagame-random-move", ImportTarget::GameRandomStart, "randomStart.moves", DisplayKind::Int(0, i32::MAX as i64)),
    ("ui", "anagame-random-winratediff", ImportTarget::GameRandomStart, "randomStart.winrateDiff", DisplayKind::Num),
    ("ui", "anagame-random-playoutdiff", ImportTarget::GameRandomStart, "randomStart.playoutsDiff", DisplayKind::Num),
    ("leelaz", "play-ponder", ImportTarget::HumanEngineGame, "playPonder", DisplayKind::Bool),
    ("leelaz", "max-game-thinking-time-seconds", ImportTarget::GameAiTime, "aiTime.fixedSeconds", DisplayKind::Int(0, i32::MAX as i64)),
];

/// Keys read together with the engine list (`Utils.getEngineData` / `getEngineDataOld`,
/// `Lizzie.java:457-488`).
const ENGINE_UI_KEYS: [&str; 5] = [
    "default-engine",
    "last-engine",
    "autoload-default",
    "autoload-last",
    "autoload-empty",
];
const LEGACY_ENGINE_KEYS: [&str; 7] = [
    "engine-command",
    "engine-command-list",
    "engine-name-list",
    "engine-preload-list",
    "engine-width-list",
    "engine-height-list",
    "engine-komi-list",
];

type Handled = Vec<(&'static str, &'static str)>;

impl Reader<'_> {
    fn auto_analysis_value(&mut self, name: String, field: &'static str, value: Option<Value>, raw: &Value) {
        if let Some(value) = value {
            self.imported(name, ImportTarget::AutoAnalysis, shown(&value));
            self.mapped.auto_analysis.push(DisplayValue { target: ImportTarget::AutoAnalysis, field, value });
        } else { self.invalid(name, ImportTarget::AutoAnalysis, raw); }
    }
    fn item(&mut self, key: String, status: ImportStatus) -> &mut ImportItemView {
        self.items.push(ImportItemView {
            key,
            status,
            target: None,
            value: None,
            reason: None,
            detail: None,
            credential: false,
        });
        self.items.last_mut().unwrap()
    }

    /// Every value shown in the preview and report passes the credential sanitizer.
    fn imported(&mut self, key: String, target: ImportTarget, value: String) {
        let it = self.item(key, ImportStatus::Imported);
        it.target = Some(target);
        it.value = Some(sanitize(&value));
    }

    /// A value Java would not use either; one holding credentials is reported without it.
    fn invalid(&mut self, key: String, target: ImportTarget, value: &Value) {
        let leaf = key.rsplit(['.', ':']).next().unwrap_or(&key).to_string();
        let credential = holds_credential(&leaf, value);
        let it = self.item(key, ImportStatus::Incompatible);
        it.target = Some(target);
        it.credential = credential;
        it.value = (!credential).then(|| sanitize(&shown(value)));
        it.reason = Some(ImportReason::InvalidValue);
    }

    /// A Java setting this product has no place for yet.
    fn not_migrated(&mut self, key: String, value: &Value) {
        let leaf = key.rsplit(['.', ':']).next().unwrap_or(&key).to_string();
        let credential = holds_credential(&leaf, value);
        let it = self.item(key, ImportStatus::NotMigrated);
        it.credential = credential;
        it.reason = Some(if credential {
            ImportReason::Credential
        } else {
            ImportReason::NotYetAvailable
        });
    }

    fn unreadable(&mut self, key: &str, detail: String) {
        let it = self.item(key.to_string(), ImportStatus::Incompatible);
        it.reason = Some(ImportReason::Unreadable);
        it.detail = Some(detail);
    }

    fn config(&mut self, root: &Map<String, Value>) {
        let empty = Map::new();
        let ui = root.get("ui").and_then(Value::as_object).unwrap_or(&empty);
        let leelaz = root.get("leelaz").and_then(Value::as_object).unwrap_or(&empty);
        let mut handled: Handled = Vec::new();

        for (key, field, kind) in [
            ("analyze-all-branch", "allBranches", 0),
            ("exit-auto-analyze-by-pause", "exitByPause", 0),
            ("exit-auto-analyze-tip", "exitTip", 0),
            ("auto-ana-diff-enable", "diffEnable", 0),
            ("auto-ana-diff-black", "diffBlack", 0),
            ("auto-ana-diff-white", "diffWhite", 0),
            ("auto-ana-diff-use-win", "diffUseWin", 0),
            ("auto-ana-diff-use-score", "diffUseScore", 0),
            ("auto-ana-diff-win-threshold", "diffWinThreshold", 1),
            ("auto-ana-diff-score-threshold", "diffScoreThreshold", 1),
            ("auto-ana-diff-time", "diffTime", 2),
            ("auto-ana-diff-playouts", "diffVisits", 2),
            ("auto-ana-diff-first-playouts", "diffFirstVisits", 2),
        ] {
            let Some(raw) = ui.get(key) else { continue };
            handled.push(("ui", key));
            let value = match kind {
                0 => as_bool(raw).map(Value::from),
                1 => as_f64(raw).map(Value::from),
                _ => as_i64(raw).and_then(|n| i32::try_from(n).ok()).map(Value::from),
            };
            self.auto_analysis_value(format!("ui.{key}"), field, value, raw);
        }
        // Config.java:1609-1613,1677-1683,1828,1939,1981. Java's actual top key is
        // show-top-tool-bar (not show-top-toolbar).
        for (key, field) in [
            ("is-classic-mode", "classicMode"),
            ("showing-independent-main", "showingIndependentMain"),
            ("showing-independent-sub", "showingIndependentSub"),
            ("independent-mainboard-locked", "independentMainLocked"),
            ("independent-subboard-locked", "independentSubLocked"),
            ("independent-main-board-top", "independentMainTop"),
            ("independent-sub-board-top", "independentSubTop"),
            ("show-subboard", "showSubBoard"), ("show-winrate-graph", "showWinrateGraph"),
            ("large-winrate-graph", "largeWinrateGraph"), ("show-comment", "showComment"),
            ("show-variation-graph", "showVariationGraph"), ("show-list-pane", "showListPane"),
            ("show-captured", "showCaptured"), ("show-status", "showStatus"),
            ("large-subboard", "largeSubBoard"), ("show-top-tool-bar", "showTopToolbar"),
            ("show-double-menu", "showDoubleMenu"), ("mains-always-ontop", "alwaysOnTop"),
            ("show-title-wr", "showWinrateInTitle"),
            ("hide-subboard-from-large-winrate", "hideSubBoardFromLargeWinrate"),
        ] {
            let Some(raw) = ui.get(key) else { continue };
            handled.push(("ui", key));
            match as_bool(raw) {
                Some(value) => {
                    self.mapped.panels.push(DisplayValue {
                        target: ImportTarget::WorkbenchPanels, field, value: Value::from(value),
                    });
                    self.imported(format!("ui.{key}"), ImportTarget::WorkbenchPanels, value.to_string());
                }
                None => self.invalid(format!("ui.{key}"), ImportTarget::WorkbenchPanels, raw),
            }
        }
        for (index, key) in [(0, "custom-layout-1"), (1, "custom-layout-2")] {
            let Some(raw) = ui.get(key) else { continue };
            handled.push(("ui", key));
            match custom_layout(raw) {
                Some(layout) => {
                    self.mapped.custom_layouts[index] = Some(layout);
                    self.imported(format!("ui.{key}"), ImportTarget::WorkbenchPanels, shown(raw));
                }
                None => self.invalid(format!("ui.{key}"), ImportTarget::WorkbenchPanels, raw),
            }
        }
        for (section, key, target, kind) in SIMPLE {
            let map = if *section == "ui" { ui } else { leelaz };
            let Some(v) = map.get(*key) else { continue };
            handled.push((section, key));
            let name = format!("{section}.{key}");
            let value = match kind {
                Kind::Bool(set) => as_bool(v).map(|b| {
                    set(&mut self.mapped, b);
                    b.to_string()
                }),
                Kind::U32(set) => as_i64(v).and_then(|n| u32::try_from(n).ok()).map(|n| {
                    set(&mut self.mapped, n);
                    n.to_string()
                }),
                Kind::U64(set) => as_i64(v).and_then(|n| u64::try_from(n).ok()).map(|n| {
                    set(&mut self.mapped, n);
                    n.to_string()
                }),
                Kind::I32(set) => as_i64(v).and_then(|n| i32::try_from(n).ok()).map(|n| {
                    set(&mut self.mapped, n);
                    n.to_string()
                }),
                Kind::Str(set) => v.as_str().map(|s| {
                    set(&mut self.mapped, s.to_string());
                    s.to_string()
                }),
                Kind::NonNegative(set) => as_f64(v).filter(|n| n.is_finite() && *n >= 0.0).map(|n| {
                    set(&mut self.mapped, n);
                    n.to_string()
                }),
            };
            match value {
                Some(value) => self.imported(name, *target, value),
                None => self.invalid(name, *target, v),
            }
        }
        // `isAnalysisCommandCustomized`: without its flag, a command other than the default one
        // was the user's.
        if self.mapped.analysis_command_customized.is_none() {
            if let Some(command) = &self.mapped.analysis_command {
                let normalized = |c: &str| join_command(&split_command(c.trim())).unwrap_or_default();
                let command = normalized(command);
                self.mapped.analysis_command_customized =
                    Some(!command.is_empty() && command != normalized(DEFAULT_ANALYSIS_COMMAND));
            }
        }
        // The analysis command's relative files resolve like an engine command's.
        if let Some(command) = self.mapped.analysis_command.take() {
            self.mapped.analysis_command = Some(resolve_command(&command, &self.source.roots(), &mut Vec::new()));
        }
        // `resolveConfiguredPath`: a relative model path is found under the Java folders.
        if let Some(path) = self.mapped.quick_model_path.take() {
            let relative = Path::new(path.trim());
            let roots = self.source.roots();
            let resolved = if path.trim().is_empty() || relative.is_absolute() {
                path
            } else {
                roots
                    .iter()
                    .map(|root| root.join(relative))
                    .find(|p| p.is_file())
                    .unwrap_or_else(|| roots[0].join(relative))
                    .to_string_lossy()
                    .into_owned()
            };
            self.mapped.quick_model_path = Some(resolved);
        }
        if let Some(path) = self.mapped.human_sl_model_path.take() {
            let relative = Path::new(path.trim());
            let roots = self.source.roots();
            let resolved = if path.trim().is_empty() || relative.is_absolute() {
                path
            } else {
                roots.iter().map(|root| root.join(relative)).find(|p| p.is_file())
                    .unwrap_or_else(|| roots[0].join(relative)).to_string_lossy().into_owned()
            };
            self.mapped.human_sl_model_path = Some(resolved);
        }
        if let Some(raw) = ui.get("human-sl-training-last-settings") {
            handled.push(("ui", "human-sl-training-last-settings"));
            if raw.is_object() {
                let mut training = serde_json::Map::new();
                for (java, product) in [
                    ("mode", "mode"), ("opponent-preset", "opponent"),
                    ("dan-rank", "danRank"), ("rank", "rank"),
                    ("player-color", "playerColor"), ("move-time-seconds", "moveTimeSeconds"),
                    ("handicap", "handicap"), ("komi", "komi"),
                ] {
                    if let Some(value) = raw.get(java) { training.insert(product.into(), value.clone()); }
                }
                let training: crate::settings::HumanSlTrainingSettings =
                    serde_json::from_value(Value::Object(training)).unwrap_or_default();
                let advanced = raw.get("advanced-visible").and_then(as_bool).unwrap_or(false);
                self.mapped.human_sl_training = Some(training.clone());
                self.mapped.human_sl_advanced_visible = Some(advanced);
                self.imported("ui.human-sl-training-last-settings".into(), ImportTarget::HumanSlTraining,
                    shown(&serde_json::json!({"training": training, "advancedVisible": advanced})));
            } else {
                self.invalid("ui.human-sl-training-last-settings".into(), ImportTarget::HumanSlTraining, raw);
            }
        }

        if let Some(v) = ui.get("use-language") {
            handled.push(("ui", "use-language"));
            // `AppLocale.fromConfigValue`: an unknown number follows the system.
            match as_i64(v) {
                Some(n) => {
                    let lang = match n {
                        1 => Language::ZhCn,
                        2 => Language::EnUs,
                        3 => Language::KoKr,
                        4 => Language::JaJp,
                        5 => Language::ZhTw,
                        6 => Language::ThTh,
                        _ => Language::System,
                    };
                    self.mapped.language = Some(lang);
                    self.imported("ui.use-language".into(), ImportTarget::Language, format!("{lang:?}"));
                }
                None => self.invalid("ui.use-language".into(), ImportTarget::Language, v),
            }
        }

        if let Some(v) = ui.get("extra-mode") {
            handled.push(("ui", "extra-mode"));
            let mode = read_extra_mode(v);
            self.mapped.extra_mode = Some(mode);
            self.imported("ui.extra-mode".into(), ImportTarget::ExtraMode, format!("{mode:?}"));
        }

        if let Some(v) = ui.get("recent-file-paths") {
            handled.push(("ui", "recent-file-paths"));
            // `Config.getRecentFilePaths`: `[{"path": …}]`, oldest first, at most five.
            match v.as_array() {
                Some(list) => {
                    let mut paths: Vec<String> = list
                        .iter()
                        .filter_map(|e| e.get("path").and_then(Value::as_str))
                        .filter(|p| !p.is_empty())
                        .map(str::to_string)
                        .collect();
                    if paths.len() > RECENT_LIMIT {
                        paths.drain(..paths.len() - RECENT_LIMIT);
                    }
                    self.imported(
                        "ui.recent-file-paths".into(),
                        ImportTarget::RecentFiles,
                        paths.join("\n"),
                    );
                    self.mapped.recent_files = Some(paths);
                }
                None => self.invalid("ui.recent-file-paths".into(), ImportTarget::RecentFiles, v),
            }
        }

        self.appearance(ui, &mut handled);
        self.hawkeye(ui, &mut handled);
        self.network(ui, &mut handled);
        self.display(ui, leelaz, &mut handled);
        self.game(ui, leelaz, &mut handled);
        self.matches(ui, &mut handled);
        self.engines(ui, leelaz, &mut handled);

        for (section, value) in root {
            match value.as_object() {
                Some(map) => {
                    for (key, v) in map {
                        if !handled.iter().any(|(s, k)| s == section && k == key) {
                            self.not_migrated(format!("{section}.{key}"), v);
                        }
                    }
                }
                None => self.not_migrated(section.clone(), value),
            }
        }
    }

    fn game_value(&mut self, name: String, target: ImportTarget, field: &'static str, value: Option<Value>, raw: &Value) {
        if let Some(value) = value {
            self.imported(name, target, shown(&value));
            self.mapped.game.push(DisplayValue { target, field, value });
        } else {
            // A broken Java combo index or unusable value must not overwrite this profile.
            self.not_migrated(name, raw);
            let item = self.items.last_mut().unwrap();
            item.target = Some(target);
            item.reason = Some(ImportReason::InvalidValue);
        }
    }

    fn game(&mut self, ui: &Map<String, Value>, leelaz: &Map<String, Value>, handled: &mut Handled) {
        for (section, key, target, field, kind) in GAME {
            let map = if *section == "ui" { ui } else { leelaz };
            let Some(raw) = map.get(*key) else { continue };
            handled.push((section, key));
            let value = match kind {
                DisplayKind::Bool => as_bool(raw).map(Value::from),
                DisplayKind::Int(min, max) => as_i64(raw).filter(|n| (*min..=*max).contains(n)).map(Value::from),
                DisplayKind::Num => as_f64(raw).map(Value::from),
                DisplayKind::NonNegative => as_f64(raw).filter(|n| *n >= 0.0).map(Value::from),
            };
            self.game_value(format!("{section}.{key}"), *target, field, value, raw);
        }
        if let Some(raw) = ui.get("new-game-handicap") {
            handled.push(("ui", "new-game-handicap"));
            // NewGameDialog.java:213-245,966: stored index 0..8 selects 0,2..9 stones.
            let value = as_i64(raw).filter(|n| (0..=8).contains(n))
                .map(|n| Value::from(if n == 0 { 0 } else { n + 1 }));
            self.game_value("ui.new-game-handicap".into(), ImportTarget::HumanEngineGame, "handicap", value, raw);
        }
        if let Some(raw) = ui.get("kata-time-type") {
            handled.push(("ui", "kata-time-type"));
            let value = as_i64(raw).and_then(|n| match n {
                0 => Some("Byoyomi"), 1 => Some("Fischer"), 2 => Some("Absolute"), _ => None,
            }).map(Value::from);
            self.game_value("ui.kata-time-type".into(), ImportTarget::GameAiTime, "aiTime.kataType", value, raw);
        }
        if let Some(raw) = ui.get("advance-time-txt") {
            handled.push(("ui", "advance-time-txt"));
            // JSONObject.optString accepts any non-null scalar as text.
            let value = (!raw.is_null()).then(|| Value::from(shown(raw)));
            self.game_value("ui.advance-time-txt".into(), ImportTarget::GameAiTime, "aiTime.advanced", value, raw);
        }
        let mut flags = [false; 3];
        let mut present = false;
        for (index, key) in ["genmove-game-notime", "advance-time-settings", "kata-time-settings"].into_iter().enumerate() {
            let Some(raw) = ui.get(key) else { continue };
            handled.push(("ui", key));
            if let Some(value) = as_bool(raw) {
                flags[index] = value;
                present = true;
                self.imported(format!("ui.{key}"), ImportTarget::GameAiTime, value.to_string());
            } else {
                self.game_value(format!("ui.{key}"), ImportTarget::GameAiTime, "aiTime.mode", None, raw);
            }
        }
        if present {
            // DesktopTimeControl.selectedMode: engine-owned > raw advanced > KataGo > fixed.
            let mode = if flags[0] { "EngineOwned" } else if flags[1] { "Advanced" }
                else if flags[2] { "KataGo" } else { "Fixed" };
            self.mapped.game.push(DisplayValue {
                target: ImportTarget::GameAiTime, field: "aiTime.mode", value: Value::from(mode),
            });
        }
    }

    /// Board display settings, read like `Config.java` reads them (`opt*` with the defaults of a
    /// new profile); values Java would not use are reported.
    fn display(&mut self, ui: &Map<String, Value>, leelaz: &Map<String, Value>, handled: &mut Handled) {
        for (key, target, field, kind) in DISPLAY {
            let Some(v) = ui.get(*key) else { continue };
            handled.push(("ui", key));
            let value = match kind {
                DisplayKind::Bool => as_bool(v).map(Value::from),
                DisplayKind::Int(min, max) => as_i64(v).filter(|n| (*min..=*max).contains(n)).map(Value::from),
                DisplayKind::Num => as_f64(v).map(Value::from),
                DisplayKind::NonNegative => as_f64(v).filter(|n| *n >= 0.0).map(Value::from),
            };
            self.display_value(format!("ui.{key}"), *target, field, value, v);
        }

        if let Some(v) = leelaz.get("show-nosugg-circle") {
            handled.push(("leelaz", "show-nosugg-circle"));
            self.display_value("leelaz.show-nosugg-circle".into(), ImportTarget::Suggestions,
                "noSuggCircle", as_bool(v).map(Value::from), v);
        }

        // Numeric columns override letter columns; top-origin rows override bottom-origin
        // rows (`BoardRenderer.drawCoordinates`); Fox includes I and counts from the top.
        self.display_flags(ui, handled, ImportTarget::Coordinates, "coordinateStyle", &[
            ("use-num-coords-from-top", "NumbersFromTop"),
            ("use-num-coords-from-bottom", if ui.get("use-fox-style-coords").and_then(as_bool) == Some(true) {
                "NumbersFromTop"
            } else { "NumbersFromBottom" }),
            ("use-fox-style-coords", "Fox"),
            ("use-i-in-coords-name", "WithI"),
        ], "Normal");
        // The Branch consumer uses the last/all flags, not the legacy master checkbox.
        self.display_flags(ui, handled, ImportTarget::Suggestions, "pvVisits", &[
            ("show-pv-visits-all-move", "AllMoves"),
            ("show-pv-visits-last-move", "LastMove"),
            ("show-pv-visits", "Off"),
        ], "Off");

        // Java's five pure-network painter flags are one menu choice. `drawKataEstimate`
        // chooses BySize first, then `shouldShowCountBlockBig`; the remaining normal modes
        // are distinguished by the not-on-live flag. Small is the fallback menu choice.
        const PURE_KEYS: [&str; 5] = [
            "show-pure-estimate-normal", "show-pure-estimate-by-size",
            "show-pure-estimate-big-below", "show-pure-estimate-not-on-live", "show-pure-estimate-small",
        ];
        if PURE_KEYS.iter().any(|k| ui.contains_key(*k)) {
            let mut valid = true;
            for key in PURE_KEYS {
                let Some(v) = ui.get(key) else { continue };
                handled.push(("ui", key));
                if as_bool(v).is_some() {
                    self.imported(format!("ui.{key}"), ImportTarget::PureNetwork, shown(v));
                } else {
                    valid = false;
                    self.invalid(format!("ui.{key}"), ImportTarget::PureNetwork, v);
                }
            }
            if valid {
                let on = |key: &str, default| ui.get(key).and_then(as_bool).unwrap_or(default);
                let style = if on(PURE_KEYS[1], false) { "BySize" }
                    else if on(PURE_KEYS[2], true) { "BigBelow" }
                    else if on(PURE_KEYS[0], false) {
                        if on(PURE_KEYS[3], false) { "TransparentNotOnLive" } else { "Transparent" }
                    } else { "Small" };
                self.mapped.display.push(DisplayValue {
                    target: ImportTarget::PureNetwork,
                    field: "pureEstimate",
                    value: Value::from(style),
                });
            }
        }

        // 显示恶手损失条: Java turns it off once unless the profile was already migrated
        // (`Config.hideBlunderBarDefaultOnce`).
        const BLUNDER_MIGRATED: &str = "migrated-hide-blunder-bar-default-v1";
        if let Some(v) = ui.get("show-blunder-bar") {
            handled.push(("ui", "show-blunder-bar"));
            let migrated = ui.get(BLUNDER_MIGRATED).and_then(as_bool).unwrap_or(false);
            let value = as_bool(v).map(|on| Value::from(on && migrated));
            self.display_value("ui.show-blunder-bar".into(), ImportTarget::WinrateGraph, "blunderBar", value, v);
        }
        if ui.contains_key(BLUNDER_MIGRATED) {
            handled.push(("ui", BLUNDER_MIGRATED));
        }

        // Kata形势 squares: `drawEstimate` chooses by size first, then the big squares, then the
        // small or half-square ones, skipping same-colour stones with not-on-live; the normal
        // flag itself is not consulted.
        const KATA_KEYS: [&str; 5] = [
            "show-katago-estimate-small", "show-katago-estimate-normal", "show-katago-estimate-by-size",
            "show-katago-estimate-big-below", "show-katago-estimate-not-on-live",
        ];
        if KATA_KEYS.iter().any(|k| ui.contains_key(*k)) {
            let mut valid = true;
            for key in KATA_KEYS {
                let Some(v) = ui.get(key) else { continue };
                handled.push(("ui", key));
                if as_bool(v).is_some() {
                    self.imported(format!("ui.{key}"), ImportTarget::KataEstimate, shown(v));
                } else {
                    valid = false;
                    self.invalid(format!("ui.{key}"), ImportTarget::KataEstimate, v);
                }
            }
            if valid {
                let on = |key: &str, default| ui.get(key).and_then(as_bool).unwrap_or(default);
                let style = if on(KATA_KEYS[2], false) { "BySize" }
                    else if on(KATA_KEYS[3], false) { "BigBelow" }
                    else if on(KATA_KEYS[0], true) { "Small" }
                    else if on(KATA_KEYS[4], false) { "TransparentNotOnLive" }
                    else { "Transparent" };
                self.mapped.display.push(DisplayValue {
                    target: ImportTarget::KataEstimate,
                    field: "kataEstimateStyle",
                    value: Value::from(style),
                });
            }
        }

        // 手数: `allow-move-number`, else derived from the older keys (`Config.java:1601-1605`).
        const NUMBER_KEYS: [&str; 3] = ["show-move-number", "only-last-move-number", "allow-move-number"];
        if NUMBER_KEYS.iter().any(|k| ui.contains_key(*k)) {
            let show = ui.get("show-move-number").map(as_bool);
            let only = ui.get("only-last-move-number").map(as_i64);
            let allow = ui.get("allow-move-number").map(as_i64);
            for (key, ok) in [
                ("show-move-number", show.map(|v| v.is_some())),
                ("only-last-move-number", only.map(|v| v.is_some())),
                ("allow-move-number", allow.map(|v| v.is_some_and(|n| i32::try_from(n).is_ok()))),
            ] {
                let Some(ok) = ok else { continue };
                handled.push(("ui", key));
                let v = &ui[key];
                if ok {
                    self.imported(format!("ui.{key}"), ImportTarget::MoveNumbers, shown(v));
                } else {
                    self.invalid(format!("ui.{key}"), ImportTarget::MoveNumbers, v);
                }
            }
            let derived = show.flatten().map(|show| match only.flatten().unwrap_or(0) {
                _ if !show => 0,
                n if n > 0 => n,
                _ => -1,
            });
            let value = allow.flatten().or(derived).and_then(|n| i32::try_from(n).ok());
            if let Some(n) = value {
                self.mapped.display.push(DisplayValue {
                    target: ImportTarget::MoveNumbers,
                    field: "moveNumbers",
                    value: Value::from(n),
                });
            }
        }

        // 落子评价 > 判定依据 (`MoveRankEvaluationMode.fromConfig`).
        const MODE_KEYS: [&str; 3] = [
            "move-rank-evaluation-mode",
            "use-win-loss-in-move-rank",
            "use-score-loss-in-move-rank",
        ];
        if MODE_KEYS.iter().any(|k| ui.contains_key(*k)) {
            let named = ui
                .get(MODE_KEYS[0])
                .and_then(Value::as_str)
                .and_then(|s| match s.trim().to_lowercase().as_str() {
                    "auto" => Some("Auto"),
                    "score" => Some("Score"),
                    "winrate" => Some("Winrate"),
                    "combined" => Some("Combined"),
                    _ => None,
                });
            let win = ui.get(MODE_KEYS[1]).and_then(as_bool).unwrap_or(true);
            let score = ui.get(MODE_KEYS[2]).and_then(as_bool).unwrap_or(true);
            let mode = named.unwrap_or(match (win, score) {
                (true, false) => "Winrate",
                (false, true) => "Score",
                _ => "Auto",
            });
            for key in MODE_KEYS {
                if let Some(v) = ui.get(key) {
                    handled.push(("ui", key));
                    self.imported(format!("ui.{key}"), ImportTarget::MoveRank, shown(v));
                }
            }
            self.mapped.display.push(DisplayValue {
                target: ImportTarget::MoveRank,
                field: "moveRankMode",
                value: Value::from(mode),
            });
        }

        // 小棋盘设置: 热点图 wins over 仅棋盘棋子 (`SubBoardRenderer.draw`).
        const SUB_KEYS: [&str; 3] = ["show-heat", "show-heat-aftercalc", "subboard-raw"];
        if SUB_KEYS.iter().any(|k| ui.contains_key(*k)) {
            let flag = |key: &str| ui.get(key).map(as_bool);
            let (heat, after, raw) = (flag(SUB_KEYS[0]), flag(SUB_KEYS[1]), flag(SUB_KEYS[2]));
            let mut valid = true;
            for (key, read) in SUB_KEYS.into_iter().zip([heat, after, raw]) {
                let Some(read) = read else { continue };
                handled.push(("ui", key));
                let v = &ui[key];
                match read {
                    Some(_) => self.imported(format!("ui.{key}"), ImportTarget::SubBoard, shown(v)),
                    None => {
                        valid = false;
                        self.invalid(format!("ui.{key}"), ImportTarget::SubBoard, v);
                    }
                }
            }
            if valid {
                let on = |f: Option<Option<bool>>| f.flatten().unwrap_or(false);
                let mode = match (on(heat), on(after), on(raw)) {
                    (true, true, _) => "Visits",
                    (true, false, _) => "Policy",
                    (false, _, true) => "Raw",
                    (false, _, false) => "Variation",
                };
                self.mapped.display.push(DisplayValue {
                    target: ImportTarget::SubBoard,
                    field: "subBoard",
                    value: Value::from(mode),
                });
            }
        }
    }

    /// Mutually exclusive projection of Java flags, in the consumer's precedence order.
    fn display_flags(&mut self, ui: &Map<String, Value>, handled: &mut Handled,
        target: ImportTarget, field: &'static str, flags: &[(&'static str, &'static str)], fallback: &'static str) {
        if !flags.iter().any(|(key, _)| ui.contains_key(*key)) {
            return;
        }
        let mut chosen = None;
        let mut has_valid = false;
        for &(key, mode) in flags {
            let Some(v) = ui.get(key) else { continue };
            handled.push(("ui", key));
            match as_bool(v) {
                Some(on) => {
                    self.imported(format!("ui.{key}"), target, shown(v));
                    has_valid = true;
                    if on && chosen.is_none() { chosen = Some(mode); }
                }
                None => self.invalid(format!("ui.{key}"), target, v),
            }
        }
        if has_valid {
            self.mapped.display.push(DisplayValue { target, field, value: Value::from(chosen.unwrap_or(fallback)) });
        }
    }

    fn display_value(&mut self, key: String, target: ImportTarget, field: &'static str, value: Option<Value>, raw: &Value) {
        match value {
            Some(value) => {
                self.imported(key, target, shown(raw));
                self.mapped.display.push(DisplayValue { target, field, value });
            }
            None => self.invalid(key, target, raw),
        }
    }

    /// 超级鹰眼 (`Config.java:1801-1827, 1891-1893, 1947-1956, 2021-2024`). Java writes 总在最前 as
    /// `badmoves-always-on-top` but reads `badmoves-always-ontop`: only the key Java reads takes
    /// effect, the other is reported as unused.
    fn hawkeye(&mut self, ui: &Map<String, Value>, handled: &mut Handled) {
        enum Read {
            Bool,
            /// An integer within the range.
            Int(i64, i64),
            Branch,
        }
        const KEYS: &[(&str, &str, Read)] = &[
            ("show-badmoves-frame", "showFrame", Read::Bool),
            ("badmoves-always-ontop", "alwaysOnTop", Read::Bool),
            ("movelist-selected-indextop", "topPage", Read::Int(0, 2)),
            ("movelist-selected-index", "chart", Read::Int(0, 8)),
            ("moveList-selected-branch", "branch", Read::Branch),
            ("show-movelist-matchai", "showList", Read::Bool),
            ("show-winrate-matchai", "showGraph", Read::Bool),
            ("move-list-winrate-threshold", "winrateThreshold", Read::Int(0, 99)),
            ("move-list-score-threshold", "scoreThreshold", Read::Int(0, 99)),
            ("move-list-visits-threshold", "visitsThreshold", Read::Int(0, 999_999)),
            ("movelist-top-curnode", "topCurrentMove", Read::Bool),
            ("opening-end-move", "openingEndMove", Read::Int(i32::MIN as i64, i32::MAX as i64)),
            ("middle-end-move", "middleEndMove", Read::Int(i32::MIN as i64, i32::MAX as i64)),
            ("game-statistics-custom-start", "customStart", Read::Int(i32::MIN as i64, i32::MAX as i64)),
            ("game-statistics-custom-end", "customEnd", Read::Int(i32::MIN as i64, i32::MAX as i64)),
            ("winrate-diff-range1", "winrateRange1", Read::Int(i32::MIN as i64, i32::MAX as i64)),
            ("winrate-diff-range2", "winrateRange2", Read::Int(i32::MIN as i64, i32::MAX as i64)),
            ("score-diff-range1", "scoreRange1", Read::Int(i32::MIN as i64, i32::MAX as i64)),
            ("score-diff-range2", "scoreRange2", Read::Int(i32::MIN as i64, i32::MAX as i64)),
            ("loss-panel-select-winrate", "lossWinrate", Read::Bool),
        ];
        for &(key, field, ref read) in KEYS {
            let Some(v) = ui.get(key) else { continue };
            handled.push(("ui", key));
            let value = match read {
                Read::Bool => as_bool(v).map(Value::from),
                Read::Int(min, max) => as_i64(v).filter(|n| (*min..=*max).contains(n)).map(Value::from),
                // `showBranch.setSelectedIndex`: 1 is 当前, anything else 主干.
                Read::Branch => as_i64(v).map(|n| Value::from(if n == 1 { "Current" } else { "Main" })),
            };
            match value {
                Some(value) => {
                    self.mapped.hawkeye.push((field, value));
                    self.imported(format!("ui.{key}"), ImportTarget::Hawkeye, shown(v));
                }
                None => self.invalid(format!("ui.{key}"), ImportTarget::Hawkeye, v),
            }
        }
        if let Some(v) = ui.get("move-list-filter-current") {
            handled.push(("ui", "move-list-filter-current"));
            match as_bool(v) {
                Some(b) => {
                    self.mapped.hawkeye_range.0 = Some(b);
                    self.imported("ui.move-list-filter-current".into(), ImportTarget::Hawkeye, shown(v));
                }
                None => self.invalid("ui.move-list-filter-current".into(), ImportTarget::Hawkeye, v),
            }
        }
        for (key, first) in [("match-ai-firstmove", true), ("match-ai-lastmove", false)] {
            let Some(v) = ui.get(key) else { continue };
            handled.push(("ui", key));
            match as_i64(v).and_then(|n| i32::try_from(n).ok()) {
                Some(n) => {
                    if first {
                        self.mapped.hawkeye_range.1 = Some(n);
                    } else {
                        self.mapped.hawkeye_range.2 = Some(n);
                    }
                    self.imported(format!("ui.{key}"), ImportTarget::Hawkeye, shown(v));
                }
                None => self.invalid(format!("ui.{key}"), ImportTarget::Hawkeye, v),
            }
        }
        if ui.contains_key("badmoves-always-on-top") {
            handled.push(("ui", "badmoves-always-on-top"));
            let it = self.item("ui.badmoves-always-on-top".into(), ImportStatus::NotMigrated);
            it.reason = Some(ImportReason::Obsolete);
        }
    }

    /// `ui.network-proxy-mode` / `-host` / `-port` (`NetworkProxy.settings`): a missing mode is
    /// `direct`; `manual` needs a host and a port 1–65535. Values Java would reject are reported
    /// and the setting is left as it is.
    fn network(&mut self, ui: &Map<String, Value>, handled: &mut Handled) {
        const KEYS: [&str; 3] = ["network-proxy-mode", "network-proxy-host", "network-proxy-port"];
        if !KEYS.iter().any(|k| ui.contains_key(*k)) {
            return;
        }
        for key in KEYS {
            if ui.contains_key(key) {
                handled.push(("ui", key));
            }
        }
        let mode = match ui.get("network-proxy-mode").map(|v| v.as_str().map(str::trim)) {
            None => Some(ProxyMode::Direct),
            Some(Some("direct")) => Some(ProxyMode::Direct),
            Some(Some("system")) => Some(ProxyMode::System),
            Some(Some("manual")) => Some(ProxyMode::Manual),
            Some(_) => None,
        };
        let host = ui
            .get("network-proxy-host")
            .map(|v| v.as_str().map(|s| s.trim().to_string()));
        // `parsePort`: a number or a numeric string.
        let port = ui.get("network-proxy-port").map(|v| match v {
            Value::Number(_) | Value::String(_) => shown(v).trim().parse::<i64>().ok(),
            _ => None,
        });
        let host_ok = matches!(&host, None | Some(Some(_)));
        let port_ok = port.is_none() || port.is_some_and(|p| p.is_some_and(|n| (1..=65535).contains(&n)));
        let manual_ok = mode != Some(ProxyMode::Manual)
            || (host.as_ref().is_some_and(|h| h.as_ref().is_some_and(|h| !h.is_empty()))
                && port.is_some_and(|p| p.is_some()));
        let Some(mode) = mode.filter(|_| host_ok && port_ok && manual_ok) else {
            for key in KEYS {
                if let Some(v) = ui.get(key) {
                    self.invalid(format!("ui.{key}"), ImportTarget::NetworkProxy, v);
                }
            }
            return;
        };
        let settings = NetworkSettings {
            proxy_mode: mode,
            proxy_host: host.flatten().unwrap_or_default(),
            proxy_port: port.flatten().map_or(0, |n| n as u16),
        };
        for key in KEYS {
            if let Some(v) = ui.get(key) {
                self.imported(format!("ui.{key}"), ImportTarget::NetworkProxy, shown(v));
            }
        }
        self.mapped.network = Some(settings);
    }

    /// The engine list and the start-up choice.
    fn engines(&mut self, ui: &Map<String, Value>, leelaz: &Map<String, Value>, handled: &mut Handled) {
        let flag = |key: &str| ui.get(key).and_then(as_bool).unwrap_or(false);
        let int = |key: &str| ui.get(key).and_then(as_i64).unwrap_or(-1);
        let entries: Vec<Map<String, Value>> = match leelaz.get("engine-settings-list") {
            Some(Value::Array(list)) => {
                handled.push(("leelaz", "engine-settings-list"));
                list.iter()
                    .map(|e| e.as_object().cloned().unwrap_or_default())
                    .collect()
            }
            Some(other) => {
                handled.push(("leelaz", "engine-settings-list"));
                self.invalid("leelaz.engine-settings-list".into(), ImportTarget::Engines, other);
                return;
            }
            None if leelaz.contains_key("engine-command") => {
                for key in LEGACY_ENGINE_KEYS {
                    if leelaz.contains_key(key) {
                        handled.push(("leelaz", key));
                    }
                }
                legacy_entries(leelaz, int("default-engine"))
            }
            None => {
                // Start-up choices without any engine to start.
                for key in ENGINE_UI_KEYS {
                    if let Some(v) = ui.get(key) {
                        handled.push(("ui", key));
                        let it = self.item(format!("ui.{key}"), ImportStatus::Incompatible);
                        it.value = Some(sanitize(&shown(v)));
                        it.reason = Some(ImportReason::InvalidValue);
                        it.detail = Some("no engine list".into());
                    }
                }
                return;
            }
        };
        for key in ENGINE_UI_KEYS {
            if ui.contains_key(key) {
                handled.push(("ui", key));
            }
        }

        // `Utils.normalizeDefaultEngineFlags`: the first `isDefault` entry, else `default-engine`
        // while `autoload-default` asks for it.
        let mut default = entries
            .iter()
            .position(|e| e.get("isDefault").and_then(as_bool).unwrap_or(false));
        if default.is_none() && flag("autoload-default") {
            default = usize::try_from(int("default-engine"))
                .ok()
                .filter(|&i| i < entries.len());
        }

        let roots = self.source.roots();
        let mut mapped: Vec<MappedEngine> = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let (view, profile) = engine(index, entry, default == Some(index), &roots);
            if let Some(mut profile) = profile {
                if profile.id.is_empty() || mapped.iter().any(|m| m.profile.id == profile.id) {
                    let existing: Vec<EngineProfile> = mapped.iter().map(|m| m.profile.clone()).collect();
                    profile.id = new_profile_id(&existing);
                }
                mapped.push(MappedEngine {
                    java_index: index,
                    credential: view.credential,
                    profile,
                });
            }
            self.engines.push(view);
        }
        self.imported(
            "leelaz.engine-settings-list".into(),
            ImportTarget::Engines,
            format!("{} / {}", mapped.len(), entries.len()),
        );
        self.mapped.engines = Some(mapped);
        self.mapped.default_engine = default;

        // `Lizzie.java:457-488`: default, else last, else none, else ask.
        let startup = if flag("autoload-default") {
            EngineStartup::Default
        } else if flag("autoload-last") {
            EngineStartup::Last
        } else if flag("autoload-empty") {
            EngineStartup::Empty
        } else {
            EngineStartup::Manual
        };
        self.mapped.startup = Some(startup);
        for key in ["autoload-default", "autoload-last", "autoload-empty"] {
            if let Some(v) = ui.get(key) {
                self.imported(format!("ui.{key}"), ImportTarget::EngineStartup, shown(v));
            }
        }
        if ui.contains_key("default-engine") {
            let value = default.map_or("-1".to_string(), |d| d.to_string());
            self.imported("ui.default-engine".into(), ImportTarget::Engines, value);
        }
        if let Some(v) = ui.get("last-engine") {
            self.mapped.last_engine = as_i64(v)
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&i| i < entries.len());
            self.imported("ui.last-engine".into(), ImportTarget::LastEngine, shown(v));
        }
    }

    fn persist(&mut self, root: &Map<String, Value>) {
        for (section, value) in root {
            let Some(map) = value.as_object() else {
                if section == "autosave" {
                    // Java's template keeps an `autosave` entry that nothing writes or reads any
                    // more (`Board.autosave` is never called; 存档与读档 replaced it).
                    if value.as_str().is_none_or(|s| !s.is_empty()) {
                        let it = self.item(format!("persist:{section}"), ImportStatus::NotMigrated);
                        it.reason = Some(ImportReason::Obsolete);
                    }
                    continue;
                }
                self.not_migrated(format!("persist:{section}"), value);
                continue;
            };
            for (key, v) in map {
                let name = format!("persist:{section}.{key}");
                match (section.as_str(), key.as_str()) {
                    ("ui-persist", "independent-main-board" | "independent-sub-board") => {
                        match java_window(v) {
                            Some(placement) => {
                                if key == "independent-main-board" { self.mapped.independent_main_board = Some(placement); }
                                else { self.mapped.independent_sub_board = Some(placement); }
                                self.imported(name, ImportTarget::WorkbenchPanels, shown(v));
                            }
                            None => self.invalid(name, ImportTarget::WorkbenchPanels, v),
                        }
                    }
                    ("filesystem", "last-folder") => match v {
                        // Java's default is "" (no folder yet).
                        Value::String(s) if s.is_empty() => {}
                        Value::String(s) => {
                            self.mapped.last_folder = Some(s.clone());
                            self.imported(name, ImportTarget::LastFolder, s.clone());
                        }
                        other => self.invalid(name, ImportTarget::LastFolder, other),
                    },
                    ("filesystem", "last-image-folder") => match v {
                        Value::String(s) if s.is_empty() => {}
                        Value::String(s) => {
                            self.mapped.last_image_folder = Some(s.clone());
                            self.imported(name, ImportTarget::LastImageFolder, s.clone());
                        }
                        other => self.invalid(name, ImportTarget::LastImageFolder, other),
                    },
                    ("ui-persist", "leftover-left-share" | "comment-height-share" | "variation-graph-share") => {
                        let field = match key.as_str() {
                            "leftover-left-share" => "leftoverLeftShare",
                            "comment-height-share" => "commentHeightShare",
                            _ => "variationGraphShare",
                        };
                        match as_f64(v).filter(|n| n.is_finite()) {
                            Some(value) => {
                                // LizzieFrame.java:7984-8008 clamps Java's persisted shares.
                                self.mapped.panel_sizes.push(DisplayValue {
                                    target: ImportTarget::PanelSizes, field, value: Value::from(value.clamp(0.0, 1.0)),
                                });
                                self.imported(name, ImportTarget::PanelSizes, shown(v));
                            }
                            None => self.invalid(name, ImportTarget::PanelSizes, v),
                        }
                    }
                    ("ui-persist", "board-postion-propotion") => {
                        match as_i64(v) {
                            Some(value) => {
                                self.mapped.panel_sizes.push(DisplayValue {
                                    target: ImportTarget::PanelSizes, field: "boardPositionProportion",
                                    value: Value::from(value.clamp(0, 8)),
                                });
                                self.imported(name, ImportTarget::PanelSizes, shown(v));
                            }
                            None => self.invalid(name, ImportTarget::PanelSizes, v),
                        }
                    }
                    ("ui-persist", "main-window-other") => {
                        // LizzieFrame.java:946-950 reads exactly five entries, first is the toolbar.
                        match v.as_array().filter(|a| a.len() == 5).and_then(|a| as_i64(&a[0])) {
                            Some(height) => {
                                self.mapped.bottom_toolbar = Some(match height {
                                    0 => crate::settings::BottomToolbar::Hidden,
                                    70 => crate::settings::BottomToolbar::Detailed,
                                    _ => crate::settings::BottomToolbar::Simple,
                                });
                                self.imported(format!("{name}[0]"), ImportTarget::BottomToolbar, height.to_string());
                                let item = self.item(name, ImportStatus::Incompatible);
                                item.reason = Some(ImportReason::WindowCoordinates);
                            }
                            None => self.invalid(name, ImportTarget::BottomToolbar, v),
                        }
                    }
                    ("ui-persist", "toolbar-parameter") => {
                        // Keep a not-migrated report for the engine-match and other toolbar state.
                        self.not_migrated(name.clone(), v);
                        let Some(values) = v.as_array().filter(|a| a.len() == 51) else {
                            self.invalid(name, ImportTarget::Suggestions, v);
                            continue;
                        };
                        // Config.java:3307-3331 saves 1/-1 checkboxes and -1 for empty fields.
                        // BottomToolbar.java:2378-2379,2412-2434 starts black/white/time on and
                        // only sets boxes true: even saved -1 leaves those three checked.
                        for (index, field) in [
                            (9, "black"), (10, "white"), (11, "timeEnabled"), (12, "timeSeconds"),
                            (13, "playoutsEnabled"), (14, "playouts"),
                            (15, "firstPlayoutsEnabled"), (16, "firstPlayouts"),
                        ] {
                            let raw = &values[index];
                            let value = as_i64(raw).and_then(|n| i32::try_from(n).ok()).map(|n| match index {
                                9..=11 => Value::from(true),
                                13 | 15 => Value::from(n > 0),
                                _ => Value::from(if n > 0 { n } else { -1 }),
                            });
                            if let Some(value) = value {
                                self.imported(format!("{name}[{index}]"), ImportTarget::AutoPlay, shown(&value));
                                self.mapped.autoplay.push(DisplayValue { target: ImportTarget::AutoPlay, field, value });
                            } else {
                                self.invalid(format!("{name}[{index}]"), ImportTarget::AutoPlay, raw);
                            }
                        }
                        self.match_toolbar(values, &name);
                        for (index, field) in [(40, "autoplayMainSeconds"), (41, "autoplaySubMillis")] {
                            let raw = &values[index];
                            let value = as_i64(raw).filter(|n| *n > 0 && *n <= i64::from(i32::MAX))
                                .map(Value::from);
                            // Java restores only positive integer intervals; -1 is its empty
                            // field sentinel, not an active timer.
                            let value = if as_i64(raw) == Some(-1) { Some(Value::from(-1)) } else { value };
                            self.display_value(format!("{name}[{index}]"), ImportTarget::Suggestions, field, value, raw);
                        }
                        // BottomToolbar restores a field only past its sentinel (start >= 0, the
                        // others > 0), otherwise keeping the control's default: start 1, time 2,
                        // the rest empty. Its checkboxes are only ever switched on, and 按时间
                        // starts selected, so Java always shows it on.
                        for (index, field) in [
                            (0, "startMove"), (1, "endMove"), (2, "timeEnabled"), (3, "timeSeconds"),
                            (4, "autoSave"), (5, "visitsEnabled"), (6, "visits"),
                            (7, "firstVisitsEnabled"), (8, "firstVisits"),
                        ] {
                            let raw = &values[index];
                            let value = as_i64(raw).and_then(|n| i32::try_from(n).ok()).map(|n| match index {
                                0 => Value::from(if n >= 0 { n } else { 1 }),
                                2 => Value::from(true),
                                4 | 5 | 7 => Value::from(n > 0),
                                3 => Value::from(if n > 0 { n } else { 2 }),
                                _ => Value::from(if n > 0 { n } else { -1 }),
                            });
                            self.auto_analysis_value(format!("{name}[{index}]"), field, value, raw);
                        }
                        for (index, field) in [(47, "black"), (48, "white")] {
                            let raw = &values[index];
                            self.auto_analysis_value(format!("{name}[{index}]"), field, as_bool(raw).map(Value::from), raw);
                        }
                    }
                    // Window bounds and the tables' column pixels follow Java's own scaling,
                    // fonts and layout.
                    ("ui-persist", "main-window-position" | "window-maximized" | "suggestions-list-position"
                        | "suggestions-list-position-7" | "suggestions-list-position-9" | "main-window-list"
                        | "badmoves-list-position") => {
                        let it = self.item(name, ImportStatus::Incompatible);
                        it.value = Some(shown(v));
                        it.reason = Some(ImportReason::WindowCoordinates);
                    }
                    _ => self.not_migrated(name, v),
                }
            }
        }
    }

    /// Other Java data next to the configuration. Commentary settings are read separately;
    /// credentials are discovered by the platform adapter and require independent consent.
    fn other_files(&mut self) {
        let dir = self.source.config_dir.clone();
        if dir.join("theme").is_dir() {
            let it = self.item("file:theme".into(), ImportStatus::NotMigrated);
            it.reason = Some(ImportReason::NotYetAvailable);
        }
        let models = crate::strength::java_models(&dir.join("strength-models"));
        if !models.is_empty() {
            let it = self.item("file:strength-models".into(), ImportStatus::Imported);
            it.target = Some(ImportTarget::StrengthModels);
            it.value = Some(models.len().to_string());
        }
        if dir.join("chatgpt/accounts.json").exists() {
            let item = self.item("file:chatgpt/accounts.json".into(), ImportStatus::NotMigrated);
            item.reason = Some(ImportReason::Credential);
            item.detail = Some("ChatGPT sessions are not migrated; sign in again.".into());
        }
    }
}

/// `Utils.getEngineDataOld`: `engine-command` is entry 0, `engine-command-list[i-1]` entry i;
/// names, preload, sizes and komi are indexed by entry.
fn legacy_entries(leelaz: &Map<String, Value>, default_engine: i64) -> Vec<Map<String, Value>> {
    let list = |key: &str| leelaz.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let commands = list("engine-command-list");
    let fields = [
        ("name", list("engine-name-list")),
        ("preload", list("engine-preload-list")),
        ("width", list("engine-width-list")),
        ("height", list("engine-height-list")),
        ("komi", list("engine-komi-list")),
    ];
    let count = if leelaz.contains_key("engine-command-list") {
        commands.len() + 1
    } else {
        1
    };
    let mut out = Vec::new();
    for i in 0..count {
        let command = if i == 0 {
            leelaz.get("engine-command").map(shown).unwrap_or_default()
        } else {
            commands.get(i - 1).map(shown).unwrap_or_default()
        };
        if i > 0 && command.is_empty() {
            continue;
        }
        let mut e = Map::new();
        e.insert("command".into(), Value::String(command));
        for (field, values) in &fields {
            if let Some(v) = values.get(i) {
                e.insert((*field).into(), v.clone());
            }
        }
        e.insert("isDefault".into(), Value::Bool(default_engine == i as i64));
        out.push(e);
    }
    out
}

/// Fields of an engine entry this product uses; the rest are listed as not migrated.
const ENGINE_FIELDS: [&str; 13] = [
    "id",
    "name",
    "command",
    "preload",
    "komi",
    "width",
    "height",
    "isDefault",
    "initialCommand",
    "useJavaSSH",
    "threadPolicy",
    "gtpConfigurationProtocol",
    "gtpConfigurationProfile",
];
/// Fields of Java's management of its own bundled KataGo package: they do not apply here.
const JAVA_PACKAGE_FIELDS: [&str; 2] = ["managedProfileType", "managedProfileCommand"];
/// SSH connection fields: never read for the report.
const SSH_FIELDS: [&str; 6] = ["ip", "port", "userName", "password", "useKeyGen", "keyGenPath"];
/// One Java engine entry (`Utils.engineDataFromJson` defaults).
fn engine(
    index: usize,
    e: &Map<String, Value>,
    is_default: bool,
    roots: &[PathBuf],
) -> (ImportEngineView, Option<EngineProfile>) {
    let text = |k: &str| e.get(k).map(shown).unwrap_or_default();
    let name = text("name");
    let original = text("command");
    let initial = text("initialCommand");
    let present = |k: &str, v: &Value| !v.is_null() && v.as_str() != Some("") && !SSH_FIELDS.contains(&k);
    let unmigrated_fields: Vec<String> = e
        .iter()
        .filter(|(k, v)| {
            !ENGINE_FIELDS.contains(&k.as_str()) && !JAVA_PACKAGE_FIELDS.contains(&k.as_str()) && present(k, v)
        })
        .map(|(k, _)| k.clone())
        .collect();
    let incompatible_fields: Vec<String> = e
        .iter()
        .filter(|(k, v)| JAVA_PACKAGE_FIELDS.contains(&k.as_str()) && present(k, v))
        .map(|(k, _)| k.clone())
        .collect();
    let mut view = ImportEngineView {
        index: index as u32,
        name: name.clone(),
        command: sanitize(&original),
        status: ImportStatus::Imported,
        reason: None,
        credential: false,
        is_default,
        problems: Vec::new(),
        unmigrated_fields,
        incompatible_fields,
    };
    if e.get("useJavaSSH").and_then(as_bool).unwrap_or(false) {
        view.status = ImportStatus::NotMigrated;
        view.reason = Some(ImportReason::RemoteEngine);
        view.credential = ["userName", "password", "keyGenPath"]
            .iter()
            .any(|f| e.get(*f).and_then(Value::as_str).is_some_and(|s| !s.is_empty()));
        return (view, None);
    }

    let mut problems = Vec::new();
    let size = |field: &str, problems: &mut Vec<EngineProblem>| match e.get(field) {
        None => 19,
        Some(v) => match as_i64(v).filter(|n| (2..=52).contains(n)) {
            Some(n) => n as u16,
            None => {
                problems.push(EngineProblem::InvalidValue { field: field.into() });
                19
            }
        },
    };
    let width = size("width", &mut problems);
    let height = size("height", &mut problems);
    let komi = match e.get("komi") {
        None => 7.5,
        Some(v) => as_f64(v).unwrap_or_else(|| {
            problems.push(EngineProblem::InvalidValue { field: "komi".into() });
            7.5
        }),
    };
    // `EngineThreadPolicy` keys are kept as Java wrote them; the environment fingerprint decides
    // at launch whether stored benchmark results still apply to the files used here.
    let thread_policy = match e.get("threadPolicy") {
        None | Some(Value::Null) => None,
        Some(v) => serde_json::from_value::<ThreadPolicy>(v.clone()).ok().or_else(|| {
            problems.push(EngineProblem::InvalidValue {
                field: "threadPolicy".into(),
            });
            None
        }),
    };
    // `Utils.engineDataFromJson` / `engineDataToJson`: both a protocol and a profile object.
    let protocol = e
        .get("gtpConfigurationProtocol")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let gtp_configuration = match e.get("gtpConfigurationProfile").and_then(Value::as_object) {
        Some(profile) if !protocol.is_empty() => Some(GtpConfiguration {
            protocol: protocol.to_string(),
            profile: profile.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        }),
        _ => None,
    };

    let command = resolve_command(&original, roots, &mut problems);
    view.command = sanitize(&command);
    view.credential = sanitize(&original) != original || sanitize(&initial) != initial;
    view.problems = problems;
    let profile = EngineProfile {
        id: text("id"),
        name,
        command,
        initial_command: initial,
        preload: e.get("preload").and_then(as_bool).unwrap_or(false),
        width,
        height,
        komi,
        is_default,
        thread_policy,
        gtp_configuration,
        ..EngineProfile::default()
    };
    (view, Some(profile))
}

/// The command with paths relative to the Java folders made absolute (as Java would launch it),
/// recording paths that do not exist. A command that needs no change is kept as written.
fn resolve_command(command: &str, roots: &[PathBuf], problems: &mut Vec<EngineProblem>) -> String {
    let Some(spec) = prepare_launch(command, roots) else {
        problems.push(EngineProblem::EmptyCommand);
        return command.to_string();
    };
    let mut tokens = vec![spec.program];
    tokens.extend(spec.args);

    let program = Path::new(&tokens[0]);
    let found = if program.is_absolute() {
        program.is_file()
    } else if tokens[0].contains(['/', '\\']) {
        false
    } else {
        // A bare name is looked up on PATH when the process starts.
        on_path(&tokens[0])
    };
    if !found {
        problems.push(EngineProblem::ProgramNotFound {
            path: sanitize(&tokens[0]),
        });
    }
    for w in tokens.windows(2) {
        if FILE_PATH_OPTIONS.contains(&w[0].as_str()) && !w[1].starts_with('-') && !Path::new(&w[1]).exists() {
            problems.push(EngineProblem::FileNotFound {
                option: w[0].clone(),
                path: sanitize(&w[1]),
            });
        }
    }
    if tokens == split_command(command.trim()) {
        return command.to_string();
    }
    join_command(&tokens).unwrap_or_else(|| command.to_string())
}

fn on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let suffixes: Vec<String> = if cfg!(windows) {
        let ext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.BAT;.CMD".into());
        std::iter::once(String::new())
            .chain(ext.split(';').map(str::to_string))
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&paths).any(|dir| suffixes.iter().any(|s| dir.join(format!("{name}{s}")).is_file()))
}

impl Mapped {
    /// This product's settings after the import. Engines with credentials are included only when
    /// their Java index is in `copy_credentials`.
    pub fn apply(&self, config: &Config, persisted: &Persisted, copy_credentials: &[u32]) -> (Config, Persisted) {
        let mut c = config.clone();
        let mut p = persisted.clone();
        macro_rules! set {
            ($field:expr, $value:expr) => {
                if let Some(v) = &$value {
                    $field = v.clone();
                }
            };
        }
        set!(c.record.read_komi, self.read_komi);
        set!(c.record.load_sgf_last, self.load_sgf_last);
        set!(c.record.show_replace_file_hint, self.show_replace_file_hint);
        set!(c.record.append_winrate_to_comment, self.append_winrate_to_comment);
        set!(c.ui.language, self.language);
        set!(c.ui.play_sound, self.play_sound);
        set!(c.ui.extra_mode, self.extra_mode);
        if let Ok(mut values) = serde_json::to_value(&c.panels) {
            for value in &self.panels { value.set(&mut values); }
            c.panels = serde_json::from_value(values).unwrap_or(c.panels);
        }
        // Config.loadPanelModeSettings (3907-3912) applies this after loading the flags.
        if c.ui.extra_mode.hides_comment() { c.panels.show_comment = false; }
        for (index, layout) in self.custom_layouts.iter().enumerate() {
            if let Some(layout) = layout { c.custom_layouts[index] = Some(layout.clone()); }
        }
        set!(p.independent_main_board, self.independent_main_board.map(Some));
        set!(p.independent_sub_board, self.independent_sub_board.map(Some));
        if let Ok(mut values) = serde_json::to_value(p.panel_sizes) {
            for value in &self.panel_sizes { value.set(&mut values); }
            p.panel_sizes = serde_json::from_value(values).unwrap_or(p.panel_sizes);
        }
        set!(p.bottom_toolbar, self.bottom_toolbar);
        let mut game = serde_json::to_value(&c.game).expect("settings serialize");
        for value in &self.game { value.set(&mut game); }
        c.game = serde_json::from_value(game).expect("typed imported game values");
        let mut autoplay = serde_json::to_value(p.autoplay).expect("settings serialize");
        for value in &self.autoplay { value.set(&mut autoplay); }
        p.autoplay = serde_json::from_value(autoplay).expect("typed imported autoplay values");
        let mut matches = serde_json::to_value(&c.matches).expect("settings serialize");
        for value in &self.matches { value.set(&mut matches); }
        c.matches = serde_json::from_value(matches).expect("typed imported match values");
        let mut toolbar = serde_json::to_value(&p.matches).expect("settings serialize");
        for value in &self.match_toolbar { value.set(&mut toolbar); }
        p.matches = serde_json::from_value(toolbar).expect("typed imported match toolbar values");
        set!(c.recent_files, self.recent_files);
        if let Some(folder) = &self.last_folder {
            p.last_folder = Some(folder.clone());
        }
        if let Some(folder) = &self.last_image_folder {
            p.last_image_folder = Some(folder.clone());
        }
        set!(c.recovery.auto_save_on_exit, self.auto_save_on_exit);
        set!(c.recovery.resume_previous_game, self.resume_previous_game);
        set!(c.recovery.zoom_thumbnails, self.zoom_thumbnails);
        set!(c.engine.analysis.interval_centisec, self.interval_centisec);
        set!(c.engine.analysis.limit_time, self.limit_time);
        set!(
            c.engine.analysis.max_analyze_time_seconds,
            self.max_analyze_time_seconds
        );
        set!(c.engine.analysis.limit_playout, self.limit_playout);
        set!(c.engine.analysis.limit_playouts, self.limit_playouts);
        set!(c.engine.analysis.limit_max_suggestion, self.limit_max_suggestion);
        set!(c.engine.analysis.limit_branch_length, self.limit_branch_length);
        set!(c.engine.analysis.fast_engine_change, self.fast_engine_change);
        set!(
            c.engine.analysis.show_ponder_limited_tips,
            self.show_ponder_limited_tips
        );
        set!(c.engine.kata_rules, self.kata_rules);
        set!(
            c.engine.startup_benchmark_dismissed_signature,
            self.startup_benchmark_dismissed_signature
        );
        set!(
            c.engine.startup_benchmark_dismissed_version,
            self.startup_benchmark_dismissed_version
        );
        set!(c.engine.auto_load_kata_rules, self.auto_load_kata_rules);
        set!(c.engine.startup_benchmark, self.startup_benchmark);
        set!(c.network, self.network);
        set!(c.commentary, self.commentary);
        set!(c.edit.allow_double_click, self.allow_double_click);
        set!(c.edit.allow_drag, self.allow_drag);
        set!(c.edit.show_right_menu, self.show_right_menu);
        set!(c.edit.show_new_board_hint, self.show_new_board_hint);
        set!(c.edit.show_markup_tools, self.show_markup_tools);
        set!(c.edit.other_size_width, self.other_size_width);
        set!(c.edit.other_size_height, self.other_size_height);
        set!(c.edit.no_capture, self.no_capture);
        set!(c.display.winrate_always_black, self.winrate_always_black);
        set!(c.display.score_lead_with_komi, self.score_lead_with_komi);
        c.display = with_display(&c.display, &self.display);
        set!(c.record.auto_quick_analyze_on_load, self.auto_quick_analyze_on_load);
        let ae = &mut c.analysis_engine;
        set!(ae.command, self.analysis_command);
        set!(ae.command_customized, self.analysis_command_customized);
        set!(ae.max_visits, self.analysis_max_visits);
        set!(ae.use_current_rules, self.analysis_use_current_rules);
        set!(ae.specific_rules, self.analysis_specific_rules);
        set!(ae.always_override, self.analysis_always_override);
        set!(ae.preload, self.analysis_preload);
        set!(ae.auto_quit, self.analysis_auto_quit);
        set!(ae.reuse_current_engine, self.analysis_reuse_current_engine);
        set!(ae.batch_visits, self.analysis_batch_visits);
        set!(c.flash_range.start_move, self.flash_start_move);
        set!(c.flash_range.end_move, self.flash_end_move);
        let mut auto = serde_json::to_value(&c.auto_analysis).expect("settings serialize");
        for value in &self.auto_analysis { value.set(&mut auto); }
        c.auto_analysis = serde_json::from_value(auto).expect("typed imported auto-analysis values");
        set!(c.whole_game.deep_visits, self.whole_game_visits);
        set!(c.region.allow_moves, self.region_allow_moves);
        set!(c.region.avoid_moves, self.region_avoid_moves);
        set!(c.region.allow_custom_moves, self.region_allow_custom_moves);
        set!(c.region.avoid_custom_moves, self.region_avoid_custom_moves);
        set!(c.tracking.max_visits, self.tracking_max_visits);
        set!(c.tracking.show_outline, self.tracking_show_outline);
        set!(c.tracking.outline_opacity, self.tracking_outline_opacity);
        set!(c.quick_model.enabled, self.quick_model_enabled);
        if let Some(path) = &self.quick_model_path {
            c.quick_model.path = (!path.trim().is_empty()).then(|| path.clone());
        }
        if let Some(training) = &self.human_sl_training {
            c.human_sl.training = training.clone().normalized();
        }
        set!(c.human_sl.advanced_visible, self.human_sl_advanced_visible);
        if let Some(path) = &self.human_sl_model_path {
            c.human_sl.model_path = (!path.trim().is_empty()).then(|| path.clone());
        }
        set!(c.strength.humansl_background, self.strength_background);
        let ts = &mut c.tsumego;
        set!(ts.frame.to_play, self.tsumego_to_play);
        set!(ts.frame.attacker, self.tsumego_attacker);
        set!(ts.frame.ko_threat, self.tsumego_ko_threat);
        set!(ts.frame.wall_distance, self.tsumego_wall_distance);
        set!(ts.show_button, self.tsumego_show_button);
        set!(ts.capture.black_offset, self.capture_black_offset);
        set!(ts.capture.black_percent, self.capture_black_percent);
        set!(ts.capture.white_offset, self.capture_white_offset);
        set!(ts.capture.white_percent, self.capture_white_percent);
        set!(ts.capture.gray_offset, self.capture_gray_offset);
        let pl = &mut c.problems;
        set!(pl.showing, self.problems_showing);
        set!(pl.side, self.problem_side);
        set!(pl.sort, self.problem_sort);
        set!(pl.winrate_threshold, self.problem_winrate_threshold);
        set!(pl.playouts_threshold, self.problem_playouts_threshold);
        set!(pl.hide_control_pane, self.problem_hide_control_pane);
        c.hawkeye = with_hawkeye(&c.hawkeye, self);
        let mut appearance = serde_json::to_value(&c.appearance).expect("appearance serializes");
        for (field, value) in &self.appearance { appearance[*field] = value.clone(); }
        c.appearance = serde_json::from_value(appearance).expect("validated appearance values");
        let mut theme = serde_json::to_value(&c.appearance_defaults).expect("theme serializes");
        for (field, value) in &self.theme { theme[*field] = value.clone(); }
        c.appearance_defaults = serde_json::from_value(theme).expect("validated theme values");
        if let Some(engines) = &self.engines {
            let kept: Vec<&MappedEngine> = engines
                .iter()
                .filter(|m| !m.credential || copy_credentials.contains(&(m.java_index as u32)))
                .collect();
            c.engine.engines = kept.iter().map(|m| m.profile.clone()).collect();
            c.engine.last_engine = self
                .last_engine
                .and_then(|i| kept.iter().find(|m| m.java_index == i))
                .map(|m| m.profile.id.clone());
            set!(c.engine.startup, self.startup);
        }
        (c, p)
    }

    /// Settings of this product the import would change, once this product has settings of its
    /// own (`has_settings`; on first use there is nothing to protect). Any of them may have been
    /// chosen, even at its default value. An empty engine list and a missing last folder hold
    /// no choice.
    pub fn replaces(&self, config: &Config, persisted: &Persisted, has_settings: bool) -> Vec<ImportTarget> {
        let mut out = Vec::new();
        if let Some(folder) = &self.last_folder {
            if persisted.last_folder.as_ref().is_some_and(|f| f != folder) {
                out.push(ImportTarget::LastFolder);
            }
        }
        if let Some(folder) = &self.last_image_folder {
            if persisted.last_image_folder.as_ref().is_some_and(|f| f != folder) {
                out.push(ImportTarget::LastImageFolder);
            }
        }
        if let Ok(current) = serde_json::to_value(persisted.panel_sizes) {
            if self.panel_sizes.iter().any(|v| v.current(&current).is_none_or(|c| c != v.value)) {
                out.push(ImportTarget::PanelSizes);
            }
        }
        if self.bottom_toolbar.is_some_and(|v| v != persisted.bottom_toolbar) {
            out.push(ImportTarget::BottomToolbar);
        }
        if let Ok(current) = serde_json::to_value(persisted.autoplay) {
            if self.autoplay.iter().any(|v| v.current(&current).is_none_or(|c| c != v.value)) {
                out.push(ImportTarget::AutoPlay);
            }
        }
        if self.custom_layouts.iter().enumerate().any(|(i, layout)| layout.is_some() && layout != &config.custom_layouts[i])
            || self.independent_main_board.is_some_and(|v| Some(v) != persisted.independent_main_board)
            || self.independent_sub_board.is_some_and(|v| Some(v) != persisted.independent_sub_board) {
            out.push(ImportTarget::WorkbenchPanels);
        }
        let matches = serde_json::to_value(&config.matches).expect("settings serialize");
        let toolbar = serde_json::to_value(&persisted.matches).expect("settings serialize");
        if self.matches.iter().any(|v|v.current(&matches).is_none_or(|c|c!=v.value))
            || self.match_toolbar.iter().any(|v|v.current(&toolbar).is_none_or(|c|c!=v.value)) {
            out.push(ImportTarget::EngineMatch);
        }
        if !has_settings {
            return out;
        }
        macro_rules! cmp {
            ($target:expr, $current:expr, $new:expr) => {
                if $new.as_ref().is_some_and(|v| *v != $current) {
                    out.push($target);
                }
            };
        }
        let a_json = serde_json::to_value(&config.appearance).expect("appearance serializes");
        let t_json = serde_json::to_value(&config.appearance_defaults).expect("theme serializes");
        if self.appearance.iter().any(|(field,v)| a_json.get(*field) != Some(v))
            || self.theme.iter().any(|(field,v)| t_json.get(*field) != Some(v)) {
            out.push(ImportTarget::Appearance);
        }
        let a = &config.engine.analysis;
        cmp!(ImportTarget::ReadKomi, config.record.read_komi, self.read_komi);
        let e = &config.edit;
        cmp!(
            ImportTarget::AllowDoubleClick,
            e.allow_double_click,
            self.allow_double_click
        );
        cmp!(ImportTarget::AllowDrag, e.allow_drag, self.allow_drag);
        cmp!(ImportTarget::RightClickMenu, e.show_right_menu, self.show_right_menu);
        cmp!(
            ImportTarget::NewBoardHint,
            e.show_new_board_hint,
            self.show_new_board_hint
        );
        cmp!(ImportTarget::MarkupTools, e.show_markup_tools, self.show_markup_tools);
        if self.other_size_width.is_some_and(|v| v != e.other_size_width)
            || self.other_size_height.is_some_and(|v| v != e.other_size_height)
        {
            out.push(ImportTarget::OtherBoardSize);
        }
        cmp!(ImportTarget::NoCapture, e.no_capture, self.no_capture);
        let d = &config.display;
        cmp!(
            ImportTarget::WinrateAlwaysBlack,
            d.winrate_always_black,
            self.winrate_always_black
        );
        cmp!(
            ImportTarget::ScoreLeadWithKomi,
            d.score_lead_with_komi,
            self.score_lead_with_komi
        );
        if let Ok(current) = serde_json::to_value(d) {
            for v in &self.display {
                if v.current(&current).is_none_or(|c| c != v.value) && !out.contains(&v.target) {
                    out.push(v.target);
                }
            }
        }
        if let Ok(current) = serde_json::to_value(&config.game) {
            for value in &self.game {
                if value.current(&current).is_none_or(|c| c != value.value) && !out.contains(&value.target) {
                    out.push(value.target);
                }
            }
        }
        if let Ok(current) = serde_json::to_value(&config.panels) {
            if self.panels.iter().any(|v| v.current(&current).is_none_or(|c| c != v.value)) {
                out.push(ImportTarget::WorkbenchPanels);
            }
        }
        let r = &config.recovery;
        cmp!(
            ImportTarget::AutoSaveOnExit,
            r.auto_save_on_exit,
            self.auto_save_on_exit
        );
        cmp!(
            ImportTarget::ResumePreviousGame,
            r.resume_previous_game,
            self.resume_previous_game
        );
        cmp!(ImportTarget::ZoomThumbnails, r.zoom_thumbnails, self.zoom_thumbnails);
        cmp!(
            ImportTarget::LoadSgfLast,
            config.record.load_sgf_last,
            self.load_sgf_last
        );
        cmp!(
            ImportTarget::ReplaceFileHint,
            config.record.show_replace_file_hint,
            self.show_replace_file_hint
        );
        cmp!(
            ImportTarget::AppendWinrateToComment,
            config.record.append_winrate_to_comment,
            self.append_winrate_to_comment
        );
        cmp!(ImportTarget::Language, config.ui.language, self.language);
        cmp!(ImportTarget::PlaySound, config.ui.play_sound, self.play_sound);
        cmp!(ImportTarget::ExtraMode, config.ui.extra_mode, self.extra_mode);
        cmp!(ImportTarget::RecentFiles, config.recent_files, self.recent_files);
        cmp!(
            ImportTarget::AnalysisInterval,
            a.interval_centisec,
            self.interval_centisec
        );
        cmp!(ImportTarget::LimitTime, a.limit_time, self.limit_time);
        cmp!(
            ImportTarget::MaxAnalyzeTime,
            a.max_analyze_time_seconds,
            self.max_analyze_time_seconds
        );
        cmp!(ImportTarget::LimitPlayout, a.limit_playout, self.limit_playout);
        cmp!(ImportTarget::LimitPlayouts, a.limit_playouts, self.limit_playouts);
        cmp!(
            ImportTarget::LimitMaxSuggestion,
            a.limit_max_suggestion,
            self.limit_max_suggestion
        );
        cmp!(
            ImportTarget::LimitBranchLength,
            a.limit_branch_length,
            self.limit_branch_length
        );
        cmp!(
            ImportTarget::FastEngineChange,
            a.fast_engine_change,
            self.fast_engine_change
        );
        cmp!(
            ImportTarget::PonderLimitTips,
            a.show_ponder_limited_tips,
            self.show_ponder_limited_tips
        );
        cmp!(ImportTarget::KataRules, config.engine.kata_rules, self.kata_rules);
        cmp!(
            ImportTarget::AutoLoadKataRules,
            config.engine.auto_load_kata_rules,
            self.auto_load_kata_rules
        );
        cmp!(
            ImportTarget::StartupBenchmark,
            config.engine.startup_benchmark,
            self.startup_benchmark
        );
        cmp!(ImportTarget::NetworkProxy, config.network, self.network);
        cmp!(ImportTarget::Commentary, config.commentary, self.commentary);
        cmp!(
            ImportTarget::AutoQuickAnalysis,
            config.record.auto_quick_analyze_on_load,
            self.auto_quick_analyze_on_load
        );
        let ae = &config.analysis_engine;
        let analysis_engine = [
            self.analysis_command.as_ref().is_some_and(|v| *v != ae.command),
            self.analysis_command_customized
                .is_some_and(|v| v != ae.command_customized),
            self.analysis_max_visits.is_some_and(|v| v != ae.max_visits),
            self.analysis_use_current_rules
                .is_some_and(|v| v != ae.use_current_rules),
            self.analysis_specific_rules
                .as_ref()
                .is_some_and(|v| *v != ae.specific_rules),
            self.analysis_always_override.is_some_and(|v| v != ae.always_override),
            self.analysis_preload.is_some_and(|v| v != ae.preload),
            self.analysis_auto_quit.is_some_and(|v| v != ae.auto_quit),
            self.analysis_reuse_current_engine
                .is_some_and(|v| v != ae.reuse_current_engine),
            self.analysis_batch_visits.is_some_and(|v| v != ae.batch_visits),
        ];
        if analysis_engine.contains(&true) {
            out.push(ImportTarget::AnalysisEngine);
        }
        if self.flash_start_move.is_some_and(|v| v != config.flash_range.start_move)
            || self.flash_end_move.is_some_and(|v| v != config.flash_range.end_move)
        {
            out.push(ImportTarget::FlashRange);
        }
        let auto = serde_json::to_value(&config.auto_analysis).expect("settings serialize");
        if self.auto_analysis.iter().any(|v| v.current(&auto).as_ref() != Some(&v.value)) {
            out.push(ImportTarget::AutoAnalysis);
        }
        if self.whole_game_visits.is_some_and(|v| v != config.whole_game.deep_visits) {
            out.push(ImportTarget::WholeGameVisits);
        }
        let region = &config.region;
        if self.region_allow_moves.is_some_and(|v| v != region.allow_moves)
            || self.region_avoid_moves.is_some_and(|v| v != region.avoid_moves)
            || self.region_allow_custom_moves.is_some_and(|v| v != region.allow_custom_moves)
            || self.region_avoid_custom_moves.is_some_and(|v| v != region.avoid_custom_moves)
        {
            out.push(ImportTarget::AnalysisRegion);
        }
        let tracking = &config.tracking;
        if self.tracking_max_visits.is_some_and(|v| v != tracking.max_visits)
            || self.tracking_show_outline.is_some_and(|v| v != tracking.show_outline)
            || self.tracking_outline_opacity.is_some_and(|v| v != tracking.outline_opacity)
        {
            out.push(ImportTarget::Tracking);
        }
        let qm = &config.quick_model;
        if self.quick_model_enabled.is_some_and(|v| v != qm.enabled)
            || self
                .quick_model_path
                .as_ref()
                .is_some_and(|v| qm.path.as_ref() != Some(v))
        {
            out.push(ImportTarget::QuickModel);
        }
        if self.human_sl_training.as_ref().is_some_and(|v| v != &config.human_sl.training)
            || self.human_sl_advanced_visible.is_some_and(|v| v != config.human_sl.advanced_visible)
        {
            out.push(ImportTarget::HumanSlTraining);
        }
        if self.human_sl_model_path.as_ref().is_some_and(|v| config.human_sl.model_path.as_ref() != Some(v)) {
            out.push(ImportTarget::HumanSlModel);
        }
        if self.strength_background.is_some_and(|v| v != config.strength.humansl_background) {
            out.push(ImportTarget::StrengthBackground);
        }
        let ts = &config.tsumego;
        if self.tsumego_to_play.is_some_and(|v| v != ts.frame.to_play)
            || self.tsumego_attacker.is_some_and(|v| v != ts.frame.attacker)
            || self.tsumego_ko_threat.is_some_and(|v| v != ts.frame.ko_threat)
            || self.tsumego_wall_distance.is_some_and(|v| v != ts.frame.wall_distance)
            || self.tsumego_show_button.is_some_and(|v| v != ts.show_button)
        {
            out.push(ImportTarget::Tsumego);
        }
        let cs = &ts.capture;
        if self.capture_black_offset.is_some_and(|v| v != cs.black_offset)
            || self.capture_black_percent.is_some_and(|v| v != cs.black_percent)
            || self.capture_white_offset.is_some_and(|v| v != cs.white_offset)
            || self.capture_white_percent.is_some_and(|v| v != cs.white_percent)
            || self.capture_gray_offset.is_some_and(|v| v != cs.gray_offset)
        {
            out.push(ImportTarget::CaptureTsumego);
        }
        let pl = &config.problems;
        let problems = [
            self.problems_showing.is_some_and(|v| v != pl.showing),
            self.problem_side.is_some_and(|v| v != pl.side),
            self.problem_sort.is_some_and(|v| v != pl.sort),
            self.problem_winrate_threshold
                .is_some_and(|v| v != pl.winrate_threshold),
            self.problem_playouts_threshold
                .is_some_and(|v| v != pl.playouts_threshold),
            self.problem_hide_control_pane
                .is_some_and(|v| v != pl.hide_control_pane),
        ];
        if problems.contains(&true) {
            out.push(ImportTarget::ProblemList);
        }
        if with_hawkeye(&config.hawkeye, self) != config.hawkeye {
            out.push(ImportTarget::Hawkeye);
        }
        if let Some(engines) = &self.engines {
            let new: Vec<&EngineProfile> = engines.iter().map(|m| &m.profile).collect();
            let current: Vec<&EngineProfile> = config.engine.engines.iter().collect();
            if !current.is_empty() && current != new {
                out.push(ImportTarget::Engines);
            }
            cmp!(ImportTarget::EngineStartup, config.engine.startup, self.startup);
            // The engine used last, as `apply` sets it when every credential is copied.
            let last = self
                .last_engine
                .and_then(|i| engines.iter().find(|m| m.java_index == i))
                .map(|m| m.profile.id.as_str());
            if config.engine.last_engine.as_deref() != last {
                out.push(ImportTarget::LastEngine);
            }
        }
        out
    }
}

fn read_extra_mode(value: &Value) -> ExtraMode {
    let text = value.as_str().map(str::trim);
    let number = as_i64(value).or_else(|| text.and_then(|t| t.parse().ok()));
    match (number, text) {
        (Some(1), _) | (None, Some("Four_Sub")) => ExtraMode::FourSub,
        (Some(2), _) | (None, Some("Double_Engine")) => ExtraMode::DoubleEngine,
        (Some(3), _) | (None, Some("Thinking")) => ExtraMode::Thinking,
        (Some(7), _) | (None, Some("Min")) => ExtraMode::Min,
        (Some(8), _) | (None, Some("Float_Board")) => ExtraMode::FloatBoard,
        _ => ExtraMode::Normal,
    }
}

fn java_window(value: &Value) -> Option<crate::settings::WindowPlacement> {
    let a = value.as_array().filter(|a| a.len() == 4)?;
    Some(crate::settings::WindowPlacement {
        x: i32::try_from(as_i64(&a[0])?).ok()?,
        y: i32::try_from(as_i64(&a[1])?).ok()?,
        width: u32::try_from(as_i64(&a[2])?).ok()?,
        height: u32::try_from(as_i64(&a[3])?).ok()?,
        maximized: false,
    })
}

fn custom_layout(value: &Value) -> Option<crate::settings::CustomLayout> {
    let o = value.as_object()?;
    // JSONObject.optBoolean/optInt use false/0 when absent, not the new-profile defaults.
    let flag = |key: &str| o.get(key).and_then(as_bool).unwrap_or(false);
    let panels = crate::settings::PanelSettings {
        show_sub_board: flag("show-subboard"),
        show_winrate_graph: flag("show-winrate-graph"),
        show_comment: flag("show-comment"),
        show_status: flag("show-status"),
        show_variation_graph: flag("show-variation-graph"),
        show_captured: flag("show-captured"),
        show_list_pane: flag("show-list-pane"),
        large_sub_board: flag("large-subboard"),
        large_winrate_graph: flag("large-winrate-graph"),
        showing_independent_main: flag("independent-main-board"),
        showing_independent_sub: flag("independent-sub-board"),
        ..Default::default()
    };
    let window = o.get("main-frame-position").and_then(java_window).map(|mut p| {
        p.maximized = flag("main-frame-maxsize"); p
    }).or_else(|| flag("main-frame-maxsize").then_some(crate::settings::WindowPlacement {
        x: 0, y: 0, width: 0, height: 0, maximized: true,
    }));
    Some(crate::settings::CustomLayout {
        panels,
        board_position_proportion: o.get("board-position-proportion").and_then(as_i64).unwrap_or(0).clamp(0, 8) as u8,
        window,
        independent_main: o.get("independent-main-board-position").and_then(java_window),
        independent_sub: o.get("independent-sub-board-position").and_then(java_window),
    })
}

impl Reader<'_> {
    /// TeacherSettings.load at the fixed Java baseline, without its source-file rewrite.
    fn teacher_settings(&mut self) -> bool {
        use crate::commentary::settings::{self, CommentarySettings, Preferences, Provider};
        let file = self.source.config_dir.join("teacher.properties");
        if !file.is_file() { return false; }
        let properties = match teacher_properties(&file) {
            Ok(properties) => properties,
            Err(()) => {
                let item = self.item("file:teacher.properties".into(), ImportStatus::Incompatible);
                item.reason = Some(ImportReason::Unreadable);
                return false;
            }
        };
        let get = |name: &str, default: &str| properties.get(name).cloned().unwrap_or_else(|| default.into());
        let provider = match get("provider", "API_KEY").as_str() {
            "API_KEY" => Provider::ApiKey, "CHATGPT" => Provider::ChatGpt, _ => Provider::Unselected,
        };
        let mut base_url = get("baseUrl", "");
        if base_url.trim().is_empty() {
            base_url = if properties.contains_key("provider") { String::new() } else { "https://api.openai.com/v1".into() };
        }
        let model = get("model", "");
        let valid = (|| {
            let base_url = if base_url.is_empty() { base_url } else { settings::validate_base_url(&base_url)? };
            let model = if model.trim().is_empty() { String::new() } else { settings::validate_model(&model)? };
            Ok::<_, crate::commentary::CommentaryFailure>((base_url, model))
        })();
        let (base_url, model) = match valid {
            Ok(values) => values,
            Err(_) => {
                let item = self.item("file:teacher.properties".into(), ImportStatus::Incompatible);
                item.reason = Some(ImportReason::InvalidValue);
                return false;
            }
        };
        let rank_mode = if get("teacher.rankMode", "k") == "d" { "d" } else { "k" };
        let number = |name: &str, low: i32, high: i32, default: i32| {
            properties.get(name).and_then(|v| v.parse::<i32>().ok()).map(|v| v.clamp(low, high)).unwrap_or(default)
        };
        let legacy_key = properties.get("apiKey").is_some_and(|key| !key.trim().is_empty());
        self.mapped.commentary = Some(CommentarySettings {
            provider, base_url, model,
            // The key itself lives in Java's credential store; remembering it here starts only after
            // the separately confirmed copy into this client's store succeeds.
            remember_api_key: false,
            preferences: Preferences { rank_mode: rank_mode.into(), rank_num: number("teacher.rankNum", 1, if rank_mode == "d" { 9 } else { 18 }, 5),
                style_index: number("teacher.styleIndex", 0, 4, 0), density_index: number("teacher.densityIndex", 0, 2, 1),
                pace_index: number("teacher.paceIndex", 0, 2, 1), variation_index: number("teacher.variationIndex", 0, 2, 1) },
        });
        let item = self.item("file:teacher.properties".into(), ImportStatus::Imported);
        item.target = Some(ImportTarget::Commentary);
        if provider == Provider::ChatGpt {
            let item = self.item("credential:chatgpt-session".into(), ImportStatus::NotMigrated);
            item.reason = Some(ImportReason::Credential);
            item.credential = true;
            item.detail = Some("ChatGPT sessions are not migrated; sign in again.".into());
        }
        legacy_key
    }
}

/// Read only after the adapter has obtained explicit API-key import consent. This value must
/// never be retained in Candidate, a report, configuration, or a workbench projection.
pub fn read_java_commentary_api_key(directory: &Path) -> Option<String> {
    teacher_properties(&directory.join("teacher.properties")).ok()?.remove("apiKey")
        .map(|key| key.trim().to_owned()).filter(|key| !key.is_empty())
}

/// java.util.Properties.load(InputStream): Latin-1, continuations and Unicode escapes.
fn teacher_properties(path: &Path) -> Result<std::collections::BTreeMap<String, String>, ()> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(|_| ())?.take(1_048_577).read_to_end(&mut bytes).map_err(|_| ())?;
    if bytes.len() > 1_048_576 { return Err(()); }
    let text: String = bytes.iter().map(|byte| char::from(*byte)).collect();
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut properties = std::collections::BTreeMap::new();
    let mut logical = String::new();
    let mut continued = false;
    for line in text.split('\n') {
        let line = line.trim_start_matches([' ', '\t', '\u{c}']);
        if !continued && (line.is_empty() || line.starts_with(['#', '!'])) { continue; }
        logical.push_str(line);
        let slashes = logical.as_bytes().iter().rev().take_while(|byte| **byte == b'\\').count();
        continued = slashes % 2 == 1;
        if continued { logical.pop(); continue; }
        let (key, value) = property_pair(&logical)?;
        properties.insert(key, value);
        logical.clear();
    }
    if !logical.is_empty() {
        let (key, value) = property_pair(&logical)?;
        properties.insert(key, value);
    }
    Ok(properties)
}
fn property_pair(logical: &str) -> Result<(String, String), ()> {
    let mut escaped = false;
    let mut split = logical.len();
    for (index, character) in logical.char_indices() {
        if !escaped && matches!(character, '=' | ':' | ' ' | '\t' | '\u{c}') { split = index; break; }
        escaped = !escaped && character == '\\';
    }
    let mut value = logical[split..].trim_start_matches([' ', '\t', '\u{c}']);
    if value.starts_with(['=', ':']) { value = &value[1..]; }
    value = value.trim_start_matches([' ', '\t', '\u{c}']);
    Ok((property_unescape(&logical[..split])?, property_unescape(value)?))
}
fn property_unescape(text: &str) -> Result<String, ()> {
    let mut units = Vec::new();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        let character = if character == '\\' {
            match chars.next() {
                Some('u') => {
                    let mut unit = 0u16;
                    for _ in 0..4 { unit = (unit << 4) | chars.next().and_then(|c| c.to_digit(16)).ok_or(())? as u16; }
                    units.push(unit); continue;
                }
                Some('t') => '\t', Some('r') => '\r', Some('n') => '\n', Some('f') => '\u{c}',
                Some(character) => character, None => break,
            }
        } else { character };
        let mut encoded = [0; 2];
        units.extend_from_slice(character.encode_utf16(&mut encoded));
    }
    Ok(String::from_utf16_lossy(&units))
}
