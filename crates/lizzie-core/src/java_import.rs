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

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::api::{
    EngineProblem, ImportEngineView, ImportFailure, ImportItemView, ImportReason, ImportStatus, ImportTarget,
};
use crate::engine::gtp::{prepare_launch, split_command, FILE_PATH_OPTIONS};
use crate::engine::profile::{new_profile_id, EngineProfile, EngineStartup};
use crate::sanitize::sanitize;
use crate::settings::{Config, Language, Persisted, RECENT_LIMIT};

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
    pub language: Option<Language>,
    pub play_sound: Option<bool>,
    pub recent_files: Option<Vec<String>>,
    pub last_folder: Option<String>,
    /// Local engines in Java order (remote ones are not included).
    pub engines: Option<Vec<MappedEngine>>,
    pub startup: Option<EngineStartup>,
    /// Java index of the engine used last.
    pub last_engine: Option<usize>,
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
}

impl Candidate {
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
    Ok(Candidate {
        source: source.clone(),
        config_file,
        persist_file,
        mapped: r.mapped,
        items: r.items,
        engines: r.engines,
    })
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
    Str(fn(&mut Mapped, String)),
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

    fn imported(&mut self, key: String, target: ImportTarget, value: String) {
        let it = self.item(key, ImportStatus::Imported);
        it.target = Some(target);
        it.value = Some(value);
    }

    fn invalid(&mut self, key: String, target: ImportTarget, value: &Value) {
        let it = self.item(key, ImportStatus::Incompatible);
        it.target = Some(target);
        it.value = Some(shown(value));
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
                Kind::Str(set) => v.as_str().map(|s| {
                    set(&mut self.mapped, s.to_string());
                    s.to_string()
                }),
            };
            match value {
                Some(value) => self.imported(name, *target, value),
                None => self.invalid(name, *target, v),
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
                        it.value = Some(shown(v));
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
                self.not_migrated(format!("persist:{section}"), value);
                continue;
            };
            for (key, v) in map {
                let name = format!("persist:{section}.{key}");
                match (section.as_str(), key.as_str()) {
                    ("filesystem", "last-folder") => match v {
                        // Java's default is "" (no folder yet).
                        Value::String(s) if s.is_empty() => {}
                        Value::String(s) => {
                            self.mapped.last_folder = Some(s.clone());
                            self.imported(name, ImportTarget::LastFolder, s.clone());
                        }
                        other => self.invalid(name, ImportTarget::LastFolder, other),
                    },
                    ("ui-persist", "main-window-position" | "window-maximized") => {
                        let it = self.item(name, ImportStatus::Incompatible);
                        it.value = Some(shown(v));
                        it.reason = Some(ImportReason::WindowCoordinates);
                    }
                    _ => self.not_migrated(name, v),
                }
            }
        }
    }

    /// Other Java data next to the configuration: reported, never read.
    fn other_files(&mut self) {
        let dir = self.source.config_dir.clone();
        for (name, credential) in [
            ("save", false),
            ("teacher.properties", true),
            ("secure-credentials", true),
        ] {
            if dir.join(name).exists() {
                let it = self.item(format!("file:{name}"), ImportStatus::NotMigrated);
                it.credential = credential;
                it.reason = Some(if credential {
                    ImportReason::Credential
                } else {
                    ImportReason::NotYetAvailable
                });
            }
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
const ENGINE_FIELDS: [&str; 10] = [
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
];
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
    let unmigrated_fields: Vec<String> = e
        .iter()
        .filter(|(k, v)| {
            !ENGINE_FIELDS.contains(&k.as_str())
                && !SSH_FIELDS.contains(&k.as_str())
                && !v.is_null()
                && v.as_str() != Some("")
        })
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
            path: tokens[0].clone(),
        });
    }
    for w in tokens.windows(2) {
        if FILE_PATH_OPTIONS.contains(&w[0].as_str()) && !w[1].starts_with('-') && !Path::new(&w[1]).exists() {
            problems.push(EngineProblem::FileNotFound {
                option: w[0].clone(),
                path: w[1].clone(),
            });
        }
    }
    if tokens == split_command(command.trim()) {
        return command.to_string();
    }
    join_command(&tokens).unwrap_or_else(|| command.to_string())
}

