//! Analysis information beside the candidates: Kata形势 (the search ownership), each move's
//! result against the analysis of the position it was played in (统计, AI总评分, 问题手) and
//! 终局点目.

use super::analysis::AnalysisStore;
use super::Workbench;
use crate::api::{
    ApiError, ErrorKind, KataEstimateMode, KataEstimateView, MoveChange, MoveEvaluation, ProblemListView,
    ProblemMoveView, ScoreCommand, ScoreView, StatisticsView,
};
use crate::record::{DocumentEdit, NodeId, NodeKind, Stone, ROOT};
use crate::scoring::Marks;
use crate::settings::{KataEstimateState, ProblemListSettings, ProblemSort};

/// Kata形势 this session (`Config.showKataGoEstimate*`, `isHiddenKataEstimate`).
#[derive(Clone, Copy, Debug)]
pub(super) struct EstimateSession {
    pub state: KataEstimateState,
    /// 隐藏: requested but drawn nowhere; `.` then toggles the boards instead.
    pub hidden: bool,
}

impl EstimateSession {
    /// The state a start begins with (`Config.java:1757-1760`).
    pub fn starting(saved: KataEstimateState) -> EstimateSession {
        EstimateSession {
            state: saved,
            hidden: saved.hidden(),
        }
    }
}

/// 终局点目 in progress (`LizzieFrame.isInScoreMode`, `Board.boardGroupInfo`).
#[derive(Clone, Debug)]
pub(super) struct Scoring {
    /// Document, node and position stamp the marks belong to.
    at: (u64, NodeId, u64),
    marks: Marks,
    /// Analysis was running when scoring began (`ponderStatusBeforeScore`).
    resume: bool,
}

impl Workbench {
    // -------------------------------------------------------------------- Kata形势

    /// Tells the runtime whether KataGo should report ownership.
    pub(super) fn apply_estimate(&mut self) {
        self.engines
            .set_ownership(self.estimate.state.shown, self.config.display.use_moves_ownership);
    }

    /// Kata形势显示 menu (`Menu.java:2231-2348`). 关闭 always remembers it off; the board
    /// choices are remembered only with 记忆评估显示状态.
    pub fn set_kata_estimate(&mut self, mode: KataEstimateMode) -> Result<bool, ApiError> {
        let state = |shown, on_main, on_sub| KataEstimateState { shown, on_main, on_sub };
        let current = self.estimate.state;
        let (next, hidden) = match mode {
            KataEstimateMode::Close => (
                KataEstimateState {
                    shown: false,
                    ..current
                },
                false,
            ),
            KataEstimateMode::Hide => (state(true, false, false), true),
            KataEstimateMode::Main => (state(true, true, false), false),
            KataEstimateMode::Sub => (state(true, false, true), false),
            KataEstimateMode::Both => (state(true, true, true), false),
        };
        let saved = self.config.display.kata_estimate;
        let remembered = match mode {
            KataEstimateMode::Close => KataEstimateState { shown: false, ..saved },
            _ if self.config.display.kata_estimate_save_status => next,
            _ => saved,
        };
        let changed = (next, hidden) != (current, self.estimate.hidden) || remembered != saved;
        if remembered != saved {
            self.commit_config(|c| c.display.kata_estimate = remembered)?;
        }
        self.estimate = EstimateSession { state: next, hidden };
        self.apply_estimate();
        Ok(changed)
    }

    /// `.` and the bottom bar Kata形势 (`LizzieFrame.toggleShowKataEstimate`): only with a KataGo
    /// foreground engine; not remembered.
    pub fn toggle_kata_estimate(&mut self) -> bool {
        if !self.engines.foreground().is_some_and(|f| self.engines.is_katago(f)) {
            return false;
        }
        let s = &mut self.estimate.state;
        if self.estimate.hidden {
            s.on_main = !s.on_main;
            s.on_sub = !s.on_sub;
        } else {
            s.shown = !s.shown;
            if !s.on_main && !s.on_sub {
                s.on_sub = true;
            }
            s.on_main = true;
        }
        self.apply_estimate();
        true
    }

