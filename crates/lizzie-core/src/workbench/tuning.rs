//! Single writer of profile tuning and benchmark task identity (`commitBenchmarkResult`).
use super::Workbench;
use crate::api::{ApiError, BenchmarkTaskView, ErrorKind, TuningOutcome, TuningProfileView, TuningView};
use crate::engine::benchmark::BenchmarkJob;
use crate::engine::{
    benchmark::{self, Observation, Target},
    tuning::{self, GtpConfiguration, ThreadPolicy, ThreadSource},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct Active {
    job: BenchmarkJob,
    view: BenchmarkTaskView,
    source_revision: u64,
    initial_setup: bool,
    automatic: bool,
    progress: benchmark::Progress,
}
#[derive(Clone)]
struct MeasuredReview {
    id: u64,
    engine_id: String,
    command: String,
    environment: serde_json::Value,
    launch: crate::engine::gtp::LaunchSpec,
    report: serde_json::Value,
}
#[derive(Default)]
pub(super) struct TuningFlow {
    revision: u64,
    sent: Option<u64>,
    sent_profiles: Vec<crate::engine::profile::EngineProfile>,
    next: u64,
    active: Option<Active>,
    measured_review: Option<MeasuredReview>,
    outcome: Option<TuningOutcome>,
}
impl Drop for TuningFlow {
    fn drop(&mut self) {
        if let Some(task) = &self.active {
            task.job.cancel.store(true, Ordering::Release);
        }
    }
}
fn error(key: &str) -> ApiError {
    ApiError::new(ErrorKind::InvalidArgument, key)
}
impl Workbench {
    pub fn set_measured_hardware(&mut self, hardware: Option<(String, String, u64)>) {
        let available = hardware.is_some();
        let changed = self.engines.set_measured_hardware(hardware);
        if changed
            && available
            && self
                .engines
                .foreground()
                .and_then(|id| self.engines.profile_of(id))
                .is_some_and(|p| p.thread_policy.as_ref().is_some_and(|t| t.measured_scenes.is_some()))
        {
            let _ = self.restart_engine();
        }
    }
    pub fn set_apple_hardware(&mut self, hardware: Option<serde_json::Value>) {
        let changed = self.engines.set_apple_hardware(hardware);
        if changed
            && self
                .engines
                .foreground()
                .and_then(|id| self.engines.profile_of(id))
                .is_some_and(|p| p.thread_policy.as_ref().is_some_and(|t| t.tuning_profiles.is_some()))
        {
            let _ = self.restart_engine();
        }
    }
    pub fn refresh_tuning(&mut self) {
        self.tuning.revision += 1;
    }
    fn reload_tuned_profile(&mut self, profile: &crate::engine::profile::EngineProfile) -> Result<(), ApiError> {
        let Some(instance) = self.engines.instance_of(&profile.id) else {
            return Ok(());
        };
        if self.engines.foreground() == Some(instance) {
            self.restart_engine()?;
        } else {
            let boot = self.bootstrap(profile);
            self.engines.terminate(instance);
            self.engines.start(profile, boot).map_err(|f| error(&f.message))?;
        }
        Ok(())
    }
    pub fn measured_target(&self, engine_id: &str) -> Result<Target, ApiError> {
        let mut target = self.tuning_target(engine_id)?;
        let p = self
            .config
            .engine
            .engines
            .iter()
            .find(|p| p.id == engine_id)
            .ok_or_else(|| error("EngineThreadPolicy.targetDeleted"))?;
        tuning::apply_launch_policy(&p.command, p.thread_policy.as_ref(), &mut target.launch);
        Ok(target)
    }
    pub fn review_measured_tuning(
        &mut self,
        engine_id: &str,
        command: &str,
        report: serde_json::Value,
    ) -> Result<u64, ApiError> {
        tuning::validate_measured_report(&report).map_err(|e| error(&e))?;
        if report["scene"] != "live" {
            return Err(error("Whole-game analysis settings are unavailable"));
        }
        if !self
            .config
            .engine
            .engines
            .iter()
            .any(|p| p.id == engine_id && p.command == command)
        {
            return Err(error("EngineThreadPolicy.targetDeleted"));
        }
        let current = self.measured_target(engine_id)?;
        let environment = current.environment.clone();
        let launch = current.launch.clone();
        self.tuning.next += 1;
        let id = self.tuning.next;
        self.tuning.measured_review = Some(MeasuredReview {
            id,
            engine_id: engine_id.into(),
            command: command.into(),
            environment,
            launch,
            report,
        });
        Ok(id)
    }
    pub fn measured_review(&self, engine_id: &str, id: u64) -> Result<(Target, serde_json::Value), ApiError> {
        let review = self
            .tuning
            .measured_review
            .as_ref()
            .filter(|r| r.id == id && r.engine_id == engine_id)
            .ok_or_else(|| error("EngineThreadPolicy.benchmarkBusy"))?;
        let target = self.measured_target(engine_id)?;
        if target.command != review.command
            || target.environment != review.environment
            || target.launch != review.launch
        {
            return Err(error("EngineThreadPolicy.environmentChanged"));
        }
        Ok((target, review.report.clone()))
    }
    pub fn confirm_measured_tuning(&mut self, engine_id: &str, id: u64) -> Result<(), ApiError> {
        let (target, report) = self.measured_review(engine_id, id)?;
        self.commit_measured_tuning(engine_id, &target.command, report)?;
        self.tuning.measured_review = None;
        Ok(())
    }
    pub fn benchmark_running(&self) -> bool {
        self.tuning.active.is_some()
    }
    pub fn begin_benchmark(&mut self, engine_id: &str, experimental: bool) -> Result<BenchmarkJob, ApiError> {
        if self.tuning.active.is_some() || self.resource_task_active() {
            return Err(error("EngineThreadPolicy.benchmarkBusy"));
        }
        if experimental && !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err(error("AutoSetup.experimentalBenchmarkUnavailable"));
        }
        let profile = self
            .config
            .engine
            .engines
            .iter()
            .find(|p| p.id == engine_id)
            .ok_or_else(|| error("EngineThreadPolicy.targetDeleted"))?;
        if experimental
            && profile
                .managed
                .as_ref()
                .is_none_or(|m| m.asset_id != "macos-arm64" || m.command != profile.command)
        {
            return Err(error("AutoSetup.experimentalBenchmarkUnavailable"));
        }
        let mut target = Target::capture(&profile.command, self.engines.roots()).map_err(|e| error(&e))?;
        target.launch.library_dirs = self.engines.library_dirs().to_vec();
        let policy = profile.thread_policy.clone().unwrap_or_default();
        self.tuning.next += 1;
        let job = BenchmarkJob {
            id: self.tuning.next,
            target,
            cancel: Arc::new(AtomicBool::new(false)),
            experimental,
        };
        self.tuning.active = Some(Active {
            job: job.clone(),
            source_revision: policy.source_revision,
            initial_setup: policy.initial_setup,
            automatic: false,
            progress: benchmark::Progress::default(),
            view: BenchmarkTaskView {
                id: job.id,
                engine_id: engine_id.into(),
                experimental,
                automatic: false,
                permille: 0,
                status_lines: Vec::new(),
                cancelling: false,
            },
        });
        self.tuning.outcome = None;
        self.tuning.revision += 1;
        self.sync_engine();
        Ok(job)
    }
    pub fn benchmark_progress(&mut self, id: u64, line: &str) -> bool {
        let Some(task) = self.tuning.active.as_mut().filter(|task| task.job.id == id) else {
            return false;
        };
        task.view.permille = task.progress.line(line);
        task.view
            .status_lines
            .push(crate::sanitize::sanitize(line).chars().take(300).collect());
        if task.view.status_lines.len() > 8 {
            task.view.status_lines.remove(0);
        }
        self.tuning.revision += 1;
        true
    }
    pub fn stop_benchmark(&mut self) -> bool {
        let Some(task) = &mut self.tuning.active else {
            return false;
        };
        task.job.cancel.store(true, Ordering::Release);
        task.view.cancelling = true;
        self.tuning.revision += 1;
        true
    }
    pub fn dismiss_startup_notice(&mut self, id: u64) -> bool {
        if self
            .tuning
            .active
            .as_ref()
            .is_some_and(|t| t.job.id == id && t.automatic)
        {
            self.stop_benchmark()
        } else {
            false
        }
    }
    /// Stale completions are ignored. No failed/cancelled task changes the saved policy.
    pub fn complete_benchmark(&mut self, id: u64, result: Result<Observation, String>) -> bool {
        if self.tuning.active.as_ref().is_none_or(|task| task.job.id != id) {
            return false;
        }
        let task = self.tuning.active.take().unwrap();
        let engine_id = task.view.engine_id.clone();
        let cancelled = task.job.cancel.load(Ordering::Acquire);
        let outcome = if cancelled {
            Err("AutoSetup.benchmarkCancelled".to_string())
        } else {
            result.and_then(|r| {
            if r.failure { return Err("AutoSetup.benchmarkFailed".into()); }
            if r.recommended_threads == 0 { return Err("EngineThreadPolicy.invalidRecommendation".into()); }
            let index = self.config.engine.engines.iter().position(|p| p.id==engine_id).ok_or("EngineThreadPolicy.targetDeleted")?;
            self.commit_config(|config| {
                let policy = config.engine.engines[index].thread_policy.get_or_insert_with(ThreadPolicy::default);
                policy.threads=r.recommended_threads; policy.current_threads=r.current_threads; policy.backend=r.backend.clone();
                policy.nn_server_threads=r.nn_server_threads; policy.batch_size=r.batch_size;
                policy.visits_per_second=r.visits_per_second; policy.nn_evals_per_second=r.nn_evals_per_second; policy.average_batch_size=r.average_batch_size;
                policy.updated_at=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0,|d| d.as_millis() as u64);
                policy.summary=format!("{} threads · {:.1} visits/s · {:.1} NN evals/s",r.recommended_threads,r.visits_per_second,r.nn_evals_per_second);
                policy.environment=Some(task.job.target.environment.clone());
                policy.tuning_profiles=r.apple_fingerprint.as_ref().map(|fingerprint|if let Some(candidate)=&r.apple_candidate{serde_json::json!({"candidate":candidate,"environment":task.job.target.environment,"hardware":r.apple_hardware,"fingerprintDigest":fingerprint,"mode":"EXPERIMENTAL_HARDWARE","threads":r.recommended_threads})}else{serde_json::json!({"fingerprintDigest":fingerprint,"mode":"OFFICIAL_THREADS","threads":r.recommended_threads,"devices":[],"batch":0})});
                policy.topology=r.apple_candidate.as_ref().map(|c|c.id.clone()).unwrap_or_default();
                if task.initial_setup && task.source_revision==0 && policy.source_revision==task.source_revision && policy.initial_setup { policy.source=ThreadSource::BENCHMARK; }
                policy.initial_setup=false;
            }).map_err(|e| e.message)?;
            let profile = self.config.engine.engines[index].clone();
            if profile.command==task.job.target.command {
                if r.apple_candidate.is_some() || r.nn_server_threads>0 || r.batch_size>0 {let _=self.reload_tuned_profile(&profile);}else{self.engines.apply_thread_policy(&profile);}
            }
            Ok(())
        })
        };
        if cancelled && task.automatic {
            let signature = benchmark::startup_signature(&task.job.target);
            let _ = self.commit_config(|c| {
                c.engine.startup_benchmark_dismissed_signature = signature;
                c.engine.startup_benchmark_dismissed_version = 5;
            });
        }
        let success = outcome.is_ok();
        let message = outcome.err().unwrap_or_else(|| "AutoSetup.benchmarkDone".into());
        let known = message.starts_with("AutoSetup.") || message.starts_with("EngineThreadPolicy.");
        let key = if known {
            message.clone()
        } else {
            "AutoSetup.benchmarkFailed".into()
        };
        let detail = if known {
            String::new()
        } else {
            crate::sanitize::sanitize(&message).chars().take(300).collect()
        };
        // The log contains only the key: helper stderr may include full user paths or credentials.
        self.log(!success, format!("benchmark {key}"));
        self.tuning.outcome = Some(TuningOutcome {
            task_id: id,
            engine_id,
            key,
            detail,
            success,
        });
        self.tuning.revision += 1;
        self.sync_engine();
        true
    }
    pub fn set_thread_source(&mut self, engine_id: &str, source: ThreadSource) -> Result<(), ApiError> {
        let index = self
            .config
            .engine
            .engines
            .iter()
            .position(|p| p.id == engine_id)
            .ok_or_else(|| error("EngineThreadPolicy.targetDeleted"))?;
        if !tuning::is_local_katago(&self.config.engine.engines[index].command) {
            return Err(error("EngineThreadPolicy.unknownBenchmarkTarget"));
        }
        if source == ThreadSource::BENCHMARK
            && self.config.engine.engines[index]
                .thread_policy
                .as_ref()
                .is_none_or(|p| p.threads == 0)
        {
            return Err(error("EngineThreadPolicy.invalidRecommendation"));
        }
        self.commit_config(|c| {
            let policy = c.engine.engines[index]
                .thread_policy
                .get_or_insert_with(ThreadPolicy::default);
            policy.source = source;
            policy.source_revision += 1;
        })?;
        let profile = self.config.engine.engines[index].clone();
        if source == ThreadSource::CFG {
            self.reload_tuned_profile(&profile)?;
        } else {
            self.engines.apply_thread_policy(&profile);
        }
        self.tuning.revision += 1;
        Ok(())
    }
    pub fn tuning_target(&self, engine_id: &str) -> Result<Target, ApiError> {
        let p = self
            .config
            .engine
            .engines
            .iter()
            .find(|p| p.id == engine_id)
            .ok_or_else(|| error("EngineThreadPolicy.targetDeleted"))?;
        let mut target = Target::capture(&p.command, self.engines.roots()).map_err(|e| error(&e))?;
        target.launch.library_dirs = self.engines.library_dirs().to_vec();
        Ok(target)
    }
    pub fn gtp_configuration_target(
        &self,
        engine_id: &str,
    ) -> Result<(String, crate::engine::gtp::LaunchSpec), ApiError> {
        let p = self
            .config
            .engine
            .engines
            .iter()
            .find(|p| p.id == engine_id)
            .ok_or_else(|| error("EngineThreadPolicy.targetDeleted"))?;
        let mut spec = crate::engine::gtp::prepare_launch(&p.command, self.engines.roots())
            .ok_or_else(|| error("EngineThreadPolicy.unknownBenchmarkTarget"))?;
        spec.library_dirs = self.engines.library_dirs().to_vec();
        Ok((p.command.clone(), spec))
    }
    pub fn save_gtp_configuration(
        &mut self,
        engine_id: &str,
        command: &str,
        configuration: GtpConfiguration,
    ) -> Result<(), ApiError> {
        let index = self
            .config
            .engine
            .engines
            .iter()
            .position(|p| p.id == engine_id && p.command == command)
            .ok_or_else(|| error("EngineThreadPolicy.targetDeleted"))?;
        self.commit_config(|c| c.engine.engines[index].gtp_configuration = Some(configuration.clone()))?;
        let profile = self.config.engine.engines[index].clone();
        self.engines.apply_gtp_configuration(&profile);
        self.tuning.revision += 1;
        Ok(())
    }
    pub fn restore_measured_tuning(&mut self, engine_id: &str) -> Result<(), ApiError> {
        let index = self
            .config
            .engine
            .engines
            .iter()
            .position(|p| p.id == engine_id)
            .ok_or_else(|| error("EngineThreadPolicy.targetDeleted"))?;
        self.commit_config(|c| {
            if let Some(p) = &mut c.engine.engines[index].thread_policy {
                p.measured_scenes = None;
            }
        })?;
        self.tuning.revision += 1;
        let profile = self.config.engine.engines[index].clone();
        self.reload_tuned_profile(&profile)?;
        Ok(())
    }
    pub fn commit_measured_tuning(
        &mut self,
        engine_id: &str,
        command: &str,
        report: serde_json::Value,
    ) -> Result<(), ApiError> {
        let index = self
            .config
            .engine
            .engines
            .iter()
            .position(|p| p.id == engine_id && p.command == command)
            .ok_or_else(|| error("EngineThreadPolicy.targetDeleted"))?;
        tuning::validate_measured_report(&report).map_err(|s| error(&s))?;
        if report["scene"] != "live" {
            return Err(error("Whole-game analysis settings are unavailable"));
        }
        let scene = report["scene"]
            .as_str()
            .ok_or_else(|| error("Invalid measured scene"))?
            .to_string();
        self.commit_config(|c| {
            let policy=c.engine.engines[index].thread_policy.get_or_insert_with(ThreadPolicy::default);
            let scenes=policy.measured_scenes.get_or_insert_with(|| serde_json::json!({}));
            scenes[&scene]=serde_json::json!({"schemaVersion":1,"command":command,"report":report,"acceptedAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0,|s| s.as_millis() as u64)});
        })?;
        self.tuning.revision += 1;
        let profile = self.config.engine.engines[index].clone();
        self.reload_tuned_profile(&profile)?;
        Ok(())
    }
    pub fn set_startup_benchmark(&mut self, enabled: bool) -> Result<(), ApiError> {
        self.commit_config(|c| c.engine.startup_benchmark = enabled)?;
        self.tuning.revision += 1;
        Ok(())
    }
    /// Opt-in startup scheduling is driven by the shell after its delay/readiness wait.
    pub fn begin_startup_benchmark(&mut self) -> Option<BenchmarkJob> {
        if !self.config.engine.startup_benchmark || self.benchmark_running() || self.resource_task_active() {
            return None;
        }
        let fg = self.engines.foreground()?;
        if self.engines.phase(fg) != Some(crate::engine::runtime::Phase::Ready) {
            return None;
        }
        let profile = self.engines.profile_of(fg)?.clone();
        if !tuning::is_local_katago(&profile.command) {
            return None;
        }
        let target = self.tuning_target(&profile.id).ok()?;
        let apple = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        if apple
            && profile
                .managed
                .as_ref()
                .is_none_or(|m| m.asset_id != "macos-arm64" || m.command != profile.command)
        {
            return None;
        }
        if if apple {
            self.engines.apple_profile_matches(&profile)
        } else {
            profile.thread_policy.as_ref().is_some_and(|p| p.threads > 0)
        } {
            return None;
        }
        if self.config.engine.startup_benchmark_dismissed_version >= 5
            && self.config.engine.startup_benchmark_dismissed_signature == benchmark::startup_signature(&target)
        {
            return None;
        }
        let job = self.begin_benchmark(&profile.id, false).ok()?;
        if let Some(task) = &mut self.tuning.active {
            task.automatic = true;
            task.view.automatic = true;
        }
        Some(job)
    }
    pub fn startup_engine_ready(&self) -> bool {
        self.engines
            .foreground()
            .is_some_and(|id| self.engines.phase(id) == Some(crate::engine::runtime::Phase::Ready))
    }
    pub(super) fn tuning_view(&self) -> TuningView {
        let profiles = self
            .config
            .engine
            .engines
            .iter()
            .map(|p| {
                let launch = crate::engine::gtp::prepare_launch(&p.command, self.engines.roots());
                let environment = launch.as_ref().and_then(|l| tuning::environment(&p.command, l).ok());
                let policy = p.thread_policy.clone().unwrap_or_default();
                let environment_status = if policy.threads == 0 {
                    None
                } else if policy.environment.is_none() || environment.is_none() {
                    Some("EngineThreadPolicy.environmentUnknown".into())
                } else if policy.environment != environment {
                    Some("EngineThreadPolicy.environmentChanged".into())
                } else {
                    None
                };
                let path = |key: &str| {
                    environment
                        .as_ref()
                        .and_then(|e| e[key]["path"].as_str())
                        .unwrap_or("")
                        .to_string()
                };
                let model = path("model");
                TuningProfileView {
                    engine_id: p.id.clone(),
                    name: p.name.clone(),
                    eligible: tuning::is_local_katago(&p.command),
                    engine: launch.as_ref().map(|l| l.program.clone()).unwrap_or_default(),
                    config: path("config"),
                    b11: self
                        .weight_model_name(&model)
                        .is_some_and(|n| crate::resources::is_b11_model(&n)),
                    model,
                    policy,
                    environment_status,
                }
            })
            .collect();
        TuningView {
            profiles,
            task: self.tuning.active.as_ref().map(|t| t.view.clone()),
            startup_notice: self
                .tuning
                .active
                .as_ref()
                .filter(|t| t.automatic)
                .map(|t| t.view.clone()),
            outcome: self.tuning.outcome.clone(),
            apple_silicon: cfg!(all(target_os = "macos", target_arch = "aarch64")),
        }
    }
    pub(super) fn take_tuning_view(&mut self, full: bool) -> Option<TuningView> {
        // Profile edits outside this module also change policy summaries.
        if !full
            && self.tuning.sent == Some(self.tuning.revision)
            && self.tuning.sent_profiles == self.config.engine.engines
        {
            return None;
        }
        let mut view = self.tuning_view();
        if !full && self.tuning.sent == Some(self.tuning.revision) {
            view.outcome = None;
        }
        self.tuning.sent = Some(self.tuning.revision);
        self.tuning.sent_profiles = self.config.engine.engines.clone();
        self.tuning.outcome = None;
        Some(view)
    }
}
