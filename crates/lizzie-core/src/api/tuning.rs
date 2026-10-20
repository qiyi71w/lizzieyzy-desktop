//! Contract for engine performance: thread policy, benchmark, measured tuning and the GTP
//! configuration of an engine.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum TuningCommand {
    /// Placeholder until the tuning slice lands.
    Refresh,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum TuningResult {
    Done,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct TuningView {}
