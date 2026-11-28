//! 边下边提醒's rating of the player's moves (Java `training/RankAwareWinrate`, `MoveVerdict`,
//! `MoveFeedback`, `PositionReview`): KataGo's score lead measured against how much games of the
//! opponent's rank still swing, the win chance it gives, and the rating of a move by how much of
//! that chance it lost.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The spread used when the engine reports none: about the middle game of amateur dan players
/// (`RankAwareWinrate.FALLBACK_SCORE_STDEV`).
pub const FALLBACK_SCORE_STDEV: f64 = 12.0;
/// Below this the spread is noise (`RankAwareWinrate.MIN_SCORE_STDEV`).
pub const MIN_SCORE_STDEV: f64 = 1.0;
/// Win chance lost by a 缓手, 问题手 and 恶手 (`MoveVerdict.SLOW_LOSS` …).
pub const SLOW_LOSS: f64 = 0.05;
pub const PROBLEM_LOSS: f64 = 0.10;
pub const BLUNDER_LOSS: f64 = 0.20;
/// Below this win chance or above its complement the game is decided for players of the rank, and
/// points rate the move too (`MoveVerdict.DECIDED`).
pub const DECIDED: f64 = 0.05;
/// Points that make a 问题手 and a 恶手 once the game is decided (`PROBLEM_POINTS`, `BLUNDER_POINTS`).
pub const PROBLEM_POINTS: f64 = 10.0;
pub const BLUNDER_POINTS: f64 = 20.0;
/// Below this share of players, finding the best move counts as rare (`MoveFeedback.RARE_SHARE`).
pub const RARE_SHARE: f64 = 0.10;
/// Fewer visits than this give a move's value too much noise to judge it by
/// (`PositionReview.MIN_RELIABLE_VISITS`).
const MIN_RELIABLE_VISITS: u32 = 16;

/// The chance that a player of the opponent's rank who leads by `lead` points goes on to win, from
/// 0 to 1, for the HumanSL score standard deviation `stdev` of the position; NaN without a lead
/// (`RankAwareWinrate.of`).
pub fn win_chance(lead: f64, stdev: Option<f64>) -> f64 {
    if !lead.is_finite() {
        return f64::NAN;
    }
    normal_cdf(lead / spread(stdev))
}

fn spread(stdev: Option<f64>) -> f64 {
    match stdev {
        Some(s) if s.is_finite() && s > 0.0 => s.max(MIN_SCORE_STDEV),
        _ => FALLBACK_SCORE_STDEV,
    }
}

/// The standard normal distribution function, accurate to about 1e-7 (Abramowitz–Stegun 7.1.26).
fn normal_cdf(x: f64) -> f64 {
    let z = x.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * z);
    let erf = 1.0
        - ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t + 0.254829592)
            * t
            * (-z * z).exp();
    if x >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

/// How 边下边提醒 rates one of the player's moves, best first (`MoveVerdict`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum Verdict {
    /// 妙手: KataGo's choice, which few players of the rank find, where their usual move loses.
    Brilliant,
    Good,
    /// 缓手: the win chance dropped by 5% or more.
    Slow,
    /// 问题手: by 10% or more.
    Problem,
    /// 恶手: by 20% or more.
    Blunder,
}

impl Verdict {
    /// The rating of a move that took the player's win chance from `win_before` (with the best
    /// move) to `win_after` and gave away `points_lost` against the best move (`MoveVerdict.of`).
    /// The drop is rated by the whole percentage shown, so a "−5%" is never a good move; in a
    /// decided game many points lost still make a mistake.
    pub fn of(win_before: f64, win_after: f64, points_lost: f64) -> Verdict {
        // Java's Math.round: half up.
        let loss = ((win_before - win_after).max(0.0) * 100.0 + 0.5).floor() / 100.0;
        let by_win = if loss >= BLUNDER_LOSS {
            Verdict::Blunder
        } else if loss >= PROBLEM_LOSS {
            Verdict::Problem
        } else if loss >= SLOW_LOSS {
            Verdict::Slow
        } else {
            Verdict::Good
        };
        if win_before < DECIDED || win_before > 1.0 - DECIDED {
            let by_points = if points_lost >= BLUNDER_POINTS {
                Verdict::Blunder
            } else if points_lost >= PROBLEM_POINTS {
                Verdict::Problem
            } else {
                Verdict::Good
            };
            return by_win.max(by_points);
        }
        by_win
    }

