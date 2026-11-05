//! Cached strength reports and the single-owner HumanSL 40 preparation queue.
use super::Workbench;
use crate::{
    api::{
        ApiError, CommandResult, StrengthCommand, StrengthManualOutcome, StrengthManualView, StrengthStatusView,
        StrengthView,
    },
    engine::{
        gtp,
        kata_analysis::{self, QueryOptions},
        runtime::{InstanceId, LeaseAvailability, RuntimeEvent},
    },
    record::{NodeKind, ROOT},
    strength::{self as s, AnalysisInput, CandidateInput, MainLineInput, MoveInput, PositionInput},
};
use serde_json::{json, Value};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileIdentity {
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
}
impl FileIdentity {
    fn at(path: PathBuf) -> Option<Self> {
        let meta = fs::metadata(&path).ok()?;
        meta.is_file().then(|| Self {
            path,
            size: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}
struct Prepared {
    root: u64,
    game: Value,
    policies: Value,
}
struct Pending {
    id: String,
    turn: usize,
    profile: usize,
    file: PathBuf,
    deadline: Instant,
}
type LaunchResult = Result<(String, String), String>;
struct Task {
    generation: u64,
    root: u64,
    game: Value,
    manual: bool,
    worker: Option<InstanceId>,
    launch: Option<Receiver<LaunchResult>>,
    engine: FileIdentity,
    fingerprint: String,
    policies: Value,
    offset: usize,
    profiles: Vec<usize>,
    pending: Vec<Pending>,
}
struct HumanVerification {
    identity: FileIdentity,
    valid: Option<bool>,
    result: Option<Receiver<bool>>,
}
pub(super) struct StrengthFlow {
    active_model: String,
    status: StrengthStatusView,
    manual: Option<StrengthManualView>,
    root: Option<u64>,
    game_key: String,
    attempted: String,
    prepared: Option<Prepared>,
    task: Option<Task>,
    generation: u64,
    next_observe: Instant,
    closed: bool,
    fingerprints: HashMap<PathBuf, (FileIdentity, String)>,
    human_verified: Mutex<HashMap<PathBuf, HumanVerification>>,
    human_installed: bool,
    launch_gate: Arc<Mutex<()>>,
    /// `selectableModels()`, re-listed when the dashboard asks for a report and after model
    /// changes (Java re-reads it when the dialog opens); listed boosters are parsed once.
    models: Vec<s::StrengthModel>,
    /// SHA-256 the HumanSL model must have (`HumanSlStrength40.HUMAN_SHA`).
    human_digest: String,
    /// The document and game a 补算 that needed the model download was asked for (the dialog's
    /// capture): the run resumed after the download prepares it or fails with gameChanged.
    download_target: Option<(u64, String)>,
}
impl Default for StrengthFlow {
    fn default() -> Self {
        Self {
            active_model: "bundled:xgboost20tun".into(),
            status: StrengthStatusView {
                key: "HumanSL40.backgroundIdle".into(),
                completed: 0,
                total: 0,
                revision: 0,
            },
            manual: None,
            root: None,
            game_key: String::new(),
            attempted: String::new(),
            prepared: None,
            task: None,
            generation: 0,
            next_observe: Instant::now(),
            closed: false,
            fingerprints: HashMap::new(),
            human_verified: Mutex::new(HashMap::new()),
            human_installed: false,
            launch_gate: Arc::new(Mutex::new(())),
            models: vec![s::StrengthModel::xgboost20tun(), s::StrengthModel::humansl40()],
            human_digest: s::HUMAN_SHA.into(),
            download_target: None,
        }
    }
}
impl StrengthFlow {
    fn update(&mut self, key: &str, completed: u32, total: u32) {
        if self.status.key != key || self.status.completed != completed || self.status.total != total {
            self.status = StrengthStatusView {
                key: key.into(),
                completed,
                total,
                revision: self.status.revision.wrapping_add(1),
            };
        }
    }
}
fn failure(message: String) -> StrengthManualOutcome {
    if message.starts_with("HumanSL40.") {
        StrengthManualOutcome::Failed {
            message_key: message,
            detail: None,
        }
    } else {
        StrengthManualOutcome::Failed {
            message_key: "HumanSL40.failed".into(),
            detail: Some(message),
        }
    }
}
fn resolve_executable(program: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    roots
        .iter()
        .map(|root| root.join(path))
        .chain(std::env::var_os("PATH").into_iter().flat_map(|p| {
            std::env::split_paths(&p)
                .map(|root| root.join(path))
                .collect::<Vec<_>>()
        }))
        .find(|p| p.is_file())
}
impl Workbench {
    pub(super) fn strength_dir(&self, name: &str) -> PathBuf {
        self.storage.resources_dir.parent().unwrap_or(Path::new(".")).join(name)
    }
    /// Controlled-engine scenarios admit their stand-in HumanSL model by its own digest instead of
    /// the official one (the official network is not part of the test fixtures).
    pub fn set_strength_human_digest(&mut self, digest: &str) {
        self.strength.human_digest = digest.into();
    }
    pub(super) fn strength_reload_models(&mut self) {
        self.strength.models = s::selectable_models(&self.strength_dir("strength-models"), &self.strength.models);
    }
    /// `effectiveModel` over the cached selectable models.
    fn strength_model(&self, id: &str) -> s::StrengthModel {
        self.strength
            .models
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .unwrap_or_else(s::StrengthModel::xgboost20tun)
    }
    fn strength_input(&self) -> MainLineInput {
        let rec = &self.doc.record;
        let nodes = rec.path_to(rec.line_end(ROOT));
        // BoardData.tryToLimitMoves: candidates past 最大候选数 are bestMovesOutOfRange, which only
        // the HumanSL 40 snapshot reads. The display keeps a recently shown one a little longer;
        // that is the board's drawing memory, not the analysis.
        let limit = self.config.engine.analysis.limit_max_suggestion as usize;
        let shown = if limit > 0 && !self.config.display.no_sugg_circle {
            limit
        } else {
            usize::MAX
        };
        let positions = nodes
            .iter()
            .map(|&id| {
                let node = rec.node(id);
                let action = (id != ROOT).then(|| MoveInput {
                    color: node.kind.action_color().unwrap_or(node.to_play),
                    move_number: node.move_number,
                    coordinate: match node.kind {
                        NodeKind::Move { point, .. } => gtp::vertex(rec.size, point),
                        _ => "pass".into(),
                    },
                    ordinary: node.kind.is_action() && node.setup.is_empty(),
                });
                let analysis = self.store.get(rec, id).map(|a| {
                    let mut candidates: Vec<_> = a
                        .candidates
                        .iter()
                        .map(|c| CandidateInput {
                            coordinate: c.point.map_or_else(|| "pass".into(), |p| gtp::vertex(rec.size, p)),
                            order: c.order,
                            visits: c.visits,
                            winrate: c.winrate,
                            score_lead: c.score_lead,
                            kata: c.score_lead.is_some(),
                            prior: c.prior / 100.0,
                        })
                        .collect();
                    let extra_candidates = candidates.split_off(shown.min(candidates.len()));
                    AnalysisInput {
                        to_play: a.to_play,
                        winrate: a.winrate,
                        score_lead: a.score_lead,
                        kata: a.score_lead.is_some(),
                        has_payload: a.visits > 0
                            || a.root_visits.is_some()
                            || !a.engine.is_empty()
                            || !a.candidates.is_empty()
                            || a.score_lead.is_some()
                            || self.store.ownership(rec, id).is_some(),
                        candidates,
                        extra_candidates,
                    }
                });
                PositionInput { action, analysis }
            })
            .collect();
        let game = serde_json::from_str(&kata_analysis::query(
            "strength40-capture",
            rec,
            *nodes.last().unwrap_or(&ROOT),
            &QueryOptions {
                max_visits: 1,
                komi: self.komi,
                rules: self.quick_rules(),
                pv_visits: false,
                ownership: false,
                moves_ownership: false,
            },
        ))
        .expect("KataGo query is JSON");
        MainLineInput {
            board_width: u32::from(rec.size.width),
            board_height: u32::from(rec.size.height),
            game,
            has_setup_stones: nodes.iter().any(|&id| !rec.node(id).setup.is_empty()),
            positions,
        }
    }
    fn strength_engine(&self) -> Option<PathBuf> {
        // discoverAssets: the local setup (the installed package of the current or default engine,
        // else that engine's KataGo executable, complete or not), then the analysis command.
        let profile = self
            .engines
            .foreground()
            .and_then(|id| self.engines.profile_of(id))
            .or_else(|| {
                self.config
                    .engine
                    .default_index()
                    .and_then(|i| self.config.engine.engines.get(i))
            });
        let managed = profile.and_then(|p| p.managed.as_ref()).map(|m| m.asset_id.as_str());
        if let Ok(inventory) = self.resources.store().inventory() {
            let recommended = self
                .resources
                .catalog()
                .assets
                .values()
                .find(|a| Some(a.id.as_str()) == managed);
            if let Some(package) = inventory
                .packages
                .iter()
                .find(|p| p.usable && recommended.is_some_and(|a| p.is_current(a)))
            {
                return Some(package.executable.clone());
            }
        }
        // KataGoAutoSetupHelper.discoverFromCommand: a KataGo-named program in gtp/analysis mode.
        let saved = profile
            .and_then(|p| gtp::prepare_launch(&p.command, self.engines.roots()))
            .filter(|spec| {
                Path::new(&spec.program)
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().to_lowercase().contains("katago"))
                    && spec
                        .args
                        .iter()
                        .any(|a| a.eq_ignore_ascii_case("gtp") || a.eq_ignore_ascii_case("analysis"))
            });
        if let Some(engine) = saved.and_then(|spec| resolve_executable(&spec.program, self.engines.roots())) {
            return Some(engine);
        }
        let base = self.analysis_command_base(false);
        if base.trim() == crate::settings::DEFAULT_ANALYSIS_COMMAND {
            return None;
        }
        let spec = gtp::prepare_launch(&base, self.engines.roots())?;
        if Path::new(&spec.program)
            .file_stem()
            .is_some_and(|s| s.eq_ignore_ascii_case("ssh"))
        {
            return None;
        }
        resolve_executable(&spec.program, self.engines.roots())
    }
    fn strength_human_candidate(&self) -> Option<PathBuf> {
        self.config
            .human_sl
            .model_path
            .as_ref()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .or_else(|| {
                let model = self.resources.catalog().models.get(super::resources::HUMAN_SL_MODEL)?;
                let path = self.resources.store().human_sl_models_dir().join(&model.file_name);
                path.is_file().then_some(path)
            })
    }
    fn strength_human_valid(&self, path: &Path, wait: bool) -> bool {
        self.strength_human_verification(path, wait).unwrap_or(false)
    }
    /// Whether the model at `path` is the HumanSL 40 network; `None` while its checksum is still
    /// being computed (only when not `wait`ing). T22's verification of the same file answers
    /// first, whichever of the two finished.
    fn strength_human_verification(&self, path: &Path, wait: bool) -> Option<bool> {
        if let Some(valid) = self.human_sl_official_verification(path) {
            return Some(valid);
        }
        let Some(identity) = FileIdentity::at(path.to_path_buf()) else {
            return Some(false);
        };
        let mut cache = self.strength.human_verified.lock().unwrap();
        if !cache.get(path).is_some_and(|entry| entry.identity == identity) {
            let (send, result) = mpsc::channel();
            let path = path.to_path_buf();
            let expected = self.strength.human_digest.clone();
            std::thread::spawn(move || {
                let valid =
                    crate::resources::sha256_file(&path).is_ok_and(|digest| digest.eq_ignore_ascii_case(&expected));
                let _ = send.send(valid);
            });
            cache.insert(
                identity.path.clone(),
                HumanVerification {
                    identity,
                    valid: None,
                    result: Some(result),
                },
            );
        }
        let entry = cache.get_mut(path).unwrap();
        if let Some(valid) = entry.valid {
            return Some(valid);
        }
        let result = entry.result.take()?;
        // No verification-cache lock is held while hashing/waiting. Views only poll;
        // an explicit manual request waits for its exact checksum before admitting work.
        drop(cache);
        let valid = if wait {
            result.recv().ok()
        } else {
            result.try_recv().ok()
        };
        let mut cache = self.strength.human_verified.lock().unwrap();
        let entry = cache.get_mut(path).unwrap();
        if let Some(valid) = valid {
            entry.valid = Some(valid);
        } else {
            entry.result = Some(result);
        }
        entry.valid
    }
    pub fn strength(&mut self, op: StrengthCommand) -> Result<CommandResult, ApiError> {
        match op {
            StrengthCommand::Report => {
                self.strength_reload_models();
                let input = self.strength_input();
                let model = self.strength_model(&self.strength.active_model);
                let report = if !model.human_sl {
                    s::estimate_with_model(&input, &model, &self.config.display)
                } else {
                    s::capture(&input, &self.config.display)
                        .ok()
                        .and_then(|capture| {
                            let prepared = self.strength.prepared.as_ref().filter(|p| p.root == self.doc.id)?;
                            let count = capture.game["moves"].as_array()?.len();
                            let moves = prepared.game["moves"].as_array()?;
                            if count > moves.len() {
                                return None;
                            }
                            let mut prefix = prepared.game.clone();
                            prefix["moves"] = json!(&moves[..count]);
                            (s::canonical(&prefix) == s::canonical(&capture.game))
                                .then(|| s::report(&capture, &prepared.policies, &self.config.display).ok())
                                .flatten()
                        })
                        .unwrap_or_else(|| {
                            let mut empty = input;
                            empty.positions.clear();
                            s::estimate_with_model(&empty, &model, &self.config.display)
                        })
                };
                Ok(CommandResult::Strength {
                    report: Box::new(report),
                })
            }
            StrengthCommand::SelectModel { id } => {
                self.strength_reload_models();
                self.strength.active_model = self.strength_model(&id).id;
                Ok(CommandResult::Done)
            }
            StrengthCommand::ImportModel { path } => {
                match s::import_model(Path::new(&path), &self.strength_dir("strength-models")) {
                    Ok(model) => {
                        self.strength.active_model = model.id;
                        self.strength_reload_models();
                        Ok(CommandResult::Done)
                    }
                    Err(error) => Ok(CommandResult::StrengthImportFailed {
                        detail: error.to_string(),
                    }),
                }
            }
            StrengthCommand::SetBackground { enabled } => {
                self.commit_config(|c| c.strength.humansl_background = enabled)?;
                self.strength.attempted.clear();
                if self.strength.task.as_ref().is_some_and(|t| !t.manual) {
                    self.strength_cancel(false);
                }
                self.strength_observe();
                Ok(CommandResult::Done)
            }
            StrengthCommand::RunHumanSl { resume } => {
                let requested = self.strength.download_target.take();
                let changed = || {
                    Ok(CommandResult::Refused {
                        message_key: "HumanSL40.gameChanged".into(),
                    })
                };
                let snapshot = match s::preparation_snapshot(&self.strength_input(), &self.config.display) {
                    Ok(s) => s,
                    Err(_) if resume => return changed(),
                    Err(key) => return Ok(CommandResult::Refused { message_key: key }),
                };
                let target = (self.doc.id, s::canonical(&snapshot.game));
                if resume && requested.as_ref() != Some(&target) {
                    return changed();
                }
                if self.strength_engine().is_none() {
                    return Ok(CommandResult::Refused {
                        message_key: "HumanSL40.needEngine".into(),
                    });
                }
                let Some(human) = self.strength_human_candidate() else {
                    self.strength.download_target = Some(target);
                    return Ok(CommandResult::Refused {
                        message_key: "HumanSL40.downloadRequired".into(),
                    });
                };
                if !self.strength_human_valid(&human, true) {
                    return Ok(CommandResult::Refused {
                        message_key: "HumanSL40.invalidModel".into(),
                    });
                }
                self.strength_cancel(false);
                let run = self.strength.manual.as_ref().map_or(1, |m| m.run.wrapping_add(1));
                self.strength.manual = Some(StrengthManualView {
                    run,
                    running: true,
                    outcome: None,
                });
                self.strength.root = Some(self.doc.id);
                self.strength.game_key = target.1;
                self.strength_start(snapshot.game, true);
                Ok(CommandResult::Done)
            }
            StrengthCommand::CancelHumanSl => {
                if self.strength.task.as_ref().is_some_and(|t| t.manual) {
                    self.strength_cancel(true);
                }
                Ok(CommandResult::Done)
            }
        }
    }
    fn strength_cancel(&mut self, suppress: bool) {
        if let Some(task) = self.strength.task.take() {
            if let Some(worker) = task.worker {
                self.engines.terminate(worker);
            }
            self.strength.attempted = if suppress {
                self.strength.game_key.clone()
            } else {
                String::new()
            };
            self.strength.update(
                "HumanSL40.cancelled",
                self.strength.status.completed,
                self.strength.status.total,
            );
            if task.manual {
                self.strength_finish_manual(StrengthManualOutcome::Cancelled);
            }
        }
    }
    fn strength_invalidate(&mut self) {
        self.strength_cancel(false);
        self.strength.root = None;
        self.strength.game_key.clear();
        self.strength.attempted.clear();
        self.strength.prepared = None;
        self.strength.update("HumanSL40.backgroundIdle", 0, 0);
    }
    fn strength_observe(&mut self) {
        if self.strength.closed {
            return;
        }
        let Ok(snapshot) = s::preparation_snapshot(&self.strength_input(), &self.config.display) else {
            self.strength_invalidate();
            return;
        };
        let key = s::canonical(&snapshot.game);
        let total = snapshot.game["moves"].as_array().map_or(0, Vec::len) as u32;
        if self.strength.root != Some(self.doc.id) || self.strength.game_key != key {
            self.strength_cancel(false);
            self.strength.root = Some(self.doc.id);
            self.strength.game_key = key;
            self.strength.attempted.clear();
            self.strength.prepared = None;
            self.strength.update("HumanSL40.backgroundIdle", 0, total);
        }
        let reserved = self.match_active()
            || self.game_active()
            || self.human_sl_active()
            || self.benchmark_running()
            || self.engines.lease_availability() == LeaseAvailability::ExistingLease
            || self.engines.raw_net_active();
        let foreground = self.engines.analysing(self.doc.id, self.cursor)
            || self.flash_active()
            || self.whole_game_active()
            || self.batch_engaged()
            || self.auto_analysis_active()
            || self.quick_running();
        let enabled = self.config.strength.humansl_background;
        if !enabled || self.paused || reserved {
            if self.strength.task.as_ref().is_some_and(|t| !t.manual) {
                self.strength_cancel(false);
            }
            if self.strength.task.is_none() {
                self.strength.update(
                    if enabled {
                        "HumanSL40.backgroundPaused"
                    } else {
                        "HumanSL40.backgroundOff"
                    },
                    0,
                    self.strength.status.total,
                );
            }
            return;
        }
        if self.strength.task.is_none() {
            if self.strength.prepared.is_some() {
                self.strength.update("HumanSL40.backgroundReady", total, total);
            } else {
                if matches!(
                    self.strength.status.key.as_str(),
                    "HumanSL40.backgroundOff" | "HumanSL40.backgroundPaused"
                ) {
                    self.strength
                        .update("HumanSL40.backgroundIdle", self.strength.status.completed, total);
                }
                if foreground && self.strength.attempted != self.strength.game_key {
                    self.strength_start(snapshot.game, false);
                }
            }
        }
    }
    fn strength_start(&mut self, game: Value, manual: bool) {
        if !manual {
            if let Some(human) = self.strength_human_candidate() {
                if self.strength_human_verification(&human, false).is_none() {
                    return;
                }
            }
        }
        self.strength.attempted = self.strength.game_key.clone();
        let total = game["moves"].as_array().map_or(0, Vec::len);
        self.strength.update("HumanSL40.preparing", 0, total as u32);
        let assets = (|| {
            let engine = self
                .strength_engine()
                .and_then(FileIdentity::at)
                .ok_or("HumanSL40.needEngine")?;
            let human = self.strength_human_candidate().ok_or("HumanSL40.downloadRequired")?;
            if !self.strength_human_valid(&human, true) {
                return Err("HumanSL40.invalidModel");
            }
            Ok((engine, human))
        })();
        let (engine, human) = match assets {
            Ok(a) => a,
            Err(e) => {
                self.strength_failed(e.into());
                return;
            }
        };
        let cache = self.strength_dir("strength-humansl40-policy-cache");
        let cached = self
            .strength
            .fingerprints
            .get(&engine.path)
            .filter(|(stamp, _)| stamp == &engine)
            .map(|(_, fp)| fp.clone());
        let (send, receive) = mpsc::channel();
        let path = engine.path.clone();
        let gate = self.strength.launch_gate.clone();
        // Binary hashing and config I/O run off the application lock. The result is consumed only by this task generation.
        std::thread::spawn(move || {
            // Java's single-thread queue also serializes cancelled launches with their successor.
            let _guard = gate.lock().unwrap();
            let result = (|| {
                let mut parts = s::policy_command(&path, &human, &cache).map_err(|e| e.to_string())?;
                // HumanSlAnalysisRunner.buildHumanSlCommand: the runner also names the network as
                // the human model; KataGo refuses the policy config's humanSL* keys without it.
                parts.extend(["-human-model".into(), human.to_string_lossy().into_owned()]);
                let fingerprint = match cached {
                    Some(fp) => fp,
                    None => s::fingerprint(&path, &cache).map_err(|e| e.to_string())?,
                };
                Ok((
                    gtp::join_command(&parts).ok_or("Empty strength policy command")?,
                    fingerprint,
                ))
            })();
            let _ = send.send(result);
        });
        self.strength.generation = self.strength.generation.wrapping_add(1);
        let mut policies = json!({});
        for profile in s::PROFILES {
            policies[profile] = json!(vec![Value::Null; total + 1]);
        }
        self.strength.task = Some(Task {
            generation: self.strength.generation,
            root: self.doc.id,
            game,
            manual,
            worker: None,
            launch: Some(receive),
            engine,
            fingerprint: String::new(),
            policies,
            offset: 0,
            profiles: vec![0; total],
            pending: Vec::new(),
        });
    }
    fn strength_failed(&mut self, message: String) {
        let manual = self.strength.task.as_ref().map_or_else(
            || self.strength.manual.as_ref().is_some_and(|m| m.running),
            |t| t.manual,
        );
        if let Some(task) = self.strength.task.take() {
            if let Some(worker) = task.worker {
                self.engines.terminate(worker);
            }
        }
        let key = if message.starts_with("HumanSL40.") {
            message.as_str()
        } else {
            "HumanSL40.failed"
        };
        self.strength
            .update(key, self.strength.status.completed, self.strength.status.total);
        if manual {
            self.strength_finish_manual(failure(message));
        }
    }
    pub(super) fn strength_deadline(&self) -> Option<Instant> {
        if self.strength.closed {
            return None;
        }
        Some(
            self.strength
                .next_observe
                .min(if self.strength.task.as_ref().is_some_and(|t| t.launch.is_some()) {
                    Instant::now() + Duration::from_millis(20)
                } else {
                    self.strength
                        .task
                        .as_ref()
                        .and_then(|t| t.pending.iter().map(|p| p.deadline).min())
                        .unwrap_or(self.strength.next_observe)
                }),
        )
    }
    pub(super) fn strength_tick(&mut self, now: Instant) -> bool {
        if self.strength.closed {
            return false;
        }
        let revision = self.strength.status.revision;
        let installed = self
            .strength_human_candidate()
            .is_some_and(|p| self.strength_human_valid(&p, false));
        let assets_changed = installed != self.strength.human_installed;
        self.strength.human_installed = installed;
        if !self.strength.closed && now >= self.strength.next_observe {
            self.strength.next_observe = now + Duration::from_secs(1);
            self.strength_observe();
        }
        let launch = self
            .strength
            .task
            .as_ref()
            .and_then(|t| t.launch.as_ref())
            .and_then(|r| match r.try_recv() {
                Ok(v) => Some(v),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err("Policy preparation stopped".into())),
                Err(mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = launch {
            match result.and_then(|(command, fp)| {
                self.engines
                    .start_worker(&command, "HumanSL 40 policy")
                    .map(|worker| (worker, fp))
            }) {
                Ok((worker, fp)) => {
                    let t = self.strength.task.as_mut().unwrap();
                    t.launch = None;
                    t.worker = Some(worker);
                    t.fingerprint = fp.clone();
                    self.strength
                        .fingerprints
                        .insert(t.engine.path.clone(), (t.engine.clone(), fp));
                    self.strength_pump(now);
                }
                Err(error) => self.strength_failed(error),
            }
        }
        if self
            .strength
            .task
            .as_ref()
            .is_some_and(|t| t.pending.iter().any(|p| now >= p.deadline))
        {
            self.strength_failed("HumanSL policy query timed out after 180 seconds".into());
        }
        revision != self.strength.status.revision || assets_changed
    }
    fn strength_pump(&mut self, now: Instant) {
        let cache = self.strength_dir("strength-humansl40-policy-cache");
        let Some(mut task) = self.strength.task.take() else {
            return;
        };
        let Some(worker) = task.worker else {
            self.strength.task = Some(task);
            return;
        };
        let result = (|| -> Result<bool, String> {
            loop {
                let total = task.profiles.len();
                if task.offset == total {
                    return Ok(true);
                }
                let end = (task.offset + 4).min(total);
                for turn in task.offset..end {
                    while task.profiles[turn] < s::PROFILES.len() && !task.pending.iter().any(|p| p.turn == turn) {
                        let profile = task.profiles[turn];
                        let query = s::request(&task.game, turn, s::PROFILES[profile])?;
                        let id = query["id"].as_str().unwrap().to_owned();
                        let file = cache.join(format!("{}.json", s::cache_key(&task.fingerprint, &query)));
                        let policy = fs::read(&file)
                            .ok()
                            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                            .and_then(|v| s::validate_policy(&v, &id).ok());
                        if let Some(policy) = policy {
                            task.policies[s::PROFILES[profile]][turn] = json!(policy);
                            task.profiles[turn] += 1;
                        } else {
                            if !self.engines.worker_send(worker, &query.to_string()) {
                                return Err("Policy engine input closed".into());
                            }
                            task.pending.push(Pending {
                                id,
                                turn,
                                profile,
                                file,
                                deadline: now + Duration::from_secs(180),
                            });
                        }
                    }
                }
                let done = task.profiles[task.offset..end]
                    .iter()
                    .take_while(|&&p| p == s::PROFILES.len())
                    .count()
                    + task.offset;
                if done > self.strength.status.completed as usize {
                    self.strength.update("HumanSL40.progress", done as u32, total as u32);
                }
                if done == end {
                    task.offset = end;
                } else {
                    return Ok(false);
                }
            }
        })();
        self.strength.task = Some(task);
        match result {
            Ok(true) => self.strength_complete(),
            Ok(false) => {}
            Err(error) => self.strength_failed(error),
        }
    }
    /// The admitted manual run ended with `outcome`.
    fn strength_finish_manual(&mut self, outcome: StrengthManualOutcome) {
        if let Some(m) = self.strength.manual.as_mut() {
            m.running = false;
            m.outcome = Some(outcome);
        }
    }
    fn strength_complete(&mut self) {
        let task = self.strength.task.take().unwrap();
        if let Some(worker) = task.worker {
            self.engines.terminate(worker);
        }
        if task.manual {
            let current = task.root == self.doc.id
                && s::preparation_snapshot(&self.strength_input(), &self.config.display)
                    .is_ok_and(|snapshot| s::canonical(&snapshot.game) == s::canonical(&task.game));
            self.strength_finish_manual(if current {
                StrengthManualOutcome::Ready
            } else {
                failure("HumanSL40.gameChanged".into())
            });
        }
        self.strength.prepared = Some(Prepared {
            root: task.root,
            game: task.game,
            policies: task.policies,
        });
        let total = self.strength.status.total;
        self.strength.update("HumanSL40.backgroundReady", total, total);
    }
    pub(super) fn strength_event(&mut self, event: RuntimeEvent) -> Option<RuntimeEvent> {
        let worker = self.strength.task.as_ref().and_then(|t| t.worker);
        match event {
            RuntimeEvent::WorkerLine { instance, line } if Some(instance) == worker => {
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    return None;
                };
                let task = self.strength.task.as_mut().unwrap();
                let index = task.pending.iter().position(|p| value["id"] == p.id)?;
                let pending = task.pending.remove(index);
                match s::validate_policy(&value, &pending.id) {
                    Ok(policy) => {
                        let temporary =
                            pending
                                .file
                                .with_extension(format!("{}-{}.tmp", std::process::id(), task.generation));
                        let write = (|| -> std::io::Result<()> {
                            fs::write(&temporary, json!({"id":pending.id,"humanPolicy":policy}).to_string())?;
                            fs::rename(&temporary, &pending.file)
                        })();
                        if let Err(error) = write {
                            let _ = fs::remove_file(&temporary);
                            self.strength_failed(error.to_string());
                            return None;
                        }
                        task.policies[s::PROFILES[pending.profile]][pending.turn] = json!(policy);
                        task.profiles[pending.turn] += 1;
                        self.strength_pump(Instant::now());
                    }
                    Err(error) => self.strength_failed(error),
                }
                None
            }
            RuntimeEvent::WorkerExited { instance, message, .. } if Some(instance) == worker => {
                self.strength_failed(message);
                None
            }
            other => Some(other),
        }
    }
    pub fn shutdown_strength(&mut self) {
        self.strength.closed = true;
        self.strength_cancel(false);
        self.strength.prepared = None;
    }
    pub(super) fn strength_view(&self) -> StrengthView {
        let mut hasher = DefaultHasher::new();
        self.doc.id.hash(&mut hasher);
        self.strength.status.revision.hash(&mut hasher);
        self.strength.game_key.hash(&mut hasher);
        let rec = &self.doc.record;
        let mut node = Some(ROOT);
        for _ in 0..211 {
            let Some(id) = node else {
                break;
            };
            rec.node(id).stamp.hash(&mut hasher);
            if let Some(a) = self.store.get(rec, id) {
                a.engine.hash(&mut hasher);
                a.visits.hash(&mut hasher);
                a.winrate.to_bits().hash(&mut hasher);
                a.score_lead.map(f64::to_bits).hash(&mut hasher);
                for c in &a.candidates {
                    c.point.map(|p| (p.x, p.y)).hash(&mut hasher);
                    c.order.hash(&mut hasher);
                    c.visits.hash(&mut hasher);
                    c.winrate.to_bits().hash(&mut hasher);
                    c.score_lead.map(f64::to_bits).hash(&mut hasher);
                    c.prior.to_bits().hash(&mut hasher);
                }
            }
            node = rec.main_child(id);
        }
        StrengthView {
            models: self.strength.models.iter().map(s::StrengthModel::view).collect(),
            active: self.strength_model(&self.strength.active_model).id,
            background: self.config.strength.humansl_background,
            status: self.strength.status.clone(),
            manual: self.strength.manual.clone(),
            stamp: hasher.finish(),
        }
    }
}
