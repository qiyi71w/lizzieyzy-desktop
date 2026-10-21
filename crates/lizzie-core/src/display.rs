//! Board display projection (棋盘显示): what the main board draws on and next to the stones of
//! the current position, from the record, the stored analysis and the display settings.
//!
//! Ported from the fixed Java (`wimi321/lizzieyzy-next` `cb53acc`):
//! - move numbers: `BoardData.moveNumberList` as `BoardHistoryList.place` / `pass` and
//!   `BoardHistoryNode.addOrGoto` build it, the SGF `MN` renumbering
//!   (`BoardHistoryList.moveNumber`) and `BoardRenderer.drawMoveNumbers`;
//! - 落子评价: `BoardRenderer.drawMoveRankMark`, `Board.lastWinrateDiff` /
//!   `lastScoreMeanDiff` / `isMatchAi` / `updateIsBest` and `MoveRankDefinition`;
//! - 下一手 详细: `BoardRenderer.drawNextMoves` and `notEnoughSuggestionAt`.
//!
//! Two Java defects are corrected (both only change numbers Java shows inconsistently):
//! - Java's `pass` leaves `moveMNNumber` at -1, so after a pass inside a branch numbered from 1
//!   (or after an `MN` renumbering) the following moves show their total move number again; here
//!   a pass counts like a move, so the branch keeps counting.
//! - Java gives `moveMNNumber` 1 to any node directly under the root, a setup node included, so
//!   a record whose first node after the root holds setup stones numbered its first move 2; here
//!   only a move or pass starts the count, since setup stones take no move number.
//!
//! A setup node that removes a stone also removes its number (Java kept it on the empty point).

use serde::Serialize;
use ts_rs::TS;

use crate::api::NodeAnalysisView;
use crate::record::{GameRecord, NodeId, NodeKind, Point, Position, Stone};
use crate::settings::{DisplaySettings, MoveRankMode};
use crate::viewpoint::{ShownValues, Viewpoint};

/// Java `moveNumberList` at the end of a path: the number each point shows (0: none), and
/// `moveMNNumber` of every node on the path (`None` for Java's -1).
#[derive(Clone, Debug, PartialEq)]
pub struct Numbering {
    pub list: Vec<u32>,
    pub counts: Vec<Option<u32>>,
}

fn sgf_mn(rec: &GameRecord, id: NodeId) -> Option<u32> {
    rec.node(id)
        .properties
        .iter()
        .find(|p| p.id == "MN")
        .and_then(|p| p.values.first())
        .and_then(|v| v.trim().parse().ok())
}

/// The numbers of the position at the end of `path` (root first).
pub fn numbering(rec: &GameRecord, path: &[NodeId]) -> Numbering {
    let size = rec.size;
    let mut pos = Position::new(size);
    let mut list = vec![0u32; size.area()];
    let mut counts = Vec::with_capacity(path.len());
    let mut count: Option<u32> = None;
    for (k, &id) in path.iter().enumerate() {
        let node = rec.node(id);
        pos.enter(node);
        if k > 0 {
            count = match node.kind {
                NodeKind::Move { .. } | NodeKind::Pass { .. } if k == 1 => Some(1),
                NodeKind::Move { .. } | NodeKind::Pass { .. } => count.map(|c| c + 1),
                NodeKind::Snapshot | NodeKind::DummyPass => count,
            };
            let parent = rec.node(path[k - 1]);
            let first = parent
                .children
                .iter()
                .copied()
                .find(|&c| rec.node(c).kind != NodeKind::DummyPass);
            if node.branch_numbers_from_one && first != Some(id) {
                list.fill(0);
                count = Some(if node.kind == NodeKind::DummyPass { 0 } else { 1 });
            }
            if let NodeKind::Move { point, .. } = node.kind {
                list[size.index(point)] = count.unwrap_or(node.move_number);
            }
            if let Some(mn) = sgf_mn(rec, id) {
                count = Some(mn);
                if let NodeKind::Move { point, .. } = node.kind {
                    list[size.index(point)] = mn;
                    let mut running = mn;
                    for &before in path[..k].iter().rev() {
                        let b = rec.node(before);
                        if b.children.len() > 1 {
                            break;
                        }
                        if let NodeKind::Move { point, .. } = b.kind {
                            if b.move_number >= running {
                                running = running.saturating_sub(1);
                                list[size.index(point)] = running;
                            }
                        }
                    }
                }
            }
        }
        for (n, stone) in list.iter_mut().zip(&pos.stones) {
            if stone.is_none() {
                *n = 0;
            }
        }
        counts.push(count);
    }
    Numbering { list, counts }
}

