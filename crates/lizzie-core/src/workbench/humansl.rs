//! HumanSL owns a training session, not the user's committed display settings. The companion
//! speaks analysis JSON; the foreground engine is only paused and later resynchronized.
mod ai;
mod review;
use super::exporting::java_text;
use super::Workbench;
use crate::api::*;
use crate::engine::{
    gtp,
    rules::{KataRules, RuleSet},
    runtime::{InstanceId, LeaseAvailability, RuntimeEvent},
};
use crate::humansl::{self, protocol, take_back, CoachLevelKind};
use crate::record::{DocumentEdit, NodeId, NodeKind, Point, Stone, StoneChange};
use crate::scoring;
use crate::settings::{HumanSlTrainingSettings, PlayerColor, TrainingMode};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct HumanSlFlow {
    next: u64,
    session: Option<Session>,
    notices: Vec<HumanSlNotice>,
    continuation: Option<ExitContinuation>,
    ready_continuation: Option<HumanSlContinuation>,
    random: u64,
}

enum ExitContinuation {
    Command(Box<Command>),
    Ui(HumanSlContinuation),
    /// The board goes to this position of the finished game, when it is still in the game's record.
    ReviewAt { doc: u64, node: Option<NodeId> },
}
struct Session {
    id: u64,
    worker: Option<InstanceId>,
    command: String,
    phase: HumanSlPhase,
    stage: HumanSlStartupStage,
    settings: HumanSlTrainingSettings,
    advanced: bool,
    from_current: bool,
    start: Instant,
    tick: Instant,
    turn: Instant,
    old_paused: bool,
    old_limit_stop: Option<(u64, NodeId)>,
    old_secondary_limit_stop: Option<(u64, NodeId)>,
    human: Stone,
    human_ms: u64,
    ai_ms: u64,
    failed: bool,
    request: Option<Request>,
    /// The game's document and its position (Java `liveNode`): where the game stands, whatever
    /// the board shows while the player browses. Moves and the AI's replies are played there.
    doc: u64,
    live: NodeId,
    generation: u64,
    training_start_node: Option<NodeId>,
    /// Where the game started (Java `gameStartNode`): the two passes that end it come after it.
    start_node: NodeId,
    /// The komi a new game starts with, decided at the start; once the game is on, the game's
    /// komi (Java `gameKomi`), which rounds its count.
    komi: f64,
    /// The AI's hopeless turns so far against its level's thresholds.
    resignation: humansl::Resignation,
    /// KataGo counts the game after both players passed.
    counting: bool,
    /// How the game ended; the session stays on for review until it is finished.
    result: Option<HumanSlResult>,
    /// Take-backs in this game (`takeBacks`).
    take_backs: u32,
    /// The continuations this game's take-backs withdrew (`takenBackLine`); the game's alone.
    taken_back: take_back::TakenBack,
    /// 边下边提醒's ratings and help (Java `MoveReviewer`, `CoachAssist`); only in that mode, and
    /// gone once the session ends, which drops whatever answer is still to come.
    review: Option<review::Reviewer>,
    restore: Option<u64>,
    resume: bool,
    started_game: bool,
    diagnostics: std::collections::VecDeque<String>,
}
struct Request {
    id: String,
    generation: u64,
    node: NodeId,
    stamp: u64,
    /// When the AI's turn began: the think time counts from here, across a retry.
    started: Instant,
    deadline: Instant,
    attempt: u8,
    phase: Search,
    /// A policy level's legal moves from its probe, the pass left out.
    candidates: Vec<humansl::Candidate>,
    /// Where the AI may pass, whether players of a policy level would mostly pass here.
    human_pass: bool,
    /// The human policy's share on its most popular move, which paces the reply.
    top_share: Option<f64>,
    /// KataGo's root winrate and score lead for the AI, which its resignation goes by; a policy
    /// level has them only from its pass check.
    evaluation: Option<(f64, f64)>,
    selected: Option<String>,
    /// The earliest the selected move is played.
    pace: Instant,
    /// Whether the rules last sent with the request have KataGo's button: a count is judged under
    /// the rules it was asked under, whatever the foreground plays when the answer comes.
    button: bool,
    /// The reply answers a move of the player's that is being rated: once its think time is over
    /// it waits for that rating, and a 恶手 holds it (Java `lastJudgement`).
    verdict: bool,
    /// Until when the reply waits for that rating.
    verdict_wait: Option<Instant>,
}
/// What the companion is asked: readiness, a policy level's one-visit probe and, when the AI may
/// pass, KataGo's own pass check, or a search level's search; then the reply waits for its pace,
/// and after a 恶手 it is held for the player's decision. Once both players passed, KataGo's count
/// of the game.
#[derive(PartialEq)]
enum Search {
    Ready,
    Probe,
    PassCheck,
    Play,
    Pacing,
    Held,
    Count,
}
/// Where the board stood for an action on the game: on the game's position, brought back to it
/// from a browsed one, or nowhere, the game being gone.
enum GameShown {
    Already,
    Returned,
    Ended,
}

fn refused(key: &str) -> CommandResult {
    CommandResult::Refused {
        message_key: key.into(),
    }
}

/// An edit other than the player's move during a session: deleting moves says to use 悔棋
/// instead (Java `HumanSlGameController.showDeleteBlockedHint`, which the UI shows for 3 s);
/// anything else is refused as the game holds the record.
pub(super) fn human_sl_edit_refusal(op: &EditCommand) -> CommandResult {
    match op {
        EditCommand::DeleteMove { .. }
        | EditCommand::DeleteMoveNoHint
        | EditCommand::DeleteBranch { .. }
        | EditCommand::EditStone {
            change: StoneChange::Delete,
            ..
        } => refused("HumanSlGame.deleteBlocked"),
        _ => refused("AnalysisSettings.reuseStatus.human_sl_game"),
    }
}
impl Workbench {
    pub fn human_sl_active(&self) -> bool {
        self.human_sl.session.is_some()
    }

