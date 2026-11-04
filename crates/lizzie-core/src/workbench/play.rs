//! 人机对弈 (Java `isPlayingAgainstLeelaz` / `isAnaPlayingAgainstLeelaz`): a game's participants
//! (the human's colour and the reserved foreground engine), its lifecycle from accepted to given
//! back, the engine-move transaction, the human's clock and the end of the game. Also the bottom
//! toolbar's 自动落子 and Alt+, (one engine move), which share the move decision.
//!
//! The document changes only through [`Workbench::apply_edit`]. An engine move is bound to the
//! game, the document, the node (with its stamp) and the engine request it answers; anything else
//! is a late answer and changes nothing.

use std::path::Path;
use std::time::{Duration, Instant};

use super::{SaveJob, Workbench};
use crate::api::{
    ApiError, Command, CommandResult, EditCommand, ErrorKind, GameClockView, GameCommand, GameEnd, GameKind,
    GameNotice, GameOutcomeView, GamePhase, GameView, HawkeyeCommand, NodeAnalysisView, RecoveryCommand,
    TuningCommand,
};
use crate::engine::runtime::{GenmoveReply, LeaseAvailability, Phase, RuntimeEvent};
use crate::play::{self, ClockTick, EngineClock, HumanClock};
use crate::record::{DocumentEdit, NodeId, NodeKind, Point, Stone, ROOT};
use crate::settings::{AutoPlaySettings, GameSettings};

/// `AnalysisSettings.reuseStatus.play_mode`: another mode was refused during a game.
pub const PLAY_MODE: &str = "AnalysisSettings.reuseStatus.play_mode";
/// `LizzieFrame.openFileFailed.inGame`.
pub const OPEN_IN_GAME: &str = "LizzieFrame.openFileFailed.inGame";
/// `LizzieFrame.waitEngineLoadingHint`.
const WAIT_ENGINE: &str = "LizzieFrame.waitEngineLoadingHint";
const TICK: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Accepted,
    Playing,
    Paused,
    Ending,
}

/// The engine move asked for: which position it answers.
struct Request {
    id: u64,
    node: NodeId,
    stamp: u64,
    sent: Instant,
}

struct Session {
    id: u64,
    kind: GameKind,
    human: Stone,
    reservation: u64,
    step: Step,
    settings: GameSettings,
    doc: u64,
    show: (bool, bool),
    /// Setup commands still unanswered; the handicap command and the parameters being read for
    /// the restore.
    setup_sent: bool,
    awaiting: Vec<u64>,
    handicap: Option<(u64, u32)>,
    capture: Vec<(u64, &'static str)>,
    restore: Vec<String>,
    request: Option<Request>,
    next_request: u64,
    clock: Option<HumanClock>,
    next_tick: Option<Instant>,
    engine_clock: Option<EngineClock>,
    resign: u32,
    passed: bool,
    raw_net: bool,
}

/// Alt+, : one engine move outside a game.
struct Manual {
    reservation: u64,
    node: NodeId,
    stamp: u64,
    color: Stone,
    answered: bool,
}

#[derive(Default)]
pub(super) struct PlayFlow {
    session: Option<Session>,
    manual: Option<Manual>,
    next: u64,
    /// The 自动落子 box (session only, like Java's toolbar check box).
    autoplay: bool,
    /// A game asked for while the previous one gives the engine back.
    queued: Option<GameCommand>,
    outcome: Option<GameOutcomeView>,
    notices: Vec<GameNotice>,
    voices: Vec<u8>,
    jobs: Vec<SaveJob>,
    saves: Vec<u64>,
}

fn notice(key: &str) -> GameNotice {
    GameNotice { message_key: key.to_string(), prefix: None }
}

fn refused(key: &str) -> CommandResult {
    CommandResult::Refused { message_key: key.to_string() }
}

fn color_sgf(c: Stone) -> &'static str {
    if c == Stone::Black { "B" } else { "W" }
}

/// Characters a file name cannot hold on the supported systems.
fn file_safe(name: &str) -> String {
    name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c }).collect()
}

impl Workbench {
    // -------------------------------------------------------------------- queries

    pub(super) fn game_active(&self) -> bool {
        self.play.session.is_some()
    }

    /// A game or Alt+, holds the foreground engine: whether it analyses now. 终局点目 stops it
    /// (`startFinalScore`).
    pub(super) fn play_analyse(&self) -> Option<bool> {
        if let Some(s) = &self.play.session {
            let analyse = s.kind == GameKind::Analysis
                && s.step == Step::Playing
                && self.scoring.is_none()
                && (self.config.game.play_ponder || self.to_play() != s.human);
            return Some(analyse);
        }
        self.play.manual.as_ref().map(|_| false)
    }

    fn to_play(&self) -> Stone {
        self.doc.record.node(self.cursor).to_play
    }

    /// A refusal for an editing command during a game; `None` lets it run.
    pub(super) fn play_edit_refusal(&self, op: &EditCommand) -> Option<CommandResult> {
        let s = self.play.session.as_ref()?;
        match op {
            EditCommand::Play { .. } | EditCommand::Pass | EditCommand::PlayBestMove => match s.step {
                Step::Accepted => Some(refused(WAIT_ENGINE)),
                Step::Paused | Step::Ending => Some(CommandResult::Unchanged),
                Step::Playing if self.to_play() != s.human => Some(CommandResult::Unchanged),
                Step::Playing if matches!(op, EditCommand::Play { color: Some(c), .. } if *c != s.human) => {
                    Some(CommandResult::Unchanged)
                }
                Step::Playing => None,
            },
            // Comments and marks leave the game alone; a new board replaces the record, which ends
            // the game first.
            EditCommand::SetComment { .. }
            | EditCommand::ToggleMark { .. }
            | EditCommand::ClearMarks
            | EditCommand::ClearEditCache
            | EditCommand::NewBoard { .. }
            | EditCommand::ClearBoard
            | EditCommand::SetBoardSize { .. } => None,
            _ => Some(refused(PLAY_MODE)),
        }
    }

