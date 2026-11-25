//! HumanSL owns a training session, not the user's committed display settings. The companion
//! speaks analysis JSON; the foreground engine is only paused and later resynchronized.
mod ai;
use super::exporting::java_text;
use super::Workbench;
use crate::api::*;
use crate::engine::{
    gtp,
    rules::{KataRules, RuleSet},
    runtime::{InstanceId, LeaseAvailability, RuntimeEvent},
};
use crate::humansl::{self, protocol, CoachLevelKind};
use crate::record::{DocumentEdit, NodeId, NodeKind, Point, Stone};
use crate::scoring;
use crate::settings::{HumanSlTrainingSettings, PlayerColor, TrainingMode};
use serde_json::Value;
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
}
/// What the companion is asked: readiness, a policy level's one-visit probe and, when the AI may
/// pass, KataGo's own pass check, or a search level's search; then the reply waits for its pace.
/// Once both players passed, KataGo's count of the game.
#[derive(PartialEq)]
enum Search {
    Ready,
    Probe,
    PassCheck,
    Play,
    Pacing,
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
        let base = self.analysis_command_base(false);
        let command = match self.human_sl_command(&base, &model) {
            Ok(c) => c,
            Err(reason) => {
                self.human_sl.notices.push(HumanSlNotice::StartFailed {
                    reason: reason.into(),
                    detail: None,
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
            Err(detail) => {
                self.engines.set_human_sl(false);
                self.paused = old_paused;
                self.sync_engine();
                self.human_sl.notices.push(HumanSlNotice::StartFailed {
                    reason: "HumanSlTraining.preparing.failedDetail".into(),
                    detail: Some(redact_detail(&detail)),
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
            self.human_sl_even_komi()
        };
        let resignation = humansl::Resignation::for_play(settings.level.play());
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
    /// are; else, after a handicap game, the komi new games start with; else the current komi.
    pub(super) fn human_sl_even_komi(&self) -> f64 {
        let rules = self.engines.foreground().and_then(|f| self.engines.confirmed_rules(f));
        RuleSet::even_game_komi(rules, self.komi, self.doc.record.info.handicap > 0, None)
    }

    fn human_sl_command(&self, base: &str, model: &std::path::Path) -> Result<String, &'static str> {
        if base.trim() == crate::settings::DEFAULT_ANALYSIS_COMMAND {
            return Err("AnalysisEngineCommandHelper.noEngineSelected");
        }
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
        if s.request.is_some() || self.doc.record.node(s.live).to_play != s.human || !self.human_sl_commit(point) {
            return CommandResult::Unchanged;
        }
        let now = Instant::now();
        let s = self.human_sl.session.as_mut().unwrap();
        s.human_ms = s.human_ms.saturating_add(now.duration_since(s.turn).as_millis() as u64);
        s.turn = now;
        if point.is_none() && self.human_sl_both_passed() {
            self.human_sl_count(now);
        } else {
            self.human_sl_schedule(now);
        }
        CommandResult::Done
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
        let Some((node, komi)) = self
            .human_sl
            .session
            .as_ref()
            .and_then(|s| Some((s.request.as_ref()?.node, s.komi)))
        else {
            return;
        };
        let rules = match self.quick_rules() {
            Value::String(name) => KataRules::parse(&name),
            rules => KataRules::parse(&rules.to_string()),
        };
        let first_passer = rules
            .is_some_and(|r| r.has_button())
            .then(|| scoring::first_passer(&self.doc.record, node))
            .flatten();
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
            if to_play == Some(s.human) {
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
    /// shows, and moves the game on to it.
    fn human_sl_commit(&mut self, point: Option<Point>) -> bool {
        let Some(s) = self.human_sl.session.as_ref() else {
            return false;
        };
        let at = s.live;
        let force = s.training_start_node == Some(at);
        let color = self.doc.record.node(at).to_play;
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
        let resume_review =
            self.human_sl.continuation.is_none() && s.resume && s.settings.mode == TrainingMode::PostGameReview;
        self.paused = if resume_review { false } else { s.old_paused };
        self.limit_stop = if resume_review { None } else { s.old_limit_stop };
        self.secondary_limit_stop = if resume_review { None } else { s.old_secondary_limit_stop };
        self.sync_engine();
        self.human_sl.notices.push(HumanSlNotice::Ended);
        match self.human_sl.continuation.take() {
            Some(ExitContinuation::Ui(continuation)) => {
                self.human_sl.ready_continuation = Some(continuation);
            }
            command => self.human_sl.continuation = command,
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
    pub(super) fn human_sl_view(&self) -> Option<HumanSlView> {
        let s = self.human_sl.session.as_ref()?;
        // While KataGo counts and after the game, nobody is to move and the clocks stand.
        let playing = s.phase == HumanSlPhase::Playing && !s.counting && s.result.is_none();
        let human_turn = playing && self.doc.record.node(s.live).to_play == s.human;
        Some(HumanSlView {
            id: s.id,
            phase: s.phase,
            stage: (s.phase == HumanSlPhase::Preparing).then_some(s.stage),
            preparing_secs: s.start.elapsed().as_secs().min(u64::from(u32::MAX)) as u32,
            mode: s.settings.mode,
            level: s.settings.level,
            human_is_black: s.human == Stone::Black,
            human_ms: s.human_ms,
            ai_ms: s.ai_ms,
            clock: playing.then_some(if human_turn {
                HumanSlSide::Human
            } else {
                HumanSlSide::Ai
            }),
            human_turn,
            ai_thinking: s.request.as_ref().is_some_and(|r| r.phase != Search::Count),
            ai_failed: s.failed,
            analysis_display: if s.settings.mode == TrainingMode::LiveAnalysis {
                HumanSlAnalysisDisplay::Show
            } else {
                HumanSlAnalysisDisplay::Suppress
            },
            counting: s.counting,
            result: s.result.clone(),
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
            Command::Edit { .. }
            | Command::SetKomi { .. }
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
            | Command::ToggleRawNet
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
