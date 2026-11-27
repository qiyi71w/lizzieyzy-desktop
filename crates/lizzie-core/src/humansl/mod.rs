//! AI 陪练's opponent: its levels, how a level picks a move from KataGo's HumanSL answer, how
//! long the AI appears to think, when it resigns, what a take-back withdraws, how 边下边提醒 rates
//! the player's moves and helps within a turn, and the analysis protocol. No application or
//! process state.
pub mod assist;
pub mod choose;
pub mod feedback;
mod level;
pub mod passing;
pub mod protocol;
mod resign;
pub(crate) mod take_back;
mod think_time;

pub use level::{CoachGrade, CoachLevel, CoachLevelKind, PlaySettings};
pub use resign::Resignation;
pub use think_time::{think_time, Pace};

/// The komi of a handicap game, where the player takes Black and receives the stones
/// (`CoachSetupForm.HANDICAP_KOMI`).
pub const HANDICAP_KOMI: f64 = 0.5;

/// A legal move with its HumanSL policy odds.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub vertex: String,
    pub probability: f64,
}
