//! Java `KataGoRuntimeHelper` NVIDIA redistributable metadata and DLL/license selection.
use super::{invalid, validate_sha256, DownloadSpec, ResourceResult};
use serde::{Deserialize, Serialize};

pub const CUDA_MANIFEST_URL: &str = "https://developer.download.nvidia.com/compute/cuda/redist/redistrib_12.8.0.json";
pub const CUDNN_MANIFEST_URL: &str = "https://developer.download.nvidia.com/compute/cudnn/redist/redistrib_9.8.0.json";
pub const NVRTC_VERSION: &str = "12.8.61";
pub const NVRTC_SHA256: &str = "e43603b09f8a52d681ceb814c00b655af19da53692ab91671dabbf8071c8f93d";
pub const TENSORRT_URL: &str = "https://developer.download.nvidia.com/compute/machine-learning/tensorrt/10.9.0/zip/TensorRT-10.9.0.34.Windows.win10.cuda-12.8.zip";
pub const TENSORRT_SHA256: &str = "c2758eb60191f01a47b24f54700e5463f577ebe129cd18fe835d0aa9f1e1a16d";
pub const TENSORRT_SIZE: u64 = 1_845_842_538;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePackage {
    pub key: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}
impl RuntimePackage {
    pub fn download_spec(&self) -> ResourceResult<DownloadSpec> {
        let file_name = self
            .url
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("Runtime URL has no archive name"))?;
        Ok(DownloadSpec {
            url: self.url.clone(),
            file_name: file_name.into(),
            size: Some(self.size),
            sha256: Some(self.sha256.clone()),
        })
    }
}
pub fn runtime_component_keys(backend: &str) -> &'static [&'static str] {
    match backend {
        "cuda" => &["cuda_cudart", "libcublas", "libnvjitlink", "cuda_nvrtc", "cudnn"],
        "tensorrt" => &[
            "cuda_cudart",
            "libcublas",
            "libnvjitlink",
            "cuda_nvrtc",
            "cudnn",
            "tensorrt",
        ],
        _ => &[],
    }
}
/// `windows-x86_64`, with `cuda12` nested under cuDNN. NVRTC's version/hash are pinned.
pub fn parse_runtime_manifest(body: &str, manifest_url: &str, key: &str) -> ResourceResult<RuntimePackage> {
    if !matches!(manifest_url, CUDA_MANIFEST_URL | CUDNN_MANIFEST_URL) {
        return Err(invalid("Untrusted NVIDIA manifest URL"));
    }
    let root: serde_json::Value = serde_json::from_str(body)?;
    let package = root
        .get(key)
        .ok_or_else(|| invalid(format!("Missing NVIDIA package metadata: {key}")))?;
    let platform = package
        .get("windows-x86_64")
        .ok_or_else(|| invalid("Missing NVIDIA windows-x86_64 metadata"))?;
    let platform = if key == "cudnn" {
        platform
            .get("cuda12")
            .ok_or_else(|| invalid("Missing cuDNN cuda12 metadata"))?
    } else {
        platform
    };
    let text =
        |value: &serde_json::Value, key: &str| value.get(key).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let relative = text(platform, "relative_path");
    let version = text(package, "version");
    let sha256 = text(platform, "sha256").to_lowercase();
    validate_sha256(&sha256)?;
    let size = platform
        .get("size")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("Missing NVIDIA package size"))?;
    if relative.is_empty() || version.is_empty() || relative.contains("..") || relative.contains(['\\', '?', '#']) {
        return Err(invalid("Incomplete or unsafe NVIDIA metadata"));
    }
    let url = if relative.starts_with("https://") {
        relative
    } else {
        format!(
            "{}/{}",
            manifest_url.rsplit_once('/').expect("manifest directory").0,
            relative.trim_start_matches('/')
        )
    };
    if !url.starts_with("https://developer.download.nvidia.com/")
        && !url.starts_with("https://developer.download.nvidia.cn/")
    {
        return Err(invalid("Untrusted NVIDIA package URL"));
    }
    if key == "cuda_nvrtc" && (version != NVRTC_VERSION || sha256 != NVRTC_SHA256) {
        return Err(invalid(
            "Unexpected CUDA 12.8 NVRTC metadata; pinned version and SHA-256 required",
        ));
    }
    Ok(RuntimePackage {
        key: key.into(),
        version,
        url,
        sha256,
        size,
    })
}
pub fn nvidia_runtime_packages(
    cuda_manifest: &str,
    cudnn_manifest: &str,
    backend: &str,
) -> ResourceResult<Vec<RuntimePackage>> {
    runtime_component_keys(backend)
        .iter()
        .map(|key| match *key {
            "tensorrt" => Ok(tensorrt_package()),
            "cudnn" => parse_runtime_manifest(cudnn_manifest, CUDNN_MANIFEST_URL, key),
            _ => parse_runtime_manifest(cuda_manifest, CUDA_MANIFEST_URL, key),
        })
        .collect()
}
pub fn tensorrt_package() -> RuntimePackage {
    RuntimePackage {
        key: "tensorrt".into(),
        version: "10.9.0.34".into(),
        url: TENSORRT_URL.into(),
        sha256: TENSORRT_SHA256.into(),
        size: TENSORRT_SIZE,
    }
}
pub fn should_extract_runtime_dll(key: &str, file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    lower.ends_with(".dll")
        && (key != "tensorrt"
            || ["nvinfer", "nvonnxparser", "onnx_proto", "myelin"]
                .iter()
                .any(|p| lower.starts_with(p)))
}
pub fn is_runtime_license(entry: &str) -> bool {
    let lower = entry.replace('\\', "/").to_lowercase();
    lower.rsplit('/').next() == Some("license.txt") || lower.contains("/license")
}