/// Variation-tree numbers (`VariationTree.java:285-298`): the main trunk (lane 0) shows each
/// move's real number; other nodes `moveMNNumber`, which shares the board's branch/MN/pass rules,
/// without replaying a board position for every node. Setup nodes have no move number.
pub fn tree_numbers(rec: &GameRecord) -> Vec<u32> {
    let mut numbers = vec![0; rec.node_count()];
    let mut trunk = vec![false; rec.node_count()];
    let mut at = Some(crate::record::ROOT);
    while let Some(id) = at {
        trunk[id as usize] = true;
        at = rec.main_child(id);
    }
    let mut pending = vec![(crate::record::ROOT, None)];
    while let Some((id, parent_count)) = pending.pop() {
        let node = rec.node(id);
        let mut count = match node.kind {
            NodeKind::Move { .. } | NodeKind::Pass { .. } => {
                if node.parent == Some(crate::record::ROOT) { Some(1) }
                else { parent_count.map(|n: u32| n.saturating_add(1)) }
            }
            NodeKind::Snapshot | NodeKind::DummyPass => parent_count,
        };
        if let Some(parent) = node.parent {
            let first = rec.node(parent).children.iter().copied()
                .find(|&c| rec.node(c).kind != NodeKind::DummyPass);
            if node.branch_numbers_from_one && first != Some(id) {
                count = Some(if node.kind == NodeKind::DummyPass { 0 } else { 1 });
            }
            count = sgf_mn(rec, id).or(count);
        }
        if matches!(node.kind, NodeKind::Move { .. } | NodeKind::Pass { .. }) {
            numbers[id as usize] = if trunk[id as usize] { node.move_number } else { count.unwrap_or(node.move_number) };
        }
        pending.extend(node.children.iter().map(|&child| (child, count)));
    }
    numbers
}

/// Captures made by a hover variation, under the current preview rules (0 = no length limit).
pub fn pv_captures(base: &Position, to_play: Stone, pv: &[Option<Point>], limit: u32, remove_dead: bool) -> Vec<Vec<Point>> {
    if !remove_dead {
        return Vec::new();
    }
    let mut pos = base.clone();
    let mut color = to_play;
    pv.iter().take(if limit == 0 { usize::MAX } else { limit as usize }).map(|mv| {
        let mut captured = Vec::new();
        if let Some(point) = *mv {
            pos.place(color, point, &mut captured);
        }
        color = color.opposite();
        captured
    }).collect()
}

/// A move number drawn on a stone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct MoveNumberLabel {
    pub point: Point,
    pub number: u32,
    /// The last move: drawn red, with the red corner triangle.
    pub last: bool,
}

/// 落子评价 rank (`MoveRankDefinition.Rank`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
pub enum MoveRank {
    Best,
    Good,
    Normal,
    Inaccuracy,
    Mistake,
    Blunder,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct RankMark {
    pub point: Point,
    pub rank: MoveRank,
    /// The current position's move: the mark fills the last-move ring.
    pub last: bool,
}

/// 下一手 详细: how the next move of the record compares with the best candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum NextMoveVerdict {
    /// Win rate loss of 10 or more (or a score loss of 5 or more): red, on the wood.
    Bad,
    /// 1.5 (1 without a score) or more, or a score loss of 1 or more: orange.
    Questionable,
    Good,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct NextMoveInfo {
    pub point: Point,
    pub verdict: NextMoveVerdict,
    /// The result after the move, for the shown side (总是显示黑胜率 / 目差显示 applied).
    pub winrate: f64,
    /// `None` without KataGo scores on both positions.
    pub score_lead: Option<f64>,
    /// The mover's score after the move minus the best candidate's (目差显示为与最佳选点之差).
    pub score_diff: Option<f64>,
    /// The candidate on that point is not shown: the move was played and analysed further
    /// than the candidate (`shouldIgnoreBestMove`).
    pub hides_candidate: bool,
    /// The actual next move followed by its best candidate's principal variation.
    pub pv: Vec<Option<Point>>,
    pub pv_captures: Vec<Vec<Point>>,
    /// `nextPvVisits`: the reply's visits for the actual move, then the reply's `pvVisits`;
    /// empty when the engine reported no `pvVisits`.
    pub pv_visits: Vec<u64>,
}

/// What the main board draws besides stones, marks and candidates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct BoardOverlayView {
    /// Move numbers outside a hovered variation. Empty while 手数 is off or 落子评价 shows.
    pub move_numbers: Vec<MoveNumberLabel>,
    /// 落子评价 marks, at most one per point.
    pub ranks: Vec<RankMark>,
    /// The last-move ring gets the red corner triangle (落子评价 of several moves).
    pub rank_corner: bool,
    /// 下一手 详细, while the next-move hints are on.
    pub next_move: Option<NextMoveInfo>,
    /// The 自定义信息顺序 dialog's KataGo sample as a candidate of this position would show it.
    pub info_order_sample: ShownValues,
}

