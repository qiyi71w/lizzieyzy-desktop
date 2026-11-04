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
        disable_wrn: bool,
    },
    StartToolbar,
    Stop,
    Pause,
    SetValues {
        settings: MatchSettings,
        session: MatchSessionSettings,
        disable_wrn: bool,
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
    pub rules_target_choice: Option<String>,
    pub opening_index: Option<u32>,
    pub elapsed_seconds: f64,
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
    pub disable_wrn: bool,
    pub first_engine: u32,
    pub second_engine: u32,
    pub openings: Vec<String>,
    pub rules_prefill: String,
    pub rules_choice: Option<String>,
    pub rules_corrupt: bool,
}
