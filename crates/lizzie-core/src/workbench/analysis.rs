//! Application flow for engines and analysis: which engine is in the foreground, the user's
//! analysis pause, analysis limits, the stored analysis of the open document, and the engine
//! settings. The engine runtime owns processes and protocol; this module decides what the
//! foreground engine should do and where its results belong.
//!
//! Results are stored on the node they were requested for, even when the user has moved on
//! (a valid result for another node is history, not noise); only the current node's results are
//! shown as current analysis. Results for a document that is no longer open, or for a komi that
//! no longer applies, are dropped.

use std::collections::HashMap;
use std::sync::Arc;

use super::Workbench;
use crate::api::{
    ApiError, CandidateView, ConsoleLineKind, ConsoleLineView, EngineEntryView, EngineFailureView, EngineNotice,
    EngineScope, EngineSettingsView, EngineState, EngineView, ErrorKind, ForegroundEngineView, HistoryPoint,
    NodeAnalysisView, RulesPromptView,
};
use crate::browse::Navigation;
use crate::engine::gtp::InfoLine;
use crate::engine::profile::{new_profile_id, AnalysisSettings, EngineProfile, EngineStartup};
use crate::engine::rules::KataRules;
use crate::engine::runtime::{
    Bootstrap, ConsoleKind, EngineFailure, EngineInput, InstanceId, Phase, RulesOutcome, RuntimeEvent, Target,
    UserCommandError,
};
use crate::record::{BoardSize, DocumentEdit, GameRecord, NodeId, Position, Stone, ROOT};
use crate::sanitize::sanitize;

/// The rules the open session asks for (`BoardHistoryList.SessionRulesTarget`).
#[derive(Clone, Debug, Default)]
pub(super) enum SessionRules {
    /// No declaration: the engine's own rules are used.
    #[default]
    Unspecified,
    /// A declaration KataGo understands (`raw` is the SGF text; `None` for a manual choice).
    Valid { raw: Option<String>, rules: KataRules },
    /// A declaration that is not usable KataGo rules.
    Invalid { raw: String },
}

/// Analysis stored for the open document (`BoardData` of every node).
#[derive(Default)]
pub(super) struct AnalysisStore {
    doc: u64,
    nodes: HashMap<NodeId, NodeAnalysisView>,
}

impl AnalysisStore {
    pub(super) fn reset(&mut self, doc: u64) {
        self.doc = doc;
        self.nodes.clear();
    }
}

/// A line for the application log.
pub struct LogLine {
    pub warn: bool,
    pub text: String,
}

fn node_analysis(node: NodeId, to_play: Stone, komi: f64, engine: String, line: InfoLine) -> Option<NodeAnalysisView> {
    let best = line.candidates.first()?;
    let (winrate, score_lead, score_stdev) = (best.winrate, best.score_lead, best.score_stdev);
    let visits = line.candidates.iter().map(|c| c.visits).sum();
    Some(NodeAnalysisView {
        node,
        engine,
        to_play,
        visits,
        root_visits: line.root_visits,
        winrate,
        score_lead,
        score_stdev,
        komi,
        candidates: line
            .candidates
            .into_iter()
            .map(|c| CandidateView {
                point: c.point,
                visits: c.visits,
                allocation_visits: c.allocation_visits,
                winrate: c.winrate,
                score_lead: c.score_lead,
                score_stdev: c.score_stdev,
                prior: c.prior,
                lcb: c.lcb,
                order: c.order,
                pv: c.pv,
                pv_captures: Vec::new(),
            })
            .collect(),
    })
}

/// Plays every candidate's variation out from `base` (`Branch`, at most `limit` moves; 0 = the
/// whole variation): what each move captures.
fn fill_pv_captures(result: &mut NodeAnalysisView, base: &Position, limit: u32) {
    let limit = if limit == 0 { usize::MAX } else { limit as usize };
    for c in &mut result.candidates {
        let mut pos = base.clone();
        let mut color = result.to_play;
        c.pv_captures =
            c.pv.iter()
                .take(limit)
                .map(|mv| {
                    let mut captured = Vec::new();
                    if let Some(p) = *mv {
                        pos.place(color, p, &mut captured);
                    }
                    color = color.opposite();
                    captured
                })
                .collect();
    }
}

