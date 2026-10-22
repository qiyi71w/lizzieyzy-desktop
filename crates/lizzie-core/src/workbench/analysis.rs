//! Application flow for engines and analysis: which engine is in the foreground, 双引擎模式's
//! secondary engine, the user's analysis pause, analysis limits, the stored analysis of the open
//! document, and the engine settings. The engine runtime owns processes and protocol; this
//! module decides what the foreground and the secondary engine should do and where their
//! results belong.
//!
//! Results are stored on the node they were requested for, even when the user has moved on
//! (a valid result for another node is history, not noise); only the current node's results are
//! shown as current analysis. Results for a document that is no longer open, or for a komi that
//! no longer applies, are dropped. Each engine's results go to its own store: the secondary
//! engine is never taken for the foreground engine.

use std::collections::HashMap;
use std::sync::Arc;

use super::Workbench;
use crate::api::{
    ApiError, CandidateView, ConsoleLineKind, ConsoleLineView, EngineEntryView, EngineFailureView, EngineNotice,
    EngineScope, EngineSettingsView, EngineState, EngineView, ErrorKind, ForegroundEngineView, HistoryPoint,
    NodeAnalysisView, RulesPromptView, SecondaryEngineView, SecondaryView,
};
use crate::browse::Navigation;
use crate::display::{self, BoardOverlayView};
use crate::engine::gtp::InfoLine;
use crate::engine::profile::{new_profile_id, AnalysisSettings, EngineProfile, EngineStartup};
use crate::engine::rules::KataRules;
use crate::engine::runtime::{
    Bootstrap, CheckedLaunch, ConsoleKind, EngineFailure, EngineInput, InstanceId, LaunchCheck, Phase, RulesOutcome,
    RuntimeEvent, Target, UserCommandError,
};
use crate::record::{BoardSize, DocumentEdit, GameRecord, NodeId, Position, Stone, ROOT};
use crate::sanitize::sanitize;
use crate::settings::ExtraMode;
use crate::viewpoint::{ShownValues, Viewpoint};

/// What a rules decision is about: the session rules revision, the foreground engine and the
/// 双引擎模式 secondary engine taking part (`SessionRulesSynchronizer` checks both endpoints).
pub(super) type RulesKey = (u64, InstanceId, Option<InstanceId>);

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

/// Analysis stored for the open document (`BoardData` of every node). Each result keeps the
/// stamp of the node it was made for: after an edit only results whose node is still in the
/// record with the same stamp (the same position) count.
#[derive(Default)]
pub(super) struct AnalysisStore {
    doc: u64,
    nodes: HashMap<NodeId, (u64, NodeAnalysisView)>,
    /// The Kata形势 ownership (`BoardData.estimateArray`), Black-positive: read from a saved
    /// payload, or reported live while ownership was requested.
    ownership: HashMap<NodeId, Vec<f64>>,
    metadata: HashMap<NodeId, crate::formats::saved_analysis::Metadata>,
}

impl AnalysisStore {
    pub(super) fn reset(&mut self, doc: u64) {
        self.doc = doc;
        self.nodes.clear();
        self.ownership.clear();
        self.metadata.clear();
    }

    pub(super) fn get(&self, rec: &GameRecord, node: NodeId) -> Option<&NodeAnalysisView> {
        let (stamp, view) = self.nodes.get(&node)?;
        (rec.contains(node) && rec.node(node).stamp == *stamp).then_some(view)
    }

    pub(super) fn ownership(&self, rec: &GameRecord, node: NodeId) -> Option<&Vec<f64>> {
        self.get(rec, node)?;
        self.ownership.get(&node)
    }

    /// Replaces the node's result and its ownership (none when the result has none).
    pub(super) fn insert(&mut self, node: NodeId, stamp: u64, view: NodeAnalysisView, ownership: Option<Vec<f64>>) {
        self.nodes.insert(node, (stamp, view));
        self.metadata.remove(&node);
        match ownership {
            Some(o) => self.ownership.insert(node, o),
            None => self.ownership.remove(&node),
        };
    }

    /// Drops results whose node was removed or edited.
    pub(super) fn retain_valid(&mut self, rec: &GameRecord) {
        self.nodes
            .retain(|&node, (stamp, _)| rec.contains(node) && rec.node(node).stamp == *stamp);
        let nodes = &self.nodes;
        self.ownership.retain(|node, _| nodes.contains_key(node));
        self.metadata.retain(|node, _| nodes.contains_key(node));
    }

