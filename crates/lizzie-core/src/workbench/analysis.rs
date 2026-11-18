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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::Workbench;
use crate::api::{
    AnalysisScope, ApiError, CandidateView, ConsoleLineKind, ConsoleLineView, EngineEntryView,
    EngineFailureView, EngineNotice, EngineScope, EngineSettingsView, EngineState, EngineView, ErrorKind,
    ForegroundEngineView, HistoryPoint, NodeAnalysisView, RulesPromptView, SecondaryEngineView, SecondaryView,
    ClickReviewView,
};
use crate::browse::Navigation;
use crate::display::{self, BoardOverlayView};
use crate::engine::gtp::InfoLine;
use crate::engine::profile::{new_profile_id, AnalysisSettings, EngineProfile, EngineStartup};
use crate::engine::rules::KataRules;
use crate::engine::ssh::RemoteFailure;
use crate::engine::runtime::{
    Bootstrap, CheckedLaunch, ConsoleKind, EngineFailure, EngineInput, InstanceId, LaunchCheck, ModelIdentity,
    ModelLookup, Phase, RulesOutcome, RuntimeEvent, Target, UserCommandError,
};
use crate::record::{BoardSize, DocumentEdit, GameRecord, NodeId, NodeKind, Point, Position, Stone, ROOT};
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
    /// Results the next live result replaces even with fewer visits (`BoardData.isChanged`): a
    /// 强制计算区域 changed the search they were made with.
    stale: HashSet<NodeId>,
    /// When each kept candidate's search last grew (`MoveData.lastTimeUnlimitedTime`).
    kept_since: HashMap<NodeId, HashMap<Option<Point>, Instant>>,
    /// Candidates beyond 最大候选点数 that stayed in the node's `bestMoves` because 选点重点研究
    /// tracked them when the result came (`tryToLimitMoves`' attended points); the board does
    /// not draw them for that.
    attended: HashMap<NodeId, Vec<Point>>,
    /// 最大候选点数 (0 = none) each result was limited with when it came or was loaded
    /// (`tryToLimitMoves` splits `bestMoves` then; a later change of the setting does not).
    limits: HashMap<NodeId, usize>,
}

/// How long a kept candidate whose visits stopped growing stays kept.
const KEPT_FOR: Duration = Duration::from_secs(3);

impl AnalysisStore {
    pub(super) fn reset(&mut self, doc: u64) {
        self.doc = doc;
        self.nodes.clear();
        self.ownership.clear();
        self.metadata.clear();
        self.stale.clear();
        self.kept_since.clear();
        self.attended.clear();
        self.limits.clear();
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
        self.stale.remove(&node);
        match ownership {
            Some(o) => self.ownership.insert(node, o),
            None => self.ownership.remove(&node),
        };
    }

    /// An engine's new result for the node (`BoardData.tryToSetBestMoves`): beyond `limit`
    /// candidates (0 = none) the one just past it stays a candidate while it was one of the node's
    /// previous candidates and is still searched, and so do `tracked` points (`tryToLimitMoves`);
    /// the others are not. Results are in candidate order.
    pub(super) fn update(
        &mut self,
        node: NodeId,
        stamp: u64,
        mut view: NodeAnalysisView,
        ownership: Option<Vec<f64>>,
        limit: usize,
        tracked: &[Point],
    ) {
        let previous = self.nodes.get(&node).filter(|(s, _)| *s == stamp).map(|(_, v)| v);
        let since = self.kept_since.remove(&node).unwrap_or_default();
        let mut kept = HashMap::new();
        let mut attended = Vec::new();
        if limit > 0 && view.candidates.len() > limit {
            let now = Instant::now();
            // The previous candidates as the estimate and the tables saw them (`bestMoves`).
            let last: Vec<&CandidateView> = previous.map_or_else(Vec::new, |p| {
                p.candidates.iter().enumerate().filter(|(i, c)| *i < limit || c.kept).map(|(_, c)| c).collect()
            });
            for c in &view.candidates[limit..] {
                if let Some(p) = c.point.filter(|p| tracked.contains(p)) {
                    attended.push(p);
                }
                let mut keep = None;
                if (c.order as usize) < limit + 1 {
                    for (s, before) in last.iter().enumerate().take(limit + 1) {
                        if (s >= limit && !before.kept) || c.point != before.point {
                            continue;
                        }
                        let time = since.get(&before.point).copied();
                        if c.visits > before.visits || !before.kept {
                            keep = Some(now);
                        } else if let Some(t) = time.filter(|t| now.duration_since(*t) < KEPT_FOR) {
                            keep = Some(t);
                        }
                    }
                }
                if let Some(t) = keep {
                    kept.insert(c.point, t);
                }
            }
        }
        for c in &mut view.candidates {
            c.kept = kept.contains_key(&c.point);
        }
        self.insert(node, stamp, view, ownership);
        self.limits.insert(node, limit);
        if !kept.is_empty() {
            self.kept_since.insert(node, kept);
        }
        if attended.is_empty() {
            self.attended.remove(&node);
        } else {
            self.attended.insert(node, attended);
        }
    }

    /// The points [`Self::update`] kept beyond the limit because they were tracked.
    pub(super) fn attended(&self, node: NodeId) -> &[Point] {
        self.attended.get(&node).map_or(&[], Vec::as_slice)
    }

    /// The 最大候选点数 (0 = none) the node's result was limited with: candidates past it are
    /// the node's `bestMovesOutOfRange` unless [kept](CandidateView::kept) or [attended](Self::attended).
    pub(super) fn limit(&self, node: NodeId) -> usize {
        self.limits.get(&node).copied().unwrap_or(0)
    }

    /// `refreshFocusCandidateVisibility` after the tracked points changed: the node's result is
    /// limited again with no previous one, so nothing stays kept and only the points tracked now
    /// stay beyond `limit`.
    pub(super) fn refocus(&mut self, node: NodeId, limit: usize, tracked: &[Point]) {
        let Some((_, view)) = self.nodes.get_mut(&node) else {
            return;
        };
        for c in &mut view.candidates {
            c.kept = false;
        }
        self.kept_since.remove(&node);
        self.limits.insert(node, limit);
        let attended: Vec<Point> = match limit {
            0 => Vec::new(),
            _ => view.candidates.iter().skip(limit).filter_map(|c| c.point.filter(|p| tracked.contains(p))).collect(),
        };
        if attended.is_empty() {
            self.attended.remove(&node);
        } else {
            self.attended.insert(node, attended);
        }
    }