    /// What a command does instead while a game runs or gives its engine back; `None` lets it
    /// run. Opening another record is refused (`LizzieFrame.openFileFailed.inGame`), browsing is
    /// ignored (`navigateHistoryNoRefresh`), Space ends the game (`togglePonderMannul`), and
    /// commands that would change or borrow the game's engine are refused as play mode. An ended
    /// game whose engine does not confirm its restore (a move it never answers) can be released
    /// by restarting or closing that engine.
    pub fn game_gate(&self, command: &Command) -> Option<CommandResult> {
        if !self.game_active() {
            return None;
        }
        let ending = self.play.session.as_ref().is_some_and(|s| s.step == Step::Ending);
        match command {
            Command::RestartEngine | Command::ShutdownEngines { .. } if ending => None,
            Command::ChooseAndOpenRecord
            | Command::OpenRecord { .. }
            | Command::PasteRecord { .. }
            | Command::Recovery { op: RecoveryCommand::LoadSlot { .. } } => Some(refused(OPEN_IN_GAME)),
            Command::Navigate { .. }
            | Command::GotoNode { .. }
            | Command::GotoMoveNumber { .. }
            | Command::JumpToProblemMove { .. }
            | Command::Hawkeye {
                op: HawkeyeCommand::GotoRow { .. } | HawkeyeCommand::GotoMove { .. } | HawkeyeCommand::GotoMoveNumber { .. },
            } => Some(CommandResult::Unchanged),
            Command::SwitchEngine { .. }
            | Command::RestartEngine
            | Command::ShutdownEngines { .. }
            | Command::SaveEngineSettings { .. }
            | Command::SetKomi { .. }
            | Command::SetKataRules { .. }
            | Command::SendGtpCommand { .. }
            | Command::ToggleRawNet
            | Command::StopRawNet
            | Command::StartFlashAnalysis { .. }
            | Command::RedoFlashAnalysis
            | Command::StartAutoAnalysis { .. }
            | Command::StartWholeGameAnalysis { .. }
            | Command::ResumeWholeGameAnalysis
            | Command::Region { .. }
            | Command::Tracking { .. }
            | Command::Batch { .. }
            | Command::Tuning { op: TuningCommand::StartBenchmark { .. } } => Some(refused(PLAY_MODE)),
            Command::Edit { op } => self.play_edit_refusal(op),
            _ => None,
        }
    }

    // -------------------------------------------------------------------- commands

    pub fn game(&mut self, op: GameCommand) -> Result<CommandResult, ApiError> {
        match op {
            GameCommand::Start { kind, settings, autoplay, engine } => self.game_start(kind, settings, autoplay, engine),
            GameCommand::Continue { kind, human } => self.game_continue(kind, human),
            GameCommand::Pause => Ok(self.game_pause()),
            GameCommand::Resign => {
                let Some(s) = self.play.session.as_ref().filter(|s| matches!(s.step, Step::Playing | Step::Paused)) else {
                    return Ok(CommandResult::Unchanged);
                };
                let result = format!("{}+R", color_sgf(s.human.opposite()));
                self.game_end(GameEnd::HumanResigned, Some(result));
                Ok(CommandResult::Done)
            }
            GameCommand::End => Ok(if self.game_stop() { CommandResult::Done } else { CommandResult::Unchanged }),
            GameCommand::Regret => Ok(self.game_regret()),
            GameCommand::SetAiTime { time, ponder, autoplay } => {
                self.commit_config(|c| {
                    c.game.ai_time = time;
                    c.game.play_ponder = ponder;
                })?;
                self.persisted.autoplay = autoplay;
                self.save_persisted()?;
                if self.play.session.as_ref().is_some_and(|s| s.kind == GameKind::Genmove && s.step != Step::Ending) {
                    self.send_ai_time();
                }
                self.sync_engine();
                Ok(CommandResult::Done)
            }
            GameCommand::SetRandomStart { settings } => {
                if !settings.winrate_diff.is_finite() || !settings.playouts_diff.is_finite() {
                    return Err(ApiError::new(ErrorKind::InvalidArgument, "random start"));
                }
                self.commit_config(|c| c.game.random_start = settings)?;
                Ok(CommandResult::Done)
            }
            GameCommand::EngineMove => Ok(self.manual_engine_move()),
            GameCommand::SetAutoPlay { enabled, settings } => {
                self.play.autoplay = enabled;
                if self.persisted.autoplay != settings {
                    self.persisted.autoplay = settings;
                    self.save_persisted()?;
                }
                self.sync_engine();
                Ok(CommandResult::Done)
            }
        }
    }

    /// A game asked for while one runs: that one ends, this one waits for its engine.
    fn queue_after_current(&mut self, op: GameCommand) -> bool {
        if self.play.session.is_none() && self.play.manual.is_none() {
            return false;
        }
        self.game_stop();
        self.play.queued = Some(op);
        true
    }

