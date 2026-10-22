//! 自动棋谱快析 (Java `AutomaticQuickAnalysisTask`, `AutomaticQuickAnalysisEngineAdapter`): after
//! a record is opened, its main-trunk moves without analysis are analysed in the background, by a
//! KataGo analysis process or by borrowing the foreground engine (the foreground lease).
//!
//! A task belongs to one document. It makes attempts: an attempt asks for every missing move and
//! ends when the answers are in (completed), when something fails (the task retries later with a
//! growing delay; the foreground engine may analyse meanwhile) or when the task is cancelled. The
//! foreground engine stays idle while a task is active, and resumes when the task ends unless the
//! user paused analysis or the borrowed engine could not be given back.
//!
//! The user's pause cancels the task of that document for good: resuming analysis does not bring
//! it back; opening a record starts afresh. Results go to the node they were asked for, in the
//! document they were asked for, as the foreground engine's results do; they never mark the
//! document changed.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use super::analysis::{fill_pv_captures, more_visits, node_analysis};
use super::resources::QUICK_MODEL;
use super::Workbench;
use crate::api::{
    ApiError, LeaseAvailabilityView, QuickAnalysisView, QuickModelStatus, QuickModelView, UnrestoredReason,
};
use crate::engine::analysis_command::{self, recommended_threads};
use crate::engine::gtp::{join_command, prepare_launch, InfoLine};
use crate::engine::kata_analysis::{self, QueryOptions, Response};
use crate::engine::runtime::{InstanceId, LeaseAvailability, LeaseFailure, Phase, RulesOutcome, RuntimeEvent};
use crate::formats::ReadError;
use crate::record::{GameRecord, NodeId, RecordFormat, Stone, ROOT};
use crate::resources as r;
use crate::settings::{AnalysisEngineSettings, PvVisitsMode};

/// Java `RETRY_MILLIS`, `MAX_RETRY_MILLIS`, `WATCHDOG_MILLIS`, `NAVIGATION_MILLIS`.
const RETRY: Duration = Duration::from_millis(1800);
const MAX_RETRY: Duration = Duration::from_millis(30_000);
const WATCHDOG: Duration = Duration::from_millis(30_000);
const NAVIGATION: Duration = Duration::from_millis(700);
/// Visits a borrowed engine searches per position (Java `FLASH_ANALYSIS_MIN_VISITS`).
const LEASE_VISITS: u64 = 16;
/// A borrowed engine first analyses an overview of longer games (first, last, every 8th move).
const OVERVIEW_MIN: usize = 24;
const OVERVIEW_STEP: usize = 8;
/// Managed Windows NVIDIA engines whose GPU an analysis process would compete for
/// (`isBundledNvidiaCommand`, `isBundledTensorRtCommand`).
const NVIDIA_ASSETS: [&str; 2] = ["windows-nvidia", "windows-tensorrt"];
const TENSORRT_ASSET: &str = "windows-tensorrt";

/// An open whose record waits until the borrowed engine is given back.
pub(super) type DeferredOpen = (u64, Result<(RecordFormat, GameRecord), ReadError>);

#[derive(Default)]
pub(super) struct QuickFlow {
    task: Option<Task>,
    next_task: u64,
    /// The document whose quick analysis the user's pause cancelled (`userCancelledQuickAnalysisRoot`).
    cancelled_doc: Option<u64>,
    /// A cancelled task's borrowed engine still being given back.
    releasing: Option<u64>,
    deferred_open: Option<DeferredOpen>,
    /// A task starts here after browsing, when the open record still lacks analysis.
    start_at: Option<Instant>,
    /// A preloaded analysis process and its command (`analysis-engine-preload`).
    warm: Option<(InstanceId, String)>,
    /// Lightweight model files checked against the catalog digest, by path
    /// (`isValidQuickAnalysisModelFile`); a changed size or time is checked again.
    verified: Mutex<HashMap<PathBuf, Verified>>,
}

struct Verified {
    size: u64,
    modified: Option<SystemTime>,
    sha256: String,
    valid: bool,
}

/// What a request's rules depend on besides the record (`currentAnalysisRulesSignature`).
pub(super) type RulesSignature = (bool, String, bool, String);

struct Task {
    doc: u64,
    failures: u32,
    retry_at: Option<Instant>,
    navigation_until: Option<Instant>,
    watchdog: Option<Instant>,
    attempt: Option<Attempt>,
    /// A failed attempt let the foreground engine analyse until the next attempt.
    foreground_resumed: bool,
    borrowed: bool,
    /// The rules settings the current attempt asks under; answers under others are dropped.
    rules: RulesSignature,
}

enum Attempt {
    Process {
        instance: InstanceId,
        /// The command it was started with: a hand-over keeps the process for 闪电分析.
        command: String,
        queries: HashMap<String, Query>,
        failed: bool,
    },
    Lease {
        lease: u64,
        queue: VecDeque<NodeId>,
        /// Giving the engine back after the attempt ended (failed or not).
        closing: Option<bool>,
    },
}

/// What a dedicated query, or a borrowed engine's search, was asked for.
pub(super) struct Query {
    pub(super) node: NodeId,
    pub(super) stamp: u64,
    pub(super) komi: f64,
    pub(super) rules_rev: u64,
    pub(super) to_play: Stone,
}

enum Readiness {
    Ready,
    Wait,
    Stop,
}

