//! The cross-language contract: commands, results, errors and the workbench projection.
//!
//! These Rust types are the single authority. `ui/src/contract/generated.ts` is generated from
//! them (see `tests/contract.rs`); the UI, its shortcuts and the acceptance channel all send the
//! same [`Command`] values and read the same [`WorkbenchView`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::browse::Navigation;
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
}

/// Help > 诊断与日志: where the logs are and their sizes (`None` when a log does not exist).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct LogsStatus {
    pub directory: String,
    pub app_log_bytes: Option<u64>,
    pub crash_log_bytes: Option<u64>,
}
