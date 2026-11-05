//! The cross-language contract: commands, results, errors and the workbench projection.
//!
//! These Rust types are the single authority. `ui/src/contract/generated.ts` is generated from
//! them (see `tests/contract.rs`); the UI, its shortcuts and the acceptance channel all send the
//! same [`Command`] values and read the same [`WorkbenchView`].

mod batch;
mod editing;
mod export;
mod hawkeye;
mod humansl;
mod humansl_settings;
mod matches;
mod recovery;
mod resources;
mod tuning;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use batch::*;
pub use editing::*;
pub use export::*;
pub use hawkeye::*;
pub use humansl::*;
pub use humansl_settings::*;
pub use matches::*;
pub use recovery::*;
pub use resources::*;
pub use tuning::*;
pub use crate::strength::{StrengthModelView, StrengthReport, StrengthSideReport, StrengthSegment, StrengthSummary, StrengthSample, StrengthConfidence, StrengthMoveCategory};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum StrengthCommand {
    Report,
    SelectModel { id: String },
    ImportModel { path: String },
    SetBackground { enabled: bool },
    /// 补算 HumanSL. `resume`: the same request after the model download it needed; it prepares
    /// the game it was asked for, else is refused with `HumanSL40.gameChanged`.
    RunHumanSl { resume: bool },
    CancelHumanSl,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthView {
    pub models: Vec<StrengthModelView>,
    pub active: String,
    pub background: bool,
    pub status: StrengthStatusView,
    pub manual: Option<StrengthManualView>,
    pub stamp: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthStatusView {
    pub key: String,
    pub completed: u32,
    pub total: u32,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthManualView {
    /// Numbers each admitted 补算 HumanSL; a dialog only takes the outcome of the run it started
    /// (an earlier run's outcome stays visible until the next run is admitted).
    pub run: u32,
    pub running: bool,
    pub outcome: Option<StrengthManualOutcome>,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum StrengthManualOutcome {
    Ready,
    Cancelled,
    Failed { message_key: String, detail: Option<String> },
}

use crate::browse::Navigation;
use crate::engine::profile::{AnalysisSettings, EngineProfile, EngineStartup};
use crate::record::{BoardSize, GameInfo, Mark, NodeId, Point, RecordFormat, Stone};
use crate::settings::{EditSettings, Language};

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
    Strength { op: StrengthCommand },
    /// 主题 drafts apply together only on 保存设置 (ConfigDialog2.java:6191-6286).
    SetAppearanceSettings {
        settings: crate::appearance::AppearanceSettings,
        theme: Box<crate::appearance::ThemeSettings>,
    },
    SetFrameFontSize { size: i32 },
    AddTheme { name: String },
    DeleteTheme { name: String },
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
    /// 文件 > 更多保存 > 保存纯净棋谱 / 保存纯净棋谱(带评论) / 保存纯净分支 (Ctrl+Shift+S, Ctrl+Alt+S):
    /// the save chooser (last folder, `*.sgf`, `suggested_name`), then write that export.
    /// `labels` word the win-rate summaries of [`ExportKind::RawWithComments`].
    ExportRecord {
        kind: ExportKind,
        suggested_name: String,
        labels: CommentLabels,
    },
    /// Write export `kind` to a known path (after `NeedsExportOverwriteConfirmation`,
    /// automation), exactly as named.
    ExportRecordTo {
        kind: ExportKind,
        path: String,
        labels: CommentLabels,
    },
    /// 复制棋谱到剪贴板 (Ctrl+C): the record as "保存" writes it, as clipboard text.
    CopyRecord,
    /// 保存主棋盘截图 (Alt+S) / 保存小棋盘截图 (Shift+S) / 保存胜率图 (Shift+Alt+S): the picture
    /// chooser (PNG, JPG, GIF, BMP; last picture folder), then write `png` (base64 PNG of the
    /// panel as shown) in the format of the chosen name. Answers `Exported`,
    /// `NeedsImageOverwriteConfirmation` or `ExportFailed`.
    ExportImage {
        png: String,
    },
    /// Write a picture to a known path (after `NeedsImageOverwriteConfirmation`, automation).
    ExportImageTo {
        path: String,
        png: String,
    },
    /// 复制主棋盘到剪贴板 (Shift+C) / 复制小棋盘到剪贴板 (Alt+C): `png` as a clipboard picture.
    CopyImage {
        png: String,
    },
    /// 存档与读档.
    Recovery {
        op: RecoveryCommand,
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
    /// 记录选点信息到评论中 (Java `ui.append-winrate-to-comment`).
    SetAppendWinrateToComment {
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
    /// Fixed Java layout entry; leaving DoubleEngine stops secondary analysis, not its process.
    SetExtraMode {
        mode: crate::settings::ExtraMode,
    },
    SetClassicMode,
    ToggleIndependentBoard { board: crate::settings::IndependentBoard },
    CloseIndependentBoard { board: crate::settings::IndependentBoard },
    /// SetCustomMode / 可视化面板设置 确定: mode, panel flags and board position, plus the
    /// 自定义布局 `save_slot` snapshot with the main `window` bounds, as one change.
    CommitLayout {
        mode: crate::settings::ExtraMode,
        panels: crate::settings::PanelSettings,
        board_position_proportion: u8,
        save_slot: Option<u8>,
        window: Option<crate::settings::WindowPlacement>,
    },
    ApplyCustomLayout { slot: u8 },
    /// Secondary engine menu `[n] name` (`EngineManager.switchEngine(index, false)`), in 双引擎模式
    /// only: engine `index` becomes the secondary engine. The foreground engine is refused
    /// (`EngineManager.sameEngineHint`).
    SwitchSecondaryEngine {
        index: u32,
    },
    /// Secondary engine menu > "重启当前引擎" (`EngineManager.restartEngine2`).
    RestartSecondaryEngine,
    /// Secondary engine menu > "关闭当前引擎" (`EngineManager.killThisEngines2`).
    ShutdownSecondaryEngine,
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
    /// `replace` confirms replacing the settings listed in the preview's `replaces`; `recovery`
    /// is present when the user chose to import the Java 存档与读档 slots.
    ApplyJavaImport {
        preview: u64,
        copy_credentials: Vec<u32>,
        replace: bool,
        recovery: Option<RecoveryImport>,
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
    /// Document editing (Edit / Game menus, board input, edit toolbars, dialogs).
    Edit {
        op: EditCommand,
    },
    /// Edit menu check items and editing prompts (启用双击找子, 启用拖动棋子, 显示新建棋盘确认对话框,
    /// 标记工具, right-click menu, other board size).
    SetEditSettings {
        settings: EditSettings,
    },
    /// 显示 > 总是显示黑胜率 and 目差显示 (盘面 / 目差).
    SetDisplaySettings {
        settings: crate::settings::DisplaySettings,
    },
    /// 界面 > 面板 and its Java keyboard shortcuts.
    TogglePanel { panel: crate::settings::WorkbenchPanel },
    /// 综合设置 > 显示; drafts become committed together on 保存设置.
    SetPanelSettings { settings: crate::settings::PanelSettings },
    /// A finished in-frame separator drag; motion drafts never enter the core.
    SetPanelSizes { sizes: crate::settings::PanelSizes },
    NudgeBoardPosition { delta: i32 },
    RestoreDefaultPanelSizes,
    SetBottomToolbar { mode: crate::settings::BottomToolbar },
    /// 纯网络 (H): Off → Policy → Ownership → Off for KataGo, Off ↔ Policy otherwise.
    ToggleRawNet,
    /// Turn pure network off, resuming ordinary analysis (policy T).
    StopRawNet,
    /// 显示 > KataGo相关设置 > Kata形势显示: 关闭 / 隐藏 / 显示在大棋盘上 / 显示在小棋盘上 /
    /// 显示在大小棋盘上.
    SetKataEstimate {
        mode: KataEstimateMode,
    },
    /// `.` (with 使用快捷键) and the bottom bar Kata形势 (`LizzieFrame.toggleShowKataEstimate`);
    /// only with a KataGo foreground engine.
    ToggleKataEstimate,
    /// 终局点目 (Ctrl+Q, 棋局 menu, bottom bar) and its dialog.
    Score {
        op: ScoreCommand,
    },
    /// 设置 > 加载棋谱后快速刷新胜率曲线 (`ui.auto-quick-analyze-on-load`). Turning it off cancels a
    /// running automatic quick analysis.
    SetAutoQuickAnalyze {
        enabled: bool,
    },
    /// 设置 > 打开后跳到最后一手 (`ui.load-sgf-last`).
    SetLoadSgfLast {
        enabled: bool,
    },
    /// 精析设置 > 确定 (`AnalysisSettings.saveConfig`). `shown_command` is the command the dialog
    /// showed when it opened; `edited` is set once 自动生成 / 选择引擎 changed the command. A command
    /// becomes customized when it differs from what was shown, or was edited, unless the current
    /// engine is reused. Answers `Refused { AnalysisSettings.commandRequired }` for an empty
    /// command without reuse; `command_customized` in `settings` is ignored.
    SaveAnalysisEngineSettings {
        settings: crate::settings::AnalysisEngineSettings,
        shown_command: String,
        edited: bool,
    },
    /// 精析设置 > 选择引擎, and opening the dialog while the command is not customized: the
    /// analysis command made from engine `index` (`None`: the foreground engine, else the default
    /// one) by turning `gtp` into `analysis` and its config into the `analysis.cfg` beside it,
    /// which is created from KataGo's example when missing (`AnalysisEngineCommandHelper`).
    /// Answers `AnalysisCommand` or `Refused` with the helper's message key.
    GenerateAnalysisCommand {
        index: Option<u32>,
    },
    /// 快速曲线轻量模型 > 自动使用.
    SetQuickModelEnabled {
        enabled: bool,
    },
    /// The 评论/问题手 panel: page, side filter, sort, control bar.
    SetProblemList {
        settings: crate::settings::ProblemListSettings,
    },
    /// A 问题手 row (click, Enter, Space; `jumpToProblemMove`): browse to the position before the
    /// move at `node` and mark the move that was played there.
    JumpToProblemMove {
        node: NodeId,
    },
    /// 闪电分析 (`LizzieFrame.flashAnalyzeGame`): 整盘速览, 局部精析 (saving the move range, as
    /// `AnalysisPartGame` does) or 全部分支精析. An automatic quick analysis hands its engine over
    /// first (`MANUAL_FLASH`). Answers `Refused` with `LizzieFrame.analysisCommandMissing` when no
    /// analysis command is set and the current engine is not reused, or with
    /// `AnalysisSettings.reuseStatus.<availability>` when the foreground engine cannot be borrowed.
    StartFlashAnalysis {
        target: FlashTarget,
    },
    /// The progress dialog's 取消分析 (`destroyAnalysisEngine`): completed results stay.
    CancelFlashAnalysis,
    /// 精析设置 > 确定并重新计算: the running flash analysis stops and the last one starts again.
    RedoFlashAnalysis,
    /// 强制计算区域 (toolbar 设置计算区域 / 设置不计算区域 / 清除区域, Alt+drag and Alt+click, and the
    /// board's right-click 只分析此点 … 清除分析与不分析). The region belongs to the session, not the
    /// record: it never changes the game.
    Region {
        op: RegionCommand,
    },
    /// 强制计算区域's depth choices (`selectAllowPopup` / `selectAvoidPopup`); an analysis running
    /// with a region is asked again with the new depth.
    SetRegionSettings {
        settings: crate::settings::RegionSettings,
    },
    /// 选点重点研究 (right-click 评估此点 / 取消评估 / 清除所有评估选点) on the current position. A
    /// point that cannot be tracked now (engine without `focus`, occupied, outside a kept region,
    /// already tracked) answers `Unchanged`.
    Tracking {
        op: TrackingCommand,
    },
    /// Settings > 选点重点研究累计计算量目标 and 选点评估关注外框; a changed target ends the tracking of
    /// the current position (`invalidateTrackingAnalysis`).
    SetTrackingSettings {
        settings: crate::settings::TrackingSettings,
    },
    StartAutoAnalysis { settings: crate::settings::AutoAnalysisSettings },
    StopAutoAnalysis,
    AnswerAutoAnalysisDiff { accept: bool },
    SaveAutoAnalysisSettings { settings: crate::settings::AutoAnalysisSettings },
    DismissAutoAnalysisTip { never_again: bool },
    /// 整盘精析 (Ctrl+Shift+B): two-stage main-trunk analysis.
    StartWholeGameAnalysis { visits: u32 },
    PauseWholeGameAnalysis,
    ResumeWholeGameAnalysis,
    StopWholeGameAnalysis,
    /// Pre-start estimate using the current engine's stored benchmark.
    EstimateWholeGameAnalysis { visits: u32 },
    /// 超级鹰眼: open/close, the side boxes and navigation from its lists and charts.
    Hawkeye {
        op: HawkeyeCommand,
    },
    /// 超级鹰眼's remembered choices: pages, 分支, 统计 range, list thresholds, chart thresholds,
    /// 总在最前 (`ui.*` keys of `MoveListFrame`, `SetThreshold`). Values outside the dialog's
    /// ranges are an `InvalidArgument` error with nothing changed; the open state is `Toggle`'s.
    SetHawkeyeSettings {
        settings: crate::settings::HawkeyeSettings,
    },
    /// A 超级鹰眼 list's 导出为xls (`Utils.exportTable`): the save chooser (`(*.xls)`), `.xls`
    /// appended to the chosen name (a name containing `.xlsx` writes nothing), then the cells as
    /// tab-separated UTF-16LE text with a byte-order mark, header row first. Answers `Exported`,
    /// `ExportFailed` or `Unchanged`.
    ExportTable {
        rows: Vec<Vec<String>>,
    },
    /// 批量分析 / 批量精析 and their 进度表.
    Batch {
        op: BatchCommand,
    },
    /// 人机对弈 (棋局 menu, 新对局 dialogs, N / Alt+N / Enter / Alt+Enter / P / Alt+, and the
    /// double-menu game buttons) and the bottom toolbar's 自动落子.
    Game {
        op: GameCommand,
    },
    /// 引擎对战.
    Match { op: MatchCommand },
    HumanSl { op: HumanSlCommand },
    /// 抓取死活题 参数设置 确定 (`CaptureTsumeGoSettings.saveSettings`).
    SetCaptureSettings {
        settings: crate::settings::CaptureSettings,
    },
    /// 自定义工具栏 > 死活 (`show-tsume-go-menu`).
    SetTsumegoButtonShown {
        shown: bool,
    },
}

/// 人机对弈's two modes (`isPlayingAgainstLeelaz` / `isAnaPlayingAgainstLeelaz`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum GameKind {
    /// Genmove模式: the engine chooses its move (`genmove`).
    Genmove,
    /// 分析模式: the engine's analysis is played when a 落子条件 is met.
    Analysis,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum GameCommand {
    /// The 新对局 dialog's 确定: `settings` (and for 分析模式 the 落子条件 `autoplay`) are saved,
    /// engine `engine` becomes the foreground engine when it is another one, and the game is
    /// accepted; it starts once the engine is ready and set up.
    Start {
        kind: GameKind,
        settings: crate::settings::GameSettings,
        autoplay: Option<crate::settings::AutoPlaySettings>,
        engine: Option<u32>,
    },
    /// 人机续弈 from the current position. `human` plays that colour; `None` (Enter / Alt+Enter)
    /// lets the engine play the side to move now.
    Continue {
        kind: GameKind,
        human: Option<Stone>,
    },
    /// 暂停 / 继续.
    Pause,
    /// 认输: the engine wins (`Leelaz.whiteWin` / `Leelaz.blackWin`).
    Resign,
    /// 结束对局 / 终止人机对局 (Space).
    End,
    /// 悔棋: back to the human's previous turn.
    Regret,
    /// 修改AI用时: the engine's time, 后台思考 and the 分析模式 落子条件, applied to a running game.
    SetAiTime {
        time: crate::settings::AiTimeSettings,
        ponder: bool,
        autoplay: crate::settings::AutoPlaySettings,
    },
    /// AI随机开局's 确定.
    SetRandomStart {
        settings: crate::settings::RandomStartSettings,
    },
    /// Alt+, (`LizzieFrame.genmove`): the engine plays one move for the side to move.
    EngineMove,
    /// The bottom toolbar's 自动落子 box (session) and its sides and conditions (remembered).
    SetAutoPlay {
        enabled: bool,
        settings: crate::settings::AutoPlaySettings,
    },
}

/// A game's progress. `Starting` is accepted, not started: the engine is still loading or
/// answering the game's setup, and the board takes no move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum GamePhase {
    Starting,
    Playing,
    Paused,
    /// Over: the engine is being given back; other modes wait until it confirms.
    Ending,
}

/// The human's clock (限制我的用时): main time, then 读秒 seconds and periods (-1: none).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub struct GameClockView {
    pub minutes: i32,
    pub seconds: i32,
    pub byo_seconds: i32,
    pub byo_times: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct GameView {
    pub id: u64,
    pub kind: GameKind,
    pub phase: GamePhase,
    pub human: Stone,
    /// The engine is choosing its move.
    pub thinking: bool,
    /// 对弈模式 (`isInPlayMode`): analysis hidden while the game runs.
    pub play_mode: bool,
    /// Candidates shown when the game started (显示黑/白棋候选点; both off for 续弈).
    pub show_black: bool,
    pub show_white: bool,
    /// AI只使用纯网络思考.
    pub pure_net: bool,
    pub clock: Option<GameClockView>,
}

/// How a game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum GameEnd {
    EngineResigned,
    HumanResigned,
    HumanTimeout,
    /// Ended by the user with a result recorded (终局点目 确认结果).
    Completed,
    /// Ended by the user without a result, or by another record replacing the game.
    Terminated,
    /// The engine was lost or did not confirm its restore.
    Retired,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct GameOutcomeView {
    pub game: u64,
    pub end: GameEnd,
    /// `RE` written by the game (B+R, W+R, B+T, W+T).
    pub result: Option<String>,
    /// Where the finished game is saved (自动保存棋谱), once the save is under way.
    pub saved: Option<String>,
}

/// A Java message of 人机对弈 (`message_key` text, `prefix` before it when set, e.g. the engine
/// name of `Leelaz.resign`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct GameNotice {
    pub message_key: String,
    pub prefix: Option<String>,
}

/// The bottom toolbar's 自动落子.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, TS)]
pub struct AutoPlayView {
    pub enabled: bool,
    pub settings: crate::settings::AutoPlaySettings,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct AutoAnalysisView {
    pub active: bool,
    pub starting: bool,
    pub diff: bool,
    pub confirmation: bool,
    pub diff_moves: u32,
    pub diff_nodes: u32,
    pub pause_tip: bool,
    pub notice: Option<AutoAnalysisNotice>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct AutoAnalysisNotice {
    pub id: u32,
    pub message_key: String,
    pub directory: Option<String>,
    pub timed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, TS)]
pub enum WholeGameState {
    #[default]
    Idle,
    Preparing,
    Baseline,
    Deep,
    Pausing,
    Paused,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct WholeGameAnalysisView {
    pub run: u64,
    pub state: WholeGameState,
    pub completed: u32,
    pub total: u32,
    pub baseline_completed: u32,
    pub deep_completed: u32,
    pub target_visits: u32,
    pub visits: u32,
    pub percent: u32,
    pub remaining_ms: Option<u64>,
    pub mode_key: Option<String>,
    pub detail_key: String,
    /// Force result display for this document/node without changing saved display settings.
    pub show_results: bool,
    pub save_failed: bool,
}

/// What a flash analysis analyses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "kind")]
pub enum FlashTarget {
    /// 整盘速览（闪电）: every main-trunk move.
    WholeGame,
    /// 局部精析: main-trunk moves from `start_move` up to, not including, `end_move` (counted in
    /// moves and passes; -1 leaves that end open).
    MoveRange { start_move: i32, end_move: i32 },
    /// 全部分支精析: every move of every branch.
    AllBranches,
}

/// 强制计算区域 operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum RegionCommand {
    /// A rectangle chosen by dragging (`selectForceAllowAvoid`): its intersections from `from` to
    /// `to` join the region of that kind (a region of the other kind is replaced); the region is
    /// kept across positions.
    Select { allow: bool, from: Point, to: Point },
    /// The toolbar region button was pressed (`isKeepingForce`): the region is kept across
    /// positions from now on.
    Keep,
    /// 只分析此点: only this empty point is analysed, its first request one move deep.
    AllowPoint { point: Point },
    /// 增加分析此点.
    AddAllowPoint { point: Point },
    /// 去除分析此点.
    RemoveAllowPoint { point: Point },
    /// 不分析此点.
    AvoidPoint { point: Point },
    /// 持续分析/不分析 (`isKeepForcing`): every region is kept across positions while on.
    ToggleKeepForcing,
    /// 清除分析与不分析: the region is removed and analysis resumes.
    ClearForcing,
    /// Toolbar 清除区域 / Alt+click: the region is removed and no longer kept.
    Clear,
}

/// 选点重点研究 operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum TrackingCommand {
    Track { point: Point },
    Untrack { point: Point },
    Clear,
}

/// 强制计算区域 of this session.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct RegionView {
    /// The region analyses only these points (`allow`), or never these (`avoid`).
    pub allow: bool,
    /// Empty when there is no region.
    pub points: Vec<Point>,
    /// 持续分析/不分析 is on.
    pub keep_forcing: bool,
}

/// 选点重点研究 on the current position and the foreground engine's `focus` capability.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct FocusView {
    /// What the foreground engine's own start-up probe found (`Unknown` without one).
    pub capability: crate::engine::runtime::FocusCapability,
    /// A point can be tracked now (`canStartTrackingAnalysis`).
    pub available: bool,
    pub target_visits: u32,
    /// Tracked points, in the order they were chosen.
    pub points: Vec<TrackedPointView>,
    /// The engine still searches focus points the user no longer tracks (the change is sent,
    /// not yet accepted).
    pub cancellation_pending: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct TrackedPointView {
    pub point: Point,
    /// Still searched extra (below the target, progressing).
    pub active: bool,
    /// The highest visits seen for this candidate.
    pub visits: u64,
    /// 落子评价 of the candidate against the best one, when both are in the analysis.
    pub rank: Option<crate::display::MoveRank>,
}

/// Kata形势显示 menu choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub enum KataEstimateMode {
    Close,
    Hide,
    Main,
    Sub,
    Both,
}

/// 终局点目 operations.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum ScoreCommand {
    /// Enter (pausing analysis) or leave (resuming it) scoring at the current position.
    Toggle,
    /// Left click in scoring: an empty point toggles 不计入; a stone toggles its connected
    /// stones as dead.
    Point { point: Point },
    /// 重新计算: start again from the position's stones.
    Recalculate,
    /// 确认结果: write the result into the game information (RE).
    Confirm,
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
    Strength { report: Box<StrengthReport> },
    StrengthImportFailed { detail: String },
    WholeGameEstimate { millis: Option<u64> },
    /// Nothing to do (no movement, chooser or prompt cancelled, clipboard without SGF).
    Unchanged,
    /// Reading started; its end arrives as [`OpenOutcome`] in a later view.
    OpenStarted { ticket: u64 },
    /// The current record would be replaced; ask, then resend with `confirmed: true`.
    NeedsConfirmation,
    /// An edit would discard moves or the record; ask, then resend it with `confirmed: target`.
    NeedsEditConfirmation { target: EditTarget },
    /// A file was written (diagnostics package, picture).
    Exported { path: String },
    /// A picture could not be written (`LizzieFrame.saveImageErrorHint1`); nothing was replaced.
    ExportFailed { path: String, detail: String },
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
    /// Like `NeedsOverwriteConfirmation`, for an export: ask, then send `ExportRecordTo`.
    NeedsExportOverwriteConfirmation { kind: ExportKind, path: String },
    /// The picture name gained the chosen format's extension and that file exists
    /// (`LizzieFrame.fileExists`); ask, then send `ExportImageTo` with this path.
    NeedsImageOverwriteConfirmation { path: String },
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
    /// A generated analysis engine command ([`Command::GenerateAnalysisCommand`]);
    /// `generated_config` is the `analysis.cfg` created for it, if one was.
    AnalysisCommand {
        command: String,
        generated_config: Option<String>,
    },
    /// 死活题 framed the position; each colour in `no_room` is one "没有布置 [x方] 劫材的空间"
    /// message, in order.
    TsumegoFramed { no_room: Vec<Stone> },
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
    /// The file name shown when the record has no file of its own: a 存档与读档 slot it was
    /// loaded from (Java shows `game2.sgf` in the title; saving goes through the chooser).
    pub origin_name: Option<String>,
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
    /// Display numbering, including branch restarts, SGF MN and passes; 0 for no number.
    pub number: u32,
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
        /// The record holds secondary-engine analysis (`LZ2`/`LZOP2`) and 双引擎模式 is off: ask
        /// whether to switch to it (`SGFParse.doubleEngineHint`), then send `SetExtraMode`.
        double_engine_hint: bool,
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
        /// The write was an export; it does not make the record saved.
        export: Option<ExportKind>,
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
    /// Whether the saved language or frame font size differs from this session until restart.
    pub restart_required: bool,
    pub read_komi: bool,
    /// 打开后跳到最后一手 (`ui.load-sgf-last`).
    pub load_sgf_last: bool,
    /// 打开棋谱后自动快速分析 (`ui.auto-quick-analyze-on-load`).
    pub auto_quick_analyze_on_load: bool,
    pub append_winrate_to_comment: bool,
    /// Settings > "播放落子音效" (Java `ui.play-sound`, default on).
    pub play_sound: bool,
    /// Ask before "保存" overwrites the record's own file (Java `ui.show-replace-file-hint`).
    pub replace_file_hint: bool,
    /// 布局模式 (Java `ui.extra-mode`).
    pub extra_mode: crate::settings::ExtraMode,
    pub panels: crate::settings::PanelSettings,
    pub panel_sizes: crate::settings::PanelSizes,
    pub bottom_toolbar: crate::settings::BottomToolbar,
    pub comment_panel_auto_hidden: bool,
    pub custom_layouts: [Option<crate::settings::CustomLayout>; 2],
    pub independent_main_board: Option<crate::settings::WindowPlacement>,
    pub independent_sub_board: Option<crate::settings::WindowPlacement>,
    pub engine: EngineSettingsView,
    pub edit: EditSettings,
    pub display: crate::settings::DisplaySettings,
    /// 精析设置 as saved.
    pub analysis_engine: crate::settings::AnalysisEngineSettings,
    /// 快速曲线轻量模型 on the weights page.
    pub quick_model: QuickModelView,
    pub human_sl: HumanSlSettingsView,
    /// 评论/问题手 panel state.
    pub problems: crate::settings::ProblemListSettings,
    /// 局部精析's move range (`analysis-start-move`, `analysis-end-move`).
    pub flash_range: crate::settings::FlashRange,
    pub auto_analysis: crate::settings::AutoAnalysisSettings,
    pub whole_game: crate::settings::WholeGameSettings,
    /// 超级鹰眼's remembered choices.
    pub hawkeye: crate::settings::HawkeyeSettings,
    pub appearance: crate::appearance::AppearanceSettings,
    pub theme: crate::appearance::ThemeSettings,
    /// Frame size in use until the next start (LizzieFrame.java:20766-20768).
    pub active_frame_font_size: i32,
    /// 强制计算区域's depth choices.
    pub region: crate::settings::RegionSettings,
    /// 选点重点研究.
    pub tracking: crate::settings::TrackingSettings,
    /// 人机对弈 dialog settings.
    pub game: crate::settings::GameSettings,
    /// The bottom toolbar's 自动落子.
    pub autoplay: AutoPlayView,
    pub matches: MatchSettingsView,
    /// 死活题 dialog choices, 抓取死活题 参数设置 and the 死活 toolbar button.
    pub tsumego: crate::settings::TsumegoSettings,
}

/// 快速曲线轻量模型 card (`KataGoAutoSetupDialog`, `QuickAnalysisModelStatus`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct QuickModelView {
    pub status: QuickModelStatus,
    /// The installed model file.
    pub path: Option<String>,
    /// 自动使用 is on.
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum QuickModelStatus {
    /// 未下载: download offered.
    Missing,
    /// Installed, but the engine that would run it is not KataGo 1.17 or later.
    RequiresKataGo117,
    /// 已安装.
    Installed,
    /// 自动使用.
    Enabled,
}

/// Automatic quick analysis (自动棋谱快析) of the open record.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct QuickAnalysisView {
    /// A task is active (waiting, starting, running or giving its engine back).
    pub running: bool,
    /// An attempt failed and the task retries in the background
    /// (`LizzieFrame.quickAnalysis.retrying`).
    pub retrying: bool,
    /// The task borrows the foreground engine instead of running an analysis process.
    pub borrowed: bool,
}

