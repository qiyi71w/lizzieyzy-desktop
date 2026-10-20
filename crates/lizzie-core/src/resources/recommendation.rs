//! Frozen product preference CUDA > OpenCL > CPU; Java NVIDIA TensorRT advisory ranks.
use super::{Catalog, CudaCompatibility, MachineFacts, SupportTier};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub enum RecommendationReason {
    NvidiaDetected,
    NvidiaNotDetected,
    DriverSupported,
    DriverProbeRequired,
    DriverUnknown,
    DriverTooOld,
    OpenclAvailable,
    OpenclUnavailable,
    CpuFallback,
    CpuOptimized,
    CpuFeaturesUnknown,
    FuseUnknown,
    LibraryVersionUnknown,
    NativeMetal,
    RosettaUseArm64,
    TensorRtAllowed,
    TensorRtNotRecommended,
    ComputeUnknown,
    ExperimentalManualOnly,
    DependenciesMissing,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum MissingDependency {
    NvidiaGpu,
    MinimumDriver(String),
    LinuxLibrary(String),
    LoaderCacheUnknown,
    WindowsNvidiaRuntime,
    TensorRtRuntime,
    OpenclIcd,
    DirectX12,
    OpenVinoDevice,
    RocmFamily(String),
    CpuFeature(String),
    AppImageFuse,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PackageOption {
    pub asset_id: String,
    pub tier: SupportTier,
    /// Advisory eligibility only: all listed options remain manually selectable.
    pub eligible: bool,
    pub recommended: bool,
    pub reasons: Vec<RecommendationReason>,
    pub missing_dependencies: Vec<MissingDependency>,
    pub download_size: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelCard {
    pub id: String,
    pub tier: String,
    pub is_default: bool,
    pub hint_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub platform: Option<String>,
    pub packages: Vec<PackageOption>,
    pub models: Vec<ModelCard>,
}

/// No VRAM-based downgrade: unknown memory remains unknown in the machine facts.
pub fn recommend(facts: &MachineFacts, catalog: &Catalog) -> Recommendation {
    let mut packages = Vec::new();
    if let Some(platform) = facts.platform() {
        for asset in catalog.assets_for_platform(platform) {
            let mut option = PackageOption {
                asset_id: asset.id.clone(),
                tier: asset.release_tier.clone(),
                eligible: true,
                recommended: false,
                reasons: Vec::new(),
                missing_dependencies: Vec::new(),
                download_size: asset.size_bytes,
            };
            match asset.backend.as_str() {
                "cuda" | "tensorrt" => {
                    if facts.best_gpu().is_none() {
                        option.eligible = false;
                        option.reasons.push(RecommendationReason::NvidiaNotDetected);
                        option.missing_dependencies.push(MissingDependency::NvidiaGpu);
                    } else {
                        option.reasons.push(RecommendationReason::NvidiaDetected);
                    }
                    match facts.cuda_compatibility {
                        CudaCompatibility::Supported => option.reasons.push(RecommendationReason::DriverSupported),
                        CudaCompatibility::ProbeRequired => {
                            option.reasons.push(RecommendationReason::DriverProbeRequired)
                        }
                        CudaCompatibility::Unknown => option.reasons.push(RecommendationReason::DriverUnknown),
                        CudaCompatibility::Unsupported => {
                            option.eligible = false;
                            option.reasons.push(RecommendationReason::DriverTooOld);
                            option
                                .missing_dependencies
                                .push(MissingDependency::MinimumDriver("528.33".into()));
                        }
                    }
                    if facts.os != "linux" {
                        option
                            .missing_dependencies
                            .push(MissingDependency::WindowsNvidiaRuntime);
                    }
                    if asset.backend == "tensorrt" {
                        option.missing_dependencies.push(MissingDependency::TensorRtRuntime);
                        match facts.best_gpu().and_then(|g| g.compute.as_ref()).map(|c| c.rank()) {
                            Some(75..=88) => option.reasons.push(RecommendationReason::TensorRtAllowed),
                            Some(_) => {
                                option.reasons.push(RecommendationReason::TensorRtNotRecommended);
                                option.eligible = false;
                            }
                            None => {
                                option.reasons.push(RecommendationReason::ComputeUnknown);
                                option.eligible = false;
                            }
                        }
                    }
                }
                "opencl" => {
                    option.eligible = facts.opencl_icd == Some(true);
                    option.reasons.push(if option.eligible {
                        RecommendationReason::OpenclAvailable
                    } else {
                        RecommendationReason::OpenclUnavailable
                    });
                    if !option.eligible {
                        option.missing_dependencies.push(MissingDependency::OpenclIcd);
                    }
                }
                "eigen" => {
                    if asset.cpu_features.is_empty() {
                        option.reasons.push(RecommendationReason::CpuFallback);
                    } else {
                        match facts.cpu_avx2_fma {
                            Some(true) => option.reasons.push(RecommendationReason::CpuOptimized),
                            Some(false) => {
                                option.eligible = false;
                                option
                                    .missing_dependencies
                                    .push(MissingDependency::CpuFeature("AVX2+FMA".into()));
                            }
                            None => {
                                option.eligible = false;
                                option.reasons.push(RecommendationReason::CpuFeaturesUnknown);
                            }
                        }
                    }
                }
                "metal" => option.reasons.push(if facts.rosetta == Some(true) {
                    RecommendationReason::RosettaUseArm64
                } else {
                    RecommendationReason::NativeMetal
                }),
                "directml" => option.missing_dependencies.push(MissingDependency::DirectX12),
                "openvino" => option.missing_dependencies.push(MissingDependency::OpenVinoDevice),
                "rocm" => option
                    .missing_dependencies
                    .push(MissingDependency::RocmFamily(asset.gpu_family.clone())),
                _ => option.eligible = false,
            }
            if facts.os == "linux" {
                match &facts.loader_libraries {
                    Some(libraries) => {
                        for lib in &asset.external_libraries {
                            if !libraries.contains(lib) {
                                option.eligible = false;
                                option
                                    .missing_dependencies
                                    .push(MissingDependency::LinuxLibrary(lib.clone()));
                            }
                        }
                    }
                    None => {
                        option.eligible = false;
                        option.missing_dependencies.push(MissingDependency::LoaderCacheUnknown);
                    }
                }
                if asset.app_image {
                    match facts.fuse {
                        Some(false) => {
                            option.eligible = false;
                            option.missing_dependencies.push(MissingDependency::AppImageFuse);
                        }
                        None => option.reasons.push(RecommendationReason::FuseUnknown),
                        Some(true) => {}
                    }
                }
                if asset.backend == "cuda" {
                    for (prefix, required) in [
                        ("libcublas.so.", &asset.cuda),
                        ("libnvrtc.so.", &asset.cuda),
                        ("libcudnn.so.", &asset.cudnn),
                    ] {
                        let Some(required) = required else { continue };
                        let soname = format!("{prefix}{}", required.split('.').next().unwrap_or(required));
                        match facts.library_versions.get(&soname) {
                            Some(found) if compare_versions(found, required).is_lt() => {
                                option.eligible = false;
                                option
                                    .missing_dependencies
                                    .push(MissingDependency::LinuxLibrary(format!("{soname} ≥ {required}")));
                            }
                            None if !option.reasons.contains(&RecommendationReason::LibraryVersionUnknown) => {
                                option.reasons.push(RecommendationReason::LibraryVersionUnknown);
                            }
                            _ => {}
                        }
                    }
                    if let (Some(minimum), Some(driver)) =
                        (&asset.minimum_driver, facts.best_gpu().and_then(|g| g.driver.as_ref()))
                    {
                        if compare_versions(driver, minimum).is_lt() {
                            option.eligible = false;
                            option
                                .missing_dependencies
                                .push(MissingDependency::MinimumDriver(minimum.clone()));
                        }
                    }
                }
                if !option.missing_dependencies.is_empty() {
                    option.reasons.push(RecommendationReason::DependenciesMissing);
                }
            }
            if asset.release_tier == SupportTier::Experimental {
                option.reasons.push(RecommendationReason::ExperimentalManualOnly);
            }
            packages.push(option);
        }
        for backend in ["cuda", "opencl", "eigen", "metal"] {
            if let Some(option) = packages
                .iter_mut()
                .filter(|p| {
                    p.eligible && p.tier == SupportTier::Stable && catalog.assets[&p.asset_id].backend == backend
                })
                .max_by(|a, b| {
                    let a = &catalog.assets[&a.asset_id];
                    let b = &catalog.assets[&b.asset_id];
                    compare_versions(a.cuda.as_deref().unwrap_or("0"), b.cuda.as_deref().unwrap_or("0"))
                        .then_with(|| (!a.cpu_features.is_empty()).cmp(&!b.cpu_features.is_empty()))
                })
            {
                option.recommended = true;
                break;
            }
        }
    }
    let models = ["b10-balanced", "b11-flagship", "b10-lightweight"]
        .iter()
        .filter_map(|id| catalog.models.get(*id))
        .map(|m| ModelCard {
            id: m.id.clone(),
            tier: m.tier.clone(),
            is_default: m.id == catalog.default_model_id,
            hint_key: match m.tier.as_str() {
                "balanced" => "AutoSetup.recommendationBalancedHint",
                "strongest" => "AutoSetup.recommendationStrongestHint",
                _ => "AutoSetup.recommendationLightweightHint",
            }
            .into(),
        })
        .collect();
    Recommendation {
        platform: facts.platform().map(str::to_string),
        packages,
        models,
    }
}

pub(crate) fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a = a.split('.').map(|n| n.parse::<u32>().unwrap_or(0));
    let mut b = b.split('.').map(|n| n.parse::<u32>().unwrap_or(0));
    loop {
        let (left, right) = (a.next(), b.next());
        if left.is_none() && right.is_none() {
            return std::cmp::Ordering::Equal;
        }
        let order = left.unwrap_or(0).cmp(&right.unwrap_or(0));
        if !order.is_eq() {
            return order;
        }
    }
}
