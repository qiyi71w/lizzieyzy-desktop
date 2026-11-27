//! 边下边提醒 on the companion (Java `gui/MoveReviewer`): while the player thinks, the position they
//! are to move in is reviewed; once they moved, the move is judged from that review, with a
//! shorter review of the position after it when the first did not look at the move closely.
//! Reviews run one at a time in the order they were asked for, each position once; every answer
//! belongs to this session's reviewer, which the session drops when it ends, so a late answer finds
//! nothing to change.
use super::*;
use crate::humansl::assist::{Assist, CoachMarks};
use crate::humansl::feedback::{self, MoveFeedback, PositionReview, Verdict};
use serde_json::json;
use std::collections::{HashMap, VecDeque};

/// The review of the player's position: about a second on a mid-range GPU, the time limit keeping
/// slow engines to the same budget (`MoveReviewer.TURN_VISITS`, `TURN_TIME`).
const TURN_VISITS: u32 = 400;
const TURN_SECONDS: f64 = 1.5;
/// The review of the position after a move the first review hardly looked at (`AFTER_VISITS`,
/// `AFTER_TIME`).
const AFTER_VISITS: u32 = 200;
const AFTER_SECONDS: f64 = 0.8;
/// A review that has not answered by then is ended and counts as none (`MoveReviewer.TIMEOUT`).
const TIMEOUT: Duration = Duration::from_secs(15);
/// How long the AI's reply, once its think time is over, waits for the rating of the move it
/// answers (`JUDGEMENT_WAIT_MILLIS`).
pub(super) const VERDICT_WAIT: Duration = Duration::from_secs(4);

#[derive(Default)]
pub(super) struct Reviewer {
    /// Finished reviews by position, with the position's stamp they were made for.
    reviews: HashMap<NodeId, (u64, PositionReview)>,
    tasks: VecDeque<Task>,
    running: Option<Running>,
    next: u64,
    /// The rated moves still in the game, in the order played, by the position after each.
    judged: Vec<(NodeId, MoveFeedback)>,
    /// The position the player is to move in, and its review once done.
    turn: Option<NodeId>,
    turn_review: Option<PositionReview>,
    /// The player's move being rated (the position after it).
    judging: Option<NodeId>,
    pub(super) assist: Assist,
}

#[derive(Clone)]
enum Task {
    Turn(NodeId),
    Judge {
        before: NodeId,
        after: NodeId,
        vertex: String,
        number: u32,
    },
}

struct Running {
    id: String,
    task: Task,
    /// The position searched: the task's own, or a judged move's position after it.
    node: NodeId,
    stamp: u64,
    deadline: Instant,
}

impl Reviewer {
    /// The player's move being rated (`isJudging`).
    pub(super) fn judging(&self) -> Option<NodeId> {
        self.judging
    }

    /// The rating of the move that led to `node`.
    pub(super) fn rating_of(&self, node: NodeId) -> Option<&MoveFeedback> {
        self.judged.iter().rev().find(|(n, _)| *n == node).map(|(_, f)| f)
    }

    pub(super) fn ratings(&self) -> impl Iterator<Item = &MoveFeedback> {
        self.judged.iter().map(|(_, f)| f)
    }

    /// The review of the position the player is to move in at `live`, once it is done.
    pub(super) fn turn_review_at(&self, live: NodeId) -> Option<&PositionReview> {
        self.turn_review.as_ref().filter(|_| self.turn == Some(live))
    }

    /// The rating of the player's move `move_number` when it is the latest and no move is being
    /// rated (`CoachPanelState.moveCard` with `latest`).
    pub(super) fn verdict_of(&self, move_number: u32) -> Option<Verdict> {
        let (_, latest) = self.judged.last()?;
        (self.judging.is_none() && latest.move_number == move_number).then_some(latest.verdict)
    }

    /// 提示 at `live`, from the review of the position if the player is to move there and it is
    /// done (`CoachAssist.hint` with `currentReview`).
    pub(super) fn hint(&mut self, live: NodeId, size: crate::record::BoardSize) {
        let review = self.turn_review.as_ref().filter(|_| self.turn == Some(live));
        self.assist.hint(review, size);
    }

