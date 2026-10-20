//! Resource library behaviour through the same public interfaces used by setup flow/adapters.
use lizzie_core::{
    engine::{gtp::split_command, profile::EngineProfile},
    resources::*,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    fs::{self, File},
    io::{self, Cursor, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tempfile::TempDir;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn catalog() -> Catalog {
    Catalog::embedded().unwrap()
}
fn cancelled(value: bool) -> AtomicBool {
    AtomicBool::new(value)
}
fn facts(os: &str, name: Option<&str>, driver: &str) -> MachineFacts {
    let nvidia_gpus = name
        .map(|name| parse_nvidia_smi(&format!("{name}, N/A, {driver}, N/A"), true))
        .unwrap_or_default();
    MachineFacts {
        os: os.into(),
        arch: "x86_64".into(),
        cuda_compatibility: cuda_compatibility(Some(driver)),
        nvidia_status: if name.is_some() {
            NvidiaQueryStatus::Found
        } else {
            NvidiaQueryStatus::None
        },
        nvidia_gpus,
        opencl_icd: Some(false),
        ..Default::default()
    }
}
fn recommended(recommendation: &Recommendation) -> Option<&str> {
    recommendation
        .packages
        .iter()
        .find(|p| p.recommended)
        .map(|p| p.asset_id.as_str())
}

#[test]
fn embedded_catalog_preserves_java_identity_and_product_notes() {
    let c = catalog();
    c.validate().unwrap();
    assert_eq!(c.schema_version, 2);
    assert_eq!(c.katago_version, "1.18.2");
    assert_eq!(c.katago_source_commit, "47aadc08518b3e121f22539796c911002f699584");
    assert_eq!(c.engine_release_tag, "next-2026-10-17.1");
    assert_eq!(c.models.len(), 3);
    assert_eq!(c.assets.len(), 15);
    assert_eq!(c.default_model_id, "b11-flagship");
    for asset in c.assets.values() {
        assert!(asset.analysis);
        assert!(!asset.contribution);
        assert!(!asset.license_texts.is_empty());
        assert!(c.asset_url(asset).unwrap().ends_with(&asset.asset_name));
        assert_eq!(asset.executable_sha256.len(), 64);
    }
    assert!(c
        .model_url(c.model("b11-flagship").unwrap())
        .starts_with(OFFICIAL_MODEL_PREFIX));
    assert!(c
        .model_url(c.model("b10-balanced").unwrap())
        .contains("lightvector/KataGo/releases/download/v1.17.1/"));
    assert_eq!(c.assets_for_platform("windows-x64").count(), 10);
}
#[test]
fn catalog_rejects_untrusted_names_digests_urls_and_source_identity() {
    let original = serde_json::to_value(catalog()).unwrap();
    for (pointer, value) in [
        ("/assets/linux-cpu/assetName", "../engine.zip"),
        ("/assets/linux-cpu/assetName", "C:\\evil.zip"),
        ("/assets/linux-cpu/sha256", "abcd"),
        ("/assets/linux-cpu/executableSha256", "xyz"),
        (
            "/models/b11-flagship/downloadUrl",
            "https://evil.example/network.bin.gz",
        ),
        ("/engineReleaseRepository", "attacker/engines"),
        ("/katagoSourceCommit", "bad"),
        ("/engineReleaseTag", "latest"),
        (
            "/assets/linux-cpu/assetName",
            "katago-source-000000000000-linux-cpu.zip",
        ),
    ] {
        let mut json = original.clone();
        *json.pointer_mut(pointer).unwrap() = value.into();
        assert!(Catalog::from_json(&json.to_string()).is_err(), "{pointer}: {value}");
    }
    let c = catalog();
    let mut forged = c.assets["linux-cpu"].clone();
    forged.size_bytes += 1;
    assert!(c.asset_url(&forged).is_err());
}
#[test]
fn windows_recommendation_follows_driver_and_compute_rules() {
    let c = catalog();
    let r = recommend(&facts("windows", Some("NVIDIA GeForce RTX 4070"), "580.88"), &c);
    assert_eq!(recommended(&r), Some("windows-nvidia"));
    assert!(
        !r.packages
            .iter()
            .find(|p| p.asset_id == "windows-tensorrt")
            .unwrap()
            .eligible
    );
    let r = recommend(&facts("windows", Some("RTX 3060"), "580.88"), &c);
    let trt = r.packages.iter().find(|p| p.asset_id == "windows-tensorrt").unwrap();
    assert!(trt.eligible);
    assert!(!trt.recommended);
    assert!(trt.reasons.contains(&RecommendationReason::TensorRtAllowed));
    let r = recommend(&facts("windows", Some("RTX 3060"), "550.10"), &c);
    let cuda = r.packages.iter().find(|p| p.asset_id == "windows-nvidia").unwrap();
    assert!(cuda.recommended);
    assert!(cuda.reasons.contains(&RecommendationReason::DriverProbeRequired));
    assert!(cuda
        .missing_dependencies
        .contains(&MissingDependency::WindowsNvidiaRuntime));
    let r = recommend(&facts("windows", Some("RTX 3060"), "520.61"), &c);
    assert!(
        !r.packages
            .iter()
            .find(|p| p.asset_id == "windows-nvidia")
            .unwrap()
            .eligible
    );
    assert_eq!(recommended(&r), Some("windows-cpu"));
    for p in r.packages {
        if p.tier != SupportTier::Stable {
            assert!(!p.recommended);
        }
    }
}
#[test]
fn fallback_linux_dependencies_and_unknown_vram_are_explicit() {
    let c = catalog();
    let mut f = facts("windows", None, "N/A");
    f.opencl_icd = Some(true);
    assert_eq!(recommended(&recommend(&f, &c)), Some("windows-opencl"));
    f.opencl_icd = Some(false);
    assert_eq!(recommended(&recommend(&f, &c)), Some("windows-cpu"));
    let mut f = facts("linux", Some("RTX 4070"), "580.88");
    assert_eq!(f.best_gpu().unwrap().memory_mib, None);
    f.loader_libraries = Some(vec![
        "libcuda.so.1".into(),
        "libcudart.so.12".into(),
        "libcublas.so.12".into(),
    ]);
    let r = recommend(&f, &c);
    let cuda = r.packages.iter().find(|p| p.asset_id == "linux-nvidia").unwrap();
    assert!(!cuda.eligible);
    assert!(!cuda.recommended);
    assert!(cuda
        .missing_dependencies
        .contains(&MissingDependency::LinuxLibrary("libcudnn.so.9".into())));
    assert_eq!(recommended(&r), Some("linux-cpu"));
    f.loader_libraries.as_mut().unwrap().push("libcudnn.so.9".into());
    assert_eq!(recommended(&recommend(&f, &c)), Some("linux-nvidia"));
}
#[test]
fn mac_arch_rosetta_and_model_default() {
    let c = catalog();
    for (arch, rosetta, expected) in [
        ("aarch64", false, "macos-arm64"),
        ("x86_64", false, "macos-amd64"),
        ("x86_64", true, "macos-arm64"),
    ] {
        let f = MachineFacts {
            os: "macos".into(),
            arch: arch.into(),
            rosetta: Some(rosetta),
            ..Default::default()
        };
        let r = recommend(&f, &c);
        assert_eq!(recommended(&r), Some(expected));
        assert_eq!(r.models.len(), 3);
        assert_eq!(r.models.iter().find(|m| m.is_default).unwrap().id, "b11-flagship");
        if rosetta {
            assert!(r.packages[0].reasons.contains(&RecommendationReason::RosettaUseArm64));
        }
    }
}
#[test]
fn nvidia_csv_fallback_best_gpu_and_driver_boundaries() {
    let gpus = parse_nvidia_smi(
        "\"NVIDIA, RTX 3060\", 8.6, 580.88, 12288\nRTX 4070, N/A, 580.88, N/A\nMystery, N/A, N/A, N/A",
        true,
    );
    assert_eq!(gpus.len(), 3);
    assert!(!gpus[0].compute.as_ref().unwrap().inferred);
    assert!(gpus[1].compute.as_ref().unwrap().inferred);
    assert_eq!(gpus[1].memory_mib, None);
    assert_eq!(gpus[2].compute, None);
    assert_eq!(gpus[2].driver, None);
    assert_eq!(best_gpu(&gpus).unwrap().name, "RTX 4070");
    let old = parse_nvidia_smi("RTX 3060, 550.10, 12000\nRTX 3060, 550.10, 16000", false);
    assert_eq!(best_gpu(&old).unwrap().memory_mib, Some(16000));
    assert_eq!(cuda_compatibility(Some("570.65")), CudaCompatibility::Supported);
    assert_eq!(cuda_compatibility(Some("570.64")), CudaCompatibility::ProbeRequired);
    assert_eq!(cuda_compatibility(Some("528.33")), CudaCompatibility::ProbeRequired);
    assert_eq!(cuda_compatibility(Some("528.32")), CudaCompatibility::Unsupported);
    assert_eq!(cuda_compatibility(Some("N/A")), CudaCompatibility::Unknown);
    for (name, rank) in [
        ("RTX 5070", 120),
        ("RTX A5000", 86),
        ("A100", 80),
        ("T4", 75),
        ("GTX 1660", 75),
        ("GTX 1080", 61),
        ("L40S", 89),
    ] {
        assert_eq!(infer_compute(name).unwrap().rank(), rank, "{name}");
    }
}

#[derive(Default)]
struct FixtureProbe {
    outputs: BTreeMap<String, ProbeOutput>,
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeMap<PathBuf, Vec<PathBuf>>,
    env: BTreeMap<String, String>,
    calls: RefCell<Vec<(String, Vec<String>, Duration)>>,
}
impl Probe for FixtureProbe {
    fn run(&self, program: &str, args: &[String], timeout: Duration) -> io::Result<ProbeOutput> {
        self.calls.borrow_mut().push((program.into(), args.to_vec(), timeout));
        self.outputs
            .get(&format!("{program} {}", args.join(" ")))
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "fixture command unavailable"))
    }
    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "fixture file absent"))
    }
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.dirs
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "fixture dir absent"))
    }
    fn env_var(&self, key: &str) -> Option<String> {
        self.env.get(key).cloned()
    }
    fn logical_cpus(&self) -> Option<u32> {
        Some(8)
    }
}
fn success(stdout: &str) -> ProbeOutput {
    ProbeOutput {
        stdout: stdout.into(),
        exit_code: Some(0),
        ..Default::default()
    }
}
#[test]
fn machine_probe_is_bounded_and_platform_paths_are_testable() {
    let mut p = FixtureProbe::default();
    p.outputs.insert("/usr/lib/wsl/lib/nvidia-smi --query-gpu=name,compute_cap,driver_version,memory.total --format=csv,noheader,nounits".into(), success("RTX 5070 Ti, 12.0, 580.88, N/A"));
    p.outputs.insert(
        "/sbin/ldconfig -p".into(),
        success(
            "libcudnn.so.9 (libc6,x86-64) => /lib/libcudnn.so.9\nlibOpenCL.so.1 (libc6,x86-64) => /lib/libOpenCL.so.1",
        ),
    );
    p.files
        .insert("/proc/meminfo".into(), b"MemTotal: 1048576 kB\n".to_vec());
    p.files
        .insert("/proc/cpuinfo".into(), b"model name : Fixture CPU\n".to_vec());
    p.dirs.insert(
        "/etc/OpenCL/vendors".into(),
        vec!["/etc/OpenCL/vendors/vendor.icd".into()],
    );
    let f = detect_machine(&p, "linux", "x86_64");
    assert_eq!(f.total_memory_bytes, Some(1024 * 1024 * 1024));
    assert_eq!(f.cpu_name.as_deref(), Some("Fixture CPU"));
    assert_eq!(f.nvidia_status, NvidiaQueryStatus::Found);
    assert_eq!(f.best_gpu().unwrap().memory_mib, None);
    assert_eq!(f.opencl_icd, Some(true));
    assert!(p
        .calls
        .borrow()
        .iter()
        .filter(|(cmd, _, _)| cmd.contains("nvidia-smi"))
        .all(|(_, _, timeout)| *timeout == Duration::from_secs(8)));
    let mut p = FixtureProbe::default();
    p.env.insert("SystemRoot".into(), "C:\\Windows".into());
    p.files.insert("C:\\Windows\\System32\\OpenCL.dll".into(), vec![1]);
    let f = detect_machine(&p, "windows", "x86_64");
    assert_eq!(f.opencl_icd, Some(true));
    assert_eq!(
        p.calls
            .borrow()
            .iter()
            .find(|(cmd, _, _)| cmd.contains("nvidia-smi"))
            .unwrap()
            .0,
        "C:\\Windows\\System32\\nvidia-smi.exe"
    );
    let mut p = FixtureProbe::default();
    for (key, value) in [
        ("hw.model", "Mac14,6"),
        ("machdep.cpu.brand_string", "Apple M2 Max"),
        ("hw.machine", "x86_64"),
        ("hw.logicalcpu", "12"),
        ("hw.memsize", "34359738368"),
        ("sysctl.proc_translated", "1"),
    ] {
        p.outputs.insert(format!("/usr/sbin/sysctl -n {key}"), success(value));
    }
    p.outputs
        .insert("/usr/bin/sw_vers -buildVersion".into(), success("24G90"));
    let f = detect_machine(&p, "macos", "x86_64");
    assert_eq!(f.platform(), Some("macos-arm64"));
    assert_eq!(f.logical_cpus, Some(12));
    assert_eq!(f.apple.unwrap().build.as_deref(), Some("24G90"));
}

