//! Application flow for browsing and saving (应用流程): the open document, the browsing
//! position, open and save requests in flight, committed settings and the application data they
//! produce.
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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::api::{
    ApiError, DocumentView, ErrorKind, LastAction, LogsStatus, OpenOutcome, OpenSource, PendingOpen, PositionView,
    SaveOutcome, SettingsView, StoneSound, TreeNodeKind, TreeNodeView, TreeView, WorkbenchView,
};
use crate::browse::{self, Navigation};
use crate::formats::{self, sgf_write, ReadError};
use crate::record::{BoardSize, DocumentEdit, GameRecord, NodeId, NodeKind, RecordFormat, Stone, ROOT};
use crate::settings::{self, Config, Language, Persisted, Storage, WindowPlacement};

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
    pub text: String,
}

#[derive(Clone, Debug)]
struct PendingSave {
    ticket: u64,
    document: u64,
    revision: u64,
    path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenOrigin {
    File { path: String, source: OpenSource },
    Clipboard,
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
}

impl Workbench {
    /// Loads committed settings and data; starts on an empty 19x19 record, Black to play,
    /// komi 7.5 (Java startup `new Board()`).
    pub fn new(storage: Storage, system_locale: Option<&str>) -> Workbench {
        let config: Config = settings::load_json(&storage.config_file);
        let persisted: Persisted = settings::load_json(&storage.persist_file);
        let active_language = config.ui.language.resolve(system_locale);
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
                    OpenOrigin::File { .. } => FAILED_TO_OPEN_FILE,
                    OpenOrigin::Clipboard => KIFU_LOAD_FAILED,
                };
                self.outcome = Some(OpenOutcome::Failed {
                    ticket,
                    message_key: message_key.to_string(),
                    detail: e.to_string(),
                });
            }
            Ok((format, mut record)) => {
                // Java applies a record komi when automatic loading is on or the game starts
                // from setup/handicap; otherwise the session komi stays (`resetAllNoKomi`).
                let use_record_komi =
                    record.komi_declared && (self.config.record.read_komi || record.has_setup_or_handicap);
                if use_record_komi {
                    self.komi = record.info.komi;
                } else {
                    record.info.komi = self.komi;
                }
                let id = self.next_doc;
                self.next_doc += 1;
                let record = Arc::new(record);
                self.cursor = if self.config.record.load_sgf_last {
                    browse::navigate(&record, ROOT, Navigation::Last)
                } else {
                    ROOT
                };
                let path = match &pending.origin {
                    OpenOrigin::File { path, .. } => Some(path.clone()),
                    OpenOrigin::Clipboard => None,
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
                // The content is the file it came from; pasted text was never on disk.
                let saved_revision = path.as_ref().map(|_| 0);
                self.doc = Document {
                    id,
                    record,
                    format: Some(format),
                    path,
                    revision: 0,
                    saved_revision,
                };
                self.outcome = Some(OpenOutcome::Adopted { ticket, document: id });
                // Loading is silent (Java turns sound off while it builds the history).
                self.sounds.clear();
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
        let has_content = rec.node_count() > 1 || !rec.node(ROOT).setup.is_empty();
        if has_content && !confirmed {
            PasteDecision::Confirm
        } else {
            PasteDecision::Proceed
        }
    }

    /// Reads clipboard text synchronously into the same commit path as files.
    pub fn read_clipboard_record(text: &str) -> Result<(RecordFormat, GameRecord), ReadError> {
        formats::parse(RecordFormat::Sgf, text).map(|r| (RecordFormat::Sgf, r))
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
        let ticket = self.next_save;
        self.next_save += 1;
        self.saves.push(PendingSave {
            ticket,
            document: self.doc.id,
            revision: self.doc.revision,
            path: path.to_string(),
        });
        SaveJob {
            ticket,
            path: PathBuf::from(&path),
            text: sgf_write::write(&self.doc.record, application),
        }
    }

    /// Hands back a finished write. A success confirms the captured revision; while that document
    /// is still open it now belongs to the written file, and the chooser starts in its folder next
    /// time (`SgfSaveCoordinator` success callback). Returns `false` for an unknown ticket.
    pub fn complete_save(&mut self, ticket: u64, result: Result<(), String>) -> bool {
        let Some(at) = self.saves.iter().position(|s| s.ticket == ticket) else {
            return false;
        };
        let save = self.saves.remove(at);
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
                if self.doc.id == save.document {
                    self.doc.path = Some(save.path.clone());
                    self.doc.format = Some(RecordFormat::Sgf);
                    self.doc.saved_revision = Some(save.revision);
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

    /// The one content-write entry of the open document: applies `edit` and increases the
    /// revision when the content changed. Editing slices route their operations through here.
    pub fn edit_document(&mut self, edit: DocumentEdit) -> Result<bool, ApiError> {
        let record = Arc::make_mut(&mut self.doc.record);
        let changed = record
            .apply(&edit)
            .map_err(|e| ApiError::new(ErrorKind::InvalidArgument, e))?;
        if changed {
            self.doc.revision += 1;
        }
        Ok(changed)
    }

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

    pub fn set_read_komi(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.record.read_komi == enabled {
            return Ok(false);
        }
        self.commit_config(|c| c.record.read_komi = enabled)?;
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

    /// Normal exit: records the window placement unless the UI position was reset.
    pub fn persist_on_exit(&mut self, window: Option<WindowPlacement>) -> Result<(), ApiError> {
        if self.persist_reset {
            return Ok(());
        }
        if window.is_some() {
            self.persisted.window = window;
        }
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
        let outcome = self.outcome.take();
        let save_outcome = self.save_outcome.take();
        let sounds = std::mem::take(&mut self.sounds);
        self.build_view(tree, outcome, save_outcome, sounds)
    }

    /// The current state with the tree, for observers other than the UI (acceptance queries):
    /// nothing is handed over, so what the UI receives does not depend on being observed.
    pub fn inspect(&self) -> WorkbenchView {
        self.build_view(
            Some(tree_view(&self.doc)),
            self.outcome.clone(),
            self.save_outcome.clone(),
            self.sounds.clone(),
        )
    }

    fn build_view(
        &self,
        tree: Option<TreeView>,
        outcome: Option<OpenOutcome>,
        save_outcome: Option<SaveOutcome>,
        sounds: Vec<StoneSound>,
    ) -> WorkbenchView {
        WorkbenchView {
            seq: self.seq,
            document: self.document_view(),
            tree,
            position: self.position_view(),
            pending_open: self.pending.as_ref().map(|p| PendingOpen {
                ticket: p.ticket,
                path: match &p.origin {
                    OpenOrigin::File { path, .. } => Some(path.clone()),
                    OpenOrigin::Clipboard => None,
                },
            }),
            open_outcome: outcome,
            save_outcome,
            sounds,
            settings: SettingsView {
                language: self.config.ui.language,
                active_language: self.active_language,
                restart_required: self.config.ui.language.resolve(self.system_locale.as_deref())
                    != self.active_language,
                read_komi: self.config.record.read_komi,
                play_sound: self.config.ui.play_sound,
                replace_file_hint: self.config.record.show_replace_file_hint,
            },
            recent_files: self.config.recent_files.iter().rev().cloned().collect(),
            komi: self.komi,
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