    /// Drops results whose node was removed or edited.
    pub(super) fn retain_valid(&mut self, rec: &GameRecord) {
        self.nodes
            .retain(|&node, (stamp, _)| rec.contains(node) && rec.node(node).stamp == *stamp);
        let nodes = &self.nodes;
        self.ownership.retain(|node, _| nodes.contains_key(node));
        self.metadata.retain(|node, _| nodes.contains_key(node));
        self.stale.retain(|node| nodes.contains_key(node));
        self.kept_since.retain(|node, _| nodes.contains_key(node));
        self.attended.retain(|node, _| nodes.contains_key(node));
        self.limits.retain(|node, _| nodes.contains_key(node));
    }

    /// `clearBestMovesAfter(start)`: every stored result gives way to the next live one.
    pub(super) fn invalidate_all(&mut self) {
        self.stale = self.nodes.keys().copied().collect();
    }

    /// `clearbestmoves`: the node's stored result gives way to the next live one.
    pub(super) fn invalidate(&mut self, node: NodeId) {
        if self.nodes.contains_key(&node) {
            self.stale.insert(node);
        }
    }

    fn is_stale(&self, node: NodeId) -> bool {
        self.stale.contains(&node)
    }

    /// Whether `result` takes the place of the node's stored result (`BoardData`'s Lizzie
    /// cache): with the cache on only when there is none, it was marked to give way, it was
    /// made with another PDA than `pda`, the one of the new result's engine
    /// (`cachedPda != engine.pda`), or the new search went further; with the cache off always.
    pub(super) fn takes_place(&self, rec: &GameRecord, node: NodeId, result: &NodeAnalysisView, cache: bool, pda: f64) -> bool {
        !cache
            || self.is_stale(node)
            || self.pda(node) != pda
            || self.get(rec, node).is_none_or(|old| more_visits(result, old))
    }

    /// A live result just stored was made with PDA `pda` (`BoardData.pda`): it is saved with it.
    pub(super) fn made_with_pda(&mut self, node: NodeId, pda: f64) {
        if pda != 0.0 && self.nodes.contains_key(&node) {
            let metadata = crate::formats::saved_analysis::Metadata { pda, header_slots: 0 };
            self.metadata.insert(node, metadata);
        }
    }

    /// `clearBestMovesInfomation`: the node's analysis is gone; it is neither shown nor saved.
    pub(super) fn erase(&mut self, node: NodeId) {
        self.nodes.remove(&node);
        self.ownership.remove(&node);
        self.metadata.remove(&node);
        self.stale.remove(&node);
        self.kept_since.remove(&node);
        self.attended.remove(&node);
        self.limits.remove(&node);
    }

    /// The results of `nodes` (and what goes with them), moved out into a store of their own;
    /// [`Self::rejoin`] takes them back when those nodes are in the record again.
    pub(super) fn split_off(&mut self, nodes: &[NodeId]) -> AnalysisStore {
        fn take<V>(from: &mut HashMap<NodeId, V>, to: &mut HashMap<NodeId, V>, node: NodeId) {
            if let Some(v) = from.remove(&node) {
                to.insert(node, v);
            }
        }
        let mut part = AnalysisStore { doc: self.doc, ..AnalysisStore::default() };
        for &node in nodes {
            take(&mut self.nodes, &mut part.nodes, node);
            take(&mut self.ownership, &mut part.ownership, node);
            take(&mut self.metadata, &mut part.metadata, node);
            take(&mut self.kept_since, &mut part.kept_since, node);
            take(&mut self.attended, &mut part.attended, node);
            take(&mut self.limits, &mut part.limits, node);
            if self.stale.remove(&node) {
                part.stale.insert(node);
            }
        }
        part
    }

    /// Takes back the results [`Self::split_off`] moved out of this document's store. Node ids
    /// are never handed out twice, so nothing stored meanwhile is for the same nodes; results
    /// whose node is not in `rec` with the same stamp stay out.
    pub(super) fn rejoin(&mut self, part: AnalysisStore, rec: &GameRecord) {
        if part.doc != self.doc {
            return;
        }
        self.nodes.extend(part.nodes);
        self.ownership.extend(part.ownership);
        self.metadata.extend(part.metadata);
        self.stale.extend(part.stale);
        self.kept_since.extend(part.kept_since);
        self.attended.extend(part.attended);
        self.limits.extend(part.limits);
        self.retain_valid(rec);
    }

    /// `clearBestMovesInfomationAfter(start)`: every node's analysis is gone.
    pub(super) fn erase_all(&mut self) {
        self.reset(self.doc);
    }

    pub(super) fn doc(&self) -> u64 {
        self.doc
    }