    pub fn human_sl(&mut self, op: HumanSlCommand) -> Result<CommandResult, ApiError> {
        match op {
            HumanSlCommand::Start {
                settings,
                from_current,
                advanced_visible,
                stop_batch,
            } => self.human_sl_start(settings.normalized(), from_current, advanced_visible, stop_batch),
            HumanSlCommand::CancelPreparation => {
                if self
                    .human_sl
                    .session
                    .as_ref()
                    .is_some_and(|s| s.phase == HumanSlPhase::Preparing)
                {
                    self.human_sl_exit(false);
                }
                Ok(CommandResult::Done)
            }
            HumanSlCommand::Finish | HumanSlCommand::Abort => {
                self.human_sl_exit(true);
                Ok(CommandResult::Done)
            }
            HumanSlCommand::AbortThen { continuation } => {
                if !self.human_sl_active() {
                    self.human_sl.ready_continuation = Some(continuation);
                    return Ok(CommandResult::Done);
                }
                Ok(self.human_sl_queue_exit(ExitContinuation::Ui(continuation)))
            }
            HumanSlCommand::Pass => Ok(self.human_sl_human_move(None)),
            HumanSlCommand::TakeBack => Ok(self.human_sl_take_back()),
            HumanSlCommand::Resign => Ok(self.human_sl_resign()),
            HumanSlCommand::RetryAi => {
                if self
                    .human_sl
                    .session
                    .as_ref()
                    .is_some_and(|s| s.phase == HumanSlPhase::Playing && s.failed)
                {
                    self.human_sl_schedule(Instant::now());
                }
                Ok(CommandResult::Done)
            }
            HumanSlCommand::Hint => Ok(self.human_sl_hint()),
            HumanSlCommand::Decide { choice } => Ok(self.human_sl_decide(choice)),
            HumanSlCommand::ReviewFrom { move_number } => Ok(self.human_sl_review_from(move_number)),
        }
    }

    fn human_sl_start(
        &mut self,
        settings: HumanSlTrainingSettings,
        from_current: bool,
        advanced: bool,
        stop_batch: bool,
    ) -> Result<CommandResult, ApiError> {
        if self.game_active() || self.match_active() {
            return Ok(refused("LizzieFrame.engineGameStopFirstHint"));
        }
        if self.human_sl_active() {
            return Ok(CommandResult::Unchanged);
        }
        if self
            .engines
            .foreground()
            .and_then(|id| self.engines.unrestored(id))
            .is_some()
        {
            return Ok(refused("AnalysisSettings.reuseStatus.engine_state_unrestored"));
        }
        // A batch outlives a new board (Java saves the coach record under the batch file's name or
        // keeps the batch's automatic analysis running), so it is stopped first once the user agrees.
        let batch = self.batch_running();
        if batch && !stop_batch {
            return Ok(CommandResult::NeedsBatchStop);
        }
        let Some(model) = self.human_sl_model_path() else {
            return Ok(refused("HumanSlGame.missingModelDownloadPrompt"));
        };
        let command = match self
            .human_sl_base()
            .and_then(|base| self.human_sl_command(&base, &model))
        {
            Ok(c) => c,
            Err(reason) => {
                self.human_sl.notices.push(HumanSlNotice::StartFailed {
                    reason: reason.into(),
                    detail: None,
                    diagnosis: None,
                });
                return Ok(CommandResult::Done);
            }
        };
        // 批量精析 on either engine; otherwise 批量分析 analysing or loading its next record.
        if batch && !self.cancel_batch_flash() {
            self.auto_analysis_end_now();
        }
        // Java admits AI 陪练 beside an analysis borrowing the foreground engine
        // (`startHumanSlGameDialogReserved` checks only games). A new record ends quick, flash,
        // whole-game and automatic analysis when it replaces the board; a start from the current
        // position keeps the record, so such an analysis ends here instead of outliving the
        // session's foreground handling.
        if from_current {
            if self.quick_holds_foreground() {
                self.quick_cancel();
            }
            if self.flash_holds_foreground() {
                self.cancel_flash_analysis();
            }
            if self.whole_game_holds_foreground() {
                self.stop_whole_game_analysis();
            }
            if self.auto_analysis_active() {
                self.auto_analysis_end_now();
            }
        }
        self.human_sl.next += 1;
        let id = self.human_sl.next;
        let old_paused = self.paused;
        let old_limit_stop = self.limit_stop;
        let old_secondary_limit_stop = self.secondary_limit_stop;
        self.paused = true;
        self.engines.set_human_sl(true);
        self.sync_engine();
        let worker = match self.engines.start_worker(&command, "HumanSL") {
            Ok(w) => w,
            Err(error) => {
                self.engines.set_human_sl(false);
                self.paused = old_paused;
                self.sync_engine();
                self.human_sl.notices.push(HumanSlNotice::StartFailed {
                    reason: "HumanSlTraining.preparing.failedDetail".into(),
                    detail: error.diagnosis.is_none().then(|| redact_detail(&error.message)),
                    diagnosis: error.diagnosis,
                });
                self.human_sl.notices.push(HumanSlNotice::Ended);
                return Ok(CommandResult::Done);
            }
        };
        self.engines.report_worker_diagnostics(worker);
        // `CoachSetupForm.config`: with a handicap the player takes Black (the settings are
        // normalized so) and the komi is 0.5; an even game takes the even-game komi of the rules
        // KataGo plays, as the board stands before the game.
        let human = match settings.player_color {
            PlayerColor::Black => Stone::Black,
            PlayerColor::White => Stone::White,
            PlayerColor::Random => {
                if self.human_sl_random() < 0.5 {
                    Stone::Black
                } else {
                    Stone::White
                }
            }
        };
        let komi = if settings.handicap > 0 {
            humansl::HANDICAP_KOMI
        } else {
            self.human_sl_even_komi(self.chosen_rule_set())
        };
        let resignation = humansl::Resignation::for_play(settings.level.play());
        let review = (settings.mode == TrainingMode::LiveFeedback).then(review::Reviewer::default);
        let now = Instant::now();
        self.human_sl.session = Some(Session {
            id,
            worker: Some(worker),
            command,
            phase: HumanSlPhase::Preparing,
            stage: HumanSlStartupStage::Starting,
            settings,
            advanced,
            from_current,
            start: now,
            tick: now + Duration::from_secs(1),
            turn: now,
            old_paused,
            old_limit_stop,
            old_secondary_limit_stop,
            human,
            human_ms: 0,
            ai_ms: 0,
            failed: false,
            request: None,
            doc: self.doc.id,
            live: self.cursor,
            generation: 0,
            training_start_node: None,
            start_node: self.cursor,
            komi,
            resignation,
            counting: false,
            result: None,
            take_backs: 0,
            taken_back: Default::default(),
            review,
            restore: None,
            resume: false,
            started_game: false,
            diagnostics: Default::default(),
        });
        self.human_sl_request(Search::Ready, now);
        Ok(CommandResult::Done)
    }

