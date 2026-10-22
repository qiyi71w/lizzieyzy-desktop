//! 整盘精析: a main-trunk plan, 32-visit overview followed by the requested deep search.
//! Java WholeGameAnalysis{Plan,Session,ResultView}; the engine and callbacks belong to this
//! session, not the browsing cursor. Pausing returns the engine and retains the plan/results.
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::analysis::{fill_pv_captures, node_analysis};
use super::quick::{Query, RulesSignature};
use super::Workbench;
use crate::api::{ApiError, WholeGameAnalysisView, WholeGameState as State};
use crate::engine::gtp::InfoLine;
use crate::engine::kata_analysis::{self, QueryOptions, Response};
use crate::engine::runtime::{InstanceId, LeaseAvailability, RuntimeEvent};
use crate::record::{GameRecord, NodeId, ROOT};
use crate::settings::{PvVisitsMode, WholeGameSettings};

#[derive(Default)]
pub(super) struct WholeGameFlow {
    next: u64,
    session: Option<Session>,
    /// A lease must finish returning before the dialog can resume or start another run: the
    /// lease and the foreground engine it borrowed.
    releasing: Option<(u64, Option<InstanceId>)>,
}
struct Session {
    id: u64,
    doc: u64,
    record: Arc<GameRecord>,
    nodes: Vec<NodeId>,
    path: Vec<NodeId>,
    komi: f64,
    rules: RulesSignature,
    rules_rev: u64,
    state: State,
    deep: bool,
    visits: u32,
    ownership: bool,
    borrowed: bool,
    holds: bool,
    engine: Option<Engine>,
    attempts: u8,
    stage_started: Instant,
    stage_total: u32,
    answered: u32,
    guard_at: Instant,
    baseline_completed: u32,
    deep_completed: u32,
    detail: String,
    save_failed: bool,
    /// Every search is answered and the borrowed engine is being given back: the run reports
    /// completion only once the handback is confirmed.
    completing: bool,
}
enum Engine {
    Process { instance: InstanceId, queries: HashMap<String, Query> },
    Lease { lease: u64, instance: Option<InstanceId>, queue: VecDeque<NodeId>, ready: bool },
}
impl State {
    fn active(self) -> bool {
        matches!(self, Self::Preparing | Self::Baseline | Self::Deep | Self::Pausing | Self::Paused)
    }
}
impl Workbench {
    /// Java isWholeGameAnalysisStartingOrRunning: preparing, baseline/deep, pausing AND paused.
    pub(super) fn whole_game_active(&self) -> bool {
        self.whole_game.session.as_ref().is_some_and(|s| s.state.active())
    }
    /// A handback holds only the engine it returns; a successor foreground analyses at once.
    pub(super) fn whole_game_holds_foreground(&self) -> bool {
        self.whole_game.releasing.is_some_and(|(_, i)| i == self.engines.foreground())
            || self.whole_game.session.as_ref().is_some_and(|s| {
                s.state.active() && (s.holds || (s.borrowed && s.state != State::Paused))
            })
    }
    pub(super) fn whole_game_releasing(&self) -> bool { self.whole_game.releasing.is_some() }
    pub(super) fn whole_game_document_closing(&mut self) {
        if self.whole_game_active() { self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.gameChanged"); }
    }
    /// The foreground engine was switched, restarted or closed: a run borrowing it stops, keeps
    /// its answers and gives the engine back; a dedicated process continues without holding it.
    pub(super) fn whole_game_engine_changed(&mut self) {
        let Some(s) = self.whole_game.session.as_mut() else { return };
        if matches!(s.engine, Some(Engine::Lease { .. })) {
            self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.request");
        } else {
            s.holds = false;
        }
    }
    /// Space resumes the temporarily idled dedicated foreground first (Java
    /// shouldPauseFromAnalysisControl tests actual pondering, not the session's existence).
    pub(super) fn whole_game_toggle_pause(&mut self) -> bool {
        if self.paused { return false; }
        if let Some(s) = self.whole_game.session.as_mut().filter(|s| s.state.active() && s.holds) {
            s.holds = false;
            return true;
        }
        false
    }
    /// `WholeGameAnalysisPlan.create`: the root position, then every real action of the main
    /// trunk (moves and non-dummy passes).
    fn whole_game_nodes(&self) -> Vec<NodeId> {
        let rec = &self.doc.record;
        rec.path_to(rec.line_end(ROOT)).into_iter().filter(|&n| n == ROOT || rec.node(n).kind.is_action()).collect()
    }
    fn whole_game_matches(&self, s: &Session) -> bool {
        if s.doc != self.doc.id || s.record.size != self.doc.record.size || s.komi != self.komi
            || s.record.info.komi != self.doc.record.info.komi || s.record.info.rules != self.doc.record.info.rules
            || s.rules.0 != self.config.analysis_engine.use_current_rules
            || s.rules.1 != self.config.analysis_engine.specific_rules
            || s.rules.2 != self.config.engine.auto_load_kata_rules
            || s.rules.3 != self.config.engine.kata_rules || s.rules_rev != self.rules_revision {
            return false;
        }
        let rec = &self.doc.record;
        let mut next = Some(ROOT);
        for &n in &s.path {
            if next != Some(n) || !rec.contains(n) { return false; }
            let (a,b) = (rec.node(n), s.record.node(n));
            if a.stamp != b.stamp || a.kind != b.kind || a.to_play != b.to_play { return false; }
            next = a.children.first().copied();
        }
        next.is_none()
    }
    fn whole_game_has(&self, node: NodeId, visits: u32, ownership: bool) -> bool {
        self.store.get(&self.doc.record, node).is_some_and(|a| a.visits >= u64::from(visits) && !a.candidates.is_empty())
            && (!ownership || self.store.ownership(&self.doc.record, node).is_some())
    }
    fn whole_game_refresh(&self, s: &mut Session) {
        if s.doc != self.doc.id { return; }
        s.baseline_completed = s.nodes.iter().filter(|&&n| self.whole_game_has(n, 32, false)).count() as u32;
        s.deep_completed = s.nodes.iter().filter(|&&n| self.whole_game_has(n, s.visits, s.ownership)).count() as u32;
    }
    pub fn estimate_whole_game_analysis(&self, visits: u32) -> Option<u64> {
        if !(500..=1_000_000).contains(&visits) { return None; }
        let profile = self.engines.foreground().and_then(|id| self.engines.profile_of(id))
            .or_else(|| self.config.engine.default_index().and_then(|i| self.config.engine.engines.get(i)))?;
        let policy = profile.thread_policy.as_ref()?;
        if policy.threads == 0 { return None; }
        let speed = policy.visits_per_second;
        if !speed.is_finite() || speed <= 0.0 { return None; }
        let ownership = self.estimate.state.shown && !self.config.analysis_engine.reuse_current_engine;
        let required: u64 = self.whole_game_nodes().into_iter().map(|n| {
            u64::from(!self.whole_game_has(n, 32, false)) * 32
                + u64::from(!self.whole_game_has(n, visits, ownership)) * u64::from(visits)
        }).sum();
        Some(((required as f64 * 1000.0 / speed).round() as u64).max(1))
    }
    pub fn start_whole_game_analysis(&mut self, visits: u32) -> Result<Result<bool, String>, ApiError> {
        if self.whole_game_active() || self.whole_game.releasing.is_some() { return Ok(Ok(false)); }
        if !(500..=1_000_000).contains(&visits) { return Ok(Err("WholeGameAnalysis.visits.invalid".into())); }
        let path = self.doc.record.path_to(self.doc.record.line_end(ROOT));
        // `countMainlineMoves(start) == 0`: nothing but the root to analyse.
        let nodes = self.whole_game_nodes();
        if nodes.len() < 2 { return Ok(Err("WholeGameAnalysis.noGame".into())); }
        // isWholeGameAnalysisConflict: automatic analysis (isAutoAna) is an exclusive task.
        if self.auto_analysis_active() { return Ok(Err("WholeGameAnalysis.conflict".into())); }
        if self.flash_active() || self.flash_releasing() { return Ok(Err("WholeGameAnalysis.conflict.analysis".into())); }
        let borrow = self.config.analysis_engine.reuse_current_engine;
        // Silent automatic analysis relinquishes its process/lease without resuming in between.
        self.quick_cancel();
        self.discard_analysis_process();
        if borrow && !self.quick_releasing() {
            let availability = self.engines.lease_availability();
            if availability != LeaseAvailability::Available { return Ok(Err(reuse_key(availability))); }
        }
        let save_failed = self.commit_config(|c| c.whole_game = WholeGameSettings { deep_visits: visits }).is_err();
        self.whole_game.next += 1;
        let now = Instant::now();
        let mut s = Session {
            id: self.whole_game.next, doc: self.doc.id, record: self.doc.record.clone(), nodes, path,
            komi: self.komi, rules: self.quick_rules_signature(), rules_rev: self.rules_revision,
            state: State::Preparing, deep: false, visits,
            ownership: self.estimate.state.shown && !borrow, borrowed: borrow,
            holds: !borrow && !self.paused, engine: None, attempts: 0,
            stage_started: now, stage_total: 0, answered: 0, guard_at: now + Duration::from_millis(500),
            baseline_completed: 0, deep_completed: 0, detail: "WholeGameAnalysis.preparing".into(), save_failed, completing: false,
        };
        self.whole_game_refresh(&mut s);
        self.whole_game.session = Some(s);
        self.whole_game_prepare();
        self.sync_engine();
        Ok(Ok(true))
    }
    fn whole_game_prepare(&mut self) {
        if self.quick_releasing() || self.whole_game.releasing.is_some() { return; }
        let Some(mut s) = self.whole_game.session.take() else { return };
        if s.state != State::Preparing { self.whole_game.session = Some(s); return; }
        if !self.whole_game_matches(&s) {
            self.whole_game.session = Some(s);
            return self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.gameChanged");
        }
        if s.engine.is_some() { self.whole_game.session = Some(s); return; }
        let engine = if s.borrowed {
            match self.engines.begin_lease() {
                Ok(lease) => Some(Engine::Lease { lease, instance: self.engines.foreground(), queue: VecDeque::new(), ready: false }),
                Err(_) => None,
            }
        } else {
            let base = self.analysis_command_base(false);
            let threads = crate::engine::analysis_command::recommended_threads(std::thread::available_parallelism().map_or(1, |n| n.get() as u32));
            let command = crate::engine::analysis_command::whole_game(&base, self.config.engine.analysis.limit_branch_length, threads);
            // Java requires a unique accepted saved-engine scene for this exact configured command.
            let mut matches = self.config.engine.engines.iter().filter(|p| p.thread_policy.as_ref()
                .and_then(|policy| policy.measured_scenes.as_ref())
                .and_then(|scenes| scenes.get("whole-game"))
                .is_some_and(|scene| scene["command"].as_str() == Some(base.as_str())));
            let first = matches.next();
            let saved = if matches.next().is_none() { first } else { None };
            self.engines.start_whole_game_worker(&command, &base, saved).ok()
                .map(|instance| Engine::Process { instance, queries: HashMap::new() })
        };
        s.engine = engine;
        let failed = s.engine.is_none();
        let pending = matches!(&s.engine, Some(Engine::Process { instance, .. }) if self.engines.worker_launch_pending(*instance));
        self.whole_game.session = Some(s);
        if failed { self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.engine"); }
        else if !pending { self.whole_game_dispatch(); }
        self.drain_engine_events();
    }
    fn whole_game_dispatch(&mut self) {
        let Some(mut s) = self.whole_game.session.take() else { return };
        loop {
            if !self.whole_game_matches(&s) {
                self.whole_game.session = Some(s);
                return self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.gameChanged");
            }
            self.whole_game_refresh(&mut s);
            let visits = if s.deep { s.visits } else { 32 };
            let ownership = s.deep && s.ownership;
            let missing: Vec<_> = s.nodes.iter().copied().filter(|&n| !self.whole_game_has(n, visits, ownership)).collect();
            if missing.is_empty() {
                if !s.deep { s.deep = true; s.attempts = 0; continue; }
                self.whole_game.session = Some(s);
                return self.whole_game_finish(State::Complete, "WholeGameAnalysis.complete");
            }
            if s.attempts >= 3 {
                self.whole_game.session = Some(s);
                return self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.request");
            }
            s.attempts += 1;
            s.state = if s.deep { State::Deep } else { State::Baseline };
            s.detail = if s.deep { "WholeGameAnalysis.deep" } else { "WholeGameAnalysis.baseline" }.into();
            s.stage_started = Instant::now(); s.stage_total = missing.len() as u32; s.answered = 0;
            let mut sent = true;
            match s.engine.as_mut().unwrap() {
                Engine::Process { instance, queries } => {
                    queries.clear();
                    let options = QueryOptions { max_visits: visits, komi: s.komi, rules: self.quick_rules(),
                        pv_visits: self.config.display.pv_visits != PvVisitsMode::Off, ownership,
                        moves_ownership: ownership && self.config.display.use_moves_ownership };
                    for n in missing {
                        let id = format!("whole{}-{}-{}-{n}", s.id, s.deep as u8, s.attempts);
                        sent &= self.engines.worker_send(*instance, &kata_analysis::query(&id, &s.record, n, &options));
                        let node = s.record.node(n);
                        queries.insert(id, Query { node: n, stamp: node.stamp, komi: s.komi, rules_rev: s.rules_rev, to_play: node.to_play });
                    }
                }
                Engine::Lease { queue, .. } => *queue = missing.into(),
            }
            self.whole_game.session = Some(s);
            if !sent { return self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.request"); }
            self.whole_game_lease_next();
            return;
        }
    }
    fn whole_game_lease_next(&mut self) {
        if self.whole_game.session.as_ref().is_some_and(|s| !self.whole_game_matches(s)) {
            return self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.gameChanged");
        }
        loop {
            let Some(s) = self.whole_game.session.as_mut() else { return };
            let Some(Engine::Lease { lease, queue, ready: true, .. }) = s.engine.as_mut() else { return };
            let (lease, node, visits) = (*lease, queue.pop_front(), if s.deep { s.visits } else { 32 });
            let Some(node) = node else { return self.whole_game_dispatch(); };
            let mut target = self.analysis_target(); target.node = node;
            let rules = self.quick_lease_rules();
            if self.engines.lease_search(lease, &target, u64::from(visits), rules.as_deref()) { return; }
            // Refused nodes remain missing and are retried by the next of the three stage passes.
        }
    }
    /// Always preserves deeper primary analysis, even if 精析设置's 总是覆盖 is enabled.
    fn whole_game_store(&mut self, q: &Query, line: InfoLine, borrowed: bool) {
        let Some(s) = self.whole_game.session.as_ref() else { return };
        if !self.whole_game_matches(s) || self.doc.id != self.store.doc() || !self.doc.record.contains(q.node)
            || self.doc.record.node(q.node).stamp != q.stamp || q.komi != self.komi || q.rules_rev != self.rules_revision { return; }
        if self.whole_game_has(q.node, if s.deep { s.visits } else { 32 }, s.deep && s.ownership) { return; }
        let name = super::exporting::java_text(self.active_language, "AnalysisEngine.wholeGameDeepAnalyze");
        let (visits, winrate, score) = (line.root_visits, line.root_winrate, line.root_score_lead);
        let Some((mut result, ownership)) = node_analysis(q.node, q.to_play, q.komi, name, line) else { return };
        if let Some(v) = visits.filter(|v| *v > 0) { result.visits = v; }
        if borrowed {
            if let Some(w) = winrate.filter(|w| w.is_finite()) { result.winrate = w; }
            if let Some(v) = score.filter(|v| v.is_finite()) { result.score_lead = Some(v); }
        }
        if let Some(old) = self.store.get(&self.doc.record, q.node).filter(|old| old.visits > result.visits) {
            if ownership.is_some() && self.store.ownership(&self.doc.record, q.node).is_none() {
                self.store.insert(q.node, q.stamp, old.clone(), ownership);
            }
            return;
        }
        let ownership = ownership.or_else(|| self.store.ownership(&self.doc.record, q.node).cloned());
        fill_pv_captures(&mut result, &self.doc.record.position(q.node), self.config.engine.analysis.limit_branch_length, !self.config.edit.no_capture);
        self.store.insert(q.node, q.stamp, result, ownership);
    }
    fn whole_game_finish(&mut self, state: State, detail: &str) {
        let Some(mut s) = self.whole_game.session.take() else { return };
        self.whole_game_refresh(&mut s);
        s.completing = false;
        if let Some(engine) = s.engine.take() {
            match engine {
                Engine::Process { instance, .. } => self.engines.terminate(instance),
                Engine::Lease { lease, instance, .. } => {
                    self.engines.end_lease(lease);
                    if self.engines.lease_alive(lease) { self.whole_game.releasing = Some((lease, instance)); }
                }
            }
        }
        if state == State::Complete && self.whole_game.releasing.is_some() {
            // Stays in the deep stage (99%, still exclusive) until LeaseEnded confirms the return.
            s.completing = true;
        } else {
            s.state = state; s.detail = detail.into();
        }
        if state == State::Pausing && self.whole_game.releasing.is_none() { s.state = State::Paused; s.detail = "WholeGameAnalysis.paused".into(); }
        self.whole_game.session = Some(s);
        self.drain_engine_events();
        self.sync_engine();
    }
    pub fn pause_whole_game_analysis(&mut self) -> bool {
        if !self.whole_game.session.as_ref().is_some_and(|s| !s.completing && matches!(s.state, State::Preparing | State::Baseline | State::Deep)) { return false; }
        self.whole_game_finish(State::Pausing, "WholeGameAnalysis.pausing"); true
    }
    pub fn resume_whole_game_analysis(&mut self) -> bool {
        let Some(s) = self.whole_game.session.as_mut() else { return false };
        if s.state != State::Paused || self.whole_game.releasing.is_some() { return false; }
        s.state = State::Preparing; s.detail = "WholeGameAnalysis.preparing".into(); s.attempts = 0;
        self.whole_game_prepare(); self.sync_engine(); true
    }
    pub fn stop_whole_game_analysis(&mut self) -> bool {
        if !self.whole_game_active() { return false; }
        self.whole_game_finish(State::Cancelled, "WholeGameAnalysis.cancelled"); true
    }
    pub(super) fn whole_game_deadline(&self) -> Option<Instant> {
        self.whole_game.session.as_ref().filter(|s| s.state.active() && !s.completing).map(|s| s.guard_at)
    }
    pub(super) fn whole_game_tick(&mut self, now: Instant) -> bool {
        let Some(s) = self.whole_game.session.as_ref().filter(|s| s.state.active() && !s.completing) else { return false };
        if !self.whole_game_matches(s) { self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.gameChanged"); return true; }
        self.whole_game.session.as_mut().unwrap().guard_at = now + Duration::from_millis(500);
        self.whole_game_prepare();
        true
    }
    pub(super) fn whole_game_event(&mut self, event: RuntimeEvent) -> Option<RuntimeEvent> {
        let (process, lease) = match self.whole_game.session.as_ref().and_then(|s| s.engine.as_ref()) {
            Some(Engine::Process { instance, .. }) => (Some(*instance), None),
            Some(Engine::Lease { lease, .. }) => (None, Some(*lease)), _ => (None, None),
        };
        match event {
            RuntimeEvent::WorkerLine { instance, line } if process == Some(instance) => {
                if !self.whole_game.session.as_ref().is_some_and(|s| self.whole_game_matches(s)) {
                    self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.gameChanged"); return None;
                }
                let (id, result) = match kata_analysis::parse(&line, self.doc.record.size) {
                    Response::Result { id, line } => (id, Some(line)),
                    Response::Error { id: Some(id), .. } => (id, None), _ => return None,
                };
                let Some(s) = self.whole_game.session.as_mut() else { return None };
                let Some(Engine::Process { queries, .. }) = s.engine.as_mut() else { return None };
                let Some(q) = queries.remove(&id) else { return None };
                let done = queries.is_empty(); s.answered += 1;
                if let Some(line) = result { self.whole_game_store(&q, line, false); }
                if done { self.whole_game_dispatch(); }
            }
            RuntimeEvent::WorkerExited { instance, .. } if process == Some(instance) => self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.request"),
            RuntimeEvent::WorkerReady { instance } if process == Some(instance) => self.whole_game_dispatch(),
            RuntimeEvent::WorkerLaunchFailed { instance, .. } if process == Some(instance) => self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.engine"),
            RuntimeEvent::LeaseReady { lease: l } if lease == Some(l) => {
                if let Some(Engine::Lease { ready, .. }) = self.whole_game.session.as_mut().and_then(|s| s.engine.as_mut()) { *ready = true; }
                self.whole_game_lease_next();
            }
            RuntimeEvent::LeaseResult { lease: l, doc, node, stamp, komi, rules_rev, to_play, line } if lease == Some(l) => {
                if self.whole_game.session.as_ref().is_some_and(|s| s.doc == doc) {
                    self.whole_game_store(&Query { node, stamp, komi, rules_rev, to_play }, line, true);
                    if let Some(s) = self.whole_game.session.as_mut() { s.answered += 1; }
                }
                self.whole_game_lease_next();
            }
            RuntimeEvent::LeaseSearchFailed { lease: l } if lease == Some(l) => self.whole_game_lease_next(),
            RuntimeEvent::LeaseEnded { lease: l, failure } if lease == Some(l) || self.whole_game.releasing.is_some_and(|r| r.0 == l) => {
                let requested = self.whole_game.releasing.take().is_some();
                if failure.is_some() || !requested { self.whole_game_finish(State::Failed, "WholeGameAnalysis.error.request"); }
                else if let Some(s) = self.whole_game.session.as_mut() {
                    if s.completing { s.completing = false; s.state = State::Complete; s.detail = "WholeGameAnalysis.complete".into(); }
                    else if s.state == State::Pausing { s.state = State::Paused; s.detail = "WholeGameAnalysis.paused".into(); }
                }
                self.borrowed_engine_returned(); self.sync_engine();
            }
            other => return Some(other),
        }
        None
    }
    pub(super) fn whole_game_results_for_current(&self) -> bool {
        self.whole_game.session.as_ref().is_some_and(|s| s.doc == self.doc.id)
            && self.store.get(&self.doc.record, self.cursor).is_some_and(|a| a.visits > 0 && !a.candidates.is_empty())
    }
    pub(super) fn whole_game_view(&self) -> WholeGameAnalysisView {
        let Some(s) = self.whole_game.session.as_ref() else {
            return WholeGameAnalysisView { run: 0, state: State::Idle, completed: 0, total: 0, baseline_completed: 0, deep_completed: 0, target_visits: 0,
                visits: self.config.whole_game.deep_visits, percent: 0, remaining_ms: None, mode_key: None, detail_key: "WholeGameAnalysis.ready".into(), show_results: false, save_failed: false };
        };
        let mut baseline = s.baseline_completed; let mut deep = s.deep_completed;
        if s.doc == self.doc.id && matches!(s.state, State::Baseline | State::Deep) {
            baseline = s.nodes.iter().filter(|&&n| self.whole_game_has(n, 32, false)).count() as u32;
            deep = s.nodes.iter().filter(|&&n| self.whole_game_has(n, s.visits, s.ownership)).count() as u32;
        }
        let total = s.nodes.len() as u32;
        let percent = match s.state { State::Complete => 100, State::Idle | State::Preparing => 0, _ => ((baseline * 20 + deep * 80) / total.max(1)).min(99) };
        let remaining_ms = (matches!(s.state, State::Baseline | State::Deep) && s.answered > 0).then(|| {
            (s.stage_started.elapsed().as_millis().max(1) * u128::from(s.stage_total.saturating_sub(s.answered)) / u128::from(s.answered)).min(u128::from(u64::MAX)) as u64
        });
        WholeGameAnalysisView { run: s.id, state: s.state, completed: if s.deep { deep } else { baseline }, total,
            baseline_completed: baseline, deep_completed: deep, target_visits: if s.deep { s.visits } else { 32 }, visits: s.visits, percent, remaining_ms,
            mode_key: Some(if s.borrowed { "WholeGameAnalysis.mode.localShared" } else { "WholeGameAnalysis.mode.local" }.into()), detail_key: s.detail.clone(),
            show_results: self.whole_game_results_for_current(), save_failed: s.save_failed }
    }
}
fn reuse_key(a: LeaseAvailability) -> String {
    format!("AnalysisSettings.reuseStatus.{}", match a {
        LeaseAvailability::Available => "available", LeaseAvailability::NoForegroundEngine => "no_foreground_engine",
        LeaseAvailability::EngineNotReady => "engine_not_ready", LeaseAvailability::NotKatago => "not_katago",
        LeaseAvailability::MissingCapability => "missing_capability", LeaseAvailability::ExistingLease => "existing_lease",
        LeaseAvailability::EngineStateUnrestored => "engine_state_unrestored",
    })
}