/// `BoardData.adoptOrdinaryAnalysis` with the cache on: a result replaces the stored one only
/// with more visits (root visits when both report them).
fn more_visits(new: &NodeAnalysisView, old: &NodeAnalysisView) -> bool {
    match (new.root_visits, old.root_visits) {
        (Some(n), Some(o)) => n > o,
        _ => new.visits > old.visits,
    }
}

impl Workbench {
    // -------------------------------------------------------------------- start-up and exit

    /// Starts the engine chosen for start-up (`autoload-*`) and the preloaded engines.
    pub fn start_engines(&mut self) {
        let settings = &self.config.engine;
        let startup = settings.startup_index();
        let preload: Vec<usize> = settings
            .engines
            .iter()
            .enumerate()
            .filter(|(i, e)| e.preload && Some(*i) != startup)
            .map(|(i, _)| i)
            .collect();
        if let Some(i) = startup {
            let _ = self.switch_engine(i as u32);
        }
        for i in preload {
            let profile = self.config.engine.engines[i].clone();
            if let Err(f) = self.engines.start(&profile, self.bootstrap(&profile)) {
                self.engine_failed(*f);
            }
        }
    }

    /// Exit: every engine is ended (quit, then killed after the grace period). The platform
    /// adapter waits for the processes outside the workbench lock.
    pub fn end_engines(&mut self) {
        self.engines.terminate_all();
    }

    pub fn console_seq(&self) -> u64 {
        self.engines.console_seq()
    }

    pub fn take_logs(&mut self) -> Vec<LogLine> {
        std::mem::take(&mut self.logs)
    }

    fn log(&mut self, warn: bool, text: String) {
        self.logs.push(LogLine { warn, text });
    }

    // -------------------------------------------------------------------- the foreground wish

    fn analysis_target(&self) -> Target {
        Target {
            doc: self.doc.id,
            node: self.cursor,
            record: self.doc.record.clone(),
            komi: self.komi,
            rules: match &self.session_rules {
                SessionRules::Valid { rules, .. } if !self.rules_overridden() => Some(rules.clone()),
                _ => None,
            },
            rules_rev: self.rules_revision,
        }
    }

    /// The user chose to analyse this session with the foreground engine's current rules.
    fn rules_overridden(&self) -> bool {
        self.engines
            .foreground()
            .is_some_and(|fg| self.rules_override == Some((self.rules_revision, fg)))
    }

    /// `SessionRulesSynchronizer.permitsOrdinaryAnalysis`: no declared rules, an explicit
    /// continue, or the engine confirmed exactly the declared rules.
    fn rules_permit_analysis(&self) -> bool {
        let Some(fg) = self.engines.foreground() else {
            return true;
        };
        match &self.session_rules {
            SessionRules::Unspecified => true,
            _ if self.rules_overridden() => true,
            SessionRules::Valid { rules, .. } => self.engines.rules_outcome(fg, rules) == RulesOutcome::Confirmed,
            SessionRules::Invalid { .. } => false,
        }
    }

    /// Asks once per (rules target, engine) when the declared rules cannot be applied
    /// (`offerContinueAfterRulesFailure` / `requestLifecycleRulesOverride`).
    fn check_rules_prompt(&mut self) {
        let Some(fg) = self.engines.foreground() else { return };
        if self.engines.phase(fg) != Some(Phase::Ready) || self.rules_permit_analysis() {
            return;
        }
        let key = (self.rules_revision, fg);
        if self.rules_prompted == Some(key) {
            return;
        }
        let (target, failure) = match &self.session_rules {
            SessionRules::Invalid { raw } => (raw.clone(), "INVALID_DECLARATION"),
            SessionRules::Valid { raw, rules } => match self.engines.rules_outcome(fg, rules) {
                RulesOutcome::Failed(f) => (raw.clone().unwrap_or_else(|| rules.field_summary()), f),
                _ => return,
            },
            SessionRules::Unspecified => return,
        };
        self.rules_prompted = Some(key);
        self.next_prompt += 1;
        self.pending_prompt = Some((self.next_prompt, key));
        self.rules_prompt = Some(RulesPromptView {
            prompt: self.next_prompt,
            target,
            observed: self.engines.confirmed_rules(fg).map(KataRules::field_summary),
            failure: failure.to_string(),
        });
    }