/// A manual flash analysis in progress (`WaitForAnalysis`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct FlashAnalysisView {
    /// Identity of this run; the UI's 隐藏界面 applies to one run.
    pub run: u64,
    /// The engine is still starting or being borrowed (进度: 引擎加载中...).
    pub loading: bool,
    /// Positions answered so far, of `total`.
    pub done: u32,
    pub total: u32,
    /// The foreground engine is borrowed instead of an analysis process.
    pub borrowed: bool,
}

/// How a flash analysis ended, once.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum FlashOutcome {
    /// Every position was answered (分析完毕).
    Completed { run: u64 },
    /// Stopped with the Java resource key of the explanation
    /// (`AnalysisEngine.requestDispatchFailed`, `AnalysisEngine.foregroundRestoreFailed`).
    Failed { run: u64, message_key: String },
    /// The analysis process failed (reported as `engine_failure`); 精析设置 opens with
    /// 确定并重新计算 (`AnalysisSettings(true, true)`).
    EngineFailed { run: u64 },
}

/// 问题手 list and its progress badge (`ProblemListSnapshot`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ProblemListView {
    /// Moves of each side over the win-rate threshold, in the chosen order.
    pub black: Vec<ProblemMoveView>,
    pub white: Vec<ProblemMoveView>,
    /// Main-trunk moves from move 2 whose position before and after are analysed.
    pub analyzed: u32,
    /// Main-trunk moves from move 2.
    pub total: u32,
    /// Background analysis is running (`评估中`).
    pub running: bool,
    /// The move a jump marked on the board (`clickbadmove`), while browsing the position before it.
    pub mark: Option<Point>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ProblemMoveView {
    /// The move's node.
    pub node: NodeId,
    pub move_number: u32,
    pub mover: Stone,
    pub point: Point,
    /// Win rate lost by the mover, percentage points.
    pub winrate_loss: f64,
    /// Points lost, when both positions have KataGo scores.
    pub score_loss: Option<f64>,
    /// Visits of the position after the move.
    pub playouts: u64,
    /// 1–5 by the 落子评价 win-rate thresholds; 5 is shown in the loss colour.
    pub tier: u8,
    /// The browsing position is this move, or the position before it with the move marked.
    pub current: bool,
}

