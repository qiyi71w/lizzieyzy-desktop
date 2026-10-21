//! Java `BundledKataGoProfile`, `applyEngineProfile`, and experimental command overrides.
use super::{invalid, ManagedProfile, ResourceResult};
use crate::engine::{
    profile::{new_profile_id, EngineProfile},
    tuning::ThreadPolicy,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn is_managed(profile: &EngineProfile) -> bool {
    profile
        .managed
        .as_ref()
        .is_some_and(|marker| !marker.command.is_empty() && marker.command == profile.command)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum ExperimentalBackend {
    DirectMl,
    OpenVinoGpu,
    OpenVinoNpu,
    RocmGfx103x,
    RocmGfx110x,
    RocmGfx1151,
    RocmGfx120x,
}
impl ExperimentalBackend {
    pub fn asset_id(self) -> &'static str {
        match self {
            Self::DirectMl => "windows-directml",
            Self::OpenVinoGpu | Self::OpenVinoNpu => "windows-openvino",
            Self::RocmGfx103x => "windows-rocm-gfx103x",
            Self::RocmGfx110x => "windows-rocm-gfx110x",
            Self::RocmGfx1151 => "windows-rocm-gfx1151",
            Self::RocmGfx120x => "windows-rocm-gfx120x",
        }
    }
    pub fn install_dir(self) -> &'static str {
        match self {
            Self::DirectMl => "windows-x64-directml",
            Self::OpenVinoGpu => "windows-x64-openvino-gpu",
            Self::OpenVinoNpu => "windows-x64-openvino-npu",
            Self::RocmGfx103x => "windows-x64-rocm-gfx103x",
            Self::RocmGfx110x => "windows-x64-rocm-gfx110x",
            Self::RocmGfx1151 => "windows-x64-rocm-gfx1151",
            Self::RocmGfx120x => "windows-x64-rocm-gfx120x",
        }
    }
}
/// Java's command parser has quoting but no escape syntax. Reject quotes/newlines rather than
/// silently building a different command for paths containing them.
fn quoted(value: &str) -> ResourceResult<String> {
    if value.contains(['"', '\r', '\n', '\0']) {
        return Err(invalid("Path cannot be represented in an engine command"));
    }
    Ok(format!("\"{value}\""))
}
pub fn build_gtp_command(
    executable: &Path,
    weight: &Path,
    config: &Path,
    backend: Option<ExperimentalBackend>,
    resources_dir: &Path,
) -> ResourceResult<String> {
    let exe = quoted(&executable.to_string_lossy())?;
    let weight = quoted(&weight.to_string_lossy())?;
    let config = quoted(&config.to_string_lossy())?;
    let mut command = format!("{exe} gtp -model {weight} -config {config}");
    let overrides = match backend {
        Some(ExperimentalBackend::DirectMl) => Some("onnxProvider=directml".to_string()),
        Some(ExperimentalBackend::OpenVinoGpu | ExperimentalBackend::OpenVinoNpu) => {
            let cache = resources_dir.join("runtime/katago-openvino-cache");
            let mut value = format!("onnxProvider=openvino,onnxOpenVINOCacheDir={}", cache.to_string_lossy());
            if backend == Some(ExperimentalBackend::OpenVinoNpu) {
                value.push_str(",onnxOpenVINODeviceType=NPU");
            }
            Some(value)
        }
        _ => None,
    };
    if let Some(value) = overrides {
        command.push_str(" -override-config ");
        command.push_str(&quoted(&value)?);
    }
    Ok(command)
}
#[derive(Clone, Debug)]
pub struct SetupChoice {
    pub asset_id: String,
    pub executable: PathBuf,
    pub weight: PathBuf,
    pub config: PathBuf,
    pub model_display_name: String,
    /// Header identity from read_weight_model_name (not a guessed identity from a renamed file).
    pub model_name: Option<String>,
    /// Version from explicit verification or the already-running engine's identification.
    pub engine_version: Option<String>,
    pub experimental_backend: Option<ExperimentalBackend>,
    /// None adds a profile. Some(id) must name a still-managed profile.
    pub update_profile_id: Option<String>,
    pub set_as_default: bool,
    pub switch_now: bool,
}
#[derive(Clone, Debug)]
pub struct SetupPlan {
    pub profile: EngineProfile,
    pub replaces_profile_id: Option<String>,
    pub set_as_default: bool,
    pub switch_now: bool,
}
/// Returns one complete profile for the workbench's single writer to commit after confirmation.
/// The plan never selects or modifies an unmanaged profile, including stale ownership markers.
pub fn plan_setup(profiles: &[EngineProfile], choice: &SetupChoice, resources_dir: &Path) -> ResourceResult<SetupPlan> {
    if let Some(name) = &choice.model_name {
        if !super::weight_compatibility(name, choice.engine_version.as_deref()).compatible {
            return Err(invalid(
                "Transformer weights require KataGo >= 1.17.0; verify the engine version before applying",
            ));
        }
    }
    let mut profile = if let Some(id) = &choice.update_profile_id {
        let profile = profiles
            .iter()
            .find(|p| &p.id == id)
            .ok_or_else(|| invalid("Selected profile no longer exists"))?;
        if !is_managed(profile) {
            return Err(invalid("Selected profile is not managed; nothing changed"));
        }
        profile.clone()
    } else {
        EngineProfile {
            id: new_profile_id(profiles),
            thread_policy: Some(ThreadPolicy::initial_setup()),
            ..Default::default()
        }
    };
    profile.name = format!("KataGo ({})", choice.model_display_name);
    profile.command = build_gtp_command(
        &choice.executable,
        &choice.weight,
        &choice.config,
        choice.experimental_backend,
        resources_dir,
    )?;
    profile.managed = Some(ManagedProfile {
        command: profile.command.clone(),
        asset_id: choice.asset_id.clone(),
        weight_path: choice.weight.to_string_lossy().into_owned(),
        config_path: choice.config.to_string_lossy().into_owned(),
    });
    if choice.set_as_default {
        profile.is_default = true;
    }
    Ok(SetupPlan {
        profile,
        replaces_profile_id: choice.update_profile_id.clone(),
        set_as_default: choice.set_as_default,
        switch_now: choice.switch_now,
    })
}