/// `major.minor` of a version text like `1.16.3` or `v1.17.0`.
fn version_at_least(version: &str, major: u32, minor: u32) -> Option<bool> {
    let digits = version.trim().trim_start_matches(|c: char| !c.is_ascii_digit());
    let mut parts = digits.split(|c: char| !c.is_ascii_digit());
    let a: u32 = parts.next()?.parse().ok()?;
    let b: u32 = parts.next()?.parse().ok()?;
    Some((a, b) >= (major, minor))
}

/// The order a borrowed engine searches in: the overview first for longer games, then the rest.
fn lease_order(nodes: Vec<NodeId>) -> VecDeque<NodeId> {
    if nodes.len() < OVERVIEW_MIN {
        return nodes.into();
    }
    let last = nodes.len() - 1;
    let overview: Vec<usize> = [0, last]
        .into_iter()
        .chain((OVERVIEW_STEP..last).step_by(OVERVIEW_STEP))
        .collect();
    let mut order: VecDeque<NodeId> = overview.iter().map(|&i| nodes[i]).collect();
    order.extend(
        nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| !overview.contains(i))
            .map(|(_, &n)| n),
    );
    order
}

fn new_task(doc: u64) -> Task {
    Task {
        doc,
        failures: 0,
        retry_at: None,
        navigation_until: None,
        watchdog: None,
        attempt: None,
        foreground_resumed: false,
        borrowed: false,
        rules: Default::default(),
    }
}

impl Workbench {
    // -------------------------------------------------------------------- state

    /// The foreground engine leaves the position to a running quick analysis.
    pub(super) fn quick_holds_foreground(&self) -> bool {
        self.quick.task.as_ref().is_some_and(|t| !t.foreground_resumed)
    }

    /// Main-trunk moves and passes (not placeholders) without analysis (`hasMissingMainlineAnalysis`).
    fn quick_missing(&self) -> Vec<NodeId> {
        let rec = &self.doc.record;
        rec.path_to(rec.line_end(ROOT))
            .into_iter()
            .filter(|&n| rec.node(n).kind.is_action())
            .filter(|&n| self.store.get(rec, n).is_none_or(|a| a.visits == 0))
            .collect()
    }

    /// `canContinueQuickAnalysisAfterHistoryNavigation`, the user's cancellation, and no manual
    /// flash analysis using the analysis engine.
    fn quick_allowed(&self) -> bool {
        self.config.record.auto_quick_analyze_on_load
            && !self.paused
            && self.quick.cancelled_doc != Some(self.doc.id)
            && !self.flash_active()
            && !self.auto_analysis_active()
            && !self.whole_game_active()
    }

    fn quick_readiness(&self, borrow: bool) -> Readiness {
        if !self.quick_allowed() {
            return Readiness::Stop;
        }
        // Which path applies depends on the foreground engine: wait while it starts.
        let foreground = self.engines.foreground();
        if foreground.is_some_and(|f| self.engines.phase(f) == Some(Phase::Starting)) {
            return Readiness::Wait;
        }
        // The record's declared rules reach the engine first (`kifuEngineSyncPending`).
        if let (Some(f), Some(rules)) = (foreground, self.target_rules()) {
            if self.engines.phase(f) == Some(Phase::Ready)
                && self.engines.rules_outcome(f, rules) == RulesOutcome::Pending
            {
                return Readiness::Wait;
            }
        }
        if !borrow {
            return Readiness::Ready;
        }
        match self.engines.lease_availability() {
            LeaseAvailability::Available => Readiness::Ready,
            LeaseAvailability::EngineNotReady | LeaseAvailability::ExistingLease => Readiness::Wait,
            _ => Readiness::Stop,
        }
    }

    // -------------------------------------------------------------------- engine choice