/// Whether the foreground engine can be borrowed (`AnalysisSettings.reuseStatus.*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum LeaseAvailabilityView {
    Available,
    NoForegroundEngine,
    EngineNotReady,
    NotKatago,
    MissingCapability,
    ExistingLease,
    EngineStateUnrestored,
    /// 人机对弈 holds the engine (`AnalysisSettings.reuseStatus.play_mode`).
    PlayMode,
    HumanSlGame,
}

/// Why the foreground engine was not given back (`LizzieFrame.foregroundUnrestored.reason.*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum UnrestoredReason {
    /// 停止失败.
    Stop,
    /// 恢复失败.
    Restore,
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
    /// Automatic quick analysis of the open record.
    pub quick: QuickAnalysisView,
    /// Manual flash analysis in progress.
    pub flash: Option<FlashAnalysisView>,
    pub auto_analysis: AutoAnalysisView,
    /// 批量分析 / 批量精析 and their 进度表.
    pub batch: BatchView,
    /// How a flash analysis ended, once.
    pub flash_outcome: Option<FlashOutcome>,
    /// 强制计算区域 of this session.
    pub region: RegionView,
    /// 选点重点研究 of the current position.
    pub focus: FocusView,
    /// Session projection, retained after completion/cancel/failure.
    pub whole_game: WholeGameAnalysisView,
    /// 问题手 list and its progress badge.
    pub problems: ProblemListView,
    /// Stored analysis of the current node (whether or not the engine is analysing it now).
    pub analysis: Option<NodeAnalysisView>,
    /// Move numbers, 落子评价 and 下一手 详细 of the current position, as the display settings
    /// choose them.
    pub board: crate::display::BoardOverlayView,
    /// Foreground engine's pure-network result, never from another position or instance.
    #[serde(rename = "rawNet")]
    pub raw_net: Option<crate::engine::raw_net::RawNetView>,
    /// Stored results along the current line, its continuation and the main trunk, root first
    /// (the win-rate graph's data).
    pub history: Vec<HistoryPoint>,
    /// Coloured analysed tree nodes, including branches outside the graph's current line.
    pub theme_nodes: Vec<crate::appearance::ThemeNodeColor>,
    /// Each move's result against the analysis of the position it was played in, and the AI
    /// match score (统计 panel, captured panel, 超级鹰眼).
    pub statistics: StatisticsView,
    /// 超级鹰眼 while its window is open (computed only then).
    pub hawkeye: Option<HawkeyeView>,
    /// Kata形势显示 and the ownership it draws.
    pub kata_estimate: KataEstimateView,
    /// 终局点目, while scoring.
    pub score: Option<ScoreView>,
    /// 双引擎模式's right board: the secondary engine's results for the current node. Present
    /// only in 双引擎模式.
    pub secondary: Option<SecondaryView>,
    /// 记录选点信息到评论中: the current comment as Java shows it while an engine analyses,
    /// with the analysis summary appended (Java writes it into the node, `appendComment`);
    /// absent when the comment pane shows `position.comment` unchanged.
    pub comment_with_analysis: Option<String>,
    /// An engine that failed to start or stopped unexpectedly, once.
    pub engine_failure: Option<EngineFailureView>,
    /// A notice about analysis, once.
    pub engine_notice: Option<EngineNotice>,
    /// The running 人机对弈 (also while it gives the engine back).
    pub game: Option<GameView>,
    /// How the last game ended, once.
    pub game_outcome: Option<GameOutcomeView>,
    /// 人机对弈 messages, once each, in order.
    pub game_notices: Vec<GameNotice>,
    pub engine_match: Option<MatchView>,
    pub match_outcome: Option<MatchOutcomeView>,
    pub match_notices: Vec<MatchNotice>,
    pub match_winrate_image: Option<MatchWinrateImageRequest>,
    pub human_sl: Option<HumanSlView>,
    pub strength: StrengthView,
    pub human_sl_notices: Vec<HumanSlNotice>,
    pub human_sl_continuation: Option<HumanSlContinuation>,
    /// 读秒 countdown voices to play (`sound/<n>.wav`), once each.
    pub byoyomi_voices: Vec<u8>,
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
    pub editing: EditingView,
    /// 存档与读档.
    pub recovery: RecoveryView,
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
    /// The user did not choose to import it.
    NotChosen,
    /// Java keeps the key but no longer uses it.
    Obsolete,
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
    Appearance,
    ReadKomi,
    LoadSgfLast,
    ReplaceFileHint,
    AppendWinrateToComment,
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
    /// `ui.allow-double-click`.
    AllowDoubleClick,
    /// `ui.allow-drag`.
    AllowDrag,
    /// `ui.show-right-menu`.
    RightClickMenu,
    /// `ui.show-new-board-hint`.
    NewBoardHint,
    /// `ui.show-markup-tools`.
    MarkupTools,
    /// `ui.other-size-width` / `ui.other-size-height`.
    OtherBoardSize,
    /// `ui.no-capture`.
    NoCapture,
    /// `ui.auto-save-exit`.
    AutoSaveOnExit,
    /// `ui.resume-previous-game`.
    ResumePreviousGame,
    /// `ui.load-save-zoom`.
    ZoomThumbnails,
    /// `persist:filesystem.last-image-folder`.
    LastImageFolder,
    /// `ui.win-rate-always-black`.
    WinrateAlwaysBlack,
    /// `ui.show-katago-score-lead-with-komi`.
    ScoreLeadWithKomi,
    /// `ui.show-coordinates`.
    Coordinates,
    /// `ui.show-name-in-board`.
    NamesOnBoard,
    /// 手数: `ui.allow-move-number` (or the older `show-move-number` / `only-last-move-number`),
    /// `movenumber-from-one`, `new-move-number-in-branch`, `show-moveall-inbranch`,
    /// `show-var-move`.
    MoveNumbers,
    /// 落子评价: `ui.move-rank-mark-last-move`, `txt-move-rank-mark-last-move`,
    /// `move-rank-evaluation-mode` (or the older `use-win-loss-in-move-rank` /
    /// `use-score-loss-in-move-rank`), `win-loss-threshold-1..5`, `score-loss-threshold-1..5`.
    MoveRank,
    /// 选点信息: `ui.show-winrate-in-suggestion`, `-playouts-`, `-scoremean-`,
    /// `show-score-as-diff`, `suggestion-info-*`, `delay-show-candidates`,
    /// `delay-candidates-seconds`, `show-suggestion-order`, `show-suggestion-maxred`,
    /// `white-suggestion-white`, `show-suggestion-variations`, `norefresh-onmouse-move`,
    /// `replay-branch-interval-seconds`, `min-playout-ratio-for-stats`.
    Suggestions,
    /// 下一手: `ui.show-next-moves`, `show-next-move-blunder`, `min-playouts-for-next-move`.
    NextMoves,
    /// 小棋盘设置: `ui.show-heat`, `show-heat-aftercalc`, `subboard-raw`, `no-refresh-on-sub`.
    SubBoard,
    /// 纯网络 ownership painter: Java's five `ui.show-pure-estimate-*` choice flags.
    PureNetwork,
    /// 胜率图设置: `ui.show-win-rate-line`, `show-score-lead-line`, `show-winrate-graph-fill`,
    /// `show-blunder-bar`, `show-mouse-over-winrate-graph`, `initial-max-score-lead`.
    WinrateGraph,
    /// Kata形势显示: `ui.show-katago-estimate*`, `save-kata-estimate-status`,
    /// `use-moves-ownership`, `shortcut-kata-estimate`.
    KataEstimate,
    /// 终局点目 rules: `ui.use-territory-in-score`.
    Scoring,
    /// AI match: `ui.match-ai-moves`, `match-ai-percents-playouts`.
    MatchAi,
    /// 独立选点列表: `ui.show-suggestions-frame`, `suggestions-always-ontop`,
    /// `show-bestmoves-list`, `show-bestmoves-graph`, `anaframe-show-next`,
    /// `anaframe-use-mousemove`.
    SuggestionsFrame,
    /// Fixed Java layout selection (`ui.extra-mode`: 0, 1, 2, 3, 7, 8).
    ExtraMode,
    /// Workbench panel/toolbar flags, floating board bounds and custom layout snapshots.
    WorkbenchPanels,
    PanelSizes,
    BottomToolbar,
    /// 加载棋谱后快速刷新胜率曲线: `ui.auto-quick-analyze-on-load`.
    AutoQuickAnalysis,
    /// 精析设置: `ui.analysis-engine-command`, `-command-customized`, `analysis-max-visits`,
    /// `analysis-use-current-rules`, `analysis-specific-rules`, `analysis-always-override`,
    /// `analysis-engine-preload`, `analysis-auto-quit`, `analysis-reuse-current-engine`.
    AnalysisEngine,
    /// 快速曲线轻量模型: `ui.quick-analysis-lightweight-model-enabled`,
    /// `katago-quick-analysis-model-path`.
    QuickModel,
    HumanSlTraining,
    HumanSlModel,
    StrengthBackground,
    /// The model directories of `strength-models` (棋力评估 导入模型).
    StrengthModels,
    /// 死活题: `ui.tsume-go-to-play`, `tsume-go-attaker`, `tsume-go-ko-threat`,
    /// `tsume-go-wall-distance`, `show-tsume-go-menu`.
    Tsumego,
    /// 抓取死活题 参数设置: `ui.capture-black-offset`, `capture-black-percent`,
    /// `capture-white-offset`, `capture-white-percent`, `capture-gray-percent`.
    CaptureTsumego,
    /// 问题手: `ui.is-showing-blunder-table`, `problem-list-side-filter`, `problem-list-sort`,
    /// `problem-list-winrate-threshold`, `blunder-playouts-threshold`,
    /// `hide-blunder-table-control-pane`.
    ProblemList,
    /// 局部精析: `ui.analysis-start-move`, `analysis-end-move`.
    FlashRange,
    AutoAnalysis,
    /// 人机对弈: `ui.check-play-black`, `check-continue-play`, `new-game-handicap`,
    /// `use-free-handicap`, `new-game-komi`, `limit-my-time`, `my-save-time`,
    /// `my-byoyomo-seconds`, `my-byoyomo-times`, `kata-visits-playouts-settings`,
    /// `kata-visits-txt`, `kata-playouts-txt`, `use-play-mode`, `new-game-show-black`,
    /// `new-game-show-white`, `auto-save-played-game`, `use-pure-net-in-game`,
    /// `anagame-resign-start-move`, `anagame-resign-move`, `anagame-resign-percent`,
    /// `disable-wrn-in-game`; `leelaz.play-ponder`.
    HumanEngineGame,
    /// AI用时: `ui.advance-time-settings`, `advance-time-txt`, `kata-time-settings`,
    /// `kata-time-type`, `kata-time-main-time-mins`, `kata-time-byoyomi-secs`,
    /// `kata-time-byoyomi-times`, `kata-time-fisher-increment-secs`, `genmove-game-notime`;
    /// `leelaz.max-game-thinking-time-seconds`.
    GameAiTime,
    /// AI随机开局: `ui.enable-anagame-randomstart`, `anagame-random-move`,
    /// `anagame-random-winratediff`, `anagame-random-playoutdiff`.
    GameRandomStart,
    /// 自动落子: `persist:ui-persist.toolbar-parameter` slots 9–16.
    AutoPlay,
    /// 引擎对战 dialog and toolbar values.
    EngineMatch,
    /// 整盘精析: `ui.whole-game-analysis-deep-visits`.
    WholeGameVisits,
    /// 强制计算区域: `ui.select-allow-moves`, `select-avoid-moves`, `select-allow-custom-moves`,
    /// `select-avoid-custom-moves`.
    AnalysisRegion,
    /// 选点重点研究: `ui.tracking-analysis-max-visits` (older `tracking-engine-max-visits`),
    /// `show-tracking-point-outline`, `tracking-point-outline-opacity`.
    Tracking,
    /// 超级鹰眼: `ui.show-badmoves-frame`, `badmoves-always-ontop`, `movelist-selected-indextop`,
    /// `movelist-selected-index`, `moveList-selected-branch`, `show-movelist-matchai`,
    /// `show-winrate-matchai`, `move-list-winrate-threshold`, `move-list-score-threshold`,
    /// `move-list-visits-threshold`, `movelist-top-curnode`, `move-list-filter-current`,
    /// `match-ai-firstmove`, `match-ai-lastmove`, `opening-end-move`, `middle-end-move`,
    /// `game-statistics-custom-start`, `game-statistics-custom-end`, `winrate-diff-range1/2`,
    /// `score-diff-range1/2`, `loss-panel-select-winrate`.
    Hawkeye,
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
    /// Java 存档与读档 slots; imported only when the user chooses to.
    pub recovery: Vec<ImportSlotView>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ImportReportView {
    pub config_file: String,
    pub items: Vec<ImportItemView>,
    pub engines: Vec<ImportEngineView>,
    /// The imported language applies after a restart.
    pub restart_required: bool,
    pub recovery: Vec<ImportSlotView>,
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

/// The secondary engine (副引擎, Java `Lizzie.leelaz2`) of 双引擎模式: it analyses the same
/// position as the foreground engine but is never the foreground engine. The seat is kept while
/// 双引擎模式 is off (its analysis stops); it is not remembered across restarts.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct SecondaryEngineView {
    /// Position in the engine list.
    pub index: u32,
    /// The configured name, else the GTP name.
    pub name: String,
    pub katago: bool,
    /// Speaks `kata-analyze` / `lz-analyze`.
    pub can_analyse: bool,
    /// Identified and synced to the current position.
    pub ready: bool,
    /// It is analysing the current node (its own 思考 开/关).
    pub analysing: bool,
    /// An analysis limit stopped its analysis of the current node.
    pub stopped_by_limit: bool,
}