    /// Display settings changes that act on Kata形势: choosing a square style turns it on for a
    /// KataGo engine (`Menu.java:2388-2389`), and 记忆评估显示状态 writes the state (or, turned
    /// off, clears it) at once (`Menu.java:2357-2371`).
    pub(super) fn estimate_settings_changed(&mut self, previous: &crate::settings::DisplaySettings) {
        let now = &self.config.display;
        if now.kata_estimate_style != previous.kata_estimate_style
            && !self.estimate.state.shown
            && self.engines.foreground().is_some_and(|f| self.engines.is_katago(f))
        {
            self.toggle_kata_estimate();
        }
        self.apply_estimate();
    }

    /// The settings to commit when 记忆评估显示状态 changes.
    pub(super) fn remembered_estimate(&self, save_status: bool) -> KataEstimateState {
        if save_status {
            self.estimate.state
        } else {
            KataEstimateState {
                shown: false,
                on_main: false,
                on_sub: false,
            }
        }
    }

    pub(super) fn kata_estimate_view(&self) -> KataEstimateView {
        let rec = &self.doc.record;
        let ownership = self.estimate.state.shown.then(|| {
            // The previous position's estimate until this one has its own (`preEstimateArray`).
            self.store
                .ownership(rec, self.cursor)
                .or_else(|| rec.parent(self.cursor).and_then(|p| self.store.ownership(rec, p)))
                .cloned()
        });
        KataEstimateView {
            state: self.estimate.state,
            hidden: self.estimate.hidden,
            ownership: ownership.flatten(),
        }
    }

    // -------------------------------------------------------------------- 统计

    /// `Board.isMatchAi` and `updateMovelist` for the move leading to `node`, when the position
    /// it was played in has analysis.
    fn evaluation(&self, node: NodeId) -> Option<MoveEvaluation> {
        self.evaluation_in(&self.store, node)
    }

    /// [`Self::evaluation`] from `store`'s results: the foreground engine's, or 双引擎模式's
    /// secondary engine's (`updateMovelist2`, `nodeInfo2`).
    pub(super) fn evaluation_in(&self, store: &AnalysisStore, node: NodeId) -> Option<MoveEvaluation> {
        let rec = &self.doc.record;
        let n = rec.node(node);
        let NodeKind::Move { color, point } = n.kind else {
            return None;
        };
        let before = store.get(rec, n.parent?)?;
        if before.visits == 0 {
            return None;
        }
        let s = &self.config.display;
        let max = before.candidates.iter().map(|c| c.visits).max().unwrap_or(0) as f64;
        let index = before.candidates.iter().position(|c| c.point == Some(point));
        let (mut matched, mut share, mut match_ai, mut best) = (None, 0.0, false, false);
        if let Some(i) = index {
            let ratio = before.candidates[i].visits as f64 / max;
            if ratio * 100.0 >= s.match_ai_percents_playouts && (i as u32) < s.match_ai_moves {
                match_ai = true;
                best = i == 0;
            }
            share = if i == 0 { 1.0 } else { ratio };
            matched = Some(i as u32 + 1);
        }
        // `NodeInfo.analyzed` needs both positions (`previousPlayouts > 0 && playouts > 0`); then
        // the position analysed before the move is its own parent, which `lastWinrateDiff` finds.
        let after = store.get(rec, node).filter(|a| a.visits > 0);
        let change = self.change_in(store, n.parent?, node);
        // The mover's view of a result (`100 - winrate`, `-scoreMean` for the side to move next).
        let mover = |a: &crate::api::NodeAnalysisView| {
            let own = a.to_play == color;
            (
                if own { a.winrate } else { 100.0 - a.winrate },
                a.score_lead.map(|v| if own { v } else { -v }),
            )
        };
        let (best_winrate, best_score) = mover(before);
        let (winrate, score) = after.map(mover).map_or((None, None), |(w, s)| (Some(w), s));
        Some(MoveEvaluation {
            node,
            move_number: n.move_number,
            mover: color,
            point,
            matched,
            share,
            match_ai,
            best,
            analyzed: after.is_some(),
            winrate_change: change.map(|c| c.winrate),
            score_change: change.and_then(|c| c.score),
            winrate,
            score,
            best_winrate,
            best_score,
            visits_before: before.visits,
            visits_after: after.map_or(0, |a| a.visits),
        })
    }

