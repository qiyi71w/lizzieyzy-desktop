//! 边下边提醒's help within one turn, with its marks on the main board (Java `gui/CoachAssist`,
//! `CoachBoardMarks`): after a 恶手 the AI holds its reply while the player decides, and on the
//! player's turn a hint shows the area of KataGo's choice first and the point second.
use super::feedback::{area_bounds, area_of, MoveFeedback, PositionReview};
use crate::engine::gtp;
use crate::record::{BoardSize, Point};
use serde::Serialize;
use ts_rs::TS;

/// How far the hint of the turn went (`CoachAssist.Hint`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum HintStep {
    /// Asked for before the position's review was ready; given once it is.
    Waiting,
    /// The area of KataGo's choice is marked.
    Area,
    /// The point is marked (or KataGo would pass, which has no point).
    Point,
}

/// What the coach marks on the main board (`CoachBoardMarks`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct CoachMarks {
    /// One of the nine areas, as inclusive `[x0, y0, x1, y1]`.
    pub area: Option<[u16; 4]>,
    pub point: Option<Point>,
}

#[derive(Default)]
pub struct Assist {
    decision: Option<MoveFeedback>,
    best_shown: bool,
    hint: Option<HintStep>,
    hint_move: Option<String>,
    hint_share: Option<f64>,
    hint_area: Option<u8>,
    hints_used: u32,
    marks: CoachMarks,
}

impl Assist {
    /// The AI holds its reply to the 恶手 `feedback` until the player decides (`decideAbout`).
    pub fn decide_about(&mut self, feedback: MoveFeedback) {
        self.clear_turn();
        self.decision = Some(feedback);
    }

    /// The 恶手 the player is deciding about.
    pub fn decision(&self) -> Option<&MoveFeedback> {
        self.decision.as_ref()
    }

    pub fn best_shown(&self) -> bool {
        self.best_shown
    }

    /// 看 AI 推荐: marks KataGo's choice in the position before the 恶手 (`showBest`).
    pub fn show_best(&mut self, size: BoardSize) {
        let Some(point) = self.decision.as_ref().and_then(|d| point(size, &d.best_move)) else {
            return;
        };
        self.marks = CoachMarks { area: None, point: Some(point) };
        self.best_shown = true;
    }

    /// 提示 / H on the player's turn: the area of KataGo's choice first, then its point; asked
    /// again after the point, nothing changes. `review` is the review of the position the player
    /// is to move in, `None` while it runs (`CoachAssist.hint`).
    pub fn hint(&mut self, review: Option<&PositionReview>, size: BoardSize) {
        let Some(best) = review.and_then(PositionReview::best) else {
            if self.hint.is_none() {
                self.hint = Some(HintStep::Waiting);
            }
            return;
        };
        let review = review.unwrap();
        let Some(point) = point(size, &best.vertex) else {
            // KataGo would pass here: there is no area to show.
            if self.hint != Some(HintStep::Point) {
                self.hints_used += 1;
            }
            self.hint_move = Some(best.vertex.clone());
            self.hint_share = review.human_share(&best.vertex);
            self.hint = Some(HintStep::Point);
            self.marks = CoachMarks::default();
            return;
        };
        match self.hint {
            None | Some(HintStep::Waiting) => {
                let area = area_of(point.x, point.y, size.width, size.height);
                self.hint_move = Some(best.vertex.clone());
                self.hint_share = review.human_share(&best.vertex);
                self.hint_area = Some(area);
                self.marks = CoachMarks { area: Some(area_bounds(area, size.width, size.height)), point: None };
                self.hint = Some(HintStep::Area);
                self.hints_used += 1;
            }
            Some(HintStep::Area) => {
                self.marks = CoachMarks { area: None, point: Some(point) };
                self.hint = Some(HintStep::Point);
            }
            Some(HintStep::Point) => {}
        }
    }

    /// The position's review arrived: a hint asked for meanwhile is given now (`reviewArrived`).
    pub fn review_arrived(&mut self, review: Option<&PositionReview>, size: BoardSize) {
        if self.hint == Some(HintStep::Waiting) && review.is_some() {
            self.hint = None;
            self.hint(review, size);
        }
    }

    pub fn step(&self) -> Option<HintStep> {
        self.hint
    }

    /// KataGo's choice the hint is about, `pass` included.
    pub fn hint_move(&self) -> Option<&str> {
        self.hint_move.as_deref()
    }

    pub fn hint_share(&self) -> Option<f64> {
        self.hint_share
    }

    /// The hint's area, 0 to 8 as [`area_of`].
    pub fn hint_area(&self) -> Option<u8> {
        self.hint_area
    }

    /// Positions in this game where the player asked for a hint (`hintsUsed`).
    pub fn hints_used(&self) -> u32 {
        self.hints_used
    }

    pub fn marks(&self) -> &CoachMarks {
        &self.marks
    }

    /// The player moved, took back or the game ended: the turn's help and marks are gone
    /// (`clearTurn`).
    pub fn clear_turn(&mut self) {
        self.decision = None;
        self.best_shown = false;
        self.hint = None;
        self.hint_move = None;
        self.hint_share = None;
        self.hint_area = None;
        self.marks = CoachMarks::default();
    }
}

fn point(size: BoardSize, vertex: &str) -> Option<Point> {
    gtp::parse_vertex(size, vertex).ok().flatten()
}