    /// The installed 快速曲线轻量模型: the configured file, else the one in the models folder,
    /// with the catalog's name, size and digest.
    pub(super) fn quick_model_path(&self) -> Option<PathBuf> {
        let model = self.resources.catalog().models.get(QUICK_MODEL)?;
        let installed = self.resources.store().quick_models_dir().join(&model.file_name);
        self.config
            .quick_model
            .path
            .as_ref()
            .map(PathBuf::from)
            .into_iter()
            .chain(std::iter::once(installed))
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&model.file_name))
                    && self.quick_model_valid(p, model.size_bytes, &model.sha256)
            })
    }

    /// Whether `path` is the catalog's file; its digest is computed once per size and time.
    fn quick_model_valid(&self, path: &Path, size: u64, sha256: &str) -> bool {
        let Ok(meta) = std::fs::metadata(path) else {
            return false;
        };
        if !meta.is_file() || meta.len() != size {
            return false;
        }
        let modified = meta.modified().ok();
        let mut verified = self.quick.verified.lock().unwrap();
        if let Some(v) = verified
            .get(path)
            .filter(|v| v.size == size && v.modified == modified && v.sha256 == sha256)
        {
            return v.valid;
        }
        let valid = r::sha256_file(path).is_ok_and(|d| d.eq_ignore_ascii_case(sha256));
        verified.insert(
            path.to_path_buf(),
            Verified {
                size,
                modified,
                sha256: sha256.to_string(),
                valid,
            },
        );
        valid
    }

    /// A download verified the lightweight model at `path` against the catalog.
    pub(super) fn quick_model_downloaded(&self, path: &Path) {
        let (Some(model), Ok(meta)) = (
            self.resources.catalog().models.get(QUICK_MODEL),
            std::fs::metadata(path),
        ) else {
            return;
        };
        self.quick.verified.lock().unwrap().insert(
            path.to_path_buf(),
            Verified {
                size: meta.len(),
                modified: meta.modified().ok(),
                sha256: model.sha256.clone(),
                valid: true,
            },
        );
    }

    /// Whether the foreground engine is KataGo 1.17 or later; `None` when unknown.
    fn foreground_katago_version_ok(&self) -> Option<bool> {
        let fg = self.engines.foreground()?;
        if !self.engines.is_katago(fg) {
            return Some(false);
        }
        let (_, version, _) = self.engines.identity(fg)?;
        version_at_least(&version, 1, 17)
    }

    /// The local KataGo and the model a lightweight quick analysis runs, when it is wanted and
    /// possible (`resolveQuickAnalysisEngineCommand`).
    fn quick_light_parts(&self) -> Option<(String, PathBuf)> {
        if !self.config.quick_model.enabled || self.foreground_katago_version_ok() != Some(true) {
            return None;
        }
        let model = self.quick_model_path()?;
        let profile = self.engines.profile_of(self.engines.foreground()?)?;
        let exe = prepare_launch(&profile.command, self.engines.roots())?.program;
        Some((exe, model))
    }

    /// Borrow the foreground engine instead of starting an analysis process
    /// (`analysis-reuse-current-engine`, `automaticallyReusesPrimaryForeground`).
    fn quick_borrows(&self) -> bool {
        if self.config.analysis_engine.reuse_current_engine {
            return true;
        }
        let Some(marker) = self
            .engines
            .foreground()
            .and_then(|f| self.engines.profile_of(f))
            .filter(|p| r::is_managed(p))
            .and_then(|p| p.managed.as_ref())
        else {
            return false;
        };
        NVIDIA_ASSETS.contains(&marker.asset_id.as_str())
            && (marker.asset_id == TENSORRT_ASSET || self.quick_light_parts().is_none())
    }

    /// 自动生成 / 选择引擎: the analysis command made from engine `index` (the foreground engine,
    /// else the default one), creating the `analysis.cfg` it names when missing. Returns the
    /// command and the created file.
    pub fn generate_analysis_command(&self, index: Option<u32>) -> Result<(String, Option<String>), &'static str> {
        let engines = &self.config.engine;
        let profile = match index {
            Some(i) => engines
                .engines
                .get(i as usize)
                .ok_or("AnalysisEngineCommandHelper.noEngineSelected")?,
            None => self
                .engines
                .foreground()
                .and_then(|f| self.engines.profile_of(f))
                .or_else(|| engines.default_index().and_then(|i| engines.engines.get(i)))
                .ok_or("AnalysisEngineCommandHelper.noDefaultEngine")?,
        };
        let generated = analysis_command::from_engine_command(&profile.command, self.engines.roots())?;
        let created = match generated.config {
            Some(path) if !path.exists() => {
                r::create_file_once(&path, r::ANALYSIS_EXAMPLE)
                    .map_err(|_| "AnalysisEngineCommandHelper.generateConfigFailed")?;
                Some(path.to_string_lossy().into_owned())
            }
            _ => None,
        };
        Ok((generated.command, created))
    }

    /// The analysis process command for this launch, tuned for the request.
    fn quick_command(&self) -> String {
        self.tuned_analysis_command(&self.analysis_command_base(true))
    }

    /// The analysis engine command before tuning: the lightweight model (when `light` and it is
    /// wanted), the customized command, or one made from the current engine (else the saved one).
    pub(super) fn analysis_command_base(&self, light: bool) -> String {
        let a = &self.config.analysis_engine;
        let light = light
            .then(|| self.quick_light_parts())
            .flatten()
            .and_then(|(exe, model)| {
                let config = self.resources.store().analysis_config();
                if !config.is_file() {
                    r::create_file_once(&config, r::ANALYSIS_EXAMPLE).ok()?;
                }
                join_command(&[
                    exe,
                    "analysis".into(),
                    "-model".into(),
                    model.to_string_lossy().into_owned(),
                    "-config".into(),
                    config.to_string_lossy().into_owned(),
                    "-quit-without-waiting".into(),
                ])
            });
        match light {
            Some(command) => command,
            None if a.command_customized => a.command.clone(),
            None => self
                .generate_analysis_command(None)
                .map_or_else(|_| a.command.clone(), |(c, _)| c),
        }
    }

    /// `optimizeAnalysisEngineCommand`: one more visit than 单步计算量, the PV length and threads.
    pub(super) fn tuned_analysis_command(&self, base: &str) -> String {
        let a = &self.config.analysis_engine;
        // Apple Silicon runs this application's KataGo with its own thread profile.
        let apple = (cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
            .then(|| prepare_launch(base, self.engines.roots()))
            .flatten()
            .filter(|spec| PathBuf::from(&spec.program).starts_with(self.resources.store().root()))
            .map(|_| recommended_threads(std::thread::available_parallelism().map_or(1, |n| n.get() as u32)));
        analysis_command::optimize(
            base,
            a.max_visits.max(1) + 1,
            self.config.engine.analysis.limit_branch_length,
            apple,
        )
    }

    /// `resolveLocalAnalysisRules`.
    pub(super) fn quick_rules(&self) -> serde_json::Value {
        let a = &self.config.analysis_engine;
        let parse = |text: &str| {
            serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .filter(|v| v.is_object())
        };
        let rules = if !a.use_current_rules {
            parse(&a.specific_rules)
        } else {
            self.engines
                .foreground()
                .and_then(|f| self.engines.confirmed_rules(f))
                .and_then(|r| parse(&r.to_gtp_argument()))
                .or_else(|| {
                    let e = &self.config.engine;
                    (e.auto_load_kata_rules && !e.kata_rules.trim().is_empty())
                        .then(|| parse(&e.kata_rules))
                        .flatten()
                })
        };
        rules.unwrap_or_else(|| serde_json::json!("tromp-taylor"))
    }

    /// The rules a borrowed engine searches with (`resolveRemoteGtpRules`): 使用指定规则, else
    /// the record's session rules.
    pub(super) fn quick_lease_rules(&self) -> Option<String> {
        let a = &self.config.analysis_engine;
        if a.use_current_rules {
            return self.target_rules().map(|r| r.to_gtp_argument());
        }
        let rules = a.specific_rules.trim();
        Some(if rules.is_empty() { "tromp-taylor" } else { rules }.to_string())
    }

    pub(super) fn quick_rules_signature(&self) -> RulesSignature {
        let (a, e) = (&self.config.analysis_engine, &self.config.engine);
        (
            a.use_current_rules,
            a.specific_rules.clone(),
            e.auto_load_kata_rules,
            e.kata_rules.clone(),
        )
    }

    // -------------------------------------------------------------------- lifecycle

    /// Starts a task for the open record when the settings allow and a main-trunk move lacks
    /// analysis.
    fn quick_start(&mut self) {
        if self.quick.task.is_none() && self.quick_allowed() && !self.quick_missing().is_empty() {
            self.quick.next_task += 1;
            self.quick.task = Some(new_task(self.doc.id));
            self.quick_progress(Instant::now());
        }
    }

    /// A record was adopted: a new analysis context (`GAME_CHANGED` for the previous task).
    pub(super) fn quick_after_load(&mut self) {
        self.quick_cancel();
        self.quick.cancelled_doc = None;
        self.quick_start();
    }

    /// The browsing position changed: an active task waits for browsing to settle before its
    /// next attempt; without one, analysis that is still missing starts once it settles.
    pub(super) fn quick_navigated(&mut self) {
        let now = Instant::now();
        if let Some(t) = self.quick.task.as_mut() {
            t.navigation_until = Some(now + NAVIGATION);
            if t.attempt.is_none() {
                t.retry_at = Some(now + NAVIGATION);
            }
        } else if self.quick_allowed() && !self.quick_missing().is_empty() {
            self.quick.start_at = Some(now + NAVIGATION);
        }
    }

    /// Ends the task without resuming anything (`cancel`): a process stops at once, a borrowed
    /// engine is given back and that is awaited (`releasing`).
    pub(super) fn quick_cancel(&mut self) {
        self.quick_end(false);
    }

    /// 闪电分析 takes over (`MANUAL_FLASH`): the task ends without the foreground engine resuming
    /// in between; its analysis process stays for the flash analysis, a borrowed engine is given
    /// back first (`returnAutomaticQuickAnalysisUse`).
    pub(super) fn quick_hand_over(&mut self) {
        self.quick_end(true);
    }

    fn quick_end(&mut self, keep_process: bool) {
        self.quick.start_at = None;
        let Some(mut task) = self.quick.task.take() else {
            return;
        };
        match task.attempt.take() {
            Some(Attempt::Process { instance, command, .. }) if keep_process => {
                self.keep_analysis_process(instance, command)
            }
            Some(Attempt::Process { instance, .. }) => self.engines.terminate(instance),
            Some(Attempt::Lease { lease, closing, .. }) => {
                if closing.is_none() {
                    self.engines.end_lease(lease);
                }
                if self.engines.lease_alive(lease) {
                    self.quick.releasing = Some(lease);
                }
            }
            None => {}
        }
        self.drain_engine_events();
    }

    /// The idle analysis process (Java's one `analysisEngine`): kept for the next quick or flash
    /// analysis with the same command; one it replaces ends.
    pub(super) fn keep_analysis_process(&mut self, instance: InstanceId, command: String) {
        if let Some((old, _)) = self.quick.warm.replace((instance, command)) {
            if old != instance {
                self.engines.terminate(old);
            }
        }
    }

    /// 整盘精析 owns a distinct workload; an idle quick/flash process must not survive takeover.
    pub(super) fn discard_analysis_process(&mut self) {
        if let Some((instance, _)) = self.quick.warm.take() { self.engines.terminate(instance); }
    }

    /// An analysis process running `command`: the idle one when it matches (`isAnalysisEngineReusable`),
    /// else a new one (an idle one with another command ends).
    pub(super) fn take_analysis_process(&mut self, command: &str) -> Result<InstanceId, String> {
        match self.quick.warm.take() {
            Some((id, warm)) if warm == command && self.engines.worker_alive(id) => Ok(id),
            Some((id, _)) => {
                self.engines.terminate(id);
                self.engines.start_worker(command, "AnalysisEngine")
            }
            None => self.engines.start_worker(command, "AnalysisEngine"),
        }
    }

    /// Space / 暂停 while a quick analysis runs: the pause also cancels this document's quick
    /// analysis. Resuming while the borrowed engine is given back waits for it; a failed hand-back
    /// keeps the pause. Returns whether the toggle was handled here.
    pub(super) fn quick_toggle_pause(&mut self) -> bool {
        if !self.paused && self.quick.task.is_some() {
            self.paused = true;
            self.quick.cancelled_doc = Some(self.doc.id);
            self.quick_cancel();
            true
        } else if self.paused && self.quick.releasing.is_some() {
            self.paused = false;
            true
        } else {
            false
        }
    }

    /// The engine was switched, restarted or closed: the quick analysis ends (`ENGINE_SWITCH`),
    /// as does a flash analysis borrowing it.
    pub(super) fn quick_engine_changed(&mut self) {
        self.quick_cancel();
        self.flash_engine_changed();
    }

    /// A cancelled task's borrowed engine is still being given back.
    pub(super) fn quick_releasing(&self) -> bool {
        self.quick.releasing.is_some()
    }

    /// A finished read waits while a borrowed engine is being given back (background analyses
    /// of the record end first); returns the read when it can be adopted now.
    pub(super) fn quick_defer_open(&mut self, open: DeferredOpen) -> Option<DeferredOpen> {
        if self
            .quick
            .task
            .as_ref()
            .is_some_and(|t| matches!(t.attempt, Some(Attempt::Lease { .. })))
        {
            self.quick_cancel();
        }
        self.flash_document_closing();
        self.whole_game_document_closing();
        if self.quick.releasing.is_some() || self.flash_releasing() || self.whole_game_releasing() {
            self.quick.deferred_open = Some(open);
            None
        } else {
            Some(open)
        }
    }

    /// A borrowed engine came back: a read waiting for it is adopted once no other one is out.
    pub(super) fn complete_deferred_open(&mut self) {
        if self.quick.releasing.is_none() && !self.flash_releasing() && !self.whole_game_releasing() {
            if let Some((ticket, result)) = self.quick.deferred_open.take() {
                self.complete_open(ticket, result);
            }
        }
    }

    fn quick_settle(&mut self) {
        self.quick.task = None;
        // Completed or unavailable: the foreground engine resumes (unless paused).
        self.sync_engine();
    }

    fn quick_progress(&mut self, now: Instant) {
        let Some(t) = self.quick.task.as_ref() else { return };
        if t.attempt.is_some() {
            return;
        }
        if t.doc != self.doc.id {
            return self.quick_settle();
        }
        let borrow = self.quick_borrows();
        let readiness = self.quick_readiness(borrow);
        if matches!(readiness, Readiness::Stop) {
            return self.quick_settle();
        }
        let missing = self.quick_missing();
        if missing.is_empty() {
            return self.quick_settle();
        }
        let rules = self.quick_rules_signature();
        let t = self.quick.task.as_mut().unwrap();
        if let Some(until) = t.navigation_until.filter(|&u| u > now) {
            t.retry_at = Some(until);
            return;
        }
        if matches!(readiness, Readiness::Wait) {
            t.retry_at = Some(now + RETRY);
            return;
        }
        t.retry_at = None;
        t.borrowed = borrow;
        t.watchdog = Some(now + WATCHDOG);
        // Each attempt takes the position from the foreground engine again
        // (`pauseForegroundAnalysisForRequest`).
        t.foreground_resumed = false;
        t.rules = rules;
        if borrow {
            match self.engines.begin_lease() {
                Ok(lease) => {
                    self.quick.task.as_mut().unwrap().attempt = Some(Attempt::Lease {
                        lease,
                        queue: lease_order(missing),
                        closing: None,
                    });
                }
                Err(_) => return self.quick_released(true, false, now),
            }
        } else {
            self.quick_start_process(missing, now);
        }
        self.sync_engine();
    }

    fn quick_start_process(&mut self, missing: Vec<NodeId>, now: Instant) {
        let command = self.quick_command();
        let instance = match self.take_analysis_process(&command) {
            Ok(id) => id,
            Err(message) => {
                self.log(true, format!("quick analysis engine failed to start: {message}"));
                return self.quick_released(true, false, now);
            }
        };
        let prefix = format!("quick{}", self.quick.next_task);
        let (queries, sent) = self.send_analysis_queries(instance, &missing, &prefix);
        self.quick.task.as_mut().unwrap().attempt = Some(Attempt::Process {
            instance,
            command,
            queries,
            failed: false,
        });
        if !sent {
            self.quick_finish_attempt(true, now);
        }
    }

    /// One query per node to analysis process `instance` (ids `<prefix>-<node>`), under the
    /// current komi and analysis rules; returns what each asked for and whether all were written.
    pub(super) fn send_analysis_queries(
        &mut self,
        instance: InstanceId,
        nodes: &[NodeId],
        prefix: &str,
    ) -> (HashMap<String, Query>, bool) {
        let rec = self.doc.record.clone();
        let options = QueryOptions {
            max_visits: self.config.analysis_engine.max_visits.max(1) + 1,
            komi: self.komi,
            rules: self.quick_rules(),
            pv_visits: self.config.display.pv_visits != PvVisitsMode::Off,
            ownership: false,
            moves_ownership: false,
        };
        let mut queries = HashMap::new();
        let mut sent = true;
        for &node in nodes {
            let id = format!("{prefix}-{node}");
            sent &= self
                .engines
                .worker_send(instance, &kata_analysis::query(&id, &rec, node, &options));
            let n = rec.node(node);
            queries.insert(
                id,
                Query {
                    node,
                    stamp: n.stamp,
                    komi: self.komi,
                    rules_rev: self.rules_revision,
                    to_play: n.to_play,
                },
            );
        }
        (queries, sent)
    }

    /// The attempt ended: its process stops; a borrowed engine is given back first.
    fn quick_finish_attempt(&mut self, failed: bool, now: Instant) {
        let Some(t) = self.quick.task.as_mut() else { return };
        t.watchdog = None;
        match t.attempt.take() {
            Some(Attempt::Process { instance, .. }) => {
                // An automatic quick analysis never keeps its process (`isAutomaticBackgroundTask`).
                self.engines.terminate(instance);
                self.quick_released(failed, false, now);
            }
            Some(Attempt::Lease {
                lease,
                queue,
                closing: None,
            }) if self.engines.lease_alive(lease) => {
                t.attempt = Some(Attempt::Lease {
                    lease,
                    queue,
                    closing: Some(failed),
                });
                self.engines.end_lease(lease);
                self.drain_engine_events();
            }
            Some(Attempt::Lease { closing, .. }) => self.quick_released(closing.unwrap_or(failed), false, now),
            None => {}
        }
    }

    /// `released`: the attempt's engine is back (or gone); the task ends or retries.
    fn quick_released(&mut self, failed: bool, restore_failed: bool, now: Instant) {
        let Some(t) = self.quick.task.as_ref() else { return };
        if t.doc != self.doc.id || matches!(self.quick_readiness(self.quick_borrows()), Readiness::Stop) {
            return self.quick_settle();
        }
        let t = self.quick.task.as_mut().unwrap();
        if failed {
            t.failures = (t.failures + 1).min(4);
            // A failed attempt is the foreground engine's opportunity, unless its restore failed.
            if !restore_failed {
                t.foreground_resumed = true;
            }
        } else {
            t.failures = 0;
        }
        if self.quick_missing().is_empty() {
            return self.quick_settle();
        }
        let t = self.quick.task.as_mut().unwrap();
        let delay = (RETRY * (1 << t.failures)).min(MAX_RETRY);
        let navigation = t
            .navigation_until
            .map_or(Duration::ZERO, |u| u.saturating_duration_since(now));
        t.retry_at = Some(now + delay.max(navigation));
        self.sync_engine();
    }

    /// Asks the borrowed engine about the next missing move, or ends the attempt.
    fn quick_lease_next(&mut self, now: Instant) {
        loop {
            let Some(Task {
                attempt: Some(Attempt::Lease { lease, queue, .. }),
                ..
            }) = self.quick.task.as_mut()
            else {
                return;
            };
            let lease = *lease;
            let Some(node) = queue.pop_front() else {
                return self.quick_finish_attempt(false, now);
            };
            let rec = &self.doc.record;
            if !rec.contains(node) || self.store.get(rec, node).is_some_and(|a| a.visits > 0) {
                continue;
            }
            let mut target = self.analysis_target();
            target.node = node;
            let rules = self.quick_lease_rules();
            if !self
                .engines
                .lease_search(lease, &target, LEASE_VISITS, rules.as_deref())
            {
                return self.quick_finish_attempt(true, now);
            }
            if let Some(t) = self.quick.task.as_mut() {
                t.watchdog = Some(now + WATCHDOG);
            }
            return;
        }
    }

    /// Stores a quick-analysis result where it was asked for, while the task still asks under
    /// the same rules settings.
    fn quick_store(&mut self, doc: u64, q: &Query, line: InfoLine, borrowed: bool) {
        if self
            .quick
            .task
            .as_ref()
            .is_none_or(|t| t.rules != self.quick_rules_signature())
        {
            return;
        }
        self.store_background_result(doc, q, line, borrowed);
    }

    /// Stores a quick or flash analysis result where it was asked for, if that still applies; a
    /// stored result is replaced only by one with more visits unless 总是覆盖 is set. Its
    /// playouts are the root's visits when reported; a borrowed search also takes the root's own
    /// winrate and score (`AnalysisEngine.parseResult`, `completeRemoteGtpJob`).
    pub(super) fn store_background_result(&mut self, doc: u64, q: &Query, line: InfoLine, borrowed: bool) {
        if doc != self.doc.id || doc != self.store.doc() || q.komi != self.komi || q.rules_rev != self.rules_revision {
            return;
        }
        let rec = self.doc.record.clone();
        if !rec.contains(q.node) || rec.node(q.node).stamp != q.stamp {
            return;
        }
        let name = super::exporting::java_text(self.active_language, "AnalysisEngine.flashAnalyze");
        let (root_visits, root_winrate, root_score) = (line.root_visits, line.root_winrate, line.root_score_lead);
        let Some((mut result, ownership)) = node_analysis(q.node, q.to_play, q.komi, name, line) else {
            return;
        };
        if let Some(visits) = root_visits.filter(|v| *v > 0) {
            result.visits = visits;
        }
        if borrowed {
            if let Some(w) = root_winrate.filter(|w| w.is_finite()) {
                result.winrate = w;
            }
            if let Some(s) = root_score.filter(|s| s.is_finite()) {
                result.score_lead = Some(s);
            }
        }
        let replace = self.config.analysis_engine.always_override
            || self.store.get(&rec, q.node).is_none_or(|old| more_visits(&result, old));
        if replace {
            let limit = self.config.engine.analysis.limit_branch_length;
            fill_pv_captures(&mut result, &rec.position(q.node), limit, !self.config.edit.no_capture);
            self.store.insert(q.node, q.stamp, result, ownership);
        }
    }

    /// Runtime events of quick analysis; any other event is handed back.
    pub(super) fn quick_event(&mut self, ev: RuntimeEvent) -> Option<RuntimeEvent> {
        let now = Instant::now();
        let current_lease = match self.quick.task.as_ref().and_then(|t| t.attempt.as_ref()) {
            Some(Attempt::Lease { lease, closing, .. }) => Some((*lease, closing.is_some())),
            _ => None,
        };
        let current_process = match self.quick.task.as_ref().and_then(|t| t.attempt.as_ref()) {
            Some(Attempt::Process { instance, .. }) => Some(*instance),
            _ => None,
        };
        let ours = |lease: u64| current_lease.is_some_and(|(l, _)| l == lease);
        match ev {
            RuntimeEvent::LeaseReady { lease } if ours(lease) => {
                if current_lease == Some((lease, false)) {
                    self.quick_lease_next(now);
                }
            }
            RuntimeEvent::LeaseResult {
                lease,
                doc,
                node,
                stamp,
                komi,
                rules_rev,
                to_play,
                line,
            } if ours(lease) => {
                if current_lease == Some((lease, false)) {
                    let q = Query {
                        node,
                        stamp,
                        komi,
                        rules_rev,
                        to_play,
                    };
                    self.quick_store(doc, &q, line, true);
                    self.quick_lease_next(now);
                }
            }
            RuntimeEvent::LeaseSearchFailed { lease } if ours(lease) => {
                if current_lease == Some((lease, false)) {
                    self.quick_finish_attempt(true, now);
                }
            }
            RuntimeEvent::LeaseEnded { lease, failure } if ours(lease) || self.quick.releasing == Some(lease) => {
                let restore_failed = failure.is_some_and(LeaseFailure::unrestores);
                if let Some(f) = failure {
                    self.log(true, format!("foreground engine lease {lease} ended: {f:?}"));
                }
                if self.quick.releasing == Some(lease) {
                    self.quick.releasing = None;
                    // `finishUserAnalysisPauseCleanup`: an engine not given back keeps the pause.
                    if restore_failed {
                        self.paused = true;
                    }
                    self.borrowed_engine_returned();
                    self.sync_engine();
                } else {
                    let closing = match self.quick.task.as_mut().and_then(|t| t.attempt.take()) {
                        Some(Attempt::Lease { closing, .. }) => closing,
                        _ => None,
                    };
                    // An end the attempt did not ask for is a failure.
                    let failed = closing.unwrap_or(true) || failure.is_some();
                    self.quick_released(failed, restore_failed, now);
                }
            }
            RuntimeEvent::WorkerLine { instance, line } if current_process == Some(instance) => {
                self.quick_worker_line(instance, &line, now)
            }
            RuntimeEvent::WorkerExited {
                instance,
                message,
                output,
            } if current_process == Some(instance)
                || self.quick.warm.as_ref().is_some_and(|(id, _)| *id == instance) =>
            {
                if self.quick.warm.as_ref().is_some_and(|(id, _)| *id == instance) {
                    self.quick.warm = None;
                }
                if current_process == Some(instance) {
                    let tail = output
                        .iter()
                        .rev()
                        .take(5)
                        .rev()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" / ");
                    self.log(true, format!("quick analysis engine exited: {message} | {tail}"));
                    if let Some(t) = self.quick.task.as_mut() {
                        t.attempt = None; // already gone
                        t.watchdog = None;
                    }
                    self.quick_released(true, false, now);
                }
            }
            other => return Some(other),
        }
        None
    }

    fn quick_worker_line(&mut self, instance: InstanceId, line: &str, now: Instant) {
        let size = self.doc.record.size;
        let Some(Task {
            attempt:
                Some(Attempt::Process {
                    instance: own,
                    queries,
                    failed,
                    ..
                }),
            doc,
            watchdog,
            ..
        }) = self.quick.task.as_mut()
        else {
            return;
        };
        if *own != instance {
            return;
        }
        let doc = *doc;
        match kata_analysis::parse(line, size) {
            Response::Result { id, line } => {
                if let Some(q) = queries.remove(&id) {
                    *watchdog = Some(now + WATCHDOG);
                    self.quick_store(doc, &q, line, false);
                }
            }
            Response::Error { id, message } => {
                let known = id.as_ref().is_some_and(|id| queries.remove(id).is_some());
                *failed = true;
                self.log(true, format!("quick analysis error: {message}"));
                if !known {
                    return self.quick_finish_attempt(true, now);
                }
            }
            Response::Other => return,
        }
        if let Some(Task {
            attempt: Some(Attempt::Process { queries, failed, .. }),
            ..
        }) = self.quick.task.as_ref()
        {
            if queries.is_empty() {
                let failed = *failed;
                self.quick_finish_attempt(failed, now);
            }
        }
    }

    /// The foreground engine became ready: a preloaded analysis process starts
    /// (`preloadConfiguredAnalysisEngineAfterStartup`), unless an analysis already has one.
    pub(super) fn quick_preload(&mut self) {
        if !self.config.analysis_engine.preload
            || self.quick.warm.is_some()
            || self.quick.task.is_some()
            || self.quick_borrows()
            || self.flash_active()
            || self.whole_game_active()
        {
            return;
        }
        let command = self.quick_command();
        match self.engines.start_worker(&command, "AnalysisEngine") {
            Ok(id) => self.quick.warm = Some((id, command)),
            Err(message) => self.log(true, format!("analysis engine preload failed: {message}")),
        }
    }

    /// The analysis engine or lightweight model settings changed: an idle preloaded process no
    /// longer applies.
    pub(super) fn quick_drop_warm(&mut self) {
        if let Some((id, _)) = self.quick.warm.take() {
            self.engines.terminate(id);
        }
    }

    // -------------------------------------------------------------------- timers

    pub(super) fn quick_deadline(&self) -> Option<Instant> {
        let task = self.quick.task.as_ref();
        [
            self.quick.start_at,
            task.and_then(|t| t.retry_at),
            task.and_then(|t| t.watchdog),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Timed quick-analysis work due at `now`; returns whether anything changed.
    pub(super) fn quick_tick(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if self.quick.start_at.is_some_and(|at| at <= now) {
            self.quick.start_at = None;
            self.quick_start();
            changed = true;
        }
        if let Some(t) = self.quick.task.as_mut() {
            if t.retry_at.is_some_and(|at| at <= now) {
                t.retry_at = None;
                self.quick_progress(now);
                changed = true;
            }
        }
        if self
            .quick
            .task
            .as_ref()
            .and_then(|t| t.watchdog)
            .is_some_and(|at| at <= now)
        {
            // `armWatchdog`: an attempt still in progress gets more time.
            let in_progress = match self.quick.task.as_ref().and_then(|t| t.attempt.as_ref()) {
                Some(Attempt::Process { instance, queries, .. }) => {
                    self.engines.worker_alive(*instance) && !queries.is_empty()
                }
                Some(Attempt::Lease { lease, .. }) => self.engines.lease_alive(*lease),
                None => false,
            };
            if in_progress {
                self.quick.task.as_mut().unwrap().watchdog = Some(now + WATCHDOG);
            } else {
                self.quick_finish_attempt(true, now);
                changed = true;
            }
        }
        changed
    }

    // -------------------------------------------------------------------- settings

    pub fn set_auto_quick_analyze(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.record.auto_quick_analyze_on_load == enabled {
            return Ok(false);
        }
        self.commit_config(|c| c.record.auto_quick_analyze_on_load = enabled)?;
        if !enabled {
            self.quick_cancel();
            self.sync_engine();
        }
        Ok(true)
    }

    /// 精析设置 > 确定 (`AnalysisSettings.saveConfig`). The command counts as customized once it
    /// was edited or differs from the one shown, unless the current engine is reused. Refused
    /// (`Err` key) without a command unless the current engine is reused.
    pub fn save_analysis_engine_settings(
        &mut self,
        mut settings: AnalysisEngineSettings,
        shown_command: &str,
        edited: bool,
    ) -> Result<Result<bool, &'static str>, ApiError> {
        settings.command = settings.command.trim().to_string();
        if !settings.reuse_current_engine && settings.command.is_empty() {
            return Ok(Err("AnalysisSettings.commandRequired"));
        }
        settings.max_visits = settings.max_visits.max(1);
        let previous = &self.config.analysis_engine;
        settings.command_customized = previous.command_customized
            || (!settings.reuse_current_engine && (edited || settings.command != shown_command.trim()));
        if *previous == settings {
            return Ok(Ok(false));
        }
        let restart =
            previous.command != settings.command || previous.reuse_current_engine != settings.reuse_current_engine;
        self.commit_config(|c| c.analysis_engine = settings)?;
        if restart {
            self.quick_drop_warm();
        }
        Ok(Ok(true))
    }

    pub fn set_quick_model_enabled(&mut self, enabled: bool) -> Result<bool, ApiError> {
        if self.config.quick_model.enabled == enabled {
            return Ok(false);
        }
        self.commit_config(|c| c.quick_model.enabled = enabled)?;
        self.quick_drop_warm();
        Ok(true)
    }

    // -------------------------------------------------------------------- projection

    pub(super) fn quick_view(&self) -> QuickAnalysisView {
        let task = self.quick.task.as_ref();
        QuickAnalysisView {
            running: task.is_some() || self.quick.releasing.is_some(),
            retrying: task.is_some_and(|t| t.retry_at.is_some()),
            borrowed: task.is_some_and(|t| t.borrowed && t.attempt.is_some()) || self.quick.releasing.is_some(),
        }
    }

    /// Background analysis of the record is under way (`isAnalysisInProgress`).
    pub(super) fn quick_running(&self) -> bool {
        self.quick.task.is_some()
    }

    pub(super) fn quick_model_view(&self) -> QuickModelView {
        let path = self.quick_model_path();
        let enabled = self.config.quick_model.enabled;
        let status = match &path {
            None => QuickModelStatus::Missing,
            Some(_) if self.foreground_katago_version_ok() == Some(false) => QuickModelStatus::RequiresKataGo117,
            Some(_) if enabled => QuickModelStatus::Enabled,
            Some(_) => QuickModelStatus::Installed,
        };
        QuickModelView {
            status,
            path: path.map(|p| p.to_string_lossy().into_owned()),
            enabled,
        }
    }

    pub(super) fn lease_view(&self) -> (LeaseAvailabilityView, Option<UnrestoredReason>) {
        let availability = match self.engines.lease_availability() {
            LeaseAvailability::Available => LeaseAvailabilityView::Available,
            LeaseAvailability::NoForegroundEngine => LeaseAvailabilityView::NoForegroundEngine,
            LeaseAvailability::EngineNotReady => LeaseAvailabilityView::EngineNotReady,
            LeaseAvailability::NotKatago => LeaseAvailabilityView::NotKatago,
            LeaseAvailability::MissingCapability => LeaseAvailabilityView::MissingCapability,
            LeaseAvailability::ExistingLease => LeaseAvailabilityView::ExistingLease,
            LeaseAvailability::EngineStateUnrestored => LeaseAvailabilityView::EngineStateUnrestored,
        };
        let unrestored = self
            .engines
            .foreground()
            .and_then(|f| self.engines.unrestored(f))
            .map(|f| match f {
                LeaseFailure::RestoreFailed | LeaseFailure::TransportClosed => UnrestoredReason::Restore,
                _ => UnrestoredReason::Stop,
            });
        (availability, unrestored)
    }
}