    pub(super) fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub(super) fn pda(&self, node: NodeId) -> f64 {
        self.metadata.get(&node).map_or(0.0, |m| m.pda)
    }

    pub(super) fn payload(&self, rec: &GameRecord, node: NodeId) -> Option<String> {
        let a = self.get(rec, node)?;
        Some(crate::formats::saved_analysis::write(a, rec.size,
            self.metadata.get(&node).copied().unwrap_or_default(), self.ownership(rec, node)))
    }

    /// The analysis saved under `tags` in every node of `rec`.
    pub(super) fn load(
        &mut self,
        rec: &GameRecord,
        tags: [&str; 2],
        settings: &AnalysisSettings,
        no_sugg_circle: bool,
    ) {
        let max = if settings.limit_max_suggestion > 0 && !no_sugg_circle {
            settings.limit_max_suggestion as usize
        } else { 361 };
        let mut pending = vec![ROOT];
        while let Some(id) = pending.pop() {
            let node = rec.node(id);
            pending.extend(node.children.iter().copied());
            if let Some(p) = crate::formats::saved_analysis::load(
                &node.properties,
                tags,
                id,
                rec.size,
                node.to_play,
                rec.info.komi,
                max,
                settings.limit_branch_length,
            ) {
                self.insert(id, node.stamp, p.view, p.ownership);
                self.metadata.insert(id, p.metadata);
            }
        }
    }
}

/// A line for the application log.
pub struct LogLine {
    pub warn: bool,
    pub text: String,
}

/// KataGo ownership of the side to move, Black-positive and rounded to thousandths (the
/// painters use it as an alpha of at most 255 levels).
fn black_ownership(values: Vec<f64>, to_play: Stone) -> Vec<f64> {
    let sign = if to_play == Stone::Black { 1.0 } else { -1.0 };
    values.into_iter().map(|v| (v * sign * 1000.0).round() / 1000.0 + 0.0).collect()
}

/// A reported line as a stored result and its Black-positive ownership.
fn node_analysis(node: NodeId, to_play: Stone, komi: f64, engine: String, line: InfoLine) -> Option<(NodeAnalysisView, Option<Vec<f64>>)> {
    let best = line.candidates.first()?;
    let (winrate, score_lead, score_stdev) = (best.winrate, best.score_lead, best.score_stdev);
    let visits = line.candidates.iter().map(|c| c.visits).sum();
    let ownership = line.ownership.map(|o| black_ownership(o, to_play));
    let view = NodeAnalysisView {
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
                pv_visits: c.pv_visits,
                pv_captures: Vec::new(),
                shown: ShownValues::default(),
                side_to_move: ShownValues::default(),
                ownership: c.ownership.map(|o| black_ownership(o, to_play)),
            })
            .collect(),
        black: ShownValues::default(),
    };
    Some((view, ownership))
}

