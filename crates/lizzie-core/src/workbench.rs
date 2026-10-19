//! Application flow for browsing (应用流程): the open document, the browsing position, open
//! requests in flight, committed settings and the application data they produce.
//!
//! This is the only writer of that state. Platform adapters run file reading off the UI thread
//! and hand the result back with the ticket they were given; a result whose ticket is no longer
//! the latest request is dropped, so a slow earlier open can never replace a later one, and a
//! failed or abandoned read leaves the current document untouched.

use std::path::Path;
use std::sync::Arc;

use crate::api::{
    ApiError, DocumentView, ErrorKind, LastAction, OpenOutcome, OpenSource, PendingOpen, PositionView, SettingsView,
    TreeNodeKind, TreeNodeView, TreeView, WorkbenchView,
};
use crate::browse::{self, Navigation};
use crate::formats::{self, ReadError};
use crate::record::{BoardSize, GameRecord, NodeId, NodeKind, RecordFormat, Stone, ROOT};
use crate::settings::{self, Config, Language, Persisted, Storage, WindowPlacement};

/// Java `GameInfo.DEFAULT_KOMI`.
pub const DEFAULT_KOMI: f64 = 7.5;

/// Java `LizzieFrame.prompt.failedToOpenFile`: file opens of either format.
pub const FAILED_TO_OPEN_FILE: &str = "LizzieFrame.prompt.failedToOpenFile";
/// Java `KifuLoad.failed`: clipboard loads.
pub const KIFU_LOAD_FAILED: &str = "KifuLoad.failed";

#[derive(Clone, Debug)]
pub struct Document {
    pub id: u64,
    pub record: Arc<GameRecord>,
    pub format: Option<RecordFormat>,
    pub path: Option<String>,
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
    seq: u64,
    tree_sent_for: Option<u64>,
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
            },
            cursor: ROOT,
            komi: DEFAULT_KOMI,
            next_doc: 2,
            next_ticket: 1,
            pending: None,
            outcome: None,
            seq: 0,
            tree_sent_for: None,
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
        settings::save_json(&self.storage.config_file, &self.config)
            .map_err(|e| ApiError::new(ErrorKind::Io, format!("saving settings failed: {e}")))
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
                self.doc = Document {
                    id,
                    record,
                    format: Some(format),
                    path,
                };
                self.outcome = Some(OpenOutcome::Adopted { ticket, document: id });
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

    // ---------------------------------------------------------------- browsing

    fn move_to(&mut self, target: NodeId) -> bool {
        if target == self.cursor {
            return false;
        }
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
        self.config.record.read_komi = enabled;
        self.save_config()?;
        Ok(true)
    }

    pub fn set_language(&mut self, language: Language) -> Result<bool, ApiError> {
        if self.config.ui.language == language {
            return Ok(false);
        }
        self.config.ui.language = language;
        self.save_config()?;
        Ok(true)
    }

    /// Help > "清除所有个人数据": the personal data this product stores so far is the recent
    /// record list. (Java removed a mistyped `recent-files` key and left the list in place; the
    /// list is actually cleared here.)
    pub fn clear_personal_data(&mut self) -> Result<bool, ApiError> {
        if self.config.recent_files.is_empty() {
            return Ok(false);
        }
        self.config.recent_files.clear();
        self.save_config()?;
        Ok(true)
    }

    /// View > "重置界面位置" (`Config.deletePersist(true)`): deletes the persist file; window
    /// placement and last folder return to defaults at the next start.
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

    /// Builds the next view. `full` always includes the tree (initial load, automation query);
    /// otherwise the tree is included only for a document the UI has not received yet.
    pub fn view(&mut self, full: bool) -> WorkbenchView {
        self.seq += 1;
        let tree = if full || self.tree_sent_for != Some(self.doc.id) {
            self.tree_sent_for = Some(self.doc.id);
            Some(tree_view(&self.doc))
        } else {
            None
        };
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
            open_outcome: self.outcome.take(),
            settings: SettingsView {
                language: self.config.ui.language,
                active_language: self.active_language,
                restart_required: self.config.ui.language.resolve(self.system_locale.as_deref())
                    != self.active_language,
                read_komi: self.config.record.read_komi,
            },
            recent_files: self.config.recent_files.iter().rev().cloned().collect(),
            komi: self.komi,
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