    /// 缓手, 问题手 and 恶手 (`MoveVerdict.isMistake`).
    pub fn is_mistake(self) -> bool {
        matches!(self, Verdict::Slow | Verdict::Problem | Verdict::Blunder)
    }
}

/// A move KataGo searched, with its score lead for the player to move.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewCandidate {
    /// The move as KataGo names it, such as `Q16` or `pass`.
    pub vertex: String,
    pub visits: u32,
    pub lead: f64,
}

/// What the coach engine found in one position: KataGo's values for the moves it searched and how
/// often players of the opponent's rank play each move. Leads are in points for the player to
/// move (`PositionReview`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PositionReview {
    /// KataGo's score lead for the player to move; NaN when not reported.
    pub root_lead: f64,
    /// The HumanSL model's score standard deviation, when the engine reported one.
    pub score_stdev: Option<f64>,
    /// The searched moves, best first.
    pub candidates: Vec<ReviewCandidate>,
    /// The HumanSL policy: move name (`Q16`, `pass`) to probability.
    pub human_shares: Vec<(String, f64)>,
}

/// A move's name as reviews compare it: `pass`, or the vertex in capitals.
pub fn normalize(vertex: &str) -> String {
    let t = vertex.trim();
    if t.eq_ignore_ascii_case("pass") {
        "pass".into()
    } else {
        t.to_ascii_uppercase()
    }
}

impl PositionReview {
    /// KataGo's choice: the first candidate.
    pub fn best(&self) -> Option<&ReviewCandidate> {
        self.candidates.first()
    }

    /// The lead after `vertex` from this search, or NaN when the search did not look at it closely
    /// enough, so that the position after it has to be searched instead (`leadAfter`).
    pub fn lead_after(&self, vertex: &str) -> f64 {
        let name = normalize(vertex);
        let Some(best) = self.best() else {
            return f64::NAN;
        };
        if normalize(&best.vertex) == name {
            return best.lead;
        }
        let needed = MIN_RELIABLE_VISITS.max(best.visits / 20);
        self.candidates
            .iter()
            .find(|c| normalize(&c.vertex) == name)
            .map_or(f64::NAN, |c| if c.visits >= needed { c.lead } else { f64::NAN })
    }

    /// How often players of the rank play `vertex`, from 0 to 1; unknown without a policy
    /// (`humanShare`).
    pub fn human_share(&self, vertex: &str) -> Option<f64> {
        if self.human_shares.is_empty() {
            return None;
        }
        let name = normalize(vertex);
        Some(self.human_shares.iter().find(|(m, _)| *m == name).map_or(0.0, |(_, p)| *p))
    }

    /// The move players of the rank play most often here, the first by name among equals
    /// (`commonMove`).
    pub fn common_move(&self) -> Option<&str> {
        let mut common: Option<(&str, f64)> = None;
        for (name, share) in &self.human_shares {
            if common.is_none_or(|(c, high)| *share > high || (*share == high && name.as_str() < c)) {
                common = Some((name, *share));
            }
        }
        common.map(|(name, _)| name)
    }
}