    /// The mover's change from the analysis at `from` to that at `to`, both analysed
    /// (`Board.lastWinrateDiff`, `drawMoveStatistics`).
    fn change(&self, from: NodeId, to: NodeId) -> Option<MoveChange> {
        self.change_in(&self.store, from, to)
    }

    fn change_in(&self, store: &AnalysisStore, from: NodeId, to: NodeId) -> Option<MoveChange> {
        let rec = &self.doc.record;
        let before = store.get(rec, from).filter(|a| a.visits > 0)?;
        let after = store.get(rec, to).filter(|a| a.visits > 0)?;
        // `after` is for the side to move after the move, the mover's opponent.
        let opponent = |a: &crate::api::NodeAnalysisView, value: f64, flipped: f64| {
            if a.to_play == after.to_play {
                value
            } else {
                flipped
            }
        };
        Some(MoveChange {
            winrate: opponent(before, before.winrate, 100.0 - before.winrate) - after.winrate,
            score: before
                .score_lead
                .zip(after.score_lead)
                .map(|(b, a)| opponent(before, b, -b) - a),
        })
    }

    pub(super) fn statistics(&self) -> StatisticsView {
        if self.store.is_empty() {
            return StatisticsView::default();
        }
        let rec = &self.doc.record;
        let moves: Vec<MoveEvaluation> = self
            .graph_nodes()
            .into_iter()
            .filter_map(|n| self.evaluation(n))
            .collect();
        // AI总评分 (`LizzieFrame.drawCaptured`): the moves up to here and the one played from here.
        let mut counted = rec.path_to(self.cursor);
        counted.extend(rec.main_child(self.cursor));
        counted.sort_unstable();
        let (mut black, mut white) = ((0.0, 0u32), (0.0, 0u32));
        for e in moves.iter().filter(|e| counted.binary_search(&e.node).is_ok()) {
            let side = if e.mover == Stone::Black {
                &mut black
            } else {
                &mut white
            };
            side.0 += e.share;
            side.1 += 1;
        }
        let mean = |(sum, count): (f64, u32)| (count > 0).then(|| sum * 100.0 / count as f64);
        let last_move = rec.parent(self.cursor).and_then(|p| {
            self.change(p, self.cursor)
                .or_else(|| rec.parent(p).and_then(|g| self.change(g, self.cursor)))
        });
        StatisticsView {
            moves,
            ai_score_black: mean(black),
            ai_score_white: mean(white),
            last_move,
        }
    }

    // -------------------------------------------------------------------- 问题手

    /// The marked problem move while the position before it is browsed (`clickbadmove`).
    pub(super) fn problem_mark(&self) -> Option<NodeId> {
        self.problem_mark
            .filter(|&(doc, at, _)| doc == self.doc.id && at == self.cursor)
            .map(|(_, _, node)| node)
    }

