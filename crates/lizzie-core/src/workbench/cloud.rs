//! Cloud account projection and provider switching; the shell owns credentials and network IO.
use super::Workbench;
use crate::api::{ApiError, EngineScope, ErrorKind, ZhiziConnection};
use crate::cloud::{CloudSettings, Provider};
use crate::engine::cloud::{CloudLaunch, CloudProvider};
use crate::engine::profile::{new_profile_id, EngineProfile};
use crate::engine::runtime::{InstanceId, Phase};

/// [`Workbench::disconnect_zhizi`]'s refusal while a 智子 engine is busy.
pub const ZHIZI_BUSY: &str = "RemoteCompute.zhiziSession.blocked";
/// A 智子 activation requested before a 断开智子引擎 and committed after it.
pub const ZHIZI_ACTIVATION_RETIRED: &str = "RemoteCompute.zhiziSession.disconnected";

/// A remote-compute switch request, taken when it begins, before any work outside the workbench
/// (the shell reads the account first). A 断开智子引擎 in between retires it, so a 智子 engine is
/// not connected again by a request already under way (Java `RemoteComputeEngineSwitch.cancelZhizi`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloudTicket(u64);

fn is_zhizi(profile: &EngineProfile) -> bool {
    profile.cloud.as_ref().is_some_and(|c| c.provider == CloudProvider::Zhizi)
}

/// Every running 智子 engine among `running` (the runtime's instances): the foreground and
/// secondary ones, those fast engine change keeps, and those whose profile is no longer listed
/// (Java `zhiziConnectionTargets`). Analysis processes are not among them.
fn zhizi_engines<'a>(running: impl IntoIterator<Item = (InstanceId, &'a EngineProfile, Phase)>) -> Vec<InstanceId> {
    running.into_iter().filter(|(_, profile, _)| is_zhizi(profile)).map(|(id, _, _)| id).collect()
}

impl Workbench {
    /// Commit account-owned fields. Only catalog refresh supplies its original saved args;
    /// compare-before-write keeps a later provider activation's launch parameters intact.
    pub fn cloud_account_settings(&mut self, settings: &CloudSettings, catalog_args_if_unchanged: Option<&str>) -> Result<(), ApiError> {
        self.commit_config(|c| {
            c.cloud.identifier = settings.identifier.clone();
            c.cloud.remember_token = settings.remember_token;
            c.cloud.remember_password = settings.remember_password;
            if let Some(expected) = catalog_args_if_unchanged {
                c.cloud.catalog = settings.catalog.clone();
                c.cloud.catalog_updated_at = settings.catalog_updated_at;
                if c.cloud.args == expected {
                    c.cloud.args = settings.args.clone();
                }
            }
        })
    }

    /// Update session credential availability without changing the configured provider or engine.
    /// Returns the previous availability so a failed import can restore its staged session state.
    pub fn set_cloud_authenticated(&mut self, authenticated: bool) -> bool {
        std::mem::replace(&mut self.cloud_authenticated, authenticated)
    }

    pub(super) fn first_usable_local_engine(&self) -> Option<usize> {
        self.config.engine.engines.iter().position(|profile| {
            !profile.is_remote()
                && crate::engine::gtp::prepare_launch(&profile.command, self.engines.roots())
                    .is_some_and(|spec| usable_local(&spec))
        })
    }

    /// A ticket for an activation that begins now ([`Self::activate_cloud`]).
    pub fn cloud_ticket(&self) -> CloudTicket {
        CloudTicket(self.zhizi_disconnects)
    }