/// `moveOverlayOrder`: 落子评价 shows while move numbers are off (outside play modes).
pub fn shows_ranks(s: &DisplaySettings) -> bool {
    s.move_numbers == 0 && s.move_rank >= 0
}

/// `SuggestionInfoOrderSettings` previews a fixed KataGo candidate (68.5%, lead 5.7, side to move)
/// with the shown side and 盘面 of the current position (`Utils.convertScoreToString`).
pub fn info_order_sample(s: &DisplaySettings, to_play: Stone, komi: f64) -> ShownValues {
    Viewpoint::from(s).candidate(68.5, Some(5.7), to_play, komi)
}

/// `drawMoveNumbers` for the position at the end of `path` (no hovered variation).
pub fn number_labels(rec: &GameRecord, path: &[NodeId], numbering: &Numbering, s: &DisplaySettings) -> Vec<MoveNumberLabel> {
    let Some(&current) = path.last() else { return Vec::new() };
    let node = rec.node(current);
    if node.kind == NodeKind::Snapshot || shows_ranks(s) {
        return Vec::new();
    }
    let all_in_branch = s.all_numbers_in_branch && !rec.is_main_trunk(current);
    let allow = s.move_numbers;
    if allow == 0 && !all_in_branch {
        return Vec::new();
    }
    let last_point = match node.kind {
        NodeKind::Move { point, .. } => Some(point),
        _ => None,
    };
    let last = numbering.list.iter().copied().max().unwrap_or(0);
    let width = rec.size.width as usize;
    let mut out = Vec::new();
    for (i, &n) in numbering.list.iter().enumerate() {
        if n == 0 {
            continue;
        }
        if !all_in_branch && allow > -1 && i64::from(last) - i64::from(n) >= i64::from(allow) {
            continue;
        }
        let number = if !all_in_branch && s.move_numbers_from_one && allow > 0 && last > allow as u32 {
            n - (last - allow as u32)
        } else {
            n
        };
        let point = Point {
            x: (i % width) as u16,
            y: (i / width) as u16,
        };
        out.push(MoveNumberLabel {
            point,
            number,
            last: last_point == Some(point),
        });
    }
    out
}

/// `MoveRankDefinition.reachesThreshold`.
fn reaches(s: &DisplaySettings, winrate_diff: f64, score_diff: Option<f64>, level: usize) -> bool {
    let win = winrate_diff <= s.win_loss_thresholds[level - 1];
    let score = score_diff.is_some_and(|d| d <= s.score_loss_thresholds[level - 1]);
    match s.move_rank_mode {
        MoveRankMode::Score => score,
        MoveRankMode::Winrate => win,
        MoveRankMode::Combined => win || score,
        MoveRankMode::Auto if score_diff.is_none() => win,
        MoveRankMode::Auto => score || (level >= 4 && win),
    }
}

/// `MoveRankDefinition.classifyDiffs`: the diffs are the mover's change (negative is a loss).
pub fn classify(s: &DisplaySettings, winrate_diff: f64, score_diff: Option<f64>) -> MoveRank {
    const BY_LEVEL: [MoveRank; 5] = [
        MoveRank::Good,
        MoveRank::Normal,
        MoveRank::Inaccuracy,
        MoveRank::Mistake,
        MoveRank::Blunder,
    ];
    (1..=5)
        .rev()
        .find(|&level| reaches(s, winrate_diff, score_diff, level))
        .map_or(MoveRank::Best, |level| BY_LEVEL[level - 1])
}

/// `Board.lastWinrateDiff` / `lastScoreMeanDiff` of the move into `after`: the mover's change
/// from the position before (`before`, side to move there) to the position after.
fn diffs(before: &NodeAnalysisView, after: &NodeAnalysisView) -> (f64, Option<f64>) {
    let same = before.to_play == after.to_play;
    let winrate = if same {
        before.winrate - after.winrate
    } else {
        (100.0 - before.winrate) - after.winrate
    };
    let score = match (before.score_lead, after.score_lead) {
        (Some(b), Some(a)) => Some(if same { b - a } else { -b - a }),
        _ => None,
    };
    (winrate, score)
}

fn candidate_index(a: &NodeAnalysisView, point: Point) -> Option<usize> {
    a.candidates.iter().position(|c| c.point == Some(point))
}

/// `Board.updateIsBest`: the move is the first candidate of the position before.
fn played_best(before: &NodeAnalysisView, point: Point) -> bool {
    before.candidates.first().is_some_and(|c| c.point == Some(point))
}

/// `Board.isMatchAi(.., match-ai-moves 3, match-ai-percents-playouts 20).isBest`: the first
/// candidate, with at least 20% of the most visits.
fn matched_best(before: &NodeAnalysisView, point: Point) -> bool {
    let max = before.candidates.iter().map(|c| c.visits).max().unwrap_or(0);
    candidate_index(before, point) == Some(0)
        && max > 0
        && before.candidates[0].visits as f64 / max as f64 * 100.0 >= 20.0
}