/// 双引擎模式's right board (Java `boardRenderer2`): the secondary engine's stored results for the
/// current node and the board marks drawn from them. The position itself is `position`.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct SecondaryView {
    /// Stored secondary analysis of the current node (`BoardData.bestMoves2` and summary).
    pub analysis: Option<NodeAnalysisView>,
    /// Move numbers, 落子评价 and 下一手 详细 drawn from the secondary analysis.
    pub board: crate::display::BoardOverlayView,
    /// The secondary engine's pure-network result for the current position.
    #[serde(rename = "rawNet")]
    pub raw_net: Option<crate::engine::raw_net::RawNetView>,
    /// Kata形势 of the secondary engine (`estimateArray2`): Black-positive ownership of the current
    /// node, else of its parent; absent while Kata形势 is off or nothing was reported.
    pub ownership: Option<Vec<f64>>,
    /// The secondary engine's stored results on the current line and the main trunk, like
    /// `history` (`winrate2`, `scoreMean2` of each node): the 副引擎 独立选点列表's actual-move row
    /// and its score columns (`isContainsKataData2`).
    pub history: Vec<HistoryPoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct EngineView {
    pub engines: Vec<EngineEntryView>,
    pub foreground: Option<ForegroundEngineView>,
    /// The 双引擎模式 secondary engine, while one is chosen.
    pub secondary: Option<SecondaryEngineView>,
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
    /// 复用当前前台引擎 target status (`AnalysisSettings.reuseStatus`).
    pub lease: LeaseAvailabilityView,
    /// The foreground engine was not given back after it was borrowed: it does not analyse until
    /// it is restarted (`LizzieFrame.display.foregroundUnrestored`).
    pub unrestored: Option<UnrestoredReason>,
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
    /// KataGo `pvVisits`, one count per reported variation move; empty when absent.
    pub pv_visits: Vec<u64>,
    /// For each `pv` move, the stones it captures when played out from the node's position
    /// (`Branch`; none under no-capture); the board removes them with `remove-dead-in-variation`.
    pub pv_captures: Vec<Vec<Point>>,
    /// Win rate and score lead as the board and the candidate table show them (总是显示黑胜率,
    /// 目差显示); set by the view projection.
    pub shown: crate::viewpoint::ShownValues,
    /// The side to move's numbers with 盘面 applied, ignoring 总是显示黑胜率 (独立选点列表).
    pub side_to_move: crate::viewpoint::ShownValues,
    /// KataGo `movesOwnership` after this move, Black-positive, row-major from the top-left
    /// (按选点显示评估); absent unless requested and reported.
    pub ownership: Option<Vec<f64>>,
    /// Kept among the candidates beyond 最大候选点数 (`MoveData.lastTimeUnlimited`,
    /// BoardData.tryToLimitMoves): it was within the limit at the node's previous result and is
    /// still searched (more visits, or within 3 seconds).
    pub kept: bool,
}

