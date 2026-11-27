//! The game split into opening, middle game and endgame at two boundaries, each side's moves of
//! each phase estimated on their own by the model of the whole report (Java `StrengthPhases`,
//! `StrengthPhaseCard.verdict` and `StrengthPhaseView.review`).
use super::*;
use serde_json::Value;

/// The last move of the opening and of the middle game, unless the user moves them.
pub const DEFAULT_PHASE_BOUNDS: [u32; 2] = [60, 150];
/// The fewest moves a phase can be made to span.
pub const MIN_PHASE_MOVES: u32 = 10;
/// The latest move a boundary can be, far past any game (the dividers stop there too).
pub const MAX_PHASE_BOUND: u32 = 1000;
/// The fewest analysed moves of a side that a phase estimate is shown for.
const MIN_PHASE_SAMPLES: u32 = 6;
/// Rank differences below this are even; from [`CLEAR`] on one side is clearly better.
const EVEN: f64 = 0.3;
const CLEAR: f64 = 1.0;
/// The human-style model reads the first 210 moves of a game.
const HUMAN_STYLE_MOVES: u32 = 210;

/// Stored boundaries: two JSON integers, the opening at least [`MIN_PHASE_MOVES`] long and the
/// middle game too, ending by move [`MAX_PHASE_BOUND`]. Anything else (not two integers, reversed
/// or equal, a phase too short, a boundary past the limit) is `None`, and the caller uses
/// [`DEFAULT_PHASE_BOUNDS`].
pub fn parse_phase_bounds(value: &Value) -> Option<[u32; 2]> {
    let [opening, middle] = value.as_array()?.as_slice() else {
        return None;
    };
    let bounds = [
        u32::try_from(opening.as_u64()?).ok()?,
        u32::try_from(middle.as_u64()?).ok()?,
    ];
    usable_phase_bounds(bounds).then_some(bounds)
}

/// Whether `bounds` leave the opening and the middle game at least [`MIN_PHASE_MOVES`] long and
/// end the middle game by move [`MAX_PHASE_BOUND`].
pub fn usable_phase_bounds(bounds: [u32; 2]) -> bool {
    bounds[0] >= MIN_PHASE_MOVES
        && bounds[1] <= MAX_PHASE_BOUND
        && bounds[0] <= bounds[1].saturating_sub(MIN_PHASE_MOVES)
}

/// What the phases view shows for one report at one pair of boundaries.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthPhases {
    /// The boundaries used: the last move of the opening and of the middle game.
    pub bounds: [u32; 2],
    pub default_bounds: [u32; 2],
    pub min_phase_moves: u32,
    pub max_bound: u32,
    /// The last move of the game: the main line's length, or the last analysed move if later.
    pub last_move: u32,
    /// Opening, middle game and endgame.
    pub phases: Vec<StrengthPhase>,
    pub review: PhaseReview,
}

/// One phase: its moves `first` to `last` (`last < first`: the game ended before it), each
/// side's estimate over its own moves in it and who played it better.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthPhase {
    pub first: u32,
    pub last: u32,
    pub black: PhaseSide,
    pub white: PhaseSide,
    pub verdict: PhaseVerdict,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct PhaseSide {
    pub summary: StrengthSummary,
    /// Enough analysed moves (six) for the rank to be shown.
    pub ranked: bool,
    /// This is the side's best or weakest phase.
    pub badge: Option<PhaseBadge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum PhaseBadge {
    Best,
    Weakest,
}

/// The line under a phase's ruler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum PhaseVerdict {
    /// The game ended before this phase.
    NotReached,
    /// The phase starts after the moves the human-style model reads.
    HumanStyleLimit {
        moves: u32,
    },
    /// A side has too few analysed moves in it to be ranked.
    TooFew,
    /// The ranks differ by less than 0.3.
    Even,
    /// By less than 1.0; `black`: Black played better.
    Slightly {
        black: bool,
    },
    Clearly {
        black: bool,
    },
}

/// The phase each side may want to look at again in review.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum PhaseReview {
    /// A side has fewer than two ranked phases: nothing to compare.
    None,
    /// Neither side has a weakest phase clearly below its best one.
    Steady,
    /// One side's weakest phase (an index into the phases).
    One { black: bool, phase: u32 },
    /// Both sides', different phases.
    Both { black: u32, white: u32 },
    /// Both sides' weakest phase is the same one.
    Same { phase: u32 },
}

/// The phases of `report`'s game, estimated by `model` (the model of `report`), split at `bounds`
/// (unusable ones give the defaults); `total_moves` is the main line's length.
pub fn phases(report: &StrengthReport, model: &StrengthModel, bounds: [u32; 2], total_moves: u32) -> StrengthPhases {
    let bounds = if usable_phase_bounds(bounds) {
        bounds
    } else {
        DEFAULT_PHASE_BOUNDS
    };
    let last_move = report
        .black
        .samples
        .iter()
        .chain(&report.white.samples)
        .map(|s| s.move_number)
        .fold(total_moves, u32::max);
    let estimate = |side: &StrengthSideReport, first: u32, last: u32| {
        let part: Vec<StrengthSample> = side
            .samples
            .iter()
            .filter(|s| (first..=last).contains(&s.move_number))
            .cloned()
            .collect();
        estimator::summary(&part, model)
    };
    let spans = [
        (1, bounds[0].min(last_move)),
        (bounds[0] + 1, bounds[1].min(last_move)),
        (bounds[1] + 1, last_move),
    ]
    .map(|(first, last)| PhaseSpan {
        first,
        last,
        black: estimate(&report.black, first, last),
        white: estimate(&report.white, first, last),
    });
    judge_phases(bounds, last_move, model.human_sl, spans)
}

