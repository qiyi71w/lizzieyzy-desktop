//! Engine resources (引擎资源): the trusted catalog of KataGo packages and models, machine
//! detection, recommendation, downloading, offline import, reuse of installed resources and
//! explicit verification.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Marks an engine profile whose command this client's resource store wrote.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct ManagedProfile {
    /// The command as written; the profile stays managed only while its command is unchanged.
    pub command: String,
}