/// A queued response: status, total length, body.
type Response = (u16, Option<u64>, Vec<u8>);

struct MemoryFetch {
    responses: RefCell<VecDeque<Response>>,
    offsets: RefCell<Vec<Option<u64>>>,
}
impl MemoryFetch {
    fn new(responses: Vec<Response>) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            offsets: RefCell::new(Vec::new()),
        }
    }
}
impl Fetch for MemoryFetch {
    fn get(&self, _: &str, offset: Option<u64>) -> ResourceResult<FetchResponse> {
        self.offsets.borrow_mut().push(offset);
        let (status, total_length, data) = self.responses.borrow_mut().pop_front().expect("unexpected GET");
        Ok(FetchResponse {
            status,
            total_length,
            body: Box::new(Cursor::new(data)),
        })
    }
}
fn spec(data: &[u8]) -> DownloadSpec {
    DownloadSpec {
        url: "https://fixture.invalid/resource".into(),
        file_name: "resource.zip".into(),
        size: Some(data.len() as u64),
        sha256: Some(digest(data)),
    }
}
#[test]
fn download_resumes_and_reuses_verified_target() {
    let dir = TempDir::new().unwrap();
    let data = b"a verified package";
    fs::write(dir.path().join("resource.zip.part"), &data[..5]).unwrap();
    let fetch = MemoryFetch::new(vec![(206, Some(data.len() as u64), data[5..].to_vec())]);
    let result = download(&fetch, dir.path(), &spec(data), &cancelled(false), |_| {}).unwrap();
    assert_eq!(fetch.offsets.borrow().as_slice(), &[Some(5)]);
    assert_eq!(fs::read(&result.path).unwrap(), data);
    let reused = download(&fetch, dir.path(), &spec(data), &cancelled(false), |_| {}).unwrap();
    assert!(reused.reused);
    assert_eq!(fetch.offsets.borrow().len(), 1);
}
#[test]
fn complete_partial_moves_without_network_and_416_restarts_invalid_partial() {
    let dir = TempDir::new().unwrap();
    let data = b"whole package";
    fs::write(dir.path().join("resource.zip.part"), data).unwrap();
    let fetch = MemoryFetch::new(vec![]);
    download(&fetch, dir.path(), &spec(data), &cancelled(false), |_| {}).unwrap();
    assert!(fetch.offsets.borrow().is_empty());
    fs::remove_file(dir.path().join("resource.zip")).unwrap();
    fs::write(dir.path().join("resource.zip.part"), b"stale").unwrap();
    let fetch = MemoryFetch::new(vec![
        (416, Some(3), Vec::new()),
        (200, Some(data.len() as u64), data.to_vec()),
    ]);
    download(&fetch, dir.path(), &spec(data), &cancelled(false), |_| {}).unwrap();
    assert_eq!(fetch.offsets.borrow().as_slice(), &[Some(5), None]);
}
#[test]
fn range_ignored_restarts_and_integrity_failure_discards_only_partial() {
    let dir = TempDir::new().unwrap();
    let data = b"whole package";
    fs::write(dir.path().join("resource.zip.part"), b"old").unwrap();
    let fetch = MemoryFetch::new(vec![
        (200, Some(data.len() as u64), data.to_vec()),
        (200, Some(data.len() as u64), data.to_vec()),
    ]);
    download(&fetch, dir.path(), &spec(data), &cancelled(false), |_| {}).unwrap();
    assert_eq!(fetch.offsets.borrow().as_slice(), &[Some(3), None]);
    fs::write(dir.path().join("resource.zip"), b"previous target").unwrap();
    let fetch = MemoryFetch::new(vec![(200, Some(data.len() as u64), vec![b'X'; data.len()])]);
    let error = download(&fetch, dir.path(), &spec(data), &cancelled(false), |_| {}).unwrap_err();
    assert!(matches!(error, ResourceError::Integrity { .. }));
    assert!(!dir.path().join("resource.zip.part").exists());
    assert_eq!(fs::read(dir.path().join("resource.zip")).unwrap(), b"previous target");
}
#[test]
fn download_cancel_keeps_partial_and_remote_unhashed_weights_need_over_one_mib() {
    let dir = TempDir::new().unwrap();
    let data = vec![7; 200_000];
    let token = cancelled(false);
    let fetch = MemoryFetch::new(vec![(200, Some(data.len() as u64), data.clone())]);
    let error = download(&fetch, dir.path(), &spec(&data), &token, |_| {
        token.store(true, Ordering::Relaxed)
    })
    .unwrap_err();
    assert!(matches!(error, ResourceError::Cancelled));
    assert!(dir.path().join("resource.zip.part").is_file());
    assert!(!dir.path().join("resource.zip").exists());
    fs::remove_file(dir.path().join("resource.zip.part")).unwrap();
    let mut nohash = spec(b"tiny");
    nohash.size = None;
    nohash.sha256 = None;
    let fetch = MemoryFetch::new(vec![(200, Some(4), b"tiny".to_vec())]);
    assert!(matches!(
        download(&fetch, dir.path(), &nohash, &cancelled(false), |_| {}),
        Err(ResourceError::Integrity { .. })
    ));
}

fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
fn fixture_package(dir: &Path, bytes: &[u8], extra: Option<(&str, &[u8])>) -> (Catalog, PathBuf) {
    let path = dir.join("engine.zip");
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("pkg/katago", bytes),
        ("pkg/default_gtp.cfg", b"numSearchThreads = 8\n"),
        (
            "pkg/analysis_example.cfg",
            b"numAnalysisThreads = 2\nnumSearchThreadsPerAnalysisThread = 16\n",
        ),
        ("pkg/licenses/LICENSE", b"license"),
    ];
    if let Some(extra) = extra {
        entries.push(extra);
    }
    archive(&path, &entries);
    let mut c = catalog();
    let asset = c.assets.get_mut("linux-cpu").unwrap();
    asset.sha256 = sha256_file(&path).unwrap();
    asset.size_bytes = fs::metadata(&path).unwrap().len();
    asset.executable_sha256 = digest(bytes);
    (c, path)
}
#[test]
fn package_install_receipt_inventory_configs_and_repair() {
    let temp = TempDir::new().unwrap();
    let (c, zip) = fixture_package(temp.path(), b"engine one", None);
    let store = ResourceStore::new(temp.path().join("resources"));
    let installed = store
        .install_package(&c, "linux-cpu", &zip, ResourceSource::Download, None, &cancelled(false))
        .unwrap();
    assert_eq!(fs::read(&installed.executable).unwrap(), b"engine one");
    assert_eq!(installed.receipt.archive_sha256, c.assets["linux-cpu"].sha256);
    assert!(installed.directory.join("resource.json").is_file());
    assert!(installed.directory.join("pkg/licenses/LICENSE").is_file());
    fs::write(store.gtp_config(), "user edits\n").unwrap();
    store
        .install_package(&c, "linux-cpu", &zip, ResourceSource::Download, None, &cancelled(false))
        .unwrap();
    assert_eq!(fs::read_to_string(store.gtp_config()).unwrap(), "user edits\n");
    fs::remove_file(store.analysis_config()).unwrap();
    store.repair_analysis_config().unwrap();
    assert!(fs::read_to_string(store.analysis_config())
        .unwrap()
        .contains("numAnalysisThreads = 2"));
    let inventory = store.inventory().unwrap();
    assert_eq!(inventory.packages.len(), 1);
    assert!(inventory.packages[0].usable);
    fs::write(&installed.executable, "changed size").unwrap();
    assert!(!store.inventory().unwrap().packages[0].usable);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(&installed.executable).unwrap().permissions().mode() & 0o111,
            0
        );
    }
}
#[test]
fn failed_install_restores_previous_package_and_preserves_config_obstacle() {
    let temp = TempDir::new().unwrap();
    let (c, zip) = fixture_package(temp.path(), b"old engine", None);
    let store = ResourceStore::new(temp.path().join("resources"));
    let first = store
        .install_package(&c, "linux-cpu", &zip, ResourceSource::Download, None, &cancelled(false))
        .unwrap();
    let receipt = fs::read(first.directory.join("resource.json")).unwrap();
    fs::rename(store.root().join("configs"), store.root().join("saved-configs")).unwrap();
    fs::write(store.root().join("configs"), "keep me").unwrap();
    let (new_catalog, zip) = fixture_package(temp.path(), b"replacement engine", None);
    assert!(store
        .install_package(
            &new_catalog,
            "linux-cpu",
            &zip,
            ResourceSource::Import,
            None,
            &cancelled(false)
        )
        .is_err());
    assert_eq!(fs::read(&first.executable).unwrap(), b"old engine");
    assert_eq!(fs::read(first.directory.join("resource.json")).unwrap(), receipt);
    assert_eq!(fs::read_to_string(store.root().join("configs")).unwrap(), "keep me");
}
#[test]
fn package_checksums_zip_slip_and_unknown_offline_import_leave_install_unchanged() {
    let temp = TempDir::new().unwrap();
    let (c, zip) = fixture_package(temp.path(), b"old engine", None);
    let store = ResourceStore::new(temp.path().join("resources"));
    let first = store
        .import_package(&c, "linux-x64", &zip, None, &cancelled(false))
        .unwrap();
    assert_eq!(first.receipt.source, ResourceSource::Import);
    let (bad, zip) = fixture_package(temp.path(), b"new engine", Some(("../../escaped.txt", b"escape")));
    assert!(store
        .install_package(&bad, "linux-cpu", &zip, ResourceSource::Import, None, &cancelled(false))
        .is_err());
    assert!(!temp.path().join("escaped.txt").exists());
    assert_eq!(fs::read(&first.executable).unwrap(), b"old engine");
    let (mut wrong, zip) = fixture_package(temp.path(), b"new engine", None);
    wrong.assets.get_mut("linux-cpu").unwrap().executable_sha256 = "0".repeat(64);
    assert!(store
        .install_package(
            &wrong,
            "linux-cpu",
            &zip,
            ResourceSource::Download,
            None,
            &cancelled(false)
        )
        .is_err());
    fs::write(&zip, "not a trusted archive").unwrap();
    let error = store
        .import_package(&c, "linux-x64", &zip, None, &cancelled(false))
        .unwrap_err();
    assert!(matches!(error, ResourceError::ImportMismatch));
    assert!(error.to_string().contains("nothing changed"));
    assert_eq!(fs::read(&first.executable).unwrap(), b"old engine");
}
#[test]
fn weight_import_identifies_header_uniquely_and_inventory_uses_recorded_digest() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("renamed.bin.gz");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(b"kata1-tf3-b11c768-s12002M-d6304M\n16\n19\n19\nrest")
        .unwrap();
    let data = encoder.finish().unwrap();
    fs::write(&source, &data).unwrap();
    let mut c = catalog();
    let model = c.models.get_mut("b11-flagship").unwrap();
    model.size_bytes = data.len() as u64;
    model.sha256 = digest(&data);
    let store = ResourceStore::new(temp.path().join("resources"));
    let first = store.import_weight(&c, &source, &cancelled(false)).unwrap();
    let second = store.import_weight(&c, &source, &cancelled(false)).unwrap();
    assert_eq!(first.model_name.as_deref(), Some("kata1-tf3-b11c768-s12002M-d6304M"));
    assert_eq!(first.catalog_model_id.as_deref(), Some("b11-flagship"));
    assert_ne!(first.path, second.path);
    assert_eq!(second.path.file_name().unwrap(), "renamed-1.bin.gz");
    assert_eq!(store.inventory().unwrap().weights.len(), 2);
    fs::write(&first.path, b"changed").unwrap();
    assert_eq!(
        store
            .inventory()
            .unwrap()
            .weights
            .iter()
            .find(|w| w.path == first.path)
            .unwrap()
            .sha256,
        None
    );
    assert!(!weight_compatibility(first.model_name.as_ref().unwrap(), Some("1.16.4")).compatible);
    assert!(weight_compatibility(first.model_name.as_ref().unwrap(), Some("1.17.0")).compatible);
    assert!(!weight_compatibility(first.model_name.as_ref().unwrap(), None).compatible);
    assert!(weight_compatibility("kata1-b18c384-s123-d456", None).compatible);
    assert_eq!(
        parse_weight_header(b"renamed\n16\n19\n19\nrest"),
        Some("renamed".into())
    );
    assert_eq!(parse_weight_header(b"garbage"), None);
    assert_eq!(parse_weight_header(b"net 0 19 19 \n"), None);
}
#[test]
fn official_strongest_and_html_table_preserve_integrity_and_metadata() {
    let name = "kata1-tf3-b11c768-s12002M-d6304M";
    let json = serde_json::json!({"name": name, "model_file": format!("{OFFICIAL_MODEL_PREFIX}{name}.bin.gz"), "model_file_sha256": "a".repeat(64), "model_file_bytes": 262017809, "created_at": "2026-09-25T00:00:00Z"});
    let weight = parse_official_strongest(&json.to_string()).unwrap();
    assert!(weight.transformer);
    assert!(weight.recommended);
    assert_eq!(weight.size_bytes, Some(262017809));
    assert_eq!(weight.published_at, "2026-09-25");
    let mut bad = json.clone();
    bad["model_file"] = "https://evil.example/model.bin.gz".into();
    assert!(parse_official_strongest(&bad.to_string()).is_err());
    let mut bad = json;
    bad["model_file_sha256"] = "bad".into();
    assert!(parse_official_strongest(&bad.to_string()).is_err());
    let html = format!(
        r#"Strongest confidently-rated network:</span><a href="{OFFICIAL_MODEL_PREFIX}{name}.bin.gz">{name}</a><table class="table mt-3"><tr><th>Name</th></tr><tr><td>{name}</td><td>2026-09-25</td><td>+1234 &plusmn; 5</td><td><a href="{OFFICIAL_MODEL_PREFIX}{name}.bin.gz">download</a></td></tr><tr><td>{name}</td><td>duplicate</td><td>0</td><td><a href="/duplicate">x</a></td></tr></table>"#
    );
    let rows = parse_official_weight_table(&html).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].recommended);
    assert_eq!(rows[0].elo, "+1234 ± 5");
    assert_eq!(rows[0].sha256, None);
    assert_eq!(rows[0].size_bytes, None);
    assert!(parse_official_weight_table("maintenance page").is_err());
}
#[test]
fn verification_classifies_java_markers_and_uses_exact_probe_arguments() {
    let valid = classify_version(Ok(success("KataGo v1.18.2\nCUDA")), false);
    assert_eq!(valid.status, ValidationStatus::Valid);
    assert_eq!(valid.version.as_deref(), Some("1.18.2"));
    assert_eq!(parse_katago_version("KataGo v1.17"), Some("1.17.0".into()));
    for (message, expected) in [
        ("error=193", ValidationStatus::WrongArchitecture),
        ("bad cpu type", ValidationStatus::WrongArchitecture),
        ("exec format error", ValidationStatus::WrongArchitecture),
        ("not a valid Win32 application", ValidationStatus::WrongArchitecture),
        ("error=126", ValidationStatus::MissingDependency),
        ("missing CUDNN DLL", ValidationStatus::MissingDependency),
        ("error loading shared libraries", ValidationStatus::MissingDependency),
        ("library not loaded", ValidationStatus::MissingDependency),
        ("crashed", ValidationStatus::StartFailed),
    ] {
        let output = ProbeOutput {
            exit_code: Some(1),
            stderr: message.into(),
            ..Default::default()
        };
        assert_eq!(classify_version(Ok(output), false).status, expected);
    }
    assert_eq!(
        classify_version(
            Ok(ProbeOutput {
                timed_out: true,
                ..Default::default()
            }),
            false
        )
        .status,
        ValidationStatus::TimedOut
    );
    let p = FixtureProbe::default();
    assert_eq!(
        verify_engine(&p, Path::new("engine"), true, true).status,
        ValidationStatus::Active
    );
    assert!(p.calls.borrow().is_empty());
    verify_engine(&p, Path::new("engine"), true, false);
    assert_eq!(p.calls.borrow()[0].2, Duration::from_secs(20));
    let args = cuda_probe_args(Path::new("gtp.cfg"), Path::new("weight.bin.gz"));
    assert_eq!(args.join(" "), "benchmark -config gtp.cfg -model weight.bin.gz -t 1 -n 1 -v 2 -time 1 -no-server-thread-test -no-half-batch-size-test -override-config numSearchThreads=1,logToStderr=false,logAllGTPCommunication=false,logSearchInfo=false");
    let result = verify_cuda(
        &p,
        Path::new("engine"),
        Path::new("gtp.cfg"),
        Path::new("weight.bin.gz"),
    );
    assert!(!result.passed);
    assert_eq!(p.calls.borrow().last().unwrap().2, Duration::from_secs(300));
}
#[test]
fn managed_setup_plan_roundtrips_paths_and_never_overwrites_unmanaged_profiles() {
    let temp = TempDir::new().unwrap();
    let choice = SetupChoice {
        asset_id: "windows-openvino".into(),
        executable: PathBuf::from("C:\\engine folder\\katago.exe"),
        weight: PathBuf::from("C:\\weights\\model.bin.gz"),
        config: PathBuf::from("C:\\config folder\\gtp.cfg"),
        model_display_name: "Transformer 11B".into(),
        model_name: Some("kata1-tf3-b11c768-s12002M-d6304M".into()),
        engine_version: Some("1.18.2".into()),
        experimental_backend: Some(ExperimentalBackend::OpenVinoNpu),
        update_profile_id: None,
        set_as_default: true,
        switch_now: true,
    };
    let plan = plan_setup(&[], &choice, temp.path()).unwrap();
    assert!(is_managed(&plan.profile));
    assert!(plan.profile.thread_policy.as_ref().unwrap().initial_setup);
    assert_eq!(plan.profile.name, "KataGo (Transformer 11B)");
    let command = split_command(&plan.profile.command);
    assert_eq!(command[0], choice.executable.to_string_lossy());
    assert_eq!(command[3], choice.weight.to_string_lossy());
    assert_eq!(command[5], choice.config.to_string_lossy());
    assert!(command[7].contains("onnxOpenVINODeviceType=NPU"));
    let mut choice = choice;
    choice.update_profile_id = Some(plan.profile.id.clone());
    choice.engine_version = Some("1.16.4".into());
    assert!(plan_setup(std::slice::from_ref(&plan.profile), &choice, temp.path()).is_err());
    choice.engine_version = Some("1.18.2".into());
    assert_eq!(
        plan_setup(std::slice::from_ref(&plan.profile), &choice, temp.path())
            .unwrap()
            .profile
            .id,
        plan.profile.id
    );
    let mut edited = plan.profile;
    edited.command.push_str(" -custom");
    assert!(!is_managed(&edited));
    assert!(plan_setup(&[edited], &choice, temp.path()).is_err());
    let manual = EngineProfile {
        id: "manual".into(),
        command: "custom engine".into(),
        ..Default::default()
    };
    choice.update_profile_id = Some("manual".into());
    assert!(plan_setup(&[manual], &choice, temp.path()).is_err());
}
#[test]
fn runtime_manifests_pins_dll_filter_staged_install_and_cache_cleanup() {
    let manifest = serde_json::json!({"cuda_cudart": {"version": "12.8.57", "windows-x86_64": {"relative_path": "cuda_cudart/windows-x86_64/package.zip", "sha256": "a".repeat(64), "size": "12345"}}, "cuda_nvrtc": {"version": NVRTC_VERSION, "windows-x86_64": {"relative_path": "cuda_nvrtc/windows-x86_64/nvrtc.zip", "sha256": NVRTC_SHA256, "size": "300000000"}}});
    let spec = parse_runtime_manifest(&manifest.to_string(), CUDA_MANIFEST_URL, "cuda_cudart").unwrap();
    assert_eq!(spec.size, 12345);
    assert!(spec
        .url
        .starts_with("https://developer.download.nvidia.com/compute/cuda/redist/"));
    parse_runtime_manifest(&manifest.to_string(), CUDA_MANIFEST_URL, "cuda_nvrtc").unwrap();
    let mut wrong = manifest;
    wrong["cuda_nvrtc"]["version"] = "12.8.99".into();
    assert!(parse_runtime_manifest(&wrong.to_string(), CUDA_MANIFEST_URL, "cuda_nvrtc").is_err());
    assert_eq!(runtime_component_keys("cuda").len(), 5);
    assert_eq!(runtime_component_keys("tensorrt").len(), 6);
    assert_eq!(tensorrt_package().size, 1_845_842_538);
    assert!(should_extract_runtime_dll("tensorrt", "nvinfer_10.dll"));
    assert!(!should_extract_runtime_dll("tensorrt", "unrelated.dll"));
    assert!(should_extract_runtime_dll("cuda_cudart", "cudart64_12.dll"));
    let temp = TempDir::new().unwrap();
    let zip = temp.path().join("runtime.zip");
    archive(
        &zip,
        &[
            ("pkg/bin/nvinfer_10.dll", b"runtime"),
            ("pkg/bin/unrelated.dll", b"skip"),
            ("pkg/LICENSE.txt", b"license"),
        ],
    );
    let package = RuntimePackage {
        key: "tensorrt".into(),
        version: "fixture".into(),
        url: TENSORRT_URL.into(),
        sha256: sha256_file(&zip).unwrap(),
        size: fs::metadata(&zip).unwrap().len(),
    };
    let store = ResourceStore::new(temp.path().join("resources"));
    fs::create_dir_all(store.runtime_downloads_dir()).unwrap();
    fs::write(store.runtime_downloads_dir().join("done.zip"), b"cache").unwrap();
    fs::write(store.runtime_downloads_dir().join("pending.zip.part"), b"part").unwrap();
    store
        .install_nvidia_runtime(&[(package.clone(), zip.clone())], &cancelled(false))
        .unwrap();
    assert_eq!(
        fs::read(store.runtime_dir().join("nvinfer_10.dll")).unwrap(),
        b"runtime"
    );
    assert!(!store.runtime_dir().join("unrelated.dll").exists());
    assert!(store.runtime_dir().join("licenses/tensorrt-LICENSE.txt").is_file());
    assert_eq!(store.runtime_cache_size().unwrap(), 9);
    let mut bad = package;
    bad.sha256 = "0".repeat(64);
    assert!(store.install_nvidia_runtime(&[(bad, zip)], &cancelled(false)).is_err());
    assert_eq!(
        fs::read(store.runtime_dir().join("nvinfer_10.dll")).unwrap(),
        b"runtime"
    );
    assert_eq!(store.clean_runtime_cache(false).unwrap(), 5);
    assert_eq!(store.runtime_cache_size().unwrap(), 4);
    assert_eq!(store.clean_runtime_cache(true).unwrap(), 4);
}

