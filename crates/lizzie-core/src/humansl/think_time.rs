//! AI 思考时间: how long a player of the opponent's level would take over a move (Java
//! `training/HumanThinkTime`). Quick when nearly everyone at that level plays the same move,
//! longer when the choice is open, a little quicker in the opening. It only paces the reply: the
//! move is chosen, with the same search, before this applies.
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;

/// The AI 思考时间 setting. Java's `NATURAL` / `FAST` / `SLOW` read as aliases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Pace {
    /// 自然: quick for obvious replies, longer where the choice is open.
    #[default]
    #[serde(alias = "NATURAL")]
    Natural,
    /// 快: about a second a move.
    #[serde(alias = "FAST")]
    Fast,
    /// 慢: a few seconds more, like a serious game.
    #[serde(alias = "SLOW")]
    Slow,
}

impl Pace {
    /// The pace's factor on the natural time, and its bounds in milliseconds.
    fn scale(self) -> (f64, u64, u64) {
        match self {
            Pace::Natural => (1.0, 400, 8_000),
            Pace::Fast => (0.4, 250, 1_800),
            Pace::Slow => (2.0, 800, 14_000),
        }
    }
}

const MIN_MILLIS: f64 = 400.0;
const MAX_MILLIS: f64 = 8_000.0;
const OPENING_BASE_MILLIS: f64 = 1_000.0;
const BASE_MILLIS: f64 = 1_600.0;
const OPENING_SHARE_OF_BOARD: f64 = 0.1;
const UNKNOWN_TOP_PROBABILITY: f64 = 0.45;

/// The earliest the AI's reply may be played after its request started.
///
/// `top_human_probability` is the human policy's share on its most popular move (`None` or not
/// finite when unknown); `random` is uniform in [0, 1), so equal positions do not always take
/// equally long.
pub fn think_time(
    top_human_probability: Option<f64>,
    move_number: u32,
    board_area: usize,
    random: f64,
    pace: Pace,
) -> Duration {
    let top = top_human_probability
        .filter(|p| p.is_finite())
        .map_or(UNKNOWN_TOP_PROBABILITY, |p| p.clamp(0.0, 1.0));
    let difficulty = 0.6 + 2.2 * (1.0 - top).powf(1.5);
    let jitter = 0.75 + 0.6 * random.clamp(0.0, 1.0);
    let base = if f64::from(move_number) < board_area as f64 * OPENING_SHARE_OF_BOARD {
        OPENING_BASE_MILLIS
    } else {
        BASE_MILLIS
    };
    // Java's Math.round: half up.
    let natural = (base * difficulty * jitter + 0.5).floor().clamp(MIN_MILLIS, MAX_MILLIS);
    let (factor, min, max) = pace.scale();
    let paced = ((natural * factor + 0.5).floor() as u64).clamp(min, max);
    Duration::from_millis(paced)
}