    fn game_start(
        &mut self,
        kind: GameKind,
        settings: GameSettings,
        autoplay: Option<AutoPlaySettings>,
        engine: Option<u32>,
    ) -> Result<CommandResult, ApiError> {
        if !settings.komi.is_finite() || !settings.resign_percent.is_finite() || ![0, 2, 3, 4, 5, 6, 7, 8, 9].contains(&settings.handicap) {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "game settings"));
        }
        let conditions = autoplay.unwrap_or(self.persisted.autoplay);
        if kind == GameKind::Analysis && !settings.pure_net && !conditions.has_condition() {
            return Ok(refused("NewAnaGameDialog.wrongAiMoveSettings"));
        }
        if self.queue_after_current(GameCommand::Start { kind, settings: settings.clone(), autoplay, engine }) {
            return Ok(CommandResult::Done);
        }
        if let Some(refusal) = self.foreground_mode_conflict() {
            return Ok(refusal);
        }
        self.commit_config(|c| c.game = settings.clone())?;
        if let Some(a) = autoplay {
            self.persisted.autoplay = a;
            self.save_persisted()?;
        }
        if let Some(index) = engine {
            self.switch_engine(index)?;
        }
        let Some(fg) = self.engines.foreground() else {
            return Ok(refused("NewAnaGameDialog.noEngineHint"));
        };
        // `startAnalyzeGameDialogReserved`: an engine that cannot analyse plays a Genmove game.
        let kind = if kind == GameKind::Analysis && self.engines.phase(fg) == Some(Phase::Ready) && !self.engines.can_analyze(fg) {
            GameKind::Genmove
        } else {
            kind
        };
        let reservation = match self.engines.reserve_game() {
            Ok(r) => r,
            Err(a) => return Ok(refused(&super::flash::reuse_status_key(a))),
        };
        let human = if settings.play_black { Stone::Black } else { Stone::White };
        let size = self.doc.record.size;
        match (kind, settings.continue_play) {
            (_, false) => self.reopen_empty_board(size),
            // `board.clear` + `setlist(moveList)`: a new record with the moves up to here.
            (GameKind::Genmove, true) => self.replay_to_new_board(),
            (GameKind::Analysis, true) => {}
        }
        self.komi_changed = true;
        self.start_komi = None;
        self.apply_komi(settings.komi);
        let me = super::exporting::java_text(self.active_language, "NewAnaGameDialog.me");
        let engine_name = self.engines.engine_name(fg).unwrap_or_default();
        let (black, white) = if human == Stone::Black { (me, engine_name) } else { (engine_name, me) };
        let _ = self.edit_document(DocumentEdit::SetPlayers { black, white });
        let handicap = (!settings.continue_play && settings.handicap >= 2).then_some(settings.handicap);
        self.begin_session(kind, human, reservation, settings.clone(), (settings.show_black, settings.show_white), handicap);
        Ok(CommandResult::Done)
    }

    /// 人机续弈 (`continueAiPlaying`): from the current position with the saved settings.
    fn game_continue(&mut self, kind: GameKind, human: Option<Stone>) -> Result<CommandResult, ApiError> {
        if self.queue_after_current(GameCommand::Continue { kind, human }) {
            return Ok(CommandResult::Done);
        }
        let Some(fg) = self.engines.foreground() else {
            return Ok(CommandResult::Unchanged);
        };
        if let Some(refusal) = self.foreground_mode_conflict() {
            return Ok(refusal);
        }
        // An engine that cannot analyse continues in Genmove mode (Menu.java:3038, 3054).
        let kind = if kind == GameKind::Analysis && self.engines.phase(fg) == Some(Phase::Ready) && !self.engines.can_analyze(fg) {
            GameKind::Genmove
        } else {
            kind
        };
        if kind == GameKind::Analysis && !self.persisted.autoplay.has_condition() {
            // LizzieFrame.java:13586-13592.
            self.persisted.autoplay.time_enabled = true;
            self.persisted.autoplay.time_seconds = self.config.game.ai_time.fixed_seconds.max(1) as i32;
            self.save_persisted()?;
        }
        let reservation = match self.engines.reserve_game() {
            Ok(r) => r,
            Err(a) => return Ok(refused(&super::flash::reuse_status_key(a))),
        };
        // Enter / Alt+Enter (`continueNow`, `fromShortCut`): the engine plays the side to move.
        let shortcut = human.is_none();
        let human = human.unwrap_or(self.to_play().opposite());
        let settings = self.config.game.clone();
        self.begin_session(kind, human, reservation, settings, (false, false), None);
        if shortcut {
            self.play.notices.push(notice("LizzieFrame.startContinueGame"));
        }
        Ok(CommandResult::Done)
    }

    /// Another mode holding the foreground engine refuses a game (`showForegroundEngineLeaseConflict`).
    fn foreground_mode_conflict(&self) -> Option<CommandResult> {
        let busy = self.quick_holds_foreground()
            || self.flash_holds_foreground()
            || self.whole_game_holds_foreground()
            || self.batch_holds_foreground()
            || self.auto_analysis_active();
        busy.then(|| refused(&super::flash::reuse_status_key(LeaseAvailability::ExistingLease)))
    }

    /// A new record holding the root position and the moves up to the current node.
    fn replay_to_new_board(&mut self) {
        let rec = self.doc.record.clone();
        let path = rec.path_to(self.cursor);
        let start = rec.position(ROOT);
        let root_to_play = rec.node(ROOT).to_play;
        self.reopen_empty_board(rec.size);
        let w = rec.size.width as usize;
        for (i, stone) in start.stones.iter().enumerate() {
            if let Some(c) = stone {
                let point = Point { x: (i % w) as u16, y: (i / w) as u16 };
                let _ = self.apply_edit(DocumentEdit::SetupStone { point, stone: Some(*c) });
            }
        }
        if root_to_play != Stone::Black {
            let _ = self.apply_edit(DocumentEdit::SetupSideToPlay { color: root_to_play });
        }
        let rules = self.play_rules();
        let mut at = ROOT;
        for &n in &path[1..] {
            let edit = match rec.node(n).kind {
                NodeKind::Move { color, point } => DocumentEdit::Play { at, color, point, alternate: false, rules },
                NodeKind::Pass { color } => DocumentEdit::Pass { at, color },
                _ => continue,
            };
            match self.apply_edit(edit).ok().and_then(|a| a.node) {
                Some(next) => at = next,
                None => break,
            }
        }
        self.cursor = at;
    }

    fn begin_session(
        &mut self,
        kind: GameKind,
        human: Stone,
        reservation: u64,
        settings: GameSettings,
        show: (bool, bool),
        handicap: Option<u32>,
    ) {
        self.play.next += 1;
        self.paused = true;
        // `stopAiPlayingAndPolicy` before a game: no pure-network display is left running.
        self.engines.stop_raw_net();
        if kind == GameKind::Analysis {
            // The engine's side of the bottom bar's 自动落子 (`chkAutoPlayBlack/White`).
            self.play.autoplay = true;
            self.persisted.autoplay.black = human == Stone::White;
            self.persisted.autoplay.white = human == Stone::Black;
            if let Err(e) = self.save_persisted() {
                self.log(true, format!("saving the 自动落子 sides failed: {}", e.message));
            }
        }
        self.play.session = Some(Session {
            id: self.play.next,
            kind,
            human,
            reservation,
            step: Step::Accepted,
            settings,
            doc: self.doc.id,
            show,
            setup_sent: false,
            awaiting: Vec::new(),
            handicap: handicap.map(|h| (0, h)),
            capture: Vec::new(),
            restore: Vec::new(),
            request: None,
            next_request: 1,
            clock: None,
            next_tick: None,
            engine_clock: None,
            resign: 0,
            passed: false,
            raw_net: false,
        });
        self.sync_engine();
        self.play_setup();
    }

    /// The game's setup once its engine is ready (`syncBoard`): time, KataGo limits, the
    /// 广度拓展 switch and the handicap; parameters it changes are read first for the restore.
    fn play_setup(&mut self) {
        let Some(s) = self.play.session.as_ref().filter(|s| s.step == Step::Accepted && !s.setup_sent) else { return };
        let (res, kind) = (s.reservation, s.kind);
        let Some(fg) = self.engines.game_instance(res).filter(|&f| self.engines.phase(f) == Some(Phase::Ready)) else {
            return;
        };
        let settings = s.settings.clone();
        let katago = self.engines.is_katago(fg);
        let readable = katago && self.engines.has_command(fg, "kata-get-param");
        let mut capture: Vec<&'static str> = Vec::new();
        let mut set: Vec<String> = Vec::new();
        match kind {
            GameKind::Genmove => {
                let time = play::ai_time_commands(&settings.ai_time, katago);
                if time.commands.iter().any(|c| c.starts_with("kata-set-param maxTime")) {
                    capture.push("maxTime");
                }
                set.extend(time.commands);
                if settings.kata_limits && katago {
                    capture.extend(["maxVisits", "maxPlayouts"]);
                    let limit = |v: i64| if v > 0 { v.to_string() } else { "1125899906842624".to_string() };
                    set.push(format!("kata-set-param maxVisits {}", limit(settings.kata_visits)));
                    set.push(format!("kata-set-param maxPlayouts {}", limit(settings.kata_playouts)));
                }
            }
            GameKind::Analysis => {
                if settings.disable_wrn && katago {
                    capture.push("analysisWideRootNoise");
                    set.push("kata-set-param analysisWideRootNoise 0".to_string());
                }
            }
        }
        // `setGameStatus`: KataGoCustom (not KataGoPda) engines are told a game runs.
        let custom = self
            .engines
            .identity(fg)
            .is_some_and(|(name, _, _)| name.starts_with("KataGoCustom"));
        if custom {
            set.push("startGame".to_string());
        }
        let mut awaiting = Vec::new();
        let mut captured = Vec::new();
        if readable {
            for name in capture {
                if let Some(n) = self.engines.game_send(res, &format!("kata-get-param {name}")) {
                    awaiting.push(n);
                    captured.push((n, name));
                }
            }
        }
        for c in &set {
            if let Some(n) = self.engines.game_send(res, c) {
                awaiting.push(n);
            }
        }
        let handicap = s.handicap.map(|(_, h)| h);
        let handicap_cmd = handicap.and_then(|h| {
            let command = if katago && settings.free_handicap { "place_free_handicap" } else { "fixed_handicap" };
            self.engines.game_send(res, &format!("{command} {h}"))
        });
        if kind == GameKind::Genmove {
            self.install_engine_clock(katago);
        }
        let s = self.play.session.as_mut().unwrap();
        s.setup_sent = true;
        s.capture = captured;
        s.awaiting = awaiting;
        if custom {
            s.restore.push("stopGame".to_string());
        }
        if let (Some(n), Some(h)) = (handicap_cmd, handicap) {
            s.handicap = Some((n, h));
            s.awaiting.push(n);
        }
        if s.awaiting.is_empty() {
            self.play_started();
        }
    }

    /// `installPlayingAgainstHumanCountDown` for the time just sent.
    fn install_engine_clock(&mut self, katago: bool) {
        let Some(s) = self.play.session.as_mut() else { return };
        let time = play::ai_time_commands(&self.config.game.ai_time, katago);
        if time.kata_mismatch {
            self.play.notices.push(notice("LizzieFrame.sendTimes.kataGoTimeMismatch"));
        }
        s.engine_clock = match time.clock {
            None => None,
            Some(line) => {
                let clock = EngineClock::parse(&line, s.human == Stone::White);
                if clock.is_none() {
                    self.play.notices.push(notice("EngineManager.parseAdvcanceTimeSettingsFailed"));
                }
                clock
            }
        };
    }

    /// 修改AI用时 during a Genmove game (`sendAiTime`).
    fn send_ai_time(&mut self) {
        let Some(s) = self.play.session.as_ref() else { return };
        let res = s.reservation;
        let Some(fg) = self.engines.game_instance(res) else { return };
        let katago = self.engines.is_katago(fg);
        for c in play::ai_time_commands(&self.config.game.ai_time, katago).commands {
            self.engines.game_send(res, &c);
        }
        self.install_engine_clock(katago);
    }

    /// Setup answered: the game has started.
    fn play_started(&mut self) {
        let Some(s) = self.play.session.as_mut() else { return };
        s.step = Step::Playing;
        if s.settings.limit_my_time {
            s.clock = Some(HumanClock::new(s.settings.save_minutes(), s.settings.byoyomi_seconds(), s.settings.byoyomi_times()));
            s.next_tick = Some(Instant::now() + TICK);
        }
        if s.kind == GameKind::Analysis && s.settings.pure_net && !self.engines.raw_net_active() {
            s.raw_net = self.engines.toggle_raw_net(false);
        }
        self.sync_engine();
        self.request_engine_move();
    }

    fn game_pause(&mut self) -> CommandResult {
        let Some(s) = self.play.session.as_mut() else { return CommandResult::Unchanged };
        s.step = match s.step {
            Step::Playing => Step::Paused,
            Step::Paused => Step::Playing,
            _ => return CommandResult::Unchanged,
        };
        if s.step == Step::Playing {
            s.next_tick = s.clock.map(|_| Instant::now() + TICK);
        }
        self.sync_engine();
        self.request_engine_move();
        CommandResult::Done
    }

    /// 结束对局 / Space: the game ends; a result already recorded makes it complete.
    fn game_stop(&mut self) -> bool {
        if let Some(m) = self.play.manual.take() {
            self.engines.release_game(m.reservation, &[]);
        }
        let Some(s) = self.play.session.as_ref() else { return false };
        if s.step == Step::Ending {
            return false;
        }
        let end = if self.doc.record.info.result.is_empty() { GameEnd::Terminated } else { GameEnd::Completed };
        self.game_end(end, None);
        true
    }

    /// 悔棋 (`undoForRightClick`): back to the human's previous turn; an engine move still being
    /// chosen is dropped when it arrives.
    fn game_regret(&mut self) -> CommandResult {
        let Some(s) = self.play.session.as_mut().filter(|s| matches!(s.step, Step::Playing | Step::Paused)) else {
            return CommandResult::Unchanged;
        };
        s.request = None;
        let (res, human) = (s.reservation, s.human);
        self.engines.cancel_genmove(res);
        let mut target = self.cursor;
        for _ in 0..2 {
            match self.doc.record.parent(target) {
                Some(p) => target = p,
                None => break,
            }
            if self.doc.record.node(target).to_play == human {
                break;
            }
        }
        if target == self.cursor {
            return CommandResult::Unchanged;
        }
        self.cursor = target;
        self.sounds.clear();
        self.sync_engine();
        self.request_engine_move();
        CommandResult::Done
    }

    /// Ends the game: result, the played line as main line, the automatic save, analysis left
    /// paused (Java `notPondering`), then the engine is given back.
    fn game_end(&mut self, end: GameEnd, result: Option<String>) {
        let Some(s) = self.play.session.as_mut().filter(|s| s.step != Step::Ending) else { return };
        s.step = Step::Ending;
        s.request = None;
        s.clock = None;
        s.next_tick = None;
        let (id, res, auto_save, raw_net, kind) = (s.id, s.reservation, s.settings.auto_save, s.raw_net, s.kind);
        self.engines.cancel_genmove(res);
        if let Some(r) = &result {
            let _ = self.edit_document(DocumentEdit::SetResult { result: r.clone() });
        }
        let _ = self.edit_document(DocumentEdit::SetAsMain { node: self.cursor });
        let saved = (auto_save && end != GameEnd::Retired).then(|| self.queue_game_save());
        if raw_net {
            self.engines.stop_raw_net();
        }
        self.paused = true;
        if kind == GameKind::Analysis {
            // `stopAiPlayingAndPolicy`: 自动落子 and its sides are cleared.
            self.play.autoplay = false;
            self.persisted.autoplay.black = false;
            self.persisted.autoplay.white = false;
            if let Err(e) = self.save_persisted() {
                self.log(true, format!("saving the 自动落子 sides failed: {}", e.message));
            }
        }
        self.play.outcome = Some(GameOutcomeView { game: id, end, result, saved });
        self.play_give_back();
        self.sync_engine();
    }

    /// Gives an ended game's engine back, once the setup it sent is answered: the restore needs
    /// every parameter value it read. An engine already gone has nothing to restore.
    fn play_give_back(&mut self) {
        let Some(s) = self.play.session.as_mut().filter(|s| s.step == Step::Ending) else { return };
        let res = s.reservation;
        if self.engines.game_instance(res).is_none() {
            self.play_released(res);
        } else if s.awaiting.is_empty() {
            let restore = std::mem::take(&mut s.restore);
            self.engines.release_game(res, &restore);
        }
    }

    /// `autoSavePlayedGame`: `MyGames/yyyy-MM-dd-HH-mmss(<PB>_Vs_<PW>).sgf` in the application
    /// data directory (Java's work directory, as for 存档与读档 and 自动分析).
    fn queue_game_save(&mut self) -> String {
        let info = &self.doc.record.info;
        let name = if info.black.is_empty() && info.white.is_empty() {
            super::exporting::java_text(self.active_language, "GameInfo.untitled")
        } else {
            format!("{}_Vs_{}", info.black, info.white)
        };
        let stamp = chrono::Local::now().format("%Y-%m-%d-%H-%M%S");
        let work = self.storage.save_dir.parent().unwrap_or(Path::new(".")).to_path_buf();
        let path = work.join("MyGames").join(format!("{stamp}({}).sgf", file_safe(&name)));
        let path = path.to_string_lossy().into_owned();
        // Written comments must not make a clean record unsaved (as 自动分析's copies).
        let clean = !self.doc.unsaved();
        let mut job = self.begin_save(&path, "LizzieYzy Tauri");
        if clean {
            self.doc.saved_revision = Some(self.doc.revision);
        }
        job.create_parent = true;
        self.play.saves.push(job.ticket);
        self.play.jobs.push(job);
        path
    }

    pub(super) fn take_play_saves(&mut self) -> Vec<SaveJob> {
        std::mem::take(&mut self.play.jobs)
    }

    /// A game's automatic save finished: it is not the record's own file.
    pub(super) fn play_save_finished(&mut self, ticket: u64, result: &Result<(), String>) -> bool {
        let Some(at) = self.play.saves.iter().position(|t| *t == ticket) else { return false };
        self.play.saves.remove(at);
        if let Err(e) = result {
            self.log(true, format!("saving the played game failed: {e}"));
        }
        true
    }

    /// The engine was given back (or is gone): the game is over; a waiting game starts.
    fn play_released(&mut self, reservation: u64) {
        if self.play.manual.as_ref().is_some_and(|m| m.reservation == reservation) {
            self.play.manual = None;
        }
        if self.play.session.as_ref().is_some_and(|s| s.reservation == reservation) {
            self.play.session = None;
        }
        self.sync_engine();
        if self.play.session.is_none() && self.play.manual.is_none() {
            if let Some(op) = self.play.queued.take() {
                if let Err(e) = self.game(op) {
                    self.log(true, format!("starting the next game failed: {}", e.message));
                }
            }
        }
    }

    /// Another record is about to replace the game's: the game ends as the user would end it.
    pub(super) fn play_document_closing(&mut self) {
        self.play.queued = None;
        self.game_stop();
        if let Some(m) = self.play.manual.take() {
            self.engines.release_game(m.reservation, &[]);
        }
    }

    // -------------------------------------------------------------------- moves

    /// After the human's move: the clock's period starts again and, in a Genmove game, the engine
    /// is asked for its move; the first pass shows how to count the game.
    pub(super) fn play_after_human(&mut self) {
        let passed = matches!(self.doc.record.node(self.cursor).kind, NodeKind::Pass { .. });
        let Some(s) = self.play.session.as_mut() else { return };
        if let Some(c) = s.clock.as_mut() {
            c.moved();
        }
        if passed && !s.passed {
            s.passed = true;
            self.play.notices.push(notice("LizzieFrame.passInGameTip"));
        }
        self.request_engine_move();
    }

    /// A Genmove game asks its engine when the engine is to move (`genmove`).
    fn request_engine_move(&mut self) {
        let to_play = self.to_play();
        let target = self.analysis_target();
        let stop_after = !self.config.game.play_ponder;
        let Some(s) = self.play.session.as_mut() else { return };
        if s.kind != GameKind::Genmove || s.step != Step::Playing || s.request.is_some() || to_play == s.human {
            return;
        }
        let id = s.next_request;
        s.next_request += 1;
        let time_left = s.engine_clock.as_mut().and_then(EngineClock::claim);
        let node = target.node;
        let stamp = target.record.node(node).stamp;
        if self.engines.genmove(s.reservation, id, target, to_play, time_left, stop_after) {
            s.request = Some(Request { id, node, stamp, sent: Instant::now() });
        }
    }

    /// The engine's move becomes the record's next move as the engine's colour.
    fn commit_engine_move(&mut self, point: Option<Point>, color: Stone) -> Option<NodeId> {
        let at = self.cursor;
        let edit = match point {
            Some(point) => DocumentEdit::Play { at, color, point, alternate: false, rules: self.play_rules() },
            None => DocumentEdit::Pass { at, color },
        };
        let node = self.apply_edit(edit).ok()?.node?;
        if let Some(c) = self.play.session.as_mut().and_then(|s| s.clock.as_mut()) {
            c.moved();
        }
        Some(node)
    }

    fn follow_engine_move(&mut self, node: NodeId) {
        self.move_to(node);
    }

    fn engine_won(&self) -> Option<String> {
        let s = self.play.session.as_ref()?;
        Some(format!("{}+R", color_sgf(s.human)))
    }

    /// The engine's answer to a Genmove request. An answered move the record does not take is
    /// dropped from the engine board.
    fn play_genmove(&mut self, game: u64, request: u64, doc: u64, node: NodeId, stamp: u64, reply: GenmoveReply) {
        self.take_genmove(game, request, doc, node, stamp, reply);
        self.engines.game_move_dropped(game, request);
    }

    fn take_genmove(&mut self, game: u64, request: u64, doc: u64, node: NodeId, stamp: u64, reply: GenmoveReply) {
        if let Some(m) = self.play.manual.as_mut().filter(|m| m.reservation == game && !m.answered) {
            m.answered = true;
            let (color, res) = (m.color, m.reservation);
            let current = doc == self.doc.id && node == self.cursor && self.doc.record.node(node).stamp == stamp && node == m.node && stamp == m.stamp;
            if current {
                let point = match reply {
                    GenmoveReply::Move(p) => Some(Some(p)),
                    GenmoveReply::Pass => Some(None),
                    _ => None,
                };
                if let Some(point) = point {
                    if let Some(n) = self.commit_engine_move(point, color) {
                        self.follow_engine_move(n);
                        self.engines.game_move_committed(res, request, self.doc.record.node(n).stamp);
                    }
                }
            }
            self.engines.release_game(res, &[]);
            return;
        }
        let Some(s) = self.play.session.as_mut().filter(|s| s.reservation == game) else { return };
        let Some(r) = s.request.take_if(|r| r.id == request) else { return };
        if let Some(c) = s.engine_clock.as_mut() {
            c.elapse((r.sent.elapsed().as_millis() / 10) as u64);
        }
        let current = matches!(s.step, Step::Playing | Step::Paused)
            && doc == self.doc.id
            && doc == s.doc
            && node == self.cursor
            && node == r.node
            && self.doc.record.contains(node)
            && self.doc.record.node(node).stamp == stamp
            && stamp == r.stamp;
        if !current {
            return;
        }
        let engine = s.human.opposite();
        match reply {
            GenmoveReply::Resign => {
                let key = if s.human == Stone::Black { "Leelaz.blackWinAiResign" } else { "Leelaz.whiteWinAiResign" };
                self.play.notices.push(notice(key));
                let result = self.engine_won();
                self.game_end(GameEnd::EngineResigned, result);
            }
            GenmoveReply::Move(p) => self.engine_played(game, request, Some(p), engine),
            GenmoveReply::Pass => self.engine_played(game, request, None, engine),
            GenmoveReply::Failed(text) => {
                self.log(true, format!("engine move failed: {text}"));
                self.game_end(GameEnd::Retired, None);
            }
        }
    }

    fn engine_played(&mut self, game: u64, request: u64, point: Option<Point>, color: Stone) {
        match self.commit_engine_move(point, color) {
            Some(n) => {
                // The engine board keeps its move while the position follows it; then the move
                // is the node's.
                self.follow_engine_move(n);
                self.engines.game_move_committed(game, request, self.doc.record.node(n).stamp);
            }
            None => {
                self.log(true, "the engine's move is not legal here".into());
                self.game_end(GameEnd::Retired, None);
            }
        }
    }

    /// Who may move from the analysis of the current position now (`notifyAutoPlay`: the
    /// 自动落子 box and the box of the side to move): `Some(true)` a 分析模式 game's engine,
    /// `Some(false)` the bottom bar outside a game. Nobody during 终局点目.
    fn analysis_mover(&self) -> Option<bool> {
        let to_play = self.to_play();
        let a = &self.persisted.autoplay;
        let side = if to_play == Stone::Black { a.black } else { a.white };
        if !self.play.autoplay || !side || self.scoring.is_some() {
            return None;
        }
        match &self.play.session {
            Some(s) => (s.kind == GameKind::Analysis && s.step == Step::Playing && !s.settings.pure_net && to_play != s.human)
                .then_some(true),
            None => (!self.paused && self.play.manual.is_none()).then_some(false),
        }
    }

    /// Whether the foreground engine's analysis of `node` may make a move.
    pub(super) fn play_wants_analysis(&self, node: NodeId) -> bool {
        node == self.cursor && self.analysis_mover().is_some()
    }

    /// `notifyAutoPlay`: a 分析模式 game's engine, or the toolbar's 自动落子 side, plays when a
    /// 落子条件 is met by the analysis of the current position.
    pub(super) fn play_analysis(&mut self, result: &NodeAnalysisView, elapsed: Duration) {
        if result.node != self.cursor || result.candidates.is_empty() {
            return;
        }
        let to_play = self.to_play();
        let Some(game) = self.analysis_mover() else { return };
        let first = result.candidates[0].visits;
        if !play::autoplay_due(&self.persisted.autoplay, first, result.visits, elapsed.as_millis()) {
            return;
        }
        if game && self.game_resigns(Some(result.candidates[0].winrate)) {
            return;
        }
        let move_number = self.doc.record.node(self.cursor).move_number;
        let random = &self.config.game.random_start;
        let point = if random.enabled && move_number <= random.moves {
            let choices: Vec<_> = result
                .candidates
                .iter()
                .map(|c| play::Choice { point: c.point, winrate: c.winrate, visits: c.visits })
                .collect();
            let seed = std::collections::hash_map::RandomState::new();
            play::random_choice(&choices, random, |n| (std::hash::BuildHasher::hash_one(&seed, move_number) % n as u64) as usize)
                .flatten()
        } else {
            result.candidates[0].point
        };
        self.auto_move(point, to_play, game);
    }

    /// 纯网络 in a 分析模式 game: the policy's best point (`playBestHeatMove`).
    pub(super) fn play_raw_net(&mut self) {
        let to_play = self.to_play();
        let Some(s) = self.play.session.as_ref() else { return };
        let stopped = self.scoring.is_some();
        if s.kind != GameKind::Analysis || s.step != Step::Playing || !s.settings.pure_net || to_play == s.human || stopped {
            return;
        }
        let human = s.human;
        let Some(view) = self.engines.raw_net_view(&self.analysis_target()) else { return };
        let Some(policy) = view.policy.as_ref().filter(|_| view.node == self.cursor) else { return };
        let engine_winrate = view.white_winrate.map(|w| if human == Stone::Black { w * 100.0 } else { 100.0 - w * 100.0 });
        if self.game_resigns(engine_winrate) {
            return;
        }
        let w = self.doc.record.size.width as usize;
        let best = policy
            .iter()
            .enumerate()
            .filter(|(_, p)| **p > 0.0)
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| Point { x: (i % w) as u16, y: (i / w) as u16 });
        self.auto_move(best, to_play, true);
    }

    /// `notifyAnaResign`: the engine resigns after enough consecutive losing decisions.
    fn game_resigns(&mut self, engine_winrate: Option<f64>) -> bool {
        let move_number = self.doc.record.node(self.cursor).move_number;
        let Some(s) = self.play.session.as_mut() else { return false };
        s.resign = play::resign_count(s.resign, move_number, engine_winrate, s.settings.resign_start_move, s.settings.resign_percent);
        if s.resign < s.settings.resign_move {
            return false;
        }
        let name = self.engines.foreground().and_then(|f| self.engines.engine_name(f)).unwrap_or_default();
        self.play.notices.push(GameNotice { message_key: "Leelaz.resign".into(), prefix: Some(name) });
        let result = self.engine_won();
        self.game_end(GameEnd::EngineResigned, result);
        true
    }

    fn auto_move(&mut self, point: Option<Point>, color: Stone, game: bool) {
        let Some(node) = self.commit_engine_move(point, color) else { return };
        self.move_to(node);
        if !game && !self.config.game.play_ponder {
            // `nameCmd` after an automatic move when the next side is the user's.
            let a = &self.persisted.autoplay;
            let next = if self.to_play() == Stone::Black { a.black } else { a.white };
            if !next {
                self.paused = true;
                self.sync_engine();
            }
        }
    }

    /// Alt+, (`LizzieFrame.genmove`): the engine plays the side to move once, outside a game.
    fn manual_engine_move(&mut self) -> CommandResult {
        if self.game_active() || self.play.manual.is_some() {
            return CommandResult::Unchanged;
        }
        if self.foreground_mode_conflict().is_some() || self.setup_mode() {
            return CommandResult::Unchanged;
        }
        let Some(fg) = self.engines.foreground().filter(|&f| self.engines.phase(f) == Some(Phase::Ready)) else {
            return CommandResult::Unchanged;
        };
        let Ok(reservation) = self.engines.reserve_game() else { return CommandResult::Unchanged };
        let _ = fg;
        let target = self.analysis_target();
        let color = self.to_play();
        let (node, stamp) = (target.node, target.record.node(target.node).stamp);
        // `if (isPondering) ponder(); else nameCmdfornoponder()` after the move.
        let stop_after = self.paused;
        self.play.manual = Some(Manual { reservation, node, stamp, color, answered: false });
        self.sync_engine();
        self.engines.genmove(reservation, 1, target, color, None, stop_after);
        CommandResult::Done
    }

    // -------------------------------------------------------------------- engine events

    /// Engine events of the game; others pass on.
    pub(super) fn play_event(&mut self, ev: RuntimeEvent) -> Option<RuntimeEvent> {
        match ev {
            RuntimeEvent::Ready { instance } => {
                if self.play.session.as_ref().is_some_and(|s| self.engines.game_instance(s.reservation) == Some(instance)) {
                    self.play_setup();
                }
                Some(RuntimeEvent::Ready { instance })
            }
            RuntimeEvent::GameReply { game, command, ok, text } => {
                self.play_reply(game, command, ok, text);
                None
            }
            RuntimeEvent::Genmove { game, request, doc, node, stamp, reply } => {
                self.play_genmove(game, request, doc, node, stamp, reply);
                None
            }
            RuntimeEvent::GameReleased { game, failure } => {
                if let Some(f) = failure {
                    self.log(true, format!("game engine given back with failure: {f:?}"));
                }
                let playing = self.play.session.as_ref().is_some_and(|s| s.reservation == game && s.step != Step::Ending);
                if playing {
                    // The engine is gone while the game runs: retired, nothing more to restore.
                    self.game_end(GameEnd::Retired, None);
                } else {
                    self.play_released(game);
                }
                None
            }
            RuntimeEvent::RawNet { instance } => {
                self.play_raw_net();
                Some(RuntimeEvent::RawNet { instance })
            }
            ev => Some(ev),
        }
    }

    fn play_reply(&mut self, game: u64, command: u64, ok: bool, text: String) {
        let Some(s) = self.play.session.as_mut().filter(|s| s.reservation == game) else { return };
        if let Some(at) = s.capture.iter().position(|(n, _)| *n == command) {
            let (_, name) = s.capture.remove(at);
            if ok && !text.trim().is_empty() {
                s.restore.push(format!("kata-set-param {name} {}", text.trim()));
            }
        }
        let handicap = s.handicap.filter(|(n, _)| *n == command).map(|(_, h)| h);
        // A late answer to an ended game's handicap leaves the record (maybe another one) alone.
        let handicap = handicap.filter(|_| s.step == Step::Accepted && s.doc == self.doc.id);
        let waiting = !s.awaiting.is_empty();
        s.awaiting.retain(|n| *n != command);
        let answered = waiting && s.awaiting.is_empty();
        let (started, ending) = (answered && s.step == Step::Accepted, answered && s.step == Step::Ending);
        if let Some(h) = handicap {
            self.place_handicap(h, ok.then_some(text));
        }
        if started {
            self.play_started();
        }
        if ending {
            self.play_give_back();
        }
    }

    /// The handicap stones the engine placed (`isSettingHandicap`): the starting position, White
    /// to play.
    fn place_handicap(&mut self, handicap: u32, answer: Option<String>) {
        let Some(answer) = answer else {
            self.log(true, "the engine refused the handicap".into());
            return;
        };
        let size = self.doc.record.size;
        for word in answer.split_whitespace() {
            if let Ok(Some(point)) = crate::engine::gtp::parse_vertex(size, word) {
                let _ = self.apply_edit(DocumentEdit::SetupStone { point, stone: Some(Stone::Black) });
            }
        }
        let _ = self.apply_edit(DocumentEdit::SetupSideToPlay { color: Stone::White });
        let _ = self.apply_edit(DocumentEdit::SetHandicap { handicap });
        self.sync_engine();
    }

    // -------------------------------------------------------------------- clock

    pub(super) fn play_deadline(&self) -> Option<Instant> {
        self.play.session.as_ref().filter(|s| s.step == Step::Playing).and_then(|s| s.next_tick)
    }

    /// One second of the human's clock while it is the human's turn.
    pub(super) fn play_tick(&mut self, now: Instant) -> bool {
        let to_play = self.to_play();
        let ready = self.engines.foreground().is_some_and(|f| self.engines.phase(f) == Some(Phase::Ready));
        let Some(s) = self.play.session.as_mut().filter(|s| s.step == Step::Playing) else { return false };
        let Some(due) = s.next_tick.filter(|d| now >= *d) else { return false };
        s.next_tick = Some(due.max(now - TICK) + TICK);
        if to_play != s.human || !ready {
            return false;
        }
        let Some(c) = s.clock.as_mut() else { return false };
        match c.tick() {
            ClockTick::Running => {}
            ClockTick::Voice(n) => self.play.voices.push(n),
            ClockTick::Expired => {
                let key = if s.human == Stone::Black { "Byoyomi.timeOutBlack" } else { "Byoyomi.timeOutWhite" };
                let result = format!("{}+T", color_sgf(s.human.opposite()));
                self.play.notices.push(notice(key));
                self.game_end(GameEnd::HumanTimeout, Some(result));
            }
        }
        true
    }

    // -------------------------------------------------------------------- projection

    pub(super) fn play_view(&self) -> Option<GameView> {
        let s = self.play.session.as_ref()?;
        let phase = match s.step {
            Step::Accepted => GamePhase::Starting,
            Step::Playing => GamePhase::Playing,
            Step::Paused => GamePhase::Paused,
            Step::Ending => GamePhase::Ending,
        };
        let thinking = s.step != Step::Ending
            && match s.kind {
                GameKind::Genmove => s.request.is_some(),
                GameKind::Analysis => s.step == Step::Playing && self.to_play() != s.human,
            };
        Some(GameView {
            id: s.id,
            kind: s.kind,
            phase,
            human: s.human,
            thinking,
            play_mode: s.settings.use_play_mode && s.step != Step::Ending,
            show_black: s.show.0,
            show_white: s.show.1,
            pure_net: s.kind == GameKind::Analysis && s.settings.pure_net,
            clock: s.clock.map(|c| GameClockView {
                minutes: c.minutes,
                seconds: c.seconds,
                byo_seconds: c.byo_seconds,
                byo_times: c.byo_times,
            }),
        })
    }

    pub(super) fn play_autoplay_enabled(&self) -> bool {
        self.play.autoplay
    }

    pub(super) fn take_play_outcome(&mut self) -> (Option<GameOutcomeView>, Vec<GameNotice>, Vec<u8>) {
        (self.play.outcome.take(), std::mem::take(&mut self.play.notices), std::mem::take(&mut self.play.voices))
    }

    pub(super) fn play_outcome_copy(&self) -> (Option<GameOutcomeView>, Vec<GameNotice>, Vec<u8>) {
        (self.play.outcome.clone(), self.play.notices.clone(), self.play.voices.clone())
    }
}