/// One move's result against the analysis of the position it was played in (Java
/// `Board.updateMovelist` / `NodeInfo`). Present for a move whose previous position has analysis
/// (`analyzedMatchValue`). The changes are the raw differences (the lists show them); statistics
/// count a `best` move as losing nothing (`NodeInfo.getWinrateDiff`, 一选免罚).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MoveEvaluation {
    /// The node the move leads to.
    pub node: NodeId,
    pub move_number: u32,
    /// The side that played the move.
    pub mover: Stone,
    pub point: Point,
    /// 1-based rank of the move among the previous position's candidates, if it is one.
    pub matched: Option<u32>,
    /// 1 for the first candidate, else its visits over the most visits; 0 when not a candidate.
    pub share: f64,
    /// Among the first `match_ai_moves` candidates with at least `match_ai_percents_playouts`.
    pub match_ai: bool,
    pub best: bool,
    /// The position after the move is analysed too (`NodeInfo.analyzed`); the fields below that
    /// need it are `None` otherwise.
    pub analyzed: bool,
    /// Win-rate change for the mover, percentage points (negative is a loss); both positions
    /// analysed.
    pub winrate_change: Option<f64>,
    /// Score-lead change for the mover (KataGo), points.
    pub score_change: Option<f64>,
    /// The mover's win rate after the move (实战胜率).
    pub winrate: Option<f64>,
    /// The mover's score lead after the move (实战目数).
    pub score: Option<f64>,
    /// The previous position's best candidate for the mover (一选胜率 / 一选目数).
    pub best_winrate: f64,
    pub best_score: Option<f64>,
    /// Visits of the position before the move (此手计算量) and after it (下一手计算量, 0 when not
    /// analysed).
    pub visits_before: u64,
    pub visits_after: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct StatisticsView {
    /// Moves of the current line (to its end) and of the main trunk, in path order.
    pub moves: Vec<MoveEvaluation>,
    /// AI总评分 of Black and White, percent: the mean match share of their evaluated moves up to
    /// the current position and the move played from it.
    pub ai_score_black: Option<f64>,
    pub ai_score_white: Option<f64>,
    /// 统计 上一手 change for the move leading to the current node, the current node analysed:
    /// against its parent's analysis, else (Java `drawMoveStatistics`) its grandparent's, two
    /// moves back.
    pub last_move: Option<MoveChange>,
}

