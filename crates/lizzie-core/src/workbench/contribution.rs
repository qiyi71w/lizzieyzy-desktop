//! 跑谱贡献 flow (Java `ContributeEngine` + `LizzieFrame.startContributeEngine`): the exclusive
//! KataGo distributed-training session, its worker process, the watched game written into the
//! main document, pause/slow/force stop, saving games and the gates it places on the rest of the
//! workbench. Pure command/JSON/comment logic lives in [`crate::contribution`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{PendingSave, SaveJob, Workbench};
use crate::api::{
    ApiError, CandidateView, Command, CommandResult, ContributionCommand, ContributionGameView,
    ContributionNotice, ContributionPhase, ContributionTip, ContributionView, EditCommand,
    EngineFailureView, EngineScope, NodeAnalysisView,
};
use crate::contribution::{command as contribute_command, comment, output, rules, saved, snapshot};
use crate::engine::gtp::prepare_launch;
use crate::engine::runtime::{ConsoleKind, InstanceId, LeaseAvailability, RuntimeEvent};
use crate::engine::ssh::{RemoteFailure, SshSettings};
use crate::record::{DocumentEdit, NodeId, NodeKind, Stone, ROOT};
use crate::sanitize::sanitize;
use crate::settings::{ContributionWatchSettings, KataEstimateState};

/// The `engine` name of this session's console lines.
pub const CONSOLE_ENGINE: &str = "Contribute";
/// `STARTING_GAME`'s watch loop sleeps 50 ms.
const WATCH_INTERVAL: Duration = Duration::from_millis(50);
/// `pauseAndResume`'s `2000` ms throttle.
const PAUSE_THROTTLE: Duration = Duration::from_secs(2);
/// `Contribute.tips.contributingAndStartAnotherLizzieYzy`.
pub(super) const CONTRIBUTING: &str = "Contribute.tips.contributingAndStartAnotherLizzieYzy";
/// The product key for an installed engine package that cannot contribute.
pub const RESOURCE_UNSUPPORTED: &str = "contribution.resourceUnsupported";

fn refused(key: &str) -> CommandResult {
    CommandResult::Refused {
        message_key: key.to_string(),
    }
}

/// One launch of the worker (`ContributeEngine.Launch`).
struct Launch {
    worker: Option<InstanceId>,
}

/// `ContributeEngine`'s session state.
struct Session {
    id: u64,
    phase: ContributionPhase,
    paused: bool,
    slow_quitting: bool,
    tip_key: String,
    eternal_tip: bool,
    watching: Option<usize>,
    games: Vec<snapshot::Game>,
    unparsed: Vec<snapshot::Unparsed>,
    console_engine: String,
    engine_command: String,
    /// The document the watched game was last written into: a later result or held move only
    /// goes there, never into a record the user opened meanwhile.
    board: u64,
    /// `engine_command` with the password field written as `<redacted>`, built the same way: what
    /// every surface shows, whatever characters the password has.
    shown_command: String,
    pure_command: bool,
    remote: Option<SshSettings>,
    cwd: PathBuf,
    timestamp: String,
    launches: u32,
    add_cacerts: bool,
    normal_end: bool,
    launch: Launch,
    launch_error: String,
    failure: output::Failure,
    last_remote: Option<RemoteFailure>,
    terminal: bool,
    output: Vec<String>,
    version_seen: bool,
    result_hidden: bool,
    last_pause_resume: Option<Instant>,
    watch_started: bool,
    watch_tick: Instant,
    watch_elapsed: Duration,
    watch_cursor: Option<NodeId>,
    pk_comments: HashMap<(u64, NodeId), String>,
    estimate_saved: Option<(KataEstimateState, bool)>,
}

impl Session {
    fn active(&self) -> bool {
        self.phase != ContributionPhase::Exited && !self.paused
    }

    fn current(&self) -> Option<&snapshot::Game> {
        self.watching.and_then(|index| self.games.get(index))
    }
}

/// The contribution session and its one-shot notices.
pub(super) struct ContributionFlow {
    session: Option<Session>,
    notices: Vec<ContributionNotice>,
    next_id: u64,
    /// Record writes for the application's serial writer.
    jobs: Vec<SaveJob>,
    /// Writes still running: their ticket, and the session and complete game each one marks saved.
    saves: Vec<(u64, Option<(u64, usize)>)>,
    /// 保存全部棋谱's folder, announced once its writes are done.
    saved_folder: Option<String>,
    /// A write of the running saves failed.
    save_failed: bool,
}

impl Default for ContributionFlow {
    fn default() -> Self {
        ContributionFlow {
            session: None,
            notices: Vec::new(),
            next_id: 1,
            jobs: Vec::new(),
            saves: Vec::new(),
            saved_folder: None,
            save_failed: false,
        }
    }
}

fn java_double(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

fn short_name(name: &str) -> String {
    name.chars().take(19).collect()
}

/// Match 存谱's file-name rules (`EngineManager.writeToFile`'s `safe`).
fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if "/\\:*?|".contains(c) {
                '.'
            } else if "\"<>".contains(c) {
                '\''
            } else if c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// The root of a KataGo record (where `RE` is) lies at its start: no more is read.
const RESULT_READ_LIMIT: u64 = 64 * 1024;

/// `SGFParser.getResult` on the uploaded `sgf`, relative to the process's working directory: the
/// first 20 characters after `RE[`, up to `]`; empty when unreadable. Read when the game finishes,
/// like Java, so nothing can see the game complete without its result; the read is bounded.
fn read_result(cwd: &Path, sgf: &str) -> String {
    use std::io::Read;
    let path = Path::new(sgf);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(sgf.replace('\\', "/"))
    };
    let mut bytes = Vec::new();
    let read = std::fs::File::open(&path).and_then(|file| file.take(RESULT_READ_LIMIT).read_to_end(&mut bytes));
    if read.is_err() {
        return String::new();
    }
    snapshot::parse_result(&String::from_utf8_lossy(&bytes))
}

impl ContributionFlow {
    pub(super) fn exists(&self) -> bool {
        self.session.is_some()
    }

    pub(super) fn active(&self) -> bool {
        self.session.as_ref().is_some_and(Session::active)
    }

    pub(super) fn view(&self, language: crate::settings::Language) -> Option<ContributionView> {
        let session = self.session.as_ref()?;
        let game = session.current().map(|game| ContributionGameView {
            game_id: game.id.clone(),
            rating: game.rating,
            black: game.black.clone(),
            white: game.white.clone(),
            width: game.width,
            height: game.height,
            komi: game.komi,
            rules_text: rules::rules_text(&game.rules, &|key| contribution_text(language, key)),
            complete: game.complete,
            result: game.result.clone(),
        });
        Some(ContributionView {
            id: session.id,
            phase: session.phase,
            paused: session.paused,
            active: session.active(),
            slow_quitting: session.slow_quitting,
            tip: ContributionTip {
                key: session.tip_key.clone(),
                eternal: session.eternal_tip,
            },
            watching: session.watching.map_or(0, |index| index as u32 + 1),
            total: session.games.len() as u32,
            finished: session.games.iter().filter(|game| game.complete).count() as u32,
            game,
            result_hidden: session.result_hidden,
            console_engine: session.console_engine.clone(),
        })
    }

    pub(super) fn take_notices(&mut self) -> Vec<ContributionNotice> {
        std::mem::take(&mut self.notices)
    }

