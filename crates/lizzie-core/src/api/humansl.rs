use super::ScoreVerdict;
use crate::humansl::assist::{CoachMarks, HintStep};
use crate::humansl::feedback::{MoveFeedback, Verdict};
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
    /// 悔棋: the game goes back before the player's last move and the AI's reply, or only the
    /// player's move while the AI thinks; the moves stay in the record
    /// (`HumanSlGameController.takeBack`).
    TakeBack,
    /// The player gives up: the AI wins by resignation (`HumanSlGameController.resign`).
    Resign,
    RetryAi,
    /// 提示 / H: in 边下边提醒 the area of KataGo's choice, then its point; in 实战后复盘 a message
    /// that analysis stays hidden (`HumanSlGameController.hint`).
    Hint,
    /// The player's answer while the AI holds its reply to a 恶手.
    Decide { choice: HumanSlDecision },
    Finish,
    Abort,
    AbortThen { continuation: HumanSlContinuation },
    /// After the game in 边下边提醒: the session ends as 结束并复盘 does, without starting the
    /// analysis, and the board shows the position before move `move_number` of the game
    /// (`HumanSlGameController.reviewFrom`). Nothing happens while the game is on.
    ReviewFrom { move_number: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HumanSlContinuation {
    NewGame,
    NewAnalysisGame,
    EngineGame,
    /// 再来一盘: the AI 陪练 setup again, with the last game's choices.
    HumanSlSetup,
}

/// The choices after a 恶手 while the AI holds its reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HumanSlDecision {
    /// 重下这手: the 悔棋 of the 恶手; the held reply is dropped (`retryBlunder`).
    Retry,
    /// 看 AI 推荐: marks KataGo's choice before the 恶手 on the board (`showBestMove`).
    ShowBest,
    /// 不重下，继续对局: the AI plays the reply it held (`continueAfterBlunder`).
    PlayOn,
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
    /// The game started and the session is not over yet (Java `LizzieFrame.hasActiveHumanSlGame`:
    /// the controller is set once the game plays and cleared when it is torn down). A session
    /// cancelled during its preparation never has a game, also while it stops.
    pub game_started: bool,
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
    /// 悔棋 is possible: the game is on and has a move of the player's to withdraw
    /// (`canTakeBack`).
    pub can_take_back: bool,
    /// Take-backs in this game (`takeBackCount`).
    pub take_backs: u32,
    /// The player's last move in the game; rated in 边下边提醒 only (`CoachPanelState.moveCard`).
    pub last_move: Option<HumanSlLastMove>,
    /// Moves of this game so far, both players' and passes (`movesPlayed`).
    pub moves: u32,
    /// The game's komi (`gameKomi`).
    pub komi: f64,
    /// Stones captured by Black and by White at the game's position, whatever the board shows
    /// (`captures`).
    pub black_captures: u32,
    pub white_captures: u32,
    /// The player's win chance through the game in percent, oldest first: in 边下边提醒 the
    /// rank-aware chance around each rated move and in the position to move in, in 边下边分析 the
    /// foreground analysis of each analysed position of the game; `None` in 实战后复盘
    /// (`HumanSlGameController.humanWinrates`).
    pub win_chances: Option<Vec<f64>>,
    /// 边下边提醒's ratings and help; `None` in the other modes and once the session is ending.
    pub feedback: Option<HumanSlFeedbackView>,
}
/// 边下边提醒 in a game (Java `MoveReviewer`, `CoachAssist`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HumanSlFeedbackView {
    /// From the player's move until its rating arrives (`isJudging`).
    pub judging: bool,
    /// Every rated move of the player's still in the game, in the order played (`feedback`).
    pub ratings: Vec<MoveFeedback>,
    /// How many of them got each rating: 妙手, 好棋, 缓手, 问题手, 恶手 (`count`).
    pub counts: [u32; 5],
    /// The 恶手 among them (`keyMistakes`).
    pub key_mistakes: u32,
    /// The three worst mistakes among them, in the order played (`worstMistakes`).
    pub worst: Vec<MoveFeedback>,
    /// Positions of this game where the player asked for a hint (`hintsUsed`).
    pub hints: u32,
    /// 提示 is possible: the player is to move in the game and no reply is held (`canHint`).
    pub can_hint: bool,
    /// The hint of this turn, once asked for.
    pub hint: Option<HumanSlHintView>,
    /// The 恶手 the AI holds its reply for, while the player decides (`isAwaitingDecision`).
    pub decision: Option<HumanSlDecisionView>,
    /// The hint's or 看 AI 推荐's marks on the main board.
    pub marks: CoachMarks,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HumanSlLastMove {
    pub move_number: u32,
    /// The move, such as `Q16`, or `pass`.
    pub vertex: String,
    /// Its rating; `None` while it is rated or when it could not be.
    pub verdict: Option<Verdict>,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HumanSlHintView {
    pub step: HintStep,
    /// The marked area, 0 to 8 row by row from the top left (`CoachBoardMarks.areaOf`).
    pub area: Option<u8>,
    /// KataGo's choice, `pass` included, from the first step on.
    pub vertex: Option<String>,
    /// How often players of the level play it.
    pub share: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HumanSlDecisionView {
    pub rating: MoveFeedback,
    /// 看 AI 推荐 was chosen: KataGo's choice is marked.
    pub best_shown: bool,
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
    /// H in 实战后复盘: analysis stays hidden until the game ends (`HumanSlGame.analysisHidden`).
    AnalysisHidden,
    /// `diagnosis`: the engine was refused before it started, with its own explanation (a
    /// packaged engine too new for this macOS); no detail goes with it.
    StartFailed { reason: String, detail: Option<String>, diagnosis: Option<crate::engine::diagnosis::StartupDiagnosis> },
    ExitRecoveryFailed,
    Ended,
}
