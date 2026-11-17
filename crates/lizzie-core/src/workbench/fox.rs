//! Fox live play. Workbench is the only writer of transport, protocol, reservation and document
//! state. Engine choices are never document edits: only the protocol's confirmed action history
//! crosses the existing DocumentEdit seam. The display cursor never becomes the search target.
use std::{collections::VecDeque, sync::Arc, time::{Duration, Instant}};
use super::{Workbench, Document};
use crate::{api::*, engine::{rules::KataRules, runtime::{InstanceId, Phase, RuntimeEvent}}, fox::{GameState, Listener, Snapshot, SubmissionFact, TransportEvent}, record::{DocumentEdit, NodeId, PlayRules, Point, Stone, ROOT}};

const TIMEOUT: Duration = Duration::from_secs(5);
const UNLIMITED_TIME: f64 = 1e20;
const UNLIMITED_VISITS: u64 = 1_125_899_906_842_624;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Idle, Inspect, Setup, Ready, Observe, Restore, Releasing }
enum Check { Ack, Commands, TimeModes, Rules, Time, Visits, Pda, VerifyRules(KataRules), Number(f64), PdaValue(f64), Selection(Snapshot, u64) }
enum Step { Send(String, Check), Ready, InspectDone, RestoreDone, TimeLeft(Snapshot), Search(Snapshot) }
struct Await { id: u64, check: Check, until: Instant }
pub(super) struct FoxFlow {
    epoch: Instant, clock: Instant, next_tick: Option<Instant>, listener: Option<Listener>, token: u64,
    state: GameState, mode: Mode, reservation: Option<u64>, instance: Option<InstanceId>,
    inspected_instance: Option<InstanceId>, captured_rules: Option<KataRules>, selected_rules: Option<KataRules>,
    captured_time: Option<f64>, captured_visits: Option<u64>, selected_time: f64, selected_visits: u64,
    desired_pda: Option<f64>, confirmed_pda: Option<f64>, pda_status: String, pda_updating: bool,
    queue: VecDeque<Step>, awaiting: Option<Await>, search: u64, searched: Option<(u64,u64,u64)>,
    search_snapshot: Option<Snapshot>, refresh_deadline: Option<u64>,
    status: String, final_status: String, outcome: Option<FoxStartOutcome>, owns: bool,
    adopted: Option<u64>, applied: usize, live: Option<NodeId>, document: Option<u64>,
    candidates: Option<NodeAnalysisView>, transcript: VecDeque<String>, transcript_bytes: usize, port: u16,
}
impl Default for FoxFlow {
    fn default() -> Self { Self {
        epoch: Instant::now(), clock: Instant::now(), next_tick: None, listener: None, token: 0, state: GameState::default(),
        mode: Mode::Idle, reservation: None, instance: None, inspected_instance: None,
        captured_rules: None, selected_rules: None, captured_time: None, captured_visits: None,
        selected_time: UNLIMITED_TIME, selected_visits: UNLIMITED_VISITS,
        desired_pda: None, confirmed_pda: None, pda_status: String::new(), pda_updating: false, queue: VecDeque::new(), awaiting: None,
        search: 0, searched: None, search_snapshot: None, refresh_deadline: None,
        status: "Fox.waiting".into(), final_status: String::new(), outcome: None, owns: false,
        adopted: None, applied: 0, live: None, document: None, candidates: None, transcript: VecDeque::new(), transcript_bytes: 0, port: 7000,
    } }
}
fn refused(key: &str) -> CommandResult { CommandResult::Refused { message_key: key.into() } }
fn color(c: char) -> Stone { if c == 'B' { Stone::Black } else { Stone::White } }
impl Workbench {
    fn fox_instant(&self) -> Instant { self.fox.clock.max(Instant::now()) }
    fn fox_now(&self) -> u64 { self.fox_instant().saturating_duration_since(self.fox.epoch).as_nanos().min(u64::MAX as u128) as u64 }
    pub(super) fn fox_owns_board(&self) -> bool { self.fox.owns }
    pub(super) fn fox_holds_engine(&self) -> bool { self.fox.reservation.is_some() }
    pub(super) fn fox_deadline(&self) -> Option<Instant> { self.fox.next_tick }
    pub(super) fn fox_can_browse(&self) -> bool {
        (!self.fox.owns && !self.fox_holds_engine()) || (self.fox.live.is_some() && !matches!(self.fox.mode, Mode::Restore | Mode::Releasing))
    }
    fn fox_log(&mut self, text: &str) {
        let safe = crate::sanitize::sanitize(&format!("[{}] {text}", self.fox.state.snapshot().connection));
        let bounded: String = safe.chars().take(2048).filter(|c| !c.is_control() || *c == '\t').collect();
        while self.fox.transcript.len() >= 160 || self.fox.transcript_bytes + bounded.len() > 65_536 {
            let Some(old) = self.fox.transcript.pop_front() else { break; };
            self.fox.transcript_bytes -= old.len();
        }
        self.fox.transcript_bytes += bounded.len(); self.fox.transcript.push_back(bounded);
    }
    /// Shared command admission; saves and display-only browsing remain available. Direct public
    /// mutation methods enforce the same ownership, rather than trusting the Tauri adapter.
    pub(super) fn fox_gate(&mut self, command: &Command) -> Option<CommandResult> {
        if !self.fox.owns && !self.fox_holds_engine() { return None; }
        match command {
            Command::Fox { .. } | Command::SaveRecord { .. } | Command::ChooseAndSaveRecord { .. }
            | Command::SaveRecordTo { .. } | Command::CopyRecord | Command::ExportRecord { .. }
            | Command::ExportRecordTo { .. } | Command::ExportImage { .. } | Command::ExportImageTo { .. }
            | Command::CopyImage { .. } | Command::SetLanguage { .. } | Command::SetPlaySound { .. }
            | Command::SetDisplaySettings { .. } | Command::SetFrameFontSize { .. }
            | Command::CommitLayout { .. } | Command::ApplyCustomLayout { .. }
            | Command::ToggleIndependentBoard { .. } | Command::CloseIndependentBoard { .. }
            | Command::OpenLogFolder | Command::ExportDiagnostics | Command::SetReplaceFileHint { .. }
            | Command::SetConsoleVisible { .. } | Command::SetQuickLinks { .. } | Command::LaunchQuickLink { .. }
            | Command::ChooseQuickLinkProgram { .. } => None,
            Command::Navigate { .. } | Command::GotoNode { .. } | Command::GotoMoveNumber { .. }
            | Command::JumpToProblemMove { .. } => if self.fox.live.is_some() && self.fox.mode != Mode::Restore && self.fox.mode != Mode::Releasing { None } else { Some(CommandResult::Unchanged) },
            Command::SendGtpCommand { command } if command.trim().starts_with("kata-set-param playoutDoublingAdvantage ") => None,
            _ => Some(refused("Fox.conflict")),
        }
    }
    pub fn fox(&mut self, op: FoxCommand) -> Result<CommandResult, ApiError> {
        match op {
            FoxCommand::Prepare => {
                if self.fox.listener.is_some() || self.fox_holds_engine() { return Ok(CommandResult::Unchanged); }
                self.fox.inspected_instance = None; self.fox.selected_rules = None; self.fox.capture_reset();
                if !self.fox_admit_engine() { self.fox.status = "Fox.engineUnavailable".into(); return Ok(CommandResult::Done); }
                self.fox.mode = Mode::Inspect;
                self.fox.status = "SetKataRules.status.pending".into();
                self.fox_capture_queue(); self.fox.queue.push_back(Step::InspectDone);
                self.fox_pump();
            }
            FoxCommand::Start { port, max_time, max_visits, warning_confirmed, replace, document, revision } => {
                if self.fox.listener.is_some() || self.fox_holds_engine() { return Ok(CommandResult::Unchanged); }
                self.fox.outcome = None;
                if port == 0 { return Ok(self.fox_start_failed("Fox.invalidPort")); }
                if !max_time.is_finite() || max_time < 0.0 || max_time > UNLIMITED_TIME || max_visits > UNLIMITED_VISITS { return Ok(self.fox_start_failed("Fox.invalidLimits")); }
                if self.fox.inspected_instance != self.engines.foreground() || self.fox.captured_time.is_none() || self.fox.captured_visits.is_none() { return Ok(self.fox_start_failed("Fox.limitsUnavailable")); }
                if !warning_confirmed { self.fox.outcome = Some(FoxStartOutcome::NeedsWarning); return Ok(CommandResult::Done); }
                if replace == FoxReplaceChoice::Cancel { self.fox.outcome = Some(FoxStartOutcome::Cancelled); return Ok(CommandResult::Done); }
                if self.doc.record.has_children_of_root() || !self.doc.record.node(ROOT).setup.is_empty() {
                    if replace == FoxReplaceChoice::Save { self.fox.outcome = Some(FoxStartOutcome::SaveRequested); return Ok(CommandResult::Done); }
                    if replace != FoxReplaceChoice::Replace || document != self.doc.id || revision != self.doc.revision {
                        self.fox.outcome = Some(FoxStartOutcome::NeedsReplacement { document: self.doc.id, revision: self.doc.revision }); return Ok(CommandResult::Done);
                    }
                }
                let rules = match &self.session_rules {
                    super::analysis::SessionRules::Valid { rules, .. } => Some(rules.clone()),
                    super::analysis::SessionRules::Unspecified => self.fox.captured_rules.clone(),
                    _ => None,
                };
                let Some(rules) = rules.filter(KataRules::has_required_fields) else { return Ok(self.fox_start_failed("Fox.rulesUnavailable")); };
                let listener = match Listener::bind(port) { Ok(l) => l, Err(_) => return Ok(self.fox_start_failed("Fox.portBusy")) };
                if !self.fox_engine_available() { return Ok(self.fox_start_failed("Fox.engineUnavailable")); }
                self.fox.state = GameState::default();
                self.fox.selected_time = if max_time == 0.0 { UNLIMITED_TIME } else { max_time };
                self.fox.selected_visits = if max_visits == 0 { UNLIMITED_VISITS } else { max_visits };
                self.fox.state.set_direct_max_time(self.fox.selected_time).map_err(|e| ApiError::new(ErrorKind::InvalidArgument, e))?;
                self.fox.selected_rules = Some(rules);
                self.fox.listener = Some(listener); self.fox.port = port; self.fox.owns = true;
                self.fox.adopted = None; self.fox.live = None; self.fox.applied = 0; self.fox.document = None;
                self.fox.transcript.clear(); self.fox.transcript_bytes = 0;
                self.fox.mode = Mode::Idle; self.fox.status = "Fox.listening".into();
                self.fox.next_tick = Some(self.fox_instant() + Duration::from_millis(100));
                self.fox.outcome = Some(FoxStartOutcome::Listening);
            }
            FoxCommand::Stop => { self.fox_stop(); }
            FoxCommand::ReturnLive => { if let Some(live) = self.fox.live { if self.fox.mode != Mode::Restore && self.fox.mode != Mode::Releasing { self.goto_node(live)?; } } }
            FoxCommand::SetPda { value } => return self.fox_set_pda(value),
        }
        Ok(CommandResult::Done)
    }
    fn fox_start_failed(&mut self, key: &str) -> CommandResult {
        self.fox.status = key.into(); self.fox.outcome = Some(FoxStartOutcome::Failed { message_key: key.into() }); CommandResult::Done
    }
    fn fox_begin_setup(&mut self) {
        self.fox.capture_reset(); self.fox.mode = Mode::Setup;
        self.fox.status = "Fox.listening".into(); self.fox_capture_queue();
        let selected = self.fox.selected_rules.as_ref().unwrap().clone();
        self.fox.queue.push_back(Step::Send(format!("kata-set-rules {}", selected.to_gtp_argument()), Check::Ack));
        self.fox.queue.push_back(Step::Send("kata-get-rules".into(), Check::VerifyRules(selected)));
        self.fox.queue.push_back(Step::Send(format!("kata-set-param maxVisits {}", self.fox.selected_visits), Check::Ack));
        self.fox.queue.push_back(Step::Send("kata-get-param maxVisits".into(), Check::Number(self.fox.selected_visits as f64)));
        self.fox.queue.push_back(Step::Ready); self.sync_engine(); self.fox_pump();
    }
    fn fox_engine_available(&self) -> bool {
        let Some(id) = self.engines.foreground() else { return false; };
        !self.double_engine() && self.engines.phase(id) == Some(Phase::Ready) && self.engines.is_katago(id)
            && !self.game_active() && !self.match_active() && !self.human_sl_active() && !self.quick_holds_foreground()
            && !self.flash_holds_foreground() && !self.whole_game_holds_foreground() && !self.batch_holds_foreground() && !self.auto_analysis_active()
            && self.engines.lease_availability() == crate::engine::runtime::LeaseAvailability::Available
    }
    fn fox_admit_engine(&mut self) -> bool {
        if !self.fox_engine_available() { return false; }
        let Some(id) = self.engines.foreground() else { return false; };
        let Ok(game) = self.engines.reserve_game() else { return false; };
        self.fox.reservation = Some(game); self.fox.instance = Some(id);
        self.fox.next_tick = Some(Instant::now() + Duration::from_millis(100));
        self.engines.game_invalidate_search(game);
        true
    }
    fn fox_capture_queue(&mut self) {
        for (command, check) in [
            ("stop", Check::Ack), ("list_commands", Check::Commands), ("kata-list_time_settings", Check::TimeModes),
            ("kata-get-rules", Check::Rules), ("kata-get-param maxTime", Check::Time),
            ("kata-get-param maxVisits", Check::Visits), ("kata-get-param playoutDoublingAdvantage", Check::Pda),
        ] { self.fox.queue.push_back(Step::Send(command.into(), check)); }
    }
    fn fox_invalidate(&mut self) {
        if self.fox.pda_updating {
            self.fox.pda_updating = false; self.fox.pda_status = "Fox.pdaFailed".into();
            if let Some(game) = self.fox.reservation { self.engines.game_mark_restore_failed(game); }
        }
        self.fox.search += 1; self.fox.searched = None; self.fox.search_snapshot = None;
        self.fox.candidates = None;
        self.fox.queue.clear(); self.fox.awaiting = None;
        if let Some(game) = self.fox.reservation { self.engines.game_invalidate_search(game); }
    }
    fn fox_protocol_changed(&mut self) {
        if self.fox.pda_updating {
            // PDA has its own acknowledged transaction; an opponent move invalidates search
            // identity but must not strand an already sent parameter setter without readback.
            self.fox.search += 1; self.fox.searched = None; self.fox.candidates = None;
            return;
        }
        self.fox_invalidate(); self.fox.queue.push_back(Step::Send("stop".into(), Check::Ack)); self.fox_pump();
    }
    pub(super) fn fox_stop(&mut self) {
        let now = self.fox_now(); self.fox.state.stop_automation("Fox.stopped", now);
        self.fox.state.revoke_recovery("Fox.noCheckpoint");
        if let Some(mut listener) = self.fox.listener.take() { listener.close(); }
        self.fox.token += 1;
        self.fox_release("Fox.stopped");
    }
    fn fox_fail(&mut self, key: &str) {
        let now = self.fox_now(); self.fox.state.stop_automation(key, now);
        self.fox_invalidate(); self.fox.status = key.into();
        if self.fox.live.is_some() && self.fox.reservation.is_some() {
            self.fox.mode = Mode::Observe;
            self.fox.queue.push_back(Step::Send("stop".into(), Check::Ack)); self.fox_pump();
        } else { self.fox_release(key); }
    }
    fn fox_release(&mut self, key: &str) {
        if matches!(self.fox.mode, Mode::Restore | Mode::Releasing) { return; }
        self.fox_invalidate(); self.fox.final_status = key.into(); self.fox.status = "Fox.restoring".into();
        if self.fox.reservation.is_none() {
            self.fox.mode = Mode::Idle; self.fox.status = key.into();
            self.fox.owns = self.fox.listener.is_some() && (self.fox.state.snapshot().active || self.fox.status == "Fox.unrestored");
            return;
        }
        self.fox.mode = Mode::Restore;
        self.fox.queue.push_back(Step::Send("stop".into(), Check::Ack));
        if let Some(rules) = self.fox.captured_rules.clone() {
            self.fox.queue.push_back(Step::Send(format!("kata-set-rules {}", rules.to_gtp_argument()), Check::Ack));
            self.fox.queue.push_back(Step::Send("kata-get-rules".into(), Check::VerifyRules(rules)));
        }
        if let Some(time) = self.fox.captured_time {
            self.fox.queue.push_back(Step::Send(format!("kata-set-param maxTime {time}"), Check::Ack));
            self.fox.queue.push_back(Step::Send("kata-get-param maxTime".into(), Check::Number(time)));
        }
        if let Some(visits) = self.fox.captured_visits {
            self.fox.queue.push_back(Step::Send(format!("kata-set-param maxVisits {visits}"), Check::Ack));
            self.fox.queue.push_back(Step::Send("kata-get-param maxVisits".into(), Check::Number(visits as f64)));
        }
        self.fox.queue.push_back(Step::Send("kata-time_settings none".into(), Check::Ack));
        let target = self.analysis_target();
        let game = self.fox.reservation.unwrap();
        let Some(commands) = self.engines.game_restore_position_commands(game, &target) else { self.fox_restore_or_fail(); return; };
        for command in commands { self.fox.queue.push_back(Step::Send(command, Check::Ack)); }
        self.fox.queue.push_back(Step::RestoreDone); self.fox_pump();
    }
    fn fox_replay_document(&self, node: NodeId) -> Vec<String> {
        let record = &self.doc.record; let mut commands = vec![format!("boardsize {}", record.size.width), "clear_board".into(), format!("komi {}", self.komi)];
        let root = record.node(ROOT);
        if !root.setup.add_black.is_empty() {
            commands.push(format!("set_free_handicap {}", root.setup.add_black.iter().map(|p|crate::engine::gtp::vertex(record.size,*p)).collect::<Vec<_>>().join(" ")));
        }
        for n in record.path_to(node) { match record.node(n).kind {
            crate::record::NodeKind::Move { color, point } => commands.push(format!("play {} {}", if color == Stone::Black { 'B' } else { 'W' }, crate::engine::gtp::vertex(record.size, point))),
            crate::record::NodeKind::Pass { color } => commands.push(format!("play {} pass", if color == Stone::Black { 'B' } else { 'W' })), _ => {}
        } }
        commands
    }
    fn fox_pump(&mut self) {
        if self.fox.awaiting.is_some() { return; }
        loop {
            let Some(step) = self.fox.queue.pop_front() else { return; };
            match step {
                Step::Send(command, check) => {
                    let Some(game) = self.fox.reservation else { return; };
                    self.fox_log(&format!("> {command}"));
                    let Some(id) = self.engines.game_send(game, &command) else { self.fox_restore_or_fail(); return; };
                    self.fox.awaiting = Some(Await { id, check, until: self.fox_instant() + TIMEOUT }); return;
                }
                Step::Ready => { self.fox.mode = Mode::Ready; self.fox.status = if self.fox.state.snapshot().initialized { self.fox.state.snapshot().status_key } else { "Fox.listening".into() }; self.fox_update(); }
                Step::InspectDone => {
                    self.fox.inspected_instance = self.fox.instance;
                    self.fox.mode = Mode::Releasing; self.fox.final_status = "Fox.waiting".into();
                    if let Some(game) = self.fox.reservation { self.engines.release_game(game, &[]); }
                    return;
                }
                Step::RestoreDone => {
                    self.fox.mode = Mode::Releasing;
                    if let Some(game) = self.fox.reservation { self.engines.release_game(game, &[]); }
                    return;
                }
                Step::TimeLeft(snapshot) => {
                    let Some(clock) = snapshot.clock.as_ref() else { self.fox_fail("Fox.clockRisk"); return; };
                    let seconds = if snapshot.own_turn() { snapshot.fox_deadline_nanos.saturating_sub(self.fox_now()) as f64 / 1e9 } else if clock.main_seconds > 0 { clock.main_seconds as f64 } else { clock.byo_seconds as f64 };
                    let periods = if clock.main_seconds > 0 { 0 } else { clock.periods };
                    self.fox.queue.push_front(Step::Search(snapshot.clone()));
                    self.fox.queue.push_front(Step::Send(format!("time_left {} {seconds} {periods}", snapshot.ai), Check::Ack));
                }
                Step::Search(snapshot) => {
                    let now = self.fox_now();
                    let remaining = snapshot.submit_deadline_nanos.saturating_sub(now) as f64 / 1e9;
                    let limit = if snapshot.own_turn() {
                        let budget = remaining.min(self.fox.selected_time);
                        budget - (budget * 0.5).min(0.5_f64.max(budget * 0.05))
                    } else { self.fox.selected_time.min(5.0) };
                    if limit <= 0.0 { self.fox_fail("Fox.clockRisk"); return; }
                    // Budget is calculated after replay, not when the turn was first queued.
                    self.fox.queue.push_front(Step::Send("kata-get-param maxTime".into(), Check::Number(limit)));
                    self.fox.queue.push_front(Step::Send(format!("kata-set-param maxTime {limit}"), Check::Ack));
                    self.fox.search_snapshot = Some(snapshot);
                }
            }
            if self.fox.awaiting.is_some() { return; }
        }
    }
    fn fox_restore_or_fail(&mut self) {
        let pda_failed = self.fox.pda_updating || self.fox.pda_status == "Fox.pdaFailed";
        if pda_failed { self.fox.pda_updating = false; self.fox.pda_status = "Fox.pdaFailed".into(); if let Some(game) = self.fox.reservation { self.engines.game_mark_restore_failed(game); } }
        if matches!(self.fox.mode, Mode::Restore | Mode::Releasing) {
            self.fox.queue.clear(); self.fox.awaiting = None; self.fox.state.revoke_recovery("Fox.unrestored");
            self.fox.status = "Fox.unrestored".into(); self.fox.final_status = "Fox.unrestored".into();
            self.fox.mode = Mode::Releasing;
            if let Some(game) = self.fox.reservation { self.engines.abandon_game(game); }
        } else if self.fox.mode == Mode::Observe { self.fox_release("Fox.engineError"); }
        else { self.fox_fail(if pda_failed { "Fox.pdaFailed" } else { "Fox.engineError" }); }
    }
    pub(super) fn fox_event(&mut self, event: RuntimeEvent) -> Option<RuntimeEvent> {
        match event {
            RuntimeEvent::GameReply { game, command, ok, text } if self.fox.reservation == Some(game) => {
                let Some(awaiting) = self.fox.awaiting.take_if(|a| a.id == command) else { return None; };
                self.fox_log(&format!("{} {}", if ok { "=" } else { "?" }, text));
                if !ok || !self.fox_check(awaiting.check, &text) { self.fox_restore_or_fail(); return None; }
                self.fox_pump(); self.fox_start_search(); None
            }
            RuntimeEvent::GameReleased { game, failure } if self.fox.reservation == Some(game) => {
                self.fox.reservation = None; self.fox.awaiting = None; self.fox.queue.clear(); self.fox.mode = Mode::Idle;
                self.fox.candidates = None;
                self.fox.status = if failure.is_some() { self.fox.state.revoke_recovery("Fox.unrestored"); "Fox.unrestored".into() } else { self.fox.final_status.clone() };
                self.fox.owns = self.fox.listener.is_some() && (self.fox.state.snapshot().active || self.fox.status == "Fox.unrestored");
                if self.fox.listener.is_none() { self.fox.next_tick = None; }
                self.sync_engine();
                if failure.is_none() && self.fox.listener.is_some() { self.fox_update(); }
                None
            }
            RuntimeEvent::Analysis { instance, node, stamp, to_play, komi, engine_name, line, .. } if self.fox.instance == Some(instance) && self.fox_holds_engine() => {
                if self.fox.mode == Mode::Ready && self.fox.search_snapshot.is_some() && Some(node) == self.fox.live && self.doc.record.node(node).stamp == stamp {
                    if let Some(best) = line.candidates.first() { self.fox_log(&format!("info visits {} winrate {}", best.visits, best.winrate)); }
                    self.fox.candidates = super::analysis::node_analysis(node, to_play, komi, engine_name, line).map(|x| x.0);
                }
                None
            }
            RuntimeEvent::Failed(failure) if self.fox.instance == Some(failure.instance) => {
                self.fox.state.revoke_recovery("Fox.recoveryEngineChanged"); self.fox_fail("Fox.engineError"); Some(RuntimeEvent::Failed(failure))
            }
            other => Some(other),
        }
    }
    fn fox_check(&mut self, check: Check, text: &str) -> bool {
        match check {
            Check::Ack => true,
            Check::Commands => ["kata-search_analyze_cancellable", "kata-get-rules", "kata-set-rules", "kata-get-param", "kata-set-param", "kgs-time_settings", "kata-time_settings", "kata-list_time_settings", "set_free_handicap", "time_left", "stop"].iter().all(|required| text.split_whitespace().any(|c| c == *required)),
            Check::TimeModes => text.split_whitespace().any(|s| s == "none"),
            Check::Rules => { self.fox.captured_rules = KataRules::parse(text).filter(KataRules::has_required_fields); self.fox.captured_rules.is_some() },
            Check::Time => { self.fox.captured_time = text.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0); self.fox.captured_time.is_some() },
            Check::Visits => { self.fox.captured_visits = text.trim().parse::<u64>().ok().filter(|v| *v > 0); self.fox.captured_visits.is_some() },
            Check::Pda => { let value = text.trim().parse::<f64>().ok().filter(|v| v.is_finite()); self.fox.confirmed_pda = value; self.fox.desired_pda = value; value.is_some() },
            Check::VerifyRules(expected) => KataRules::parse(text).as_ref() == Some(&expected),
            Check::Number(expected) => text.trim().parse::<f64>().ok() == Some(expected),
            Check::PdaValue(expected) => {
                if text.trim().parse::<f64>().ok() != Some(expected) { self.fox.pda_status = "Fox.pdaFailed".into(); return false; }
                self.fox.confirmed_pda = Some(expected); self.fox.pda_updating = false;
                match self.commit_config(|c| { c.engine.kata_pda = Some(expected); c.engine.kata_pda_enabled = expected != 0.0; }) {
                    Ok(()) => self.fox.pda_status.clear(),
                    Err(error) => {
                        // Readback is still authoritative; a storage failure is not an engine failure.
                        self.fox.pda_status = "Fox.pdaFailed".into();
                        self.fox_log(&error.message);
                    }
                }
                true
            }
            Check::Selection(snapshot, search) => {
                self.fox.search_snapshot = None;
                let current = self.fox.state.snapshot();
                if search != self.fox.search || snapshot.connection != current.connection || snapshot.game != current.game || snapshot.revision != current.revision { return true; }
                if self.fox.instance != self.engines.foreground() || self.fox.reservation.and_then(|g|self.engines.game_instance(g)) != self.fox.instance { return false; }
                let vertex = text.trim().strip_prefix("play ").unwrap_or(text.trim());
                if vertex == "cancelled" { if snapshot.own_turn() { self.fox_fail("Fox.clockRisk"); } return true; }
                if vertex.eq_ignore_ascii_case("resign") { self.fox_fail("Fox.officialAction"); return true; }
                if !snapshot.own_turn() { return true; }
                let now = self.fox_now();
                let pending = match self.fox.state.prepare_submission(&snapshot, search, vertex, now) { Ok(Some(p)) => p, _ => { self.fox_fail("Fox.clockRisk"); return true; } };
                let token = self.fox.token;
                let epoch = self.fox.epoch; let clock = self.fox.clock;
                let sent = if let Some(listener) = &mut self.fox.listener { listener.send(token, &pending.body, || {
                    let claimed = clock.max(Instant::now()).saturating_duration_since(epoch).as_nanos().min(u64::MAX as u128) as u64;
                    self.fox.state.claim_submission(&pending, claimed)
                }) } else { return false; };
                match sent { Ok(true) => self.fox.status = "Fox.pending".into(), _ => self.fox_fail("Fox.confirmationFailed") }
                true
            }
        }
    }
    fn fox_start_search(&mut self) {
        if self.fox.awaiting.is_some() || !self.fox.queue.is_empty() { return; }
        let Some(snapshot) = self.fox.search_snapshot.take() else { return; };
        if !snapshot.searchable() || snapshot.own_turn() && self.fox_now() >= snapshot.submit_deadline_nanos { self.fox_fail("Fox.clockRisk"); return; }
        let Some(game) = self.fox.reservation else { return; };
        let Some(node) = self.fox.live else { return; };
        let mut target = self.analysis_target(); target.node = node;
        let Some(id) = self.engines.game_search(game, target, color(snapshot.to_play.unwrap_or('B'))) else { self.fox_fail("Fox.engineError"); return; };
        let until = if snapshot.own_turn() { self.fox.epoch + Duration::from_nanos(snapshot.submit_deadline_nanos) } else { self.fox_instant() + Duration::from_secs(7) };
        self.fox_log(&format!("> kata-search_analyze_cancellable {} 10", snapshot.to_play.unwrap_or('B')));
        self.fox.search_snapshot = Some(snapshot.clone());
        self.fox.awaiting = Some(Await { id, check: Check::Selection(snapshot, self.fox.search), until });
    }
    fn fox_update(&mut self) {
        let mut snapshot = self.fox.state.snapshot();
        if !snapshot.active { return; }
        if !snapshot.complete && self.fox.state.has_checkpoint() {
            if self.fox.instance != self.engines.foreground() { self.fox.state.revoke_recovery("Fox.recoveryEngineChanged"); }
            if self.fox.state.recover_history(self.fox_now()) {
                snapshot = self.fox.state.snapshot();
                self.fox.adopted = Some(snapshot.game);
                self.fox.status = snapshot.status_key.clone();
            }
            else { self.fox.status = self.fox.state.recovery_status(); return; }
        }
        if snapshot.searchable() && self.fox.mode == Mode::Idle && self.fox.listener.is_some() && self.fox.status != "Fox.unrestored" {
            if self.fox_admit_engine() { self.fox_begin_setup(); } else { self.fox.status = "Fox.engineUnavailable".into(); }
            return;
        }
        if !snapshot.searchable() || !matches!(self.fox.mode, Mode::Ready | Mode::Observe) { return; }
        if self.fox_apply(&snapshot).is_err() { self.fox_fail("Fox.incomplete"); return; }
        if self.fox.mode == Mode::Observe { self.fox.mode = Mode::Ready; }
        if snapshot.pending.is_some() { return; }
        if self.fox.desired_pda != self.fox.confirmed_pda { if !self.fox.pda_updating { self.fox_queue_pda(); } return; }
        if snapshot.own_turn() && !snapshot.clock_refreshed {
            let now = self.fox_now();
            if self.fox.state.begin_clock_refresh(now) {
                self.fox.refresh_deadline = Some(snapshot.submit_deadline_nanos.min(now.saturating_add(5_000_000_000)));
                self.fox.status = "Fox.refreshingClock".into();
                let sent = self.fox.listener.as_mut().map(|l| l.send(self.fox.token, &format!("AFSTATUS,{}", snapshot.ai), || true));
                if !matches!(sent, Some(Ok(true))) { self.fox_fail("Fox.clockRisk"); }
            }
            return;
        }
        let identity = (snapshot.connection, snapshot.game, snapshot.revision);
        if self.fox.searched == Some(identity) || self.fox.awaiting.is_some() || !self.fox.queue.is_empty() { return; }
        self.fox.searched = Some(identity); self.fox.search += 1;
        self.fox.queue.push_back(Step::Send("stop".into(), Check::Ack));
        let rules = snapshot.rules.as_ref().unwrap();
        for command in self.fox_replay_document(self.fox.live.unwrap()) { self.fox.queue.push_back(Step::Send(command, Check::Ack)); }
        self.fox.queue.push_back(Step::Send(format!("kgs-time_settings byoyomi {} {} {}", rules.main_seconds, rules.byo_seconds, rules.periods), Check::Ack));
        self.fox.queue.push_back(Step::TimeLeft(snapshot)); self.fox_pump();
    }
    fn fox_apply(&mut self, snapshot: &Snapshot) -> Result<(), ()> {
        if self.fox.adopted != Some(snapshot.game) || self.fox.live.is_none() {
            let rules = snapshot.rules.as_ref().ok_or(())?;
            let selected = self.fox.selected_rules.as_ref().ok_or(())?.to_gtp_argument();
            let escaped = selected.replace('\\', "\\\\").replace(']', "\\]");
            let mut sgf = format!("(;GM[1]FF[4]SZ[{}]KM[{}]RU[{}]PL[{}]", rules.size, rules.komi, escaped, if rules.setup.is_empty() { 'B' } else { 'W' });
            if !rules.setup.is_empty() { sgf.push_str(&format!("HA[{}]AB", rules.setup.len())); for p in &rules.setup { sgf.push_str(&format!("[{}{}]", (b'a'+p.x) as char, (b'a'+p.y) as char)); } }
            sgf.push(')');
            let record = crate::formats::parse(crate::record::RecordFormat::Sgf, &sgf, false).map_err(|_| ())?;
            self.doc = Document { id: self.next_doc, record: Arc::new(record), format: None, path: None, revision: 0, saved_revision: None, origin_name: None };
            self.next_doc += 1; self.cursor = ROOT; self.komi = rules.komi;
            self.sounds.clear(); self.reset_editing(); self.limit_stop = None; self.komi_changed = false;
            self.fox.adopted = Some(snapshot.game); self.fox.applied = 0; self.fox.live = Some(ROOT); self.fox.document = Some(self.doc.id);
            self.publish_record_rules(Some(&selected)); self.reset_analysis();
            // `Board.setHistory` of the live game tells 棋盘同步工具 its history was replaced.
            self.readboard_history_overwritten();
        }
        if self.fox.document != Some(self.doc.id) || self.fox.applied > snapshot.actions.len() { return Err(()); }
        let following = Some(self.cursor) == self.fox.live;
        let allow_suicide = self.fox.selected_rules.as_ref().is_some_and(|r|r.field("suicide").and_then(serde_json::Value::as_bool) == Some(true));
        while self.fox.applied < snapshot.actions.len() {
            let action = &snapshot.actions[self.fox.applied]; let at = self.fox.live.ok_or(())?;
            let op = if let Some(p) = &action.placement { DocumentEdit::Play { at, color: color(action.color), point: Point { x:p.x.into(), y:p.y.into() }, alternate: false, rules: PlayRules { allow_suicide, no_capture:false } } } else { DocumentEdit::Pass { at, color:color(action.color) } };
            let applied = self.apply_edit(op).map_err(|_| ())?;
            let node = applied.node.filter(|n| *n != at).ok_or(())?;
            self.fox.live = Some(node); self.fox.applied += 1;
            if following && self.config.ui.play_sound { self.sounds.push(super::step_sound(&self.doc.record, node)); }
        }
        if following { self.cursor = self.fox.live.unwrap(); }
        Ok(())
    }
    pub(super) fn fox_tick(&mut self, now: Instant) -> bool {
        if self.fox.next_tick.is_none() { return false; }
        self.fox.clock = self.fox.clock.max(now);
        self.fox.next_tick = Some(now + Duration::from_millis(100));
        let nanos = self.fox_now();
        let events = self.fox.listener.as_mut().map(Listener::poll).unwrap_or_default();
        let changed = !events.is_empty();
        for event in events {
            match event {
                TransportEvent::Connected { token } => {
                    self.fox.token = token; self.fox.state.connected(nanos); self.fox.status = "Fox.syncing".into();
                    if matches!(self.fox.mode, Mode::Ready | Mode::Observe) { self.fox.mode = Mode::Ready; self.fox_protocol_changed(); }
                }
                TransportEvent::Received { token, body } if token == self.fox.token => {
                    let before = self.fox.state.snapshot();
                    if self.fox.state.receive(&body, nanos).is_err() { self.fox.state.reject(nanos); }
                    let snapshot = self.fox.state.snapshot();
                    if matches!(self.fox.mode, Mode::Restore | Mode::Releasing) { continue; }
                    if snapshot.active { self.fox.owns = true; }
                    self.fox.status = snapshot.status_key.clone();
                    if before.revision != snapshot.revision && matches!(self.fox.mode, Mode::Ready | Mode::Observe) {
                        self.fox_protocol_changed();
                    }
                    if !snapshot.active && snapshot.status_key == "Fox.terminal" { self.fox_release(&snapshot.status_key); }
                    else if snapshot.automation_blocked { self.fox_fail(&snapshot.status_key); }
                    else { self.fox_update(); }
                }
                TransportEvent::Disconnected { token } if token == self.fox.token => {
                    self.fox.state.disconnected(nanos); self.fox.status = "Fox.disconnected".into();
                    if matches!(self.fox.mode, Mode::Ready | Mode::Observe) { self.fox.mode = Mode::Observe; self.fox_protocol_changed(); }
                }
                TransportEvent::Problem { token: None, key } if key == "Fox.duplicateClient" => {
                    if !matches!(self.fox.mode, Mode::Restore | Mode::Releasing) { self.fox.status = key; }
                }
                TransportEvent::Problem { token, key } if token.is_none_or(|token| token == self.fox.token) => {
                    if !matches!(self.fox.mode, Mode::Restore | Mode::Releasing) { self.fox_fail(&key); }
                }
                _ => {}
            }
        }
        if self.fox.state.has_checkpoint() && self.fox.instance != self.engines.foreground() { self.fox.state.revoke_recovery("Fox.recoveryEngineChanged"); self.fox_fail("Fox.recoveryEngineChanged"); }
        if self.fox.awaiting.as_ref().is_some_and(|a| now >= a.until) {
            if self.fox.awaiting.as_ref().is_some_and(|a|matches!(&a.check, Check::Selection(s, _) if s.own_turn())) { self.fox_fail("Fox.clockRisk"); }
            else { self.fox_restore_or_fail(); }
        }
        let snapshot = self.fox.state.snapshot();
        if snapshot.searchable() && self.fox.mode == Mode::Ready {
            if let Some(pending) = &snapshot.pending {
                if nanos.saturating_sub(pending.prepared_nanos) >= 5_000_000_000 { self.fox_fail("Fox.confirmationFailed"); }
            } else if snapshot.own_turn() && (nanos >= snapshot.submit_deadline_nanos || !snapshot.clock_refreshed && self.fox.refresh_deadline.is_some_and(|d| nanos >= d)) { self.fox_fail("Fox.clockRisk"); }
            else { self.fox_update(); }
        }
        self.fox_pump(); self.fox_start_search();
        changed || self.fox.owns || self.fox_holds_engine()
    }
    pub(super) fn fox_display_analysis(&self) -> Option<NodeAnalysisView> {
        if self.fox.mode != Mode::Ready || Some(self.cursor) != self.fox.live { return None; }
        self.fox.candidates.as_ref().map(|s| self.shown_analysis_value(s))
    }
    pub(super) fn fox_set_pda(&mut self, value: f64) -> Result<CommandResult, ApiError> {
        if !value.is_finite() || !(-3.0..=3.0).contains(&value) { self.fox.pda_status = "Fox.pdaInvalid".into(); return Ok(CommandResult::Done); }
        if self.fox.mode != Mode::Ready || self.fox.reservation.is_none() { self.fox.pda_status = "Fox.pdaNotReady".into(); return Ok(CommandResult::Done); }
        let reason = self.fox.instance.map(|i| self.engines.fox_pda_unavailable(i)).unwrap_or("Fox.pdaUnknown");
        if !reason.is_empty() { self.fox.pda_status = reason.into(); return Ok(CommandResult::Done); }
        self.fox.desired_pda = Some(value);
        if self.fox.pda_updating { return Ok(CommandResult::Done); }
        let now = self.fox_now(); self.fox.state.cancel_unwritten_for_pda(now);
        if self.fox.state.snapshot().pending.is_none() { self.fox_queue_pda(); }
        Ok(CommandResult::Done)
    }
    fn fox_queue_pda(&mut self) {
        if self.fox.state.snapshot().pending.is_some() { return; }
        let Some(value) = self.fox.desired_pda else { return; };
        self.fox_invalidate(); self.fox.pda_updating = true; self.fox.pda_status = "Fox.pdaUpdating".into();
        for (command, check) in [("stop".into(), Check::Ack), (format!("kata-set-param playoutDoublingAdvantage {value}"), Check::Ack), ("kata-get-param playoutDoublingAdvantage".into(), Check::PdaValue(value))] { self.fox.queue.push_back(Step::Send(command, check)); }
        self.fox_pump();
    }
    pub(super) fn fox_view(&self) -> FoxView {
        let snapshot = self.fox.state.snapshot(); let now = self.fox_now();
        let map = |f: &SubmissionFact| FoxSubmissionView { connection:f.submission.connection, game:f.submission.game, revision:f.submission.revision, search:f.submission.search, vertex:f.submission.action.vertex(snapshot.rules.as_ref().map_or(19, |r|r.size)), outcome:format!("{:?}", f.outcome), written:f.written_nanos != 0, reason:f.reason.clone() };
        let submissions: Vec<_> = self.fox.state.submissions().iter().rev().take(64).map(map).collect();
        let pending = self.fox.state.latest_submission().filter(|f| snapshot.pending.as_ref().is_some_and(|p|p.search == f.submission.search)).map(map);
        FoxView {
            status_key:self.fox.status.clone(), listening:self.fox.listener.is_some(), port:self.fox.port,
            owns_board:self.fox.owns, preparing:matches!(self.fox.mode,Mode::Inspect|Mode::Setup), restoring:matches!(self.fox.mode,Mode::Restore|Mode::Releasing), ready:self.fox.mode == Mode::Ready,
            can_start:self.fox.listener.is_none() && self.fox.reservation.is_none() && self.fox.inspected_instance.is_some_and(|i|Some(i) == self.engines.foreground() && self.engines.phase(i) == Some(Phase::Ready) && self.engines.unrestored(i).is_none()), can_stop:self.fox.listener.is_some(),
            rules: self.fox.selected_rules.as_ref().or_else(|| match &self.session_rules { super::analysis::SessionRules::Valid { rules,.. } => Some(rules), _ => self.fox.captured_rules.as_ref() }).map(KataRules::to_gtp_argument),
            max_time:self.fox.captured_time.map(|t|if t >= UNLIMITED_TIME {0.0}else{t}), max_visits:self.fox.captured_visits.map(|v|if v>=UNLIMITED_VISITS{0}else{v}),
            connection:snapshot.connection,game:snapshot.game,revision:snapshot.revision,live_node:self.fox.live,
            browsing:self.fox.owns && self.fox.live.is_some() && self.fox.live != Some(self.cursor), browsing_status_key:if snapshot.searchable(){"Fox.browsing"}else{"Fox.retainedObservation"}.into(),
            clock:snapshot.clock.as_ref().map(|c|FoxClockView{main_seconds:c.main_seconds as f64,byo_seconds:c.byo_seconds as f64,periods:c.periods,remaining_seconds:if snapshot.own_turn() && snapshot.searchable(){snapshot.submit_deadline_nanos.saturating_sub(now) as f64/1e9}else{0.0}}),
            pending,submissions,desired_pda:self.fox.desired_pda,confirmed_pda:self.fox.confirmed_pda,pda_status_key:self.fox.pda_status.clone(),transcript:self.fox.transcript.iter().cloned().collect(),outcome:self.fox.outcome.clone(),
        }
    }
}
impl FoxFlow {
    fn capture_reset(&mut self) {
        self.captured_rules=None; self.captured_time=None; self.captured_visits=None;
        self.desired_pda=None; self.confirmed_pda=None; self.pda_status.clear(); self.pda_updating=false;
        self.queue.clear(); self.awaiting=None;
    }
}
