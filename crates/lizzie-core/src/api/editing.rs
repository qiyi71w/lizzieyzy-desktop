//! Document editing commands and their projection: the Edit and Game menu entries, board clicks
//! and context menus, the edit toolbars, comment and markup editing, game information, new boards
//! and starting-position setup. Menus, shortcuts, mouse input and acceptance automation all send
//! these same operations; the application flow ([`crate::workbench`]) applies them through the
//! document's one write path.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::record::{MarkTool, NodeId, Point, Stone, StoneChange, Transform};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "op")]
pub enum EditCommand {
    /// Board click (`LizzieFrame.onClicked` → `Board.place`): a stone on the current position.
    /// `color: None` plays the side to play (交替落子) and follows the main continuation played at
    /// the same point; 添加黑子 / 添加白子 and their right-click send the colour.
    Play { point: Point, color: Option<Stone> },
    /// 停一手(P): a pass by the side to play (ignored in starting-position setup).
    Pass,
    /// 插入黑子 / 插入白子: a move after the current one; what followed is played again after it.
    Insert { point: Point, color: Stone },
    /// RightClickMenu2 反色 / 删除 / 移动 and dragging a stone: change the stone at `point` where it
    /// was placed; the following moves are played again.
    EditStone { point: Point, change: StoneChange },
    /// 删除一手(Delete): the current node and everything after it. Answers
    /// `NeedsEditConfirmation` when that removes following moves; resend with that target as
    /// `confirmed`. At the root it clears the board.
    DeleteMove { confirmed: Option<EditTarget> },
    /// Bottom toolbar 删除 (`deleteMoveNoHint`): like [`EditCommand::DeleteMove`] without asking.
    DeleteMoveNoHint,
    /// 删除分支(Shift+Delete): the branch holding the current node, from where it leaves its
    /// junction.
    DeleteBranch { confirmed: Option<EditTarget> },
    /// Shift+←/→: the current branch swaps with the previous (`up`) or next variation.
    MoveBranch { up: bool },
    /// 设为主分支(L).
    SetAsMain,
    /// 撤销删除 / 撤销编辑: back to the record before the last delete, insert or stone edit.
    UndoEdit,
    /// 恢复删除 / 恢复编辑.
    RedoEdit,
    /// 清除编辑缓存.
    ClearEditCache,
    /// 交换落子顺序: the other side plays next when the foreground engine can analyse either side
    /// (`canAddPlayer`); otherwise a pass.
    ChangeTurn,
    /// 让AI落子最佳一手(,): the best candidate of the current analysis.
    PlayBestMove,
    /// Middle click on a variation / 添加为分支 at a branch end: its moves from the current
    /// position, which moves to the end of them.
    PlayVariation { moves: Vec<Point> },
    /// RightClickMenu 添加为分支 (`LizzieFrame.addSuggestionAsBranch`): the variation becomes a new
    /// branch of the current node and browsing stays; at the end of a side branch it is played
    /// like [`EditCommand::PlayVariation`].
    AddVariationAsBranch { moves: Vec<Point> },
    /// 自动补完征子.
    ContinueLadder,
    /// Double click on a stone / 跳到此步 (`gotoAnyMoveByCoords`, `findMove`): the move that
    /// played the stone at `point`, on the current line; with `any_branch` also in other branches.
    FindMove { point: Point, any_branch: bool },
    /// Comment pane edit, committed for `node` (the node it was opened on).
    SetComment { node: NodeId, comment: String },
    /// Markup tool click on the current node.
    ToggleMark { point: Point, tool: MarkTool },
    /// 清除标记 of the current node.
    ClearMarks,
    /// 编辑棋局信息(I): player names and komi.
    SetGameInfo { black: String, white: String, komi: f64 },
    /// 新建 / 新建空白棋盘 / 清空棋盘 button (`newEmptyBoard`): asks first while the record has
    /// moves and 显示新建棋盘确认对话框 is on.
    NewBoard { confirmed: Option<EditTarget> },
    /// 清空棋盘(Ctrl+Home) (`Board.clear`): a new empty board without asking.
    ClearBoard,
    /// 设置棋盘大小(Ctrl+I): a new empty board of this size.
    SetBoardSize { width: u16, height: u16 },
    /// 交换黑白 / 旋转 / 翻转.
    Transform { transform: Transform },
    /// 起始局面设置模式 on or off. Turning it on with moves in the record asks first (the position
    /// becomes the starting position, moves are discarded).
    SetSetupMode {
        enabled: bool,
        confirmed: Option<EditTarget>,
    },
    /// Setup mode board click: `Some` places or replaces a stone, `None` erases it.
    SetupStone { point: Point, stone: Option<Stone> },
    /// 清空全部设置子.
    SetupClearAll,
    /// 黑先 / 白先.
    SetupSideToPlay { color: Stone },
    /// 将当前盘面设为起始局面: asks first when the record has moves.
    ConvertToStartingPosition { confirmed: Option<EditTarget> },
    /// 试下 / 恢复 (`LizzieFrame.tryPlay`): starting keeps the record and drops what follows the
    /// current node; ending brings the kept record and browsing position back.
    TryPlay,
    /// 死活题 确定 (`TsumeGoFrame`): the choices are kept and the current position becomes a
    /// framed problem, a new game whose root holds the problem, the attacker's wall, both
    /// territories and the ko threats, with the chosen side to play and the frame's komi (as a
    /// komi the user set). A position with stones out to every edge stays as it is.
    FrameTsumego {
        settings: crate::settings::TsumegoFrameSettings,
    },
    /// The result of 抓取死活题 (`CaptureTsumeGo.processStoneData`): a new board, square of the
    /// captured width when the current one is smaller than the capture, with the captured stones
    /// in the reported corner, Black to play.
    LoadCapturedProblem {
        problem: crate::tsumego::capture::CapturedProblem,
    },
}

/// What a confirmation was asked for: the document, its revision and the current node then. A
/// confirmation applies only while all three are unchanged, so an answer cannot reach another
/// record, a later version or another position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct EditTarget {
    pub document: u64,
    pub revision: u64,
    pub node: NodeId,
}

/// Editing state of the application flow.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct EditingView {
    /// 起始局面设置模式 is on.
    pub setup_mode: bool,
    /// 撤销删除 has a record to go back to.
    pub can_undo: bool,
    /// 恢复删除 has a record to go forward to.
    pub can_redo: bool,
    /// 试下 is on.
    pub trying: bool,
}