    /// Commit the provider and give the foreground seat to its engine profile; returns that
    /// profile's id. The outcome (ready or failed) arrives in later views. A 智子 activation whose
    /// `ticket` a 断开智子引擎 retired is refused with [`ZHIZI_ACTIVATION_RETIRED`].
    pub fn activate_cloud(&mut self, ticket: CloudTicket, draft: CloudSettings, confirmed: bool) -> Result<String, ApiError> {
        if !confirmed {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "RemoteCompute.error.activationRequired"));
        }
        if draft.provider == Provider::Zhizi && ticket != self.cloud_ticket() {
            return Err(ApiError::new(ErrorKind::Unavailable, ZHIZI_ACTIVATION_RETIRED));
        }
        if self.setup_mode() || self.match_active() || self.game_active() || self.human_sl_active()
            || self.foreground_lifecycle_held().is_some()
        {
            return Err(ApiError::new(ErrorKind::Unavailable, "RemoteCompute.error.engineSwitchFailed"));
        }
        let mut settings = self.config.cloud.clone();
        settings.provider = draft.provider;
        settings.args = draft.args;
        settings.custom_url = draft.custom_url.trim().replace(['\r', '\n'], "");
        let (provider, command) = match settings.provider {
            Provider::Zhizi => (CloudProvider::Zhizi, "remote-compute://zhizi"),
            Provider::Custom => {
                let url = url::Url::parse(&settings.custom_url)
                    .map_err(|_| ApiError::new(ErrorKind::InvalidArgument, "RemoteCompute.error.linkScheme"))?;
                if !matches!(url.scheme(), "ws" | "wss") || url.host_str().is_none() {
                    return Err(ApiError::new(ErrorKind::InvalidArgument, "RemoteCompute.error.linkScheme"));
                }
                (CloudProvider::Custom, "remote-compute://custom-websocket")
            }
            Provider::Local => return self.cloud_local(),
        };
        // Java createOrUpdateZhizi/CustomEngine store the display name as the profile name.
        let name = cloud_display_name(self.active_language, &settings);
        let mut profiles = self.config.engine.engines.clone();
        let index = profiles.iter().position(|p| p.cloud.as_ref().is_some_and(|c| c.provider == provider));
        let mut profile = index.map(|i| profiles[i].clone()).unwrap_or_else(|| EngineProfile {
            id: new_profile_id(&profiles), ..EngineProfile::default()
        });
        profile.command = command.into();
        profile.name = name;
        profile.ssh = None;
        profile.cloud = Some(CloudLaunch { provider, identifier: settings.identifier.clone(),
            args: settings.args.clone(), url: settings.custom_url.clone() });
        profile.preload = false;
        profile.width = self.doc.record.size.width;
        profile.height = self.doc.record.size.height;
        profile.komi = 7.5;
        profile.initial_command.clear();
        for p in &mut profiles { p.is_default = false; }
        profile.is_default = true;
        let id = profile.id.clone();
        let index = match index {
            Some(i) => { profiles[i] = profile; i }
            None => { profiles.push(profile); profiles.len() - 1 }
        };
        self.save_engine_settings_with_cloud(profiles, self.config.engine.startup, Some(settings))?;
        self.cloud_activation = Some(id.clone());
        self.switch_engine(index as u32)?;
        self.cloud_activation_settle();
        Ok(id)
    }

    /// Java chooses the first usable local engine, not an SSH or another cloud profile. Returns
    /// the chosen profile's id, like [`Self::activate_cloud`].
    pub fn cloud_local(&mut self) -> Result<String, ApiError> {
        if self.setup_mode() || self.match_active() || self.game_active() || self.human_sl_active()
            || self.foreground_lifecycle_held().is_some()
        {
            return Err(ApiError::new(ErrorKind::Unavailable, "RemoteCompute.error.engineSwitchFailed"));
        }
        let index = self.first_usable_local_engine()
            .ok_or_else(|| ApiError::new(ErrorKind::Unavailable, "RemoteCompute.error.localMissing"))?;
        let mut profiles = self.config.engine.engines.clone();
        for (i, p) in profiles.iter_mut().enumerate() { p.is_default = i == index; }
        let id = profiles[index].id.clone();
        let mut settings = self.config.cloud.clone();
        settings.provider = Provider::Local;
        self.save_engine_settings_with_cloud(profiles, self.config.engine.startup, Some(settings))?;
        self.cloud_activation = Some(id.clone());
        self.switch_engine(index as u32)?;
        self.cloud_activation_settle();
        Ok(id)
    }

    pub fn cancel_cloud_start(&mut self) -> bool {
        let pending = self.cloud_activation.take();
        let Some(fg) = self.engines.foreground() else { return false };
        if pending.as_deref() != self.engines.profile_of(fg).map(|p| p.id.as_str())
            || self.engines.phase(fg) != Some(Phase::Starting) {
            return false;
        }
        self.shutdown_engines(EngineScope::Current)
    }

    /// The remote compute switch succeeded once the new foreground engine is ready and holds the
    /// position wished for it (its board sync confirmed): only then the analysis process may
    /// preload (`preloadQuickAnalysisEngineForKifuBrowsing` after the switch).
    pub(super) fn cloud_activation_settle(&mut self) {
        let Some(id) = self.cloud_activation.as_deref() else { return };
        let Some(fg) = self.engines.foreground() else { return };
        if self.engines.profile_of(fg).is_some_and(|p| p.id == id)
            && self.engines.phase(fg) == Some(Phase::Ready)
            && self.engines.holds_wish(fg)
        {
            self.cloud_activation = None;
            self.preload_analysis_engine();
        }
    }

    /// Whether 断开智子引擎 must wait (Java `zhiziSelectionBusy` and `Leelaz.zhiziConnectionBusy`):
    /// one of `engines` is starting, restoring after a restart or held by a task, the remote
    /// compute switch to one has not completed (its first board sync neither confirmed nor
    /// refused), a secondary switch still starting may fall back to or from one, or an engine
    /// match, also while it retires, has a 智子 participant.
    fn zhizi_busy(&self, engines: &[InstanceId]) -> bool {
        let switching = self.secondary_fallback.filter(|(new, _)| self.engines.phase(*new) == Some(Phase::Starting));
        let activating = self.cloud_activation.as_deref().is_some_and(|pending| {
            self.engines.foreground()
                .filter(|&fg| engines.contains(&fg) && !self.engines.complete(fg) && !self.engines.sync_refused(fg))
                .and_then(|fg| self.engines.profile_of(fg))
                .is_some_and(|p| p.id == pending)
        });
        activating
            || engines.iter().any(|&id| self.engines.occupied(id) || self.restart_restores(id))
            || switching.is_some_and(|(new, previous)| engines.contains(&new) || engines.contains(&previous))
            || self.match_uses(is_zhizi)
    }

    /// The 主/对比智子引擎 connections, whatever engine or account is chosen (Java
    /// `EngineManager.zhiziConnectionState`), from the `running` instances the engine view lists.
    pub(super) fn zhizi_connection(&self, running: &[(InstanceId, &EngineProfile, Phase)]) -> ZhiziConnection {
        let engines = zhizi_engines(running.iter().copied());
        if self.zhizi_busy(&engines) {
            ZhiziConnection::Busy
        } else if engines.is_empty() {
            ZhiziConnection::Disconnected
        } else {
            ZhiziConnection::Connected
        }
    }

    /// 断开智子引擎 (Java `EngineManager.disconnectZhiziEngines`): ends every running 智子 engine
    /// and nothing else. A seat one held is left empty, as Java's selection rolls back to no
    /// current engine; local and other remote engines keep their connections and seats. Remote
    /// compute switches already under way are retired, so none connects 智子 again; only a new
    /// explicit choice does. Closing the connection does not establish that the service stopped
    /// billing. Returns [`ZhiziConnection::Busy`] and changes nothing while
    /// [`Self::zhizi_connection`] is busy, else [`ZhiziConnection::Disconnected`].
    pub fn disconnect_zhizi(&mut self) -> ZhiziConnection {
        let engines = zhizi_engines(self.engines.running_profiles());
        if self.zhizi_busy(&engines) {
            return ZhiziConnection::Busy;
        }
        self.zhizi_disconnects += 1;
        let pending_zhizi = self.cloud_activation.as_ref()
            .is_some_and(|id| self.config.engine.engines.iter().any(|p| p.id == *id && is_zhizi(p)));
        if pending_zhizi {
            self.cloud_activation = None;
        }
        if engines.is_empty() {
            return ZhiziConnection::Disconnected;
        }
        if self.engines.foreground().is_some_and(|f| engines.contains(&f)) {
            self.auto_analysis_abandon();
            self.quick_engine_changed();
        }
        for id in engines {
            self.engines.terminate(id);
        }
        self.drain_engine_events();
        self.sync_engine();
        ZhiziConnection::Disconnected
    }
}