    /// YES: analyse with the engine's current rules for this target and engine. NO: pause.
    pub fn answer_rules_prompt(&mut self, prompt: u64, accept: bool) -> bool {
        let Some((_, key)) = self.pending_prompt.take_if(|(id, _)| *id == prompt) else {
            return false;
        };
        if self.engines.foreground().map(|fg| (self.rules_revision, fg)) != Some(key) {
            return false; // another document, rules or engine since the question
        }
        if accept {
            self.rules_override = Some(key);
        } else {
            self.paused = true;
        }
        self.sync_engine();
        true
    }

    /// Publishes the session rules of a newly adopted document: an SGF's root `RU`
    /// (`publishExternalRules`); other formats declare none.
    pub(super) fn publish_record_rules(&mut self, declared: Option<&str>) {
        self.session_rules = match declared {
            None => SessionRules::Unspecified,
            Some(raw) => match KataRules::parse(raw).filter(KataRules::has_required_fields) {
                Some(rules) => SessionRules::Valid {
                    raw: Some(raw.to_string()),
                    rules,
                },
                None => SessionRules::Invalid { raw: raw.to_string() },
            },
        };
        self.rules_revision += 1;
        self.pending_prompt = None;
    }

    /// Tells the runtime what the foreground engine should do now.
    pub(super) fn sync_engine(&mut self) {
        let here = (self.doc.id, self.cursor);
        if self.limit_stop.is_some_and(|l| l != here) {
            // Java resumes after a limit stop once the position changes.
            self.limit_stop = None;
        }
        let analyse = !self.paused && self.limit_stop.is_none() && self.rules_permit_analysis();
        let target = self.analysis_target();
        self.engines.want(Some(target), analyse);
        self.check_rules_prompt();
    }

    fn bootstrap(&self, profile: &EngineProfile) -> Bootstrap {
        let s = &self.config.engine;
        let rules = if s.auto_load_kata_rules {
            KataRules::parse(&s.kata_rules)
        } else {
            None
        };
        let _ = profile;
        Bootstrap { komi: self.komi, rules }
    }

    /// Whether the open game is still a new empty game: no moves, no setup and not read from a
    /// record (`shouldApplyInitialEngineKomiToCurrentGame`, `boardEmpty`).
    fn game_is_empty(&self) -> bool {
        let rec = &self.doc.record;
        self.doc.format.is_none() && rec.node_count() == 1 && rec.node(ROOT).setup.is_empty()
    }

    /// `Board.reopenOnlyBoard`: an empty game of another size replaces the empty one.
    fn reopen_empty_board(&mut self, size: BoardSize) {
        let id = self.next_doc;
        self.next_doc += 1;
        self.doc = super::Document {
            id,
            record: Arc::new(GameRecord::empty(size, self.komi)),
            format: None,
            path: None,
            revision: 0,
            saved_revision: None,
        };
        self.cursor = ROOT;
        self.store.reset(id);
        self.publish_record_rules(None);
    }

    // -------------------------------------------------------------------- engine operations

    /// Engine menu `[n] name`: the engine becomes the foreground engine; a running instance (kept
    /// or preloaded) is reused. The previous one keeps running with fast engine change.
    pub fn switch_engine(&mut self, index: u32) -> Result<bool, ApiError> {
        let Some(profile) = self.config.engine.engines.get(index as usize).cloned() else {
            return Err(ApiError::new(ErrorKind::InvalidArgument, format!("no engine {index}")));
        };
        let current = self
            .engines
            .foreground()
            .and_then(|f| self.engines.profile_of(f))
            .map(|p| p.id.clone());
        if current.as_deref() == Some(profile.id.as_str()) {
            return Ok(false);
        }
        // An engine's board size applies to a new empty game (`reopenOnlyBoard`).
        let size = BoardSize {
            width: profile.width,
            height: profile.height,
        };
        if self.game_is_empty() && size != self.doc.record.size {
            self.reopen_empty_board(size);
        }
        // The engine's komi becomes the game komi unless the user chose it (`targetKomi`): at
        // once for a running engine, for a new one when it is ready and the game still empty.
        let target_komi = if self.komi_changed { self.komi } else { profile.komi };
        self.start_komi = None;
        let instance = match self.engines.instance_of(&profile.id) {
            Some(i) => {
                self.apply_komi(target_komi);
                i
            }
            None => match self.engines.start(&profile, self.bootstrap(&profile)) {
                Ok(i) => {
                    self.log(
                        false,
                        format!("engine start [{}] {}", profile.name, sanitize(&profile.command)),
                    );
                    self.start_komi = Some((i, target_komi));
                    i
                }
                Err(f) => {
                    self.engine_failed(*f);
                    return Ok(true);
                }
            },
        };
        let keep = self.config.engine.analysis.fast_engine_change;
        self.limit_stop = None;
        self.limit_tip_shown = false;
        self.engines.set_foreground(Some(instance), keep);
        if self.config.engine.startup == EngineStartup::Last {
            let id = profile.id.clone();
            let _ = self.commit_config(|c| c.engine.last_engine = Some(id));
        }
        self.sync_engine();
        Ok(true)
    }