#[test]
fn header_limits_and_missing_facts_do_not_invent_identity_or_capacity() {
    let mut long_prefix = vec![b' '; 1024];
    long_prefix.extend_from_slice(b"net\n16\n19\n19\n");
    assert_eq!(parse_weight_header(&long_prefix), None);
    assert_eq!(
        parse_weight_header(format!("{}\n16\n19\n19\n", "x".repeat(257)).as_bytes()),
        None
    );
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("model.bin.gz");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&long_prefix).unwrap();
    fs::write(&path, encoder.finish().unwrap()).unwrap();
    assert_eq!(read_weight_model_name(&path), None);
    let onnx = temp.path().join("model.onnx.gz");
    fs::copy(&path, &onnx).unwrap();
    assert_eq!(read_weight_model_name(&onnx), None);
    let p = FixtureProbe::default();
    let machine = detect_machine(&p, "linux", "x86_64");
    assert_eq!(machine.total_memory_bytes, None);
    assert_eq!(machine.cpu_name, None);
    assert_eq!(machine.nvidia_status, NvidiaQueryStatus::Failed);
    assert_eq!(machine.loader_libraries, None);
    let mut p = FixtureProbe::default();
    p.outputs.insert(
        "nvidia-smi --query-gpu=name,compute_cap,driver_version,memory.total --format=csv,noheader,nounits".into(),
        success(""),
    );
    assert_eq!(
        detect_machine(&p, "linux", "x86_64").nvidia_status,
        NvidiaQueryStatus::None
    );
}

