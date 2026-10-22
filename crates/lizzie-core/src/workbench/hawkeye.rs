//! 超级鹰眼 (`MoveListFrame`): whether its window is open, its side boxes, navigation from its
//! lists and charts, and the projection it draws, computed from the per-move evaluation
//! ([`Workbench::evaluation_in`]) of the line it reads, only while it is open.

use super::analysis::AnalysisStore;
use super::Workbench;
use crate::api::{
    ApiError, ErrorKind, HawkeyeCommand, HawkeyeMistake, HawkeyePane, HawkeyeRunning, HawkeyeSide, HawkeyeSpread,
    HawkeyeStage, HawkeyeView, MoveEvaluation,
};
use crate::record::{NodeId, NodeKind, Stone, ROOT};
use crate::settings::{HawkeyeBranch, HawkeyePlacement, HawkeyeSettings};

/// The window's session state (the 黑 / 白 boxes are not remembered).
#[derive(Clone, Copy, Debug)]
pub(super) struct HawkeyeSession {
    pub black: bool,
    pub white: bool,
}

impl Default for HawkeyeSession {
    fn default() -> Self {
        HawkeyeSession {
            black: true,
            white: true,
        }
    }
}

/// `[black, white]` index of a side.
fn side(stone: Stone) -> usize {
    usize::from(stone == Stone::White)
}

/// Java's statistics count a first-candidate move as losing nothing (`NodeInfo.getWinrateDiff`,
/// `getScoreMeanDiff`), and an analysed move without a score lead as changing it by 0
/// (`NodeInfo.scoreMeanDiff` stays 0; `drawKeyPanel` 6527-6530, `drawMin` 2298-2302 count it).
fn counted(e: &MoveEvaluation) -> (f64, f64) {
    if e.best {
        (0.0, 0.0)
    } else {
        (e.winrate_change.unwrap_or(0.0), e.score_change.unwrap_or(0.0))
    }
}

/// `drawLossPanel`: 0 above the first threshold, 5 at or beyond the fifth.
fn bucket(change: f64, thresholds: &[f64; 5]) -> usize {
    thresholds.iter().position(|&t| change > t).unwrap_or(5)
}

/// `drawMin` 胜率损失统计 / 目数损失统计: under the first threshold (gains too), between, beyond.
fn miss(change: f64, range1: f64, range2: f64) -> usize {
    let loss = change.abs();
    if change > 0.0 || loss < range1 {
        0
    } else if loss <= range2 {
        1
    } else {
        2
    }
}

/// `drawMin` 胜率损失分布 / 目数损失分布: every analysed move counts; gains add to nothing
/// (`if (diff > 0) continue`).
fn spread_add(s: &mut HawkeyeSpread, change: f64, range1: f64, range2: f64) {
    s.analyzed += 1;
    if change > 0.0 {
        return;
    }
    let (loss, square) = (change.abs(), change * change);
    s.sum += loss;
    s.squares += square;
    for (band, range) in [(&mut s.above1, range1), (&mut s.above2, range2)] {
        if loss >= range {
            band.count += 1;
            band.sum += loss;
            band.squares += square;
        }
    }
}

/// `bigMistakeInfo`: each side's five largest losses, then in move order.
fn mistakes(moves: &[MoveEvaluation], picked: &[(u32, f64, Stone)]) -> Vec<HawkeyeMistake> {
    let mut out = Vec::new();
    for stone in [Stone::Black, Stone::White] {
        let mut mine: Vec<&(u32, f64, Stone)> = picked.iter().filter(|p| p.2 == stone).collect();
        mine.sort_by(|a, b| a.1.total_cmp(&b.1));
        out.extend(mine.into_iter().take(5).map(|&(index, change, _)| HawkeyeMistake { index, change }));
    }
    out.sort_by_key(|m| moves[m.index as usize].move_number);
    out
}

impl Workbench {
    /// 超级鹰眼's open state, side boxes and navigation.
    pub fn hawkeye(&mut self, op: HawkeyeCommand) -> Result<bool, ApiError> {
        match op {
            HawkeyeCommand::Toggle => {
                let open = !self.config.hawkeye.show_frame;
                self.commit_config(|c| c.hawkeye.show_frame = open)?;
                if open {
                    self.hawkeye_session = HawkeyeSession::default();
                } else {
                    // `toggleBadMoves` closing: `clickbadmove = outOfBoundCoordinate`.
                    self.problem_mark = None;
                }
                Ok(true)
            }
            HawkeyeCommand::SetSides { black, white } => {
                let changed = (self.hawkeye_session.black, self.hawkeye_session.white) != (black, white);
                self.hawkeye_session = HawkeyeSession { black, white };
                Ok(changed)
            }
            HawkeyeCommand::GotoMove { node } => self.hawkeye_goto_move(node),
            // A click left of the chart sends Java's -1 (`moveNumber`), which walks back to the
            // root like move number 0 (`goToMoveNumberHelper` stops at the root).
            HawkeyeCommand::GotoMoveNumber { number: 0 } => self.goto_node(ROOT),
            HawkeyeCommand::GotoMoveNumber { number } => Ok(self.goto_move_number(number)),
        }
    }