    pub(super) fn notices(&self) -> Vec<ContributionNotice> {
        self.notices.clone()
    }
}

impl Workbench {
    // -------------------------------------------------------------------- projection

    pub(super) fn contribution_view(&self) -> Option<ContributionView> {
        self.contribution.view(self.active_language)
    }

    pub(super) fn take_contribution_notices(&mut self) -> Vec<ContributionNotice> {
        self.contribution.take_notices()
    }

    pub(super) fn contribution_notices(&self) -> Vec<ContributionNotice> {
        self.contribution.notices()
    }

    /// Whether a contribution session exists (ended but not yet closed included): no analysis
    /// engine starts while it does (T45 restart, T46 preload).
    pub(super) fn contribution_session_exists(&self) -> bool {
        self.contribution.exists()
    }

    /// Java `isContributing`: gates the board, other modes and engine switching.
    pub(super) fn contribution_active(&self) -> bool {
        self.contribution.active()
    }

    // -------------------------------------------------------------------- commands

    pub fn contribution(&mut self, op: ContributionCommand) -> Result<CommandResult, ApiError> {
        match op {
            ContributionCommand::SaveSettings { mut settings } => {
                // The custom command as typed is kept by the platform adapter; only its concealed
                // form is ever saved.
                settings.command = contribute_command::conceal_passwords(&settings.command);
                if settings == self.config.contribution {
                    return Ok(CommandResult::Unchanged);
                }
                self.commit_config(|config| config.contribution = settings)?;
                Ok(CommandResult::Done)
            }
            // The platform adapter owns the credential and the native chooser/browser.
            ContributionCommand::SetPassword { .. }
            | ContributionCommand::PasswordState { .. }
            | ContributionCommand::ChooseFile { .. }
            | ContributionCommand::OpenLink { .. } => Ok(CommandResult::Unchanged),
            // The shell reads the stored password and kept command and calls `contribution_start`.
            ContributionCommand::Start => self.contribution_start(None, None),
            ContributionCommand::PauseResume => Ok(self.contribution_pause_resume()),
            ContributionCommand::SlowQuit => Ok(self.contribution_slow_quit()),
            ContributionCommand::ForceQuit => Ok(self.contribution_force_quit()),
            ContributionCommand::CloseView => Ok(self.contribution_close_view()),
            ContributionCommand::WatchGame { index } => Ok(self.contribution_watch_game(index)),
            ContributionCommand::SaveAllGames => Ok(self.contribution_save_all()),
            ContributionCommand::SetWatchSettings { settings } => self.set_contribution_watch(settings),
            ContributionCommand::ToggleResultHidden => Ok(self.contribution_toggle_result()),
            ContributionCommand::SetShowMenu { enabled } => {
                if self.config.contribution.show_menu == enabled {
                    return Ok(CommandResult::Unchanged);
                }
                self.commit_config(|config| config.contribution.show_menu = enabled)?;
                Ok(CommandResult::Done)
            }
            ContributionCommand::SavedEngines => Ok(CommandResult::ContributionSavedEngines {
                candidates: saved::candidates(&self.config.engine.engines, self.engines.roots()),
            }),
            ContributionCommand::Qualify { engine_path } => {
                Ok(CommandResult::ContributionQualification {
                    qualification: saved::qualification(&engine_path, &self.storage.resources_dir),
                })
            }
        }
    }

    fn set_contribution_watch(
        &mut self,
        mut settings: ContributionWatchSettings,
    ) -> Result<CommandResult, ApiError> {
        if !settings.auto_play_interval.is_finite() {
            settings.auto_play_interval = 2.0;
        }
        let always_last_move = settings.always_last_move;
        let changed = self.config.contribution.watch != settings;
        if changed {
            self.commit_config(|config| config.contribution.watch = settings)?;
        }
        let moved = if always_last_move {
            self.contribution_goto_line_end()
        } else {
            false
        };
        Ok(if changed || moved {
            CommandResult::Done
        } else {
            CommandResult::Unchanged
        })
    }

    /// `LizzieFrame.startContributeEngine[Reserved]`: every refusal happens before anything is
    /// changed. `password` is the training password and `kept_command` the custom command as typed,
    /// both read by the platform adapter.
    pub fn contribution_start(
        &mut self,
        password: Option<String>,
        kept_command: Option<&str>,
    ) -> Result<CommandResult, ApiError> {
        if self.benchmark_running() {
            return Ok(refused("Contribute.tips.blockedByKataGoTuning"));
        }
        if self.game_active() || self.match_active() {
            return Ok(refused(super::play::PLAY_MODE));
        }
        if self.human_sl_active() {
            return Ok(refused("AnalysisSettings.reuseStatus.human_sl_game"));
        }
        // Java only takes a reservation when a foreground engine exists; a borrowed or held
        // engine is the conflict, an absent one is not.
        let availability = self.engines.lease_availability();
        if matches!(
            availability,
            LeaseAvailability::ExistingLease
                | LeaseAvailability::PlayMode
                | LeaseAvailability::HumanSlGame
                | LeaseAvailability::EngineStateUnrestored
        ) {
            return Ok(refused(&super::flash::reuse_status_key(availability)));
        }
        // Java first takes the foreground's lifecycle reservation (`startContributeEngine`): Web
        // 试下 or a GMA hand / its restore holding the engine refuses it with this message
        // (`beginExclusiveGtpLifecycleReservationInternal`, `beginReadBoardGmaSession`,
        // Leelaz.java:15190-15198, 21342-21357; LizzieFrame.java:16668-16670). Java's engine
        // object outlives its process, so a 试下 whose engine died still refuses it.
        if self.web_trial_active() || self.readboard.gma.reserved() {
            return Ok(refused("AnalysisSettings.reuseStatus.existing_lease"));
        }
        if self
            .contribution
            .session
            .as_ref()
            .is_some_and(|session| session.phase != ContributionPhase::Exited)
        {
            return Ok(refused("Contribute.tips.alreadyTraining"));
        }
        let mut settings = self.config.contribution.clone();
        if settings.user_name.is_empty() {
            self.contribution.notices.push(ContributionNotice::OpenSettings {
                key: "Contribute.tips.noUserName".into(),
            });
            return Ok(CommandResult::Done);
        }
        if settings.engine_path.trim().is_empty()
            && (!settings.use_command || settings.command.trim().is_empty())
        {
            self.contribution.notices.push(ContributionNotice::OpenSettings {
                key: "Contribute.tips.noEnginePath".into(),
            });
            return Ok(CommandResult::Done);
        }
        // A custom command runs `command`, not the path field (§1.5: unknown, allowed).
        if !settings.use_command {
            if let crate::api::ContributionQualification::Managed {
                status: crate::resources::CapabilityStatus::Unsupported,
                ..
            } = saved::qualification(&settings.engine_path, &self.storage.resources_dir)
            {
                return Ok(refused(RESOURCE_UNSUPPORTED));
            }
        }
        // A previous ended session's window is replaced.
        if let Some(mut previous) = self.contribution.session.take() {
            self.contribution_restore_estimate(&mut previous);
        }
        self.shutdown_engines(EngineScope::All);
        // EngineManager.isEmpty: the idle preloaded analysis process goes too.
        self.warmup_stop();
        self.quick_drop_warm();
        let size = self.doc.record.size;
        self.reopen_empty_board(size, false);
        let timestamp = chrono::Local::now().format("%Y-%m-%d-%H").to_string();
        let cwd = self
            .storage
            .save_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join("contribution");
        let _ = std::fs::create_dir_all(&cwd);
        let shown_command = sanitize(&contribute_command::build(&settings, password.as_ref().map(|_| "<redacted>")));
        settings.command = contribute_command::fill_passwords(&settings.command, kept_command);
        let command = contribute_command::build(&settings, password.as_deref());
        let remote = (settings.use_command && settings.remote).then(|| settings.ssh.clone());
        let id = self.contribution.next_id;
        self.contribution.next_id += 1;
        let estimate_saved = settings.show_estimate.then_some((self.estimate.state, self.estimate.hidden));
        if estimate_saved.is_some() {
            self.contribution_apply_estimate();
        }
        self.contribution.session = Some(Session {
            id,
            phase: ContributionPhase::Starting,
            paused: false,
            slow_quitting: false,
            tip_key: "ContributeView.lblTip".into(),
            eternal_tip: false,
            watching: None,
            games: Vec::new(),
            unparsed: Vec::new(),
            console_engine: CONSOLE_ENGINE.into(),
            engine_command: command,
            board: self.doc.id,
            shown_command,
            pure_command: settings.use_command && settings.use_pure_command,
            remote,
            cwd,
            timestamp,
            launches: 0,
            add_cacerts: false,
            normal_end: false,
            launch: Launch { worker: None },
            launch_error: String::new(),
            failure: output::Failure::Other,
            last_remote: None,
            terminal: false,
            output: Vec::new(),
            version_seen: false,
            result_hidden: false,
            last_pause_resume: None,
            watch_started: false,
            watch_tick: Instant::now(),
            watch_elapsed: Duration::ZERO,
            watch_cursor: None,
            pk_comments: HashMap::new(),
            estimate_saved,
        });
        self.contribution_start_launch();
        Ok(CommandResult::Done)
    }

