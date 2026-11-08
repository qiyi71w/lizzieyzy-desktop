//! 跑谱贡献 (KataGo distributed training, Java `ContributeEngine`, `ContributeView`,
//! `ContributeSettings`): commands, the session projection and its one-time notices.
use crate::resources::CapabilityStatus;
use crate::settings::{ContributionSettings, ContributionWatchSettings};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum ContributionCommand {
    /// 保存设置 (`ContributeSettings.saveConfig`): the whole group; the password goes through
    /// [`ContributionCommand::SetPassword`].
    SaveSettings { settings: ContributionSettings },
    /// Platform credential storage: remember (empty forgets) the training password of `user`.
    /// Answers [`crate::api::CommandResult::ContributionPassword`].
    SetPassword { user: String, password: String },
    /// Where the training password of `user` is kept. Answers `ContributionPassword`.
    PasswordState { user: String },
    /// 可视化KataGo分布式训练 / 开始跑谱贡献! / the failure window's 重启引擎
    /// (`LizzieFrame.startContributeEngine`).
    Start,
    /// 暂停 / 恢复 (`ContributeEngine.pauseAndResume`).
    PauseResume,
    /// 顺序结束(延迟): finish the current games, then quit (`slowQuit`).
    SlowQuit,
    /// 强制结束(立刻) (`normalQuit`): stop now; unfinished games are lost.
    ForceQuit,
    /// 关闭此界面 / closing the window after the contribution ended. Refused while it runs
    /// (`ContributeView.closeTip`).
    CloseView,
    /// 第一局 / 上一局 / 下一局 / 最后一局 / 跳转: 0-based; negative is ignored, past the end
    /// selects the last game (`setWatchGame`).
    WatchGame { index: i64 },
    /// 保存所有棋谱 (`saveAllGames`).
    SaveAllGames,
    /// The contribution window's own check boxes and fields, saved as they change.
    SetWatchSettings { settings: ContributionWatchSettings },
    /// 显示 / 隐藏 the result; this session only.
    ToggleResultHidden,
    /// Settings > 显示跑谱贡献菜单 (`show-Contribute`).
    SetShowMenu { enabled: bool },
    /// 已配置引擎…: what each saved engine entry would fill in. Answers
    /// `ContributionSavedEngines`.
    SavedEngines,
    /// The contribution capability of the engine `engine_path` names (a settings draft).
    /// Answers `ContributionQualification`.
    Qualify { engine_path: String },
    /// 浏览: the system file chooser. Answers `FileChosen` or `Unchanged`.
    ChooseFile { kind: ContributionFile, title: String },
    /// 注册 / KataGo官方网站 in the system browser.
    OpenLink { link: ContributionLink },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ContributionFile {
    Engine,
    Config,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ContributionLink {
    /// `https://katagotraining.org/accounts/signup/`
    SignUp,
    /// `https://katagotraining.org/`
    OfficialWebsite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum ContributionPhase {
    /// Launched, no game seen yet (or relaunching after a failure).
    Starting,
    Running,
    /// The process ended (finished, failed or stopped); the window offers 关闭此界面.
    Exited,
}

/// The contribution window's tip line (`lblTip`): a Java text key.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ContributionTip {
    pub key: String,
    /// `Signal to stop` was seen: only pause/resume tips replace it (`setEternalTip`).
    pub eternal: bool,
}

/// The watched game (`ContributeView` type/komi/result/rules lines).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ContributionGameView {
    pub game_id: String,
    /// 评分对局 (the players differ) versus 自对弈训练.
    pub rating: bool,
    pub black: String,
    pub white: String,
    pub width: u16,
    pub height: u16,
    pub komi: f64,
    /// `Utils.getRuleString` in the session language, lines separated by `\r\n`.
    pub rules_text: String,
    pub complete: bool,
    /// Empty until the game is complete and its result was read.
    pub result: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ContributionView {
    /// Session identity; a new start gets a new one.
    pub id: u64,
    pub phase: ContributionPhase,
    pub paused: bool,
    /// Java `isContributing`: starting or running and not paused. Gates the board, new games,
    /// other engine modes and engine switching.
    pub active: bool,
    /// 顺序结束 was sent.
    pub slow_quitting: bool,
    pub tip: ContributionTip,
    /// The watched game, 1-based; 0 before the first game.
    pub watching: u32,
    pub total: u32,
    pub finished: u32,
    pub game: Option<ContributionGameView>,
    pub result_hidden: bool,
    /// The `engine` name of this session's console lines (`engine-console` events).
    pub console_engine: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum ContributionNotice {
    /// A message box with this Java text key (e.g. `Contribute.tips.exitedAfterSignal`).
    Message { key: String },
    /// `Contribute.version.outOfDate` (`version`) `Contribute.version.outOfDate2`; the
    /// contribution was stopped and its window closed.
    OutOfDate { version: String },
    /// Show `key`, then open the settings dialog (no user name / engine path).
    OpenSettings { key: String },
    /// `EngineGameInfo.engineGameEndHintKifuPos` + `folder`.
    Saved { folder: String },
    /// The contribution engine ended abnormally (`EngineFailedMessage` with 重启引擎).
    Failed { failure: crate::api::EngineFailureView },
}

/// `SavedEnginePathResolver.Kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum SavedEngineKind {
    Local,
    Remote,
    Indirect,
    Unknown,
}

/// `SavedEnginePathResolver.Reason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum SavedEngineReason {
    EmptyCommand,
    UnparseableCommand,
    MissingFile,
    RemoteTransport,
    IndirectLauncher,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct SavedEngineCandidate {
    /// Position in the engine list, from 1.
    pub index: u32,
    pub name: String,
    pub kind: SavedEngineKind,
    /// The normalized absolute executable when it can be filled in.
    pub executable: Option<String>,
    /// The engine path field text for `executable` (quoted when needed).
    pub field_text: Option<String>,
    pub reason: Option<SavedEngineReason>,
}

/// What this client knows about an engine's 跑谱贡献 capability (a resource package's catalog
/// entry); a qualification is never inferred from 官方 origin or analysis support alone.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum ContributionQualification {
    /// An engine package this client installed: its catalog capability.
    Managed {
        asset_id: String,
        version: String,
        backend: String,
        status: CapabilityStatus,
    },
    /// A custom command or an engine this client did not install: not known.
    Unknown,
}