    /// `refreshProblemListSnapshot` / `buildProblemMoveEntry`: main-trunk moves from move 2 whose
    /// mover lost at least the threshold of win rate, best moves excepted.
    pub(super) fn problem_list(&self) -> ProblemListView {
        let rec = &self.doc.record;
        let s = &self.config.problems;
        let thresholds = self.config.display.win_loss_thresholds.map(f64::abs);
        let mark = self.problem_mark();
        let current_number = rec.node(self.cursor).move_number;
        let (mut black, mut white, mut analyzed, mut total) = (Vec::new(), Vec::new(), 0, 0);
        for node in rec.path_to(rec.line_end(ROOT)) {
            let n = rec.node(node);
            let NodeKind::Move { color, point } = n.kind else {
                continue;
            };
            let Some(parent) = n.parent.filter(|_| n.move_number > 1) else {
                continue;
            };
            total += 1;
            let analysed = |at| self.store.get(rec, at).filter(|a| a.visits > 0);
            let (Some(before), Some(after)) = (analysed(parent), analysed(node)) else {
                continue;
            };
            analyzed += 1;
            let Some(e) = self.evaluation(node).filter(|e| !e.best) else {
                continue;
            };
            if before.visits < s.playouts_threshold || after.visits < s.playouts_threshold {
                continue;
            }
            let loss = (-e.winrate_change.unwrap_or(0.0)).max(0.0);
            if loss < s.winrate_threshold {
                continue;
            }
            let tier = (1..=5u8)
                .rev()
                .find(|&k| loss >= thresholds[usize::from(k) - 1])
                .unwrap_or(u8::from(loss > 0.0));
            let entry = ProblemMoveView {
                node,
                move_number: n.move_number,
                mover: color,
                point,
                winrate_loss: loss,
                score_loss: e.score_change.map(|c| (-c).max(0.0)),
                playouts: after.visits,
                tier,
                // `isCurrentProblemMove`: by move number, or the marked move one ahead.
                current: current_number == n.move_number || (mark == Some(node) && current_number + 1 == n.move_number),
            };
            if color == Stone::Black {
                black.push(entry);
            } else {
                white.push(entry);
            }
        }
        for list in [&mut black, &mut white] {
            match s.sort {
                ProblemSort::LossDesc => list.sort_by(|a, b| {
                    b.winrate_loss
                        .total_cmp(&a.winrate_loss)
                        .then(a.move_number.cmp(&b.move_number))
                }),
                ProblemSort::MoveAsc => list.sort_by_key(|e| e.move_number),
                ProblemSort::MoveDesc => list.sort_by_key(|e| std::cmp::Reverse(e.move_number)),
            }
        }
        ProblemListView {
            black,
            white,
            analyzed,
            total,
            // `AnalysisEngine.isAnalysisInProgress`: quick and flash analysis share the engine.
            running: self.quick_running() || self.flash_active(),
            mark: mark.and_then(|m| match rec.node(m).kind {
                NodeKind::Move { point, .. } => Some(point),
                _ => None,
            }),
        }
    }

    /// `jumpToProblemMove`: the position before the main-trunk move, with the move marked.
    pub fn jump_to_problem_move(&mut self, node: NodeId) -> Result<bool, ApiError> {
        let rec = &self.doc.record;
        let parent = rec
            .contains(node)
            .then(|| rec.node(node))
            .filter(|n| matches!(n.kind, NodeKind::Move { .. }) && rec.is_main_trunk(node))
            .and_then(|n| n.parent)
            .ok_or_else(|| ApiError::new(ErrorKind::InvalidArgument, format!("no problem move {node}")))?;
        let moved = self.goto_node(parent)?;
        let mark = Some((self.doc.id, parent, node));
        let marked = self.problem_mark != mark;
        self.problem_mark = mark;
        Ok(moved || marked)
    }