/// A change for the side that played the move (negative is a loss).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, TS)]
pub struct MoveChange {
    /// Percentage points.
    pub winrate: f64,
    /// Points (KataGo).
    pub score: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct KataEstimateView {
    /// Session state; [`crate::settings::DisplaySettings::kata_estimate`] is what the next
    /// start begins with.
    pub state: crate::settings::KataEstimateState,
    /// 隐藏: requested but drawn on neither board; `.` toggles the boards.
    pub hidden: bool,
    /// Black-positive ownership of the current node, else (none yet) of its parent, row-major
    /// from the top-left; absent while Kata形势 is off or nothing was reported.
    pub ownership: Option<Vec<f64>>,
}

/// One intersection while scoring (`GroupStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub struct ScorePoint {
    /// Whose area the point counts for; `None` for neutral (dame).
    pub owner: Option<Stone>,
    /// A stone marked dead.
    pub dead: bool,
    /// An empty point marked 不计入.
    pub excluded: bool,
}

/// 终局点目 result (`Board.showGroupResult`, `ScoreResult.setScore`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ScoreView {
    pub node: NodeId,
    /// Row-major from the top-left.
    pub points: Vec<ScorePoint>,
    /// 数目 (territory) instead of 数子 (area).
    pub territory: bool,
    /// Living stones (less the handicap White's handicap bonus takes back under area scoring).
    pub black_alive: u32,
    pub white_alive: u32,
    /// Counted points that are not the side's own living stones.
    pub black_points: u32,
    pub white_points: u32,
    /// Prisoners each side took, dead stones included.
    pub black_captures: u32,
    pub white_captures: u32,
    pub komi: f64,
    pub black_total: f64,
    pub white_total: f64,
    /// SGF result written by 确认结果 (`B+3.5`, `W+0.5`; Black wins ties like Java).
    pub result: String,
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
    /// Black's numbers as the statistics panel and the title show them (the lead with the komi
    /// under 盘面); set by the view projection.
    pub black: crate::viewpoint::ShownValues,
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
    /// Black's numbers as the win-rate graph shows them (the lead with the komi under 盘面).
    pub black: crate::viewpoint::ShownValues,
    /// This position's result as a candidate of the previous position (the candidate table's
    /// actual-move row).
    pub as_candidate: crate::viewpoint::AsCandidate,
    /// The best candidate's score uncertainty (KataGo), for the actual-move row's 目数标准差.
    pub score_stdev: Option<f64>,
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
    /// No foreground engine starts while 起始局面设置模式 is on (`EngineManager.setupModeActive`).
    SetupModeActive,
    /// In 双引擎模式 the foreground and the secondary engine must differ
    /// (`EngineManager.sameEngineHint`); the switch was not made.
    SameEngine,
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