    /// The komi of an even game under the rules the foreground KataGo plays
    /// (`CoachSetupContext.ofMainWindow`, `RulePreset.evenGameKomi`): that of the rule set they
    /// are, the rule set `chosen` last ([`Workbench::chosen_rule_set`]) telling apart those with
    /// the same rules; else, after a handicap game, the komi new games start with (the chosen rule
    /// set's, or 7.5); else the current komi.
    pub(super) fn human_sl_even_komi(&self, chosen: Option<&RuleSet>) -> f64 {
        let rules = self.engines.foreground().and_then(|f| self.engines.confirmed_rules(f));
        RuleSet::even_game_komi(rules, self.komi, self.doc.record.info.handicap > 0, chosen)
    }

    /// The companion's analysis command before the HumanSL model is added, or the key of the first
    /// part it lacks, in Java's order: the engine, its analysis config, its weight
    /// (`AnalysisEngineCommandHelper.resolveHumanSlCommand`, `missingPart`). The command is 精析设置's
    /// own or the one made from the foreground (else default) engine. As in Java, only a command of
    /// this application's KataGo (Java's bundled engine; here one 一键设置 installed in the resource
    /// directory) has its files checked; a command of the user's own runs as written, since its
    /// paths may be the engine's own (a Windows engine run from WSL reads `D:\…`). An engine set up
    /// for play only gets its `analysis.cfg` made beside its config and is not reported; when that
    /// file cannot be made, the analysis config is what is missing.
    fn human_sl_base(&self) -> Result<String, &'static str> {
        const NO_ENGINE: &str = "AnalysisEngineCommandHelper.noEngineSelected";
        const NO_ANALYSIS_CONFIG: &str = "AnalysisEngineCommandHelper.noAnalysisConfig";
        const NO_WEIGHT: &str = "AnalysisEngineCommandHelper.noWeight";
        let a = &self.config.analysis_engine;
        let base = if a.command_customized {
            a.command.clone()
        } else {
            match self.generate_analysis_command(None) {
                Ok((command, _)) => command,
                Err("AnalysisEngineCommandHelper.generateConfigFailed") => {
                    let engine = self.analysis_source(None).map_or("", |p| p.command.as_str());
                    let missing = self
                        .own_launch(engine)
                        .is_some_and(|spec| !Path::new(&spec.program).is_file());
                    return Err(if missing { NO_ENGINE } else { NO_ANALYSIS_CONFIG });
                }
                Err(_) => a.command.clone(),
            }
        };
        if base.trim().is_empty() || base.trim() == crate::settings::DEFAULT_ANALYSIS_COMMAND {
            return Err(NO_ENGINE);
        }
        let Some(spec) = self.own_launch(&base) else {
            return Ok(base);
        };
        // The value after the first `-config`/`--config` (or model) option, as the launch resolves it.
        let file = |options: [&str; 2]| {
            spec.args
                .windows(2)
                .find(|w| options.contains(&w[0].as_str()))
                .map(|w| match &spec.working_dir {
                    Some(dir) => dir.join(&w[1]),
                    None => PathBuf::from(&w[1]),
                })
                .is_some_and(|path| path.is_file())
        };
        if !Path::new(&spec.program).is_file() {
            Err(NO_ENGINE)
        } else if !file(["-config", "--config"]) {
            Err(NO_ANALYSIS_CONFIG)
        } else if !file(["-model", "--model"]) {
            Err(NO_WEIGHT)
        } else {
            Ok(base)
        }
    }

    /// How `command` launches, when its program is in this application's resource directory.
    fn own_launch(&self, command: &str) -> Option<gtp::LaunchSpec> {
        gtp::prepare_launch(command, self.engines.roots())
            .filter(|spec| Path::new(&spec.program).starts_with(self.resources.store().root()))
    }

    fn human_sl_command(&self, base: &str, model: &Path) -> Result<String, &'static str> {
        let mut parts = gtp::split_command(base);
        let tensorrt = self
            .engines
            .foreground()
            .and_then(|id| self.engines.profile_of(id))
            .and_then(|p| p.managed.as_ref())
            .is_some_and(|m| m.asset_id == "windows-tensorrt")
            || parts
                .first()
                .is_some_and(|p| p.to_ascii_lowercase().contains("windows-tensorrt"));
        if tensorrt {
            let catalog = self.resources.catalog();
            let asset = catalog
                .assets
                .get("windows-nvidia")
                .ok_or("HumanSlGame.error.tensorRtCompanionMissing")?;
            let inventory = self
                .resources
                .store()
                .inventory()
                .map_err(|_| "HumanSlGame.error.tensorRtCompanionMissing")?;
            let path = inventory
                .packages
                .into_iter()
                .find(|p| p.is_current(asset))
                .map(|p| p.executable)
                .ok_or("HumanSlGame.error.tensorRtCompanionMissing")?;
            if let Some(exe) = parts.first_mut() {
                *exe = path.to_string_lossy().into_owned();
            }
        }
        let command = gtp::join_command(&parts).ok_or("HumanSlGame.error.noEngine")?;
        protocol::command(&command, model)
    }

    fn human_sl_prepared(&mut self, now: Instant) {
        let Some(s) = self.human_sl.session.as_ref() else {
            return;
        };
        let settings = s.settings.clone();
        let advanced = s.advanced;
        let current = s.from_current;
        let human = s.human;
        let komi = s.komi;
        // Persist before changing the document: a disk failure cannot leave a half-started game.
        if let Err(e) = self.commit_config(|c| {
            c.human_sl.training = settings.clone();
            c.human_sl.advanced_visible = advanced;
        }) {
            self.human_sl_start_failed(e.message);
            return;
        }
        if !current {
            let size = self.doc.record.size;
            self.reopen_empty_board(size, false);
            self.komi_changed = true;
            self.start_komi = None;
            self.apply_komi(komi);
            let _ = self.apply_edit(DocumentEdit::SetHandicap {
                handicap: u32::from(settings.handicap),
            });
            if settings.handicap >= 2 && size.width == 19 && size.height == 19 {
                for point in fixed_handicap(settings.handicap) {
                    let _ = self.apply_edit(DocumentEdit::SetupStone {
                        point,
                        stone: Some(Stone::Black),
                    });
                }
                let _ = self.apply_edit(DocumentEdit::SetupSideToPlay { color: Stone::White });
            }
            // Literal keys keep them in the Java string extraction.
            let lang = self.active_language;
            let me = java_text(lang, "HumanSlGame.humanPlayer");
            // configurePlayerNames: "HumanSL AI (野狐 3D, KataGo 强3段)".
            let level = settings.level;
            let fox = java_text(lang, "HumanSlTraining.level.fox").replace("{0}", level.fox());
            let plain = match level.kind() {
                CoachLevelKind::Kyu => java_text(lang, "HumanSlTraining.rank.kyuValue"),
                CoachLevelKind::Dan | CoachLevelKind::StrongDan => java_text(lang, "HumanSlTraining.rank.danValue"),
            }
            .replace("{0}", &level.rank().to_string());
            let kata_go = if level.kind() == CoachLevelKind::StrongDan {
                java_text(lang, "HumanSlTraining.rankTable.strong").replace("{0}", &plain)
            } else {
                plain
            };
            let ai = format!("{} ({fox}, KataGo {kata_go})", java_text(lang, "HumanSlGame.aiPlayer"));
            let (black, white) = if human == Stone::Black { (me, ai) } else { (ai, me) };
            let _ = self.apply_edit(DocumentEdit::SetPlayers { black, white });
        }
        let s = self.human_sl.session.as_mut().unwrap();
        s.phase = HumanSlPhase::Playing;
        s.started_game = true;
        s.stage = HumanSlStartupStage::Ready;
        s.request = None;
        s.turn = now;
        s.doc = self.doc.id;
        s.live = self.cursor;
        s.start_node = self.cursor;
        // A game from the current position keeps the record's komi.
        s.komi = self.komi;
        if current {
            s.training_start_node = Some(self.cursor);
        }
        self.paused = settings.mode != TrainingMode::LiveAnalysis;
        self.sync_engine();
        self.human_sl_schedule(now);
        self.human_sl_review_turn(now);
    }

    fn human_sl_start_failed(&mut self, detail: String) {
        let mut diagnostics = self
            .human_sl
            .session
            .as_ref()
            .map(|s| s.diagnostics.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        diagnostics.push(detail);
        let detail = redact_detail(
            &diagnostics
                .into_iter()
                .rev()
                .take(10)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n"),
        );
        self.human_sl.notices.push(HumanSlNotice::StartFailed {
            reason: "HumanSlTraining.preparing.failedDetail".into(),
            detail: Some(detail),
            diagnosis: None,
        });
        self.human_sl_exit(false);
    }

    /// The player's stone at `point`, or pass. While the player browses, the board only goes back
    /// to the game's position, saying so; the move or pass is not played (Java `atLiveGame`).
    /// Once both players passed, KataGo counts the game; while it counts and after the game,
    /// nothing is played.
    pub(super) fn human_sl_human_move(&mut self, point: Option<Point>) -> CommandResult {
        if !self.human_sl_game_on() {
            return CommandResult::Unchanged;
        }
        match self.human_sl_to_game() {
            GameShown::Ended => return CommandResult::Done,
            GameShown::Returned => {
                self.human_sl.notices.push(HumanSlNotice::BackToLiveGame);
                return CommandResult::Done;
            }
            GameShown::Already => {}
        }
        let s = self.human_sl.session.as_ref().unwrap();
        let before = s.live;
        if s.request.is_some() || self.doc.record.node(s.live).to_play != s.human || !self.human_sl_commit(point) {
            return CommandResult::Unchanged;
        }
        let now = Instant::now();
        let s = self.human_sl.session.as_mut().unwrap();
        s.human_ms = s.human_ms.saturating_add(now.duration_since(s.turn).as_millis() as u64);
        s.turn = now;
        let (after, judged) = (s.live, s.review.is_some());
        self.human_sl_judge(before, after, now);
        if point.is_none() && self.human_sl_both_passed() {
            self.human_sl_count(now);
        } else {
            self.human_sl_schedule(now);
            if let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.request.as_mut()) {
                r.verdict = judged;
            }
        }
        CommandResult::Done
    }

    /// 提示 / H (`HumanSlGameController.hint`): in 边下边提醒 the next step of the turn's hint on
    /// the game's position, which a browsing board first goes back to (saying so, without a
    /// hint); in 实战后复盘 a message that analysis stays hidden. 边下边分析 has no hint.
    fn human_sl_hint(&mut self) -> CommandResult {
        let Some(s) = self.human_sl.session.as_ref() else {
            return CommandResult::Unchanged;
        };
        if s.phase != HumanSlPhase::Playing {
            return CommandResult::Unchanged;
        }
        match s.settings.mode {
            TrainingMode::LiveAnalysis => return CommandResult::Unchanged,
            TrainingMode::PostGameReview => {
                self.human_sl.notices.push(HumanSlNotice::AnalysisHidden);
                return CommandResult::Done;
            }
            TrainingMode::LiveFeedback => {}
        }
        if !self.human_sl_can_hint() {
            return CommandResult::Unchanged;
        }
        match self.human_sl_to_game() {
            GameShown::Ended => return CommandResult::Done,
            GameShown::Returned => {
                self.human_sl.notices.push(HumanSlNotice::BackToLiveGame);
                return CommandResult::Done;
            }
            GameShown::Already => {}
        }
        let size = self.doc.record.size;
        if let Some(s) = self.human_sl.session.as_mut() {
            let live = s.live;
            if let Some(r) = s.review.as_mut() {
                r.hint(live, size);
            }
        }
        CommandResult::Done
    }

    /// A hint can be asked for: 边下边提醒, the game on, the player to move and no reply held
    /// (`canHint`).
    fn human_sl_can_hint(&self) -> bool {
        self.human_sl_game_on()
            && self.human_sl.session.as_ref().is_some_and(|s| {
                s.review.is_some() && s.request.is_none() && self.doc.record.node(s.live).to_play == s.human
            })
    }

    /// The player's answer while the AI holds its reply to a 恶手: 重下这手 is the 悔棋 of the
    /// 恶手, 看 AI 推荐 marks KataGo's choice before it, and 不重下，继续对局 plays the reply, the
    /// time of the decision counting for the player (`retryBlunder`, `showBestMove`,
    /// `continueAfterBlunder`).
    fn human_sl_decide(&mut self, choice: HumanSlDecision) -> CommandResult {
        let held = self.human_sl_game_on()
            && self
                .human_sl
                .session
                .as_ref()
                .is_some_and(|s| s.request.as_ref().is_some_and(|r| r.phase == Search::Held));
        if !held {
            return CommandResult::Unchanged;
        }
        let size = self.doc.record.size;
        let now = Instant::now();
        match choice {
            HumanSlDecision::Retry => self.human_sl_take_back(),
            HumanSlDecision::ShowBest => {
                let s = self.human_sl.session.as_mut().unwrap();
                if let Some(r) = s.review.as_mut() {
                    r.assist.show_best(size);
                }
                CommandResult::Done
            }
            HumanSlDecision::PlayOn => {
                let s = self.human_sl.session.as_mut().unwrap();
                s.human_ms = s.human_ms.saturating_add(now.saturating_duration_since(s.turn).as_millis() as u64);
                s.turn = now;
                if let Some(r) = s.review.as_mut() {
                    r.assist.clear_turn();
                }
                self.human_sl_apply_ai(now);
                CommandResult::Done
            }
        }
    }

    /// The game is being played: the session plays, KataGo is not counting it and it has no
    /// result yet.
    fn human_sl_game_on(&self) -> bool {
        self.human_sl
            .session
            .as_ref()
            .is_some_and(|s| s.phase == HumanSlPhase::Playing && !s.counting && s.result.is_none())
    }

    /// The player resigns, also while the AI thinks: the AI wins and the game stays on the board
    /// for review (`HumanSlGameController.resign`). From a browsed position the board only goes
    /// back to the game's.
    fn human_sl_resign(&mut self) -> CommandResult {
        if !self.human_sl_game_on() {
            return CommandResult::Unchanged;
        }
        match self.human_sl_to_game() {
            GameShown::Ended => return CommandResult::Done,
            GameShown::Returned => {
                self.human_sl.notices.push(HumanSlNotice::BackToLiveGame);
                return CommandResult::Done;
            }
            GameShown::Already => {}
        }
        let winner = self.human_sl.session.as_ref().unwrap().human.opposite();
        self.human_sl_end_game(HumanSlResult::Resigned { winner }, Instant::now());
        CommandResult::Done
    }

    /// 悔棋 (`HumanSlGameController.takeBack`): the game's position goes back before the
    /// player's last move and the AI's reply, or only the player's move while the AI thinks or
    /// its reply waits for its think time, which is dropped. The moves stay in the record as the
    /// game's withdrawn continuations; the board goes back with the game, and only an analysing
    /// foreground engine follows it (in the hidden modes it is not told). From a browsed position
    /// the board only goes back to the game's. The time of the turn withdrawn counts for nobody.
    fn human_sl_take_back(&mut self) -> CommandResult {
        if !self.human_sl_game_on() {
            return CommandResult::Unchanged;
        }
        match self.human_sl_to_game() {
            GameShown::Ended => return CommandResult::Done,
            GameShown::Returned => {
                self.human_sl.notices.push(HumanSlNotice::BackToLiveGame);
                return CommandResult::Done;
            }
            GameShown::Already => {}
        }
        let moves = self.human_sl_take_back_moves();
        if moves == 0 {
            return CommandResult::Unchanged;
        }
        // A reply on its way, or chosen and waiting, belongs to the position taken back.
        self.human_sl_drop_request();
        let now = Instant::now();
        let record = &self.doc.record;
        let Some(s) = self.human_sl.session.as_mut() else {
            return CommandResult::Unchanged;
        };
        s.taken_back.capture(record, s.live, moves);
        let mut target = s.live;
        for _ in 0..moves {
            target = record.parent(target).expect("a withdrawn move has a parent");
        }
        s.live = target;
        s.take_backs += 1;
        s.turn = now;
        if let Some(r) = s.review.as_mut() {
            r.assist.clear_turn();
        }
        // What was said about the withdrawn moves goes with them, and so does a review of them
        // still running (`takenBack`).
        self.human_sl_review_taken_back(now);
        self.move_to(target);
        self.human_sl_review_turn(now);
        CommandResult::Done
    }

    /// How many moves 悔棋 withdraws now (`takeBackMoves`); none outside the game's line.
    fn human_sl_take_back_moves(&self) -> usize {
        let Some(s) = self.human_sl.session.as_ref() else {
            return 0;
        };
        let record = &self.doc.record;
        if s.doc != self.doc.id || !record.contains(s.live) {
            return 0;
        }
        let path = record.path_to(s.live);
        let Some(start) = path.iter().position(|&n| n == s.start_node) else {
            return 0;
        };
        take_back::moves(record.node(s.live).to_play == s.human, path.len() - 1 - start)
    }

    /// Both players passed one after the other since the game started
    /// (`HumanSlGameController.bothPlayersPassed`).
    fn human_sl_both_passed(&self) -> bool {
        let Some(s) = self.human_sl.session.as_ref() else {
            return false;
        };
        let record = &self.doc.record;
        let pass = |node: NodeId| matches!(record.node(node).kind, NodeKind::Pass { .. });
        s.live != s.start_node
            && pass(s.live)
            && record
                .parent(s.live)
                .is_some_and(|previous| previous != s.start_node && pass(previous))
    }

    /// Both players passed: KataGo counts the game, and then it is over
    /// (`HumanSlGameController.countAndEnd`). The clocks stop.
    fn human_sl_count(&mut self, now: Instant) {
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        s.counting = true;
        s.turn = now;
        self.human_sl.notices.push(HumanSlNotice::BothPassed);
        self.human_sl_request(Search::Count, now);
    }

    /// KataGo's count arrived (`response`), or failed or timed out (`None`): the result rounds
    /// its score lead to the game's komi, giving a draw to the first player to pass when the rules
    /// the count was asked under have KataGo's button. Without a count the game is not counted.
    fn human_sl_counted(&mut self, response: Option<&Value>, now: Instant) {
        let Some((node, komi, button)) = self
            .human_sl
            .session
            .as_ref()
            .and_then(|s| s.request.as_ref().map(|r| (r.node, s.komi, r.button)))
        else {
            return;
        };
        let first_passer = button.then(|| scoring::first_passer(&self.doc.record, node)).flatten();
        let black_to_play = self.doc.record.node(node).to_play == Stone::Black;
        let result = response
            .and_then(|r| r["rootInfo"]["scoreLead"].as_f64())
            .map(|lead| if black_to_play { lead } else { -lead })
            .and_then(|lead| scoring::verdict_of_lead(lead, komi, first_passer))
            .map_or(HumanSlResult::Uncounted, |verdict| HumanSlResult::Counted { verdict });
        self.human_sl_end_game(result, now);
    }

    /// The game is over (`HumanSlGameController.endGame`): no more moves or replies, the clocks
    /// stop, and a counted result or a resignation goes into the record's result (`RE`) through
    /// the document; a game that was not counted leaves it as it was. The session stays on.
    fn human_sl_end_game(&mut self, result: HumanSlResult, now: Instant) {
        let to_play = self
            .human_sl
            .session
            .as_ref()
            .map(|s| self.doc.record.node(s.live).to_play);
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        if !s.counting {
            let elapsed = now.saturating_duration_since(s.turn).as_millis() as u64;
            // A held reply waits for the player's decision.
            let held = s.request.as_ref().is_some_and(|r| r.phase == Search::Held);
            if to_play == Some(s.human) || held {
                s.human_ms = s.human_ms.saturating_add(elapsed);
            } else {
                s.ai_ms = s.ai_ms.saturating_add(elapsed);
            }
        }
        s.turn = now;
        s.counting = false;
        s.failed = false;
        s.request = None;
        s.generation += 1;
        let side = |winner: Stone| if winner == Stone::Black { "B+" } else { "W+" };
        let record = match &result {
            HumanSlResult::Counted { verdict } => Some(scoring::sgf_result(verdict)),
            HumanSlResult::Resigned { winner } => Some(format!("{}R", side(*winner))),
            HumanSlResult::Uncounted => None,
        };
        s.result = Some(result);
        // What the game withdrew is the game's alone.
        s.taken_back = Default::default();
        if let Some(r) = s.review.as_mut() {
            r.assist.clear_turn();
        }
        if let Some(result) = record {
            let _ = self.apply_edit(DocumentEdit::SetResult { result });
        }
    }

    /// Shows the game's position again after the player browsed (Java `returnToGamePosition`).
    /// The session ends instead when the game's document was replaced or its moves deleted.
    fn human_sl_to_game(&mut self) -> GameShown {
        let Some(s) = self.human_sl.session.as_ref() else {
            return GameShown::Ended;
        };
        let live = s.live;
        if s.doc != self.doc.id || !self.doc.record.contains(live) {
            self.human_sl_lost_game();
            return GameShown::Ended;
        }
        if self.browse_to(live) {
            GameShown::Returned
        } else {
            GameShown::Already
        }
    }

    /// The game's position is gone or changed: the session ends, saying so when the game's moves
    /// were deleted (Java `gameMovesDeleted`); a replaced document ends it silently.
    fn human_sl_lost_game(&mut self) {
        if self
            .human_sl
            .session
            .as_ref()
            .is_some_and(|s| s.doc == self.doc.id && !self.doc.record.contains(s.live))
        {
            self.human_sl.notices.push(HumanSlNotice::GameMovesDeleted);
        }
        self.human_sl_exit(true);
    }

    /// Plays `point` (`None` a pass) for the side to move at the game's position, which the board
    /// shows, and moves the game on to it. A move this game withdrew is followed again with what
    /// came after it (`replayMatching`); a different one takes the slot the game's line had
    /// among the position's continuations, the withdrawn line staying beside it (`resumeWith`).
    fn human_sl_commit(&mut self, point: Option<Point>) -> bool {
        let Some(s) = self.human_sl.session.as_ref() else {
            return false;
        };
        let at = s.live;
        let force = s.training_start_node == Some(at);
        let color = self.doc.record.node(at).to_play;
        let kind = match point {
            Some(point) => NodeKind::Move { color, point },
            None => NodeKind::Pass { color },
        };
        let node = match s.taken_back.replay(&self.doc.record, at, kind) {
            Some(node) => node,
            None => {
                let rules = self.play_rules();
                let edit = match (point, force) {
                    (Some(point), true) => DocumentEdit::PlayBranch {
                        at,
                        color,
                        point,
                        rules,
                    },
                    (Some(point), false) => DocumentEdit::Play {
                        at,
                        color,
                        point,
                        alternate: false,
                        rules,
                    },
                    (None, true) => DocumentEdit::PassBranch { at, color },
                    (None, false) => DocumentEdit::Pass { at, color },
                };
                let Ok(applied) = self.apply_edit(edit) else {
                    return false;
                };
                let Some(node) = applied.node else { return false };
                node
            }
        };
        let Some(s) = self.human_sl.session.as_mut() else {
            return false;
        };
        if let Some(slot) = s.taken_back.resume(&self.doc.record, at, node) {
            while let Some(index) = self.doc.record.node(at).children.iter().position(|&c| c == node) {
                if index == slot
                    || !self
                        .apply_edit(DocumentEdit::MoveBranch { node, up: index > slot })
                        .is_ok_and(|a| a.changed)
                {
                    break;
                }
            }
        }
        self.move_to(node);
        if let Some(s) = self.human_sl.session.as_mut() {
            s.live = node;
        }
        true
    }

    fn human_sl_exit(&mut self, resume: bool) {
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        if s.phase == HumanSlPhase::Stopping {
            return;
        }
        s.resume |= resume && s.started_game;
        s.phase = HumanSlPhase::Stopping;
        s.generation += 1;
        s.request = None;
        // The ratings end with the session: an answer still to come finds nothing (`close`).
        s.review = None;
        if let Some(worker) = s.worker.take() {
            self.engines.terminate(worker);
        }
        self.paused = true;
        self.engines.want(None, false);
        self.engines.want_secondary(None, false);
        // An analysis that borrowed the foreground engine beside AI 陪练 (Java admits it, T27)
        // still holds it and gives it back itself: the companion never used it, so the session
        // ends without a handback of its own.
        if self.engines.exclusive_gtp_availability() == LeaseAvailability::ExistingLease {
            self.human_sl_ended();
            return;
        }
        let target = self.analysis_target();
        match self.engines.restore_human_sl(&target) {
            Ok(Some(id)) => self.human_sl.session.as_mut().unwrap().restore = Some(id),
            Ok(None) => self.human_sl_ended(),
            Err(_) => self.human_sl_cleanup_failed(),
        }
    }
    fn human_sl_cleanup_failed(&mut self) {
        if let Some(s) = self.human_sl.session.as_mut() {
            s.phase = HumanSlPhase::CleanupPending;
            s.restore = None;
        }
        self.human_sl.notices.push(HumanSlNotice::ExitRecoveryFailed);
    }
    fn human_sl_queue_exit(&mut self, continuation: ExitContinuation) -> CommandResult {
        if self.human_sl.continuation.is_some() {
            return CommandResult::Unchanged;
        }
        self.human_sl.continuation = Some(continuation);
        if !self.human_sl.session.as_ref().is_some_and(|s| s.phase == HumanSlPhase::CleanupPending) {
            self.human_sl_exit(true);
        }
        CommandResult::Done
    }

    fn human_sl_ended(&mut self) {
        let Some(s) = self.human_sl.session.take() else { return };
        self.engines.set_human_sl(false);
        // Analysis resumes on the finished game after the modes that hid it.
        let resume_review =
            self.human_sl.continuation.is_none() && s.resume && s.settings.mode != TrainingMode::LiveAnalysis;
        self.paused = if resume_review { false } else { s.old_paused };
        self.limit_stop = if resume_review { None } else { s.old_limit_stop };
        self.secondary_limit_stop = if resume_review { None } else { s.old_secondary_limit_stop };
        self.sync_engine();
        self.human_sl.notices.push(HumanSlNotice::Ended);
        match self.human_sl.continuation.take() {
            Some(ExitContinuation::Ui(continuation)) => {
                self.human_sl.ready_continuation = Some(continuation);
            }
            Some(ExitContinuation::ReviewAt { doc, node }) => {
                if let Some(node) = node.filter(|&n| doc == self.doc.id && self.doc.record.contains(n)) {
                    self.browse_to(node);
                }
            }
            command => self.human_sl.continuation = command,
        }
    }

    /// After the game in 边下边提醒: ends the session and then shows the game's position before
    /// move `move_number`, found going back from the game's position no further than its start
    /// (`reviewFrom`). Without that position the session still ends.
    fn human_sl_review_from(&mut self, move_number: u32) -> CommandResult {
        let Some(s) = self.human_sl.session.as_ref() else {
            return CommandResult::Unchanged;
        };
        if s.phase != HumanSlPhase::Playing || s.result.is_none() || s.settings.mode != TrainingMode::LiveFeedback {
            return CommandResult::Unchanged;
        }
        let record = &self.doc.record;
        let mut node = (s.doc == self.doc.id && record.contains(s.live)).then_some(s.live);
        let target = loop {
            let Some(n) = node else { break None };
            if record.node(n).move_number + 1 == move_number {
                break Some(n);
            }
            if n == s.start_node {
                break None;
            }
            node = record.parent(n);
        };
        let doc = s.doc;
        self.human_sl_queue_exit(ExitContinuation::ReviewAt { doc, node: target })
    }

    /// The game's line: its start, then each position up to the game's position; empty when the
    /// game's document is no longer open.
    fn human_sl_game_line(&self, s: &Session) -> Vec<NodeId> {
        let record = &self.doc.record;
        if s.doc != self.doc.id || !record.contains(s.live) {
            return Vec::new();
        }
        let mut path = record.path_to(s.live);
        let start = path.iter().position(|&n| n == s.start_node).unwrap_or(0);
        path.drain(..start);
        path
    }

    /// The player's last move on the game's `line`, rated in 边下边提醒 (`lastHumanMove`,
    /// `CoachPanelState.moveCard`).
    fn human_sl_last_move(&self, s: &Session, line: &[NodeId]) -> Option<HumanSlLastMove> {
        let record = &self.doc.record;
        line.iter().skip(1).rev().find_map(|&node| {
            let n = record.node(node);
            let vertex = match n.kind {
                NodeKind::Move { color, point } if color == s.human => gtp::vertex(record.size, point),
                NodeKind::Pass { color } if color == s.human => "pass".to_string(),
                _ => return None,
            };
            Some(HumanSlLastMove {
                move_number: n.move_number,
                vertex,
                verdict: s.review.as_ref().and_then(|r| r.verdict_of(n.move_number)),
            })
        })
    }

    /// The player's win chance through the game in percent (`humanWinrates`): 边下边提醒's
    /// rank-aware chances, or in 边下边分析 the foreground analysis of each analysed position on the
    /// game's `line`, turned to the player's side.
    fn human_sl_win_chances(&self, s: &Session, line: &[NodeId], ratings: Option<&[humansl::feedback::MoveFeedback]>) -> Option<Vec<f64>> {
        match s.settings.mode {
            TrainingMode::LiveFeedback => {
                let turn = s.review.as_ref().and_then(|r| r.turn_review_at(s.live));
                Some(humansl::feedback::win_chances(ratings.unwrap_or_default(), turn))
            }
            TrainingMode::LiveAnalysis => Some(
                line.iter()
                    .filter_map(|&n| self.store.get(&self.doc.record, n))
                    .filter(|a| a.visits > 0)
                    .map(|a| if a.to_play == s.human { a.winrate } else { 100.0 - a.winrate })
                    .collect(),
            ),
            TrainingMode::PostGameReview => None,
        }
    }
    pub fn take_human_sl_deferred(&mut self) -> Option<Command> {
        if self.human_sl_active() || !matches!(self.human_sl.continuation, Some(ExitContinuation::Command(_))) {
            return None;
        }
        match self.human_sl.continuation.take() {
            Some(ExitContinuation::Command(command)) => Some(*command),
            _ => None,
        }
    }

    pub(super) fn take_human_sl_continuation(&mut self) -> Option<HumanSlContinuation> {
        self.human_sl.ready_continuation.take()
    }

    pub(super) fn human_sl_continuation(&self) -> Option<HumanSlContinuation> {
        self.human_sl.ready_continuation
    }

    pub(super) fn human_sl_analyse(&self) -> Option<bool> {
        self.human_sl
            .session
            .as_ref()
            .map(|s| s.phase == HumanSlPhase::Playing && s.settings.mode == TrainingMode::LiveAnalysis)
    }
    /// The session's view; `board_captures` are the captures of the position the board shows,
    /// the cursor's.
    pub(super) fn human_sl_view(&self, board_captures: (u32, u32)) -> Option<HumanSlView> {
        let s = self.human_sl.session.as_ref()?;
        // While KataGo counts and after the game, nobody is to move and the clocks stand.
        let playing = s.phase == HumanSlPhase::Playing && !s.counting && s.result.is_none();
        let human_turn = playing && self.doc.record.node(s.live).to_play == s.human;
        // While the AI holds its reply to a 恶手, the player decides on their clock.
        let held = s.request.as_ref().is_some_and(|r| r.phase == Search::Held);
        let line = self.human_sl_game_line(s);
        // The game's position, not the one the board shows while the player browses; at the
        // board's own node its captures are already known.
        let captures = line.last().map(|&live| {
            if live == self.cursor {
                board_captures
            } else {
                let position = self.doc.record.position(live);
                (position.black_captures, position.white_captures)
            }
        });
        let feedback = self.human_sl_feedback_view(s);
        Some(HumanSlView {
            id: s.id,
            phase: s.phase,
            game_started: s.started_game,
            stage: (s.phase == HumanSlPhase::Preparing).then_some(s.stage),
            preparing_secs: s.start.elapsed().as_secs().min(u64::from(u32::MAX)) as u32,
            mode: s.settings.mode,
            level: s.settings.level,
            human_is_black: s.human == Stone::Black,
            human_ms: s.human_ms,
            ai_ms: s.ai_ms,
            clock: playing.then_some(if human_turn || held {
                HumanSlSide::Human
            } else {
                HumanSlSide::Ai
            }),
            human_turn,
            ai_thinking: s.request.as_ref().is_some_and(|r| r.phase != Search::Count && r.phase != Search::Held),
            ai_failed: s.failed,
            analysis_display: if s.settings.mode == TrainingMode::LiveAnalysis {
                HumanSlAnalysisDisplay::Show
            } else {
                HumanSlAnalysisDisplay::Suppress
            },
            counting: s.counting,
            result: s.result.clone(),
            // canTakeBack: also while the AI thinks, not while KataGo counts or after the game.
            can_take_back: playing && self.human_sl_take_back_moves() > 0,
            take_backs: s.take_backs,
            last_move: self.human_sl_last_move(s, &line),
            // movesPlayed: the positions after the game's start, passes included.
            moves: line.len().saturating_sub(1) as u32,
            komi: s.komi,
            black_captures: captures.map_or(0, |c| c.0),
            white_captures: captures.map_or(0, |c| c.1),
            win_chances: self.human_sl_win_chances(s, &line, feedback.as_ref().map(|f| f.ratings.as_slice())),
            feedback,
        })
    }
    pub(super) fn human_sl_notices(&self) -> Vec<HumanSlNotice> {
        self.human_sl.notices.clone()
    }
    pub(super) fn take_human_sl_notices(&mut self) -> Vec<HumanSlNotice> {
        std::mem::take(&mut self.human_sl.notices)
    }

    pub(super) fn human_sl_gate(&mut self, command: &Command) -> Option<CommandResult> {
        if !self.human_sl_active() {
            return None;
        }
        let defer = matches!(
            command,
            Command::ChooseAndOpenRecord
                | Command::OpenRecord { .. }
                | Command::PasteRecord { .. }
                | Command::Recovery {
                    op: RecoveryCommand::LoadSlot { .. }
                }
                | Command::Game {
                    op: GameCommand::Start { .. } | GameCommand::Continue { .. }
                }
                | Command::Match {
                    op: MatchCommand::Start { .. }
                }
                | Command::Edit {
                    op: EditCommand::NewBoard { .. } | EditCommand::ClearBoard | EditCommand::SetBoardSize { .. }
                }
        );
        if defer {
            if self.human_sl.continuation.is_some() {
                return Some(CommandResult::Unchanged);
            }
            return Some(self.human_sl_queue_exit(ExitContinuation::Command(Box::new(command.clone()))));
        }
        match command {
            Command::HumanSl { .. } => None,
            Command::Edit {
                op: EditCommand::Play { .. } | EditCommand::Pass,
            } => None,
            Command::Edit { op } => Some(human_sl_edit_refusal(op)),
            // H (Java `Input` with `activeCoachGame`): from the game's start until the session is
            // over, a mode that hides analysis keeps the raw network off (H asks for a hint there);
            // 实时分析, the preparation and a session that never started a game leave it to the
            // player.
            Command::ToggleRawNet => self
                .human_sl
                .session
                .as_ref()
                .is_some_and(|s| s.started_game && s.settings.mode != TrainingMode::LiveAnalysis)
                .then(|| refused("AnalysisSettings.reuseStatus.human_sl_game")),
            // A click on the players' names toggles their win rates, also during the game (Java
            // `Input` hands the game's clicks to `LizzieFrame.onClicked`, whose
            // `togglePlayerNameWinratesAt` comes first); the other display settings stay put.
            Command::SetDisplaySettings { settings }
                if crate::settings::DisplaySettings {
                    name_winrates: self.config.display.name_winrates,
                    ..settings.clone()
                } == self.config.display =>
            {
                None
            }
            Command::SetKomi { .. }
            | Command::SetKataRules { .. }
            | Command::SwitchEngine { .. }
            | Command::CloudActivate { .. }
            | Command::CloudLocal
            | Command::CloudCancel
            | Command::RestartEngine
            | Command::ShutdownEngines { .. }
            | Command::SaveEngineSettings { .. }
            | Command::SendGtpCommand { .. }
            | Command::SaveAnalysisEngineSettings { .. }
            | Command::SetEditSettings { .. }
            | Command::SetDisplaySettings { .. }
            | Command::SetExtraMode { .. }
            | Command::SetClassicMode
            | Command::Score { .. }
            | Command::ToggleAnalysis
            | Command::StartFlashAnalysis { .. }
            | Command::StartAutoAnalysis { .. }
            | Command::StartWholeGameAnalysis { .. }
            | Command::ResumeWholeGameAnalysis
            | Command::Region { .. }
            | Command::Tracking { .. }
            | Command::Batch { .. }
            | Command::Tuning { .. }
            | Command::Game { .. }
            | Command::Match { .. } => Some(refused("AnalysisSettings.reuseStatus.human_sl_game")),
            _ => None,
        }
    }
}