    /// "重启当前引擎".
    pub fn restart_engine(&mut self) -> Result<bool, ApiError> {
        let Some(fg) = self.engines.foreground() else {
            return Ok(false);
        };
        let Some(id) = self.engines.profile_of(fg).map(|p| p.id.clone()) else {
            return Ok(false);
        };
        self.engines.terminate(fg);
        let Some(index) = self.config.engine.index_of(&id) else {
            return Ok(true);
        };
        self.switch_engine(index as u32)
    }

    /// "关闭当前引擎" / "关闭当前以外引擎" / "关闭所有引擎".
    pub fn shutdown_engines(&mut self, scope: EngineScope) -> bool {
        let fg = self.engines.foreground();
        let targets: Vec<InstanceId> = self
            .engines
            .running()
            .into_iter()
            .map(|(id, _, _)| id)
            .filter(|&id| match scope {
                EngineScope::Current => Some(id) == fg,
                EngineScope::Others => Some(id) != fg,
                EngineScope::All => true,
            })
            .collect();
        for id in &targets {
            self.engines.terminate(*id);
        }
        !targets.is_empty()
    }

    /// User pause / resume (`togglePonderMannul`). Resuming also restarts an analysis a limit
    /// stopped. Never written to disk.
    pub fn toggle_analysis(&mut self) -> Result<bool, ApiError> {
        if self.engines.foreground().is_none() {
            // Java retries the unavailable primary engine.
            if let Some(i) = self
                .config
                .engine
                .startup_index()
                .or(self.config.engine.default_index())
            {
                return self.switch_engine(i as u32);
            }
            return Ok(false);
        }
        let running = !self.paused && self.limit_stop.is_none();
        self.paused = running;
        self.limit_stop = None;
        self.sync_engine();
        Ok(true)
    }