    /// The 评论/问题手 page, 黑/白, 问题手排序 and the control bar (`SidebarPanel`, `BlunderListPanel`).
    pub fn set_problem_list(&mut self, settings: ProblemListSettings) -> Result<bool, ApiError> {
        if !settings.winrate_threshold.is_finite() || settings.winrate_threshold < 0.0 {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "problem list threshold"));
        }
        if self.config.problems == settings {
            return Ok(false);
        }
        self.commit_config(|c| c.problems = settings)?;
        Ok(true)
    }

    // -------------------------------------------------------------------- 终局点目

    fn scoring_key(&self) -> (u64, NodeId, u64) {
        (self.doc.id, self.cursor, self.doc.record.node(self.cursor).stamp)
    }

    /// Fresh marks for the current position (`Board.getGroupInfo`).
    fn fresh_marks(&self) -> Marks {
        Marks::new(&self.doc.record.position(self.cursor))
    }

    pub fn score(&mut self, op: ScoreCommand) -> Result<bool, ApiError> {
        match op {
            ScoreCommand::Toggle => {
                if let Some(s) = self.scoring.take() {
                    // `endFinalScore`.
                    if s.resume {
                        self.paused = false;
                    }
                } else {
                    // `startFinalScore`.
                    let resume = !self.paused && self.engines.foreground().is_some();
                    self.paused = true;
                    self.scoring = Some(Scoring {
                        at: self.scoring_key(),
                        marks: self.fresh_marks(),
                        resume,
                    });
                }
                self.sync_engine();
                Ok(true)
            }
            ScoreCommand::Point { point } => {
                let key = self.scoring_key();
                let fresh = self.fresh_marks();
                let Some(s) = self.scoring.as_mut() else {
                    return Ok(false);
                };
                if point.x >= fresh.size.width || point.y >= fresh.size.height {
                    return Err(ApiError::new(crate::api::ErrorKind::InvalidArgument, "point"));
                }
                if s.at != key {
                    s.at = key;
                    s.marks = fresh;
                }
                s.marks.toggle(point);
                Ok(true)
            }
            ScoreCommand::Recalculate => {
                let (key, marks) = (self.scoring_key(), self.fresh_marks());
                let Some(s) = self.scoring.as_mut() else {
                    return Ok(false);
                };
                s.at = key;
                s.marks = marks;
                Ok(true)
            }
            ScoreCommand::Confirm => {
                let Some(view) = self.score_view() else {
                    return Ok(false);
                };
                self.edit_document(DocumentEdit::SetResult { result: view.result })
            }
        }
    }

    /// The count of the current position: the session's marks, or none after the position
    /// changed since they were made.
    pub(super) fn score_view(&self) -> Option<ScoreView> {
        let s = self.scoring.as_ref()?;
        let fresh;
        let marks = if s.at == self.scoring_key() {
            &s.marks
        } else {
            fresh = self.fresh_marks();
            &fresh
        };
        let position = self.doc.record.position(self.cursor);
        let territory = self.config.display.territory_scoring;
        Some(marks.score(self.cursor, &position, self.komi, territory, self.handicap_bonus()))
    }

    /// White's handicap compensation under KataGo's reported `whiteHandicapBonus`
    /// (`ScoreResult.setScore`, `countBlackHandicapStones`): the Black moves played before
    /// White's first move, with the starting position's Black stones.
    fn handicap_bonus(&self) -> u32 {
        let bonus = self
            .engines
            .foreground()
            .and_then(|f| self.engines.confirmed_rules(f))
            .and_then(|r| {
                r.field("whiteHandicapBonus")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        let minus = match bonus.as_deref() {
            Some("N") => 0,
            Some("N-1") => 1,
            _ => return 0,
        };
        let rec = &self.doc.record;
        let mut count = 0u32;
        for n in rec.path_to(self.cursor).into_iter().rev() {
            match rec.node(n).kind {
                NodeKind::Move {
                    color: Stone::Black, ..
                } => count += 1,
                NodeKind::Move {
                    color: Stone::White, ..
                }
                | NodeKind::Pass { color: Stone::White } => count = 0,
                _ => {}
            }
        }
        let setup = &rec.node(ROOT).setup;
        count += setup.add_black.len() as u32;
        if !setup.add_white.is_empty() {
            count = 0;
        }
        if count > 1 {
            count - minus
        } else {
            0
        }
    }
}
