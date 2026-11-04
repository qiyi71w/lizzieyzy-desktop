use crate::settings::{HumanSlTrainingSettings, OpponentPreset, TrainingMode};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum HumanSlCommand {
    Start {
        settings: HumanSlTrainingSettings,
        from_current: bool,
        advanced_visible: bool,
    },
    CancelPreparation,
    Pass,
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
    pub opponent: OpponentPreset,
    pub rank: u8,
    pub dan_rank: bool,
    pub human_is_black: bool,
    pub human_ms: u64,
    pub ai_ms: u64,
    pub clock: Option<HumanSlSide>,
    pub human_turn: bool,
    pub ai_thinking: bool,
    pub ai_failed: bool,
    pub analysis_display: HumanSlAnalysisDisplay,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum HumanSlNotice {
    AiPassed,
    AiMoveFailed,
    StartFailed { reason: String, detail: Option<String> },
    ExitRecoveryFailed,
    Ended,
}