    fn contribution_apply_estimate(&mut self) {
        let mut state = self.estimate.state;
        state.shown = true;
        if !state.on_main && !state.on_sub {
            state.on_main = true;
            state.on_sub = true;
        } else if !state.on_main && !self.config.panels.show_sub_board {
            state.on_main = true;
        }
        self.estimate = super::information::EstimateSession {
            state,
            hidden: false,
        };
        self.apply_estimate();
    }

    fn contribution_restore_estimate(&mut self, session: &mut Session) {
        if let Some((state, hidden)) = session.estimate_saved.take() {
            self.estimate = super::information::EstimateSession { state, hidden };
            self.apply_estimate();
        }
    }

    fn contribution_start_launch(&mut self) {
        let Some(session) = self.contribution.session.as_mut() else {
            return;
        };
        session.launches += 1;
        session.normal_end = false;
        session.eternal_tip = false;
        session.paused = false;
        session.phase = ContributionPhase::Starting;
        session.terminal = false;
        session.launch_error.clear();
        session.failure = output::Failure::Other;
        session.last_remote = None;
        session.output.clear();
        session.version_seen = false;
        session.tip_key = "ContributeView.lblTip".into();
        let command = session.engine_command.clone();
        let shown = session.shown_command.clone();
        let ssh = session.remote.clone();
        let cwd = session.cwd.clone();
        session.launch = Launch { worker: None };
        match self.engines.start_worker_in_dir(
            &command,
            &shown,
            CONSOLE_ENGINE,
            ssh.as_ref(),
            Some(cwd.as_path()),
        ) {
            Ok(worker) => {
                self.engines.report_worker_stderr(worker);
                if let Some(session) = self.contribution.session.as_mut() {
                    session.launch.worker = Some(worker);
                }
            }
            Err(error) => {
                if let Some(session) = self.contribution.session.as_mut() {
                    session.launch_error = error;
                }
                self.contribution_process_ended(false);
            }
        }
    }

    fn contribution_pause_resume(&mut self) -> CommandResult {
        let Some(session) = self.contribution.session.as_mut() else {
            return CommandResult::Unchanged;
        };
        if session.phase == ContributionPhase::Exited {
            return CommandResult::Unchanged;
        }
        let now = Instant::now();
        if session
            .last_pause_resume
            .is_some_and(|last| now.duration_since(last) < PAUSE_THROTTLE)
        {
            return refused("Contribute.pauseResumeTooFrequently");
        }
        session.last_pause_resume = Some(now);
        self.contribution_toggle_pause();
        CommandResult::Done
    }

    fn contribution_toggle_pause(&mut self) {
        let Some(session) = self.contribution.session.as_mut() else {
            return;
        };
        let line = if session.paused { "resume" } else { "pause" };
        if let Some(worker) = session.launch.worker {
            self.engines.worker_send(worker, line);
        }
        session.paused = !session.paused;
    }

    fn contribution_slow_quit(&mut self) -> CommandResult {
        let Some(session) = self.contribution.session.as_ref() else {
            return CommandResult::Unchanged;
        };
        if session.phase == ContributionPhase::Exited {
            return CommandResult::Unchanged;
        }
        // 顺序结束(延迟): a paused session resumes first (`ContributeView`'s button).
        if session.paused {
            self.contribution_toggle_pause();
        }
        let Some(session) = self.contribution.session.as_mut() else {
            return CommandResult::Unchanged;
        };
        if session.slow_quitting {
            return refused("Contribute.slowQuit.repeatedly");
        }
        if let Some(worker) = session.launch.worker {
            self.engines.worker_send(worker, "quit");
        }
        session.slow_quitting = true;
        CommandResult::Done
    }

    fn contribution_force_quit(&mut self) -> CommandResult {
        let Some(mut session) = self.contribution.session.take() else {
            return CommandResult::Unchanged;
        };
        session.terminal = true;
        if let Some(worker) = session.launch.worker.take() {
            self.engines.send_line_then_terminate(worker, "forcequit");
        }
        self.contribution_restore_estimate(&mut session);
        CommandResult::Done
    }

    fn contribution_close_view(&mut self) -> CommandResult {
        let Some(session) = self.contribution.session.as_ref() else {
            return CommandResult::Unchanged;
        };
        if session.phase != ContributionPhase::Exited {
            return refused("ContributeView.closeTip");
        }
        let mut session = self.contribution.session.take().unwrap();
        self.contribution_restore_estimate(&mut session);
        CommandResult::Done
    }

    fn contribution_watch_game(&mut self, index: i64) -> CommandResult {
        if index < 0 {
            return CommandResult::Unchanged;
        }
        let Some(session) = self.contribution.session.as_ref() else {
            return CommandResult::Unchanged;
        };
        if session.games.is_empty() {
            return CommandResult::Unchanged;
        }
        let target = (index as usize).min(session.games.len() - 1);
        if session.watching == Some(target) {
            return CommandResult::Unchanged;
        }
        self.contribution_set_watching(target);
        CommandResult::Done
    }

    fn contribution_toggle_result(&mut self) -> CommandResult {
        let Some(session) = self.contribution.session.as_mut() else {
            return CommandResult::Unchanged;
        };
        session.result_hidden = !session.result_hidden;
        CommandResult::Done
    }

    /// App exit (`end_engines`): stop the process without residue and drop the session silently.
    pub(super) fn contribution_end_for_exit(&mut self) {
        let Some(mut session) = self.contribution.session.take() else {
            return;
        };
        session.terminal = true;
        if let Some(worker) = session.launch.worker.take() {
            self.engines.send_line_then_terminate(worker, "forcequit");
        }
        self.contribution.notices.clear();
        self.contribution_restore_estimate(&mut session);
    }

