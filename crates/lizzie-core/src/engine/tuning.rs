//! Engine performance settings stored on an engine profile: the KataGo search-thread policy with
//! its benchmark results, and the GTP configuration profile.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Java `EngineData.threadPolicy`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct ThreadPolicy {
    /// Set by the first automatic setup of a profile (Java `initialSetup`).
    pub initial_setup: bool,
}

impl ThreadPolicy {
    /// The policy of a profile created by engine resource setup (Java
    /// `{"source": "CFG", "sourceRevision": 0, "initialSetup": true}`).
    pub fn initial_setup() -> ThreadPolicy {
        ThreadPolicy { initial_setup: true }
    }
}

/// Java `gtpConfigurationProtocol` / `gtpConfigurationProfile`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct GtpConfiguration {}
