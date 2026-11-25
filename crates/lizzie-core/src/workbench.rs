//! Application flow (应用流程): the open document, the browsing position, open and save requests
//! in flight, engines and analysis ([`analysis`]), committed settings, the Java settings import
//! ([`importing`]), engine resources ([`resources`]), engine performance ([`tuning`]) and the
//! application data they produce.
//!
//! This is the only writer of that state. Platform adapters run file reading and writing off the
//! UI thread and hand the result back with the ticket they were given; a read whose ticket is no
//! longer the latest request is dropped, so a slow earlier open can never replace a later one, and
//! a failed or abandoned read leaves the current document untouched.
//!
//! A save captures the document text and its identity (document and revision) when it starts,
//! like Java `SgfSaveCoordinator` capturing on the event thread. Its completion confirms only that
//! captured revision of that document: content edited meanwhile stays unsaved, and a document
//! opened meanwhile is not touched. Adapters complete saves in the order they started (one
//! serial writer, like Java `SgfSaveQueue`).

mod analysis;
mod appearance;
mod auto_analysis;
mod batch;
mod commentary;
mod cloud;
mod contribution;
mod diagnostics;
mod editing;
mod exporting;
mod flash;
mod fox;
mod fox_kifu;
mod hawkeye;
mod importing;
mod information;
mod humansl;
mod humansl_model;
mod kata_params;
mod strength;
mod online;
mod matches;
mod panels;
mod play;
mod quick;
mod quick_links;
mod readboard;
mod recovery;
mod resources;
mod restart;
mod search;
mod search_threads;
mod tencent_kifu;
mod tsumego;
mod tuning;
mod web_board;
mod update;
mod whole_game;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use analysis::AnalysisStore;
pub use analysis::LogLine;
pub use editing::{NOT_SQUARE, SETUP_ENGINE_RUNNING, SETUP_MODE_ROOT_ONLY, SETUP_ROOT_ONLY};
pub use flash::ANALYSIS_COMMAND_MISSING;
pub use readboard::ReadBoardInstall;
pub use fox_kifu::FoxKifuJob;
pub use tencent_kifu::TencentKifuJob;
pub use importing::{IMPORT_NEEDS_REPLACE, IMPORT_STALE};
pub use quick_links::QUICK_LINK_FAILED;
pub use recovery::RecoveryStep;
pub use resources::{prepare_resource_setup, run_resource_job, ResourceCompletion, ResourceJob};
pub use update::{UpdateCheckJob, UpdateDownloadJob, UpdateInstallJob, UpdateStep, UpdateTask};

use crate::api::{
    ApiError, DocumentView, EngineFailureView, EngineNotice, ErrorKind, ExportKind, LastAction, LogsStatus,
    OpenOutcome, OpenSource, PendingOpen, PositionView, RulesPromptView, SaveOutcome, SettingsView, StoneSound,
    TreeNodeKind, TreeNodeView, TreeView, WorkbenchView,
};
use crate::browse::{self, Navigation};
use crate::engine::profile::AnalysisSettings;
use crate::engine::runtime::{Engines, InstanceId, Launcher};
use crate::formats::{self, saved_analysis, ReadError};
use crate::record::{BoardSize, GameRecord, NodeId, NodeKind, Point, RecordFormat, Stone, ROOT};
use crate::settings::{
    self, Config, DisplaySettings, ExtraMode, FramePlacement, Language, Persisted, PvVisitsMode, Storage,
    WindowPlacement,
};

/// Java `GameInfo.DEFAULT_KOMI`.
pub const DEFAULT_KOMI: f64 = 7.5;

/// Java `LizzieFrame.prompt.failedToOpenFile`: file opens of either format.
pub const FAILED_TO_OPEN_FILE: &str = "LizzieFrame.prompt.failedToOpenFile";
/// Java `KifuLoad.failed`: clipboard loads.
pub const KIFU_LOAD_FAILED: &str = "KifuLoad.failed";
/// Java `LizzieFrame.saveFileFailed`: any failed save.
pub const SAVE_FILE_FAILED: &str = "LizzieFrame.saveFileFailed";

// Settings-file write failures. The error message is the interface text key, then the cause on
// the next line: `"<key>\n<cause>"` (decoded in `ui/src/errorText.ts`).

/// Settings could not be written; the cause is the operating system's.
pub const SETTINGS_SAVE_FAILED: &str = "product.settingsSaveFailed";
/// The settings file could not be used at start-up and is not written over in this run; the
/// cause names the file and why it was left as it is.
pub const SETTINGS_FILE_KEPT: &str = "product.settingsFileKept";
/// The window state file (placements, panel sizes, last folders) could not be written.
pub const WINDOW_STATE_SAVE_FAILED: &str = "product.windowStateSaveFailed";
/// 重置界面位置 could not delete the window state file.
pub const WINDOW_STATE_RESET_FAILED: &str = "product.windowStateResetFailed";

fn settings_error(key: &str, cause: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorKind::Io, format!("{key}\n{cause}"))
}

#[derive(Clone, Debug)]
pub struct Document {
    pub id: u64,
    pub record: Arc<GameRecord>,
    pub format: Option<RecordFormat>,
    pub path: Option<String>,
    /// Content version, increased by every content change.
    pub revision: u64,
    /// The revision that is the content of the file at `path`, if any.
    pub saved_revision: Option<u64>,
    /// The 存档与读档 slot file it was loaded from, shown while it has no file of its own.
    pub origin_name: Option<String>,
}

impl Document {
    /// The document's own SGF file, which "保存" overwrites (`LizzieFrame.saveOriFile`: any file
    /// the record came from or was saved to, except a GIB, which is never written).
    pub fn own_sgf_file(&self) -> Option<&str> {
        self.path.as_deref().filter(|p| {
            !Path::new(p)
                .file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase().ends_with(".gib"))
        })
    }

    pub fn unsaved(&self) -> bool {
        self.saved_revision != Some(self.revision)
    }
}

/// What "保存" does for the current document (`LizzieFrame.saveOriFile`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavePlan {
    /// Overwrite the document's own file.
    Overwrite { path: String },
    /// Ask before overwriting the document's own file.
    Confirm { document: u64, file_name: String },
    /// The document has no SGF file of its own: choose one.
    Choose,
}

/// A captured save, to be written by the platform adapter and handed back with
/// [`Workbench::complete_save`].
#[derive(Clone, Debug)]
pub struct SaveJob {
    pub ticket: u64,
    pub path: PathBuf,
    /// Automatic-analysis output creates its parent folder on the serial writer.
    pub create_parent: bool,
    pub text: String,
}

#[derive(Clone, Debug)]
struct PendingSave {
    ticket: u64,
    document: u64,
    revision: u64,
    path: String,
    /// A 更多保存 export rather than a save of the record.
    export: Option<ExportKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenOrigin {
    File {
        path: String,
        source: OpenSource,
    },
    Clipboard,
    /// A record downloaded by 野狐棋谱 or 腾讯棋谱 (`loadDownloadedSgfString`): no file of its own.
    Downloaded(KifuSource),
    /// A 存档与读档 slot (`loadFile(file, fromTemp = true)`): the record gets no file of its own and
    /// the recent list and last folder stay; `name` is the slot file shown in the title.
    Recovery {
        path: String,
        name: String,
        position: SlotPosition,
    },
    /// A record read from an online link (打开在线链接, 弈客直播): no file of its own. A followed
    /// live game (`follow`) starts at the end of its main line (`syncLiveBoardStat` first sync).
    Online { follow: bool },
}

/// The kifu window a downloaded record came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KifuSource {
    Fox,
    Tencent,
}