    /// The game komi chosen by the user (toolbar field, console `komi`): recorded in the game,
    /// sent to the engine, and engine profiles no longer change it (`GameInfo.changeKomi`) — not
    /// even a starting engine's, which Java would still apply once it is ready.
    pub fn set_komi(&mut self, komi: f64) -> Result<bool, ApiError> {
        if !komi.is_finite() {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "komi"));
        }
        self.komi_changed = true;
        self.start_komi = None;
        if !self.apply_komi(komi) {
            return Ok(false);
        }
        self.sync_engine();
        Ok(true)
    }

    /// Every komi change of the game: the document records it and every stored analysis of the
    /// game is discarded (`clearBestMovesAfter(start)`).
    fn apply_komi(&mut self, komi: f64) -> bool {
        let recorded = matches!(self.edit_document(DocumentEdit::SetKomi { komi }), Ok(true));
        if !recorded && komi == self.komi {
            return false;
        }
        self.komi = komi;
        self.store.reset(self.doc.id);
        self.limit_stop = None;
        true
    }

    /// 引擎规则 (`LizzieFrame.setRules`): needs a ready KataGo; the chosen rules become the
    /// session rules (`publishManualRules`) and are remembered for auto-loading.
    pub fn set_kata_rules(&mut self, rules: &str, auto_load: bool) -> Result<Option<&'static str>, ApiError> {
        let Some(fg) = self
            .engines
            .foreground()
            .filter(|&f| self.engines.phase(f) == Some(Phase::Ready))
        else {
            return Ok(Some("LizzieFrame.setParamNoEngineHint"));
        };
        if !self.engines.is_katago(fg) {
            return Ok(Some("SetKataRules.notKataGoHint"));
        }
        let parsed = KataRules::parse(rules)
            .filter(KataRules::has_required_fields)
            .ok_or_else(|| ApiError::new(ErrorKind::InvalidArgument, format!("not KataGo rules: {rules}")))?;
        let text = rules.trim().to_string();
        self.commit_config(|c| {
            c.engine.kata_rules = text;
            c.engine.auto_load_kata_rules = auto_load;
        })?;
        // A manual rules choice discards every stored analysis (`clearBestMovesAfter(start)`).
        self.store.reset(self.doc.id);
        self.session_rules = SessionRules::Valid {
            raw: None,
            rules: parsed,
        };
        self.rules_revision += 1;
        self.pending_prompt = None;
        self.sync_engine();
        Ok(None)
    }

    /// GtpConsolePane send. Like Java, `komi <v>` sets the game komi and `undo` steps back in
    /// the record; other board-changing commands are refused.
    pub fn send_gtp(&mut self, command: &str) -> Result<Option<&'static str>, ApiError> {
        let mut words = command.split_whitespace();
        match (words.next(), words.next()) {
            (Some("komi"), Some(v)) => {
                if let Ok(k) = v.parse::<f64>() {
                    self.set_komi(k)?;
                    return Ok(None);
                }
            }
            (Some("undo"), None) => {
                self.navigate(Navigation::Back(1));
                return Ok(None);
            }
            _ => {}
        }
        match self.engines.send_user(command) {
            Ok(()) => Ok(None),
            Err(UserCommandError::Empty) => Ok(Some("")),
            Err(UserCommandError::NoEngine) => Ok(Some("LizzieFrame.setParamNoEngineHint")),
            Err(UserCommandError::ChangesBoard) => Ok(Some("product.consoleBoardCommand")),
        }
    }

    pub fn console_since(&self, seq: u64) -> Vec<ConsoleLineView> {
        self.engines
            .console_since(seq)
            .into_iter()
            .map(|l| ConsoleLineView {
                seq: l.seq,
                engine: l.engine,
                kind: match l.kind {
                    ConsoleKind::Sent => ConsoleLineKind::Sent,
                    ConsoleKind::Output => ConsoleLineKind::Output,
                    ConsoleKind::Error => ConsoleLineKind::Error,
                },
                text: l.text,
            })
            .collect()
    }

    /// 完整日志 for engine GTP traffic, written to `engine-trace.log` in the log folder.
    pub fn set_full_trace(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if enabled == self.engines.tracing() {
            return Ok(false);
        }
        if !enabled {
            self.engines.set_trace(None);
            self.log(false, "full trace stopped".into());
            return Ok(true);
        }
        let dir = self.storage.logs_dir.clone();
        let file = std::fs::create_dir_all(&dir)
            .and_then(|_| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("engine-trace.log"))
            })
            .map_err(|e| ApiError::new(ErrorKind::Io, format!("engine trace: {e}")))?;
        self.engines.set_trace(Some(Box::new(file)));
        self.log(false, "full trace started (engine GTP)".into());
        Ok(true)
    }

    pub fn set_ponder_limit_tips(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.engine.analysis.show_ponder_limited_tips == enabled {
            return Ok(false);
        }
        self.commit_config(|c| c.engine.analysis.show_ponder_limited_tips = enabled)?;
        Ok(true)
    }

    pub fn set_analysis_settings(&mut self, settings: AnalysisSettings) -> Result<bool, ApiError> {
        if self.config.engine.analysis == settings {
            return Ok(false);
        }
        let refill = settings.limit_branch_length != self.config.engine.analysis.limit_branch_length;
        self.commit_config(|c| c.engine.analysis = settings.clone())?;
        self.engines.set_interval(settings.interval_centisec);
        if refill {
            // Previews of stored analysis follow the new variation length.
            let record = self.doc.record.clone();
            for (node, result) in self.store.nodes.iter_mut() {
                fill_pv_captures(result, &record.position(*node), settings.limit_branch_length);
            }
        }
        Ok(true)
    }

    /// Engine settings saved (MoreEngines close). New entries get ids, one default at most.
    /// Engines whose launch-relevant fields changed restart; removed ones end. The foreground
    /// engine comes back as the foreground engine.
    pub fn save_engine_settings(
        &mut self,
        mut engines: Vec<EngineProfile>,
        startup: EngineStartup,
    ) -> Result<bool, ApiError> {
        for i in 0..engines.len() {
            if engines[i].id.is_empty() {
                let id = new_profile_id(&engines);
                engines[i].id = id;
            }
        }
        let mut seen_default = false;
        for e in &mut engines {
            e.is_default &= !seen_default;
            seen_default |= e.is_default;
        }
        let old = self.config.engine.engines.clone();
        if old == engines && self.config.engine.startup == startup {
            return Ok(false);
        }
        let list = engines.clone();
        self.commit_config(|c| {
            c.engine.engines = list;
            c.engine.startup = startup;
        })?;
        let fg_profile = self
            .engines
            .foreground()
            .and_then(|f| self.engines.profile_of(f))
            .map(|p| p.id.clone());
        let mut restart_fg = None;
        for (instance, id, _) in self.engines.running() {
            let launch = |p: &EngineProfile| (p.command.clone(), p.initial_command.clone(), p.width, p.height);
            let before = old.iter().find(|p| p.id == id).map(launch);
            let after = engines.iter().position(|p| p.id == id);
            let changed = after.is_none() || before != after.map(|a| launch(&engines[a]));
            if changed {
                self.engines.terminate(instance);
                if fg_profile.as_deref() == Some(id.as_str()) {
                    restart_fg = after;
                }
            }
        }
        if let Some(i) = restart_fg {
            self.switch_engine(i as u32)?;
        }
        Ok(true)
    }

    // -------------------------------------------------------------------- engine output

    /// Output of an engine process. Returns whether the view changed.
    pub fn engine_input(&mut self, instance: InstanceId, input: EngineInput) -> bool {
        let events = self.engines.input(instance, input);
        self.engine_events(events)
    }

    /// When the engine runtime has timed protocol work next (see [`Self::engine_tick`]).
    pub fn engine_timer(&self) -> Option<std::time::Instant> {
        self.engines.next_tick()
    }

    /// Advances engine protocol timers; the platform calls it at [`Self::engine_timer`].
    pub fn engine_tick(&mut self, now: std::time::Instant) {
        self.engines.tick(now);
    }

    fn engine_events(&mut self, events: Vec<RuntimeEvent>) -> bool {
        let mut changed = false;
        for ev in events {
            changed = true;
            match ev {
                RuntimeEvent::Ready { instance } => {
                    if let Some((name, version, _)) = self.engines.identity(instance) {
                        self.log(false, format!("engine ready: {name} {version}"));
                    }
                    if let Some((_, komi)) = self.start_komi.take_if(|(i, _)| *i == instance) {
                        if self.game_is_empty() {
                            self.apply_komi(komi);
                        }
                    }
                }
                RuntimeEvent::Rules { .. } => {}
                RuntimeEvent::Failed(f) => self.engine_failed(f),
                RuntimeEvent::Analysis {
                    instance,
                    doc,
                    node,
                    komi,
                    rules_rev,
                    to_play,
                    engine_name,
                    line,
                    elapsed,
                } => {
                    // Only the foreground engine's results count: a switched-away, preloaded or
                    // background engine's late lines have no effect.
                    if Some(instance) != self.engines.foreground() {
                        continue;
                    }
                    if doc != self.doc.id
                        || komi != self.komi
                        || doc != self.store.doc
                        || rules_rev != self.rules_revision
                    {
                        continue; // another document, a komi or rules that no longer apply
                    }
                    let Some(result) = node_analysis(node, to_play, komi, engine_name, line) else {
                        continue;
                    };
                    let visits = result.visits;
                    let replace = self.store.nodes.get(&node).is_none_or(|old| more_visits(&result, old));
                    if replace {
                        let mut result = result;
                        let limit = self.config.engine.analysis.limit_branch_length;
                        fill_pv_captures(&mut result, &self.doc.record.position(node), limit);
                        self.store.nodes.insert(node, result);
                    }
                    if node == self.cursor && !self.paused && self.limit_stop.is_none() {
                        self.check_limits(visits, elapsed);
                    }
                }
            }
        }
        if changed {
            // Readiness, rules answers and failures can change what the engine should do.
            self.sync_engine();
        }
        changed
    }

    /// `Leelaz` analysis limits: past the time or visit limit the analysis of this node stops
    /// (it resumes when the position changes) and the notice is shown once per engine start.
    fn check_limits(&mut self, visits: u64, elapsed: std::time::Duration) {
        let a = &self.config.engine.analysis;
        let by_visits = a.limit_playout && visits > a.limit_playouts;
        let by_time = a.limit_time && elapsed.as_secs_f64() * 1000.0 > f64::from(a.max_analyze_time_seconds) * 1000.0;
        if !(by_visits || by_time) {
            return;
        }
        self.limit_stop = Some((self.doc.id, self.cursor));
        if a.show_ponder_limited_tips && !self.limit_tip_shown {
            self.limit_tip_shown = true;
            self.engine_notice = Some(EngineNotice::StoppedByLimit);
        }
        self.sync_engine();
    }

    /// A failed engine: logged, and reported with the launch command, message and output passed
    /// through the credential sanitizer like Java's failure dialog and copy (`redactSensitiveText`).
    fn engine_failed(&mut self, f: EngineFailure) {
        let command = sanitize(&f.command);
        let message = sanitize(&f.message);
        let output: Vec<String> = f.output.iter().map(|l| sanitize(l)).collect();
        let tail = output
            .iter()
            .rev()
            .take(5)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join(" / ");
        self.log(
            true,
            format!("engine failed [{}] {command}: {message} | {tail}", f.engine_name),
        );
        self.engine_failure = Some(EngineFailureView {
            engine_name: f.engine_name,
            command,
            during_startup: f.during_startup,
            message,
            output,
        });
    }

    // -------------------------------------------------------------------- projection

    pub(super) fn engine_view(&self) -> EngineView {
        let running = self.engines.running();
        let fg = self.engines.foreground();
        let engines = self
            .config
            .engine
            .engines
            .iter()
            .map(|p| EngineEntryView {
                name: p.name.clone(),
                command: sanitize(&p.command),
                state: running
                    .iter()
                    .find(|(_, id, _)| *id == p.id)
                    .map(|(_, _, phase)| match phase {
                        Phase::Starting => EngineState::Loading,
                        Phase::Ready => EngineState::Ready,
                    }),
            })
            .collect();
        let foreground = fg.and_then(|f| {
            let profile = self.engines.profile_of(f)?;
            let index = self.config.engine.index_of(&profile.id)? as u32;
            let (gtp_name, version, _) = self.engines.identity(f)?;
            Some(ForegroundEngineView {
                index,
                name: self.engines.display_name(f)?,
                gtp_name,
                version,
                katago: self.engines.is_katago(f),
                can_analyse: self.engines.can_analyse(f),
                ready: self.engines.foreground_complete(),
                commands: self.engines.known_commands(f),
            })
        });
        let rules = fg.and_then(|f| self.engines.confirmed_rules(f));
        EngineView {
            engines,
            foreground,
            paused: self.paused,
            analysing: self.engines.analysing(self.doc.id, self.cursor),
            stopped_by_limit: self.limit_stop == Some((self.doc.id, self.cursor)),
            rules_key: rules.map(|r| r.summary_key().to_string()),
            rules: rules.map(KataRules::to_gtp_argument),
            console_seq: self.engines.console_seq(),
            full_trace: self.engines.tracing(),
        }
    }

    pub(super) fn engine_settings_view(&self) -> EngineSettingsView {
        let s = &self.config.engine;
        EngineSettingsView {
            engines: s.engines.clone(),
            startup: s.startup,
            analysis: s.analysis.clone(),
            kata_rules: s.kata_rules.clone(),
            auto_load_kata_rules: s.auto_load_kata_rules,
        }
    }

    pub(super) fn current_analysis(&self) -> Option<NodeAnalysisView> {
        self.store.nodes.get(&self.cursor).cloned()
    }

    /// Stored results on the current line (root to the line end) and the main trunk.
    pub(super) fn history(&self) -> Vec<HistoryPoint> {
        if self.store.nodes.is_empty() {
            return Vec::new();
        }
        let rec = &self.doc.record;
        let mut nodes = rec.path_to(rec.line_end(self.cursor));
        if !rec.is_main_trunk(self.cursor) {
            let trunk: Vec<NodeId> = rec
                .path_to(rec.line_end(ROOT))
                .into_iter()
                .filter(|n| !nodes.contains(n))
                .collect();
            nodes.extend(trunk);
        }
        nodes
            .into_iter()
            .filter_map(|n| {
                let a = self.store.nodes.get(&n)?;
                Some(HistoryPoint {
                    node: n,
                    move_number: rec.node(n).move_number,
                    to_play: a.to_play,
                    visits: a.visits,
                    winrate: a.winrate,
                    score_lead: a.score_lead,
                })
            })
            .collect()
    }
}