    /// `handleTableDoubleClick` (any click: Java checks no click count): 主干 first returns to the
    /// main trunk (`moveToMainTrunk`), then `goToMoveNumber(moveNumber - 1)`, which on the line
    /// holding the move is the move's parent; the move is marked (`clickbadmove`).
    fn hawkeye_goto_move(&mut self, node: NodeId) -> Result<bool, ApiError> {
        let rec = &self.doc.record;
        let parent = rec
            .contains(node)
            .then(|| rec.node(node))
            .filter(|n| matches!(n.kind, NodeKind::Move { .. }))
            .filter(|_| self.config.hawkeye.branch == HawkeyeBranch::Current || rec.is_main_trunk(node))
            .and_then(|n| n.parent)
            .ok_or_else(|| ApiError::new(ErrorKind::InvalidArgument, format!("no listed move {node}")))?;
        let moved = self.goto_node(parent)?;
        let mark = Some((self.doc.id, parent, node));
        let marked = self.problem_mark != mark;
        self.problem_mark = mark;
        Ok(moved || marked)
    }

    /// 超级鹰眼's remembered choices (`SetThreshold` spinner ranges, `MoveListFrame` fields). The
    /// open state is `Toggle`'s.
    pub fn set_hawkeye_settings(&mut self, settings: HawkeyeSettings) -> Result<bool, ApiError> {
        if settings.top_page > 2
            || settings.chart > 8
            || settings.winrate_threshold > 99
            || settings.score_threshold > 99
            || settings.visits_threshold > 999_999
        {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "hawkeye settings"));
        }
        let settings = HawkeyeSettings {
            show_frame: self.config.hawkeye.show_frame,
            ..settings
        };
        if self.config.hawkeye == settings {
            return Ok(false);
        }
        self.commit_config(|c| c.hawkeye = settings)?;
        Ok(true)
    }

    /// The window's placement when it closes or the application exits (the primary window only,
    /// `Config.persist`), unless the UI position was reset.
    pub fn persist_hawkeye(&mut self, placement: HawkeyePlacement) -> Result<(), ApiError> {
        if self.persist_reset {
            return Ok(());
        }
        self.persisted.hawkeye = Some(placement);
        self.save_persisted()
    }

    /// The projection while the window is open.
    pub(super) fn hawkeye_view(&self) -> Option<HawkeyeView> {
        let s = &self.config.hawkeye;
        if !s.show_frame {
            return None;
        }
        let rec = &self.doc.record;
        let current_move = rec.node(self.cursor).move_number;
        let range = s.range(current_move);
        let primary_katago = self.engines.foreground().is_some_and(|f| self.engines.is_katago(f));
        let secondary_katago = self.engines.secondary().is_some_and(|f| self.engines.is_katago(f));
        Some(HawkeyeView {
            black: self.hawkeye_session.black,
            white: self.hawkeye_session.white,
            first_move: range.0,
            last_move: range.1,
            current_move,
            mark: self.problem_mark().and_then(|m| match rec.node(m).kind {
                NodeKind::Move { point, .. } => Some(point),
                _ => None,
            }),
            primary: self.hawkeye_pane(&self.store, primary_katago, range),
            secondary: self
                .double_engine()
                .then(|| self.hawkeye_pane(&self.secondary_store, secondary_katago, range)),
        })
    }

    /// The line 分支 reads: 主干 is the main trunk (the current line while it is on the trunk), 当前
    /// the current line to its end (`getMainEnd` / `getEnd`).
    fn hawkeye_line(&self) -> Vec<NodeId> {
        let rec = &self.doc.record;
        let end = if self.config.hawkeye.branch == HawkeyeBranch::Main && !rec.is_main_trunk(self.cursor) {
            rec.line_end(ROOT)
        } else {
            rec.line_end(self.cursor)
        };
        rec.path_to(end)
    }

    fn hawkeye_pane(&self, store: &AnalysisStore, engine_katago: bool, range: (i32, i32)) -> HawkeyePane {
        let rec = &self.doc.record;
        let s = &self.config.hawkeye;
        let d = &self.config.display;
        let line = self.hawkeye_line();
        let katago = engine_katago || store.has_scores();
        let moves: Vec<MoveEvaluation> = line.iter().filter_map(|&n| self.evaluation_in(store, n)).collect();
        let parent_number = |e: &MoveEvaluation| i64::from(rec.parent(e.node).map_or(0, |p| rec.node(p).move_number));
        // Java keeps move N's numbers on node N - 1: `moveNumber + 1 <= last && moveNumber >= first`.
        let in_range = |e: &MoveEvaluation| {
            let p = parent_number(e);
            p + 1 <= i64::from(range.1) && p >= i64::from(range.0)
        };

        let mut pane = HawkeyePane {
            katago,
            total_moves: line.last().map_or(0, |&n| rec.node(n).move_number),
            off_trunk_from: line.iter().find(|&&n| !rec.is_main_trunk(n)).map(|&n| rec.node(n).move_number),
            points: self.points_of(store, line.clone()),
            moves: Vec::new(),
            running: Vec::with_capacity(moves.len() + 1),
            rows: Vec::new(),
            detail_rows: Vec::new(),
            pinned: None,
            sides: [HawkeyeSide::default(); 2],
            winrate_buckets: [[0; 6]; 2],
            score_buckets: [[0; 6]; 2],
            winrate_spread: [HawkeyeSpread::default(); 2],
            score_spread: [HawkeyeSpread::default(); 2],
            stages: [[HawkeyeStage::default(); 3]; 2],
            winrate_misses: [[[0; 3]; 3]; 2],
            score_misses: [[[0; 3]; 3]; 2],
            winrate_mistakes: Vec::new(),
            score_mistakes: Vec::new(),
        };
        let mut running = [HawkeyeRunning::default(); 2];
        let (mut win_picked, mut score_picked) = (Vec::new(), Vec::new());
        let (wr1, wr2) = (f64::from(s.winrate_range1), f64::from(s.winrate_range2));
        let (sr1, sr2) = (f64::from(s.score_range1), f64::from(s.score_range2));
        for (i, e) in moves.iter().enumerate() {
            let k = side(e.mover);
            let index = i as u32;
            let (win, score) = counted(e);
            pane.running.push(running);
            running[k].share_sum += e.share;
            running[k].count += 1;

            // 列表: both positions analysed, over the thresholds (raw changes); the 详细列表 also
            // needs the side checked, the 简易列表 splits by side whatever the boxes.
            let shown_side = if e.mover == Stone::Black {
                self.hawkeye_session.black
            } else {
                self.hawkeye_session.white
            };
            let score_ok = !katago
                || e.score_change
                    .map_or(s.score_threshold == 0, |c| c.abs() >= f64::from(s.score_threshold));
            if e.analyzed
                && e.winrate_change.unwrap_or(0.0).abs() >= f64::from(s.winrate_threshold)
                && e.visits_before >= u64::from(s.visits_threshold)
                && e.visits_after >= u64::from(s.visits_threshold)
                && score_ok
            {
                pane.rows.push(index);
                if shown_side {
                    pane.detail_rows.push(index);
                }
            }

            // 统计信息, 胜率吻合, 吻合度走势, 损失分布: the 统计 range.
            if in_range(e) {
                let sd = &mut pane.sides[k];
                sd.match_analyzed += 1;
                sd.share_sum += e.share;
                sd.matched += u32::from(e.match_ai);
                sd.best += u32::from(e.best);
                if e.analyzed {
                    sd.analyzed += 1;
                    if win < 0.0 {
                        sd.winrate_loss_sum -= win;
                    }
                    pane.winrate_buckets[k][bucket(win, &d.win_loss_thresholds)] += 1;
                    spread_add(&mut pane.winrate_spread[k], win, wr1, wr2);
                    if score < 0.0 {
                        sd.score_loss_sum -= score;
                    }
                    pane.score_buckets[k][bucket(score, &d.score_loss_thresholds)] += 1;
                    spread_add(&mut pane.score_spread[k], score, sr1, sr2);
                }
            }

            // 分段吻合度, 损失统计: the whole line, by stage of the move's number.
            let number = i64::from(e.move_number);
            let stage = if number <= i64::from(s.opening_end_move) {
                0
            } else if number <= i64::from(s.middle_end_move) {
                1
            } else {
                2
            };
            let st = &mut pane.stages[k][stage];
            st.match_analyzed += 1;
            st.share_sum += e.share;
            if e.analyzed {
                st.analyzed += 1;
                if win < 0.0 {
                    st.winrate_loss_sum -= win;
                }
                pane.winrate_misses[k][stage][miss(win, wr1, wr2)] += 1;
                if score < 0.0 {
                    st.score_loss_sum -= score;
                }
                pane.score_misses[k][stage][miss(score, sr1, sr2)] += 1;
            }

            // 翻车现场: from the statistics start (`moveNumber + 2 > firstMove` on node N - 1),
            // never move 1 (Java's walk stops before the root).
            let off_root = rec.parent(e.node).is_some_and(|p| p != ROOT);
            if e.analyzed && off_root && parent_number(e) + 2 > i64::from(range.0) {
                win_picked.push((index, win, e.mover));
                score_picked.push((index, score, e.mover));
            }
        }
        pane.running.push(running);
        pane.winrate_mistakes = mistakes(&moves, &win_picked);
        pane.score_mistakes = mistakes(&moves, &score_picked);

        // 置顶当前手: the move to the browsing position, on the line 分支 reads, both positions
        // analysed.
        if s.top_current_move && (s.branch == HawkeyeBranch::Current || rec.is_main_trunk(self.cursor)) {
            pane.pinned = moves
                .iter()
                .position(|e| e.node == self.cursor && e.analyzed)
                .map(|i| i as u32);
        }
        pane.moves = moves;
        pane
    }
}
