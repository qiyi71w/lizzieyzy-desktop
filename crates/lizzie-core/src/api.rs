//! The cross-language contract: commands, results, errors and the workbench projection.
//!
//! These Rust types are the single authority. `ui/src/contract/generated.ts` is generated from
//! them (see `tests/contract.rs`); the UI, its shortcuts and the acceptance channel all send the
//! same [`Command`] values and read the same [`WorkbenchView`].

mod resources;
mod tuning;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use resources::*;
pub use tuning::*;

use crate::browse::Navigation;
use crate::engine::profile::{AnalysisSettings, EngineProfile, EngineStartup};
use crate::record::{BoardSize, GameInfo, Mark, NodeId, Point, RecordFormat, Stone};
use crate::settings::Language;

/// Where an open request came from. Java only records the recent list and last folder for the
/// chooser (`loadFile(file, fromTemp=false)`); recent entries, drops and the command line open
/// "from temp" and leave both unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum OpenSource {
    Chooser,
    Recent,
    Drop,
    CommandLine,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum Command {
    /// "打开棋谱(O)", toolbar open buttons, `O`: the system chooser, then open the choice.
    ChooseAndOpenRecord,
    /// Open a known path (recent entry, drop, command line, automation).
    OpenRecord {
        path: String,
        source: OpenSource,
    },
    /// "从剪贴板粘贴棋谱(Ctrl+V)". `confirmed` answers the replace prompt.
    PasteRecord {
        confirmed: bool,
    },
    /// "保存(Ctrl+S)", toolbar save (`LizzieFrame.saveOriFile`): a record that has its own SGF
    /// file overwrites it, after the replace prompt while that hint is on; any other record (new,
    /// pasted, opened from GIB) goes through the save chooser. `confirmed` carries the document
    /// the user confirmed the replacement for; a different current document is asked about again.
    /// `suggested_name` is the chooser's default file name.
    SaveRecord {
        confirmed: Option<u64>,
        suggested_name: String,
    },
    /// "另存为(S)", `S` (`LizzieFrame.saveFile(false)`): the save chooser, then save there.
    ChooseAndSaveRecord {
        suggested_name: String,
    },
    /// Save to a known path (after an overwrite confirmation, automation), written exactly as
    /// named; an existing file is replaced without asking.
    SaveRecordTo {
        path: String,
    },
    /// Settings > "保存时显示覆盖原文件确认" and the replace prompt's "不再提示".
    SetReplaceFileHint {
        enabled: bool,
    },
    Navigate {
        nav: Navigation,
    },
    /// Variation tree click.
    GotoNode {
        node: NodeId,
    },
    /// Bottom bar move field + "跳转".
    GotoMoveNumber {
        number: u32,
    },
    /// File > "自动加载棋谱中的贴目".
    SetReadKomi {
        enabled: bool,
    },
    /// Settings language menu; takes effect after restart.
    SetLanguage {
        language: Language,
    },
    /// Settings > "播放落子音效".
    SetPlaySound {
        enabled: bool,
    },
    /// Help > "清除所有个人数据" after confirmation.
    ClearPersonalData,
    /// View > "重置界面位置".
    ResetUiPosition,
    /// Help > 诊断与日志 > "打开日志文件夹".
    OpenLogFolder,
    /// Help > 诊断与日志 > "导出诊断包": the system save chooser, then write the package.
    ExportDiagnostics,
    /// "开始/停止 分析(空格)", the bottom bar 分析 / 暂停分析 button, Space
    /// (`LizzieFrame.togglePonderMannul`). Not remembered across restarts.
    ToggleAnalysis,
    /// Engine menu `[n] name`, Ctrl+1..9 / Ctrl+0: make engine `index` the foreground engine.
    SwitchEngine {
        index: u32,
    },
    /// "重启当前引擎".
    RestartEngine,
    /// "关闭引擎" > 当前 / 当前以外 / 所有.
    ShutdownEngines {
        scope: EngineScope,
    },
    /// Engine settings (MoreEngines) saved: the whole engine list and the start-up choice.
    /// Entries with an empty id are new. Running engines whose command changed restart.
    SaveEngineSettings {
        engines: Vec<EngineProfile>,
        startup: EngineStartup,
    },
    /// Analysis limits / interval (settings dialog, engine page).
    SetAnalysisSettings {
        settings: AnalysisSettings,
    },
    /// Toolbar komi field: the game komi, sent to the engine; stored analysis is cleared.
    SetKomi {
        komi: f64,
    },
    /// 引擎规则(KataGo): apply `rules` (JSON object or preset name) to KataGo and remember them;
    /// `auto_load` applies them whenever KataGo starts.
    SetKataRules {
        rules: String,
        auto_load: bool,
    },
    /// GtpConsolePane: send a raw command to the foreground engine.
    SendGtpCommand {
        command: String,
    },
    /// 诊断与日志 > 完整日志 (engine GTP scope) on or off; "停止完整日志" turns it off.
    SetFullTrace {
        enabled: bool,
    },
    /// The "不再提示" box of the analysis-limit notice (`show-ponder-limited-tips`).
    SetPonderLimitTips {
        enabled: bool,
    },
    /// Answer to a rules prompt: continue with the engine's current rules, or pause analysis.
    AnswerRulesPrompt {
        prompt: u64,
        accept: bool,
    },
    /// Java import: the system folder chooser for the Java configuration folder (the one holding
    /// `config.txt`) or, with `program`, the Java program folder. Answers `FolderChosen`.
    ChooseJavaFolder {
        program: bool,
    },
    /// Java import: read the Java configuration in `config_dir` (or a `config.txt` path) without
    /// changing anything; relative engine paths are also looked up under `program_dir`. Answers
    /// `JavaImportPreview` or `JavaImportFailed`. A new preview replaces the previous one.
    PreviewJavaImport {
        config_dir: String,
        program_dir: Option<String>,
    },
    /// Java import: commit preview `preview`. `copy_credentials` lists the Java engine indexes whose
    /// credentials the user agreed to copy (other engines with credentials are left out);
    /// `replace` confirms replacing the settings listed in the preview's `replaces`.
    ApplyJavaImport {
        preview: u64,
        copy_credentials: Vec<u32>,
        replace: bool,
    },
    /// The first-use import offer was declined; it is not shown again.
    DismissJavaImportOffer,
    /// Engine resources: KataGo auto setup (detection, recommendation, download, offline import,
    /// reuse, verification, applying a setup), weights and acceleration components.
    Resource {
        op: ResourceCommand,
    },
    /// Engine performance: thread policy, benchmark, measured tuning, GTP configuration.
    Tuning {
        op: TuningCommand,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub enum EngineScope {
    Current,
    Others,
    All,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum CommandResult {
    /// Applied; the new state arrives as a [`WorkbenchView`].
    Done,
    /// Nothing to do (no movement, chooser or prompt cancelled, clipboard without SGF).
    Unchanged,
    /// Reading started; its end arrives as [`OpenOutcome`] in a later view.
    OpenStarted { ticket: u64 },
    /// The current record would be replaced; ask, then resend with `confirmed: true`.
    NeedsConfirmation,
    /// Diagnostics package written.
    Exported { path: String },
    /// Not done, with the Java resource key of the explanation.
    Refused { message_key: String },
    /// The record content was captured and is being written; the result arrives as
    /// [`SaveOutcome`] in a later view.
    SaveStarted { ticket: u64 },
    /// "保存" would overwrite the record's own file; ask (`LizzieFrame.replaceFileTitle`), then
    /// resend `SaveRecord` with `confirmed: Some(document)`.
    NeedsReplaceConfirmation { document: u64, file_name: String },
    /// The chosen name gained `.sgf` and that file exists (`LizzieFrame.prompt.sgfExists`); ask,
    /// then send `SaveRecordTo` with this path. The system chooser itself confirms replacing the
    /// file the user picked.
    NeedsOverwriteConfirmation { path: String },
    /// A folder was chosen ([`Command::ChooseJavaFolder`]).
    FolderChosen { path: String },
    /// What an import would do; nothing has changed yet.
    JavaImportPreview { preview: ImportPreviewView },
    /// The Java configuration could not be read; nothing has changed.
    JavaImportFailed { failure: ImportFailure },
    /// The import was written and applied.
    JavaImported { report: ImportReportView },
    /// Answer to a [`Command::Resource`].
    Resource { result: ResourceResult },
    /// Answer to a [`Command::Tuning`].
    Tuning { result: TuningResult },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum ErrorKind {
    InvalidArgument,
    Io,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ApiError {
    pub kind: ErrorKind,
    pub message: String,
}

impl ApiError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> ApiError {
        ApiError {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct DocumentView {
    /// Identity of the open document; changes on every adopted record.
    pub id: u64,
    pub format: Option<RecordFormat>,
    pub path: Option<String>,
    pub file_name: Option<String>,
    pub size: BoardSize,
    pub info: GameInfo,
    /// Content version; every content change increases it.
    pub revision: u64,
    /// The revision known to be the content of the file at `path` (read from it or confirmed
    /// written to it); `None` while the content was never on disk (new or pasted record).
    pub saved_revision: Option<u64>,
    /// The current content is not confirmed on disk: `saved_revision != Some(revision)`.
    pub unsaved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum TreeNodeKind {
    Root,
    Snapshot,
    Black,
    White,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct TreeNodeView {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    /// Children in order, without dummy placeholders.
    pub children: Vec<NodeId>,
    pub kind: TreeNodeKind,
    pub pass: bool,
    pub move_number: u32,
    pub has_comment: bool,
}

/// The whole variation tree; sent once per document, not on every navigation.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct TreeView {
    pub document: u64,
    pub nodes: Vec<TreeNodeView>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct LastAction {
    pub color: Stone,
    /// `None` for a pass.
    pub point: Option<Point>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct PositionView {
    pub document: u64,
    pub node: NodeId,
    /// Row-major, `size.width * size.height`.
    pub stones: Vec<Option<Stone>>,
    pub last_action: Option<LastAction>,
    pub snapshot: bool,
    pub move_number: u32,
    pub to_play: Stone,
    pub black_captures: u32,
    pub white_captures: u32,
    pub marks: Vec<Mark>,
    pub comment: String,
    /// Root to current node.
    pub path: Vec<NodeId>,
    /// End of the current line through first children (Java `history.getEnd()`).
    pub line_end: NodeId,
    pub main_trunk: bool,
    /// Points of the move children, first child first (Java `drawNextMoves`: passes and dummies
    /// are not shown).
    pub next_moves: Vec<Point>,
    /// Colour of the first move child; the next-move rings use it.
    pub next_color: Option<Stone>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum OpenOutcome {
    Adopted {
        ticket: u64,
        document: u64,
    },
    /// Reading failed; the previous document is unchanged. `message_key` is the Java resource
    /// key of the user message.
    Failed {
        ticket: u64,
        message_key: String,
        detail: String,
    },
}

/// Receipt of one save. It confirms exactly the document and revision that were captured when
/// the save started; edits made meanwhile, or another document opened meanwhile, stay unsaved.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum SaveOutcome {
    Saved {
        ticket: u64,
        document: u64,
        revision: u64,
        path: String,
    },
    /// Nothing was replaced; an existing file at `path` is unchanged. `message_key` is the Java
    /// resource key of the user message.
    Failed {
        ticket: u64,
        document: u64,
        path: String,
        message_key: String,
        detail: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct PendingOpen {
    pub ticket: u64,
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct SettingsView {
    pub language: Language,
    /// Language the UI is displaying in this session (resolved at start).
    pub active_language: Language,
    /// Whether the saved language differs from the one in use until restart.
    pub restart_required: bool,
    pub read_komi: bool,
    /// Settings > "播放落子音效" (Java `ui.play-sound`, default on).
    pub play_sound: bool,
    /// Ask before "保存" overwrites the record's own file (Java `ui.show-replace-file-hint`).
    pub replace_file_hint: bool,
    pub engine: EngineSettingsView,
}

/// Committed engine settings, for the engine settings dialog and the settings pages.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct EngineSettingsView {
    pub engines: Vec<EngineProfile>,
    pub startup: EngineStartup,
    pub analysis: AnalysisSettings,
    /// `kata-rules` as saved (JSON or preset name; empty = the engine's own).
    pub kata_rules: String,
    pub auto_load_kata_rules: bool,
    /// Settings > Advanced: run the official benchmark after start-up (`ui.enable-startup-benchmark`).
    pub startup_benchmark: bool,
}

/// Stone sound for a forward step onto a node (`Utils.playVoiceFile`): a capture of three or more
/// stones, a smaller capture, or a plain stone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum StoneSound {
    Stone,
    DeadStone,
    DeadStoneMore,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct WorkbenchView {
    /// Increases with every published view.
    pub seq: u64,
    pub document: DocumentView,
    /// Present when the document changed since the previous view, and in full snapshots.
    pub tree: Option<TreeView>,
    pub position: PositionView,
    pub pending_open: Option<PendingOpen>,
    /// The latest finished open, once.
    pub open_outcome: Option<OpenOutcome>,
    /// The latest finished save, once.
    pub save_outcome: Option<SaveOutcome>,
    /// Sounds of the browsing step that produced this view, one per node entered forward, in
    /// order (Java plays one per `moveHistoryForward`); handed over once, empty while sound is off.
    pub sounds: Vec<StoneSound>,
    pub settings: SettingsView,
    /// Newest first.
    pub recent_files: Vec<String>,
    /// Current session komi (toolbar komi field).
    pub komi: f64,
    pub engine: EngineView,
    /// Stored analysis of the current node (whether or not the engine is analysing it now).
    pub analysis: Option<NodeAnalysisView>,
    /// Stored results along the current line, its continuation and the main trunk, root first
    /// (the win-rate graph's data).
    pub history: Vec<HistoryPoint>,
    /// An engine that failed to start or stopped unexpectedly, once.
    pub engine_failure: Option<EngineFailureView>,
    /// A notice about analysis, once.
    pub engine_notice: Option<EngineNotice>,
    /// The session rules could not be applied to the foreground engine; ask whether to analyse
    /// with the engine's current rules (`KifuRules.failurePrompt`), once.
    pub rules_prompt: Option<RulesPromptView>,
    pub java_import: JavaImportView,
    /// Engine resources (auto setup); present when changed since the previous view, and in full
    /// snapshots.
    pub resources: Option<ResourcesView>,
    /// Engine performance (thread policy, benchmark); present when changed since the previous
    /// view, and in full snapshots.
    pub tuning: Option<TuningView>,
}

/// Java import availability.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct JavaImportView {
    /// First use (this product has no settings file yet): offer the import.
    pub offer: bool,
    /// Java configuration folders found in Java's conventional per-user / shared locations.
    pub candidates: Vec<String>,
}

/// What happens to one Java setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum ImportStatus {
    /// Taken over into this product's settings.
    Imported,
    /// Present, but its value cannot be used here.
    Incompatible,
    /// Left out: its feature is not in this product yet, or it needs a confirmation not given.
    NotMigrated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum ImportReason {
    /// The feature it belongs to is not available in this product yet.
    NotYetAvailable,
    /// It holds credentials; its value is not read.
    Credential,
    /// The value has a type or range Java would not use either.
    InvalidValue,
    /// Java window coordinates use Java's own scaling and layout, not this window's.
    WindowCoordinates,
    /// A remote (SSH) engine; remote engines are not available yet.
    RemoteEngine,
    /// The engine command holds credentials and copying them was not confirmed.
    CredentialNotConfirmed,
    /// The file could not be read as Java writes it.
    Unreadable,
}

/// One Java setting in the import preview / report. Credential values are never included.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ImportItemView {
    /// Java location: `config.txt` section and key (`ui.read-komi`), `persist` key
    /// (`persist:filesystem.last-folder`) or a file in the Java folder (`file:save`).
    pub key: String,
    pub status: ImportStatus,
    /// This product's setting it maps to.
    pub target: Option<ImportTarget>,
    /// The value taken over, for imported settings; the rejected value for incompatible ones.
    pub value: Option<String>,
    pub reason: Option<ImportReason>,
    pub detail: Option<String>,
    pub credential: bool,
}

/// This product's settings that can be imported from Java.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
pub enum ImportTarget {
    ReadKomi,
    LoadSgfLast,
    ReplaceFileHint,
    Language,
    PlaySound,
    RecentFiles,
    LastFolder,
    Engines,
    EngineStartup,
    LastEngine,
    AnalysisInterval,
    LimitTime,
    MaxAnalyzeTime,
    LimitPlayout,
    LimitPlayouts,
    LimitMaxSuggestion,
    LimitBranchLength,
    FastEngineChange,
    PonderLimitTips,
    KataRules,
    AutoLoadKataRules,
    /// `ui.enable-startup-benchmark`.
    StartupBenchmark,
    /// `ui.network-proxy-mode` / `-host` / `-port`.
    NetworkProxy,
}

/// A problem found with an imported engine without starting it.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum EngineProblem {
    /// The program was found neither at its path, under the Java folders, nor on `PATH`.
    ProgramNotFound {
        path: String,
    },
    /// A `-model` / `-config` / … file does not exist.
    FileNotFound {
        option: String,
        path: String,
    },
    EmptyCommand,
    /// A field Java would not use (`width`, `height`, `komi`); this product's default is used.
    InvalidValue {
        field: String,
    },
}

/// One Java engine entry (`leelaz.engine-settings-list`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ImportEngineView {
    /// Position in the Java engine list.
    pub index: u32,
    pub name: String,
    /// The command as it will be used (Java-relative paths resolved), credentials masked.
    pub command: String,
    pub status: ImportStatus,
    pub reason: Option<ImportReason>,
    /// The command or initial commands hold credentials: imported only when confirmed.
    pub credential: bool,
    pub is_default: bool,
    pub problems: Vec<EngineProblem>,
    /// Java fields of this entry this product does not use yet.
    pub unmigrated_fields: Vec<String>,
    /// Java fields that do not apply here: Java's management of its own bundled KataGo package
    /// (`managedProfileType`, `managedProfileCommand`); this client manages only resources it
    /// downloaded or imported itself.
    pub incompatible_fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ImportPreviewView {
    /// Answer with [`Command::ApplyJavaImport`].
    pub id: u64,
    pub config_file: String,
    pub persist_file: Option<String>,
    pub program_dir: Option<String>,
    pub items: Vec<ImportItemView>,
    pub engines: Vec<ImportEngineView>,
    /// Settings already chosen in this product that the import would replace.
    pub replaces: Vec<ImportTarget>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ImportReportView {
    pub config_file: String,
    pub items: Vec<ImportItemView>,
    pub engines: Vec<ImportEngineView>,
    /// The imported language applies after a restart.
    pub restart_required: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum ImportFailure {
    /// No `config.txt` in the chosen folder.
    NoConfig { path: String },
    /// `config.txt` could not be read.
    Unreadable { path: String, detail: String },
    /// `config.txt` is not a JSON object (line and column in `detail`).
    Invalid { path: String, detail: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct RulesPromptView {
    /// Answer with [`Command::AnswerRulesPrompt`].
    pub prompt: u64,
    /// The declared rules (the SGF `RU` text, or the chosen rules' fields).
    pub target: String,
    /// The engine's reported rules (`fieldSummary`), if any.
    pub observed: Option<String>,
    /// `SessionRulesSynchronizer.Failure` name.
    pub failure: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum EngineState {
    /// Running, identification or initial commands not answered yet.
    Loading,
    Ready,
}

/// One configured engine as the engine menu lists it.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct EngineEntryView {
    pub name: String,
    pub command: String,
    /// `None` when not running.
    pub state: Option<EngineState>,
}

/// The foreground engine (前台引擎): not a preloaded or background engine.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ForegroundEngineView {
    /// Position in the engine list.
    pub index: u32,
    /// The configured name, else the GTP name.
    pub name: String,
    pub gtp_name: String,
    pub version: String,
    pub katago: bool,
    /// Speaks `kata-analyze` / `lz-analyze`.
    pub can_analyse: bool,
    /// Identified and synced to the current position: the switch is complete.
    pub ready: bool,
    /// `list_commands` (GtpConsolePane 常用命令).
    pub commands: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct EngineView {
    pub engines: Vec<EngineEntryView>,
    pub foreground: Option<ForegroundEngineView>,
    /// The user paused analysis (Space); not remembered across restarts.
    pub paused: bool,
    /// The foreground engine is analysing the current node (Java `isPondering`).
    pub analysing: bool,
    /// An analysis limit stopped the analysis of the current node.
    pub stopped_by_limit: bool,
    /// Captured-panel rules caption key (`KataGoRules.Summary`), when KataGo reported its rules.
    pub rules_key: Option<String>,
    /// Rules the foreground KataGo reported (JSON), for the rules dialog.
    pub rules: Option<String>,
    /// Last console line; the console fetches newer lines.
    pub console_seq: u64,
    /// Full trace of engine GTP traffic is being written.
    pub full_trace: bool,
}

/// One candidate (`MoveData`). Percentages 0–100 from the side to move.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct CandidateView {
    /// `None` for a pass.
    pub point: Option<Point>,
    pub visits: u64,
    /// Root allocation (`edgeVisits` when reported, else `visits`).
    pub allocation_visits: u64,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub score_stdev: Option<f64>,
    pub prior: f64,
    pub lcb: Option<f64>,
    pub order: u32,
    /// Principal variation; `None` entries are passes.
    pub pv: Vec<Option<Point>>,
    /// For each `pv` move, the stones it captures when played out from the node's position
    /// (`Branch` with `remove-dead-in-variation`); empty for passes.
    pub pv_captures: Vec<Vec<Point>>,
}

/// Analysis stored for a node (`BoardData.bestMoves` and summary).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct NodeAnalysisView {
    pub node: NodeId,
    pub engine: String,
    pub to_play: Stone,
    /// Total visits of the candidates (`playouts`).
    pub visits: u64,
    pub root_visits: Option<u64>,
    /// The best candidate's win rate, side to move.
    pub winrate: f64,
    /// The best candidate's score lead, side to move (KataGo).
    pub score_lead: Option<f64>,
    pub score_stdev: Option<f64>,
    pub komi: f64,
    /// In `order`.
    pub candidates: Vec<CandidateView>,
}

/// One analysed node for the win-rate graph.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HistoryPoint {
    pub node: NodeId,
    pub move_number: u32,
    pub to_play: Stone,
    pub visits: u64,
    /// Side to move, 0–100.
    pub winrate: f64,
    pub score_lead: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct EngineFailureView {
    pub engine_name: String,
    /// Launch command with credentials masked.
    pub command: String,
    pub during_startup: bool,
    pub message: String,
    /// Last lines the engine printed.
    pub output: Vec<String>,
    /// Missing store-owned NVIDIA runtime components, if this managed profile can be repaired.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repair: Option<ResourceRepairView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum EngineNotice {
    /// The time or visit limit stopped the analysis (`leelaz.stopByLimit`); shown once per
    /// engine start while `show-ponder-limited-tips` is on.
    StoppedByLimit,
}

/// GtpConsolePane line.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ConsoleLineView {
    pub seq: u64,
    pub engine: String,
    pub kind: ConsoleLineKind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum ConsoleLineKind {
    Sent,
    Output,
    Error,
}

/// Help > 诊断与日志: where the logs are and their sizes (`None` when a log does not exist).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct LogsStatus {
    pub directory: String,
    pub app_log_bytes: Option<u64>,
    pub crash_log_bytes: Option<u64>,
}