/// RemoteComputeConfig.displayNameForZhiziArgs / displayNameForCustomWebSocketUrl. A custom link
/// shows only its host and explicit port: the path, query and user info may hold credentials.
fn cloud_display_name(language: crate::settings::Language, settings: &CloudSettings) -> String {
    use super::exporting::java_text;
    if settings.provider == Provider::Custom {
        let custom = java_text(language, "RemoteCompute.custom");
        let rest = settings.custom_url.split_once("://").map_or("", |(_, rest)| rest);
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        let host_port = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
        let (host, port) = match host_port.rfind(':') {
            Some(at) if !host_port[at..].contains(']') => (&host_port[..at], host_port[at + 1..].parse::<u32>().ok()),
            _ => (host_port, None),
        };
        if host.trim_matches(['[', ']']).trim().is_empty() { return custom; }
        return match port.filter(|p| *p > 0) {
            Some(port) => format!("{custom} · {host}:{port}"),
            None => format!("{custom} · {host}"),
        };
    }
    let args = settings.args.as_str();
    let option = |name: &str, fallback: &'static str| -> String {
        let parts: Vec<&str> = args.split_whitespace().collect();
        parts.windows(2).find(|w| w[0] == name).map_or(fallback.to_owned(), |w| w[1].to_owned())
    };
    let backend = if args.contains("katago-CUDA") { "CUDA" } else { "TensorRT" };
    let plan = match option("--gpu-type", "vip-share").as_str() {
        "vip-share" => java_text(language, "RemoteCompute.plan.vipShort"),
        "1x" if backend == "CUDA" => java_text(language, "RemoteCompute.plan.1xCuda"),
        "1x" => java_text(language, "RemoteCompute.plan.1x"),
        "3x" => java_text(language, "RemoteCompute.plan.3x"),
        "6x" => java_text(language, "RemoteCompute.plan.6x"),
        "12x" => java_text(language, "RemoteCompute.plan.12x"),
        "24x" => java_text(language, "RemoteCompute.plan.24x"),
        other => format!("On-demand {other}"),
    };
    let weight = crate::cloud::kata_weight(args);
    let model = match weight.to_ascii_lowercase().as_str() {
        "28bnbt" => "28B NBT".to_owned(),
        "18bnbt" => "18B NBT".to_owned(),
        "fdx" => "FDX · 40B NBT".to_owned(),
        "10b384t" => java_text(language, "RemoteCompute.weightName.10b384t"),
        "10b512t" => java_text(language, "RemoteCompute.weightName.10b512t"),
        "11b768t" => java_text(language, "RemoteCompute.weightName.11b768t"),
        lower if lower.len() > 1 && lower.ends_with('b') && lower[..lower.len() - 1].bytes().all(|b| b.is_ascii_digit()) => weight.to_ascii_uppercase(),
        _ => weight.to_owned(),
    };
    format!("{} {plan} · {model} · {backend}", java_text(language, "RemoteCompute.zhizi"))
}