/// The inverse of `Utils.splitCommand`: tokens with spaces or quotes are quoted. `None` when a
/// token holds both kinds of quote, which the Java syntax cannot express.
fn join_command(tokens: &[String]) -> Option<String> {
    let mut out = Vec::with_capacity(tokens.len());
    for t in tokens {
        let needs_quotes = t.is_empty() || t.contains([' ', '"', '\'']);
        out.push(match (needs_quotes, t.contains('"'), t.contains('\'')) {
            (false, _, _) => t.clone(),
            (true, false, _) => format!("\"{t}\""),
            (true, true, false) => format!("'{t}'"),
            (true, true, true) => return None,
        });
    }
    Some(out.join(" "))
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
        set!(c.ui.language, self.language);
        set!(c.ui.play_sound, self.play_sound);
        set!(c.recent_files, self.recent_files);
        if let Some(folder) = &self.last_folder {
            p.last_folder = Some(folder.clone());
        }
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
        set!(c.engine.auto_load_kata_rules, self.auto_load_kata_rules);
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

    /// Settings already chosen in this product (different from its defaults) that the import
    /// would change.
    pub fn replaces(&self, config: &Config, persisted: &Persisted) -> Vec<ImportTarget> {
        let d = Config::default();
        let mut out = Vec::new();
        macro_rules! cmp {
            ($target:expr, $field:ident . $($rest:ident).+, $new:expr) => {
                if let Some(v) = &$new {
                    let current = &config.$field.$($rest).+;
                    if *current != d.$field.$($rest).+ && current != v {
                        out.push($target);
                    }
                }
            };
            ($target:expr, $field:ident, $new:expr) => {
                if let Some(v) = &$new {
                    let current = &config.$field;
                    if *current != d.$field && current != v {
                        out.push($target);
                    }
                }
            };
        }
        cmp!(ImportTarget::ReadKomi, record.read_komi, self.read_komi);
        cmp!(ImportTarget::LoadSgfLast, record.load_sgf_last, self.load_sgf_last);
        cmp!(
            ImportTarget::ReplaceFileHint,
            record.show_replace_file_hint,
            self.show_replace_file_hint
        );
        cmp!(ImportTarget::Language, ui.language, self.language);
        cmp!(ImportTarget::PlaySound, ui.play_sound, self.play_sound);
        cmp!(ImportTarget::RecentFiles, recent_files, self.recent_files);
        cmp!(
            ImportTarget::AnalysisInterval,
            engine.analysis.interval_centisec,
            self.interval_centisec
        );
        cmp!(ImportTarget::LimitTime, engine.analysis.limit_time, self.limit_time);
        cmp!(
            ImportTarget::MaxAnalyzeTime,
            engine.analysis.max_analyze_time_seconds,
            self.max_analyze_time_seconds
        );
        cmp!(
            ImportTarget::LimitPlayout,
            engine.analysis.limit_playout,
            self.limit_playout
        );
        cmp!(
            ImportTarget::LimitPlayouts,
            engine.analysis.limit_playouts,
            self.limit_playouts
        );
        cmp!(
            ImportTarget::LimitMaxSuggestion,
            engine.analysis.limit_max_suggestion,
            self.limit_max_suggestion
        );
        cmp!(
            ImportTarget::LimitBranchLength,
            engine.analysis.limit_branch_length,
            self.limit_branch_length
        );
        cmp!(
            ImportTarget::FastEngineChange,
            engine.analysis.fast_engine_change,
            self.fast_engine_change
        );
        cmp!(
            ImportTarget::PonderLimitTips,
            engine.analysis.show_ponder_limited_tips,
            self.show_ponder_limited_tips
        );
        cmp!(ImportTarget::KataRules, engine.kata_rules, self.kata_rules);
        cmp!(
            ImportTarget::AutoLoadKataRules,
            engine.auto_load_kata_rules,
            self.auto_load_kata_rules
        );
        if let Some(engines) = &self.engines {
            let new: Vec<&EngineProfile> = engines.iter().map(|m| &m.profile).collect();
            let current: Vec<&EngineProfile> = config.engine.engines.iter().collect();
            if !current.is_empty() && current != new {
                out.push(ImportTarget::Engines);
            }
            cmp!(ImportTarget::EngineStartup, engine.startup, self.startup);
        }
        if let Some(folder) = &self.last_folder {
            if persisted.last_folder.as_ref().is_some_and(|f| f != folder) {
                out.push(ImportTarget::LastFolder);
            }
        }
        out
    }
}
