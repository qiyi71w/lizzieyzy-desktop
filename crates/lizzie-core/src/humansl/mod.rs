//! AI 陪练's opponent: its levels, how a level picks a move from KataGo's HumanSL answer, how
//! long the AI appears to think, and the analysis protocol. No application or process state.
pub mod choose;
mod level;
pub mod passing;
pub mod protocol;
mod think_time;

pub use level::{CoachLevel, CoachLevelKind, PlaySettings};
pub use think_time::{think_time, Pace};

/// A legal move with its HumanSL policy odds.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub vertex: String,
    pub probability: f64,
}
