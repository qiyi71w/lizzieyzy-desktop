//! HumanSL owns a training session, not the user's committed display settings. The companion
//! speaks analysis JSON; the foreground engine is only paused and later resynchronized.
mod ai;
use super::Workbench;
use crate::api::*;
use crate::engine::{
    gtp,
    runtime::{InstanceId, RuntimeEvent},
};
use crate::humansl::{self, protocol};
use crate::record::{DocumentEdit, NodeId, Point, Stone};
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
    position_context: u64,
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
    generation: u64,
    training_start_node: Option<NodeId>,
    restore: Option<u64>,
    resume: bool,
    started_game: bool,
    diagnostics: std::collections::VecDeque<String>,
}
struct Request {
    id: String,
    generation: u64,
    doc: u64,
    node: NodeId,
    stamp: u64,
    position_context: u64,
    sent: Instant,
    deadline: Instant,
    pace: Instant,
    attempt: u8,
    phase: Search,
    candidates: Vec<humansl::Candidate>,
    allowed: Vec<String>,
    visits: u32,
    rounds: u8,
    evidence: Option<Value>,
    selected: Option<String>,
}
#[derive(PartialEq)]
enum Search {
    Ready,
    Policy,
    Verify,
    Pacing,
}

fn refused(key: &str) -> CommandResult {
    CommandResult::Refused {
        message_key: key.into(),
    }
}
impl Workbench {
    pub(super) fn human_sl_position_changed(&mut self) {
        self.human_sl.position_context = self.human_sl.position_context.wrapping_add(1);
    }
    pub fn human_sl_active(&self) -> bool {
        self.human_sl.session.is_some()
    }

    pub fn human_sl(&mut self, op: HumanSlCommand) -> Result<CommandResult, ApiError> {
        match op {
            HumanSlCommand::Start {
                settings,
                from_current,
                advanced_visible,
            } => self.human_sl_start(settings.normalized(), from_current, advanced_visible),
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
        if self.quick_holds_foreground()
            || self.flash_holds_foreground()
            || self.whole_game_holds_foreground()
            || self.batch_holds_foreground()
            || self.auto_analysis_active()
        {
            return Ok(refused("AnalysisSettings.reuseStatus.existing_lease"));
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
            generation: 0,
            training_start_node: None,
            restore: None,
            resume: false,
            started_game: false,
            diagnostics: Default::default(),
        });
        self.human_sl_request(Search::Ready, now);
        Ok(CommandResult::Done)
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
            self.reopen_empty_board(size);
            self.komi_changed = true;
            self.start_komi = None;
            self.apply_komi(settings.komi);
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
            let me = super::exporting::java_text(self.active_language, "HumanSlGame.humanPlayer");
            let label = match settings.opponent {
                crate::settings::OpponentPreset::ModernPro => {
                    super::exporting::java_text(self.active_language, "HumanSlTraining.pro.modern")
                }
                crate::settings::OpponentPreset::Online9d => {
                    super::exporting::java_text(self.active_language, "HumanSlTraining.pro.online9d")
                }
                crate::settings::OpponentPreset::Rank => {
                    let pattern = if settings.dan_rank {
                        super::exporting::java_text(self.active_language, "HumanSlTraining.rank.danValue")
                    } else {
                        super::exporting::java_text(self.active_language, "HumanSlTraining.rank.kyuValue")
                    };
                    pattern.replace("{0}", &settings.rank.to_string())
                }
            };
            let ai = format!(
                "{} ({label})",
                super::exporting::java_text(self.active_language, "HumanSlGame.aiPlayer")
            );
            let (black, white) = if human == Stone::Black { (me, ai) } else { (ai, me) };
            let _ = self.apply_edit(DocumentEdit::SetPlayers { black, white });
        }
        let s = self.human_sl.session.as_mut().unwrap();
        s.phase = HumanSlPhase::Playing;
        s.started_game = true;
        s.stage = HumanSlStartupStage::Ready;
        s.request = None;
        s.turn = now;
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

    pub(super) fn human_sl_human_move(&mut self, point: Option<Point>) -> CommandResult {
        let Some(s) = self.human_sl.session.as_ref() else {
            return CommandResult::Unchanged;
        };
        if s.phase != HumanSlPhase::Playing
            || s.request.is_some()
            || self.doc.record.node(self.cursor).to_play != s.human
        {
            return CommandResult::Unchanged;
        }
        if !self.human_sl_commit(point) {
            return CommandResult::Unchanged;
        }
        let now = Instant::now();
        let s = self.human_sl.session.as_mut().unwrap();
        s.human_ms = s.human_ms.saturating_add(now.duration_since(s.turn).as_millis() as u64);
        s.turn = now;
        self.human_sl_schedule(now);
        CommandResult::Done
    }

    fn human_sl_commit(&mut self, point: Option<Point>) -> bool {
        let color = self.doc.record.node(self.cursor).to_play;
        let force = self
            .human_sl
            .session
            .as_ref()
            .is_some_and(|s| s.training_start_node == Some(self.cursor));
        let rules = self.play_rules();
        let edit = match (point, force) {
            (Some(point), true) => DocumentEdit::PlayBranch {
                at: self.cursor,
                color,
                point,
                rules,
            },
            (Some(point), false) => DocumentEdit::Play {
                at: self.cursor,
                color,
                point,
                alternate: false,
                rules,
            },
            (None, true) => DocumentEdit::PassBranch { at: self.cursor, color },
            (None, false) => DocumentEdit::Pass { at: self.cursor, color },
        };
        let Ok(applied) = self.apply_edit(edit) else {
            return false;
        };
        let Some(node) = applied.node else { return false };
        self.move_to(node);
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
        let playing = s.phase == HumanSlPhase::Playing;
        let human_turn = playing && self.doc.record.node(self.cursor).to_play == s.human;
        Some(HumanSlView {
            id: s.id,
            phase: s.phase,
            stage: (s.phase == HumanSlPhase::Preparing).then_some(s.stage),
            preparing_secs: s.start.elapsed().as_secs().min(u64::from(u32::MAX)) as u32,
            mode: s.settings.mode,
            opponent: s.settings.opponent,
            rank: s.settings.rank,
            dan_rank: s.settings.dan_rank,
            human_is_black: s.human == Stone::Black,
            human_ms: s.human_ms,
            ai_ms: s.ai_ms,
            clock: playing.then_some(if human_turn {
                HumanSlSide::Human
            } else {
                HumanSlSide::Ai
            }),
            human_turn,
            ai_thinking: s.request.is_some(),
            ai_failed: s.failed,
            analysis_display: if s.settings.mode == TrainingMode::LiveAnalysis {
                HumanSlAnalysisDisplay::Show
            } else {
                HumanSlAnalysisDisplay::Suppress
            },
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
