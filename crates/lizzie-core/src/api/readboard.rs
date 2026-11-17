use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::{LastAction, NodeAnalysisView};
use crate::display::BoardOverlayView;
use crate::readboard::logging::{LoggingStatus, Persistence, Presentation};
use crate::record::{NodeId, Point, Stone};

/// 棋盘同步工具 (Windows ReadBoard helper) and its float board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum ReadBoardCommand {
    /// 同步 > 棋盘同步工具 (Alt+O, `LizzieFrame.openBoardSync`): starts the helper; a running one
    /// is closed first and started again.
    Open,
    /// 棋盘同步工具选项 > `Menu.alwaysKeepBoardStatSync`.
    SetAlwaysSyncBoardStat { enabled: bool },
    /// 棋盘同步工具选项 > `Menu.readBoardGetFocus`.
    SetGetFocus { enabled: bool },
    /// 设置 > `Menu.notPlaySoundInSync`.
    SetNoSoundInSync { enabled: bool },
    /// The answer to `ReadBoard.updateInstallConfirmMessage` (the helper offered an update it
    /// downloaded): `true` installs it and starts the helper again.
    AnswerUpdate { accept: bool },
    /// The answer to `ReadBoard.websocketPonderingNotice` shown for
    /// [`ReadBoardView::pondering_notice`] `generation`.
    AnswerPonderingNotice { generation: u64, decision: PonderingNoticeDecision },
    /// Float board: the wheel changed the preview depth over a candidate (`FloatBoard.doBranch`
    /// → `sendLossFocus`); the target window gets the focus back when `Menu.readBoardGetFocus`
    /// is on.
    ReturnFocus,
    /// Float board: edit mode (V, the edit button). Leaving it shows the main line's end.
    SetFloatEditMode { enabled: bool },
    /// Float board: the +/- button (F). Showing the suggestions again shows the main line's end.
    SetFloatSuggestionsHidden { hidden: bool },
    /// Float board: the window hidden for a `place` ([`ReadBoardView::float_hide_request`]) is
    /// off the screen; the `place` waiting for it is written (Java hides it on the EDT before
    /// writing). A number that is not the current request changes nothing.
    FloatBoardHidden { request: u64 },
    /// 诊断与日志 > ReadBoard: the helper's diagnostics, capture and trace writers
    /// (`readboardLoggingSet`; the UI asks before capture, `DiagnosticsDialog.captureConfirmMessage`).
    SetHelperLogging { diagnostics: bool, capture: bool, trace: bool },
}

/// What the client shows of the helper.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ReadBoardView {
    /// `readboard.exe` was found next to the program (the menu entry works; Windows only).
    pub available: bool,
    /// A helper process runs.
    pub running: bool,
    /// The helper syncs a board (`sync` … `endsync`/`stopsync`).
    pub syncing: bool,
    /// Moves played here are clicked on the target window (`bothSync`).
    pub both_sync: bool,
    /// GMA auto-play is armed for this colour (the helper's `play>…gma`).
    pub gma: Option<Stone>,
    /// The float board over the target window, while shown.
    pub float_board: Option<FloatBoardView>,
    /// The float board was hidden because a `place` is about to be sent (`foreFoxWithInBoard`)
    /// and that `place` waits until the window is off the screen: once it is, answer
    /// `FloatBoardHidden` with this number. Without the answer the `place` is never written; the
    /// local move fails at its confirmation timeout.
    pub float_hide_request: Option<u64>,
    /// A helper update waits for the user's answer (`ReadBoard.updateInstallConfirmMessage`
    /// with this version tag).
    pub update_offer: Option<String>,
    /// GMA waits for the answer to `ReadBoard.websocketPonderingNotice` (the helper asked for
    /// background pondering, which the custom WebSocket engine cannot do); answer
    /// `AnswerPonderingNotice` with this number.
    pub pondering_notice: Option<u64>,
    /// The helper's log writers (诊断与日志 > ReadBoard), while a helper runs.
    pub logging: Option<HelperLoggingView>,
    /// 棋盘同步工具选项 > `Menu.alwaysKeepBoardStatSync` (Java `ui.always-sync-boardstat`).
    pub always_sync_board_stat: bool,
    /// 棋盘同步工具选项 > `Menu.readBoardGetFocus` (Java `ui.read-board-get-focus`).
    pub get_focus: bool,
    /// 设置 > `Menu.notPlaySoundInSync` (Java `ui.not-play-sound-insync`).
    pub no_sound_in_sync: bool,
    /// Messages to show once each, in order.
    pub notices: Vec<ReadBoardNotice>,
}