/// 边下边提醒's judgement of one of the player's moves (`MoveFeedback`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MoveFeedback {
    /// The move's number in the game.
    pub move_number: u32,
    /// The move, such as `Q16`, or `pass`.
    pub vertex: String,
    pub verdict: Verdict,
    /// The player's rank-aware win chance with KataGo's best move, 0 to 1.
    pub win_before: f64,
    /// The chance after the move played.
    pub win_after: f64,
    /// Points given away against the best move, never negative.
    pub points_lost: f64,
    /// How often players of the rank play this move, when the policy is known.
    pub human_share: Option<f64>,
    /// Whether the move is KataGo's choice.
    pub played_best: bool,
    /// KataGo's choice in the position before the move.
    pub best_move: String,
    /// How often players of the rank play `best_move`.
    pub best_share: Option<f64>,
}

impl MoveFeedback {
    /// The change of the win chance, negative for a loss (`winChange`).
    pub fn win_change(&self) -> f64 {
        self.win_after - self.win_before
    }
}

/// Whether the position after `vertex` must be searched to judge it, because the search before
/// the move did not look at it closely (`needsPositionAfter`).
pub fn needs_position_after(before: &PositionReview, vertex: &str) -> bool {
    before.lead_after(vertex).is_nan()
}

/// How many places the summary lists as worth reviewing (`CoachPanelState.KEY_MISTAKES`).
pub const WORST_SHOWN: usize = 3;

/// How many of `ratings` got each rating, in [`Verdict`] order: 妙手, 好棋, 缓手, 问题手, 恶手
/// (`MoveReviewer.count`).
pub fn counts<'a>(ratings: impl IntoIterator<Item = &'a MoveFeedback>) -> [u32; 5] {
    let mut counts = [0; 5];
    for r in ratings {
        counts[r.verdict as usize] += 1;
    }
    counts
}

/// The game's key mistakes: its 恶手, a 问题手 not counting (`MoveReviewer.keyMistakes`).
pub fn key_mistakes<'a>(ratings: impl IntoIterator<Item = &'a MoveFeedback>) -> u32 {
    blunders(&counts(ratings))
}

/// The 恶手 among [`counts`].
fn blunders(counts: &[u32; 5]) -> u32 {
    counts[Verdict::Blunder as usize]
}

/// What the summary says of a game's ratings: how many got each rating ([`counts`]), its key
/// mistakes ([`key_mistakes`]) and the [`WORST_SHOWN`] worst mistakes ([`worst_mistakes`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RatingSummary {
    pub counts: [u32; 5],
    pub key_mistakes: u32,
    pub worst: Vec<MoveFeedback>,
}

impl RatingSummary {
    pub fn of<'a>(ratings: impl Iterator<Item = &'a MoveFeedback> + Clone) -> RatingSummary {
        let counts = counts(ratings.clone());
        RatingSummary {
            counts,
            key_mistakes: blunders(&counts),
            worst: worst_mistakes(ratings, WORST_SHOWN),
        }
    }
}

/// The `n` worst of the player's mistakes, in the order played (`MoveReviewer.worstMistakes`): the
/// most severe rating first, among equal ratings the largest drop of the win chance, then the
/// earlier move.
pub fn worst_mistakes<'a>(ratings: impl IntoIterator<Item = &'a MoveFeedback>, n: usize) -> Vec<MoveFeedback> {
    let mut worst: Vec<&MoveFeedback> = ratings.into_iter().filter(|r| r.verdict.is_mistake()).collect();
    // A stable sort keeps the order played among equals.
    worst.sort_by(|a, b| b.verdict.cmp(&a.verdict).then(a.win_change().total_cmp(&b.win_change())));
    worst.truncate(n);
    worst.sort_by_key(|r| r.move_number);
    worst.into_iter().cloned().collect()
}