    pub(super) fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Some result carries a KataGo score (`Board.isContainsKataData`).
    pub(super) fn has_scores(&self) -> bool {
        self.nodes.values().any(|(_, v)| v.score_lead.is_some())
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
        let limit = if settings.limit_max_suggestion > 0 && !no_sugg_circle {
            settings.limit_max_suggestion as usize
        } else { 0 };
        let max = if limit > 0 { limit } else { 361 };
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
                self.limits.insert(id, limit);
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
pub(super) fn node_analysis(
    node: NodeId,
    to_play: Stone,
    komi: f64,
    engine: String,
    line: InfoLine,
) -> Option<(NodeAnalysisView, Option<Vec<f64>>)> {
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
                kept: false,
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
pub(super) fn fill_pv_captures(result: &mut NodeAnalysisView, base: &Position, limit: u32, capture: bool) {
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
        let requested = self.config.engine.startup_index();
        // Java resolveStartupSelection: a forgotten login changes this launch only, not the
        // saved provider/default/last selection. Custom WebSocket profiles need no account login.
        let fallback = requested.filter(|&index| {
            !self.cloud_authenticated && self.config.engine.engines[index].cloud.as_ref()
                .is_some_and(|launch| launch.provider == crate::engine::cloud::CloudProvider::Zhizi)
        }).and_then(|_| self.first_usable_local_engine());
        let startup = fallback.or(requested);
        let settings = &self.config.engine;
        let preload: Vec<usize> = settings
            .engines
            .iter()
            .enumerate()
            .filter(|(i, e)| e.preload && Some(*i) != startup)
            .map(|(i, _)| i)
            .collect();
        if let Some(i) = startup {
            let _ = self.switch_engine_inner(i as u32, fallback.is_none());
        }
        for i in preload {
            let profile = self.config.engine.engines[i].clone();
            if let Err(f) = self.engines.start(&profile, self.bootstrap(&profile)) {
                self.engine_failed(*f);
            }
        }
    }

    /// Exit: every engine is ended (quit, then killed after the grace period) and the batch with
    /// them, so a read still in flight starts nothing. The platform adapter waits for the
    /// processes outside the workbench lock.
    pub fn end_engines(&mut self) {
        self.contribution_end_for_exit();
        self.fox_stop();
        self.batch_shutdown();
        self.shutdown_strength();
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

    /// The session rules the engines are given: the record's valid declaration, unless overridden.
    pub(super) fn target_rules(&self) -> Option<&KataRules> {
        match &self.session_rules {
            SessionRules::Valid { rules, .. } if !self.rules_overridden() => Some(rules),
            _ => None,
        }
    }

    pub(super) fn analysis_target(&self) -> Target {
        Target {
            doc: self.doc.id,
            node: self.cursor,
            record: self.doc.record.clone(),
            komi: self.komi,
            rules: if self.match_active() { self.match_rules() } else { self.target_rules().cloned() },
            rules_rev: self.rules_revision,
        }
    }

    /// The position the foreground engine is given: 批量分析's record while it runs there, else
    /// the browsing position.
    pub(super) fn foreground_target(&self) -> Target {
        if self.auto_analysis_holds_foreground() {
            if let Some(target) = self.batch_target() {
                return target;
            }
        }
        self.analysis_target()
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
    /// mirrors the primary engine's commands), and stops alone at its own limit. While
    /// 综合设置 holds analysis, or a 搜索线程数 restore runs, neither engine analyses, in every
    /// mode. A waiting analysis engine warm-up decides again on the changed state.
    pub(super) fn sync_engine(&mut self) {
        self.warmup_review();
        let hold = self.analysis_hold || self.search_threads_restoring();
        if self.fox_holds_engine() || self.fox_owns_board() {
            self.engines.want_secondary(None, false);
            self.engines.want(None, false);
            return;
        }
        if let Some(analyse) = self.human_sl_analyse() {
            self.engines.want_secondary(None, false);
            if analyse { self.engines.want(Some(self.analysis_target()), !hold); }
            else { self.engines.want(None, false); }
            return;
        }
        let here = (self.doc.id, self.cursor);
        // Java resumes after a limit stop once the position changes.
        if self.limit_stop.is_some_and(|l| l != here) {
            self.limit_stop = None;
        }
        if self.secondary_limit_stop.is_some_and(|l| l != here) {
            self.secondary_limit_stop = None;
        }
        if self.empty_board_resumed.is_some_and(|l| l != here) {
            self.empty_board_resumed = None;
        }
        if self.side_hold.is_some_and(|l| l != here) {
            self.side_hold = None;
        }
        // 批量分析 analyses its own record with the foreground engine (the user's pause stops it,
        // as Java's ponder toggle does); the secondary engine stays idle meanwhile.
        if self.auto_analysis_holds_foreground() {
            let analyse = !hold && !self.paused && !self.benchmark_running() && !self.quick_holds_foreground();
            self.engines.want_secondary(None, false);
            self.engines.want(Some(self.foreground_target()), analyse);
            return;
        }
        if let Some(analyse) = self.match_analyse() {
            self.engines.want_secondary(None, false);
            if !self.match_foreground_current() {
                self.engines.want(None, false);
                return;
            }
            self.engines.want(Some(self.analysis_target()), analyse && !hold);
            return;
        }
        // A game decides whether its engine analyses; the secondary engine stays idle.
        if let Some(analyse) = self.play_analyse() {
            let analyse = analyse && !hold && !self.benchmark_running() && self.rules_permit_analysis();
            self.engines.want_secondary(None, false);
            self.engines.want(Some(self.analysis_target()), analyse);
            self.check_rules_prompt();
            return;
        }
        // A running quick analysis leaves the foreground engine idle until it ends; ReadBoard GMA
        // keeps it following the position without analysing (its moves come from
        // `kata-genmove_analyze`).
        let analyse = !hold
            && !self.paused
            && !self.benchmark_running()
            && self.limit_stop.is_none()
            && self.side_hold.is_none()
            && !self.quick_holds_foreground()
            && !self.flash_holds_foreground()
            && !self.whole_game_holds_foreground()
            && !self.batch_holds_foreground()
            && !self.readboard_gma_busy()
            && self.rules_permit_analysis();
        let target = self.analysis_target();
        self.sync_search(analyse);
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
        let p = &s.kata_params;
        Bootstrap {
            komi: self.komi,
            rules,
            pda: p.auto_load_pda_value.filter(|_| p.auto_load_pda),
            wrn: p.auto_load_wrn_value.filter(|_| p.auto_load_wrn),
            leela_options: s.leela_options.auto_loaded(),
            custom_pda: s.custom_pda.clone(),
        }
    }

    /// Whether the open game is still a new empty game: no moves, no setup and not read from a
    /// record (`shouldApplyInitialEngineKomiToCurrentGame`, `boardEmpty`).
    fn game_is_empty(&self) -> bool {
        let rec = &self.doc.record;
        self.doc.format.is_none() && !rec.has_children_of_root() && rec.node(ROOT).setup.is_empty()
    }

    /// `Board.reopenOnlyBoard` / `clear`: a new empty game of `size` replaces the open one; a
    /// running game ends first. A clear that is not an engine game's own also ends a 在线同步
    /// follow, its first game included (`clearBoardState(false)` → `urlSgfStopSyncAction`,
    /// Board.java:5493-5525).
    pub(super) fn reopen_empty_board(&mut self, size: BoardSize, engine_game: bool) {
        self.play_document_closing();
        if !engine_game {
            self.online_stop();
        }
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
        self.quick_cancel();
        self.flash_document_closing();
        self.auto_analysis_document_replaced();
        self.whole_game_document_closing();
        self.reset_analysis();
        self.reset_editing();
        self.publish_record_rules(None);
        self.readboard_history_overwritten();
    }

    // -------------------------------------------------------------------- engine operations

    /// Engine menu `[n] name`: the engine becomes the foreground engine; a running instance (kept
    /// or preloaded) is reused. The previous one keeps running with fast engine change. Every
    /// foreground start goes through here, so none starts during setup mode
    /// (`EngineManager.setupModeActive`).
    pub fn switch_engine(&mut self, index: u32) -> Result<bool, ApiError> {
        if self.fox_owns_board() || self.fox_holds_engine() { return Err(ApiError::new(ErrorKind::Unavailable, "Fox.conflict")); }
        if let Some(key) = self.foreground_lifecycle_held() { return Err(ApiError::new(ErrorKind::Unavailable, key)); }
        self.switch_engine_inner(index, true)
    }

    /// What keeps the foreground engine from a switch, restart or restart of all engines, also one
    /// that does not come through `game_gate` (远程算力, resource setup, tuning, settings import):
    /// Java's switch first takes `reserveEngineLifecycle` (EngineManager.java:12232-12235,
    /// 14386-14426), which Web 试下 and a GMA hand or its restore refuse
    /// (Leelaz.java:15156-15160), as does a running contribution (EngineManager.java:11524-11527).
    /// Returns the refusal's message key.
    pub(super) fn foreground_lifecycle_held(&self) -> Option<&'static str> {
        if self.web_trial_active() {
            Some("AnalysisSettings.reuseStatus.application_exclusive_mode")
        } else if self.readboard.gma.reserved() {
            Some("AnalysisSettings.reuseStatus.existing_lease")
        } else if self.contribution_active() {
            Some(super::contribution::CONTRIBUTING)
        } else {
            None
        }
    }

    fn switch_engine_inner(&mut self, index: u32, remember_selection: bool) -> Result<bool, ApiError> {
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
            self.reopen_empty_board(size, false);
        }
        // The engine's komi becomes the game komi unless the user chose it (`targetKomi`): at
        // once for a running engine, for a new one when it is ready and the game still empty.
        let target_komi = if self.komi_changed { self.komi } else { profile.komi };
        self.start_komi = None;
        self.auto_analysis_abandon();
        self.quick_engine_changed();
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
                    self.engines.foreground_launch_failed(&profile);
                    self.engine_failed(*f);
                    return Ok(true);
                }
            },
        };
        let keep = self.config.engine.analysis.fast_engine_change;
        self.limit_stop = None;
        self.limit_tip_shown = false;
        self.empty_board_resumed = None;
        self.engines.set_foreground(Some(instance), keep);
        if remember_selection && self.config.engine.startup == EngineStartup::Last {
            let id = profile.id.clone();
            let _ = self.commit_config(|c| c.engine.last_engine = Some(id));
        }
        self.sync_engine();
        self.drain_engine_events();
        Ok(true)
    }

    /// "重启当前引擎". An engine that ended and was not started again is still the current engine
    /// (Java keeps `Lizzie.leelaz` and restarts `currentEngineNo`).
    pub fn restart_engine(&mut self) -> Result<bool, ApiError> {
        if self.fox_owns_board() || self.fox_holds_engine() { return Err(ApiError::new(ErrorKind::Unavailable, "Fox.conflict")); }
        if let Some(key) = self.foreground_lifecycle_held() { return Err(ApiError::new(ErrorKind::Unavailable, key)); }
        let fg = self.engines.foreground();
        let id = match fg {
            Some(fg) => self.engines.profile_of(fg).map(|p| p.id.clone()),
            None => self.engines.foreground_failure().map(|p| p.id.clone()),
        };
        let Some(id) = id else {
            return Ok(false);
        };
        self.quick_engine_changed();
        self.auto_analysis_abandon();
        if let Some(fg) = fg {
            self.engines.terminate(fg);
        }
        let Some(index) = self.config.engine.index_of(&id) else {
            return Ok(true);
        };
        self.switch_engine(index as u32)
    }

    /// "关闭当前引擎" / "关闭当前以外引擎" / "关闭所有引擎".
    pub fn shutdown_engines(&mut self, scope: EngineScope) -> bool {
        if self.fox_owns_board() || self.fox_holds_engine() { return false; }
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
        if fg.is_some_and(|f| targets.contains(&f)) {
            self.auto_analysis_abandon();
            self.quick_engine_changed();
        }
        for id in &targets {
            self.engines.terminate(*id);
        }
        self.drain_engine_events();
        !targets.is_empty()
    }

    /// The profile id of the secondary engine.
    fn secondary_profile(&self) -> Option<String> {
        self.engines
            .secondary()
            .and_then(|s| self.engines.profile_of(s))
            .map(|p| p.id.clone())
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
    /// stopped. Never written to disk. A pause stops a waiting analysis engine warm-up
    /// (`cancelLoadedGameQuickAnalysisForUserPause`).
    pub fn toggle_analysis(&mut self) -> Result<bool, ApiError> {
        if self.fox_owns_board() || self.fox_holds_engine() { return Err(ApiError::new(ErrorKind::Unavailable, "Fox.conflict")); }
        self.readboard_user_toggled_analysis();
        let toggled = self.toggle_pause_state();
        if self.paused {
            self.warmup_stop();
        }
        toggled
    }

    /// 综合设置 opened (`held`) or closed: Java's `togglePonder` around the modal dialog. No
    /// engine analyses while held; nothing else changes, so a game, an engine match, 自动分析 and
    /// the user's own pause are as they were once released. Returns whether it changed.
    pub fn hold_analysis(&mut self, held: bool) -> bool {
        if self.analysis_hold == held {
            return false;
        }
        self.analysis_hold = held;
        self.sync_engine();
        true
    }

    fn toggle_pause_state(&mut self) -> Result<bool, ApiError> {
        // Space during a game ends it (`togglePonderMannul` → `stopAiPlayingAndPolicy`).
        if self.game_active() {
            return Ok(self.game(crate::api::GameCommand::End)? == crate::api::CommandResult::Done);
        }
        if self.auto_analysis_toggle_pause() { return Ok(true); }
        if self.quick_toggle_pause() || self.flash_toggle_pause() || self.whole_game_toggle_pause() || self.batch_toggle_pause() {
            self.limit_stop = None;
            self.secondary_limit_stop = None;
            self.side_hold = None;
            self.sync_engine();
            return Ok(true);
        }
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
        // A position 分析 黑 / 白 holds resumes like a limit stop (`togglePonder` with
        // `underPonder` ponders).
        let running = !self.paused && self.limit_stop.is_none() && self.side_hold.is_none();
        self.paused = running;
        if !running && self.benchmark_running() {
            self.stop_benchmark();
        }
        // Resuming after a stop on the empty board searches on (`ponder` sets
        // `outOfPlayoutsLimit` after `stopByPlayouts`).
        if !self.paused && (self.limit_stop.is_some() || self.secondary_limit_stop.is_some()) && self.at_empty_board() {
            self.empty_board_resumed = Some((self.doc.id, self.cursor));
        }
        self.limit_stop = None;
        self.secondary_limit_stop = None;
        self.side_hold = None;
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
        self.side_hold = None;
        self.sync_engine();
        true
    }

    pub fn stop_raw_net(&mut self) -> bool {
        if !self.engines.stop_raw_net() { return false; }
        self.paused = false;
        self.limit_stop = None;
        self.secondary_limit_stop = None;
        self.side_hold = None;
        self.sync_engine();
        true
    }

    /// 分析 黑 / 白 (`chkAnalyzeBlack` / `chkAnalyzeWhite`, Menu.java:9754-9793): session only.
    /// When the changed side is to move now and ordinary analysis runs or waits for it, analysis
    /// starts or stops here at once (`ponder` / `nameCmdfornoponder`); otherwise the choice
    /// counts from the user's next move or browsing step.
    pub fn set_analyze_sides(&mut self, black: bool, white: bool) -> bool {
        if (black, white) == (self.analyze_black, self.analyze_white) {
            return false;
        }
        let changed = match self.doc.record.node(self.cursor).to_play {
            Stone::Black => black != self.analyze_black,
            Stone::White => white != self.analyze_white,
        };
        self.analyze_black = black;
        self.analyze_white = white;
        if changed && !self.paused && self.limit_stop.is_none() && self.side_choice_applies() {
            self.hold_for_side(self.cursor);
            self.sync_engine();
        }
        true
    }

    /// Ordinary analysis, the one 分析 黑 / 白 decides about: not a game, an engine match, AI
    /// 陪练 or any 自动分析 run (`isPlayingAgainstLeelaz`, `isAutoAna`).
    fn side_choice_applies(&self) -> bool {
        self.play_analyse().is_none()
            && self.match_analyse().is_none()
            && self.human_sl_analyse().is_none()
            && !self.auto_analysis_running()
    }

    /// Before the user's move or browsing step lands on `node`: running ordinary analysis waits
    /// there when the side to move at `node` is not analysed; a paused one stays paused.
    pub(super) fn hold_for_side(&mut self, node: NodeId) {
        let analysed = match self.doc.record.node(node).to_play {
            Stone::Black => self.analyze_black,
            Stone::White => self.analyze_white,
        };
        self.side_hold = (!analysed && !self.paused && self.side_choice_applies()).then_some((self.doc.id, node));
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

    /// The stores a 清除 menu item applies to: the secondary engine's only in 双引擎模式
    /// (`isDoubleEngineMode`), as for `clearBestMovesAfter`.
    fn cleared_stores(&mut self) -> impl Iterator<Item = &mut AnalysisStore> + '_ {
        let secondary = self.double_engine().then_some(&mut self.secondary_store);
        std::iter::once(&mut self.store).chain(secondary)
    }

    /// 清除Lizzie缓存 (全局 `clearBoardStat`, Shift+A; 此手 `clearbestmoves`): the stored
    /// results in `scope` stay shown and saved, and the next live result replaces them even
    /// when it is weaker. Nothing is sent to an engine and nothing is analysed again.
    pub fn clear_analysis_cache(&mut self, scope: AnalysisScope) {
        let cursor = self.cursor;
        for store in self.cleared_stores() {
            match scope {
                AnalysisScope::All => store.invalidate_all(),
                AnalysisScope::Current => store.invalidate(cursor),
            }
        }
    }

    /// 清除分析信息 (全局 `clearBestMovesInfomationAfter(start)`, 此手
    /// `clearBestMovesInfomation`): the analysis in `scope` is gone from the board, the tables
    /// and the saved record, then the current move is refreshed ([`Self::refresh_current_move`]).
    pub fn clear_analysis_info(&mut self, scope: AnalysisScope) {
        let cursor = self.cursor;
        for store in self.cleared_stores() {
            match scope {
                AnalysisScope::All => store.erase_all(),
                AnalysisScope::Current => store.erase(cursor),
            }
        }
        self.refresh_current_move();
    }

    /// `refreshCurrentMove` (重算当前局面): the current move's stored result gives way to the
    /// next live one, and the foreground engine, with the secondary one in 双引擎模式 (Java
    /// mirrors the commands to it), drops its search cache (`clear_cache`, only when it lists
    /// the command) and, if it is analysing, searches this move again.
    pub fn refresh_current_move(&mut self) {
        self.clear_analysis_cache(AnalysisScope::Current);
        let secondary = self.double_engine().then(|| self.engines.secondary()).flatten();
        for id in self.engines.foreground().into_iter().chain(secondary) {
            self.engines.refresh_analysis(id);
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
        if self.fox_owns_board() || self.fox_holds_engine() { return Err(ApiError::new(ErrorKind::Unavailable, "Fox.conflict")); }
        if self.whole_game_active() {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "WholeGameAnalysis.conflict.analysis"));
        }
        if !komi.is_finite() {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "komi"));
        }
        if self.match_active() { return Ok(self.match_revise_komi(komi)); }
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
    pub(super) fn apply_komi(&mut self, komi: f64) -> bool {
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
        if self.fox_owns_board() || self.fox_holds_engine() { return Ok(Some("Fox.conflict")); }
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
        if self.fox_owns_board() || self.fox_holds_engine() {
            let words: Vec<_> = command.split_whitespace().collect();
            if let ["kata-set-param", "playoutDoublingAdvantage", value] = words.as_slice() {
                if let Ok(value) = value.parse() { self.fox_set_pda(value)?; return Ok(None); }
            }
            return Ok(Some("Fox.conflict"));
        }
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

    /// 最大候选点数 as the stored candidates follow it (`tryToLimitMoves`); 0 keeps every one.
    pub(super) fn suggestion_limit(&self) -> usize {
        let limit = self.config.engine.analysis.limit_max_suggestion as usize;
        if self.config.display.no_sugg_circle { 0 } else { limit }
    }

    /// Puts committed analysis settings into effect: the engine update interval, the console's
    /// `always-gtp`, for a new variation length the previews of stored analysis, and for new
    /// limits a search a limit stopped resumes when it is within them now.
    pub(super) fn analysis_settings_changed(&mut self, previous: &AnalysisSettings) {
        let settings = &self.config.engine.analysis;
        self.engines.set_interval(settings.interval_centisec, settings.interval_centisec_ssh);
        self.engines.set_always_gtp(settings.always_gtp);
        let limits = |a: &AnalysisSettings| (a.limit_time, a.max_analyze_time_seconds, a.limit_playout, a.limit_playouts);
        let limits_changed = limits(settings) != limits(previous);
        if settings.limit_branch_length != previous.limit_branch_length {
            self.refresh_pv_captures();
        }
        if limits_changed {
            self.resume_within_limits();
        }
    }

    /// `Menu.reCalculateLeelazPonderingIfOutOfLimit`: the foreground search a time or visit
    /// limit stopped goes on when its visits and the time since it began are within the limits
    /// now. A stop by 空棋盘停止计算 is not a limit's and stays.
    fn resume_within_limits(&mut self) {
        let here = (self.doc.id, self.cursor);
        if self.limit_stop != Some(here) {
            return;
        }
        let Some((visits, began)) = self.limit_search else { return };
        if self.limit_reached(visits, began.elapsed()) {
            return;
        }
        self.limit_stop = None;
        self.limit_search = None;
        self.sync_engine();
    }

    /// The GTP console was shown or hidden (`always-gtp` decides what a hidden one records).
    pub fn set_console_visible(&mut self, visible: bool) {
        self.engines.set_console_visible(visible);
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
        engines: Vec<EngineProfile>,
        startup: EngineStartup,
    ) -> Result<bool, ApiError> {
        self.save_engine_settings_with_cloud(engines, startup, None)
    }

    pub(super) fn save_engine_settings_with_cloud(
        &mut self,
        mut engines: Vec<EngineProfile>,
        startup: EngineStartup,
        cloud: Option<crate::cloud::CloudSettings>,
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
        if old == engines && self.config.engine.startup == startup && cloud.is_none() {
            return Ok(false);
        }
        let list = engines.clone();
        self.commit_config(|c| {
            c.engine.engines = list;
            c.engine.startup = startup;
            if let Some(cloud) = cloud {
                c.cloud = cloud;
                c.engine.last_engine = c.engine.engines.iter().find(|p| p.is_default).map(|p| p.id.clone());
            }
        })?;
        let fg_profile = self
            .engines
            .foreground()
            .and_then(|f| self.engines.profile_of(f))
            .map(|p| p.id.clone());
        let secondary_profile = self.secondary_profile();
        let (mut restart_fg, mut restart_secondary) = (None, None);
        for (instance, id, _) in self.engines.running() {
            // `EngineManager.isSameEngine` also compares the SSH connection.
            let launch = |p: &EngineProfile| (p.command.clone(), p.initial_command.clone(), p.width, p.height, p.ssh.clone(), p.cloud.clone());
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

    /// Engine starts whose model header is read for the title's B11 speed notice, each handed
    /// out once. The platform reads them off the workbench and reports back through
    /// [`Self::finish_model_lookup`].
    pub fn take_model_lookups(&mut self) -> Vec<ModelLookup> {
        self.engines.take_model_lookups()
    }

    /// Records the model a lookup found, unless its engine was retired meanwhile. Returns
    /// whether the view changed.
    pub fn finish_model_lookup(&mut self, found: ModelIdentity) -> bool {
        self.engines.finish_model_lookup(found)
    }

    /// When timed work is due next: engine protocol timers and quick analysis (see
    /// [`Self::engine_tick`]).
    pub fn engine_timer(&self) -> Option<std::time::Instant> {
        [
            self.engines.next_tick(),
            self.fox_deadline(),
            self.quick_deadline(),
            self.whole_game_deadline(),
            self.tracking_deadline(),
            self.play_deadline(),
            self.match_deadline(),
            self.human_sl_deadline(),
            self.strength_deadline(),
            self.contribution_deadline(),
            self.readboard_deadline(),
            self.web_trial_deadline(),
            self.online_deadline(),
        ]
            .into_iter()
            .flatten()
            .min()
    }

    /// Advances engine protocol timers and quick analysis; the platform calls it at
    /// [`Self::engine_timer`]. Returns whether the view changed.
    pub fn engine_tick(&mut self, now: std::time::Instant) -> bool {
        let events = self.engines.tick(now);
        let engines = self.engine_events(events);
        let quick = self.quick_tick(now);
        let whole = self.whole_game_tick(now);
        let tracking = self.tracking_tick(now);
        let play = self.play_tick(now);
        let matches = self.match_tick(now);
        let human_sl = self.human_sl_tick(now);
        let strength = self.strength_tick(now);
        let contribution = self.contribution_tick(now);
        let readboard = self.readboard_tick(now);
        let web_trial = self.web_trial_tick(now);
        let fox = self.fox_tick(now);
        let online = self.online_tick(now);
        engines | quick | whole | tracking | play | matches | human_sl | strength | contribution | readboard | web_trial | fox | online
    }

    /// Handles runtime events produced by operations outside engine output.
    pub(super) fn drain_engine_events(&mut self) -> bool {
        let events = self.engines.take_events();
        !events.is_empty() && self.engine_events(events)
    }

    pub(super) fn engine_events(&mut self, events: Vec<RuntimeEvent>) -> bool {
        let mut changed = false;
        for ev in events {
            changed = true;
            let Some(ev) = self
                .search_threads_event(ev)
                .and_then(|ev| self.contribution_event(ev))
                .and_then(|ev| self.gma_event(ev))
                .and_then(|ev| self.fox_event(ev))
                .and_then(|ev| self.match_event(ev))
                .and_then(|ev| self.play_event(ev))
                .and_then(|ev| self.human_sl_event(ev))
                .and_then(|ev| self.strength_event(ev))
                .and_then(|ev| self.quick_event(ev))
                .and_then(|ev| self.flash_event(ev))
                .and_then(|ev| self.whole_game_event(ev))
                .and_then(|ev| self.batch_event(ev))
            else {
                continue;
            };
            match ev {
                RuntimeEvent::Ready { instance } => {
                    self.restart_ready(instance);
                    if let Some((name, version, _)) = self.engines.identity(instance) {
                        self.log(false, format!("engine ready: {name} {version}"));
                    }
                    if let Some((_, komi)) = self.start_komi.take_if(|(i, _)| *i == instance) {
                        if self.game_is_empty() {
                            self.apply_komi(komi);
                        }
                    }
                    self.readboard_engine_ready(instance);
                }
                RuntimeEvent::Rules { .. } => {}
                RuntimeEvent::RawNet { .. } => {}
                RuntimeEvent::Pondering { .. } => self.foreground_pondered(),
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
                    if self.restart_on_failure(&f) {
                        self.engine_failed(f);
                    }
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
                    // 批量分析's record has its own store and run.
                    if !secondary && Some(doc) == self.batch_doc_id() {
                        let progress = line.candidates.iter().map(|c| c.visits).sum();
                        if self.batch_foreground_result(doc, node, stamp, komi, to_play, engine_name, line, elapsed) {
                            self.restart_proven(instance);
                            self.engines.analysis_progress_accepted(instance, progress);
                        }
                        continue;
                    }
                    if doc != self.doc.id
                        || komi != self.komi
                        || doc != self.store.doc
                        || rules_rev != self.rules_revision
                    {
                        continue; // another document, a komi or rules that no longer apply
                    }
                    if !self.doc.record.contains(node) || self.doc.record.node(node).stamp != stamp {
                        continue; // the node was removed or edited since the request
                    }
                    let Some((result, ownership)) = node_analysis(node, to_play, komi, engine_name, line) else {
                        continue;
                    };
                    self.restart_proven(instance);
                    let visits = result.visits;
                    self.engines.analysis_progress_accepted(instance, visits);
                    let played = (!secondary && self.play_wants_analysis(node)).then(|| result.clone());
                    let matched = (!secondary && self.match_wants_analysis(node)).then(|| result.clone());
                    let first_visits = result.candidates.first().map_or(0, |c| c.visits);
                    // The foreground engine's line is the current search: 选点重点研究 counts its
                    // candidates, and the tracked position shows its result.
                    let tracked = !secondary
                        && self.tracking_line(
                            instance,
                            (doc, node, stamp),
                            result.candidates.iter().filter_map(|c| Some((c.point?, c.visits))),
                        );
                    // Neither 自动分析 nor the foreground engine in an engine game keeps a stored
                    // result against the current search (`BoardData`'s `protectedCache`).
                    let automatic = !secondary && self.auto_analysis_active();
                    let in_game = !secondary && self.match_active();
                    let shown = self.suggestion_limit();
                    // Attended points belong to the main board's candidates only (`isMain`).
                    let tracked_points = if secondary { Vec::new() } else { self.tracked_points(node).to_vec() };
                    let pda = self.engine_pda(instance);
                    let rec = &self.doc.record;
                    let cache = self.config.engine.analysis.enable_lizzie_cache;
                    let store = if secondary {
                        &mut self.secondary_store
                    } else {
                        &mut self.store
                    };
                    let replace = automatic || in_game || tracked || store.takes_place(rec, node, &result, cache, pda);
                    if replace {
                        let mut result = result;
                        let limit = self.config.engine.analysis.limit_branch_length;
                        fill_pv_captures(&mut result, &rec.position(node), limit, !self.config.edit.no_capture);
                        store.update(node, stamp, result, ownership, shown, &tracked_points);
                        store.made_with_pda(node, pda);
                    }
                    let running = node == self.cursor
                        && !self.paused
                        && self.limit_stop.is_none()
                        && !self.engines.raw_net_active();
                    if running && !secondary {
                        if automatic { self.auto_analysis_result(node, visits, first_visits, elapsed); }
                        else { self.check_limits(visits, elapsed); }
                    } else if running && self.secondary_limit_stop.is_none() {
                        self.check_secondary_limits(visits, elapsed);
                    }
                    if let Some(result) = played {
                        self.play_analysis(&result, elapsed);
                    }
                    if let Some(result) = matched { self.match_analysis(&result, elapsed); }
                }
                _ => {}
            }
        }
        self.auto_analysis_poll();
        // A foreground engine that ended starts again once the other consequences of its end
        // (a game retired, a lease ended) are applied.
        self.restart_due();
        let changed = self.restart_settle() || changed;
        // A 搜索线程数 step whose process ended, or a restore whose new process became ready.
        let changed = self.search_threads_settle() || changed;
        // A settling GMA hand ends once its engine holds the record's position; the reply that
        // completes that sync need not be an event.
        let changed = self.gma_settle() || changed;
        if changed {
            // Readiness, rules answers and failures can change what the engine should do.
            self.sync_engine();
            self.cloud_activation_settle();
            // GMA's notices, and an auto-play line waiting for GMA to give the engine back.
            self.readboard_after_engine();
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

    /// The current position is the empty board: the root with no setup stones
    /// (`BoardHistoryList.noStoneBoard`).
    fn at_empty_board(&self) -> bool {
        self.cursor == ROOT && self.doc.record.position(ROOT).stones.iter().all(Option::is_none)
    }

    /// 空棋盘停止计算 (`Leelaz.java:7455-7468`): an ordinary analysis of the empty board stops at
    /// its result, once per position until the user resumes it. Like the limits it belongs to,
    /// it never applies to a game, an engine match or 自动分析, whose results do not get here.
    fn stops_at_empty_board(&self) -> bool {
        self.config.engine.analysis.stop_at_empty_board
            && self.empty_board_resumed != Some((self.doc.id, self.cursor))
            && self.at_empty_board()
    }

    /// The foreground engine's limit stops the analysis of this node, the secondary engine's
    /// too (it resumes when the position changes); the notice is shown once per engine start,
    /// never for 空棋盘停止计算.
    fn check_limits(&mut self, visits: u64, elapsed: std::time::Duration) {
        let empty = self.stops_at_empty_board();
        if !empty && !self.limit_reached(visits, elapsed) {
            return;
        }
        self.limit_stop = Some((self.doc.id, self.cursor));
        self.limit_search = if empty { None } else { Instant::now().checked_sub(elapsed).map(|began| (visits, began)) };
        if !empty && self.config.engine.analysis.show_ponder_limited_tips && !self.limit_tip_shown {
            self.limit_tip_shown = true;
            self.engine_notice = Some(EngineNotice::StoppedByLimit);
        }
        self.sync_engine();
    }

    /// The secondary engine's own limit stops only its analysis of this node (Java `leelaz2`
    /// stops itself; the stop is not mirrored back), with the notice once per choice.
    fn check_secondary_limits(&mut self, visits: u64, elapsed: std::time::Duration) {
        let empty = self.stops_at_empty_board();
        if !empty && !self.limit_reached(visits, elapsed) {
            return;
        }
        self.secondary_limit_stop = Some((self.doc.id, self.cursor));
        if !empty && self.config.engine.analysis.show_ponder_limited_tips && !self.secondary_limit_tip_shown {
            self.secondary_limit_tip_shown = true;
            self.engine_notice = Some(EngineNotice::StoppedByLimit);
        }
        self.sync_engine();
    }

    /// A failed engine: logged, and reported with the launch command, message and output passed
    /// through the credential sanitizer like Java's failure dialog and copy (`redactSensitiveText`).
    pub(super) fn engine_failed(&mut self, f: EngineFailure) {
        self.log_engine_failure(&f);
        let remote = f.remote.map(|r| match r {
            RemoteFailure::Connect { detail } => RemoteFailure::Connect { detail: sanitize(&detail) },
            RemoteFailure::Lost { detail } => RemoteFailure::Lost { detail: sanitize(&detail) },
            other => other,
        });
        self.engine_failure = Some(EngineFailureView {
            engine_name: f.engine_name,
            command: sanitize(&f.command),
            during_startup: f.during_startup,
            message: sanitize(&f.message),
            output: f.output.iter().map(|l| sanitize(l)).collect(),
            repair: None,
            remote,
            profile_id: f.profile_id,
        });
    }

    /// The failure's log line: command, message and the last output lines, sanitized.
    pub(super) fn log_engine_failure(&mut self, f: &EngineFailure) {
        let tail = f.output.iter().rev().take(5).rev().map(|l| sanitize(l)).collect::<Vec<_>>().join(" / ");
        self.log(
            true,
            format!("engine failed [{}] {}: {} | {tail}", f.engine_name, sanitize(&f.command), sanitize(&f.message)),
        );
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
                leela_sai: self.engines.takes_lz_options(f),
                can_analyse: self.engines.can_analyse(f),
                ready: self.engines.foreground_complete(),
                commands: self.engines.known_commands(f),
                b11: self.engines.uses_b11(f),
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
        let (lease, unrestored) = self.lease_view();
        EngineView {
            engines,
            foreground,
            secondary,
            paused: self.paused,
            analysing: self.engines.analysing(self.doc.id, self.cursor),
            stopped_by_limit: self.limit_stop == Some((self.doc.id, self.cursor)),
            analyze_black: self.analyze_black,
            analyze_white: self.analyze_white,
            rules_key: rules.map(|r| r.summary_key().to_string()),
            rules: rules.map(KataRules::to_gtp_argument),
            console_seq: self.engines.console_seq(),
            lease,
            unrestored,
            down: fg.is_none() && self.engines.foreground_failure().is_some(),
            kata_params: self.kata_params_view(),
            search_threads: self.search_threads_view(),
            search_threads_operation: self.search_threads_operation_view(),
            custom_pda: self.custom_pda_view(),
            priority_points: if self.priority_engine == self.engines.foreground()
                && self.priority_at == Some((self.doc.id, self.cursor))
            {
                self.priority_points.clone()
            } else {
                Vec::new()
            },
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
            kata_pda: s.kata_pda,
            kata_pda_enabled: s.kata_pda_enabled,
            kata_params: s.kata_params.clone(),
            leela_options: s.leela_options.clone(),
            custom_pda: s.custom_pda.clone(),
        }
    }

    /// The display settings' conversion of the engine's numbers.
    fn viewpoint(&self) -> Viewpoint {
        Viewpoint::from(&self.config.display)
    }

    pub(super) fn current_analysis(&self) -> Option<NodeAnalysisView> {
        if self.fox_owns_board() { return self.fox_display_analysis(); }
        if self.match_active() && self.config.matches.show_previous_bestmoves {
            let node = self.doc.record.parent(self.cursor)?;
            let mut result = self.shown_analysis_at(&self.store, node)?;
            if self.config.matches.previous_bestmove_only {
                let played = match self.doc.record.node(self.cursor).kind {
                    NodeKind::Move { point, .. } => Some(point),
                    _ => None,
                };
                if let Some(index) = played.and_then(|point| result.candidates.iter().position(|candidate| candidate.point == Some(point))) {
                    result.candidates.swap(0, index);
                    result.candidates.truncate(1);
                } else {
                    result.candidates.clear();
                }
            }
            return Some(result);
        }
        self.shown_analysis(&self.store)
    }

    /// `store`'s result for the current node as the displays show it.
    fn shown_analysis(&self, store: &AnalysisStore) -> Option<NodeAnalysisView> {
        self.shown_analysis_at(store, self.cursor)
    }

    pub(super) fn shown_analysis_at(&self, store: &AnalysisStore, node: NodeId) -> Option<NodeAnalysisView> {
        let s = store.get(&self.doc.record, node)?;
        Some(self.shown_analysis_value(s))
    }

    pub(super) fn shown_analysis_value(&self, s: &NodeAnalysisView) -> NodeAnalysisView {
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
                kept: c.kept,
            })
            .collect();
        NodeAnalysisView {
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
        }
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
        self.points_of(store, self.graph_nodes())
    }

    /// `store`'s results of `nodes`, in that order, as the graphs draw them.
    pub(super) fn points_of(&self, store: &AnalysisStore, nodes: Vec<NodeId>) -> Vec<HistoryPoint> {
        let rec = &self.doc.record;
        let vp = self.viewpoint();
        nodes
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
        self.overlay_at(store, self.cursor)
    }

    /// The board marks of `node`'s position (the ReadBoard float board shows the main line's end).
    pub(super) fn overlay_at(&self, store: &AnalysisStore, node: NodeId) -> BoardOverlayView {
        let rec = &self.doc.record;
        let s = &self.config.display;
        let path = rec.path_to(node);
        let numbering = display::numbering(rec, &path);
        let analysis = |n: NodeId| store.get(rec, n);
        let rank_override = self.whole_game_results_for_current()
            && std::ptr::eq(store, &self.store)
            && rec.node(node).parent.is_some_and(|n| store.get(rec, n).is_some_and(|a| a.visits > 0));
        let ranks = if rank_override {
            let mut effective = s.clone();
            effective.move_numbers = 0;
            if effective.move_rank < 0 { effective.move_rank = 1; }
            display::rank_marks(rec, &path, &numbering, &effective, analysis)
        } else {
            display::rank_marks(rec, &path, &numbering, s, analysis)
        };
        BoardOverlayView {
            move_numbers: display::number_labels(rec, &path, &numbering, s, self.trial_start()),
            ranks,
            rank_corner: display::shows_ranks(s) && (s.move_rank == 0 || s.move_rank > 1),
            ranks_after_numbers: rank_override,
            next_move: display::next_move_info(rec, node, s, self.config.edit.no_capture,
                self.config.engine.analysis.limit_branch_length, analysis),
            info_order_sample: display::info_order_sample(s, rec.node(node).to_play, self.komi),
        }
    }

    /// 点击棋子回看 (`Board.setPressStoneInfo`, Board.java:7338-7354): back from the current node,
    /// the latest move played at `point`; the review shows its parent (`mouseOnNode`), that
    /// position's stored analysis and its 落子评价 marks (`BoardRenderer.java:313-326`).
    pub fn click_review(&self, point: Point) -> Option<ClickReviewView> {
        let rec = &self.doc.record;
        let mut node = self.cursor;
        let shown = loop {
            let parent = rec.parent(node)?;
            if matches!(rec.node(node).kind, NodeKind::Move { point: p, .. } if p == point) {
                break parent;
            }
            node = parent;
        };
        let data = rec.node(shown);
        // effectiveMoveRankMarkLimit decides between drawMoveRankMark and drawLastMoveMark; the
        // move numbers setting plays no part there.
        let mut s = self.config.display.clone();
        s.move_numbers = 0;
        let rank_override = self.whole_game_results_for_current()
            && data.parent.is_some_and(|n| self.store.get(rec, n).is_some_and(|a| a.visits > 0));
        if rank_override && s.move_rank < 0 {
            s.move_rank = 1;
        }
        let ranks = (s.move_rank >= 0).then(|| {
            let path = rec.path_to(shown);
            let numbering = display::numbering(rec, &path);
            display::rank_marks(rec, &path, &numbering, &s, |n| self.store.get(rec, n))
        });
        Some(ClickReviewView {
            document: self.doc.id,
            revision: self.doc.revision,
            from: self.cursor,
            node: shown,
            stones: rec.position(shown).stones,
            last_action: super::last_action(data.kind),
            snapshot: data.kind == NodeKind::Snapshot,
            to_play: data.to_play,
            analysis: self.shown_analysis_at(&self.store, shown),
            ranks,
        })
    }
}
