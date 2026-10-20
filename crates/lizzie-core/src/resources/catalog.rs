//! Trusted per-asset KataGo release catalog (schema v3).
use super::{invalid, matches_pattern, ResourceResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const OFFICIAL_MODEL_PREFIX: &str = "https://media.katagotraining.org/uploaded/networks/models/kata1/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum SupportTier {
    Stable,
    Optional,
    Experimental,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeRequirement {
    None,
    LinuxCudaCudnn,
    NvidiaRuntimeDownload,
    TensorrtRuntime,
    OpenclIcd,
    Metal,
    Directx12,
    Openvino,
    RocmFamily,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum AssetOrigin {
    OfficialRelease,
    ProjectBuild,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityStatus {
    Unsupported,
    Unverified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    #[serde(default)]
    pub id: String,
    pub platform: String,
    pub backend: String,
    pub asset_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub executable: String,
    pub executable_sha256: String,
    pub executable_size_bytes: u64,
    pub origin: AssetOrigin,
    pub release_repository: String,
    pub release_tag: String,
    pub version: String,
    pub source_tag: String,
    pub source_commit: String,
    pub release_tier: SupportTier,
    pub required_files: Vec<String>,
    pub external_libraries: Vec<String>,
    #[serde(default)]
    pub cpu_features: Vec<String>,
    #[serde(default)]
    pub gpu_family: String,
    pub cuda: Option<String>,
    pub cudnn: Option<String>,
    pub tensorrt: Option<String>,
    pub minimum_driver: Option<String>,
    #[serde(default)]
    pub app_image: bool,
    pub license_summary: String,
    pub license_texts: Vec<String>,
    pub runtime_requirement: RuntimeRequirement,
    pub analysis: bool,
    pub focus: CapabilityStatus,
    pub contribution: CapabilityStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[serde(default)]
    pub id: String,
    pub file_name: String,
    pub display_family: String,
    pub tier: String,
    pub architecture: String,
    pub minimum_kata_go_version: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub bundled: bool,
    #[serde(default)]
    pub download_url: String,
    #[serde(default = "default_date")]
    pub published_at: String,
    pub license_summary: String,
    pub license_texts: Vec<String>,
    pub runtime_requirement: RuntimeRequirement,
    pub support_tier: SupportTier,
    pub analysis: bool,
    pub contribution: bool,
}
fn default_date() -> String {
    "2026-07-29".into()
}
impl Model {
    pub fn model_name(&self) -> &str {
        self.file_name.strip_suffix(".bin.gz").unwrap_or(&self.file_name)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema_version: u32,
    pub model_release_tag: String,
    pub default_model_id: String,
    pub models: BTreeMap<String, Model>,
    pub assets: BTreeMap<String, Asset>,
}
impl Catalog {
    pub fn embedded() -> ResourceResult<Self> {
        Self::from_json(include_str!("../../resources/catalog.json"))
    }
    pub fn from_json(json: &str) -> ResourceResult<Self> {
        let mut catalog: Self = serde_json::from_str(json)?;
        for (id, asset) in &mut catalog.assets {
            asset.id.clone_from(id);
        }
        for (id, model) in &mut catalog.models {
            model.id.clone_from(id);
        }
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn validate(&self) -> ResourceResult<()> {
        if self.schema_version != 3
            || self.model_release_tag.is_empty()
            || !self.models.contains_key(&self.default_model_id)
        {
            return Err(invalid("Invalid catalog identity or default model"));
        }
        for (id, asset) in &self.assets {
            if !matches_pattern(r"^[A-Za-z0-9][A-Za-z0-9._-]*\.zip$", &asset.asset_name)
                || !matches_pattern(r"^[A-Za-z0-9][A-Za-z0-9_-]*$", id)
            {
                return Err(invalid("Unsafe KataGo asset name"));
            }
            validate_sha256(&asset.sha256)?;
            validate_sha256(&asset.executable_sha256)?;
            if asset.version.is_empty()
                || asset.source_tag.strip_prefix('v') != Some(asset.version.as_str())
                || !matches_pattern("^[0-9a-f]{40}$", &asset.source_commit)
            {
                return Err(invalid("Invalid KataGo source identity"));
            }
            let prefix = format!("katago-{}-", asset.source_tag);
            let trusted = match asset.origin {
                AssetOrigin::OfficialRelease => {
                    asset.release_repository == "lightvector/KataGo"
                        && asset.release_tag == asset.source_tag
                        && !asset.asset_name.contains("+bs50")
                }
                AssetOrigin::ProjectBuild => {
                    asset.release_repository == "qiyi71w/lizzieyzy-desktop" && asset.release_tag.starts_with(&prefix)
                }
            };
            if !trusted || !asset.asset_name.starts_with(&prefix) {
                return Err(invalid("Invalid trusted release identity"));
            }
            if !matches!(
                asset.platform.as_str(),
                "windows-x64" | "linux-x64" | "macos-arm64" | "macos-amd64"
            ) || asset.size_bytes == 0
                || asset.executable_size_bytes == 0
            {
                return Err(invalid("Invalid asset platform or size"));
            }
            let executable = if asset.platform == "windows-x64" {
                "katago.exe"
            } else {
                "katago"
            };
            if asset.executable != executable
                || !asset.required_files.iter().any(|p| p == executable)
                || asset.required_files.iter().any(|p| !safe_relative_path(p))
                || asset.license_summary.is_empty()
                || asset.license_texts.is_empty()
                || asset
                    .license_texts
                    .iter()
                    .any(|p| !p.starts_with("https://") && !safe_relative_path(p))
                || !asset.analysis
                || matches!(asset.backend.as_str(), "directml" | "openvino")
                    && asset.contribution != CapabilityStatus::Unsupported
            {
                return Err(invalid("Invalid package layout, license or capabilities"));
            }
        }
        if self.assets.get("windows-nvidia").is_none_or(|a| {
            a.cuda.as_deref() != Some(super::CUDA_VERSION) || a.cudnn.as_deref() != Some(super::CUDNN_VERSION)
        }) || self.assets.get("windows-tensorrt").is_none_or(|a| {
            a.cuda.as_deref() != Some(super::CUDA_VERSION) || a.tensorrt.as_deref() != Some(super::TENSORRT_VERSION)
        }) {
            return Err(invalid("Catalog NVIDIA versions disagree with runtime pins"));
        }
        for model in self.models.values() {
            if !matches_pattern(r"^[A-Za-z0-9][A-Za-z0-9._-]*\.(bin|txt)\.gz$", &model.file_name)
                || model.size_bytes == 0
            {
                return Err(invalid("Unsafe model name or size"));
            }
            validate_sha256(&model.sha256)?;
            if !model.download_url.is_empty()
                && model.download_url != format!("{OFFICIAL_MODEL_PREFIX}{}", model.file_name)
            {
                return Err(invalid("Unsupported official model download URL"));
            }
            validate_notes(
                &model.license_summary,
                &model.license_texts,
                model.analysis,
                model.contribution,
            )?;
        }
        Ok(())
    }
    pub fn assets_for_platform<'a>(&'a self, platform: &'a str) -> impl Iterator<Item = &'a Asset> {
        self.assets.values().filter(move |a| a.platform == platform)
    }
    pub fn asset(&self, id: &str) -> ResourceResult<&Asset> {
        self.assets
            .get(id)
            .ok_or_else(|| invalid(format!("Unknown asset: {id}")))
    }
    pub fn model(&self, id: &str) -> ResourceResult<&Model> {
        self.models
            .get(id)
            .ok_or_else(|| invalid(format!("Unknown model: {id}")))
    }
    pub fn asset_url(&self, asset: &Asset) -> ResourceResult<String> {
        if self.assets.get(&asset.id) != Some(asset) {
            return Err(invalid("Asset is not from trusted catalog"));
        }
        Ok(format!(
            "https://github.com/{}/releases/download/{}/{}",
            asset.release_repository, asset.release_tag, asset.asset_name
        ))
    }
    pub fn model_url(&self, model: &Model) -> String {
        if model.download_url.is_empty() {
            format!(
                "https://github.com/lightvector/KataGo/releases/download/{}/{}",
                self.model_release_tag, model.file_name
            )
        } else {
            model.download_url.clone()
        }
    }
}
fn safe_relative_path(path: &str) -> bool {
    let path = path.strip_suffix('/').unwrap_or(path);
    !path.is_empty()
        && !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn validate_notes(summary: &str, texts: &[String], analysis: bool, contribution: bool) -> ResourceResult<()> {
    if summary.is_empty() || texts.is_empty() || !analysis || contribution {
        return Err(invalid("Invalid resource license/capabilities"));
    }
    Ok(())
}
pub fn validate_sha256(sha: &str) -> ResourceResult<()> {
    if matches_pattern("^[0-9a-fA-F]{64}$", sha) {
        Ok(())
    } else {
        Err(invalid("Invalid SHA-256"))
    }
}