/// `drawMoveRankMark` along `path` back from its end.
pub fn rank_marks<'a>(
    rec: &GameRecord,
    path: &[NodeId],
    numbering: &Numbering,
    s: &DisplaySettings,
    analysis: impl Fn(NodeId) -> Option<&'a NodeAnalysisView>,
) -> Vec<RankMark> {
    if !shows_ranks(s) || path.len() < 2 {
        return Vec::new();
    }
    let size = rec.size;
    let last = path.len() - 1;
    let mut limit = s.move_rank;
    let mut drawn = vec![false; size.area()];
    let mut out = Vec::new();
    for k in (1..=last).rev() {
        if s.move_rank > 0 {
            limit -= 1;
            if limit < 0 {
                break;
            }
        }
        let node = rec.node(path[k]);
        let NodeKind::Move { point, .. } = node.kind else { continue };
        let index = size.index(point);
        if drawn[index] || numbering.list[index] != numbering.counts[k].unwrap_or(node.move_number) {
            continue;
        }
        let (Some(before), Some(after)) = (analysis(path[k - 1]), analysis(path[k])) else { continue };
        if before.visits == 0 || after.visits == 0 {
            continue;
        }
        let best = if k == last { played_best(before, point) } else { matched_best(before, point) };
        let rank = if best {
            MoveRank::Best
        } else {
            let (winrate, score) = diffs(before, after);
            classify(s, winrate, after.score_lead.and(score))
        };
        drawn[index] = true;
        out.push(RankMark {
            point,
            rank,
            last: k == last,
        });
    }
    out
}

/// `drawNextMoves` 详细 for the first move child of `node` (`current` / `next` analysis).
pub fn next_move_info<'a>(
    rec: &GameRecord,
    node: NodeId,
    s: &DisplaySettings,
    no_capture: bool,
    limit: u32,
    analysis: impl Fn(NodeId) -> Option<&'a NodeAnalysisView>,
) -> Option<NextMoveInfo> {
    if !(s.next_moves && s.next_move_info) {
        return None;
    }
    let (child, point) = rec.node(node).children.iter().find_map(|&c| match rec.node(c).kind {
        NodeKind::Move { point, .. } => Some((c, point)),
        _ => None,
    })?;
    let this = analysis(node).filter(|a| !a.candidates.is_empty())?;
    let after = analysis(child).filter(|a| !a.candidates.is_empty())?;
    // notEnoughSuggestionAt: shown unless the move is a candidate searched at least as far.
    let hides_candidate = match this.candidates.iter().find(|c| c.point == Some(point)) {
        None => false,
        Some(c) if c.order > 0 && i128::from(c.visits) < i128::from(s.next_move_min_playouts)
            && i128::from(after.visits) >= i128::from(s.next_move_min_playouts) => true,
        Some(_) => return None,
    };
    let (best, reply) = (&this.candidates[0], &after.candidates[0]);
    let winrate_diff = best.winrate - (100.0 - reply.winrate);
    let kata = best.score_lead.is_some() && reply.score_lead.is_some();
    let verdict = match (kata, best.score_lead.zip(reply.score_lead)) {
        (true, Some((b, r))) if winrate_diff >= 10.0 || b + r >= 5.0 => NextMoveVerdict::Bad,
        (true, Some((b, r))) if winrate_diff >= 1.5 || b + r >= 1.0 => NextMoveVerdict::Questionable,
        (false, _) if winrate_diff >= 10.0 => NextMoveVerdict::Bad,
        (false, _) if winrate_diff >= 1.0 => NextMoveVerdict::Questionable,
        _ => NextMoveVerdict::Good,
    };
    let shown = Viewpoint::from(s).as_candidate(reply.winrate, kata.then_some(reply.score_lead).flatten(), after.to_play, after.komi);
    let mut pv = Vec::with_capacity(reply.pv.len() + 1);
    pv.push(Some(point));
    pv.extend_from_slice(&reply.pv);
    let captures = pv_captures(&rec.position(node), this.to_play, &pv, limit, !no_capture);
    let pv_visits = if reply.pv_visits.is_empty() {
        Vec::new()
    } else {
        std::iter::once(reply.visits).chain(reply.pv_visits.iter().copied()).collect()
    };
    Some(NextMoveInfo {
        point,
        verdict,
        winrate: shown.shown.winrate,
        score_lead: shown.shown.score_lead,
        score_diff: shown.score_lead.zip(best.score_lead).map(|(mover, best)| mover - best),
        hides_candidate,
        pv,
        pv_captures: captures,
        pv_visits,
    })
}
