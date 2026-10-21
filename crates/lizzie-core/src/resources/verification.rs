//! Java `validateLocalEngine` and `buildCudaCompatibilityProbeCommand`.
use super::{Probe, ProbeOutput};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationStatus {
    Active,
    Valid,
    TimedOut,
    WrongArchitecture,
    MissingDependency,
    StartFailed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct EngineValidation {
    pub status: ValidationStatus,
    pub version: Option<String>,
    pub detail: String,
}

pub fn parse_katago_version(output: &str) -> Option<String> {
    static VERSION_PATTERN: std::sync::LazyLock<fancy_regex::Regex> = std::sync::LazyLock::new(|| {
        fancy_regex::Regex::new(r"(?i)\bKataGo\s+v(\d+)\.(\d+)(?:\.(\d+))?\b").expect("version pattern")
    });
    let c = VERSION_PATTERN.captures(output).ok()??;
    Some(format!(
        "{}.{}.{}",
        c.get(1)?.as_str(),
        c.get(2)?.as_str(),
        c.get(3).map_or("0", |m| m.as_str())
    ))
}
pub fn classify_version(result: std::io::Result<ProbeOutput>, active: bool) -> EngineValidation {
    if active {
        return EngineValidation {
            status: ValidationStatus::Active,
            version: None,
            detail: String::new(),
        };
    }
    let (detail, timed_out, success) = match result {
        Ok(output) => (output.combined(), output.timed_out, output.success()),
        Err(error) => (error.to_string(), false, false),
    };
    let normalized = detail.to_lowercase();
    let status = if timed_out {
        ValidationStatus::TimedOut
    } else if normalized.contains("fuse") || normalized.contains("cannot mount appimage") {
        ValidationStatus::MissingDependency
    } else if success {
        ValidationStatus::Valid
    } else if ["error=193", "bad cpu type", "exec format", "not a valid win32"]
        .iter()
        .any(|s| normalized.contains(s))
    {
        ValidationStatus::WrongArchitecture
    } else if ["error=126", "dll", "shared librar", "library not loaded"]
        .iter()
        .any(|s| normalized.contains(s))
    {
        ValidationStatus::MissingDependency
    } else {
        ValidationStatus::StartFailed
    };
    let version = parse_katago_version(&detail);
    EngineValidation {
        status,
        version,
        detail,
    }
}
/// No process is launched for an already active engine. NVIDIA packages get Java's 20s budget.
pub fn verify_engine(probe: &dyn Probe, executable: &Path, nvidia_package: bool, active: bool) -> EngineValidation {
    if active {
        return classify_version(Ok(ProbeOutput::default()), true);
    }
    classify_version(
        probe.run(
            &executable.to_string_lossy(),
            &["version".into()],
            Duration::from_secs(if nvidia_package { 20 } else { 8 }),
        ),
        false,
    )
}
pub fn cuda_probe_args(config: &Path, model: &Path) -> Vec<String> {
    vec![
        "benchmark".into(),
        "-config".into(),
        config.to_string_lossy().into_owned(),
        "-model".into(),
        model.to_string_lossy().into_owned(),
        "-t".into(),
        "1".into(),
        "-n".into(),
        "1".into(),
        "-v".into(),
        "2".into(),
        "-time".into(),
        "1".into(),
        "-no-server-thread-test".into(),
        "-no-half-batch-size-test".into(),
        "-override-config".into(),
        "numSearchThreads=1,logToStderr=false,logAllGTPCommunication=false,logSearchInfo=false".into(),
    ]
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct CudaProbeResult {
    pub passed: bool,
    pub timed_out: bool,
    pub detail: String,
}
/// Requires both a successful exit and a real throughput observation, not a version banner.
pub fn verify_cuda(probe: &dyn Probe, executable: &Path, config: &Path, model: &Path) -> CudaProbeResult {
    match probe.run(
        &executable.to_string_lossy(),
        &cuda_probe_args(config, model),
        Duration::from_secs(300),
    ) {
        Ok(output) => {
            let detail = output.combined();
            let passed = output.success() && crate::engine::benchmark::probe_passed(&detail);
            CudaProbeResult {
                passed,
                timed_out: output.timed_out,
                detail,
            }
        }
        Err(error) => CudaProbeResult {
            passed: false,
            timed_out: false,
            detail: error.to_string(),
        },
    }
}