    /// Moves were taken back down to `live`, move `number`: what was said about the moves after it
    /// is forgotten (`takenBack`), and a turn after them, whose review may still wait in the
    /// queue, is no longer the player's.
    fn taken_back(&mut self, record: &crate::record::GameRecord, number: u32) {
        let withdrawn = |n: NodeId| !record.contains(n) || record.node(n).move_number > number;
        self.judged.retain(|(_, f)| f.move_number <= number);
        if self.judging.is_some_and(withdrawn) {
            self.judging = None;
        }
        if self.turn.is_some_and(withdrawn) {
            self.turn = None;
            self.turn_review = None;
        }
    }
}

impl Workbench {
    /// It is the player's turn at the game's position: it is reviewed while they think
    /// (`reviewPlayerTurn`, `playerToMove`).
    pub(super) fn human_sl_review_turn(&mut self, now: Instant) {
        if !self.human_sl_game_on() {
            return;
        }
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        if self.doc.record.node(s.live).to_play != s.human {
            return;
        }
        let live = s.live;
        let Some(r) = s.review.as_mut() else { return };
        r.turn = Some(live);
        r.turn_review = None;
        r.tasks.push_back(Task::Turn(live));
        self.human_sl_review_next(now);
    }

    /// The player played from `before` to `after`: the turn's help is gone and the move is rated
    /// (`judgePlayerMove`, `judge`).
    pub(super) fn human_sl_judge(&mut self, before: NodeId, after: NodeId, now: Instant) {
        let record = &self.doc.record;
        let vertex = match record.node(after).kind {
            NodeKind::Move { point, .. } => gtp::vertex(record.size, point),
            _ => "pass".into(),
        };
        let number = record.node(after).move_number;
        let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.review.as_mut()) else {
            return;
        };
        r.assist.clear_turn();
        r.turn = None;
        r.turn_review = None;
        r.judging = Some(after);
        r.tasks.push_back(Task::Judge {
            before,
            after,
            vertex,
            number,
        });
        self.human_sl_review_next(now);
    }

    /// The review of `node` made for its current meaning, if any.
    fn human_sl_cached_review(&self, node: NodeId) -> Option<PositionReview> {
        let r = self.human_sl.session.as_ref()?.review.as_ref()?;
        let (stamp, review) = r.reviews.get(&node)?;
        (self.doc.record.contains(node) && self.doc.record.node(node).stamp == *stamp).then(|| review.clone())
    }

    /// Starts the next review once none runs, after settling the tasks the finished reviews
    /// already answer.
    fn human_sl_review_next(&mut self, now: Instant) {
        loop {
            let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.review.as_mut()) else {
                return;
            };
            if r.running.is_some() {
                return;
            }
            let Some(task) = r.tasks.pop_front() else { return };
            match task.clone() {
                Task::Turn(node) => {
                    // A turn the game has left needs no review any more.
                    if r.turn != Some(node) || !self.doc.record.contains(node) {
                        continue;
                    }
                    match self.human_sl_cached_review(node) {
                        Some(review) => self.human_sl_turn_reviewed(node, Some(review)),
                        None => self.human_sl_review_send(task, node, TURN_VISITS, TURN_SECONDS, now),
                    }
                }
                Task::Judge {
                    before,
                    after,
                    ref vertex,
                    number,
                } => {
                    // A move taken back meanwhile is not rated.
                    if !self.doc.record.contains(before) || !self.human_sl_in_game(after) {
                        self.human_sl_judged(after, None, now);
                        continue;
                    }
                    let Some(first) = self.human_sl_cached_review(before) else {
                        self.human_sl_review_send(task, before, TURN_VISITS, TURN_SECONDS, now);
                        continue;
                    };
                    if !feedback::needs_position_after(&first, vertex) {
                        let rating = feedback::judge(number, vertex, &first, None);
                        self.human_sl_judged(after, rating, now);
                        continue;
                    }
                    match self.human_sl_cached_review(after) {
                        Some(second) => {
                            let rating = feedback::judge(number, vertex, &first, Some(&second));
                            self.human_sl_judged(after, rating, now);
                        }
                        None => self.human_sl_review_send(task, after, AFTER_VISITS, AFTER_SECONDS, now),
                    }
                }
            }
        }
    }

    /// Asks the companion to review `node`; without a companion the review is none.
    fn human_sl_review_send(&mut self, task: Task, node: NodeId, visits: u32, seconds: f64, now: Instant) {
        let Some(worker) = self.human_sl_ensure_worker() else {
            self.human_sl_review_settle(task, node, None, now);
            return;
        };
        let rules = self.quick_rules();
        let Some(s) = self.human_sl.session.as_mut() else { return };
        let Some(r) = s.review.as_mut() else { return };
        r.next += 1;
        let id = format!("humansl-review-{}-{}", s.id, r.next);
        let line = protocol::review_query(
            &id,
            protocol::Position {
                record: &self.doc.record,
                node,
                komi: self.komi,
                rules,
                seconds,
            },
            s.settings.level.play().profile,
            visits,
            seconds,
        );
        r.running = Some(Running {
            id,
            task: task.clone(),
            node,
            stamp: self.doc.record.node(node).stamp,
            deadline: now + TIMEOUT,
        });
        if !self.engines.worker_send(worker, &line) {
            if let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.review.as_mut()) {
                r.running = None;
            }
            self.human_sl_review_settle(task, node, None, now);
        }
    }

    /// `review` of `node` came back for `task` (`None`: the engine could not review it): it is
    /// kept, and the task goes on from it.
    fn human_sl_review_settle(&mut self, task: Task, node: NodeId, review: Option<(u64, PositionReview)>, now: Instant) {
        let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.review.as_mut()) else {
            return;
        };
        if let Some(found) = review.clone() {
            r.reviews.insert(node, found);
        }
        let review = review.map(|(_, review)| review);
        match task {
            Task::Turn(turn) => self.human_sl_turn_reviewed(turn, review),
            Task::Judge {
                before,
                after,
                ref vertex,
                number,
            } => match review {
                // The move is judged from the first review once it is there.
                Some(_) if node == before => r.tasks.push_front(task.clone()),
                None if node == before => self.human_sl_judged(after, None, now),
                second => {
                    let rating = self
                        .human_sl_cached_review(before)
                        .and_then(|first| feedback::judge(number, vertex, &first, second.as_ref()));
                    self.human_sl_judged(after, rating, now);
                }
            },
        }
        self.human_sl_review_next(now);
    }

    /// The review of the player's position `node` arrived: a hint asked for meanwhile is given
    /// (`onReviewChanged`, `reviewArrived`).
    fn human_sl_turn_reviewed(&mut self, node: NodeId, review: Option<PositionReview>) {
        let size = self.doc.record.size;
        let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.review.as_mut()) else {
            return;
        };
        if r.turn != Some(node) {
            return;
        }
        r.turn_review = review;
        let review = r.turn_review.as_ref();
        r.assist.review_arrived(review, size);
    }

    /// Whether `node` is on the game's line up to its position.
    fn human_sl_in_game(&self, node: NodeId) -> bool {
        self.human_sl
            .session
            .as_ref()
            .is_some_and(|s| self.doc.record.contains(s.live) && self.doc.record.path_to(s.live).contains(&node))
    }

    /// Moves were taken back down to the game's position: what was said about them is forgotten,
    /// and a review searching a position taken back is ended (`takenBack`; the take-back ends the
    /// searches it makes useless).
    pub(super) fn human_sl_review_taken_back(&mut self, now: Instant) {
        let Some(s) = self.human_sl.session.as_ref() else { return };
        let number = self.doc.record.node(s.live).move_number;
        let searching_withdrawn = s
            .review
            .as_ref()
            .and_then(|r| r.running.as_ref())
            .is_some_and(|run| !self.human_sl_in_game(run.node));
        let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.review.as_mut()) else {
            return;
        };
        r.taken_back(&self.doc.record, number);
        if searching_withdrawn {
            self.human_sl_review_lost(now, true);
        }
    }

    /// The rating of the move that led to `after` (`MoveReviewer.record`): kept, replacing those
    /// of this and later moves, while the move is still in the game; a reply waiting for it may go
    /// on.
    fn human_sl_judged(&mut self, after: NodeId, rating: Option<MoveFeedback>, now: Instant) {
        let in_game = self.human_sl_in_game(after);
        let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.review.as_mut()) else {
            return;
        };
        if r.judging == Some(after) {
            r.judging = None;
        }
        if let Some(rating) = rating.filter(|_| in_game) {
            r.judged.retain(|(_, f)| f.move_number < rating.move_number);
            r.judged.push((after, rating));
        }
        self.human_sl_verdict_arrived(now);
    }

    /// A companion answer for the running review; false when `response` is not one. An answer
    /// read past the review's 15 s comes too late: the review counts as none, as when its time
    /// runs out first.
    pub(super) fn human_sl_review_line(&mut self, response: &Value, now: Instant) -> bool {
        let Some(r) = self.human_sl.session.as_ref().and_then(|s| s.review.as_ref()) else {
            return false;
        };
        let Some(running) = r.running.as_ref().filter(|run| response["id"].as_str() == Some(run.id.as_str())) else {
            return false;
        };
        if response["isDuringSearch"].as_bool() == Some(true) {
            return true;
        }
        let (node, stamp) = (running.node, running.stamp);
        let current = now < running.deadline && self.doc.record.contains(node) && self.doc.record.node(node).stamp == stamp;
        let review = (current && response.get("error").is_none())
            .then(|| protocol::review(response, &self.doc.record, node))
            .map(|review| (stamp, review));
        let running = self.human_sl.session.as_mut().and_then(|s| s.review.as_mut()).and_then(|r| r.running.take());
        if let Some(running) = running {
            self.human_sl_review_settle(running.task, node, review, now);
        }
        true
    }

    /// The running review gets no answer: the companion was stopped or exited, or the review ran
    /// out of time or searches a position taken back (then it is ended).
    pub(super) fn human_sl_review_lost(&mut self, now: Instant, terminate: bool) {
        let Some(s) = self.human_sl.session.as_mut() else { return };
        let worker = s.worker;
        let Some(running) = s.review.as_mut().and_then(|r| r.running.take()) else {
            return;
        };
        if let Some(worker) = worker.filter(|_| terminate) {
            let _ = self.engines.worker_send(
                worker,
                &json!({"id":format!("cancel-{}",running.id),"action":"terminate","terminateId":running.id}).to_string(),
            );
        }
        self.human_sl_review_settle(running.task, running.node, None, now);
    }

    /// When the running review times out.
    pub(super) fn human_sl_review_deadline(&self) -> Option<Instant> {
        self.human_sl.session.as_ref()?.review.as_ref()?.running.as_ref().map(|r| r.deadline)
    }

    /// 边下边提醒's projection of the session.
    pub(super) fn human_sl_feedback_view(&self, s: &Session) -> Option<HumanSlFeedbackView> {
        let r = s.review.as_ref()?;
        let ratings: Vec<MoveFeedback> = r.ratings().cloned().collect();
        let a = &r.assist;
        Some(HumanSlFeedbackView {
            judging: r.judging.is_some(),
            counts: feedback::counts(&ratings),
            key_mistakes: feedback::key_mistakes(&ratings),
            worst: feedback::worst_mistakes(&ratings, feedback::WORST_SHOWN),
            ratings,
            hints: a.hints_used(),
            can_hint: self.human_sl_can_hint(),
            hint: a.step().map(|step| HumanSlHintView {
                step,
                area: a.hint_area(),
                vertex: a.hint_move().map(str::to_string),
                share: a.hint_share(),
            }),
            decision: a.decision().map(|rating| HumanSlDecisionView {
                rating: rating.clone(),
                best_shown: a.best_shown(),
            }),
            // The marks belong to the game's position: a board browsing elsewhere shows none.
            marks: if s.doc == self.doc.id && self.cursor == s.live { a.marks().clone() } else { CoachMarks::default() },
        })
    }
}
