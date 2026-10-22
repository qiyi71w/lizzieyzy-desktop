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
mod auto_analysis;
mod editing;
mod exporting;
mod flash;
mod importing;
mod information;
mod quick;
mod recovery;
mod resources;
mod tuning;
mod whole_game;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use analysis::AnalysisStore;
pub use analysis::LogLine;
pub use editing::{NOT_SQUARE, SETUP_ENGINE_RUNNING, SETUP_MODE_ROOT_ONLY, SETUP_ROOT_ONLY};
pub use flash::ANALYSIS_COMMAND_MISSING;
pub use importing::{IMPORT_NEEDS_REPLACE, IMPORT_STALE};
pub use recovery::RecoveryStep;
pub use resources::{prepare_resource_setup, run_resource_job, ResourceCompletion, ResourceJob};

use crate::api::{
    ApiError, DocumentView, EngineFailureView, EngineNotice, ErrorKind, ExportKind, LastAction, LogsStatus,
    OpenOutcome, OpenSource, PendingOpen, PositionView, RulesPromptView, SaveOutcome, SettingsView, StoneSound,
    TreeNodeKind, TreeNodeView, TreeView, WorkbenchView,
};
use crate::browse::{self, Navigation};
use crate::engine::runtime::{Engines, InstanceId, Launcher};
use crate::formats::{self, saved_analysis, ReadError};
use crate::record::{BoardSize, GameRecord, NodeId, NodeKind, RecordFormat, Stone, ROOT};
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
    /// A 存档与读档 slot (`loadFile(file, fromTemp = true)`): the record gets no file of its own and
    /// the recent list and last folder stay; `name` is the slot file shown in the title.
    Recovery {
        path: String,
        name: String,
        position: SlotPosition,
    },
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
    /// Set by "重置界面位置": the persist file stays deleted until the next start.
    persist_reset: bool,
    active_language: Language,
    system_locale: Option<String>,
    doc: Document,
    cursor: NodeId,
    komi: f64,
    next_doc: u64,
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
    /// The user chose this game's komi (`GameInfo.changedKomi`): engine profiles no longer set it.
    komi_changed: bool,
    /// A newly started foreground engine whose profile komi applies to a still empty game once
    /// it is ready (`Leelaz.firstLoad`).
    start_komi: Option<(InstanceId, f64)>,
    /// A limit stopped the analysis of this position (document, node).
    limit_stop: Option<(u64, NodeId)>,
    /// The limit notice was shown since the foreground engine started.
    limit_tip_shown: bool,
    /// A limit stopped the secondary engine's analysis of this position; only its own (Java
    /// `leelaz2.nameCmd` is not mirrored back to the primary engine).
    secondary_limit_stop: Option<(u64, NodeId)>,
    /// The limit notice was shown since the secondary engine was chosen.
    secondary_limit_tip_shown: bool,
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
    engine_failure: Option<EngineFailureView>,
    engine_notice: Option<EngineNotice>,
    logs: Vec<LogLine>,
    /// No settings file existed at start (Java `isNewProfile`): the Java import is offered.
    new_profile: bool,
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
    /// The problem move a 问题手 jump marked, while browsing the position before it
    /// (`clickbadmove`): document, that position, the move's node.
    problem_mark: Option<(u64, NodeId, NodeId)>,
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
        let new_profile = !storage.config_file.exists();
        let resources = resources::ResourceFlow::new(&storage);
        // The start-up rotation of a crashed session's periodic save happens here, before any
        // resume (see `recovery`).
        let recovery = crate::recovery::Store::open(&storage.save_dir);
        let config: Config = settings::load_json(&storage.config_file);
        let persisted: Persisted = settings::load_json(&storage.persist_file);
        let active_language = config.ui.language.resolve(system_locale);
        let mut engines = Engines::new(launcher, engine_roots);
        let runtime = storage.resources_dir.join("runtime/nvidia");
        if runtime.is_dir() {
            engines.set_library_dirs(vec![runtime]);
        }
        engines.set_interval(config.engine.analysis.interval_centisec);
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
            persist_reset: false,
            active_language,
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
            komi_changed: false,
            start_komi: None,
            limit_stop: None,
            limit_tip_shown: false,
            secondary_limit_stop: None,
            secondary_limit_tip_shown: false,
            secondary_fallback: None,
            session_rules: analysis::SessionRules::Unspecified,
            rules_revision: 0,
            rules_override: None,
            rules_prompted: None,
            pending_prompt: None,
            next_prompt: 0,
            rules_prompt: None,
            engine_failure: None,
            engine_notice: None,
            logs: Vec::new(),
            new_profile,
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
            problem_mark: None,
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
        Self::write_config(&self.storage, &self.config)
    }

    fn write_config(storage: &Storage, config: &Config) -> Result<(), ApiError> {
        settings::save_json(&storage.config_file, config)
            .map_err(|e| ApiError::new(ErrorKind::Io, format!("saving settings failed: {e}")))
    }

    /// A user settings change: applied only once it is on disk, so the projection never shows a
    /// state that a restart would lose.
    fn commit_config(&mut self, change: impl FnOnce(&mut Config)) -> Result<(), ApiError> {
        let mut next = self.config.clone();
        change(&mut next);
        Self::write_config(&self.storage, &next)?;
        self.config = next;
        Ok(())
    }

    fn save_persisted(&self) -> Result<(), ApiError> {
        if self.persist_reset {
            return Ok(());
        }
        settings::save_json(&self.storage.persist_file, &self.persisted)
            .map_err(|e| ApiError::new(ErrorKind::Io, format!("saving window state failed: {e}")))
    }

    // ---------------------------------------------------------------- opening

    /// Registers an open request. Any earlier request still reading is superseded.
    pub fn begin_open(&mut self, origin: OpenOrigin) -> u64 {
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        self.pending = Some(Pending { ticket, origin });
        ticket
    }

    /// Whether `ticket` is still the request the user is waiting for.
    pub fn is_current_open(&self, ticket: u64) -> bool {
        self.pending.as_ref().is_some_and(|p| p.ticket == ticket)
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
                    OpenOrigin::Clipboard => KIFU_LOAD_FAILED,
                };
                self.outcome = Some(OpenOutcome::Failed {
                    ticket,
                    message_key: message_key.to_string(),
                    detail: e.to_string(),
                });
            }
            Ok(read) => {
                self.auto_analysis_abandon();
                // Opening waits while a borrowed foreground engine is given back.
                let Some((_, Ok((format, mut record)))) = self.quick_defer_open((ticket, Ok(read))) else {
                    self.pending = Some(pending);
                    return true;
                };
                // Java applies a record komi when automatic loading is on or the game starts
                // from setup/handicap; otherwise the session komi stays (`resetAllNoKomi`).
                let use_record_komi =
                    record.komi_declared && (self.config.record.read_komi || record.has_setup_or_handicap);
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
                    _ if self.config.record.load_sgf_last => browse::navigate(&record, ROOT, Navigation::Last),
                    _ => ROOT,
                };
                let (path, origin_name) = match &pending.origin {
                    OpenOrigin::File { path, .. } => (Some(path.clone()), None),
                    OpenOrigin::Clipboard => (None, None),
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
                // Loading is silent (Java turns sound off while it builds the history).
                self.sounds.clear();
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
                // (`startNewKifuAnalysisContextAfterSuccessfulLoad`).
                self.komi_changed = false;
                self.paused = false;
                self.reset_editing();
                self.limit_stop = None;
                let declared = match format {
                    RecordFormat::Sgf => self.doc.record.info.rules.clone(),
                    RecordFormat::Gib => None,
                };
                self.publish_record_rules(declared.as_deref());
                self.sync_engine();
                self.quick_after_load();
            }
        }
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
        self.sounds = if self.config.ui.play_sound {
            to[shared..].iter().map(|&n| step_sound(rec, n)).collect()
        } else {
            Vec::new()
        };
        self.cursor = target;
        self.sync_engine();
        self.quick_navigated();
        true
    }

    pub fn navigate(&mut self, nav: Navigation) -> bool {
        let target = browse::navigate(&self.doc.record, self.cursor, nav);
        self.move_to(target)
    }

    pub fn goto_node(&mut self, node: NodeId) -> Result<bool, ApiError> {
        if !browse::is_browsable(&self.doc.record, node) {
            return Err(ApiError::new(
                ErrorKind::InvalidArgument,
                format!("no node {node} in this document"),
            ));
        }
        Ok(self.move_to(node))
    }

    /// A number of 0 is ignored like Java's empty/invalid move field.
    pub fn goto_move_number(&mut self, number: u32) -> bool {
        if number == 0 {
            return false;
        }
        let target = browse::goto_move_number(&self.doc.record, self.cursor, number);
        self.move_to(target)
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
            return Ok(false);
        }
        let previous = self.config.display.clone();
        let mut settings = settings;
        if settings.kata_estimate_save_status != previous.kata_estimate_save_status {
            settings.kata_estimate = self.remembered_estimate(settings.kata_estimate_save_status);
        }
        self.commit_config(|c| c.display = settings)?;
        self.engines.set_pv_visits(self.config.display.pv_visits != PvVisitsMode::Off);
        self.estimate_settings_changed(&previous);
        Ok(true)
    }

    pub fn set_language(&mut self, language: Language) -> Result<bool, ApiError> {
        if self.config.ui.language == language {
            return Ok(false);
        }
        self.commit_config(|c| c.ui.language = language)?;
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
    /// record list. (Java removed a mistyped `recent-files` key and left the list in place; the
    /// list is actually cleared here.)
    pub fn clear_personal_data(&mut self) -> Result<bool, ApiError> {
        if self.config.recent_files.is_empty() {
            return Ok(false);
        }
        self.commit_config(|c| c.recent_files.clear())?;
        Ok(true)
    }

    /// View > "重置界面位置" (`Config.deletePersist(true)` + `Lizzie.resetAllHints`): deletes the
    /// persist file, so window placement and last folder return to defaults at the next start, and
    /// turns the prompts a user switched off back on.
    pub fn reset_ui_position(&mut self) -> Result<(), ApiError> {
        match std::fs::remove_file(&self.storage.persist_file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(ApiError::new(
                    ErrorKind::Io,
                    format!("deleting window state failed: {e}"),
                ))
            }
        }
        self.persist_reset = true;
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
        if self.persist_reset {
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

    // ---------------------------------------------------------------- projection

    /// Builds the next view for the UI. `full` always includes the tree (initial load, reload);
    /// otherwise the tree is included only when the UI has not received it for this document and
    /// revision. One-shot parts (open and save outcomes, sound) are handed over here.
    pub fn view(&mut self, full: bool) -> WorkbenchView {
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
        v.engine_failure = self.engine_failure.take();
        if let Some(failure) = &mut v.engine_failure {
            failure.repair = self.resource_repair_context(&failure.command);
        }
        v.engine_notice = self.engine_notice.take();
        v.flash_outcome = self.take_flash_outcome();
        v.rules_prompt = self.rules_prompt.take();
        v.resources = self.take_resources_view(full);
        v.tuning = self.take_tuning_view(full);
        v
    }

    /// The current state with the tree, for observers other than the UI (acceptance queries):
    /// nothing is handed over, so what the UI receives does not depend on being observed.
    pub fn inspect(&self) -> WorkbenchView {
        let mut v = self.build_view(Some(tree_view(&self.doc)));
        v.open_outcome = self.outcome.clone();
        v.save_outcome = self.save_outcome.clone();
        v.sounds = self.sounds.clone();
        v.engine_failure = self.engine_failure.clone();
        if let Some(failure) = &mut v.engine_failure {
            failure.repair = self.resource_repair_context(&failure.command);
        }
        v.engine_notice = self.engine_notice;
        v.flash_outcome = self.flash_outcome();
        v.rules_prompt = self.rules_prompt.clone();
        v.resources = Some(self.resources_view());
        v.tuning = Some(self.tuning_view());
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
                    OpenOrigin::Clipboard => None,
                },
            }),
            open_outcome: None,
            save_outcome: None,
            sounds: Vec::new(),
            settings: SettingsView {
                language: self.config.ui.language,
                active_language: self.active_language,
                restart_required: self.config.ui.language.resolve(self.system_locale.as_deref())
                    != self.active_language,
                read_komi: self.config.record.read_komi,
                load_sgf_last: self.config.record.load_sgf_last,
                auto_quick_analyze_on_load: self.config.record.auto_quick_analyze_on_load,
                append_winrate_to_comment: self.config.record.append_winrate_to_comment,
                play_sound: self.config.ui.play_sound,
                replace_file_hint: self.config.record.show_replace_file_hint,
                extra_mode: self.config.ui.extra_mode,
                engine: self.engine_settings_view(),
                edit: self.config.edit.clone(),
                display: self.config.display.clone(),
                analysis_engine: self.config.analysis_engine.clone(),
                quick_model: self.quick_model_view(),
                problems: self.config.problems.clone(),
                flash_range: self.config.flash_range,
                auto_analysis: self.config.auto_analysis.clone(),
                whole_game: self.config.whole_game,
            },
            recent_files: self.config.recent_files.iter().rev().cloned().collect(),
            komi: self.komi,
            engine: self.engine_view(),
            quick: self.quick_view(),
            flash: self.flash_view(),
            auto_analysis: self.auto_analysis_view(),
            flash_outcome: None,
            whole_game: self.whole_game_view(),
            problems: self.problem_list(),
            analysis: self.current_analysis(),
            board: self.board_overlay(),
            raw_net: self.raw_net_view(),
            history: self.history(),
            statistics: self.statistics(),
            kata_estimate: self.kata_estimate_view(),
            score: self.score_view(),
            secondary: self.secondary_view(),
            comment_with_analysis: self.comment_with_analysis(),
            engine_failure: None,
            engine_notice: None,
            rules_prompt: None,
            java_import: self.java_import_view(),
            resources: None,
            tuning: None,
            editing: self.editing_view(),
            recovery: self.recovery_view(),
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
        let last_action = match node.kind {
            NodeKind::Move { color, point } => Some(LastAction {
                color,
                point: Some(point),
            }),
            NodeKind::Pass { color } => Some(LastAction { color, point: None }),
            NodeKind::Snapshot | NodeKind::DummyPass => None,
        };
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
            path: rec.path_to(self.cursor),
            line_end: browse::navigate(rec, self.cursor, Navigation::Last),
            main_trunk: rec.is_main_trunk(self.cursor),
            next_moves: node
                .children
                .iter()
                .filter_map(|&c| match rec.node(c).kind {
                    NodeKind::Move { point, .. } => Some(point),
                    _ => None,
                })
                .collect(),
            next_color: node.children.iter().find_map(|&c| match rec.node(c).kind {
                NodeKind::Move { color, .. } => Some(color),
                _ => None,
            }),
        }
    }
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
    let numbers = crate::display::tree_numbers(rec);
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
            number: numbers[id as usize],
            has_comment: !n.comment.is_empty(),
        })
        .collect();
    TreeView {
        document: doc.id,
        nodes,
    }
}
