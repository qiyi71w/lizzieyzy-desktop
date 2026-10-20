//! Java `KataGoAssetCatalog` (schema v2, validation and URL builders).
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
#[serde(rename_all = "camelCase")]
pub struct Asset {
    #[serde(default)]
    pub id: String,
    pub platform: String,
    pub backend: String,
    pub asset_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    #[serde(default)]
    pub executable_sha256: String,
    #[serde(default)]
    pub runtime_profile: String,
    #[serde(default)]
    pub gpu_family: String,
    pub release_tier: SupportTier,
    #[serde(default = "dynamic")]
    pub zlib_linkage: String,
    pub license_summary: String,
    pub license_texts: Vec<String>,
    pub runtime_requirement: RuntimeRequirement,
    pub analysis: bool,
    pub contribution: bool,
}
fn dynamic() -> String {
    "dynamic".into()
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
    pub katago_version: String,
    pub katago_release_tag: String,
    pub katago_source_commit: String,
    pub origin: String,
    #[serde(default)]
    pub engine_release_repository: String,
    #[serde(default)]
    pub engine_release_tag: String,
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
        if self.schema_version != 2
            || self.katago_version.is_empty()
            || self.katago_release_tag.is_empty()
            || self.model_release_tag.is_empty()
            || !self.models.contains_key(&self.default_model_id)
        {
            return Err(invalid("Invalid catalog identity or default model"));
        }
        match self.origin.as_str() {
            "project-source-build"
                if self.engine_release_repository == "wimi321/lizzieyzy-next"
                    && matches_pattern("^[0-9a-f]{40}$", &self.katago_source_commit)
                    && matches_pattern(r"^next-\d{4}-\d{2}-\d{2}\.[1-9][0-9]*$", &self.engine_release_tag) => {}
            "official-release" if self.engine_release_repository.is_empty() && self.engine_release_tag.is_empty() => {}
            _ => return Err(invalid("Invalid trusted release identity")),
        }
        for (id, asset) in &self.assets {
            if !matches_pattern(r"^[A-Za-z0-9][A-Za-z0-9._-]*\.zip$", &asset.asset_name)
                || !matches_pattern(r"^[A-Za-z0-9][A-Za-z0-9_-]*$", id)
            {
                return Err(invalid("Unsafe KataGo asset name"));
            }
            if self.origin == "project-source-build"
                && asset.asset_name != format!("katago-source-{}-{id}.zip", &self.katago_source_commit[..12])
            {
                return Err(invalid("Source asset does not identify its commit and target"));
            }
            validate_sha256(&asset.sha256)?;
            if !asset.executable_sha256.is_empty() {
                validate_sha256(&asset.executable_sha256)?;
            }
            let static_zlib =
                self.origin == "project-source-build" && matches!(id.as_str(), "windows-nvidia" | "windows-tensorrt");
            if !matches!(asset.zlib_linkage.as_str(), "static" | "dynamic")
                || (asset.zlib_linkage == "static") != static_zlib
            {
                return Err(invalid("Invalid KataGo zlib linkage"));
            }
            if !matches!(
                asset.platform.as_str(),
                "windows-x64" | "linux-x64" | "macos-arm64" | "macos-amd64"
            ) || asset.size_bytes == 0
            {
                return Err(invalid("Invalid asset platform or size"));
            }
            validate_notes(
                &asset.license_summary,
                &asset.license_texts,
                asset.analysis,
                asset.contribution,
            )?;
        }
        if self
            .assets
            .get("windows-nvidia")
            .is_none_or(|a| a.executable_sha256.is_empty())
            || !self.assets.contains_key("windows-tensorrt")
        {
            return Err(invalid(
                "Catalog requires Windows NVIDIA and TensorRT assets and NVIDIA executable digest",
            ));
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
        let (repo, tag) = if self.origin == "project-source-build" {
            (
                self.engine_release_repository.as_str(),
                self.engine_release_tag.as_str(),
            )
        } else {
            ("lightvector/KataGo", self.katago_release_tag.as_str())
        };
        Ok(format!(
            "https://github.com/{repo}/releases/download/{tag}/{}",
            asset.asset_name
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
