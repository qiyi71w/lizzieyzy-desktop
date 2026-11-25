//! Fox AI protocol mode. Workbench is the sole writer; the UI owns only dialog drafts.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum FoxCommand {
    /// Read the current foreground engine's rules and search limits, without changing them.
    Prepare,
    /// Warning accepted separately from replacing the document. Saving exits this attempt.
    Start { port: u16, max_time: f64, max_visits: u64, warning_confirmed: bool, replace: FoxReplaceChoice, document: u64, revision: u64 },
    Stop,
    ReturnLive,
    SetPda { value: f64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub enum FoxReplaceChoice { Ask, Save, Replace, Cancel }
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct FoxView {
    pub status_key: String,
    pub listening: bool,
    pub port: u16,
    pub owns_board: bool,
    pub preparing: bool,
    pub restoring: bool,
    pub ready: bool,
    pub can_start: bool,
    pub can_stop: bool,
    pub rules: Option<String>,
    /// Key of the preset name `rules` are (`FoxRulesText.describe`), else none and the dialog
    /// words them field by field.
    pub rules_preset_key: Option<String>,
    pub max_time: Option<f64>,
    pub max_visits: Option<u64>,
    pub connection: u64,
    pub game: u64,
    pub revision: u64,
    pub live_node: Option<crate::record::NodeId>,
    pub browsing: bool,
    pub browsing_status_key: String,
    pub clock: Option<FoxClockView>,
    pub pending: Option<FoxSubmissionView>,
    pub submissions: Vec<FoxSubmissionView>,
    pub desired_pda: Option<f64>,
    pub confirmed_pda: Option<f64>,
    pub pda_status_key: String,
    pub transcript: Vec<String>,
    pub outcome: Option<FoxStartOutcome>,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct FoxClockView { pub main_seconds: f64, pub byo_seconds: f64, pub periods: u32, pub remaining_seconds: f64 }
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct FoxSubmissionView { pub connection: u64, pub game: u64, pub revision: u64, pub search: u64, pub vertex: String, pub outcome: String, pub written: bool, pub reason: String }
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum FoxStartOutcome { NeedsWarning, NeedsReplacement { document: u64, revision: u64 }, SaveRequested, Cancelled, Listening, Failed { message_key: String } }