    // -------------------------------------------------------------------- process

    pub(super) fn contribution_event(&mut self, event: RuntimeEvent) -> Option<RuntimeEvent> {
        let current = self
            .contribution
            .session
            .as_ref()
            .and_then(|session| session.launch.worker);
        match &event {
            RuntimeEvent::WorkerLine { instance, line } if Some(*instance) == current => {
                self.contribution_stdout(line);
                None
            }
            RuntimeEvent::WorkerErrorLine { instance, line } if Some(*instance) == current => {
                self.contribution_stderr(line);
                None
            }
            RuntimeEvent::WorkerExited {
                instance,
                output,
                remote,
                ..
            } if Some(*instance) == current => {
                self.contribution_exited(output, remote.as_ref());
                None
            }
            _ => Some(event),
        }
    }

    fn contribution_console(&mut self, line: &str, stderr: bool) {
        let Some(engine) = self
            .contribution
            .session
            .as_ref()
            .map(|session| session.console_engine.clone())
        else {
            return;
        };
        let kind = if stderr {
            ConsoleKind::Error
        } else {
            ConsoleKind::Output
        };
        self.engines.push_console(&engine, kind, sanitize(line));
    }

    fn contribution_stdout(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                return;
            };
            if let Some(error) = json.get("error") {
                let text = error
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| line.to_string());
                let bounded = output::bounded(&sanitize(&text), 4096);
                if let Some(session) = self.contribution.session.as_mut() {
                    session.launch_error = bounded.clone();
                }
                if self.contribution_classify(&bounded) {
                    return;
                }
                // consumeUiOutput: only an error object is shown; snapshots feed the games.
                self.contribution_console(line, false);
                return;
            }
            if snapshot::has_snapshot(&json) {
                self.contribution_ingest(&json);
            }
            return;
        }
        if self.contribution_classify(line) {
            return;
        }
        self.contribution_console(line, false);
        self.contribution_parse_line(line);
    }

    fn contribution_stderr(&mut self, line: &str) {
        if self.contribution_classify(line) {
            return;
        }
        self.contribution_console(line, true);
        if let Some(session) = self.contribution.session.as_mut() {
            session.output.push(sanitize(line));
            if session.output.len() > 30 {
                session.output.remove(0);
            }
        }
        self.contribution_parse_tips(line);
    }

    /// `classify`: records the failure/error; a permanent classification ends the session at once
    /// (no retry, no console line). Returns whether the session ended.
    fn contribution_classify(&mut self, line: &str) -> bool {
        let language = self.active_language;
        let classified = output::classify(line, &|key| contribution_text(language, key));
        let Some(session) = self.contribution.session.as_mut() else {
            return false;
        };
        if classified.failure != output::Failure::Other {
            session.failure = classified.failure;
        }
        if let Some(error) = classified.error {
            session.launch_error = error;
        }
        if session.failure.permanent() && !session.terminal {
            session.terminal = true;
            self.contribution_terminate_worker();
            self.contribution_present(false);
            return true;
        }
        false
    }

    fn contribution_parse_line(&mut self, line: &str) {
        if !self.contribution_parse_version(line) {
            return;
        }
        if line.contains("Finis") {
            self.contribution_finished(line);
        }
        self.contribution_parse_tips(line);
    }

    /// Returns `false` when the version made the session stop (the line needs no more work).
    fn contribution_parse_version(&mut self, line: &str) -> bool {
        let (seen, interesting) = match self.contribution.session.as_ref() {
            Some(session) => (session.version_seen, !session.version_seen && line.contains("KataGo")),
            None => return false,
        };
        if seen || !interesting {
            return true;
        }
        let params: Vec<&str> = line.split(' ').collect();
        if params.len() != 4 || !params[3].starts_with('v') {
            return true;
        }
        let version: Vec<&str> = params[3].split('.').collect();
        if version.len() != 3 {
            return true;
        }
        let (Ok(major), Ok(minor)) = (
            version[0][1..].parse::<u32>(),
            version[1].parse::<u32>(),
        ) else {
            return true;
        };
        if let Some(session) = self.contribution.session.as_mut() {
            session.version_seen = true;
        }
        if major <= 1 && minor < 11 {
            self.contribution.notices.push(ContributionNotice::OutOfDate {
                version: params[3].to_string(),
            });
            // Java's normalQuit + `contributeView.setVisible(false)`.
            let _ = self.contribution_force_quit();
            return false;
        }
        true
    }

    /// `parseTips`: the ordered tip updates and error texts of one line.
    fn contribution_parse_tips(&mut self, line: &str) {
        let parsed = output::tips(line);
        let language = self.active_language;
        let Some(session) = self.contribution.session.as_mut() else {
            return;
        };
        for key in parsed.errors {
            session.launch_error = contribution_text(language, key);
        }
        for tip in parsed.tips {
            match tip.kind {
                output::TipKind::Eternal => {
                    session.eternal_tip = true;
                    session.tip_key = tip.key.into();
                }
                output::TipKind::PauseResume => {
                    session.tip_key = tip.key.into();
                }
                output::TipKind::Normal => {
                    if !session.eternal_tip {
                        session.tip_key = tip.key.into();
                    }
                }
            }
        }
        if parsed.starting_game && session.phase != ContributionPhase::Exited {
            session.phase = ContributionPhase::Running;
        }
        if parsed.normal_end && !session.normal_end {
            session.normal_end = true;
            let key = "Contribute.tips.exitedAfterSignal".to_string();
            self.contribution
                .notices
                .push(ContributionNotice::Message { key });
        }
    }

    fn contribution_finished(&mut self, line: &str) {
        let params: Vec<&str> = line.split(' ').collect();
        let mut sgf = "";
        for index in 0..params.len().saturating_sub(1) {
            if params[index] == "sgf" {
                sgf = params[index + 1];
            }
        }
        if sgf.is_empty() {
            return;
        }
        let file = sgf.rsplit(['/', '\\']).next().unwrap_or(sgf);
        let game_id = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        let document = self.doc.id;
        let Some(session) = self.contribution.session.as_mut() else {
            return;
        };
        let Some(index) = session.games.iter().position(|game| game.id == game_id) else {
            return;
        };
        // A remote game's file is on the host: no result (Java `useJavaSSH`).
        let result = if session.remote.is_none() {
            read_result(&session.cwd, sgf)
        } else {
            String::new()
        };
        let game = &mut session.games[index];
        game.complete = true;
        game.result = result.clone();
        // Only the contribution's own board gets the held last move and result: a record the user
        // opened meanwhile (paused) is left alone, and the game shows when next opened.
        if session.watching != Some(index) || session.board != document {
            return;
        }
        self.contribution_play_held_last();
        // Java writes the result into the board only when it rebuilds a finished game: one that
        // finishes while watched keeps an empty `RE` and saves without its result. The result
        // belongs to the record, so it is written here too.
        if !result.is_empty() && self.doc.record.info.result != result {
            let _ = self.edit_document(DocumentEdit::SetResult { result });
        }
    }

    fn contribution_exited(&mut self, output: &[String], remote: Option<&RemoteFailure>) {
        let normal = {
            let Some(session) = self.contribution.session.as_mut() else {
                return;
            };
            if session.terminal {
                return;
            }
            session.launch.worker = None;
            if !output.is_empty() {
                session.output = output
                    .iter()
                    .rev()
                    .take(30)
                    .rev()
                    .map(|line| sanitize(line))
                    .collect();
            }
            session.last_remote = remote.cloned();
            session.normal_end || session.slow_quitting
        };
        self.contribution_process_ended(normal);
    }

    fn contribution_process_ended(&mut self, normal: bool) {
        let Some(session) = self.contribution.session.as_ref() else {
            return;
        };
        if session.terminal {
            return;
        }
        let retry = !normal && !session.failure.permanent() && session.launches < 4;
        if retry {
            let need_ca = session.failure == output::Failure::CaCertificates
                && !session.pure_command
                && session.remote.is_none()
                && !session.add_cacerts;
            if need_ca {
                let executable = prepare_launch(&session.engine_command, self.engines.roots())
                    .map(|spec| spec.program)
                    .unwrap_or_default();
                if let Some(session) = self.contribution.session.as_mut() {
                    session.add_cacerts = true;
                    session.engine_command = contribute_command::certificate_recovery(
                        &session.engine_command,
                        &executable,
                        false,
                        session.pure_command,
                    );
                    session.shown_command = contribute_command::certificate_recovery(
                        &session.shown_command,
                        &executable,
                        false,
                        session.pure_command,
                    );
                }
            }
            self.contribution_start_launch();
        } else {
            if let Some(session) = self.contribution.session.as_mut() {
                session.terminal = true;
            }
            self.contribution_present(normal);
        }
    }

    fn contribution_terminate_worker(&mut self) {
        let worker = self
            .contribution
            .session
            .as_mut()
            .and_then(|session| session.launch.worker.take());
        if let Some(worker) = worker {
            self.engines.terminate(worker);
        }
    }

    /// The process ended for good: the window shows 关闭此界面; an abnormal end adds the failure.
    fn contribution_present(&mut self, normal: bool) {
        let language = self.active_language;
        let Some(session) = self.contribution.session.as_mut() else {
            return;
        };
        session.phase = ContributionPhase::Exited;
        session.paused = false;
        let message = if session.launch_error.is_empty() {
            java_text(language, "Leelaz.engineEndUnormalHint")
        } else {
            session.launch_error.clone()
        };
        let failure = (!normal).then(|| EngineFailureView {
            engine_name: CONSOLE_ENGINE.into(),
            command: session.shown_command.clone(),
            during_startup: session.games.is_empty(),
            message: sanitize(&message),
            output: session.output.clone(),
            repair: None,
            remote: session.last_remote.clone(),
            profile_id: String::new(),
        });
        let saved = session.estimate_saved.take();
        if let Some(failure) = failure {
            self.contribution
                .notices
                .push(ContributionNotice::Failed { failure: Box::new(failure) });
        }
        if let Some((state, hidden)) = saved {
            self.estimate = super::information::EstimateSession { state, hidden };
            self.apply_estimate();
        }
    }

    // -------------------------------------------------------------------- games

    fn contribution_ingest(&mut self, json: &serde_json::Value) {
        let at_end = self.cursor == self.doc.record.line_end(ROOT);
        let Some(session) = self.contribution.session.as_mut() else {
            return;
        };
        let ingest = snapshot::ingest(&mut session.games, &mut session.unparsed, json);
        match ingest {
            snapshot::Ingest::Ignored => {}
            snapshot::Ingest::Added(index) => {
                let first = self
                    .contribution
                    .session
                    .as_ref()
                    .is_some_and(|session| session.watching.is_none());
                if let Some(session) = self.contribution.session.as_mut() {
                    if session.phase != ContributionPhase::Exited {
                        session.phase = ContributionPhase::Running;
                    }
                }
                // getJsonGameInfo: a later game only joins the list; the board stays on the
                // watched one.
                if first {
                    self.contribution_set_watching(index);
                }
            }
            snapshot::Ingest::Updated(index) => {
                if let Some(session) = self.contribution.session.as_mut() {
                    if session.phase != ContributionPhase::Exited {
                        session.phase = ContributionPhase::Running;
                    }
                }
                if self
                    .contribution
                    .session
                    .as_ref()
                    .is_some_and(|session| session.watching == Some(index))
                {
                    self.contribution_set_game(index, false);
                    if self.config.contribution.watch.always_last_move && at_end {
                        self.contribution_goto_line_end();
                    }
                }
            }
        }
    }

    fn contribution_set_watching(&mut self, index: usize) {
        if let Some(session) = self.contribution.session.as_mut() {
            session.watch_started = true;
            session.watch_tick = Instant::now();
            session.watch_elapsed = Duration::ZERO;
        }
        self.contribution_set_game(index, false);
        if self.config.contribution.watch.always_last_move {
            self.contribution_goto_line_end();
        }
        if let Some(session) = self.contribution.session.as_mut() {
            session.watch_cursor = Some(self.cursor);
        }
    }

    /// `setGameToBoard`: append what the live board is missing, or rebuild the record for another
    /// game.
    fn contribution_set_game(&mut self, index: usize, save_game: bool) {
        let Some(game) = self
            .contribution
            .session
            .as_ref()
            .and_then(|session| session.games.get(index))
            .cloned()
        else {
            return;
        };
        let Some((previous, board)) = self
            .contribution
            .session
            .as_ref()
            .map(|session| (session.watching, session.board))
        else {
            return;
        };
        // Only the contribution's own board is continued; a record the user opened meanwhile
        // (paused) is taken over by a rebuild, like Java's takeover of another position.
        let remain = (board == self.doc.id).then(|| self.contribution_remain(&game)).flatten();
        let same = previous == Some(index) && remain.is_some();
        if let Some(session) = self.contribution.session.as_mut() {
            session.watching = Some(index);
        }
        if same {
            let remain = remain.unwrap_or_default();
            if !remain.is_empty() {
                self.contribution_append(&game, &remain, !game.complete);
                if self.config.contribution.watch.always_last_move {
                    self.contribution_goto_line_end();
                }
            }
            return;
        }
        self.contribution_rebuild(&game);
        let document = self.doc.id;
        if let Some(session) = self.contribution.session.as_mut() {
            session.board = document;
        }
        if !save_game && self.config.contribution.auto_save && game.complete && !game.saved {
            self.contribution_save_game(index);
        }
    }

    /// `isContributeGameAndCurrentBoardSame`: `None` when the board is another game/position; the
    /// moves after the common prefix otherwise.
    fn contribution_remain(&self, game: &snapshot::Game) -> Option<Vec<snapshot::Move>> {
        let record = &self.doc.record;
        if record.size.width != game.width || record.size.height != game.height {
            return None;
        }
        let mut actions = Vec::new();
        let mut node = ROOT;
        while let Some(next) = record.main_child(node) {
            node = next;
            if record.node(node).kind.is_action() {
                actions.push(node);
            }
        }
        let mut moves = game.init.clone();
        moves.extend(game.moves.clone());
        let mut compared = 0;
        while compared < actions.len() && compared < moves.len() {
            if !contribution_node_matches(record.node(actions[compared]).kind, &moves[compared]) {
                return None;
            }
            compared += 1;
        }
        if compared < actions.len() {
            return Some(Vec::new());
        }
        Some(moves[compared..].to_vec())
    }

    fn contribution_rebuild(&mut self, game: &snapshot::Game) {
        let size = crate::record::BoardSize {
            width: game.width,
            height: game.height,
        };
        self.reopen_empty_board(size, false);
        let _ = self.edit_document(DocumentEdit::SetPlayers {
            black: game.black.replace(' ', ""),
            white: game.white.replace(' ', ""),
        });
        self.apply_komi(game.komi);
        let language = self.active_language;
        let rules_text = rules::rules_text(&game.rules, &|key| contribution_text(language, key));
        let comment = format!(
            "{rules_text}\r\n{} {}",
            java_text(language, "ContributeView.lblKomi"),
            java_double(game.komi)
        );
        let _ = self.edit_document(DocumentEdit::SetComment {
            node: ROOT,
            comment,
        });
        if game.complete {
            let _ = self.edit_document(DocumentEdit::SetResult {
                result: game.result.clone(),
            });
        }
        self.contribution_append(game, &game.init, false);
        self.contribution_append(game, &game.moves, !game.complete);
        let mut node = ROOT;
        while self.store.get(&self.doc.record, node).is_none() {
            match self.doc.record.main_child(node) {
                Some(next) => node = next,
                None => break,
            }
        }
        self.cursor = node;
        self.sync_engine();
    }

    fn contribution_append(&mut self, game: &snapshot::Game, moves: &[snapshot::Move], hold_last: bool) {
        for (index, mv) in moves.iter().enumerate() {
            let at = self.doc.record.line_end(ROOT);
            if let Some(analysis) = &mv.analysis {
                self.contribution_attach(at, game, analysis);
            }
            if hold_last && index + 1 == moves.len() {
                break;
            }
            self.contribution_place(at, game, mv);
        }
    }

    /// One move of `game`, under the game's own rules: its JSON `rules.suicide` decides whether a
    /// multi-stone suicide stands (Java read the last engine's `canSuicidal`, which no longer
    /// runs once the contribution started).
    fn contribution_place(&mut self, at: NodeId, game: &snapshot::Game, mv: &snapshot::Move) {
        let color = if mv.black {
            Stone::Black
        } else {
            Stone::White
        };
        let edit = match mv.point {
            Some(point) => DocumentEdit::Play {
                at,
                color,
                point,
                alternate: false,
                rules: crate::record::PlayRules {
                    allow_suicide: game.rules.get("suicide").and_then(serde_json::Value::as_bool) == Some(true),
                    no_capture: self.config.edit.no_capture,
                },
            },
            None => DocumentEdit::Pass { at, color },
        };
        let _ = self.edit_document(edit);
    }

    fn contribution_attach(
        &mut self,
        at: NodeId,
        game: &snapshot::Game,
        analysis: &snapshot::Analysis,
    ) {
        let engine = if analysis.to_play == Stone::Black {
            game.black.clone()
        } else {
            game.white.clone()
        };
        let visits = analysis.candidates.iter().map(|candidate| candidate.visits).sum();
        let mut view = NodeAnalysisView {
            node: at,
            engine: engine.clone(),
            to_play: analysis.to_play,
            visits,
            root_visits: None,
            winrate: analysis.winrate,
            score_lead: analysis.score_lead,
            score_stdev: analysis.score_stdev,
            komi: game.komi,
            candidates: analysis
                .candidates
                .iter()
                .map(|candidate| CandidateView {
                    point: candidate.point,
                    visits: candidate.visits,
                    allocation_visits: candidate.allocation_visits,
                    winrate: candidate.winrate,
                    score_lead: candidate.score_lead,
                    score_stdev: candidate.score_stdev,
                    prior: candidate.prior,
                    lcb: Some(candidate.lcb),
                    order: candidate.order,
                    pv: candidate.pv.clone(),
                    pv_visits: candidate.pv_visits.clone(),
                    pv_captures: Vec::new(),
                    shown: Default::default(),
                    side_to_move: Default::default(),
                    ownership: candidate.ownership.clone(),
                    kept: false,
                })
                .collect(),
            black: Default::default(),
        };
        let limit = self.config.engine.analysis.limit_branch_length;
        super::analysis::fill_pv_captures(
            &mut view,
            &self.doc.record.position(at),
            limit,
            !self.config.edit.no_capture,
        );
        let stamp = self.doc.record.node(at).stamp;
        self.store
            .insert(at, stamp, view.clone(), analysis.ownership.clone());
        if visits > 0 {
            self.contribution_write_pk(at, &view);
        }
    }

    /// `SGFParser.formatCommentPk` into the analysed node, replacing this session's earlier
    /// summary of the same node.
    fn contribution_write_pk(&mut self, at: NodeId, view: &NodeAnalysisView) {
        let language = self.active_language;
        let labels = crate::api::CommentLabels {
            black: java_text(language, "SGFParse.black"),
            white: java_text(language, "SGFParse.white"),
            winrate: java_text(language, "SGFParse.winrate"),
            lead: java_text(language, "SGFParse.leadJustScore"),
            lead_with_komi: java_text(language, "SGFParse.leadWithKomi"),
            stdev: java_text(language, "SGFParse.stdev"),
            playouts: java_text(language, "SGFParse.playouts"),
            komi: java_text(language, "SGFParse.komi"),
            pda: java_text(language, "SGFParse.pda"),
        };
        let previous = {
            let record = &self.doc.record;
            record
                .parent(at)
                .and_then(|parent| record.parent(parent))
                .and_then(|grand| self.store.get(record, grand))
                .map(|view| comment::PkEval {
                    engine: view.engine.clone(),
                    visits: view.visits,
                    winrate: view.winrate,
                    score_lead: view.score_lead,
                    score_stdev: view.score_stdev,
                    komi: view.komi,
                    to_play: view.to_play,
                })
        };
        let current = comment::PkEval {
            engine: view.engine.clone(),
            visits: view.visits,
            winrate: view.winrate,
            score_lead: view.score_lead,
            score_stdev: view.score_stdev,
            komi: view.komi,
            to_play: view.to_play,
        };
        let doc = self.doc.id;
        let old_comment = self.doc.record.node(at).comment.clone();
        let stripped = self
            .contribution
            .session
            .as_ref()
            .and_then(|session| session.pk_comments.get(&(doc, at)))
            .and_then(|previous| old_comment.strip_suffix(previous))
            .map(|base| base.trim_end().to_string());
        let base = stripped.unwrap_or(old_comment);
        let text = comment::format(
            &labels,
            self.config.display.score_lead_with_komi,
            &current,
            previous.as_ref(),
            &base,
            at == ROOT,
        );
        if text != self.doc.record.node(at).comment {
            let _ = self.edit_document(DocumentEdit::SetComment {
                node: at,
                comment: text.clone(),
            });
        }
        if let Some(session) = self.contribution.session.as_mut() {
            session.pk_comments.insert((doc, at), text);
        }
    }

    fn contribution_play_held_last(&mut self) {
        let Some(game) = self
            .contribution
            .session
            .as_ref()
            .and_then(Session::current)
            .cloned()
        else {
            return;
        };
        let remain = self.contribution_remain(&game).unwrap_or_default();
        if !remain.is_empty() {
            self.contribution_append(&game, &remain, false);
            if self.config.contribution.watch.always_last_move {
                self.contribution_goto_line_end();
            }
        }
    }

    fn contribution_goto_line_end(&mut self) -> bool {
        let end = self.doc.record.line_end(ROOT);
        if self.cursor == end {
            return false;
        }
        self.cursor = end;
        self.sync_engine();
        true
    }

    fn contribution_next_move(&mut self) -> bool {
        let Some(next) = self.doc.record.main_child(self.cursor) else {
            return false;
        };
        self.cursor = next;
        self.sync_engine();
        true
    }

    fn contribution_goto_move_number(&mut self, number: u32) {
        let record = &self.doc.record;
        let mut node = ROOT;
        let mut target = None;
        loop {
            if record.node(node).move_number == number {
                target = Some(node);
                break;
            }
            match record.main_child(node) {
                Some(next) => node = next,
                None => break,
            }
        }
        if let Some(target) = target {
            if self.cursor != target {
                self.cursor = target;
                self.sync_engine();
            }
        }
    }

    // -------------------------------------------------------------------- saving

    fn contribution_folder(&self) -> PathBuf {
        let timestamp = self
            .contribution
            .session
            .as_ref()
            .map(|session| session.timestamp.clone())
            .unwrap_or_default();
        self.storage
            .save_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join("ContributeGames")
            .join(timestamp)
    }

    fn contribution_save_all(&mut self) -> CommandResult {
        let Some(session) = self.contribution.session.as_ref() else {
            return CommandResult::Unchanged;
        };
        if session.games.is_empty() {
            return refused("Contribute.tips.noGameToSave");
        }
        let total = session.games.len();
        let move_number = self.doc.record.node(self.cursor).move_number;
        for index in 0..total {
            self.contribution_save_game(index);
        }
        if let Some(watching) = self
            .contribution
            .session
            .as_ref()
            .and_then(|session| session.watching)
        {
            self.contribution_set_game(watching, false);
        }
        self.contribution_goto_move_number(move_number);
        let folder = self.contribution_folder().to_string_lossy().into_owned();
        if self.contribution.saves.is_empty() {
            self.contribution.notices.push(ContributionNotice::Saved { folder });
        } else {
            self.contribution.saved_folder = Some(folder);
        }
        CommandResult::Done
    }

    /// Queues game `index` (written into the document first) for the serial writer, in
    /// `ContributeGames/<hour>/` (Java `mkdirs`); a complete game counts as saved unless the write
    /// fails.
    fn contribution_save_game(&mut self, index: usize) {
        // The game is written into the contribution's own board first (rebuilt unless it is the
        // watched game there), so another record is never saved under its name.
        let on_board = self.contribution.session.as_ref().is_some_and(|session| {
            session.watching == Some(index) && session.board == self.doc.id
        });
        if !on_board {
            if let Some(session) = self.contribution.session.as_mut() {
                session.watching = None;
            }
            self.contribution_set_game(index, true);
        }
        let Some((session_id, game_id, complete)) = self.contribution.session.as_mut().and_then(|session| {
            let id = session.id;
            session.games.get_mut(index).map(|game| {
                if game.complete {
                    game.saved = true;
                }
                (id, game.id.clone(), game.complete)
            })
        }) else {
            return;
        };
        let record = &self.doc.record;
        let name = format!(
            "{}_{}_{}_VS_{}({}).sgf",
            index + 1,
            game_id,
            short_name(&record.info.black),
            short_name(&record.info.white),
            record.info.result
        );
        let path = self.contribution_folder().join(safe_name(&name));
        let text = crate::formats::sgf_write::write(record, "LizzieYzy Tauri");
        let ticket = self.next_save;
        self.next_save += 1;
        self.saves.push(PendingSave {
            ticket,
            document: self.doc.id,
            revision: 0,
            path: path.to_string_lossy().into_owned(),
            export: None,
        });
        self.contribution
            .saves
            .push((ticket, complete.then_some((session_id, index))));
        self.contribution.jobs.push(SaveJob {
            ticket,
            path,
            create_parent: true,
            text,
        });
    }

    pub(super) fn take_contribution_saves(&mut self) -> Vec<SaveJob> {
        std::mem::take(&mut self.contribution.jobs)
    }

    /// A contribution record's write finished: a failed complete game may be saved again, and
    /// 保存全部棋谱 is announced once its last write is done (any failure is reported instead).
    /// Returns `false` for another save.
    pub(super) fn contribution_save_finished(&mut self, ticket: u64, result: &Result<(), String>) -> bool {
        let Some(at) = self.contribution.saves.iter().position(|(t, _)| *t == ticket) else {
            return false;
        };
        let (_, game) = self.contribution.saves.remove(at);
        if let Err(detail) = result {
            self.contribution.save_failed = true;
            self.log(true, format!("saving the contribution game failed: {detail}"));
            if let (Some((id, index)), Some(session)) = (game, self.contribution.session.as_mut()) {
                if session.id == id {
                    if let Some(game) = session.games.get_mut(index) {
                        game.saved = false;
                    }
                }
            }
        }
        if self.contribution.saves.is_empty() {
            let failed = std::mem::take(&mut self.contribution.save_failed);
            if let Some(folder) = self.contribution.saved_folder.take() {
                self.contribution.notices.push(if failed {
                    ContributionNotice::Message {
                        key: super::SAVE_FILE_FAILED.into(),
                    }
                } else {
                    ContributionNotice::Saved { folder }
                });
            }
        }
        true
    }

    // -------------------------------------------------------------------- watching clock

    /// The watching thread (`startWatchingGameThread`) runs while the window exists: after a clean
    /// end too; only 强制结束 (which removes the session) stops it.
    pub(super) fn contribution_deadline(&self) -> Option<Instant> {
        let session = self.contribution.session.as_ref()?;
        if !session.watch_started {
            return None;
        }
        Some(session.watch_tick + WATCH_INTERVAL)
    }

    pub(super) fn contribution_tick(&mut self, now: Instant) -> bool {
        enum Step {
            Stay,
            Advance,
            Switch(usize),
        }
        let watch = self.config.contribution.watch.clone();
        let cursor = self.cursor;
        let has_next = self.doc.record.main_child(cursor).is_some();
        let step = {
            let Some(session) = self.contribution.session.as_mut() else {
                return false;
            };
            if !session.watch_started {
                return false;
            }
            let delta = now.saturating_duration_since(session.watch_tick);
            session.watch_tick = now;
            session.watch_elapsed += delta;
            if !watch.auto_play
                || !watch.auto_play_interval.is_finite()
                || watch.auto_play_interval <= 0.0
            {
                return false;
            }
            let interval = Duration::from_secs_f64(watch.auto_play_interval);
            if session.watch_elapsed < interval {
                return false;
            }
            session.watch_elapsed = Duration::ZERO;
            let unchanged = session.watch_cursor == Some(cursor);
            let complete = session.current().is_some_and(|game| game.complete);
            let mut next_game = None;
            if unchanged && !has_next && watch.auto_play_next_game && complete {
                let mut index = session.watching.unwrap_or(0);
                loop {
                    index += 1;
                    let Some(game) = session.games.get(index) else {
                        break;
                    };
                    if watch.skip_non19 && (game.width != 19 || game.height != 19) {
                        continue;
                    }
                    next_game = Some(index);
                    break;
                }
            }
            if unchanged && has_next {
                Step::Advance
            } else if let Some(index) = next_game {
                Step::Switch(index)
            } else {
                session.watch_cursor = Some(cursor);
                Step::Stay
            }
        };
        let changed = match step {
            Step::Advance => self.contribution_next_move(),
            Step::Switch(index) => {
                self.contribution_set_watching(index);
                true
            }
            Step::Stay => return false,
        };
        if let Some(session) = self.contribution.session.as_mut() {
            session.watch_cursor = Some(self.cursor);
        }
        changed
    }

    // -------------------------------------------------------------------- gates

    /// What an editing command does while a contribution runs (`ContributeView` makes the board
    /// read-only): comments and marks still go through, board changes do nothing.
    pub(super) fn contribution_edit_refusal(&self, op: &EditCommand) -> Option<CommandResult> {
        if !self.contribution_active() {
            return None;
        }
        match op {
            EditCommand::SetComment { .. }
            | EditCommand::ToggleMark { .. }
            | EditCommand::ClearMarks
            | EditCommand::ClearEditCache => None,
            EditCommand::NewBoard { .. }
            | EditCommand::ClearBoard
            | EditCommand::SetBoardSize { .. } => Some(refused(CONTRIBUTING)),
            _ => Some(CommandResult::Unchanged),
        }
    }

    /// Commands a running contribution refuses (`LizzieFrame`'s `isContributing` gates).
    pub(super) fn contribution_gate(&mut self, command: &Command) -> Option<CommandResult> {
        match command {
            Command::Contribution { .. }
            | Command::Navigate { .. }
            | Command::GotoNode { .. }
            | Command::GotoMoveNumber { .. }
            | Command::JumpToProblemMove { .. }
            | Command::SaveRecord { .. }
            | Command::ChooseAndSaveRecord { .. }
            | Command::SaveRecordTo { .. }
            | Command::ExportRecord { .. }
            | Command::ExportRecordTo { .. }
            | Command::CopyRecord
            | Command::ExportImage { .. }
            | Command::ExportImageTo { .. }
            | Command::CopyImage { .. }
            | Command::SetKomi { .. }
            | Command::SetDisplaySettings { .. }
            | Command::SetKataEstimate { .. }
            | Command::TogglePanel { .. }
            | Command::SetPanelSettings { .. }
            | Command::SetPanelSizes { .. } => None,
            Command::Edit { op } => self.contribution_edit_refusal(op),
            Command::ChooseAndOpenRecord
            | Command::OpenRecord { .. }
            | Command::PasteRecord { .. }
            | Command::Recovery {
                op: crate::api::RecoveryCommand::LoadSlot { .. },
            }
            | Command::Game { .. }
            | Command::Match { .. }
            | Command::HumanSl { .. }
            | Command::SwitchEngine { .. }
            | Command::RestartEngine
            | Command::ShutdownEngines { .. }
            | Command::SwitchSecondaryEngine { .. }
            | Command::RestartSecondaryEngine
            | Command::ShutdownSecondaryEngine
            | Command::SaveEngineSettings { .. }
            | Command::StartFlashAnalysis { .. }
            | Command::RedoFlashAnalysis
            | Command::StartAutoAnalysis { .. }
            | Command::StartWholeGameAnalysis { .. }
            | Command::ResumeWholeGameAnalysis
            | Command::Batch { .. } => Some(refused(CONTRIBUTING)),
            Command::Tuning {
                op: crate::api::TuningCommand::StartBenchmark { .. },
            } => Some(refused(CONTRIBUTING)),
            _ => None,
        }
    }
}