/// Plays every candidate's variation out from `base` (`Branch`, at most `limit` moves; 0 = the
/// whole variation): what each move captures; nothing under `no-capture`. Whether the board
/// removes them (变化图中去掉死子, `removeDeadChainInVariation`) is the drawing's choice, so a kept
/// (frozen) preview follows that setting at once (`withSimulation`).
fn fill_pv_captures(result: &mut NodeAnalysisView, base: &Position, limit: u32, capture: bool) {
    for c in &mut result.candidates {
        c.pv_captures = display::pv_captures(base, result.to_play, &c.pv, limit, capture);
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

    pub(super) fn log(&mut self, warn: bool, text: String) {
        self.logs.push(LogLine { warn, text });
    }

    // -------------------------------------------------------------------- the engines' wishes

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

    /// 双引擎模式 is shown: the secondary engine analyses with the foreground engine.
    pub(super) fn double_engine(&self) -> bool {
        self.config.ui.extra_mode == ExtraMode::DoubleEngine
    }

    /// The secondary engine whose rules take part: 双引擎模式 with a secondary engine that has
    /// started up (one still loading does not hold back the foreground engine).
    fn rules_secondary(&self) -> Option<InstanceId> {
        self.engines
            .secondary()
            .filter(|&s| self.double_engine() && self.engines.phase(s) == Some(Phase::Ready))
    }

    fn rules_key(&self) -> Option<RulesKey> {
        Some((self.rules_revision, self.engines.foreground()?, self.rules_secondary()))
    }

    /// The user chose to analyse this session with the engines' current rules.
    fn rules_overridden(&self) -> bool {
        self.rules_key().is_some_and(|key| self.rules_override == Some(key))
    }

    /// `SessionRulesSynchronizer.permitsOrdinaryAnalysis`: no declared rules, an explicit
    /// continue, or the foreground engine and a taking-part secondary engine both confirmed
    /// exactly the declared rules.
    fn rules_permit_analysis(&self) -> bool {
        let Some((_, fg, secondary)) = self.rules_key() else {
            return true;
        };
        match &self.session_rules {
            SessionRules::Unspecified => true,
            _ if self.rules_overridden() => true,
            SessionRules::Valid { rules, .. } => [Some(fg), secondary]
                .into_iter()
                .flatten()
                .all(|id| self.engines.rules_outcome(id, rules) == RulesOutcome::Confirmed),
            SessionRules::Invalid { .. } => false,
        }
    }

    /// Asks once per (rules target, engines) when the declared rules cannot be applied
    /// (`offerContinueAfterRulesFailure` / `requestLifecycleRulesOverride`); a secondary engine
    /// that cannot apply them asks too (`rulesFailure = mirrorRules`).
    fn check_rules_prompt(&mut self) {
        let Some(key @ (_, fg, secondary)) = self.rules_key() else {
            return;
        };
        if self.engines.phase(fg) != Some(Phase::Ready) || self.rules_permit_analysis() {
            return;
        }
        if self.rules_prompted == Some(key) {
            return;
        }
        let (target, failure, engine) = match &self.session_rules {
            SessionRules::Invalid { raw } => (raw.clone(), "INVALID_DECLARATION", fg),
            SessionRules::Valid { raw, rules } => {
                let failed = [Some(fg), secondary].into_iter().flatten().find_map(|id| {
                    match self.engines.rules_outcome(id, rules) {
                        RulesOutcome::Failed(f) => Some((id, f)),
                        _ => None,
                    }
                });
                match failed {
                    Some((id, f)) => (raw.clone().unwrap_or_else(|| rules.field_summary()), f, id),
                    None => return,
                }
            }
            SessionRules::Unspecified => return,
        };
        self.rules_prompted = Some(key);
        self.next_prompt += 1;
        self.pending_prompt = Some((self.next_prompt, key));
        self.rules_prompt = Some(RulesPromptView {
            prompt: self.next_prompt,
            target,
            observed: self.engines.confirmed_rules(engine).map(KataRules::field_summary),
            failure: failure.to_string(),
        });
    }

    /// YES: analyse with the engines' current rules for this target and engines. NO: pause.
    pub fn answer_rules_prompt(&mut self, prompt: u64, accept: bool) -> bool {
        let Some((_, key)) = self.pending_prompt.take_if(|(id, _)| *id == prompt) else {
            return false;
        };
        if self.rules_key() != Some(key) {
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

    /// Tells the runtime what the foreground and the secondary engine should do now. Both hold
    /// the browsing position; the secondary engine analyses only in 双引擎模式 and only while the
    /// foreground engine may (the user's pause and a foreground limit stop it too, as Java
    /// mirrors the primary engine's commands), and stops alone at its own limit.
    pub(super) fn sync_engine(&mut self) {
        let here = (self.doc.id, self.cursor);
        // Java resumes after a limit stop once the position changes.
        if self.limit_stop.is_some_and(|l| l != here) {
            self.limit_stop = None;
        }
        if self.secondary_limit_stop.is_some_and(|l| l != here) {
            self.secondary_limit_stop = None;
        }
        let analyse =
            !self.paused && !self.benchmark_running() && self.limit_stop.is_none() && self.rules_permit_analysis();
        let target = self.analysis_target();
        if self.double_engine() {
            self.engines
                .want_secondary(Some(target.clone()), analyse && self.secondary_limit_stop.is_none());
        } else {
            self.engines.want_secondary(None, false);
        }
        self.engines.want(Some(target), analyse);
        self.check_rules_prompt();
    }

    pub(super) fn bootstrap(&self, profile: &EngineProfile) -> Bootstrap {
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
        self.doc.format.is_none() && !rec.has_children_of_root() && rec.node(ROOT).setup.is_empty()
    }

    /// `Board.reopenOnlyBoard` / `clear`: a new empty game of `size` replaces the open one.
    pub(super) fn reopen_empty_board(&mut self, size: BoardSize) {
        let id = self.next_doc;
        self.next_doc += 1;
        self.doc = super::Document {
            id,
            record: Arc::new(GameRecord::empty(size, self.komi)),
            format: None,
            path: None,
            revision: 0,
            saved_revision: None,
            origin_name: None,
        };
        self.cursor = ROOT;
        self.sounds.clear();
        self.reset_analysis();
        self.reset_editing();
        self.publish_record_rules(None);
    }

    // -------------------------------------------------------------------- engine operations

    /// Engine menu `[n] name`: the engine becomes the foreground engine; a running instance (kept
    /// or preloaded) is reused. The previous one keeps running with fast engine change. Every
    /// foreground start goes through here, so none starts during setup mode
    /// (`EngineManager.setupModeActive`).
    pub fn switch_engine(&mut self, index: u32) -> Result<bool, ApiError> {
        let Some(profile) = self.config.engine.engines.get(index as usize).cloned() else {
            return Err(ApiError::new(ErrorKind::InvalidArgument, format!("no engine {index}")));
        };
        if self.setup_mode() {
            // Changed: the notice is part of the next view.
            self.engine_notice = Some(EngineNotice::SetupModeActive);
            return Ok(true);
        }
        let current = self
            .engines
            .foreground()
            .and_then(|f| self.engines.profile_of(f))
            .map(|p| p.id.clone());
        if current.as_deref() == Some(profile.id.as_str()) {
            return Ok(false);
        }
        if self.double_engine() && self.secondary_profile().as_deref() == Some(profile.id.as_str()) {
            // `rejectSameEngineSelection`: the notice is part of the next view.
            self.engine_notice = Some(EngineNotice::SameEngine);
            return Ok(true);
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

    /// The profile id of the secondary engine.
    fn secondary_profile(&self) -> Option<String> {
        self.engines
            .secondary()
            .and_then(|s| self.engines.profile_of(s))
            .map(|p| p.id.clone())
    }

    /// 布局模式 > 默认模式 / 双引擎模式 (`Config.toggleExtraMode`). Leaving 双引擎模式 stops the
    /// secondary engine's analysis and keeps it running (`nameCmdfornoponder`); entering resumes
    /// a chosen one at the current position. Remembered across restarts (`ui.extra-mode`).
    pub fn set_extra_mode(&mut self, mode: ExtraMode) -> Result<bool, ApiError> {
        if self.config.ui.extra_mode == mode {
            return Ok(false);
        }
        self.commit_config(|c| c.ui.extra_mode = mode)?;
        self.sync_engine();
        Ok(true)
    }

    /// Secondary engine menu `[n] name` (`switchEngine(index, false)`): only in 双引擎模式, never
    /// the foreground engine (`rejectSameEngineSelection`). A running instance (kept or
    /// preloaded) is reused; a new one starts with the game komi (`targetKomi = frame.komi`).
    /// The previous secondary engine keeps running with fast engine change, and is chosen again
    /// if the new one fails while starting (`rollbackSecondaryEngineSelection`).
    pub fn switch_secondary_engine(&mut self, index: u32) -> Result<bool, ApiError> {
        let Some(profile) = self.config.engine.engines.get(index as usize) else {
            return Err(ApiError::new(ErrorKind::InvalidArgument, format!("no engine {index}")));
        };
        if !self.double_engine() || self.secondary_profile().as_deref() == Some(profile.id.as_str()) {
            return Ok(false);
        }
        // `submitEngineSwitchIfAvailable` refuses every engine switch during setup mode, the
        // secondary one included (a restart of the chosen one is not refused).
        if self.setup_mode() {
            self.engine_notice = Some(EngineNotice::SetupModeActive);
            return Ok(true);
        }
        let foreground = self
            .engines
            .foreground()
            .and_then(|f| self.engines.profile_of(f))
            .map(|p| p.id.clone());
        if foreground.as_deref() == Some(profile.id.as_str()) {
            self.engine_notice = Some(EngineNotice::SameEngine);
            return Ok(true);
        }
        self.seat_secondary(index);
        self.sync_engine();
        Ok(true)
    }

    /// Makes engine `index` (a listed engine other than the foreground one) the secondary
    /// engine: a running instance is reused, a new one started; a failed start leaves the seat
    /// as it was.
    fn seat_secondary(&mut self, index: u32) {
        let profile = self.config.engine.engines[index as usize].clone();
        let instance = match self.engines.instance_of(&profile.id) {
            Some(i) => i,
            None => match self.engines.start(&profile, self.bootstrap(&profile)) {
                Ok(i) => {
                    self.log(
                        false,
                        format!(
                            "secondary engine start [{}] {}",
                            profile.name,
                            sanitize(&profile.command)
                        ),
                    );
                    i
                }
                Err(f) => {
                    self.engine_failed(*f);
                    return;
                }
            },
        };
        let keep = self.config.engine.analysis.fast_engine_change;
        self.secondary_fallback = match self.engines.secondary() {
            Some(p) if keep && self.engines.phase(instance) != Some(Phase::Ready) => Some((instance, p)),
            _ => None,
        };
        self.secondary_limit_stop = None;
        self.secondary_limit_tip_shown = false;
        self.engines.set_secondary(Some(instance), keep);
    }

    /// Secondary engine menu > "重启当前引擎" (`restartEngine2`): a new process of the same
    /// engine, synced to the current position.
    pub fn restart_secondary_engine(&mut self) -> Result<bool, ApiError> {
        let Some(secondary) = self.engines.secondary() else {
            return Ok(false);
        };
        let Some(id) = self.secondary_profile() else {
            return Ok(false);
        };
        self.engines.terminate(secondary);
        self.secondary_fallback = None;
        if let Some(index) = self.config.engine.index_of(&id) {
            self.seat_secondary(index as u32);
        }
        self.sync_engine();
        Ok(true)
    }

    /// Secondary engine menu > "关闭当前引擎" (`killThisEngines2`): the process ends and no
    /// secondary engine is chosen; its stored results stay.
    pub fn shutdown_secondary_engine(&mut self) -> bool {
        let Some(secondary) = self.engines.secondary() else {
            return false;
        };
        self.engines.terminate(secondary);
        self.secondary_fallback = None;
        self.sync_engine();
        true
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
        if !running && self.benchmark_running() {
            self.stop_benchmark();
        }
        self.limit_stop = None;
        self.secondary_limit_stop = None;
        self.sync_engine();
        Ok(true)
    }

    /// Java enables pondering when H is turned on; switching H off resumes the same analysis
    /// path as an ordinary resume, including limits and session-rules handling.
    pub fn toggle_raw_net(&mut self) -> bool {
        if !self.engines.toggle_raw_net(self.double_engine()) {
            return false;
        }
        self.paused = false;
        self.limit_stop = None;
        self.secondary_limit_stop = None;
        self.sync_engine();
        true
    }

    pub fn stop_raw_net(&mut self) -> bool {
        if !self.engines.stop_raw_net() { return false; }
        self.paused = false;
        self.limit_stop = None;
        self.secondary_limit_stop = None;
        self.sync_engine();
        true
    }

    pub(super) fn raw_net_view(&self) -> Option<crate::engine::raw_net::RawNetView> {
        self.engines.raw_net_view(&self.analysis_target())
    }

    /// Drops every stored result of both engines for a new document or analysis context.
    pub(super) fn reset_analysis(&mut self) {
        self.store.reset(self.doc.id);
        self.secondary_store.reset(self.doc.id);
    }

    /// Java's `clearBestMovesAfter(start)` for a changed analysis context (komi, manual rules):
    /// the secondary engine's results only go in 双引擎模式; otherwise they are kept and saved.
    fn clear_best_moves(&mut self) {
        self.store.reset(self.doc.id);
        if self.double_engine() {
            self.secondary_store.reset(self.doc.id);
        }
    }

    /// Drops both engines' results whose node was removed or edited.
    pub(super) fn retain_valid_analysis(&mut self) {
        self.store.retain_valid(&self.doc.record);
        self.secondary_store.retain_valid(&self.doc.record);
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

    /// Every komi change of the game: the document records it and the stored analysis of the
    /// game is discarded (`clearBestMovesAfter(start)`).
    fn apply_komi(&mut self, komi: f64) -> bool {
        let recorded = matches!(self.edit_document(DocumentEdit::SetKomi { komi }), Ok(true));
        if !recorded && komi == self.komi {
            return false;
        }
        self.komi = komi;
        self.clear_best_moves();
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
        self.clear_best_moves();
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
        match self.engines.send_user(command, self.double_engine()) {
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
        let previous = self.config.engine.analysis.clone();
        self.commit_config(|c| c.engine.analysis = settings)?;
        self.analysis_settings_changed(&previous);
        Ok(true)
    }

    /// Puts committed analysis settings into effect: the engine update interval and, for a new
    /// variation length, the previews of stored analysis.
    pub(super) fn analysis_settings_changed(&mut self, previous: &AnalysisSettings) {
        let settings = &self.config.engine.analysis;
        self.engines.set_interval(settings.interval_centisec);
        if settings.limit_branch_length != previous.limit_branch_length {
            self.refresh_pv_captures();
        }
    }

    /// Plays the variations of stored analysis out again under the current length and rules.
    pub(super) fn refresh_pv_captures(&mut self) {
        let length = self.config.engine.analysis.limit_branch_length;
        let capture = !self.config.edit.no_capture;
        let record = self.doc.record.clone();
        for (node, (_, result)) in self.store.nodes.iter_mut().chain(self.secondary_store.nodes.iter_mut()) {
            fill_pv_captures(result, &record.position(*node), length, capture);
        }
    }

    /// Engine settings saved (MoreEngines close). New entries get ids, one default at most.
    /// Engines whose launch-relevant fields changed restart; removed ones end. The foreground
    /// and the secondary engine come back in their seats (`updateEnginesWhileMarkedUpdating`).
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
        let secondary_profile = self.secondary_profile();
        let (mut restart_fg, mut restart_secondary) = (None, None);
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
                if secondary_profile.as_deref() == Some(id.as_str()) {
                    restart_secondary = after;
                }
            }
        }
        if let Some(i) = restart_fg {
            self.switch_engine(i as u32)?;
        }
        if let Some(i) = restart_secondary {
            self.seat_secondary(i as u32);
            self.sync_engine();
        }
        Ok(true)
    }

    // -------------------------------------------------------------------- engine output

    /// Output of an engine process. Returns whether the view changed.
    pub fn engine_input(&mut self, instance: InstanceId, input: EngineInput) -> bool {
        let events = self.engines.input(instance, input);
        self.engine_events(events)
    }

    /// Engine starts waiting for an artifact fingerprint check, each handed out once. The
    /// platform runs them off the workbench and reports back through [`Self::finish_launch`].
    pub fn take_launch_checks(&mut self) -> Vec<LaunchCheck> {
        self.engines.take_launch_checks()
    }

    /// Starts the engine a [`LaunchCheck`] was made for, unless it was retired meanwhile.
    /// Returns whether the view changed.
    pub fn finish_launch(&mut self, checked: CheckedLaunch) -> bool {
        let events = self.engines.finish_launch(checked);
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
                RuntimeEvent::RawNet { .. } => {}
                RuntimeEvent::Failed(f) => {
                    // A secondary engine that fails while starting gives its seat back to the
                    // one it replaced, when that one still runs.
                    if let Some((_, previous)) = self.secondary_fallback.take_if(|(new, _)| *new == f.instance) {
                        if f.during_startup
                            && self.engines.phase(previous).is_some()
                            && self.engines.secondary().is_none()
                        {
                            self.engines.set_secondary(Some(previous), true);
                        }
                    }
                    self.engine_failed(f);
                }
                RuntimeEvent::Analysis {
                    instance,
                    doc,
                    node,
                    stamp,
                    komi,
                    rules_rev,
                    to_play,
                    engine_name,
                    line,
                    elapsed,
                } => {
                    // Only the foreground and the secondary engine's results count, each in its
                    // own store: a switched-away, preloaded or background engine's late lines
                    // have no effect.
                    let secondary = Some(instance) == self.engines.secondary();
                    if !secondary && Some(instance) != self.engines.foreground() {
                        continue;
                    }
                    if doc != self.doc.id
                        || komi != self.komi
                        || doc != self.store.doc
                        || rules_rev != self.rules_revision
                    {
                        continue; // another document, a komi or rules that no longer apply
                    }
                    let rec = &self.doc.record;
                    if !rec.contains(node) || rec.node(node).stamp != stamp {
                        continue; // the node was removed or edited since the request
                    }
                    let Some((result, ownership)) = node_analysis(node, to_play, komi, engine_name, line) else {
                        continue;
                    };
                    let visits = result.visits;
                    let store = if secondary {
                        &mut self.secondary_store
                    } else {
                        &mut self.store
                    };
                    let replace = store.get(rec, node).is_none_or(|old| more_visits(&result, old));
                    if replace {
                        let mut result = result;
                        let limit = self.config.engine.analysis.limit_branch_length;
                        fill_pv_captures(&mut result, &rec.position(node), limit, !self.config.edit.no_capture);
                        store.insert(node, stamp, result, ownership);
                    }
                    let running = node == self.cursor
                        && !self.paused
                        && self.limit_stop.is_none()
                        && !self.engines.raw_net_active();
                    if running && !secondary {
                        self.check_limits(visits, elapsed);
                    } else if running && self.secondary_limit_stop.is_none() {
                        self.check_secondary_limits(visits, elapsed);
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

    /// `Leelaz` analysis limits: past the time or visit limit (`limit-playout`, `limit-time`).
    /// A time limit of 0 means 9999 minutes (`Config.java:2153-2155`).
    fn limit_reached(&self, visits: u64, elapsed: std::time::Duration) -> bool {
        let a = &self.config.engine.analysis;
        let limit_seconds = match a.max_analyze_time_seconds {
            0 => 9999 * 60,
            s => u64::from(s),
        };
        let by_visits = a.limit_playout && visits > a.limit_playouts;
        let by_time = a.limit_time && elapsed.as_secs_f64() > limit_seconds as f64;
        by_visits || by_time
    }

    /// The foreground engine's limit stops the analysis of this node, the secondary engine's
    /// too (it resumes when the position changes); the notice is shown once per engine start.
    fn check_limits(&mut self, visits: u64, elapsed: std::time::Duration) {
        if !self.limit_reached(visits, elapsed) {
            return;
        }
        self.limit_stop = Some((self.doc.id, self.cursor));
        if self.config.engine.analysis.show_ponder_limited_tips && !self.limit_tip_shown {
            self.limit_tip_shown = true;
            self.engine_notice = Some(EngineNotice::StoppedByLimit);
        }
        self.sync_engine();
    }

    /// The secondary engine's own limit stops only its analysis of this node (Java `leelaz2`
    /// stops itself; the stop is not mirrored back), with the notice once per choice.
    fn check_secondary_limits(&mut self, visits: u64, elapsed: std::time::Duration) {
        if !self.limit_reached(visits, elapsed) {
            return;
        }
        self.secondary_limit_stop = Some((self.doc.id, self.cursor));
        if self.config.engine.analysis.show_ponder_limited_tips && !self.secondary_limit_tip_shown {
            self.secondary_limit_tip_shown = true;
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
            repair: None,
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
        let secondary = self.engines.secondary().and_then(|s| {
            let profile = self.engines.profile_of(s)?;
            let here = (self.doc.id, self.cursor);
            Some(SecondaryEngineView {
                index: self.config.engine.index_of(&profile.id)? as u32,
                name: self.engines.display_name(s)?,
                katago: self.engines.is_katago(s),
                can_analyse: self.engines.can_analyse(s),
                ready: self.engines.complete(s),
                analysing: self.engines.analysing_by(s, here.0, here.1),
                stopped_by_limit: self.secondary_limit_stop == Some(here),
            })
        });
        EngineView {
            engines,
            foreground,
            secondary,
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
            startup_benchmark: s.startup_benchmark,
        }
    }

    /// The display settings' conversion of the engine's numbers.
    fn viewpoint(&self) -> Viewpoint {
        Viewpoint::from(&self.config.display)
    }

    pub(super) fn current_analysis(&self) -> Option<NodeAnalysisView> {
        self.shown_analysis(&self.store)
    }

    /// `store`'s result for the current node as the displays show it.
    fn shown_analysis(&self, store: &AnalysisStore) -> Option<NodeAnalysisView> {
        let s = store.get(&self.doc.record, self.cursor)?;
        // Stored results keep the engine's numbers; what is shown follows the settings now. The
        // komi is the one the result was analysed with (results of another komi are dropped).
        let vp = self.viewpoint();
        // A candidate's own ownership is drawn only while Kata形势 and 按选点显示评估 are on;
        // otherwise it is not copied at all.
        let moves_ownership = self.estimate.state.shown && self.config.display.use_moves_ownership;
        let candidates = s
            .candidates
            .iter()
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
                pv: c.pv.clone(),
                pv_visits: c.pv_visits.clone(),
                pv_captures: c.pv_captures.clone(),
                shown: vp.candidate(c.winrate, c.score_lead, s.to_play, s.komi),
                side_to_move: vp.side_to_move(c.winrate, c.score_lead, s.to_play, s.komi),
                ownership: if moves_ownership { c.ownership.clone() } else { None },
            })
            .collect();
        Some(NodeAnalysisView {
            node: s.node,
            engine: s.engine.clone(),
            to_play: s.to_play,
            visits: s.visits,
            root_visits: s.root_visits,
            winrate: s.winrate,
            score_lead: s.score_lead,
            score_stdev: s.score_stdev,
            komi: s.komi,
            candidates,
            black: vp.black(s.winrate, s.score_lead, s.to_play, s.komi),
        })
    }

    /// 双引擎模式's right board (`boardRenderer2`): the secondary engine's results of the current
    /// node, the marks drawn from them, its pure network and its Kata形势 (`estimateArray2`, the
    /// previous position's until this one has its own).
    pub(super) fn secondary_view(&self) -> Option<SecondaryView> {
        if !self.double_engine() {
            return None;
        }
        let rec = &self.doc.record;
        let store = &self.secondary_store;
        let ownership = self.estimate.state.shown.then(|| {
            store
                .ownership(rec, self.cursor)
                .or_else(|| rec.parent(self.cursor).and_then(|p| store.ownership(rec, p)))
                .cloned()
        });
        Some(SecondaryView {
            analysis: self.shown_analysis(store),
            board: self.overlay(store),
            raw_net: self
                .engines
                .secondary()
                .and_then(|s| self.engines.raw_net_view_of(s, &self.analysis_target())),
            ownership: ownership.flatten(),
            history: self.history_of(store),
        })
    }

    /// The current line (root to the line end) and, off the trunk, the rest of the main trunk.
    pub(super) fn graph_nodes(&self) -> Vec<NodeId> {
        let rec = &self.doc.record;
        let mut nodes = rec.path_to(rec.line_end(self.cursor));
        if !rec.is_main_trunk(self.cursor) {
            let mut line = nodes.clone();
            line.sort_unstable();
            let trunk = rec.path_to(rec.line_end(ROOT));
            nodes.extend(trunk.into_iter().filter(|n| line.binary_search(n).is_err()));
        }
        nodes
    }

    /// Stored results on the current line (root to the line end) and the main trunk.
    pub(super) fn history(&self) -> Vec<HistoryPoint> {
        self.history_of(&self.store)
    }

    fn history_of(&self, store: &AnalysisStore) -> Vec<HistoryPoint> {
        if store.is_empty() {
            return Vec::new();
        }
        let rec = &self.doc.record;
        let vp = self.viewpoint();
        self.graph_nodes()
            .into_iter()
            .filter_map(|n| {
                let a = store.get(rec, n)?;
                Some(HistoryPoint {
                    node: n,
                    move_number: rec.node(n).move_number,
                    to_play: a.to_play,
                    visits: a.visits,
                    winrate: a.winrate,
                    score_lead: a.score_lead,
                    black: vp.black(a.winrate, a.score_lead, a.to_play, a.komi),
                    as_candidate: vp.as_candidate(a.winrate, a.score_lead, a.to_play, a.komi),
                    score_stdev: a.score_stdev,
                })
            })
            .collect()
    }

    /// Move numbers, 落子评价 and 下一手 详细 of the current position.
    pub(super) fn board_overlay(&self) -> BoardOverlayView {
        self.overlay(&self.store)
    }

    /// The board marks of the current position, the analysis-based ones from `store`.
    fn overlay(&self, store: &AnalysisStore) -> BoardOverlayView {
        let rec = &self.doc.record;
        let s = &self.config.display;
        let path = rec.path_to(self.cursor);
        let numbering = display::numbering(rec, &path);
        let analysis = |n: NodeId| store.get(rec, n);
        BoardOverlayView {
            move_numbers: display::number_labels(rec, &path, &numbering, s),
            ranks: display::rank_marks(rec, &path, &numbering, s, analysis),
            rank_corner: display::shows_ranks(s) && (s.move_rank == 0 || s.move_rank > 1),
            next_move: display::next_move_info(rec, self.cursor, s, self.config.edit.no_capture,
                self.config.engine.analysis.limit_branch_length, analysis),
            info_order_sample: display::info_order_sample(s, rec.node(self.cursor).to_play, self.komi),
        }
    }
}