/// A phase's moves and each side's estimate over them, before they are compared.
#[derive(Clone, Debug, PartialEq)]
struct PhaseSpan {
    first: u32,
    last: u32,
    black: StrengthSummary,
    white: StrengthSummary,
}

/// Compares the estimated `spans` (opening, middle game, endgame): each phase's verdict, each
/// side's best and weakest phase and what is worth a review. `human_sl`: they come from the
/// human-style model.
fn judge_phases(bounds: [u32; 2], last_move: u32, human_sl: bool, spans: [PhaseSpan; 3]) -> StrengthPhases {
    let ranked = |s: &StrengthSummary| s.sample_count >= MIN_PHASE_SAMPLES && s.rank_value.is_some_and(f64::is_finite);
    let rank = |s: &StrengthSummary| s.rank_value.unwrap_or(f64::NAN);
    let mut phases: Vec<StrengthPhase> = spans
        .into_iter()
        .map(|span| {
            let lead = (ranked(&span.black) && ranked(&span.white)).then(|| rank(&span.black) - rank(&span.white));
            let verdict = if span.last < span.first {
                PhaseVerdict::NotReached
            } else if human_sl && span.first > HUMAN_STYLE_MOVES {
                PhaseVerdict::HumanStyleLimit {
                    moves: HUMAN_STYLE_MOVES,
                }
            } else {
                match lead {
                    None => PhaseVerdict::TooFew,
                    Some(lead) if lead.abs() < EVEN => PhaseVerdict::Even,
                    Some(lead) if lead.abs() < CLEAR => PhaseVerdict::Slightly { black: lead > 0.0 },
                    Some(lead) => PhaseVerdict::Clearly { black: lead > 0.0 },
                }
            };
            let side = |summary: StrengthSummary| PhaseSide {
                ranked: ranked(&summary),
                summary,
                badge: None,
            };
            StrengthPhase {
                first: span.first,
                last: span.last,
                black: side(span.black),
                white: side(span.white),
                verdict,
            }
        })
        .collect();

    // StrengthPhases.extreme: the phase in which a side played best or worst, unless fewer than two
    // of its phases are ranked or they lie within EVEN of each other.
    let extreme = |phases: &[StrengthPhase], black: bool, best: bool| -> Option<usize> {
        let ranked: Vec<(usize, f64)> = phases
            .iter()
            .enumerate()
            .filter(|(_, p)| side(p, black).ranked)
            .map(|(i, p)| (i, rank(&side(p, black).summary)))
            .collect();
        let highest = ranked.iter().map(|r| r.1).fold(f64::NEG_INFINITY, f64::max);
        let lowest = ranked.iter().map(|r| r.1).fold(f64::INFINITY, f64::min);
        if ranked.len() < 2 || highest - lowest < EVEN {
            return None;
        }
        // The first phase reaching the extreme.
        ranked
            .iter()
            .find(|r| r.1 == if best { highest } else { lowest })
            .map(|r| r.0)
    };
    let mut review = Vec::new();
    let mut comparable = true;
    for black in [true, false] {
        comparable &= phases.iter().filter(|p| side(p, black).ranked).count() >= 2;
        let best = extreme(&phases, black, true);
        let worst = extreme(&phases, black, false);
        if let Some(b) = best {
            badge(&mut phases[b], black, PhaseBadge::Best);
        }
        if let Some(w) = worst.filter(|w| Some(*w) != best) {
            badge(&mut phases[w], black, PhaseBadge::Weakest);
        }
        // worthReviewing: the weakest phase at least twice EVEN below the best one.
        if let (Some(b), Some(w)) = (best, worst) {
            if rank(&side(&phases[b], black).summary) - rank(&side(&phases[w], black).summary) >= 2.0 * EVEN {
                review.push((black, w as u32));
            }
        }
    }
    let review = match review.as_slice() {
        [] if comparable => PhaseReview::Steady,
        [] => PhaseReview::None,
        [(black, phase)] => PhaseReview::One {
            black: *black,
            phase: *phase,
        },
        [(_, b), (_, w)] if b == w => PhaseReview::Same { phase: *b },
        [(_, b), (_, w), ..] => PhaseReview::Both { black: *b, white: *w },
    };
    StrengthPhases {
        bounds,
        default_bounds: DEFAULT_PHASE_BOUNDS,
        min_phase_moves: MIN_PHASE_MOVES,
        max_bound: MAX_PHASE_BOUND,
        last_move,
        phases,
        review,
    }
}

fn badge(phase: &mut StrengthPhase, black: bool, badge: PhaseBadge) {
    if black {
        phase.black.badge = Some(badge);
    } else {
        phase.white.badge = Some(badge);
    }
}

fn side(phase: &StrengthPhase, black: bool) -> &PhaseSide {
    if black {
        &phase.black
    } else {
        &phase.white
    }
}