#[test]
fn analysis_repair_uses_java_template_once_without_changing_gtp_config() {
    let temp = TempDir::new().unwrap();
    let store = ResourceStore::new(temp.path().join("resources"));
    assert!(store.repair_analysis_config().is_err());
    fs::create_dir_all(store.root().join("configs")).unwrap();
    fs::write(store.gtp_config(), "numSearchThreads = 999\n").unwrap();
    store.repair_analysis_config().unwrap();
    assert!(fs::read_to_string(store.analysis_config())
        .unwrap()
        .contains("numSearchThreadsPerAnalysisThread = 16"));
    assert_eq!(
        fs::read_to_string(store.gtp_config()).unwrap(),
        "numSearchThreads = 999\n"
    );
    fs::write(store.analysis_config(), "user analysis settings\n").unwrap();
    store.repair_analysis_config().unwrap();
    assert_eq!(
        fs::read_to_string(store.analysis_config()).unwrap(),
        "user analysis settings\n"
    );
}

#[test]
fn cross_platform_archive_escape_and_cancel_are_non_destructive() {
    let temp = TempDir::new().unwrap();
    let store = ResourceStore::new(temp.path().join("resources"));
    for name in ["../outside", "..\\outside", "/absolute", "C:\\absolute\\escape"] {
        let (catalog, zip) = fixture_package(temp.path(), b"engine", Some((name, b"escape")));
        assert!(
            store
                .install_package(
                    &catalog,
                    "linux-cpu",
                    &zip,
                    ResourceSource::Import,
                    None,
                    &cancelled(false)
                )
                .is_err(),
            "{name}"
        );
        assert!(store.inventory().unwrap().packages.is_empty());
    }
    let (catalog, zip) = fixture_package(temp.path(), b"engine", None);
    assert!(matches!(
        store.install_package(
            &catalog,
            "linux-cpu",
            &zip,
            ResourceSource::Import,
            None,
            &cancelled(true)
        ),
        Err(ResourceError::Cancelled)
    ));
    assert!(store.inventory().unwrap().packages.is_empty());
}