/// `LizzieFrame.ReadBoardWebSocketPonderingDecision`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PonderingNoticeDecision {
    /// 确定: GMA goes on without pondering.
    Confirm,
    /// 不再提示: as `Confirm`, and the notice is never shown again (saved).
    Suppress,
    /// The dialog was closed: GMA keeps waiting until it is armed again.
    Dismiss,
}

/// The float board window (`FloatBoard`): screen rectangle in physical pixels as the helper
/// reported it (scaled by the helper's DPI factor when it sent one).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct FloatBoardView {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// `0` Fox, `1` Tygem, `2` Sina, other values a generic client.
    pub board_type: i32,
    /// The DPI factor of the helper's scaled form, if it sent one.
    pub scale: Option<f32>,
    pub edit_mode: bool,
    pub suggestions_hidden: bool,
    /// The edit button is shown (not while both boards are synced).
    pub edit_button: bool,
    /// A click places a move (both boards synced, or edit mode; `FloatBoard.onClicked`).
    pub clicks_play: bool,
    /// What the overlay draws outside the edit mode: the main line's end
    /// (`getCurOrMainEnd(false)`), whatever node the main window shows. Absent in the edit mode,
    /// which draws the current position like the main board.
    pub scene: Option<FloatSceneView>,
}

/// The main line's end as the float board draws it (`FloatBoardRenderer.prepareSelection`):
/// stones, the last move, candidates, marks and Kata形势 of that node.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct FloatSceneView {
    pub node: NodeId,
    /// Row-major, `size.width * size.height`.
    pub stones: Vec<Option<Stone>>,
    pub last_action: Option<LastAction>,
    pub to_play: Stone,
    pub snapshot: bool,
    /// Points of the move children (variations behind an end placeholder), first child first.
    pub next_moves: Vec<Point>,
    /// Stored analysis of that node, as the main board's `analysis`.
    pub analysis: Option<NodeAnalysisView>,
    /// Move numbers and 落子评价 of that node, as the main board's `board`.
    pub board: BoardOverlayView,
    /// Kata形势 of that node, else of its parent (`preEstimateArray`); absent while Kata形势 is
    /// off or nothing was reported.
    pub ownership: Option<Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum ReadBoardNotice {
    /// A Java resource key, with its `%s` arguments in order.
    Message { key: String, args: Vec<String> },
    /// `该模式仅支持 KataGo` (a literal in Java, not a resource).
    GmaUnsupported,
}

/// 诊断与日志 > ReadBoard (`ReadBoardLoggingSnapshot`): only what the helper confirmed counts as
/// on; a missing answer is unknown, never success.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HelperLoggingView {
    pub status: LoggingStatus,
    /// The helper process's log session (`DiagnosticsDialog.helperCapability`), empty while unknown.
    pub process_session_id: String,
    pub persistence: Option<Persistence>,
    pub drop_count: Option<u32>,
    pub desired_diagnostics: bool,
    pub desired_capture: bool,
    pub desired_trace: bool,
    pub diagnostics: Presentation,
    pub capture: Presentation,
    pub trace: Presentation,
    /// `DiagnosticsDialog.captureSummary`.
    pub capture_summary: String,
}