fn fixed_handicap(handicap: u8) -> impl Iterator<Item = Point> {
    let points: &'static [(u16, u16)] = match handicap {
        2 => &[(3, 15), (15, 3)],
        3 => &[(3, 3), (15, 3), (3, 15)],
        4 => &[(3, 3), (3, 15), (15, 3), (15, 15)],
        5 => &[(3, 3), (3, 15), (15, 3), (15, 15), (9, 9)],
        6 => &[(3, 3), (3, 15), (15, 3), (15, 15), (3, 9), (15, 9)],
        7 => &[(3, 3), (3, 15), (15, 3), (15, 15), (15, 9), (3, 9), (9, 9)],
        8 => &[(3, 3), (3, 15), (15, 3), (15, 15), (9, 3), (9, 15), (3, 9), (15, 9)],
        9 => &[
            (3, 3),
            (3, 15),
            (15, 3),
            (15, 15),
            (9, 3),
            (9, 15),
            (3, 9),
            (15, 9),
            (9, 9),
        ],
        _ => &[],
    };
    points.iter().map(|&(x, y)| Point { x, y })
}

fn redact_detail(detail: &str) -> String {
    let safe = crate::sanitize::sanitize(detail);
    safe.lines()
        .map(|line| {
            line.split_whitespace()
                .map(|word| {
                    if word.starts_with('/') || word.as_bytes().get(1) == Some(&b':') {
                        word.rsplit(['/', '\\']).next().unwrap_or("[path]")
                    } else {
                        word
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
