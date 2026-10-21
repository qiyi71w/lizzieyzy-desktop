//! Contract types of engine resources (see `../contract.rs`).

use lizzie_core::api::*;
use lizzie_core::resources::{
    AppleFacts, Asset, AssetOrigin, CapabilityStatus, Catalog, ComputeCapability, CudaCompatibility, CudaProbeResult,
    EngineValidation, ExperimentalBackend, InstalledPackage, InstalledWeight, MachineFacts, ManagedProfile,
    MissingDependency, Model, ModelCard, NvidiaGpu, NvidiaQueryStatus, PackageOption, Recommendation,
    RecommendationReason, RemoteWeight, ResourceInventory, ResourceReceipt, ResourceSource, RuntimePackage,
    RuntimeRequirement, SupportTier, ValidationStatus, WeightCompatibility,
};
use lizzie_core::settings::{NetworkSettings, ProxyMode};
use ts_rs::Config;

pub fn decls(cfg: &Config) -> Vec<String> {
    crate::decls![cfg; ManagedProfile, SupportTier, RuntimeRequirement, AssetOrigin, CapabilityStatus, Asset, Model, Catalog,
        CudaCompatibility, NvidiaQueryStatus, ComputeCapability, NvidiaGpu, AppleFacts, MachineFacts,
        RecommendationReason, MissingDependency, PackageOption, ModelCard, Recommendation,
        RuntimePackage, ExperimentalBackend, ResourceSource, ResourceReceipt, InstalledPackage,
        InstalledWeight, ResourceInventory, WeightCompatibility, RemoteWeight,
        ValidationStatus, EngineValidation, CudaProbeResult,
        NetworkSettings, ProxyMode,
        ResourceItem, LocalEngineChoice, ResourceEngineChoice, ResourcePlanView, ResourceTaskView,
        ResourceOutcome, ResourceSetupView, ResourceAccelerationView, ResourceRepairView,
        ResourceCommand, ResourceResult, ResourcesView]
}