/// Where browsing starts after a slot is loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotPosition {
    /// The position it was saved at: child indexes from the root (Java replays the slot's move
    /// list).
    Path(Vec<u32>),
    /// The end of the main line (start-up resume: `while (nextMove)`).
    LineEnd,
}

#[derive(Clone, Debug)]
struct Pending {
    ticket: u64,
    origin: OpenOrigin,
}

/// What a paste request should do (`LizzieFrame.pasteSgf` / `pasteSgfDecision`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteDecision {
    /// Empty clipboard or not SGF text: silently ignored.
    Ignore,
    /// The current record has content; ask before replacing it.
    Confirm,
    Proceed,
}

pub struct Workbench {
    storage: Storage,
    config: Config,
    persisted: Persisted,
    /// Set by "重置界面位置": the persist file stays deleted until the next start, except for
    /// what Java imports write over the defaults meanwhile; this is the file's content.
    persist_reset: Option<Persisted>,
    active_language: Language,
    system_locale: Option<String>,
    active_frame_font_size: i32,
    appearance_theme: crate::appearance::ThemeSettings,
    /// See [`crate::api::SettingsView::theme_files_revision`].
    theme_files_revision: u32,
    doc: Document,
    cursor: NodeId,
    komi: f64,
    next_doc: u64,
    /// Java `Board.isPkBoard` with `isPkBoardKataB` / `isPkBoardKataW`: the document that is an
    /// engine game and the side whose engine gives its score curve. It applies to that document
    /// only; every replacement of the document (new board, opened record) has a new id, which is
    /// Java clearing the marks (`Board.clear(false)`, `setHistory`).
    pk_board: Option<(u64, Option<Stone>)>,
    /// Java `Board.isKataBoard` beyond the stores' KataGo data: the document read with a KataGo
    /// root `DZ` or rebuilt from a contribution game. A save writes `DZ[G]` for it
    /// ([`Workbench::save_mark`]); it ends with the engine-game marks.
    kata_board: Option<u64>,
    next_ticket: u64,
    pending: Option<Pending>,
    outcome: Option<OpenOutcome>,
    next_save: u64,
    saves: Vec<PendingSave>,
    save_outcome: Option<SaveOutcome>,
    seq: u64,
    /// Document and revision whose tree the UI has received.
    tree_sent_for: Option<(u64, u64)>,
    sounds: Vec<StoneSound>,
    engines: Engines,
    store: AnalysisStore,
    /// 双引擎模式's secondary engine results for the open document (`BoardData.bestMoves2` and
    /// its summary), read from `LZ2`/`LZOP2` or reported by the secondary engine.
    secondary_store: AnalysisStore,
    /// The user paused analysis (session only, like Java `userAnalysisPaused`).
    paused: bool,
    /// 综合设置 is open: no engine analyses meanwhile, whatever the mode (see
    /// [`Workbench::hold_analysis`]).
    analysis_hold: bool,
    /// 优先分析此点 of the current position (`LizzieFrame.priorityMoveCoords`): session only,
    /// cleared by a move and by leaving the position.
    priority_points: Vec<Point>,
    /// The foreground engine the recorded points were sent to: a restart or switch gives an
    /// engine without those policies, so the list is not shown for it.
    priority_engine: Option<InstanceId>,
    /// The document and node the points were recorded for: another position never shows them.
    priority_at: Option<(u64, NodeId)>,
    /// The user chose this game's komi (`GameInfo.changedKomi`): engine profiles no longer set it.
    komi_changed: bool,
    /// A newly started foreground engine whose profile komi applies to a still empty game once
    /// it is ready (`Leelaz.firstLoad`).
    start_komi: Option<(InstanceId, f64)>,
    /// A limit stopped the analysis of this position (document, node).
    limit_stop: Option<(u64, NodeId)>,
    /// The foreground search a limit (not 空棋盘停止计算) stopped: its visits and when it began
    /// (`getBestMovesPlayouts`, `startPonderTime`), judged again when the limits change.
    limit_search: Option<(u64, std::time::Instant)>,
    /// 分析 黑 / 白 on the top toolbar (`Config.analyzeBlack` / `analyzeWhite`): session only,
    /// both on at start.
    analyze_black: bool,
    analyze_white: bool,
    /// The user's move or browsing reached this position with the side to move not analysed:
    /// ordinary analysis waits here until the position changes or the user resumes it
    /// (`Leelaz.underPonder`).
    side_hold: Option<(u64, NodeId)>,
    /// The limit notice was shown since the foreground engine started.
    limit_tip_shown: bool,
    /// A limit stopped the secondary engine's analysis of this position; only its own (Java
    /// `leelaz2.nameCmd` is not mirrored back to the primary engine).
    secondary_limit_stop: Option<(u64, NodeId)>,
    /// The limit notice was shown since the secondary engine was chosen.
    secondary_limit_tip_shown: bool,
    /// The user resumed the analysis of this empty board after it stopped there: 空棋盘停止计算
    /// no longer stops it until the position changes (`Leelaz.outOfPlayoutsLimit`).
    empty_board_resumed: Option<(u64, NodeId)>,
    /// A secondary engine still starting and the kept previous one it replaced, to return to if
    /// the new one fails during start-up (`rollbackSecondaryEngineSelection`).
    secondary_fallback: Option<(InstanceId, InstanceId)>,
    session_rules: analysis::SessionRules,
    /// Increases whenever the session rules are published anew.
    rules_revision: u64,
    /// The rules target and engines the user chose to analyse with their own rules.
    rules_override: Option<analysis::RulesKey>,
    /// The rules target and engines already asked about.
    rules_prompted: Option<analysis::RulesKey>,
    /// The open question: prompt id and what it is about.
    pending_prompt: Option<(u64, analysis::RulesKey)>,
    next_prompt: u64,
    rules_prompt: Option<RulesPromptView>,
    engine_failures: Vec<EngineFailureView>,
    engine_notice: Option<EngineNotice>,
    logs: Vec<LogLine>,
    /// 诊断与日志: the switches in effect and the 完整日志 session.
    diagnostics: crate::diagnostics::Diagnostics,
    /// No settings file existed at start (Java `isNewProfile`): the Java import is offered.
    new_profile: bool,
    /// Why the settings file is not written in this run ([`settings::SettingsFile::LeftAsItIs`]);
    /// `None` when it may be.
    settings_file_fault: Option<String>,
    /// Java configuration folders found in Java's conventional locations.
    java_candidates: Vec<String>,
    /// The Java import the user is looking at, by preview id.
    java_preview: Option<(u64, crate::java_import::Candidate)>,
    next_preview: u64,
    /// Engine resources: detection, recommendation, acquisition tasks and setup plans.
    resources: resources::ResourceFlow,
    /// Engine performance: benchmark tasks and thread-policy changes.
    tuning: tuning::TuningFlow,
    /// Starting-position setup mode and the delete/edit undo cache.
    editing: editing::Editing,
    /// 存档与读档 slots.
    recovery: crate::recovery::Store,
    /// Kata形势 this session.
    estimate: information::EstimateSession,
    /// 终局点目 in progress.
    scoring: Option<information::Scoring>,
    /// 自动棋谱快析 of the open record.
    quick: quick::QuickFlow,
    /// 闪电分析 of the open record.
    flash: flash::FlashFlow,
    auto_analysis: auto_analysis::AutoAnalysisFlow,
    whole_game: whole_game::WholeGameFlow,
    /// 批量分析 / 批量精析: the queue and its background record.
    batch: batch::BatchFlow,
    /// 强制计算区域 and 选点重点研究 of the foreground engine's analysis.
    search: search::SearchFlow,
    /// The problem move a 问题手 jump marked, while browsing the position before it
    /// (`clickbadmove`): document, that position, the move's node.
    problem_mark: Option<(u64, NodeId, NodeId)>,
    /// 超级鹰眼's 黑 / 白 boxes this session.
    hawkeye_session: hawkeye::HawkeyeSession,
    /// 人机对弈 and 自动落子.
    play: play::PlayFlow,
    fox: fox::FoxFlow,
    matches: matches::MatchFlow,
    /// Engines that ended unexpectedly and start again (自动检测引擎是否存活).
    restart: restart::RestartFlow,
    /// 引擎参数 window's 搜索线程数 operation.
    threads: search_threads::SearchThreadsFlow,
    human_sl: humansl::HumanSlFlow,
    strength: strength::StrengthFlow,
    online: online::OnlineFlow,
    commentary: commentary::CommentaryFlow,
    /// 跑谱贡献: the KataGo distributed-training session.
    contribution: contribution::ContributionFlow,
    cloud_activation: Option<String>,
    /// Session credential availability; never persisted with cloud settings.
    cloud_authenticated: bool,
    /// 棋盘同步工具 and its GMA auto-play.
    readboard: readboard::ReadBoardState,
    fox_kifu: fox_kifu::FoxKifuFlow,
    tencent_kifu: tencent_kifu::TencentKifuFlow,
    /// Web 旁观's running session and a viewer's 试下.
    web_board: web_board::WebBoardFlow,
    /// 检查更新: the check, offer, package preparation and installation handover.
    update: update::UpdateFlow,
}