fn contribution_node_matches(kind: NodeKind, mv: &snapshot::Move) -> bool {
    let color = if mv.black {
        Stone::Black
    } else {
        Stone::White
    };
    match kind {
        NodeKind::Move { color: node_color, point } => {
            !mv.pass && node_color == color && Some(point) == mv.point
        }
        NodeKind::Pass { color: node_color } => mv.pass && node_color == color,
        _ => false,
    }
}

use super::exporting::java_text;

/// The Java texts `rules`/`output` name by key, spelled out so `tools/extract_java_strings.py`
/// bundles them (it collects literal `java_text` keys only).
fn contribution_text(language: crate::settings::Language, key: &str) -> String {
    match key {
        "ContributeView.rules.scoring" => java_text(language, "ContributeView.rules.scoring"),
        "ContributeView.rules.scoring.area" => java_text(language, "ContributeView.rules.scoring.area"),
        "ContributeView.rules.scoring.territory" => java_text(language, "ContributeView.rules.scoring.territory"),
        "ContributeView.rules.ko" => java_text(language, "ContributeView.rules.ko"),
        "ContributeView.rules.ko.simple" => java_text(language, "ContributeView.rules.ko.simple"),
        "ContributeView.rules.ko.situational" => java_text(language, "ContributeView.rules.ko.situational"),
        "ContributeView.rules.ko.positional" => java_text(language, "ContributeView.rules.ko.positional"),
        "ContributeView.rules.suicide" => java_text(language, "ContributeView.rules.suicide"),
        "ContributeView.rules.yes" => java_text(language, "ContributeView.rules.yes"),
        "ContributeView.rules.no" => java_text(language, "ContributeView.rules.no"),
        "ContributeView.rules.tax" => java_text(language, "ContributeView.rules.tax"),
        "ContributeView.rules.tax.none" => java_text(language, "ContributeView.rules.tax.none"),
        "ContributeView.rules.tax.all" => java_text(language, "ContributeView.rules.tax.all"),
        "ContributeView.rules.tax.seki" => java_text(language, "ContributeView.rules.tax.seki"),
        "ContributeView.rules.whiteHandicapBonus" => java_text(language, "ContributeView.rules.whiteHandicapBonus"),
        "ContributeView.rules.whiteHandicapBonus.0" => java_text(language, "ContributeView.rules.whiteHandicapBonus.0"),
        "ContributeView.rules.whiteHandicapBonus.N" => java_text(language, "ContributeView.rules.whiteHandicapBonus.N"),
        "ContributeView.rules.whiteHandicapBonus.N-1" => java_text(language, "ContributeView.rules.whiteHandicapBonus.N-1"),
        "ContributeView.rules.button" => java_text(language, "ContributeView.rules.button"),
        "Contribute.tips.useOfficialEngineSupportDistributedTraining" => {
            java_text(language, "Contribute.tips.useOfficialEngineSupportDistributedTraining")
        }
        "Contribute.tips.buildRejected" => java_text(language, "Contribute.tips.buildRejected"),
        "Contribute.tips.noCAcerts" => java_text(language, "Contribute.tips.noCAcerts"),
        "Contribute.tips.invalidUserNamePassword" => java_text(language, "Contribute.tips.invalidUserNamePassword"),
        "Contribute.tips.noResponseFromServer" => java_text(language, "Contribute.tips.noResponseFromServer"),
        "Contribute.tips.errorConnectingToServer" => java_text(language, "Contribute.tips.errorConnectingToServer"),
        "Contribute.tips.uploadError" => java_text(language, "Contribute.tips.uploadError"),
        "Contribute.tips.initialQuery" => java_text(language, "Contribute.tips.initialQuery"),
        "Contribute.tips.couldNotParseServerUrl" => java_text(language, "Contribute.tips.couldNotParseServerUrl"),
        "Contribute.tips.didYouVerifyYourEmailAddress" => {
            java_text(language, "Contribute.tips.didYouVerifyYourEmailAddress")
        }
        "Contribute.tips.modelFileWasIncompletelyDownloaded" => {
            java_text(language, "Contribute.tips.modelFileWasIncompletelyDownloaded")
        }
        _ => java_text(language, key),
    }
}
