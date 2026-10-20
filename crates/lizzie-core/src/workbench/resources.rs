//! Single writer for resource task identity, confirmation plans and persisted profiles.
use super::Workbench;
use crate::{
    api::{
        ApiError, ResourceAccelerationView, ResourceCommand, ResourceEngineChoice, ResourceItem, ResourceOutcome,
        ResourcePlanView, ResourceRepairView, ResourceResult, ResourceSetupView, ResourceTaskView, ResourcesView,
    },
    engine::{
        gtp::split_command,
        profile::{EngineProfile, EngineStartup},
        runtime::Phase,
    },
    resources as r,
    settings::{NetworkSettings, ProxyMode, Storage},
};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

struct Active {
    id: u64,
    cancel: Arc<AtomicBool>,
    view: ResourceTaskView,
    success_key: &'static str,
}
struct Plan {
    id: u64,
    snapshot: Vec<EngineProfile>,
    value: r::SetupPlan,
}
pub(super) struct ResourceFlow {
    revision: u64,
    sent: Option<u64>,
    next: u64,
    store: r::ResourceStore,
    catalog: r::Catalog,
    active: Option<Active>,
    plan: Option<Plan>,
    view: ResourcesView,
    verified: BTreeMap<String, r::EngineValidation>,
    projected_profiles: Vec<EngineProfile>,
    projected_foreground: Option<String>,
    weight_headers: BTreeMap<String, String>,
    projected_startup: Option<EngineStartup>,
    projected_version: Option<String>,
}
impl ResourceFlow {
    pub(super) fn new(storage: &Storage) -> Self {
        let catalog = r::Catalog::embedded().expect("embedded resource catalog");
        let view = ResourcesView {
            packages: catalog.assets.values().cloned().collect(),
            models: catalog.models.values().cloned().collect(),
            engine_release_url: format!(
                "https://github.com/{}/releases/tag/{}",
                catalog.engine_release_repository, catalog.engine_release_tag
            ),
            engine_version: catalog.katago_version.clone(),
            engine_source_commit: catalog.katago_source_commit.clone(),
            package_urls: catalog
                .assets
                .values()
                .map(|a| (a.id.clone(), catalog.asset_url(a).expect("trusted catalog asset")))
                .collect(),
            model_urls: catalog
                .models
                .values()
                .map(|m| (m.id.clone(), catalog.model_url(m)))
                .collect(),
            online_weights: r::pinned_weights(&catalog),
            remote_status: "idle".into(),
            ..Default::default()
        };
        let mut view = view;
        let store = r::ResourceStore::new(&storage.resources_dir);
        view.acceleration = acceleration(&store, None).unwrap_or_default();
        Self {
            revision: 1,
            sent: None,
            next: 0,
            store,
            catalog,
            active: None,
            plan: None,
            view,
            verified: BTreeMap::new(),
            projected_profiles: Vec::new(),
            projected_foreground: None,
            weight_headers: BTreeMap::new(),
            projected_startup: None,
            projected_version: None,
        }
    }
}
impl Drop for ResourceFlow {
    fn drop(&mut self) {
        if let Some(t) = &self.active {
            t.cancel.store(true, Ordering::Release);
        }
    }
}
/// An immutable off-lock operation and its cancellation token.
#[derive(Clone)]
pub struct ResourceJob {
    pub id: u64,
    pub cancel: Arc<AtomicBool>,
    pub operation: ResourceCommand,
    pub store: r::ResourceStore,
    pub catalog: r::Catalog,
    pub machine: Option<r::MachineFacts>,
    pub online: Vec<r::RemoteWeight>,
    pub active_executable: Option<(String, String)>,
    pub current_weight: Option<String>,
}
/// An off-lock result. Only complete_resource_task may admit it into application state.
#[derive(Default)]
pub struct ResourceCompletion {
    pub machine: Option<r::MachineFacts>,
    pub inventory: r::ResourceInventory,
    pub online: Option<Vec<r::RemoteWeight>>,
    pub validation: Option<(String, r::EngineValidation)>,
    pub cuda: Option<r::CudaProbeResult>,
    pub chosen: Option<(ResourceEngineChoice, String)>,
    pub acceleration: Vec<ResourceAccelerationView>,
    pub cache_bytes: Option<u64>,
    pub weight_headers: BTreeMap<String, String>,
    pub runtime_present: bool,
}
fn refused(key: &str) -> ResourceResult {
    ResourceResult::Refused {
        message_key: key.into(),
    }
}
impl Workbench {
    pub(crate) fn resource_task_active(&self) -> bool {
        self.resources.active.is_some()
    }
    pub fn cancel_resource_tasks(&mut self) {
        if let Some(t) = &self.resources.active {
            t.cancel.store(true, Ordering::Release);
        }
    }
    pub fn resource_store(&self) -> r::ResourceStore {
        self.resources.store.clone()
    }
    pub fn resource_network(&self) -> NetworkSettings {
        self.config.network.clone()
    }
    /// Starts one off-lock operation. Refusal never creates a task or changes settings.
    pub fn begin_resource_task(&mut self, operation: ResourceCommand) -> Result<ResourceJob, Box<ResourceResult>> {
        if self.resource_task_active() || self.benchmark_running() {
            return Err(Box::new(refused("EngineThreadPolicy.benchmarkBusy")));
        }
        let windows = self
            .resources
            .view
            .machine
            .as_ref()
            .map_or(cfg!(windows), |m| m.os == "windows");
        match &operation {
            ResourceCommand::Download {
                item: ResourceItem::Package { asset, confirmed, .. },
            } => {
                let a = self
                    .resources
                    .catalog
                    .assets
                    .get(asset)
                    .ok_or_else(|| refused("AutoSetup.downloadFailed"))?;
                if a.release_tier == r::SupportTier::Experimental && !confirmed {
                    return Err(Box::new(refused("AutoSetup.experimentalBackendConfirm")));
                }
                if self
                    .resources
                    .view
                    .machine
                    .as_ref()
                    .and_then(|m| m.platform())
                    .is_some_and(|p| p != a.platform)
                {
                    return Err(Box::new(refused("AutoSetup.importPackageMismatch")));
                }
            }
            ResourceCommand::ImportPackage {
                backend: Some(_),
                confirmed: false,
                ..
            } => return Err(Box::new(refused("AutoSetup.experimentalBackendConfirm"))),
            ResourceCommand::Download {
                item: ResourceItem::NvidiaRuntime { confirmed, backend },
            } => {
                if !windows {
                    return Err(Box::new(refused("AutoSetup.nvidiaRuntimeNotApplicable")));
                }
                if !matches!(backend.as_str(), "cuda" | "tensorrt") {
                    return Err(Box::new(refused("AutoSetup.nvidiaRuntimeNotApplicable")));
                }
                if !confirmed {
                    return Err(Box::new(refused("AutoSetup.tensorRtConfirmMessage")));
                }
            }
            ResourceCommand::RepairTensorRt { confirmed } => {
                if !windows {
                    return Err(Box::new(refused("AutoSetup.tensorRtNotApplicable")));
                }
                if !confirmed {
                    return Err(Box::new(refused("AutoSetup.tensorRtRepairConfirmMessage")));
                }
            }
            _ => {}
        }
        self.resources.next += 1;
        let id = self.resources.next;
        let (kind, item, phase) = match &operation {
            ResourceCommand::Inspect { .. } => ("inspect", "machine".into(), "detecting"),
            ResourceCommand::Download { item } => (
                "download",
                match item {
                    ResourceItem::Package { asset, .. } => asset.clone(),
                    ResourceItem::Model { model } => model.clone(),
                    ResourceItem::OnlineWeight { name } => name.clone(),
                    ResourceItem::NvidiaRuntime { backend, .. } => backend.clone(),
                },
                "downloading",
            ),
            ResourceCommand::Verify { .. } => ("verify", "engine".into(), "verifying"),
            ResourceCommand::ChooseLocalEngine { .. } => ("choose", "local".into(), "verifying"),
            ResourceCommand::RefreshRemoteWeights => ("refresh", "weights".into(), "downloading"),
            ResourceCommand::RepairTensorRt { .. } => ("repair", "tensorrt".into(), "downloading"),
            _ => ("import", "resource".into(), "installing"),
        };
        let success_key = match &operation {
            ResourceCommand::Inspect { .. } | ResourceCommand::RefreshRemoteWeights => "AutoSetup.refresh",
            ResourceCommand::ImportWeight { .. } => "AutoSetup.importWeightDone",
            ResourceCommand::ImportPackage { .. } => "AutoSetup.importPackageDone",
            ResourceCommand::RepairTensorRt { .. } => "AutoSetup.tensorRtRepairDone",
            ResourceCommand::RepairAnalysisConfig => "AutoSetup.repairAnalysisConfig",
            ResourceCommand::CleanNvidiaCache { .. } => "AutoSetup.cleanTensorRtCacheDone",
            ResourceCommand::Download {
                item: ResourceItem::NvidiaRuntime { .. },
            } => "AutoSetup.installNvidiaRuntimeDone",
            ResourceCommand::Download {
                item: ResourceItem::Package { backend: Some(_), .. },
            } => "AutoSetup.experimentalBackendDone",
            _ => "AutoSetup.downloadDone",
        };
        self.resources.view.detecting = matches!(operation, ResourceCommand::Inspect { .. });
        if matches!(operation, ResourceCommand::RefreshRemoteWeights) {
            self.resources.view.remote_status = "refreshing".into();
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.resources.active = Some(Active {
            id,
            cancel: cancel.clone(),
            success_key,
            view: ResourceTaskView {
                id,
                kind: kind.into(),
                item,
                bytes: 0,
                total: None,
                phase: phase.into(),
                cancellable: true,
            },
        });
        self.resources.view.last_outcome = None;
        self.resources.revision += 1;
        let active_executable = self
            .engines
            .foreground()
            .filter(|id| self.engines.phase(*id) == Some(Phase::Ready))
            .and_then(|id| {
                let p = self.engines.profile_of(id)?;
                let exe = split_command(&p.command).into_iter().next()?;
                Some((exe, self.engines.identity(id).map(|(_, v, _)| v).unwrap_or_default()))
            });
        let current_weight = self.resources_view().current_setup.weight;
        Ok(ResourceJob {
            id,
            cancel,
            operation,
            store: self.resources.store.clone(),
            catalog: self.resources.catalog.clone(),
            machine: self.resources.view.machine.clone(),
            online: self.resources.view.online_weights.clone(),
            active_executable,
            current_weight,
        })
    }
    pub fn resource_progress(&mut self, id: u64, phase: &str, bytes: u64, total: Option<u64>) -> bool {
        let Some(t) = self.resources.active.as_mut().filter(|t| t.id == id) else {
            return false;
        };
        t.view.phase = phase.into();
        t.view.bytes = bytes;
        t.view.total = total;
        self.resources.revision += 1;
        true
    }
    pub fn cancel_resource_task(&mut self, id: u64) -> ResourceResult {
        let Some(t) = self.resources.active.as_mut().filter(|t| t.id == id) else {
            return refused("AutoSetup.taskStale");
        };
        t.cancel.store(true, Ordering::Release);
        t.view.cancellable = false;
        t.view.phase = "cancelling".into();
        self.resources.revision += 1;
        ResourceResult::Cancelled
    }
    /// A stale result is discarded, including its inventory and hardware identity.
    pub fn complete_resource_task(&mut self, id: u64, result: r::ResourceResult<ResourceCompletion>) -> bool {
        if !self.resources.active.as_ref().is_some_and(|t| t.id == id) {
            return false;
        }
        let task = self.resources.active.take().expect("checked task");
        self.resources.view.detecting = false;
        let (status, key, detail) = match result {
            Ok(done) => {
                let terminal = done
                    .validation
                    .as_ref()
                    .map(|(_, v)| match v.status {
                        r::ValidationStatus::Active => ("done", "AutoSetup.validationRunning"),
                        r::ValidationStatus::Valid => ("done", "AutoSetup.validationPassed"),
                        r::ValidationStatus::TimedOut => ("failed", "AutoSetup.validationTimedOut"),
                        r::ValidationStatus::WrongArchitecture => ("failed", "AutoSetup.validationWrongArchitecture"),
                        r::ValidationStatus::MissingDependency => ("failed", "AutoSetup.validationMissingDependency"),
                        r::ValidationStatus::StartFailed => ("failed", "AutoSetup.validationStartFailed"),
                    })
                    .unwrap_or(("done", task.success_key));
                let terminal_detail = done
                    .cuda
                    .as_ref()
                    .filter(|p| !p.passed)
                    .map(|p| p.detail.as_str())
                    .or_else(|| done.validation.as_ref().map(|(_, v)| v.detail.as_str()))
                    .map(|s| safe_detail(s, &self.resources.store))
                    .unwrap_or_else(|| {
                        done.cache_bytes
                            .map(|bytes| format!("{bytes} bytes"))
                            .unwrap_or_default()
                    });
                self.resources.weight_headers.extend(done.weight_headers);
                if let Some(facts) = done.machine {
                    self.set_measured_hardware(
                        facts
                            .best_gpu()
                            .and_then(|g| Some((g.name.clone(), g.driver.clone()?, g.memory_mib?))),
                    );
                    self.resources.view.recommendation = Some(r::recommend(&facts, &self.resources.catalog));
                    self.resources.view.machine = Some(facts);
                }
                self.resources.view.inventory = done.inventory;
                self.resources.view.acceleration = done.acceleration;
                if let Some(online) = done.online {
                    self.resources.view.online_weights = online;
                    self.resources.view.remote_status = "done".into();
                }
                if let Some((exe, mut validation)) = done.validation {
                    validation.detail = safe_detail(&validation.detail, &self.resources.store);
                    self.resources.view.verification_executable = Some(exe.clone());
                    self.resources.verified.insert(exe, validation.clone());
                    self.resources.view.verification = Some(validation);
                }
                self.resources.view.cuda_probe = done.cuda.map(|mut p| {
                    p.detail = safe_detail(&p.detail, &self.resources.store);
                    p
                });
                if let Some((engine, weight)) = done.chosen {
                    self.resources.view.chosen_engine = Some(engine);
                    self.resources.view.chosen_weight = Some(weight);
                }
                if done.runtime_present {
                    self.engines.set_library_dirs(vec![self.resources.store.runtime_dir()]);
                }
                (terminal.0, terminal.1, terminal_detail)
            }
            Err(r::ResourceError::Cancelled) => ("cancelled", "AutoSetup.downloadCancelled", String::new()),
            Err(e) => {
                let key = match &e {
                    _ if task.view.kind == "refresh" => "AutoSetup.remoteRefreshFailedCached",
                    r::ResourceError::ImportMismatch => "AutoSetup.importPackageMismatch",
                    r::ResourceError::Integrity { .. } => "AutoSetup.weightChecksumFailed",
                    _ => "AutoSetup.failed",
                };
                if task.view.kind == "refresh" {
                    self.resources.view.remote_status = "AutoSetup.remoteRefreshFailedCached".into();
                }
                ("failed", key, safe_detail(&e.to_string(), &self.resources.store))
            }
        };
        self.resources.view.last_outcome = Some(ResourceOutcome {
            task: id,
            status: status.into(),
            message_key: key.into(),
            detail: detail.clone(),
        });
        self.log(
            status == "failed",
            format!("resource task {id} {status}: {key} {detail}"),
        );
        self.resources.revision += 1;
        true
    }
    /// Plan uses already-read supporting metadata so there is no filesystem access under lock.
    pub fn plan_resource_setup(&mut self, mut choice: r::SetupChoice) -> ResourceResult {
        choice.set_as_default = false;
        choice.switch_now = false;
        self.resources.revision += 1;
        if let Some(name) = &choice.model_name {
            self.resources
                .weight_headers
                .insert(choice.weight.to_string_lossy().into_owned(), name.clone());
        }
        if let Some(id) = &choice.update_profile_id {
            if self
                .config
                .engine
                .engines
                .iter()
                .find(|p| &p.id == id)
                .is_some_and(|p| !r::is_managed(p))
            {
                choice.update_profile_id = None;
            }
        }
        let plan = match r::plan_setup(&self.config.engine.engines, &choice, self.resources.store.root()) {
            Ok(p) => p,
            Err(_) => return refused("AutoSetup.setupInvalid"),
        };
        self.resources.next += 1;
        let id = self.resources.next;
        let view = ResourcePlanView {
            id,
            replaces_profile_id: plan.replaces_profile_id.clone(),
            name: plan.profile.name.clone(),
            command: plan.profile.command.clone(),
            is_default: plan.profile.is_default,
        };
        self.resources.plan = Some(Plan {
            id,
            snapshot: self.config.engine.engines.clone(),
            value: plan,
        });
        ResourceResult::Plan { plan: view }
    }
    pub fn resource_verified_version(&self, executable: &str) -> Option<String> {
        self.resources
            .verified
            .get(executable)
            .filter(|v| matches!(v.status, r::ValidationStatus::Valid | r::ValidationStatus::Active))
            .and_then(|v| v.version.clone())
            .or_else(|| {
                let id = self.engines.foreground()?;
                if self.engines.phase(id) != Some(Phase::Ready) || !self.engines.is_katago(id) {
                    return None;
                }
                let profile = self.engines.profile_of(id)?;
                if split_command(&profile.command)
                    .first()
                    .is_none_or(|program| program != executable)
                {
                    return None;
                }
                self.engines
                    .identity(id)
                    .map(|(_, version, _)| version)
                    .filter(|version| !version.is_empty())
            })
    }
    /// Header identity of a weight already read: installed inventory, or the off-lock header cache.
    pub(super) fn weight_model_name(&self, path: &str) -> Option<String> {
        self.resources
            .view
            .inventory
            .weights
            .iter()
            .find(|i| i.path == Path::new(path))
            .and_then(|w| w.model_name.clone())
            .or_else(|| self.resources.weight_headers.get(path).cloned())
    }
    /// Saved profiles' weights whose model header has not been read yet (read off-lock by the shell).
    pub fn unread_weight_headers(&self) -> Vec<String> {
        self.tuning_view()
            .profiles
            .into_iter()
            .map(|p| p.model)
            .filter(|m| !m.is_empty() && self.weight_model_name(m).is_none())
            .collect()
    }
    pub fn record_weight_headers(&mut self, headers: BTreeMap<String, String>) {
        self.resources.weight_headers.extend(headers);
        self.resources.revision += 1;
        self.refresh_tuning();
    }
    pub fn apply_resource_setup(
        &mut self,
        id: u64,
        make_default: bool,
        switch_now: bool,
    ) -> Result<ResourceResult, ApiError> {
        if self.resource_task_active() || self.benchmark_running() {
            return Ok(refused("EngineThreadPolicy.benchmarkBusy"));
        }
        let Some(plan) = self.resources.plan.as_ref().filter(|p| p.id == id) else {
            return Ok(refused("AutoSetup.planStale"));
        };
        if plan.snapshot != self.config.engine.engines {
            return Ok(refused("AutoSetup.planStale"));
        }
        let mut profile = plan.value.profile.clone();
        let index = if let Some(target) = &plan.value.replaces_profile_id {
            let Some(i) = self
                .config
                .engine
                .engines
                .iter()
                .position(|p| &p.id == target && r::is_managed(p))
            else {
                return Ok(refused("AutoSetup.unmanagedProfile"));
            };
            i
        } else {
            self.config.engine.engines.len()
        };
        let updating = index < self.config.engine.engines.len();
        if make_default {
            profile.is_default = true;
        }
        self.commit_config(|c| {
            if make_default {
                for p in &mut c.engine.engines {
                    p.is_default = false;
                }
            }
            if updating {
                c.engine.engines[index] = profile;
            } else {
                c.engine.engines.push(profile);
            }
        })?;
        self.resources.plan = None;
        self.resources.revision += 1;
        if switch_now {
            if updating {
                self.restart_engine_if_resource_profile(index)?;
            } else {
                self.switch_engine(index as u32)?;
            }
        }
        Ok(ResourceResult::Done)
    }
    fn restart_engine_if_resource_profile(&mut self, index: usize) -> Result<(), ApiError> {
        let id = &self.config.engine.engines[index].id;
        if self
            .engines
            .foreground()
            .and_then(|i| self.engines.profile_of(i))
            .is_some_and(|p| &p.id == id)
        {
            self.restart_engine()?;
        } else {
            self.switch_engine(index as u32)?;
        }
        Ok(())
    }
    pub fn set_resource_proxy(&mut self, settings: NetworkSettings) -> Result<ResourceResult, ApiError> {
        if settings.proxy_mode == ProxyMode::Manual {
            if settings.proxy_host.trim().is_empty() {
                return Ok(refused("ConfigDialog2.modern.proxy.hostRequired"));
            }
            if settings.proxy_port == 0 {
                return Ok(refused("ConfigDialog2.modern.proxy.invalidPort"));
            }
        }
        self.commit_config(|c| c.network = settings)?;
        self.resources.revision += 1;
        Ok(ResourceResult::Done)
    }
    pub fn resource_repair_context(&self, command: &str) -> Option<ResourceRepairView> {
        let p = self
            .config
            .engine
            .engines
            .iter()
            .find(|p| p.command == command && r::is_managed(p))?;
        let asset = self.resources.catalog.assets.get(&p.managed.as_ref()?.asset_id)?;
        if !cfg!(windows) || !matches!(asset.backend.as_str(), "cuda" | "tensorrt") {
            return None;
        }
        let row = self
            .resources
            .view
            .acceleration
            .iter()
            .find(|v| v.backend == asset.backend && !v.missing.is_empty())?;
        Some(ResourceRepairView {
            profile_id: p.id.clone(),
            backend: asset.backend.clone(),
            missing: row.missing.clone(),
            message_key: "EngineFailedMessage.openTensorRtRepair".into(),
        })
    }
    pub fn resource_snapshot(&self) -> ResourcesView {
        self.resources_view()
    }
    pub(super) fn resources_view(&self) -> ResourcesView {
        let mut v = self.resources.view.clone();
        v.task = self.resources.active.as_ref().map(|t| t.view.clone());
        v.network = self.config.network.clone();
        v.startup_needs_repair = self.config.engine.startup != EngineStartup::Empty
            && !self.config.engine.engines.iter().any(|p| !p.command.trim().is_empty());
        let foreground = self.engines.foreground().and_then(|i| self.engines.profile_of(i));
        let p = foreground.or_else(|| {
            self.config
                .engine
                .default_index()
                .and_then(|i| self.config.engine.engines.get(i))
        });
        if let Some(p) = p {
            let parts = split_command(&p.command);
            let arg = |name: &str| parts.windows(2).find(|a| a[0] == name).map(|a| a[1].clone());
            let weight = arg("-model");
            let model_name = weight.as_deref().and_then(|w| self.weight_model_name(w));
            v.current_setup = ResourceSetupView {
                profile_id: Some(p.id.clone()),
                executable: parts.first().cloned(),
                engine_version: parts
                    .first()
                    .and_then(|executable| self.resource_verified_version(executable)),
                weight,
                model_name: model_name.clone(),
                b11: model_name.as_deref().is_some_and(r::is_b11_model),
                config: arg("-config"),
                analysis_config: v
                    .inventory
                    .analysis_config
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                managed: r::is_managed(p),
                discovery_source: if foreground.is_some() { "foreground" } else { "default" }.into(),
            };
        }
        v
    }
    pub(super) fn take_resources_view(&mut self, full: bool) -> Option<ResourcesView> {
        let fg = self
            .engines
            .foreground()
            .and_then(|i| self.engines.profile_of(i))
            .map(|p| p.id.clone());
        let version = self
            .engines
            .foreground()
            .and_then(|id| self.engines.profile_of(id))
            .and_then(|profile| split_command(&profile.command).into_iter().next())
            .and_then(|executable| self.resource_verified_version(&executable));
        if self.resources.projected_version != version {
            self.resources.projected_version = version;
            self.resources.revision += 1;
        }
        if self.resources.projected_startup != Some(self.config.engine.startup) {
            self.resources.projected_startup = Some(self.config.engine.startup);
            self.resources.revision += 1;
        }
        if self.resources.projected_profiles != self.config.engine.engines || self.resources.projected_foreground != fg
        {
            self.resources.projected_profiles = self.config.engine.engines.clone();
            self.resources.projected_foreground = fg;
            self.resources.revision += 1;
        }
        if !full && self.resources.sent == Some(self.resources.revision) {
            return None;
        }
        self.resources.sent = Some(self.resources.revision);
        let v = self.resources_view();
        self.resources.view.last_outcome = None;
        Some(v)
    }
}
/// Reads user-selected paths outside the workbench lock before pure profile planning.
pub fn prepare_resource_setup(
    engine: ResourceEngineChoice,
    weight: String,
    version: Option<String>,
) -> r::ResourceResult<r::SetupChoice> {
    let executable = PathBuf::from(&engine.executable);
    let config = PathBuf::from(&engine.config);
    let weight = PathBuf::from(weight);
    if !executable.is_file() || !config.is_file() || !weight.is_file() {
        return Err(r::ResourceError::Invalid("Selected setup file is missing".into()));
    }
    if version.is_none() {
        return Err(r::ResourceError::Invalid(
            "Verify the selected executable before planning setup".into(),
        ));
    }
    let model_name = r::read_weight_model_name(&weight);
    let model_display_name = model_name
        .clone()
        .unwrap_or_else(|| weight.file_name().unwrap_or_default().to_string_lossy().into_owned());
    Ok(r::SetupChoice {
        asset_id: engine.asset_id,
        executable,
        weight,
        config,
        model_display_name,
        model_name,
        engine_version: version,
        experimental_backend: engine.backend,
        update_profile_id: engine.update_profile_id,
        set_as_default: false,
        switch_now: false,
    })
}
fn safe_detail(text: &str, store: &r::ResourceStore) -> String {
    let sanitized = crate::sanitize::sanitize(text);
    let root = store.root().to_string_lossy();
    sanitized
        .split_whitespace()
        .map(|s| {
            if (s.contains('/') || s.contains('\\')) && !s.starts_with(root.as_ref()) {
                "[path-or-url]"
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(512)
        .collect()
}
fn fetch_text(fetch: &dyn r::Fetch, url: &str) -> r::ResourceResult<String> {
    let mut response = fetch.get(url, None)?;
    if response.status != 200 {
        return Err(r::ResourceError::Http(response.status));
    }
    let mut body = String::new();
    response
        .body
        .by_ref()
        .take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut body)?;
    if body.len() > 4 * 1024 * 1024 {
        return Err(r::ResourceError::Invalid("Remote metadata too large".into()));
    }
    Ok(body)
}
/// Executes the real acquisition/verification seam with either native or deterministic adapters.
pub fn run_resource_job(
    job: &ResourceJob,
    fetch: &dyn r::Fetch,
    probe: &dyn r::Probe,
    mut progress: impl FnMut(&str, u64, Option<u64>),
) -> r::ResourceResult<ResourceCompletion> {
    if job.cancel.load(Ordering::Acquire) {
        return Err(r::ResourceError::Cancelled);
    }
    let mut out = ResourceCompletion::default();
    let download_item =
        |item: &ResourceItem, progress: &mut dyn FnMut(&str, u64, Option<u64>)| -> r::ResourceResult<()> {
            match item {
                ResourceItem::Package { asset, backend, .. } => {
                    let a = job.catalog.asset(asset)?;
                    let spec = r::DownloadSpec {
                        url: job.catalog.asset_url(a)?,
                        file_name: a.asset_name.clone(),
                        size: Some(a.size_bytes),
                        sha256: Some(a.sha256.clone()),
                    };
                    let d = r::download(fetch, &job.store.downloads_dir(), &spec, &job.cancel, |p| {
                        progress("downloading", p.bytes, p.total)
                    })?;
                    progress("installing", d.size, Some(d.size));
                    job.store.install_package(
                        &job.catalog,
                        asset,
                        &d.path,
                        r::ResourceSource::Download,
                        *backend,
                        &job.cancel,
                    )?;
                }
                ResourceItem::Model { model } => {
                    let m = job.catalog.model(model)?;
                    let spec = r::DownloadSpec {
                        url: job.catalog.model_url(m),
                        file_name: m.file_name.clone(),
                        size: Some(m.size_bytes),
                        sha256: Some(m.sha256.clone()),
                    };
                    let d = r::download(fetch, &job.store.weights_dir(), &spec, &job.cancel, |p| {
                        progress("downloading", p.bytes, p.total)
                    })?;
                    job.store.register_weight(&job.catalog, &d)?;
                }
                ResourceItem::OnlineWeight { name } => {
                    let m = job
                        .online
                        .iter()
                        .find(|m| &m.name == name)
                        .ok_or_else(|| r::ResourceError::Invalid("Network is not in the official cache".into()))?;
                    let spec = r::DownloadSpec {
                        url: m.download_url.clone(),
                        file_name: m.file_name(),
                        size: m.size_bytes,
                        sha256: m.sha256.clone(),
                    };
                    let d = r::download(fetch, &job.store.weights_dir(), &spec, &job.cancel, |p| {
                        progress("downloading", p.bytes, p.total)
                    })?;
                    job.store.register_weight(&job.catalog, &d)?;
                }
                ResourceItem::NvidiaRuntime { backend, .. } => {
                    let cuda = fetch_text(fetch, r::CUDA_MANIFEST_URL)?;
                    let cudnn = fetch_text(fetch, r::CUDNN_MANIFEST_URL)?;
                    let packages = r::nvidia_runtime_packages(&cuda, &cudnn, backend)?;
                    let mut archives = Vec::new();
                    for p in packages {
                        let d = r::download(
                            fetch,
                            &job.store.runtime_downloads_dir(),
                            &p.download_spec()?,
                            &job.cancel,
                            |p| progress("downloading", p.bytes, p.total),
                        )?;
                        archives.push((p, d.path));
                    }
                    progress("installing", 0, None);
                    job.store.install_nvidia_runtime(&archives, &job.cancel)?;
                }
            }
            Ok(())
        };
    match &job.operation {
        ResourceCommand::Inspect { refresh } => {
            out.machine = if *refresh || job.machine.is_none() {
                Some(r::detect_machine(probe, std::env::consts::OS, std::env::consts::ARCH))
            } else {
                job.machine.clone()
            };
        }
        ResourceCommand::Download { item } => download_item(item, &mut progress)?,
        ResourceCommand::RepairTensorRt { .. } => {
            download_item(
                &ResourceItem::Package {
                    asset: "windows-nvidia".into(),
                    confirmed: true,
                    backend: None,
                },
                &mut progress,
            )?;
            download_item(
                &ResourceItem::Package {
                    asset: "windows-tensorrt".into(),
                    confirmed: true,
                    backend: None,
                },
                &mut progress,
            )?;
            download_item(
                &ResourceItem::NvidiaRuntime {
                    backend: "tensorrt".into(),
                    confirmed: true,
                },
                &mut progress,
            )?;
        }
        ResourceCommand::ImportPackage {
            path: Some(path),
            backend,
            confirmed,
        } => {
            let platform = job
                .machine
                .as_ref()
                .and_then(|m| m.platform())
                .ok_or_else(|| r::ResourceError::Invalid("Detect the machine before importing".into()))?;
            if !confirmed {
                let size = std::fs::metadata(path)?.len();
                let experimental = job
                    .catalog
                    .assets_for_platform(platform)
                    .filter(|a| a.size_bytes == size && a.release_tier == r::SupportTier::Experimental)
                    .collect::<Vec<_>>();
                if !experimental.is_empty() {
                    let digest = r::sha256_file(Path::new(path))?;
                    if experimental.iter().any(|a| a.sha256.eq_ignore_ascii_case(&digest)) {
                        return Err(r::ResourceError::Invalid("AutoSetup.experimentalBackendConfirm".into()));
                    }
                }
            }
            job.store
                .import_package(&job.catalog, platform, Path::new(path), *backend, &job.cancel)?;
        }
        ResourceCommand::ImportWeight { path: Some(path) } => {
            job.store.import_weight(&job.catalog, Path::new(path), &job.cancel)?;
        }
        ResourceCommand::Verify { target, weight } => {
            verify_target(job, probe, target, weight.as_deref(), &mut out);
        }
        ResourceCommand::ChooseLocalEngine { choice } => {
            let engine = ResourceEngineChoice {
                asset_id: "local".into(),
                executable: choice
                    .executable
                    .clone()
                    .ok_or_else(|| r::ResourceError::Invalid("Executable selection missing".into()))?,
                config: choice
                    .config
                    .clone()
                    .ok_or_else(|| r::ResourceError::Invalid("Config selection missing".into()))?,
                backend: None,
                update_profile_id: None,
            };
            let weight = choice
                .weight
                .clone()
                .ok_or_else(|| r::ResourceError::Invalid("Weight selection missing".into()))?;
            verify_target(job, probe, &engine, Some(&weight), &mut out);
            out.chosen = Some((engine, weight));
        }
        ResourceCommand::RefreshRemoteWeights => {
            let strongest = r::parse_official_strongest(&fetch_text(fetch, r::STRONGEST_URL)?)?;
            let mut rows = r::parse_official_weight_table(&fetch_text(fetch, r::NETWORKS_URL)?)?;
            rows.retain(|w| w.name != strongest.name);
            rows.insert(0, strongest);
            out.online = Some(rows);
        }
        ResourceCommand::RepairAnalysisConfig => {
            job.store.repair_analysis_config()?;
        }
        ResourceCommand::CleanNvidiaCache { confirmed } => {
            out.cache_bytes = Some(if *confirmed {
                job.store.clean_runtime_cache(true)?
            } else {
                job.store.runtime_cache_size()?
            });
        }
        _ => {
            return Err(r::ResourceError::Invalid(
                "Unsupported off-lock resource operation".into(),
            ))
        }
    }
    if job.cancel.load(Ordering::Acquire)
        && matches!(
            job.operation,
            ResourceCommand::Inspect { .. }
                | ResourceCommand::Verify { .. }
                | ResourceCommand::ChooseLocalEngine { .. }
                | ResourceCommand::RefreshRemoteWeights
        )
    {
        return Err(r::ResourceError::Cancelled);
    }
    for weight in job.current_weight.iter().chain(out.chosen.iter().map(|(_, w)| w)) {
        if let Some(name) = r::read_weight_model_name(Path::new(weight)) {
            out.weight_headers.insert(weight.clone(), name);
        }
    }
    out.inventory = job.store.inventory()?;
    out.acceleration = acceleration(&job.store, out.machine.as_ref().or(job.machine.as_ref()))?;
    out.runtime_present = job.store.runtime_dir().is_dir();
    Ok(out)
}
fn verify_target(
    job: &ResourceJob,
    probe: &dyn r::Probe,
    target: &ResourceEngineChoice,
    weight: Option<&str>,
    out: &mut ResourceCompletion,
) {
    let backend = job
        .catalog
        .assets
        .get(&target.asset_id)
        .map(|a| a.backend.as_str())
        .unwrap_or("");
    let active = job
        .active_executable
        .as_ref()
        .filter(|(exe, _)| exe == &target.executable);
    let mut validation = r::verify_engine(
        probe,
        Path::new(&target.executable),
        matches!(backend, "cuda" | "tensorrt"),
        active.is_some(),
    );
    if let Some((_, version)) = active {
        validation.version = Some(version.clone());
    }
    if active.is_none()
        && matches!(validation.status, r::ValidationStatus::Valid)
        && backend == "cuda"
        && job
            .machine
            .as_ref()
            .is_some_and(|m| m.cuda_compatibility == r::CudaCompatibility::ProbeRequired)
    {
        out.cuda = Some(if let Some(weight) = weight {
            r::verify_cuda(
                probe,
                Path::new(&target.executable),
                Path::new(&target.config),
                Path::new(weight),
            )
        } else {
            r::CudaProbeResult {
                passed: false,
                timed_out: false,
                detail: "CUDA probe requires a weight".into(),
            }
        });
    }
    if out.cuda.as_ref().is_some_and(|p| !p.passed) {
        validation.status = r::ValidationStatus::MissingDependency;
    }
    out.validation = Some((target.executable.clone(), validation));
}
fn acceleration(
    store: &r::ResourceStore,
    machine: Option<&r::MachineFacts>,
) -> r::ResourceResult<Vec<ResourceAccelerationView>> {
    let windows = machine.map_or(cfg!(windows), |m| m.os == "windows");
    let names = std::fs::read_dir(store.runtime_dir())
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().to_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let cache = store.runtime_cache_size()?;
    let mut rows = Vec::new();
    for backend in ["cuda", "tensorrt"] {
        let mut required = vec![
            "cudart64_12.dll",
            "cublas64_12.dll",
            "cublaslt64_12.dll",
            "cudnn64_9.dll",
            "nvjitlink",
            "nvrtc64_120_0.dll",
            "nvrtc-builtins64_128.dll",
        ];
        if backend == "tensorrt" {
            required.extend(["nvinfer_10.dll", "nvinfer_plugin_10.dll", "nvonnxparser_10.dll"]);
        }
        rows.push(ResourceAccelerationView {
            backend: backend.into(),
            applicable: windows,
            missing: if windows {
                required
                    .into_iter()
                    .filter(|p| !names.iter().any(|n| n.starts_with(p)))
                    .map(str::to_string)
                    .collect()
            } else {
                Vec::new()
            },
            cache_bytes: cache,
            experimental: false,
        });
    }
    if let Some(facts) = machine.filter(|m| m.os == "linux") {
        if let Some(cuda) = rows.iter_mut().find(|r| r.backend == "cuda") {
            cuda.missing = match &facts.loader_libraries {
                Some(found) => r::LINUX_CUDA_LIBRARIES
                    .iter()
                    .filter(|lib| !found.iter().any(|f| f == **lib))
                    .map(|s| s.to_string())
                    .collect(),
                None => vec!["loader-cache-unknown".into()],
            };
        }
    }
    for backend in ["directml", "openvino", "rocm"] {
        rows.push(ResourceAccelerationView {
            backend: backend.into(),
            applicable: windows,
            missing: Vec::new(),
            cache_bytes: 0,
            experimental: true,
        });
    }
    Ok(rows)
}
