//! 超级鹰眼 (`MoveListFrame`): the projection its window draws while it is open, and its commands.
//!
//! Every number comes from the per-move evaluation ([`super::MoveEvaluation`], Java `NodeInfo`)
//! of the selected line. A move counts toward an aggregate only when Java would count it: the
//! position before analysed (`analyzedMatchValue`) for the match numbers, both positions analysed
//! (`analyzed`) for the losses. Counts and sums are sent as they are; the window divides them, so a
//! side without analysed moves shows Java's empty bar / `NaN`, never a made-up 0.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{HistoryPoint, MoveEvaluation};
use crate::record::{NodeId, Point};

/// The window's open state, filters and navigation.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum HawkeyeCommand {
    /// Y, 视图 > 面板 > 超级鹰眼, 分析 > 超级鹰眼, the toolbar and bottom bar buttons, the settings
    /// dialog's box (`LizzieFrame.toggleBadMoves`): open or close the window (and in 双引擎模式 the
    /// secondary engine's beside it). Closing clears the marked move. Remembered
    /// (`show-badmoves-frame`).
    Toggle,
    /// The 黑 / 白 boxes of the filter bar (session only, both on when the window opens).
    SetSides { black: bool, white: bool },
    /// A list row, or a 翻车现场 bar: the position before the move at `node` (on the main trunk
    /// when 分支 is 主干), with the move marked (`clickbadmove`). Java ignores row clicks while
    /// auto analysis runs (`isAutoAna`); that guard belongs to the auto-analysis owner.
    GotoMove { node: NodeId },
    /// Clicking or dragging on a curve chart (`goToMoveNumberBeyondBranch`): move `number` of the
    /// current line, else of the main trunk when the line is shorter; 0 (Java's -1 left of the
    /// chart) is the root.
    GotoMoveNumber { number: u32 },
}

/// One side's match and loss numbers over the 统计 range (`keyPanel`, 胜率吻合 / 吻合度走势 lines).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct HawkeyeSide {
    /// Moves whose position before is analysed (`matchAnalyzedCount`): the match denominators.
    pub match_analyzed: u32,
    /// Of those, AI matches (`isMatchAi`).
    pub matched: u32,
    /// Of those, the first candidate (`isBest`).
    pub best: u32,
    /// Sum of match shares (`percentsMatch`).
    pub share_sum: f64,
    /// Moves with both positions analysed (`analyzedCount`): the loss denominators, the
    /// score one too (Java counts a move without a score lead as changing it by 0).
    pub analyzed: u32,
    /// Sum of win-rate losses, percentage points (best moves lose nothing).
    pub winrate_loss_sum: f64,
    /// Sum of score losses, points.
    pub score_loss_sum: f64,
}

/// Losses at or above one 损失分布 threshold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct HawkeyeBand {
    pub count: u32,
    pub sum: f64,
    /// Sum of squared losses (Java's "方差" is this over the count).
    pub squares: f64,
}

/// One side's 胜率损失分布 / 目数损失分布 summary line: every analysed move counts, gains add nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct HawkeyeSpread {
    pub analyzed: u32,
    pub sum: f64,
    pub squares: f64,
    /// At least the first (`winrate-diff-range1` / `score-diff-range1`) and the second threshold.
    pub above1: HawkeyeBand,
    pub above2: HawkeyeBand,
}

/// One side's numbers in one stage (布局 / 中盘 / 收官) of the whole line (分段吻合度).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct HawkeyeStage {
    pub match_analyzed: u32,
    pub share_sum: f64,
    pub analyzed: u32,
    pub winrate_loss_sum: f64,
    pub score_loss_sum: f64,
}

/// A 翻车现场 entry: one of a side's five largest losses.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, TS)]
pub struct HawkeyeMistake {
    /// Index into [`HawkeyePane::moves`].
    pub index: u32,
    /// The loss (negative; 0 for a best move).
    pub change: f64,
}

/// One engine's window (Java `MoveListFrame(1)` / `MoveListFrame(2)`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HawkeyePane {
    /// KataGo / SAI columns and score pages (`isKatago`: the engine is KataGo or the record holds
    /// its score data).
    pub katago: bool,
    /// Move number at the end of the selected line (总手数).
    pub total_moves: u32,
    /// The analysed positions of the selected line, root first (the curves).
    pub points: Vec<HistoryPoint>,
    /// Move number of the selected line's first move off the main trunk; the score line is
    /// dashed from there (`!node.isMainTrunk()`).
    pub off_trunk_from: Option<u32>,
    /// Every move of the selected line whose position before is analysed, root first.
    pub moves: Vec<MoveEvaluation>,
    /// Running AI score of Black and White before each of `moves` and after the last one:
    /// share sum and count of the moves so far (吻合度走势, `getMatchValue`). `moves.len() + 1`
    /// entries, index i holding the moves before `moves[i]`.
    pub running: Vec<[HawkeyeRunning; 2]>,
    /// The 简易列表 rows (indexes into `moves`, root first): both positions analysed, over the
    /// 列表筛选 thresholds; each table takes its side's. The window sorts them.
    pub rows: Vec<u32>,
    /// The 详细列表 rows: `rows` of the sides whose 黑 / 白 box is checked.
    pub detail_rows: Vec<u32>,
    /// 置顶当前手: the current move's row, shown first whatever the order.
    pub pinned: Option<u32>,
    /// Black and White over the 统计 range.
    pub sides: [HawkeyeSide; 2],
    /// 损失分档 counts of Black and White over the 统计 range, best first: win rate and score.
    pub winrate_buckets: [[u32; 6]; 2],
    pub score_buckets: [[u32; 6]; 2],
    /// 胜率损失分布 / 目数损失分布 summaries over the 统计 range.
    pub winrate_spread: [HawkeyeSpread; 2],
    pub score_spread: [HawkeyeSpread; 2],
    /// 分段吻合度: Black and White in 布局, 中盘, 收官 of the whole line.
    pub stages: [[HawkeyeStage; 3]; 2],
    /// 胜率损失统计 / 目数损失统计: Black and White moves per stage under the first threshold,
    /// between the thresholds and over the second.
    pub winrate_misses: [[[u32; 3]; 3]; 2],
    pub score_misses: [[[u32; 3]; 3]; 2],
    /// 翻车现场(胜率) / (目数): up to five per side, in move order.
    pub winrate_mistakes: Vec<HawkeyeMistake>,
    pub score_mistakes: Vec<HawkeyeMistake>,
}

/// Share sum and count of one side's moves so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct HawkeyeRunning {
    pub share_sum: f64,
    pub count: u32,
}

/// 超级鹰眼 while it is open.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HawkeyeView {
    /// The 黑 / 白 boxes.
    pub black: bool,
    pub white: bool,
    /// The 统计 range in effect (`matchAiFirstMove`, `matchAiLastMove`): moves after `first_move`
    /// up to `last_move`; 当前 follows the browsing position.
    pub first_move: i32,
    pub last_move: i32,
    /// Move number of the browsing position.
    pub current_move: u32,
    /// The marked move (`clickbadmove`) while the position before it is browsed.
    pub mark: Option<Point>,
    pub primary: HawkeyePane,
    /// 双引擎模式's secondary engine window.
    pub secondary: Option<HawkeyePane>,
}