/// Match Java's local fallback: executable plus every model/config/jar must exist.
fn usable_local(spec: &crate::engine::gtp::LaunchSpec) -> bool {
    use std::path::Path;
    let program = Path::new(&spec.program);
    let executable = if program.is_file() {
        Some(program.to_path_buf())
    } else if program.components().count() == 1 {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths).map(|dir| dir.join(program)).find(|p| p.is_file())
        })
    } else { None };
    let Some(executable) = executable else { return false };
    let exists = |value: &str| {
        let value = value.trim();
        if value.is_empty() { return false; }
        let path = Path::new(value);
        path.is_file() || (!path.is_absolute() && (
            spec.working_dir.as_ref().is_some_and(|dir| dir.join(path).is_file())
            || executable.parent().is_some_and(|dir| dir.join(path).is_file())))
    };
    let mut args = spec.args.iter();
    while let Some(arg) = args.next() {
        let (option, inline) = arg.split_once('=').map_or((arg.as_str(), None), |(k, v)| (k, Some(v)));
        let file_option = ["-model", "--model", "-config", "--config"]
            .iter().any(|key| option.eq_ignore_ascii_case(key));
        if file_option || (inline.is_none() && option.eq_ignore_ascii_case("-jar")) {
            let Some(value) = inline.or_else(|| args.next().map(String::as_str)) else { return false };
            if !exists(value) { return false; }
        }
    }
    true
}
