use super::ScoreVerdict;
use crate::humansl::CoachLevel;
use crate::record::Stone;
use crate::settings::{HumanSlTrainingSettings, TrainingMode};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum HumanSlCommand {
    Start {
        settings: HumanSlTrainingSettings,
        from_current: bool,
        advanced_visible: bool,
        /// The user confirmed stopping a running batch analysis ([`crate::api::CommandResult::NeedsBatchStop`]).
        #[serde(default)]
        stop_batch: bool,
    },
    CancelPreparation,
    Pass,
    /// The player gives up: the AI wins by resignation (`HumanSlGameController.resign`).
    Resign,
    RetryAi,
    Finish,
    Abort,
    AbortThen { continuation: HumanSlContinuation },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HumanSlContinuation {
    NewGame,
    NewAnalysisGame,
    EngineGame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum HumanSlPhase {
    Preparing,
    Playing,
    Stopping,
    CleanupPending,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum HumanSlStartupStage {
    Starting,
    LoadingModels,
    OptimizingGpu,
    CacheReady,
    Ready,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum HumanSlSide {
    Human,
    Ai,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum HumanSlAnalysisDisplay {
    Suppress,
    Show,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HumanSlView {
    pub id: u64,
    pub phase: HumanSlPhase,
    pub stage: Option<HumanSlStartupStage>,
    pub preparing_secs: u32,
    pub mode: TrainingMode,
    pub level: CoachLevel,
    pub human_is_black: bool,
    pub human_ms: u64,
    pub ai_ms: u64,
    pub clock: Option<HumanSlSide>,
    pub human_turn: bool,
    pub ai_thinking: bool,
    pub ai_failed: bool,
    pub analysis_display: HumanSlAnalysisDisplay,
    /// KataGo counts the game after both players passed.
    pub counting: bool,
    /// How the game ended; the session stays on for the player to review until 结束并复盘.
    pub result: Option<HumanSlResult>,
}
/// How an AI 陪练 game ended (`training/GameResult`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum HumanSlResult {
    /// KataGo's count after both players passed, as 终局点目 states it.
    Counted { verdict: ScoreVerdict },
    /// `winner` won by resignation.
    Resigned { winner: Stone },
    /// KataGo did not count the game in time or failed: nothing is written to the record.
    Uncounted,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum HumanSlNotice {
    AiPassed,
    AiMoveFailed,
    /// The AI gave up (`HumanSlGame.aiResigned`).
    AiResigned,
    /// Both players passed; KataGo counts the game (`HumanSlGame.bothPassed`).
    BothPassed,
    /// The player acted on a position browsed back to: the board shows the game's position again
    /// (`HumanSlGame.backToLiveGame`).
    BackToLiveGame,
    /// The game's moves were deleted, which ended the session (`HumanSlGame.gameMovesDeleted`).
    GameMovesDeleted,
    StartFailed { reason: String, detail: Option<String> },
    ExitRecoveryFailed,
    Ended,
}