/// The player's rank-aware win chance through the game in percent, oldest first: before and after
/// each rated move, then in the position the player is to move in once `turn`, its review, gives
/// KataGo's choice (`MoveReviewer.winChances`).
pub fn win_chances(ratings: &[MoveFeedback], turn: Option<&PositionReview>) -> Vec<f64> {
    let mut chances: Vec<f64> = ratings.iter().flat_map(|r| [r.win_before * 100.0, r.win_after * 100.0]).collect();
    if let Some(review) = turn {
        if let Some(best) = review.best() {
            chances.push(win_chance(best.lead, review.score_stdev) * 100.0);
        }
    }
    chances
}

/// Judges `vertex`, move `move_number`, played where `before` was reviewed; `after` is the review
/// of the position after it, from the opponent's side, needed only when [`needs_position_after`]
/// says so. `None` when the move cannot be rated (`MoveFeedback.judge`).
pub fn judge(move_number: u32, vertex: &str, before: &PositionReview, after: Option<&PositionReview>) -> Option<MoveFeedback> {
    let best = before.best()?;
    let best_lead = best.lead;
    let mut lead = before.lead_after(vertex);
    if lead.is_nan() {
        if let Some(after) = after {
            lead = -after.root_lead;
        }
    }
    if !lead.is_finite() || !best_lead.is_finite() {
        return None;
    }
    let stdev = match (before.score_stdev, after) {
        (Some(s), _) if s.is_finite() => Some(s),
        (_, Some(after)) => after.score_stdev,
        (s, None) => s,
    };
    let win_before = win_chance(best_lead, stdev);
    let win_after = win_chance(lead, stdev);
    let points_lost = (best_lead - lead).max(0.0);
    let played_best = normalize(&best.vertex) == normalize(vertex);
    let share = before.human_share(vertex);
    let mut verdict = Verdict::of(win_before, win_after, points_lost);
    if verdict == Verdict::Good && played_best && rare_find(before, vertex, share, win_before) {
        verdict = Verdict::Brilliant;
    }
    Some(MoveFeedback {
        move_number,
        vertex: if normalize(vertex) == "pass" { "pass".into() } else { vertex.trim().into() },
        verdict,
        win_before,
        win_after,
        points_lost,
        human_share: share,
        played_best,
        best_move: best.vertex.clone(),
        best_share: before.human_share(&best.vertex),
    })
}

/// Few players of the rank play the move, and the one they usually play loses at least a 缓手's
/// worth of win chance, so finding it mattered (`isRareFind`).
fn rare_find(before: &PositionReview, vertex: &str, share: Option<f64>, win_before: f64) -> bool {
    if !share.is_some_and(|s| s < RARE_SHARE) {
        return false;
    }
    let Some(common) = before.common_move() else {
        return false;
    };
    if common == normalize(vertex) {
        return false;
    }
    let common_lead = before.lead_after(common);
    if common_lead.is_nan() {
        return false;
    }
    win_before - win_chance(common_lead, before.score_stdev) >= SLOW_LOSS
}

/// One of nine areas of the board: the corners, the sides and the centre, numbered row by row
/// from the top left, 0 to 8 (`CoachBoardMarks.areaOf`).
pub fn area_of(x: u16, y: u16, width: u16, height: u16) -> u8 {
    third(y, height) * 3 + third(x, width)
}

/// The inclusive `[x0, y0, x1, y1]` of area `index` (`CoachBoardMarks.areaBounds`).
pub fn area_bounds(index: u8, width: u16, height: u16) -> [u16; 4] {
    let column = index % 3;
    let row = index / 3;
    [
        edge(column, width),
        edge(row, height),
        edge(column + 1, width) - 1,
        edge(row + 1, height) - 1,
    ]
}

fn third(value: u16, size: u16) -> u8 {
    if value < edge(1, size) {
        0
    } else if value < edge(2, size) {
        1
    } else {
        2
    }
}

/// Where third `index` of a side starts: 0, 6, 13 and 19 on a 19×19 board.
fn edge(index: u8, size: u16) -> u16 {
    // Java's Math.round: half up.
    (f64::from(size) * f64::from(index) / 3.0 + 0.5).floor() as u16
}