impl Workbench {
    /// Loads committed settings and data; starts on an empty 19x19 record, Black to play,
    /// komi 7.5 (Java startup `new Board()`). Engines start with [`Workbench::start_engines`];
    /// `launcher` starts their processes and `engine_roots` are where relative engine paths are
    /// looked up.
    pub fn new(
        storage: Storage,
        system_locale: Option<&str>,
        launcher: Box<dyn Launcher>,
        engine_roots: Vec<PathBuf>,
    ) -> Workbench {
        let resources = resources::ResourceFlow::new(&storage);
        // The start-up rotation of a crashed session's periodic save happens here, before any
        // resume (see `recovery`).
        let recovery = crate::recovery::Store::open(&storage.save_dir);
        let loaded = settings::load_config(&storage.config_file);
        let (new_profile, settings_file_fault) = match loaded.file {
            settings::SettingsFile::Missing => (true, None),
            settings::SettingsFile::Usable => (false, None),
            settings::SettingsFile::LeftAsItIs(cause) => (false, Some(cause)),
        };
        let mut config = loaded.config;
        config.commentary.preferences = config.commentary.preferences.normalized();
        // Config.java:3907-3912: mode policy also applies when opening an older config.
        if config.ui.extra_mode.hides_comment() {
            config.panels.show_comment = false;
        }
        let persisted: Persisted = settings::load_json(&storage.persist_file);
        let active_language = config.ui.language.resolve(system_locale);
        let active_frame_font_size = config.appearance.frame_font_size;
        let appearance_theme = crate::appearance::load_theme(&storage, &config.appearance.theme, &config.appearance_defaults);
        let mut engines = Engines::new(launcher, engine_roots);
        let diagnostics = crate::diagnostics::Diagnostics::new(storage.logs_dir.clone());
        diagnostics.apply(&config.logging);
        engines.set_diagnostics(diagnostics.clone());
        let runtime = storage.resources_dir.join("runtime/nvidia");
        if runtime.is_dir() {
            engines.set_library_dirs(vec![runtime]);
        }
        engines.set_interval(config.engine.analysis.interval_centisec, config.engine.analysis.interval_centisec_ssh);
        engines.set_always_gtp(config.engine.analysis.always_gtp);
        engines.set_pv_visits(config.display.pv_visits != PvVisitsMode::Off);
        let estimate = information::EstimateSession::starting(config.display.kata_estimate);
        engines.set_ownership(estimate.state.shown, config.display.use_moves_ownership);
        let mut store = AnalysisStore::default();
        store.reset(1);
        let mut secondary_store = AnalysisStore::default();
        secondary_store.reset(1);
        Workbench {
            storage,
            config,
            persisted,
            persist_reset: None,
            active_language,
            active_frame_font_size,
            appearance_theme,
            theme_files_revision: 0,
            system_locale: system_locale.map(str::to_string),
            doc: Document {
                id: 1,
                record: Arc::new(GameRecord::empty(BoardSize::STANDARD, DEFAULT_KOMI)),
                format: None,
                path: None,
                revision: 0,
                saved_revision: None,
                origin_name: None,
            },
            cursor: ROOT,
            komi: DEFAULT_KOMI,
            next_doc: 2,
            pk_board: None,
            kata_board: None,
            next_ticket: 1,
            pending: None,
            outcome: None,
            next_save: 1,
            saves: Vec::new(),
            save_outcome: None,
            seq: 0,
            tree_sent_for: None,
            sounds: Vec::new(),
            engines,
            store,
            secondary_store,
            paused: false,
            analysis_hold: false,
            priority_points: Vec::new(),
            priority_engine: None,
            priority_at: None,
            komi_changed: false,
            start_komi: None,
            limit_stop: None,
            limit_search: None,
            analyze_black: true,
            analyze_white: true,
            side_hold: None,
            limit_tip_shown: false,
            secondary_limit_stop: None,
            secondary_limit_tip_shown: false,
            empty_board_resumed: None,
            secondary_fallback: None,
            session_rules: analysis::SessionRules::Unspecified,
            rules_revision: 0,
            rules_override: None,
            rules_prompted: None,
            pending_prompt: None,
            next_prompt: 0,
            rules_prompt: None,
            engine_failures: Vec::new(),
            engine_notice: None,
            logs: loaded.notice.into_iter().map(|text| LogLine { warn: true, text }).collect(),
            diagnostics,
            new_profile,
            settings_file_fault,
            java_candidates: Vec::new(),
            java_preview: None,
            next_preview: 0,
            resources,
            tuning: tuning::TuningFlow::default(),
            editing: editing::Editing::default(),
            recovery,
            estimate,
            scoring: None,
            quick: quick::QuickFlow::default(),
            flash: flash::FlashFlow::default(),
            auto_analysis: auto_analysis::AutoAnalysisFlow::default(),
            whole_game: whole_game::WholeGameFlow::default(),
            batch: batch::BatchFlow::default(),
            search: search::SearchFlow::default(),
            problem_mark: None,
            hawkeye_session: hawkeye::HawkeyeSession::default(),
            play: play::PlayFlow::default(),
            fox: fox::FoxFlow::default(),
            matches: matches::MatchFlow::default(),
            restart: restart::RestartFlow::default(),
            threads: search_threads::SearchThreadsFlow::default(),
            human_sl: humansl::HumanSlFlow::default(),
            strength: strength::StrengthFlow::default(),
            online: online::OnlineFlow::default(),
            commentary: commentary::CommentaryFlow::default(),
            contribution: contribution::ContributionFlow::default(),
            cloud_activation: None,
            cloud_authenticated: false,
            readboard: readboard::ReadBoardState::default(),
            fox_kifu: fox_kifu::FoxKifuFlow::default(),
            tencent_kifu: tencent_kifu::TencentKifuFlow::default(),
            web_board: web_board::WebBoardFlow::default(),
            update: update::UpdateFlow::default(),
        }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn cursor(&self) -> NodeId {
        self.cursor
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn persisted(&self) -> &Persisted {
        &self.persisted
    }

    pub fn active_language(&self) -> Language {
        self.active_language
    }

    fn save_config(&self) -> Result<(), ApiError> {
        self.write_config(&self.config)
    }

    fn write_config(&self, config: &Config) -> Result<(), ApiError> {
        self.settings_file_writable()?;
        settings::save_json(&self.storage.config_file, config).map_err(|e| settings_error(SETTINGS_SAVE_FAILED, e))
    }

    /// Settings and window state as one write ([`settings::save_both`]).
    fn save_settings_files(&self, config: &Config, persisted: Option<&Persisted>) -> Result<(), ApiError> {
        self.settings_file_writable()?;
        settings::save_both(&self.storage, config, persisted).map_err(|e| settings_error(SETTINGS_SAVE_FAILED, e))
    }

    /// Refuses to write over a settings file that was there at start-up but could not be loaded
    /// (nor kept as a backup): it holds the user's settings, which the next start can still read.
    fn settings_file_writable(&self) -> Result<(), ApiError> {
        let Some(cause) = &self.settings_file_fault else {
            return Ok(());
        };
        Err(settings_error(SETTINGS_FILE_KEPT, format_args!("{}: {cause}", self.storage.config_file.display())))
    }

    /// A user settings change: applied only once it is on disk, so the projection never shows a
    /// state that a restart would lose.
    fn commit_config(&mut self, change: impl FnOnce(&mut Config)) -> Result<(), ApiError> {
        let mut next = self.config.clone();
        change(&mut next);
        self.write_config(&next)?;
        self.config = next;
        Ok(())
    }

    fn save_persisted(&self) -> Result<(), ApiError> {
        if self.persist_reset.is_some() {
            return Ok(());
        }
        settings::save_json(&self.storage.persist_file, &self.persisted)
            .map_err(|e| settings_error(WINDOW_STATE_SAVE_FAILED, e))
    }

    // ---------------------------------------------------------------- opening

    /// Registers an open request. Any earlier request still reading is superseded.
    pub fn begin_open(&mut self, origin: OpenOrigin) -> u64 {
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        // A read still waiting is replaced; a kifu download that started it ends unopened.
        if let Some(replaced) = self.pending.replace(Pending { ticket, origin }) {
            self.download_open_ended(&replaced.origin, replaced.ticket, false);
        }
        ticket
    }

    /// Whether `ticket` is still the request the user is waiting for.
    pub fn is_current_open(&self, ticket: u64) -> bool {
        self.pending.as_ref().is_some_and(|p| p.ticket == ticket)
    }

    /// Ends an open request whose source produced no record (a failed kifu download, reported by
    /// its own window): nothing is published and the current document stays.
    fn abandon_open(&mut self, ticket: u64) {
        if let Some(abandoned) = self.pending.take_if(|p| p.ticket == ticket) {
            self.download_open_ended(&abandoned.origin, ticket, false);
            // A preload waiting for this read decides now.
            self.warmup_review();
        }
    }

    /// Tells the kifu window whose download started an open request that it ended (`adopted`:
    /// the record became the document).
    fn download_open_ended(&mut self, origin: &OpenOrigin, ticket: u64, adopted: bool) {
        match origin {
            OpenOrigin::Downloaded(KifuSource::Fox) => self.fox_kifu_open_ended(ticket, adopted),
            OpenOrigin::Downloaded(KifuSource::Tencent) => self.tencent_kifu_open_ended(ticket, adopted),
            _ => {}
        }
    }

    /// Withdraws the open `ticket`, a read waiting for a borrowed engine included: it is never
    /// adopted.
    pub(super) fn withdraw_open(&mut self, ticket: u64) {
        if self.pending.take_if(|p| p.ticket == ticket).is_some() {
            self.quick_drop_deferred_open(ticket);
        }
    }

    /// Hands back a finished read. Returns `false` (and changes nothing) for a superseded ticket.
    /// A failure keeps the current document and publishes the Java failure message.
    pub fn complete_open(&mut self, ticket: u64, result: Result<(RecordFormat, GameRecord), ReadError>) -> bool {
        let Some(pending) = self.pending.take_if(|p| p.ticket == ticket) else {
            return false;
        };
        match result {
            Err(e) => {
                let message_key = match pending.origin {
                    OpenOrigin::File { .. } | OpenOrigin::Recovery { .. } => FAILED_TO_OPEN_FILE,
                    OpenOrigin::Clipboard | OpenOrigin::Downloaded(_) | OpenOrigin::Online { .. } => KIFU_LOAD_FAILED,
                };
                self.outcome = Some(OpenOutcome::Failed {
                    ticket,
                    message_key: message_key.to_string(),
                    detail: e.to_string(),
                });
                self.online_open_failed(ticket);
            }
            Ok(_) if self.game_active() || self.match_active() || self.fox_owns_board() => {
                // A read started before the game began is not adopted into it.
                self.outcome = Some(OpenOutcome::Failed {
                    ticket,
                    message_key: play::OPEN_IN_GAME.to_string(),
                    detail: String::new(),
                });
                self.online_open_failed(ticket);
            }
            Ok(read) => {
                self.auto_analysis_document_replaced();
                // Opening waits while a borrowed foreground engine is given back.
                let Some((_, Ok((format, mut record)))) = self.quick_defer_open((ticket, Ok(read))) else {
                    self.pending = Some(pending);
                    return true;
                };
                // Java applies a record komi when automatic loading is on or the game starts
                // from setup/handicap; otherwise the session komi stays (`resetAllNoKomi`). A
                // 腾讯棋谱 record is read with automatic loading off (its KM is komi × 100).
                let read_komi = match pending.origin {
                    OpenOrigin::Downloaded(KifuSource::Tencent) => false,
                    _ => self.config.record.read_komi,
                };
                let use_record_komi = record.komi_declared && (read_komi || record.has_setup_or_handicap);
                if use_record_komi {
                    self.komi = record.info.komi;
                } else {
                    record.info.komi = self.komi;
                }
                record.number_branches_as_read(self.config.display.branch_numbers_from_one);
                let id = self.next_doc;
                self.next_doc += 1;
                let record = Arc::new(record);
                self.cursor = match &pending.origin {
                    OpenOrigin::Recovery { position, .. } => match position {
                        SlotPosition::Path(path) => crate::recovery::follow(&record, path),
                        SlotPosition::LineEnd => browse::navigate(&record, ROOT, Navigation::Last),
                    },
                    OpenOrigin::Online { follow: true } => browse::navigate(&record, ROOT, Navigation::Last),
                    _ if self.config.record.load_sgf_last => browse::navigate(&record, ROOT, Navigation::Last),
                    _ => ROOT,
                };
                let (path, origin_name) = match &pending.origin {
                    OpenOrigin::File { path, .. } => (Some(path.clone()), None),
                    OpenOrigin::Clipboard | OpenOrigin::Downloaded(_) | OpenOrigin::Online { .. } => (None, None),
                    OpenOrigin::Recovery { name, .. } => (None, Some(name.clone())),
                };
                if let OpenOrigin::File {
                    path,
                    source: OpenSource::Chooser,
                } = &pending.origin
                {
                    self.config.remember_recent(path);
                    if let Some(parent) = Path::new(path).parent() {
                        self.persisted.last_folder = Some(parent.to_string_lossy().into_owned());
                    }
                    // Failing to record the recent list must not undo the adopted document.
                    let _ = self.save_config();
                    let _ = self.save_persisted();
                }
                // The content is the file it came from; pasted text and slots are no file of the
                // record's own.
                let saved_revision = path.as_ref().map(|_| 0);
                self.doc = Document {
                    id,
                    record,
                    format: Some(format),
                    path,
                    revision: 0,
                    saved_revision,
                    origin_name,
                };
                self.read_record_marks();
                // Loading is silent (Java turns sound off while it builds the history).
                self.sounds.clear();
                self.readboard_history_overwritten();
                self.reset_analysis();
                let (settings, no_circle) = (&self.config.engine.analysis, self.config.display.no_sugg_circle);
                self.store
                    .load(&self.doc.record, saved_analysis::PRIMARY, settings, no_circle);
                self.secondary_store
                    .load(&self.doc.record, saved_analysis::SECONDARY, settings, no_circle);
                self.refresh_pv_captures();
                // Secondary-engine data offers 双引擎模式 (`SGFParse.doubleEngineHint`); the data is
                // kept whatever the answer.
                let double_engine_hint =
                    !self.secondary_store.is_empty() && self.config.ui.extra_mode != ExtraMode::DoubleEngine;
                self.outcome = Some(OpenOutcome::Adopted {
                    ticket,
                    document: id,
                    double_engine_hint,
                });
                // A loaded record is a new analysis context: its komi is not a user choice and
                // a user pause of the previous record ends
                // (`startNewKifuAnalysisContextAfterSuccessfulLoad`). A pause of 批量分析, which
                // analyses its own record, is not the previous record's and stays.
                self.komi_changed = false;
                if !self.auto_analysis_holds_foreground() {
                    self.paused = false;
                }
                self.reset_editing();
                self.limit_stop = None;
                let declared = match format {
                    RecordFormat::Sgf => self.doc.record.info.rules.clone(),
                    RecordFormat::Gib => None,
                };
                self.publish_record_rules(declared.as_deref());
                self.sync_engine();
                self.quick_after_load();
                self.online_adopted(ticket);
            }
        }
        let adopted = matches!(self.outcome, Some(OpenOutcome::Adopted { ticket: t, .. }) if t == ticket);
        self.download_open_ended(&pending.origin, ticket, adopted);
        // A preload waiting for this read decides now (also after a failed read).
        self.warmup_review();
        true
    }

    /// Paste decision for clipboard text (`SGFParser.isSGF`, `currentGameHasPasteRisk`).
    pub fn paste_decision(&self, text: &str, confirmed: bool) -> PasteDecision {
        let looks_like_sgf = text.find('(').is_some_and(|open| text[open..].contains(')'));
        if text.is_empty() || !looks_like_sgf {
            return PasteDecision::Ignore;
        }
        let rec = &self.doc.record;
        let has_content = rec.has_children_of_root() || !rec.node(ROOT).setup.is_empty();
        if has_content && !confirmed {
            PasteDecision::Confirm
        } else {
            PasteDecision::Proceed
        }
    }

    /// Reads clipboard text synchronously into the same commit path as files; `no_capture` is
    /// the `ui.no-capture` setting when the read starts, as for files.
    pub fn read_clipboard_record(text: &str, no_capture: bool) -> Result<(RecordFormat, GameRecord), ReadError> {
        formats::parse(RecordFormat::Sgf, text, no_capture).map(|r| (RecordFormat::Sgf, r))
    }

    // ---------------------------------------------------------------- saving

    /// What "保存" does now. `confirmed` is the document the user already confirmed the
    /// replacement for; a confirmation given for another document does not carry over.
    pub fn save_plan(&self, confirmed: Option<u64>) -> SavePlan {
        let Some(path) = self.doc.own_sgf_file() else {
            return SavePlan::Choose;
        };
        if self.config.record.show_replace_file_hint && confirmed != Some(self.doc.id) {
            let file_name = Path::new(path)
                .file_name()
                .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());
            return SavePlan::Confirm {
                document: self.doc.id,
                file_name,
            };
        }
        SavePlan::Overwrite { path: path.to_string() }
    }

    /// Captures the current document for a save to `path`, written exactly as named (a chosen name
    /// gets its `.sgf` from [`formats::save_target`] before this). `application` is written as the
    /// record's `AP`.
    pub fn begin_save(&mut self, path: &str, application: &str) -> SaveJob {
        self.prepare_save_comments();
        let ticket = self.next_save;
        self.next_save += 1;
        self.saves.push(PendingSave {
            ticket,
            document: self.doc.id,
            revision: self.doc.revision,
            path: path.to_string(),
            export: None,
        });
        SaveJob {
            ticket,
            path: PathBuf::from(&path),
            text: self.record_text(application),
            create_parent: false,
        }
    }

    /// Hands back a finished write. A success confirms the captured revision; while that document
    /// is still open it now belongs to the written file, and the chooser starts in its folder next
    /// time (`SgfSaveCoordinator` success callback). An export confirms nothing: only 纯净棋谱
    /// makes the written file the record's own (Java `Mode.RAW.updateCurrentFile`), still unsaved
    /// since that file lacks its comments and analysis. Returns `false` for an unknown ticket.
    pub fn complete_save(&mut self, ticket: u64, result: Result<(), String>) -> bool {
        let Some(at) = self.saves.iter().position(|s| s.ticket == ticket) else {
            return false;
        };
        let save = self.saves.remove(at);
        if self.batch_save_finished(ticket, &result)
            || self.play_save_finished(ticket, &result)
            || self.match_save_finished(ticket, &result)
            || self.contribution_save_finished(ticket, &result)
        {
            return true;
        }
        let automatic = self.auto_analysis_save_finished(ticket, result.is_ok());
        match result {
            Err(detail) => {
                self.save_outcome = Some(SaveOutcome::Failed {
                    ticket,
                    document: save.document,
                    path: save.path,
                    message_key: SAVE_FILE_FAILED.to_string(),
                    detail,
                });
            }
            Ok(()) => {
                if self.doc.id == save.document && !automatic {
                    match save.export {
                        None => {
                            self.doc.path = Some(save.path.clone());
                            self.doc.format = Some(RecordFormat::Sgf);
                            self.doc.saved_revision = Some(save.revision);
                        }
                        Some(ExportKind::Raw) => {
                            self.doc.path = Some(save.path.clone());
                            self.doc.format = Some(RecordFormat::Sgf);
                            self.doc.saved_revision = None;
                        }
                        Some(ExportKind::RawWithComments | ExportKind::Branch) => {}
                    }
                    if let Some(parent) = Path::new(&save.path).parent() {
                        self.persisted.last_folder = Some(parent.to_string_lossy().into_owned());
                        // The written file stands even if the folder cannot be remembered.
                        let _ = self.save_persisted();
                    }
                }
                self.save_outcome = Some(SaveOutcome::Saved {
                    ticket,
                    document: save.document,
                    revision: save.revision,
                    path: save.path,
                    export: save.export,
                });
            }
        }
        true
    }

    /// Whether accepted writes are still running; exit waits for them (`pendingSaves`).
    pub fn has_pending_saves(&self) -> bool {
        !self.saves.is_empty()
    }

    // ---------------------------------------------------------------- document content
    // The content write path is `apply_edit` / `edit_document` in `editing`.

    // ---------------------------------------------------------------- browsing

    /// Moves the browsing position. Every node the route enters forward sounds, in order, like
    /// Java's step-by-step navigation (`moveHistoryForward` → `Utils.playVoiceFile`, which
    /// compares the node's captures with its parent's); backward steps are silent.
    fn move_to(&mut self, target: NodeId) -> bool {
        if target == self.cursor {
            return false;
        }
        let rec = &self.doc.record;
        let from = rec.path_to(self.cursor);
        let to = rec.path_to(target);
        let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
        self.sounds = if self.sounds_on() {
            to[shared..].iter().map(|&n| step_sound(rec, n)).collect()
        } else {
            Vec::new()
        };
        self.human_sl_position_changed();
        // 优先分析此点 is recorded for the shown position only.
        self.priority_points.clear();
        self.priority_engine = None;
        self.priority_at = None;
        self.cursor = target;
        self.sync_engine();
        self.quick_navigated();
        true
    }

    /// A move, 悔棋 or browsing step of the user's own: Java sends it to the engine as `play` /
    /// `undo`, after which ordinary analysis goes on only for a side 分析 黑 / 白 keeps
    /// (`Leelaz.playMove`, `Leelaz.undo`). Whole-board syncs (opening a record, 棋盘同步) use
    /// [`Self::move_to`] and are not judged.
    pub(super) fn step_to(&mut self, target: NodeId) -> bool {
        if target == self.cursor {
            return false;
        }
        self.hold_for_side(target);
        self.move_to(target)
    }

    /// Stone sounds play (`ui.play-sound`), except while the board is synced with
    /// 同步时不播放落子声 on (`Utils.playVoiceFile`).
    fn sounds_on(&self) -> bool {
        self.config.ui.play_sound && !(self.config.read_board.no_sound_in_sync && self.readboard_syncing())
    }

    pub fn navigate(&mut self, nav: Navigation) -> bool {
        if self.match_active() || !self.fox_can_browse() { return false; }
        let target = browse::navigate(&self.doc.record, self.cursor, nav);
        self.browse_to(target)
    }

    pub fn goto_node(&mut self, node: NodeId) -> Result<bool, ApiError> {
        if self.match_active() || !self.fox_can_browse() { return Ok(false); }
        if !browse::is_browsable(&self.doc.record, node) {
            return Err(ApiError::new(
                ErrorKind::InvalidArgument,
                format!("no node {node} in this document"),
            ));
        }
        Ok(self.browse_to(node))
    }

    /// A number of 0 is ignored like Java's empty/invalid move field.
    pub fn goto_move_number(&mut self, number: u32) -> bool {
        if self.match_active() || !self.fox_can_browse() { return false; }
        if number == 0 {
            return false;
        }
        let target = browse::goto_move_number(&self.doc.record, self.cursor, number);
        self.browse_to(target)
    }

    /// The user moves the view (a synced board then no longer resumes analysis by itself).
    fn browse_to(&mut self, target: NodeId) -> bool {
        let moved = self.step_to(target);
        if moved {
            self.readboard_navigated();
        }
        moved
    }

    // ---------------------------------------------------------------- settings and data

    pub fn set_load_sgf_last(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.record.load_sgf_last == enabled {
            return Ok(false);
        }
        self.commit_config(|c| c.record.load_sgf_last = enabled)?;
        Ok(true)
    }

    pub fn set_read_komi(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.record.read_komi == enabled {
            return Ok(false);
        }
        self.commit_config(|c| c.record.read_komi = enabled)?;
        Ok(true)
    }

    pub fn set_append_winrate_to_comment(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.record.append_winrate_to_comment == enabled { return Ok(false); }
        self.commit_config(|c| c.record.append_winrate_to_comment = enabled)?;
        Ok(true)
    }

    /// 显示 settings: every display and the 带评论 export follow at once.
    pub fn set_display_settings(&mut self, settings: DisplaySettings) -> Result<bool, ApiError> {
        let Some(settings) = self.next_display(settings)? else { return Ok(false) };
        let previous = self.config.display.clone();
        self.commit_config(|c| c.display = settings)?;
        self.display_settings_changed(&previous);
        Ok(true)
    }

    /// Validates display `settings`; `None` when they are the current ones. A changed
    /// 形势判断 save choice brings back the estimate remembered for it.
    fn next_display(&self, settings: DisplaySettings) -> Result<Option<DisplaySettings>, ApiError> {
        let finite = [settings.delay_candidates_seconds, settings.replay_interval_seconds, settings.min_playout_ratio,
            settings.entire_variation_first_seconds, settings.autoplay_main_seconds, settings.initial_max_score_lead,
            settings.match_ai_percents_playouts]
            .iter()
            .chain(&settings.win_loss_thresholds)
            .chain(&settings.score_loss_thresholds)
            .all(|v| v.is_finite());
        if !finite
            || settings.move_rank < -1
            || settings.move_rank_custom == 0
            || settings.suggestion_info_order.iter().any(|r| !(1..=3).contains(r))
            || !(1..=3).contains(&settings.suggestion_color_ratio)
            || (settings.autoplay_main_seconds != -1.0 && settings.autoplay_main_seconds <= 0.0)
            || (settings.autoplay_sub_millis != -1 && settings.autoplay_sub_millis <= 0)
            || settings.initial_max_score_lead < 0.0
        {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "display settings"));
        }
        if self.config.display == settings {
            return Ok(None);
        }
        let mut settings = settings;
        if settings.kata_estimate_save_status != self.config.display.kata_estimate_save_status {
            settings.kata_estimate = self.remembered_estimate(settings.kata_estimate_save_status);
        }
        Ok(Some(settings))
    }

    /// Puts committed display settings into effect.
    fn display_settings_changed(&mut self, previous: &DisplaySettings) {
        self.engines.set_pv_visits(self.config.display.pv_visits != PvVisitsMode::Off);
        self.estimate_settings_changed(previous);
    }

    pub fn set_language(&mut self, language: Language) -> Result<bool, ApiError> {
        if self.config.ui.language == language {
            return Ok(false);
        }
        self.commit_config(|c| c.ui.language = language)?;
        Ok(true)
    }

    /// 初始化设置 确定 (`FirstUseSettings` sets every value, then saves once): display,
    /// analysis and language in one write, each then taking effect as its own setter does. A
    /// failed write keeps all three as they were. The mode gates judge the command first
    /// ([`Self::game_gate`]).
    pub fn save_init_settings(
        &mut self,
        display: DisplaySettings,
        analysis: AnalysisSettings,
        language: Language,
    ) -> Result<bool, ApiError> {
        let display = self.next_display(display)?;
        let analysis = (self.config.engine.analysis != analysis).then_some(analysis);
        let language = (self.config.ui.language != language).then_some(language);
        if display.is_none() && analysis.is_none() && language.is_none() {
            return Ok(false);
        }
        let previous_display = self.config.display.clone();
        let previous_analysis = self.config.engine.analysis.clone();
        let (display_changed, analysis_changed) = (display.is_some(), analysis.is_some());
        self.commit_config(|c| {
            if let Some(display) = display {
                c.display = display;
            }
            if let Some(analysis) = analysis {
                c.engine.analysis = analysis;
            }
            if let Some(language) = language {
                c.ui.language = language;
            }
        })?;
        if display_changed {
            self.display_settings_changed(&previous_display);
        }
        if analysis_changed {
            self.analysis_settings_changed(&previous_analysis);
        }
        Ok(true)
    }

    pub fn set_play_sound(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.ui.play_sound == enabled {
            return Ok(false);
        }
        self.commit_config(|c| c.ui.play_sound = enabled)?;
        Ok(true)
    }

    pub fn set_replace_file_hint(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.record.show_replace_file_hint == enabled {
            return Ok(false);
        }
        self.commit_config(|c| c.record.show_replace_file_hint = enabled)?;
        Ok(true)
    }

    /// Help > "清除所有个人数据": the personal data this product stores so far is the recent
    /// record list and 野狐棋谱's 近期搜索 (`fox-recent-searches`). (Java removed a mistyped
    /// `recent-files` key and left the record list in place; the list is actually cleared here.)
    pub fn clear_personal_data(&mut self) -> Result<bool, ApiError> {
        if self.config.recent_files.is_empty() && self.config.fox_kifu.recent.is_empty() {
            return Ok(false);
        }
        self.commit_config(|c| {
            c.recent_files.clear();
            c.fox_kifu.recent.clear();
        })?;
        Ok(true)
    }

    /// View > "重置界面位置" (`Config.deletePersist(true)` + `Lizzie.resetAllHints`): deletes the
    /// persist file, so window placement and last folder return to defaults at the next start, the
    /// panel sizes and 排列 at once (the Java layout), and turns the prompts a user switched off
    /// back on.
    pub fn reset_ui_position(&mut self) -> Result<(), ApiError> {
        match std::fs::remove_file(&self.storage.persist_file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(settings_error(WINDOW_STATE_RESET_FAILED, e)),
        }
        self.persist_reset = Some(Persisted::default());
        self.persisted.panel_sizes = crate::settings::PanelSizes::default();
        self.persisted.panel_arrangement = None;
        self.persisted.bottom_toolbar = crate::settings::BottomToolbar::default();
        self.persisted.independent_main_board = None;
        self.persisted.independent_sub_board = None;
        self.persisted.control_panel = None;
        self.set_replace_file_hint(true)?;
        Ok(())
    }

    /// `Config.persist` (on exit and whenever the 独立选点列表 closes): records the window
    /// placement, the 独立选点列表's placement under its column layout (7 or 9 `columns`; only
    /// when it was opened this session, the other layout's entry kept) and the main 选点列表's
    /// column widths (`None` while its score column is hidden: Java then writes an empty
    /// `main-window-list`), unless the UI position was reset.
    pub fn persist_ui(
        &mut self,
        window: Option<WindowPlacement>,
        suggestions_frame: Option<FramePlacement>,
        main_list_columns: Option<Vec<u32>>,
    ) -> Result<(), ApiError> {
        if self.persist_reset.is_some() {
            return Ok(());
        }
        if window.is_some() {
            self.persisted.window = window;
        }
        if let Some(frame) = suggestions_frame {
            if frame.columns.len() == 9 {
                self.persisted.suggestions_frame_9 = Some(frame);
            } else {
                self.persisted.suggestions_frame_7 = Some(frame);
            }
        }
        self.persisted.main_list_columns = main_list_columns;
        self.save_persisted()
    }

    /// `seq` of the last built view: a view with a greater `seq` is built after this call.
    pub fn view_seq(&self) -> u64 {
        self.seq
    }

    // ---------------------------------------------------------------- projection

    /// Builds the next view for the UI. `full` always includes the tree (initial load, reload);
    /// otherwise the tree is included only when the UI has not received it for this document and
    /// revision. One-shot parts (open and save outcomes, sound) are handed over here.
    pub fn view(&mut self, full: bool) -> WorkbenchView {
        self.refresh_commentary_view();
        self.seq += 1;
        let current = (self.doc.id, self.doc.revision);
        let tree = if full || self.tree_sent_for != Some(current) {
            self.tree_sent_for = Some(current);
            Some(tree_view(&self.doc))
        } else {
            None
        };
        let mut v = self.build_view(tree);
        v.open_outcome = self.outcome.take();
        v.save_outcome = self.save_outcome.take();
        v.sounds = std::mem::take(&mut self.sounds);
        v.engine_failures = std::mem::take(&mut self.engine_failures);
        for failure in &mut v.engine_failures {
            failure.repair = self.resource_repair_context(&failure.command);
        }
        v.engine_notice = self.engine_notice.take();
        v.flash_outcome = self.take_flash_outcome();
        v.rules_prompt = self.rules_prompt.take();
        v.resources = self.take_resources_view(full);
        v.tuning = self.take_tuning_view(full);
        (v.game_outcome, v.game_notices, v.byoyomi_voices) = self.take_play_outcome();
        (v.match_outcome, v.match_notices, v.match_winrate_image) = self.take_match_outcome();
        v.human_sl_notices = self.take_human_sl_notices();
        v.contribution_notices = self.take_contribution_notices();
        v.human_sl_continuation = self.take_human_sl_continuation();
        v.commentary.restore_question = self.take_commentary_restore();
        v.readboard.notices = self.take_readboard_notices();
        v
    }

    /// The current state with the tree, for observers other than the UI (acceptance queries):
    /// nothing is handed over, so what the UI receives does not depend on being observed.
    pub fn inspect(&self) -> WorkbenchView {
        let mut v = self.build_view(Some(tree_view(&self.doc)));
        v.open_outcome = self.outcome.clone();
        v.save_outcome = self.save_outcome.clone();
        v.sounds = self.sounds.clone();
        v.engine_failures = self.engine_failures.clone();
        for failure in &mut v.engine_failures {
            failure.repair = self.resource_repair_context(&failure.command);
        }
        v.engine_notice = self.engine_notice;
        v.flash_outcome = self.flash_outcome();
        v.rules_prompt = self.rules_prompt.clone();
        v.resources = Some(self.resources_view());
        v.tuning = Some(self.tuning_view());
        (v.game_outcome, v.game_notices, v.byoyomi_voices) = self.play_outcome_copy();
        (v.match_outcome, v.match_notices, v.match_winrate_image) = self.match_outcome_copy();
        v.human_sl_notices = self.human_sl_notices();
        v.contribution_notices = self.contribution_notices();
        v.human_sl_continuation = self.human_sl_continuation();
        v
    }

    /// Everything but the one-shot parts, which [`Self::view`] hands over and [`Self::inspect`]
    /// copies.
    fn build_view(&self, tree: Option<TreeView>) -> WorkbenchView {
        WorkbenchView {
            seq: self.seq,
            document: self.document_view(),
            tree,
            position: self.position_view(),
            pending_open: self.pending.as_ref().map(|p| PendingOpen {
                ticket: p.ticket,
                path: match &p.origin {
                    OpenOrigin::File { path, .. } | OpenOrigin::Recovery { path, .. } => Some(path.clone()),
                    OpenOrigin::Clipboard | OpenOrigin::Downloaded(_) | OpenOrigin::Online { .. } => None,
                },
            }),
            open_outcome: None,
            save_outcome: None,
            sounds: Vec::new(),
            settings: SettingsView {
                cloud: self.config.cloud.clone(),
                language: self.config.ui.language,
                active_language: self.active_language,
                restart_required: self.config.ui.language.resolve(self.system_locale.as_deref()) != self.active_language
                    || self.config.appearance.frame_font_size != self.active_frame_font_size,
                read_komi: self.config.record.read_komi,
                load_sgf_last: self.config.record.load_sgf_last,
                auto_quick_analyze_on_load: self.config.record.auto_quick_analyze_on_load,
                append_winrate_to_comment: self.config.record.append_winrate_to_comment,
                play_sound: self.config.ui.play_sound,
                replace_file_hint: self.config.record.show_replace_file_hint,
                extra_mode: self.config.ui.extra_mode,
                panels: self.config.panels.clone(),
                top_toolbar: self.config.top_toolbar,
                panel_sizes: self.persisted.panel_sizes,
                panel_arrangement: self.persisted.panel_arrangement.clone(),
                bottom_toolbar: self.persisted.bottom_toolbar,
                bottom_buttons: self.config.bottom_buttons,
                comment_panel_auto_hidden: self.config.ui.extra_mode.hides_comment(),
                custom_layouts: self.config.custom_layouts.clone(),
                independent_main_board: self.persisted.independent_main_board,
                independent_sub_board: self.persisted.independent_sub_board,
                engine: self.engine_settings_view(),
                logging: self.config.logging.clone(),
                quick_links: self.config.quick_links.clone(),
                edit: self.config.edit.clone(),
                display: self.config.display.clone(),
                analysis_engine: self.config.analysis_engine.clone(),
                quick_model: self.quick_model_view(),
                human_sl: self.human_sl_settings_view(),
                problems: self.config.problems.clone(),
                flash_range: self.config.flash_range,
                auto_analysis: self.config.auto_analysis.clone(),
                whole_game: self.config.whole_game,
                hawkeye: self.config.hawkeye.clone(),
                appearance: self.config.appearance.clone(),
                theme: self.appearance_theme.clone(),
                theme_files_revision: self.theme_files_revision,
                active_frame_font_size: self.active_frame_font_size,
                region: self.config.region,
                tracking: self.config.tracking,
                game: self.config.game.clone(),
                autoplay: crate::api::AutoPlayView {
                    enabled: self.play_autoplay_enabled(),
                    settings: self.persisted.autoplay,
                },
                matches: self.match_settings_view(),
                tsumego: self.config.tsumego,
                contribution: self.config.contribution.clone(),
            },
            recent_files: self.config.recent_files.iter().rev().cloned().collect(),
            komi: self.komi,
            engine: self.engine_view(),
            full_trace: self.diagnostics.full_trace(),
            fox: self.fox_view(),
            quick: self.quick_view(),
            flash: self.flash_view(),
            auto_analysis: self.auto_analysis_view(),
            batch: self.batch_view(),
            flash_outcome: None,
            region: self.region_view(),
            focus: self.focus_view(),
            whole_game: self.whole_game_view(),
            problems: self.problem_list(),
            analysis: self.current_analysis(),
            board: self.board_overlay(),
            raw_net: self.raw_net_view(),
            history: self.history(),
            theme_nodes: self.theme_node_colors(),
            statistics: self.statistics(),
            hawkeye: self.hawkeye_view(),
            kata_estimate: self.kata_estimate_view(),
            score: self.score_view(),
            secondary: self.secondary_view(),
            comment_with_analysis: self.comment_with_analysis(),
            engine_failures: Vec::new(),
            engine_notice: None,
            rules_prompt: None,
            java_import: self.java_import_view(),
            resources: None,
            tuning: None,
            editing: self.editing_view(),
            recovery: self.recovery_view(),
            readboard: self.readboard_view(),
            game: self.play_view(),
            game_outcome: None,
            game_notices: Vec::new(),
            engine_match: self.match_view(),
            pk_board: self.pk_board_view(),
            match_outcome: None,
            match_notices: Vec::new(),
            match_winrate_image: None,
            human_sl: self.human_sl_view(),
            strength: self.strength_view(),
            online: self.online_view(),
            commentary: self.commentary_view(),
            fox_kifu: self.fox_kifu_view(),
            tencent_kifu: self.tencent_kifu_view(),
            web_board: self.web_board_view(),
            update: self.update_view(),
            human_sl_notices: Vec::new(),
            contribution: self.contribution_view(),
            contribution_notices: Vec::new(),
            human_sl_continuation: None,
            byoyomi_voices: Vec::new(),
        }
    }

    /// Log location and sizes for Help > 诊断与日志.
    pub fn logs_status(&self) -> LogsStatus {
        let dir = &self.storage.logs_dir;
        let size = |name: &str| std::fs::metadata(dir.join(name)).ok().map(|m| m.len());
        LogsStatus {
            directory: dir.to_string_lossy().into_owned(),
            app_log_bytes: size("app.log"),
            crash_log_bytes: size("crash.log"),
        }
    }

    fn document_view(&self) -> DocumentView {
        let rec = &self.doc.record;
        DocumentView {
            id: self.doc.id,
            format: self.doc.format,
            path: self.doc.path.clone(),
            file_name: self
                .doc
                .path
                .as_deref()
                .and_then(|p| Path::new(p).file_name().map(|f| f.to_string_lossy().into_owned())),
            origin_name: self.doc.origin_name.clone(),
            size: rec.size,
            info: rec.info.clone(),
            revision: self.doc.revision,
            saved_revision: self.doc.saved_revision,
            unsaved: self.doc.unsaved(),
        }
    }

    fn position_view(&self) -> PositionView {
        let rec = &self.doc.record;
        let node = rec.node(self.cursor);
        let pos = rec.position(self.cursor);
        let last_action = last_action(node.kind);
        PositionView {
            document: self.doc.id,
            node: self.cursor,
            stones: pos.stones,
            last_action,
            snapshot: node.kind == NodeKind::Snapshot,
            move_number: node.move_number,
            to_play: node.to_play,
            black_captures: pos.black_captures,
            white_captures: pos.white_captures,
            marks: node.marks.clone(),
            comment: node.comment.clone(),
            user_comment: crate::commentary::codec::remove_blocks(&node.comment),
            ai_commentary: crate::commentary::codec::extract(&node.comment),
            path: rec.path_to(self.cursor),
            line_end: browse::navigate(rec, self.cursor, Navigation::Last),
            main_trunk: rec.is_main_trunk(self.cursor),
            next_moves: next_moves(rec, self.cursor),
            next_color: node.children.iter().find_map(|&c| match rec.node(c).kind {
                NodeKind::Move { color, .. } => Some(color),
                _ => None,
            }),
        }
    }
}

