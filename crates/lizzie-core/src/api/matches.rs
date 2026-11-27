use super::{GameKind, Stone, UnrestoredReason};
use crate::settings::MatchSettings;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Toolbar values persisted like Java toolbar-parameter, plus three explicitly session-only fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchSessionSettings {
    pub kind: GameKind,
    pub time_enabled: bool,
    pub first_seconds: f64,
    pub second_seconds: f64,
    pub visits_enabled: bool,
    pub first_visits: u32,
    pub second_visits: u32,
    pub first_move_visits_enabled: bool,
    pub first_move_visits: u32,
    pub second_move_visits: u32,
    pub batch_enabled: bool,
    pub batch_limit: u32,
    pub continue_position: bool,
    pub exchange_colors: bool,
    pub max_moves_enabled: bool,
    pub max_moves: u32,
    pub auto_save: bool,
    pub save_winrate: bool,
    pub batch_name: String,
    pub start_number_enabled: bool,
    pub start_number: u32,
    pub random_moves_enabled: bool,
    pub random_moves: u32,
    pub random_winrate_diff: f64,
}
impl Default for MatchSessionSettings {
    fn default() -> Self {
        Self {
            kind: GameKind::Analysis,
            time_enabled: true,
            first_seconds: 2.0,
            second_seconds: 2.0,
            visits_enabled: false,
            first_visits: 0,
            second_visits: 0,
            first_move_visits_enabled: false,
            first_move_visits: 0,
            second_move_visits: 0,
            batch_enabled: false,
            batch_limit: 0,
            continue_position: false,
            exchange_colors: true,
            max_moves_enabled: false,
            max_moves: 450,
            auto_save: true,
            save_winrate: false,
            batch_name: String::new(),
            start_number_enabled: false,
            start_number: 1,
            random_moves_enabled: false,
            random_moves: 16,
            random_winrate_diff: 0.3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum MatchCommand {
    Start {
        settings: MatchSettings,
        session: MatchSessionSettings,
        first_engine: u32,
        second_engine: u32,
    },
    StartToolbar,
    Stop,
    Pause,
    SetValues {
        settings: MatchSettings,
        session: MatchSessionSettings,
    },
    ReviseBatchLimit {
        limit: u32,
    },
    Intervention {
        op: MatchIntervention,
    },
    ReplaceOpenings {
        files: Vec<MatchOpeningFile>,
    },
    WinrateImageDone {
        game: u64,
        error: Option<String>,
    },
    /// Consent is bound to this game's participants/rules; Java reuses it within the batch only.
    ConfirmRules {
        game: u64,
        allow: bool,
    },
    /// 对局参数 (Java `EngineGameModule.reviseParameters`): new breadth / aggressiveness for the
    /// participant positions of game `game`; `None` leaves that value as it is. Only the values
    /// that differ from what the engines confirmed are written, each read back. Answered with
    /// [`super::CommandResult::ParametersRevised`] once the values are taken, `Unchanged` when the
    /// engines hold them all, or a refusal.
    ReviseParameters {
        game: u64,
        first_wrn: Option<f64>,
        second_wrn: Option<f64>,
        first_pda: Option<f64>,
        second_pda: Option<f64>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum MatchIntervention {
    PlayNow,
    ToggleManual,
    Resign { side: Stone },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct MatchOpeningFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum MatchPhase {
    Starting,
    Playing,
    Paused,
    Ending,
    BetweenGames,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct MatchScoreView {
    pub first_black_wins: u32,
    pub first_white_wins: u32,
    pub second_black_wins: u32,
    pub second_white_wins: u32,
    pub double_passes: u32,
    pub max_moves: u32,
    pub total_seconds: f64,
    pub first_visits: f64,
    pub second_visits: f64,
}

/// The open document is an engine game (Java `Board.isPkBoard`): the win-rate graph draws each
/// engine's curve and one score curve.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct PkBoardView {
    /// The side whose engine gives the score curve (`isPkBoardKataW`, else `isPkBoardKataB`):
    /// White when the white engine is KataGo or Sai, else Black when the black one is; `None`
    /// when neither is, and no score curve is drawn (`WinrateGraph.resolveScoreLeadAvailable`).
    pub score_side: Option<Stone>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MatchView {
    pub id: u64,
    pub game: u64,
    pub phase: MatchPhase,
    pub kind: GameKind,
    pub batch: bool,
    pub ordinal: u32,
    pub batch_limit: u32,
    pub completed_games: u32,
    pub first_engine: u32,
    pub second_engine: u32,
    pub first_name: String,
    pub second_name: String,
    pub first_is_black: bool,
    pub score: MatchScoreView,
    pub thinking_side: Option<Stone>,
    pub thinking_engine: Option<u32>,
    pub manual_moves: bool,
    pub can_play_now: bool,
    pub can_toggle_manual: bool,
    pub can_resign: bool,
    pub rules: String,
    pub rules_consent_required: bool,
    pub rules_unverified: Vec<MatchRulesUnverified>,
    /// The game's rules by name (`MatchRuleOption.displayName`, else 自定义), as the consent to
    /// unconfirmed rules names them.
    pub rules_target_name: String,
    pub opening_index: Option<u32>,
    pub elapsed_seconds: f64,
    /// The game's breadth and aggressiveness per participant position (对局参数会话); `None` in
    /// a Genmove match, which leaves both parameters alone.
    pub parameters: Option<MatchParametersView>,
}

/// One participant position's parameter in the current game (Java `EngineGameParameterState.Value`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct MatchParameterValue {
    /// The engine takes runtime changes of it: KataGo, and for aggressiveness not `KataGoPda`.
    pub supported: bool,
    /// The value the engine confirmed by read-back; `None` while unknown.
    pub confirmed: Option<f64>,
    /// The requested value while it differs from `confirmed`.
    pub target: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct MatchParameterPosition {
    /// 广度, KataGo `analysisWideRootNoise`.
    pub wrn: MatchParameterValue,
    /// 激进度, KataGo `playoutDoublingAdvantage`.
    pub pda: MatchParameterValue,
}

/// The current game's parameters (Java `EngineGameParameterState`). The game searches only while
/// nothing is pending and nothing failed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct MatchParametersView {
    /// 引擎 1 (the first engine of the dialog) and 引擎 2, whatever colour they play.
    pub first: MatchParameterPosition,
    pub second: MatchParameterPosition,
    /// Values are being written and read back.
    pub pending: bool,
    /// Why the last write was not confirmed.
    pub failures: Vec<MatchParameterFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum MatchParameter {
    /// 广度 (`analysisWideRootNoise`).
    Wrn,
    /// 激进度 (`playoutDoublingAdvantage`).
    Pda,
}

/// Why one participant did not confirm a parameter, or could not be given its value back.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum MatchParameterFailureReason {
    /// The engine cannot take the parameter (not KataGo, or aggressiveness on `KataGoPda`) and
    /// its value is not 0.
    Unsupported { parameter: MatchParameter },
    /// No answer within the engine-game command limit.
    Timeout,
    /// The engine refused `command`, or answered something that is not a number.
    Rejected { command: String, response: String },
    /// `command` could not be sent to the engine.
    NotSent { command: String },
    /// The read-back reports `actual` instead of `expected`.
    Mismatch { actual: String, expected: String },
    /// The process whose values were captured is gone; its replacement was left alone.
    ProcessChanged,
    /// The match was stopped before the engine answered.
    Interrupted,
    /// The game ended for another reason (an engine failed) before the engine answered.
    GameEnded,
    /// A revision waited too long for the game's operation in flight (a komi change).
    TurnTimeout,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MatchParameterFailure {
    /// 引擎 1 when `Some(true)`, 引擎 2 when `Some(false)`; `None` for the game as a whole.
    pub first: Option<bool>,
    pub reason: MatchParameterFailureReason,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MatchRulesUnverified {
    pub name: String,
    pub commands: String,
    pub side: Option<Stone>,
    pub reason_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum MatchEnd {
    Resigned,
    DoublePass,
    MaxMoves,
    UserStopped,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MatchOutcomeView {
    pub game: u64,
    pub ordinal: u32,
    pub games_dir: String,
    pub batch: bool,
    pub end: MatchEnd,
    pub winner: Option<Stone>,
    pub result: String,
    pub batch_finished: bool,
    pub score: MatchScoreView,
    pub saved_paths: Vec<String>,
    pub unrestored: Vec<UnrestoredReason>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MatchNotice {
    pub message_key: String,
    pub arguments: Vec<String>,
    /// Parameter failures the message reports in its `{0}`.
    pub parameter_failures: Vec<MatchParameterFailure>,
}

impl MatchNotice {
    pub fn new(message_key: &str) -> Self {
        Self {
            message_key: message_key.into(),
            arguments: Vec::new(),
            parameter_failures: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MatchWinrateImageRequest {
    pub game: u64,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct MatchSettingsView {
    pub settings: MatchSettings,
    pub session: MatchSessionSettings,
    pub first_engine: u32,
    pub second_engine: u32,
    pub openings: Vec<String>,
    pub rules_prefill: String,
    /// The `MATCH_RULE_OPTIONS` entry the prefilled rules are, by id; `None` is 自定义.
    pub rules_choice: Option<String>,
    pub rules_corrupt: bool,
}
