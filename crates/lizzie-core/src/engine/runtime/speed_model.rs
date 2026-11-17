//! Java `B11ModelNotice`: which model one engine incarnation loads, for the title's speed notice
//! only. It never changes engine parameters or judges measured speed.
use super::gtp::LaunchSpec;
use super::InstanceId;
use crate::engine::cloud::CloudProvider;
use crate::engine::profile::EngineProfile;
use std::path::PathBuf;

/// A local launch's model header, read off the workbench (Java's `model-speed-notice` reader).
/// The caller runs [`ModelLookup::run`] and hands the result to `Engines::finish_model_lookup`.
pub struct ModelLookup {
    instance: InstanceId,
    path: PathBuf,
}

/// What a [`ModelLookup`] found, for the instance it was made for.
pub struct ModelIdentity {
    pub(super) instance: InstanceId,
    pub(super) b11: bool,
}

impl ModelLookup {
    /// A filename or engine nickname is no evidence of the loaded model: only its header counts.
    pub fn run(self) -> ModelIdentity {
        ModelIdentity {
            instance: self.instance,
            b11: crate::resources::read_weight_model_name(&self.path)
                .is_some_and(|name| crate::resources::is_b11_model(&name)),
        }
    }
}

/// How an incarnation's model is known (`Leelaz.speedModelLookup`).
pub(super) enum SpeedModel {
    Known(bool),
    Lookup(ModelLookup),
}

/// `Leelaz.startEngineOwned`'s `speedModelLookup`: a Zhizi connection is known from the model it
/// selected; a local launch needs its model header read; an SSH, 自建算力 or indirect-launcher
/// engine's model is not on this host and never shows the notice.
pub(super) fn identify(instance: InstanceId, profile: &EngineProfile, spec: &LaunchSpec) -> SpeedModel {
    if let Some(cloud) = &profile.cloud {
        return SpeedModel::Known(
            cloud.provider == CloudProvider::Zhizi
                && crate::cloud::kata_weight(&cloud.args).trim().eq_ignore_ascii_case("11b768t"),
        );
    }
    if profile.ssh.is_some()
        || spec.program.trim().is_empty()
        || crate::contribution::saved::is_indirect_launcher(&spec.program)
    {
        return SpeedModel::Known(false);
    }
    // `B11ModelNotice.readLocal`: the last model option wins, as `flag value` or `flag=value`.
    let mut model = None;
    let mut args = spec.args.iter();
    while let Some(part) = args.next() {
        for flag in ["-model", "--model", "-weights", "--weights"] {
            if part == flag {
                let Some(value) = args.next() else { return SpeedModel::Known(false) };
                model = Some(value.as_str());
                break;
            }
            if let Some(value) = part.strip_prefix(flag).and_then(|rest| rest.strip_prefix('=')) {
                model = Some(value);
                break;
            }
        }
    }
    let Some(model) = model.filter(|m| !m.trim().is_empty()) else { return SpeedModel::Known(false) };
    let mut path = PathBuf::from(model);
    if !path.is_absolute() {
        let directory = spec.working_dir.clone().or_else(|| std::env::current_dir().ok()).unwrap_or_default();
        path = directory.join(path);
    }
    SpeedModel::Lookup(ModelLookup { instance, path })
}
