//! Contract for engine resources (KataGo auto setup): machine detection, recommendation,
//! download, offline import, reuse, verification, applying an engine setup, weights and
//! acceleration components.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum ResourceCommand {
    /// Detect the machine and read installed resources (`refresh`: detect again).
    Inspect { refresh: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum ResourceResult {
    Done,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct ResourcesView {}