/// The stone or pass that leads to a node of `kind`.
fn last_action(kind: NodeKind) -> Option<LastAction> {
    match kind {
        NodeKind::Move { color, point } => Some(LastAction {
            color,
            point: Some(point),
        }),
        NodeKind::Pass { color } => Some(LastAction { color, point: None }),
        NodeKind::Snapshot | NodeKind::DummyPass => None,
    }
}

/// Points of `node`'s move children, first child first (Java `drawNextMoves`: passes and dummies
/// are not shown).
fn next_moves(rec: &GameRecord, node: NodeId) -> Vec<Point> {
    rec.node(node)
        .children
        .iter()
        .filter_map(|&c| match rec.node(c).kind {
            NodeKind::Move { point, .. } => Some(point),
            _ => None,
        })
        .collect()
}

/// `Utils.playVoiceFile(node)`: compares the captures at `node` with its parent's.
fn step_sound(rec: &GameRecord, node: NodeId) -> StoneSound {
    let Some(parent) = rec.parent(node) else {
        return StoneSound::Stone;
    };
    let (now, before) = (rec.position(node), rec.position(parent));
    let captured = if now.black_captures > before.black_captures {
        now.black_captures - before.black_captures
    } else {
        now.white_captures.saturating_sub(before.white_captures)
    };
    match captured {
        0 => StoneSound::Stone,
        1 | 2 => StoneSound::DeadStone,
        _ => StoneSound::DeadStoneMore,
    }
}

fn tree_view(doc: &Document) -> TreeView {
    let rec = &doc.record;
    let nodes = rec
        .nodes()
        .filter(|(_, n)| n.kind != NodeKind::DummyPass)
        .map(|(id, n)| TreeNodeView {
            id,
            parent: n.parent,
            children: n
                .children
                .iter()
                .copied()
                .filter(|&c| rec.node(c).kind != NodeKind::DummyPass)
                .collect(),
            kind: match n.kind {
                NodeKind::Snapshot if id == ROOT => TreeNodeKind::Root,
                NodeKind::Snapshot | NodeKind::DummyPass => TreeNodeKind::Snapshot,
                NodeKind::Move {
                    color: Stone::Black, ..
                }
                | NodeKind::Pass { color: Stone::Black } => TreeNodeKind::Black,
                NodeKind::Move { .. } | NodeKind::Pass { .. } => TreeNodeKind::White,
            },
            pass: matches!(n.kind, NodeKind::Pass { .. }),
            move_number: n.move_number,
            has_comment: !n.comment.is_empty(),
        })
        .collect();
    TreeView {
        document: doc.id,
        nodes,
    }
}
